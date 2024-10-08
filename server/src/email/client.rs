extern crate google_gmail1 as gmail1;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use anyhow::Context;
use futures::future::join_all;
use google_gmail1::api::{
    Label, LabelColor, ListLabelsResponse, ListMessagesResponse, Message, Profile,
};
use leaky_bucket::RateLimiter;
use mail_parser::MessageParser;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;

use crate::{
    api_quota::{GMAIL_API_QUOTA, GMAIL_QUOTA_PER_SECOND},
    server_config::{cfg, Category, DAILY_SUMMARY_CATEGORY, UNKNOWN_CATEGORY},
    structs::response::LabelUpdate,
};

macro_rules! gmail_url {
    ($($params:expr),*) => {
        {
            const GMAIL_ENDPOINT: &str = "https://www.googleapis.com/gmail/v1/users/me";
            let list_params = vec![$($params),*];
            let path = list_params.join("/");
            format!("{}/{}", GMAIL_ENDPOINT, path)
        }
    };
}

pub struct EmailClient {
    http_client: reqwest::Client,
    access_token: String,
    rate_limiter: RateLimiter,
}

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub id: String,
    pub label_ids: Vec<String>,
    pub thread_id: String,
    pub history_id: u64,
    pub internal_date: i64,
    pub subject: Option<String>,
    pub snippet: String,
    pub body: Option<String>,
}

impl EmailClient {
    pub async fn new(
        http_client: reqwest::Client,
        access_token: String,
    ) -> anyhow::Result<EmailClient> {
        let rate_limiter = RateLimiter::builder()
            .initial(GMAIL_QUOTA_PER_SECOND)
            .interval(Duration::from_secs(1))
            .refill(GMAIL_QUOTA_PER_SECOND)
            .build();

        Ok(EmailClient {
            http_client,
            access_token,
            rate_limiter,
        })
    }

    pub async fn get_message_list(
        &self,
        page_token: Option<String>,
    ) -> anyhow::Result<ListMessagesResponse> {
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.messages_list)
            .await;

        // Add mailclerk labels to filter
        let mut label_set = cfg
            .categories
            .iter()
            .map(|c| format!("label:{}", c.mail_label))
            .collect::<HashSet<_>>();

        // Add special labels to filter
        for mail_label in &[
            UNKNOWN_CATEGORY.mail_label.clone(),
            DAILY_SUMMARY_CATEGORY.mail_label.clone(),
        ] {
            label_set.insert(format!("label:{}", mail_label));
        }

        let labels = vec!["label:inbox".to_string()]
            .into_iter()
            .chain(label_set)
            .collect::<Vec<_>>();

        let filter = labels.join(" AND NOT ");

        // -- DEBUG
        // println!("Filter: {}", filter);
        // -- DEBUG

        let mut query = vec![
            ("q".to_string(), filter),
            ("maxResults".to_string(), "150".to_string()),
        ];

        if let Some(token) = page_token {
            query.push(("pageToken".to_string(), token));
        }
        let resp = self
            .http_client
            .get(gmail_url!("messages"))
            .query(&query)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        let data = resp.json::<ListMessagesResponse>().await?;

        Ok(data)
    }

    pub async fn get_sanitized_message(&self, message_id: &str) -> anyhow::Result<EmailMessage> {
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.messages_get)
            .await;
        let id = message_id;
        let req = self
            .http_client
            .get(gmail_url!("messages", id))
            .bearer_auth(&self.access_token)
            .query(&[("format", "RAW")])
            .send()
            .await?;

        let msg = req.json::<Message>().await?;

        static RE_WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\r\t\n]+").unwrap());
        static RE_NON_UNICODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\x00-\x80]").unwrap());
        static RE_HTTP_LINK: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_\+.~#?&//=]*)").unwrap()
        });

        let id = msg.id.unwrap_or_default();
        let label_ids = msg.label_ids.unwrap_or_default();
        let thread_id = msg.thread_id.unwrap_or_default();
        let snippet = msg.snippet.unwrap_or_default();
        let history_id = msg.history_id.unwrap_or_default();
        let internal_date = msg.internal_date.unwrap_or_default();
        msg.raw
            .map(|input| {
                let msg = MessageParser::default().parse(&input);
                let (subject, body) = msg.map_or((None, None), |m| {
                    let subject = m.subject().map(|s| s.to_string());
                    let body = m.body_text(0).map(|b| b.to_string());
                    (subject, body)
                });
                let snippet = {
                    let s = RE_WHITESPACE.replace_all(&snippet, " ");
                    let s = RE_NON_UNICODE.replace_all(&s, "");
                    s.to_string()
                };
                let subject = subject.map(|s| {
                    let s = RE_WHITESPACE.replace_all(&s, " ");
                    let s = RE_NON_UNICODE.replace_all(&s, "");
                    s.to_string()
                });
                let body = body.map(|b| {
                    let b = RE_WHITESPACE.replace_all(&b, " ");
                    let b = RE_NON_UNICODE.replace_all(&b, "");
                    let b = RE_HTTP_LINK.replace_all(&b, "[LINK]");
                    b.to_string()
                });

                EmailMessage {
                    id,
                    label_ids,
                    thread_id,
                    history_id,
                    internal_date,
                    subject,
                    snippet,
                    body,
                }
            })
            .context("No raw message found")
    }

    // pub async fn get_threads(&self) -> anyhow::Result<Vec<Thread>> {
    //     let (_, resp) = self.hub.users().threads_list("me").doit().await?;
    //     Ok(resp.threads.unwrap_or_default())
    // }

    pub async fn get_labels(&self) -> anyhow::Result<Vec<Label>> {
        self.rate_limiter.acquire(GMAIL_API_QUOTA.labels_list).await;
        let resp = self
            .http_client
            .get(gmail_url!("labels"))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        let data = resp.json::<ListLabelsResponse>().await?;

        Ok(data.labels.unwrap_or_default())
    }

    pub async fn create_label(&self, label: Label) -> anyhow::Result<Label> {
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.labels_create)
            .await;
        let resp = self
            .http_client
            .post(gmail_url!("labels"))
            .bearer_auth(&self.access_token)
            .json(&label)
            .send()
            .await?;
        let data = resp.json::<serde_json::Value>().await?;
        if data.get("error").is_some() {
            return Err(anyhow::anyhow!("Error creating label: {:?}", data));
        }

        Ok(serde_json::from_value(data)?)
    }

    pub async fn delete_label(&self, label_id: String) -> anyhow::Result<()> {
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.labels_delete)
            .await;
        let resp = self
            .http_client
            .delete(gmail_url!("labels", &label_id))
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        match resp.json::<serde_json::Value>().await {
            Ok(data) if data.get("error").is_some() => {
                Err(anyhow::anyhow!("Error deleting label: {:?}", data))
            }
            Ok(unknown) => Err(anyhow::anyhow!("Unknown response: {:?}", unknown)),
            Err(_) => {
                // An empty response is expected if the label was deleted successfully
                Ok(())
            }
        }
    }

    pub async fn configure_labels_if_needed(&self) -> anyhow::Result<bool> {
        let existing_labels = self
            .get_labels()
            .await?
            .iter()
            .filter(|l| l.name.as_ref().map_or(false, |n| n.contains("mailclerk:")))
            .cloned()
            .collect::<Vec<_>>();

        // Configure labels if they need it
        let mut required_labels = cfg
            .categories
            .iter()
            .map(|c| c.mail_label.to_string())
            .collect::<HashSet<_>>();

        // Add Unknown category label
        required_labels.insert(UNKNOWN_CATEGORY.mail_label.clone());

        // Add Daily summary label
        required_labels.insert(DAILY_SUMMARY_CATEGORY.mail_label.clone());

        let existing_label_names = existing_labels
            .iter()
            .map(|l| l.name.clone().unwrap_or_default())
            .collect::<HashSet<_>>();

        let missing_labels = required_labels
            .difference(&existing_label_names)
            .cloned()
            .collect::<Vec<_>>();

        let unneeded_labels = {
            let unneeded = existing_label_names
                .difference(&required_labels)
                .cloned()
                .collect::<HashSet<_>>();

            existing_labels
                .iter()
                .filter(|l| l.name.as_ref().map_or(false, |n| unneeded.contains(n)))
                .cloned()
                .collect::<Vec<_>>()
        };

        if missing_labels.is_empty() && unneeded_labels.is_empty() {
            // Labels are already configured
            return Ok(false);
        }

        // Add missing mailclerk labels
        let add_label_tasks = missing_labels.into_iter().map(|label| {
            let (message_list_visibility, label_list_visibility) =
                if label == UNKNOWN_CATEGORY.mail_label {
                    (Some("hide".to_string()), Some("labelHide".to_string()))
                } else {
                    (Some("show".to_string()), Some("labelShow".to_string()))
                };
            let label = Label {
                id: None,
                type_: Some("user".to_string()),
                color: Some(get_label_color(&label)),
                name: Some(label.clone()),
                messages_total: None,
                messages_unread: None,
                threads_total: None,
                threads_unread: None,
                message_list_visibility,
                label_list_visibility,
            };
            async { self.create_label(label).await }
        });

        // Remove old mailclerk labels that are no longer used
        //? Maybe remove this in the future?
        let remove_label_tasks = unneeded_labels.into_iter().map(|label| async {
            let id = label.id.context("Label id not provided")?;
            self.delete_label(id).await
        });

        let results = join_all(add_label_tasks).await;
        for result in results {
            result?;
        }

        let results = join_all(remove_label_tasks).await;
        for result in results {
            result?;
        }

        Ok(true)
    }

    pub async fn label_email(
        &self,
        email_id: String,
        current_labels: Vec<String>,
        category: Category,
    ) -> anyhow::Result<LabelUpdate> {
        let user_labels = self.get_labels().await?;
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.messages_modify)
            .await;
        let (json_body, update) = build_label_update(user_labels, current_labels, category)?;
        let resp = self
            .http_client
            .post(gmail_url!("messages", &email_id, "modify"))
            .bearer_auth(&self.access_token)
            .json(&json_body)
            .send()
            .await?;
        let data = resp.json::<serde_json::Value>().await?;

        if data.get("error").is_some() {
            return Err(anyhow::anyhow!("Error labelling email: {:?}", data));
        }

        Ok(update)
    }

    pub async fn get_profile(&self) -> anyhow::Result<Profile> {
        self.rate_limiter.acquire(GMAIL_API_QUOTA.get_profile).await;
        let resp = self
            .http_client
            .get("https://www.googleapis.com/gmail/v1/users/me/profile")
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        Ok(resp.json::<Profile>().await?)
    }
}

fn build_label_update(
    user_labels: Vec<Label>,
    current_labels: Vec<String>,
    category: Category,
) -> anyhow::Result<(serde_json::Value, LabelUpdate)> {
    static RE_CATEGORY_LABEL: Lazy<Regex> = Lazy::new(|| Regex::new(r"CATEGORY_+").unwrap());

    let current_categories = current_labels
        .iter()
        .filter(|c| RE_CATEGORY_LABEL.is_match(c))
        .cloned()
        .collect::<Vec<_>>();

    let categories_to_add = category.gmail_categories;

    let categories_to_remove = current_categories
        .iter()
        .filter(|c| !categories_to_add.contains(c))
        .cloned()
        .collect::<Vec<_>>();

    let label_id = user_labels
        .iter()
        .find(|l| l.name.as_ref() == Some(&category.mail_label))
        .map(|l| l.id.clone().unwrap_or_default())
        .context(format!("Could not find {}!", category.mail_label))?;

    let (label_ids_to_add, label_names_applied) = {
        let mut label_ids = categories_to_add.clone();
        label_ids.push(label_id);
        let mut label_names = categories_to_add;
        label_names.push(category.mail_label);
        (
            label_ids.into_iter().collect::<Vec<_>>(),
            label_names.into_iter().collect::<Vec<_>>(),
        )
    };

    Ok((
        json!(
            {
                "addLabelIds": label_ids_to_add,
                "removeLabelIds": categories_to_remove
            }
        ),
        LabelUpdate {
            added: label_names_applied,
            removed: categories_to_remove,
        },
    ))
}

const LABEL_COLORS: [(&str, &str); 2] = [
    ("White", "#ffffff"),
    ("Dark Gray", "#434343"),
    // add more colors as needed...
];

fn get_label_color(_label: &str) -> LabelColor {
    let color_map = Lazy::new(|| LABEL_COLORS.iter().cloned().collect::<HashMap<_, _>>());
    let bg = color_map.get("Dark Gray").map(|c| c.to_string());
    let text = color_map.get("White").map(|c| c.to_string());
    LabelColor {
        background_color: bg,
        text_color: text,
    }
}

#[cfg(test)]
mod tests {
    use google_gmail1::api::Label;

    #[test]
    fn test_gmail_url() {
        let url = gmail_url!("messages");
        assert_eq!(url, "https://www.googleapis.com/gmail/v1/users/me/messages");
        let url = gmail_url!("messages", "123");
        assert_eq!(
            url,
            "https://www.googleapis.com/gmail/v1/users/me/messages/123"
        );
    }

    #[test]
    fn test_build_label_update() {
        let user_labels = vec![Label {
            id: Some("Label_10".to_string()),
            name: Some("mailclerk:ads".to_string()),
            ..Label::default()
        }];
        match super::build_label_update(
            user_labels,
            ["CATEGORY_SOCIAL".to_string()].to_vec(),
            super::Category {
                ai: "Advertisment".to_string(),
                mail_label: "mailclerk:ads".to_string(),
                gmail_categories: vec!["CATEGORY_PROMOTIONS".to_string()],
            },
        ) {
            Ok((json_body, update)) => {
                assert_eq!(
                    json_body,
                    serde_json::json!({
                        "addLabelIds": ["CATEGORY_PROMOTIONS", "Label_10"],
                        "removeLabelIds": ["CATEGORY_SOCIAL"]
                    })
                );
                assert_eq!(
                    update,
                    super::LabelUpdate {
                        added: vec![
                            "CATEGORY_PROMOTIONS".to_string(),
                            "mailclerk:ads".to_string()
                        ],
                        removed: vec!["CATEGORY_SOCIAL".to_string()]
                    }
                );
            }
            Err(e) => panic!("Error: {:?}", e),
        }
    }
}

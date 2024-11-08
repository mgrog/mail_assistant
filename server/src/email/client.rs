extern crate google_gmail1 as gmail1;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use crate::{
    db_core::{prelude::*, queries::get_user_inbox_settings},
    model::{
        settings_derivatives::{default_inbox_settings, CategoryInboxSettingsMap},
        user_derivatives::UserWithAccountAccess,
    },
};
use anyhow::{anyhow, Context};
use chrono::Utc;
use futures::future::join_all;
use google_gmail1::api::{
    Label, LabelColor, ListLabelsResponse, ListMessagesResponse, Message, Profile, WatchResponse,
};
use indexmap::IndexSet;
use leaky_bucket::RateLimiter;
use mail_parser::MessageParser;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;

use crate::{
    api_quota::{GMAIL_API_QUOTA, GMAIL_QUOTA_PER_SECOND},
    model::response::LabelUpdate,
    server_config::{cfg, Category, DAILY_SUMMARY_CATEGORY, UNKNOWN_CATEGORY},
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

#[derive(Default)]
/// Filter and paging options for message list
pub struct MessageListOptions {
    /// Messages more recent than this duration will be returned
    pub more_recent_than: chrono::Duration,
    pub page_token: Option<String>,
}

pub struct EmailClient {
    http_client: reqwest::Client,
    access_token: String,
    rate_limiter: RateLimiter,
    pub category_inbox_settings: CategoryInboxSettingsMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailMessage {
    pub id: String,
    pub label_ids: Vec<String>,
    pub thread_id: String,
    pub history_id: u64,
    pub internal_date: i64,
    pub from: Option<String>,
    pub subject: Option<String>,
    pub snippet: String,
    pub body: Option<String>,
}

enum EmailClientError {
    RateLimitExceeded,
    Unauthorized,
    BadRequest,
    Unknown,
}

type EmailClientResult<T> = Result<T, EmailClientError>;

// fn parse_gmail_response<T>(resp: reqwest::Response) -> EmailClientResult<T>
// where
//     T: serde::de::DeserializeOwned,
// {
//     let data = resp.json::<T>().await
//     Ok(data)
// }

impl EmailClient {
    pub async fn new(
        http_client: reqwest::Client,
        conn: DatabaseConnection,
        mut user: UserWithAccountAccess,
    ) -> anyhow::Result<EmailClient> {
        let rate_limiter = RateLimiter::builder()
            .initial(GMAIL_QUOTA_PER_SECOND)
            .interval(Duration::from_secs(1))
            .refill(GMAIL_QUOTA_PER_SECOND)
            .build();

        let access_token = user
            .get_valid_access_code(&conn, http_client.clone())
            .await
            .map_err(|e| anyhow!("Could not get new access code: {e}"))?;

        let user_configured_settings = get_user_inbox_settings(&conn, user.id)
            .await
            .context("Could not find inbox settings")?;

        let mut category_inbox_settings = default_inbox_settings();
        category_inbox_settings.extend(user_configured_settings);

        // This will be a map from mailclerk:* -> inbox_settings
        let category_inbox_settings = category_inbox_settings
            .into_iter()
            .map(|(category, setting)| (format!("mailclerk:{}", category), setting))
            .collect::<HashMap<_, _>>();

        Ok(EmailClient {
            http_client,
            access_token,
            rate_limiter,
            category_inbox_settings,
        })
    }

    // This is only used to test a new client on authentication
    pub fn from_access_code(http_client: reqwest::Client, access_token: String) -> EmailClient {
        let rate_limiter = RateLimiter::builder()
            .initial(GMAIL_QUOTA_PER_SECOND)
            .interval(Duration::from_secs(1))
            .refill(GMAIL_QUOTA_PER_SECOND)
            .build();

        EmailClient {
            http_client,
            access_token,
            rate_limiter,
            category_inbox_settings: HashMap::new(),
        }
    }

    pub async fn watch_mailbox(&self) -> anyhow::Result<WatchResponse> {
        self.rate_limiter.acquire(GMAIL_API_QUOTA.watch).await;
        const TOPIC_NAME: &str = "projects/mail-assist-434915/topics/mailclerk-user-inboxes";
        let resp = self
            .http_client
            .post(gmail_url!("watch"))
            .bearer_auth(&self.access_token)
            .json(&json!({
                "topicName": TOPIC_NAME,
                "labelIds": ["INBOX"],
                "labelFilterBehavior": "INCLUDE",
            }));

        let data = resp.send().await?;

        if !data.status().is_success() {
            let json = data.json::<serde_json::Value>().await?;
            return Err(anyhow!("Error watching mailbox: {:?}", json));
        }

        let json = data.json::<WatchResponse>().await?;

        Ok(json)
    }

    pub async fn get_message_list(
        &self,
        options: MessageListOptions,
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

        let labels_filter = labels.join(" AND NOT ");

        let time_filter = format!(
            "after:{}",
            (Utc::now() - options.more_recent_than).timestamp()
        );

        // -- DEBUG
        // println!("Filter: {}", labels_filter);
        // -- DEBUG

        let mut query = vec![
            (
                "q".to_string(),
                format!("{} {}", labels_filter, time_filter),
            ),
            ("maxResults".to_string(), "500".to_string()),
        ];

        if let Some(token) = options.page_token {
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

    pub async fn get_message_by_id(&self, message_id: &str) -> anyhow::Result<Message> {
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

        req.json::<Message>().await.context("Error getting message")
    }

    pub async fn get_sanitized_message(&self, message_id: &str) -> anyhow::Result<EmailMessage> {
        let message = self.get_message_by_id(message_id).await?;
        sanitize_message(message)
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
        if let Some(error) = data.get("error") {
            if error.get("code").map_or(false, |x| x.as_i64() == Some(409)) {
                // Label already exists
                return Ok(label);
            }
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

        // -- DEBUG
        // println!("Existing labels: {:?}", existing_labels);
        // -- DEBUG

        // Configure labels if they need it
        let mut required_labels = cfg
            .categories
            .iter()
            .chain(cfg.heuristics.iter())
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

        // -- DEBUG
        // println!("Missing labels: {:?}", existing_labels);
        // -- DEBUG

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

        static COLORS: Lazy<Vec<LabelColor>> = Lazy::new(|| {
            LABEL_COLORS
                .iter()
                .map(|(_, bg, text)| LabelColor {
                    background_color: Some(bg.to_string()),
                    text_color: Some(text.to_string()),
                })
                .collect::<Vec<_>>()
        });
        let labels_to_add = cfg
            .categories
            .iter()
            .chain(cfg.heuristics.iter())
            .map(|c| c.mail_label.to_string())
            .collect::<IndexSet<_>>();

        // Readd mailclerk labels
        let add_label_tasks = labels_to_add.into_iter().enumerate().map(|(idx, label)| {
            let (message_list_visibility, label_list_visibility) =
                if label == UNKNOWN_CATEGORY.mail_label {
                    (Some("hide".to_string()), Some("labelHide".to_string()))
                } else {
                    (
                        Some("show".to_string()),
                        Some("labelShowIfUnread".to_string()),
                    )
                };
            let label = Label {
                id: None,
                type_: Some("user".to_string()),
                color: COLORS.get(idx % COLORS.len()).cloned(),
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

        // Reset mailclerk labels
        //? Maybe remove this in the future?
        //? Probably needs to migrate existing mails to new labels
        // let remove_label_tasks = existing_labels.into_iter().map(|label| async {
        //     let id = label.id.context("Label id not provided")?;
        //     self.delete_label(id).await
        // });

        // let results = join_all(remove_label_tasks).await;
        // for result in results {
        //     match result {
        //         Ok(_) => {}
        //         Err(e) => {
        //             tracing::error!("{e}");
        //             return Err(e);
        //         }
        //     }
        // }

        let results = join_all(add_label_tasks).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("{e}");
                    // return Err(e);
                }
            }
        }

        Ok(true)
    }

    /// Gets the label id for the mailclerk daily summary label, if doesn't exist, creates it
    pub async fn get_daily_summary_label_id(&self) -> anyhow::Result<String> {
        let existing_labels = self.get_labels().await?;
        let daily_summary_label_name = DAILY_SUMMARY_CATEGORY.mail_label.clone();
        if let Some(label) = existing_labels.iter().find(|l| {
            l.name
                .as_ref()
                .map_or(false, |n| n.as_str() == daily_summary_label_name.as_str())
        }) {
            Ok(label.id.clone().context("Label id not provided")?)
        } else {
            let label = self
                .create_label(Label {
                    id: None,
                    type_: Some("user".to_string()),
                    color: Some(LabelColor {
                        background_color: Some("#ffffff".to_string()),
                        text_color: Some("#000000".to_string()),
                    }),
                    name: Some(daily_summary_label_name.clone()),
                    messages_total: None,
                    messages_unread: None,
                    threads_total: None,
                    threads_unread: None,
                    message_list_visibility: Some("show".to_string()),
                    label_list_visibility: Some("labelShowIfUnread".to_string()),
                })
                .await?;

            Ok(label.id.context("Label id not provided")?)
        }
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
        let inbox_settings = &self.category_inbox_settings;
        let (json_body, update) =
            build_label_update(user_labels, current_labels, category, inbox_settings)?;
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

    pub async fn insert_message(&self, message: Message) -> anyhow::Result<()> {
        self.rate_limiter
            .acquire(GMAIL_API_QUOTA.messages_insert)
            .await;
        self.http_client
            .post(gmail_url!("messages"))
            .bearer_auth(&self.access_token)
            .json(&message)
            .send()
            .await?;

        Ok(())
    }
}

fn sanitize_message(msg: Message) -> anyhow::Result<EmailMessage> {
    static RE_WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\r\t\n]+").unwrap());
    static RE_LONG_SPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());
    static RE_NON_ASCII: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\x20-\x7E]").unwrap());
    static RE_HTTP_LINK: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_\+.~#?&//=]*)").unwrap()
    });

    let id = msg.clone().id.unwrap_or_default();
    let label_ids = msg.clone().label_ids.unwrap_or_default();
    let thread_id = msg.thread_id.clone().unwrap_or_default();
    let snippet = msg.clone().snippet.unwrap_or_default();
    let history_id = msg.history_id.unwrap_or_default();
    let internal_date = msg.internal_date.unwrap_or_default();
    msg.raw
        .as_ref()
        .map(|input| {
            let msg = MessageParser::default().parse(input);
            let (subject, body, from) = msg.map_or((None, None, None), |m| {
                let subject = m.subject().map(|s| s.to_string());
                let body = m.body_text(0).map(|b| b.to_string());
                let from = m
                    .from()
                    .and_then(|f| f.first().and_then(|x| x.address().map(|a| a.to_string())));

                (subject, body, from)
            });
            let snippet = {
                let s = RE_NON_ASCII.replace_all(&snippet, "");
                let s = RE_WHITESPACE.replace_all(&s, " ");
                let s = RE_LONG_SPACE.replace_all(&s, " ");
                s.to_string()
            };
            let subject = subject.map(|s| {
                let s = RE_NON_ASCII.replace_all(&s, "");
                let s = RE_WHITESPACE.replace_all(&s, " ");
                let s = RE_LONG_SPACE.replace_all(&s, " ");
                s.to_string()
            });
            let body = body.map(|b| {
                let b = RE_HTTP_LINK.replace_all(&b, "[LINK]");
                let bytes = b.as_bytes();
                let b: String = html2text::from_read(bytes, 400);
                let b = RE_NON_ASCII.replace_all(&b, "");
                let b = RE_WHITESPACE.replace_all(&b, " ");
                let b = RE_LONG_SPACE.replace_all(&b, " ");
                b.to_string()
            });

            EmailMessage {
                id,
                from,
                label_ids,
                thread_id,
                history_id,
                internal_date,
                subject,
                snippet,
                body,
            }
        })
        .context(format!(
            "No raw message found in message response: {:?}",
            msg
        ))
}

fn build_label_update(
    user_labels: Vec<Label>,
    current_labels: Vec<String>,
    category: Category,
    inbox_settings: &CategoryInboxSettingsMap,
) -> anyhow::Result<(serde_json::Value, LabelUpdate)> {
    static RE_CATEGORY_LABEL: Lazy<Regex> = Lazy::new(|| Regex::new(r"CATEGORY_+").unwrap());

    let current_categories = current_labels
        .iter()
        .filter(|c| RE_CATEGORY_LABEL.is_match(c))
        .cloned()
        .collect::<Vec<_>>();

    let mut categories_to_add = category.gmail_categories;

    let mut categories_to_remove = current_categories
        .iter()
        .filter(|c| !categories_to_add.contains(c))
        .cloned()
        .collect::<Vec<_>>();

    if let Some(setting) = inbox_settings.get(&category.mail_label) {
        // if setting.skip_inbox {
        //     categories_to_remove.push("INBOX".to_string());
        // }
        if setting.mark_spam {
            categories_to_add.push("SPAM".to_string());
        }
    }

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

        if category.important.unwrap_or(false) {
            label_ids.push("IMPORTANT".to_string());
            label_names.push("IMPORTANT".to_string());
        }

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
            added: if label_names_applied.is_empty() {
                None
            } else {
                Some(label_names_applied)
            },
            removed: if categories_to_remove.is_empty() {
                None
            } else {
                Some(categories_to_remove)
            },
        },
    ))
}

const LABEL_COLORS: [(&str, &str, &str); 103] = [
    ("Silver", "#e7e7e7", "#000000"),
    ("Crimson", "#8a1c0a", "#ffffff"),
    ("Orange", "#ff7537", "#ffffff"),
    ("Gold", "#ffad47", "#000000"),
    ("Green", "#1a764d", "#ffffff"),
    ("Teal", "#2da2bb", "#ffffff"),
    ("Blue", "#1c4587", "#ffffff"),
    ("Purple", "#41236d", "#ffffff"),
    ("Salmon", "#efa093", "#000000"),
    ("Red Orange", "#fb4c2f", "#ffffff"),
    ("Light Yellow", "#fad165", "#000000"),
    ("Green", "#16a766", "#ffffff"),
    ("Light Green", "#43d692", "#000000"),
    ("Blue", "#4a86e8", "#ffffff"),
    ("Purple", "#a479e2", "#ffffff"),
    ("Pink", "#f691b3", "#000000"),
    ("Light Pink", "#f6c5be", "#000000"),
    ("Pale Peach", "#ffe6c7", "#000000"),
    ("Light Cream", "#fef1d1", "#000000"),
    ("Pale Green", "#b9e4d0", "#000000"),
    ("Very Light Green", "#c6f3de", "#000000"),
    ("Pale Blue", "#c9daf8", "#000000"),
    ("Lavender", "#e4d7f5", "#000000"),
    ("Light Pink", "#fcdee8", "#000000"),
    ("Light Orange", "#ffd6a2", "#000000"),
    ("Pale Yellow", "#fce8b3", "#000000"),
    ("Mint Green", "#89d3b2", "#000000"),
    ("Light Mint", "#a0eac9", "#000000"),
    ("Light Blue", "#a4c2f4", "#000000"),
    ("Light Lavender", "#d0bcf1", "#000000"),
    ("Light Pink", "#fbc8d9", "#000000"),
    ("Coral", "#e66550", "#ffffff"),
    ("Light Orange", "#ffbc6b", "#000000"),
    ("Pale Yellow", "#fcda83", "#000000"),
    ("Green", "#44b984", "#ffffff"),
    ("Light Green", "#68dfa9", "#000000"),
    ("Blue", "#6d9eeb", "#ffffff"),
    ("Lavender", "#b694e8", "#ffffff"),
    ("Pink", "#f7a7c0", "#000000"),
    ("Dark Red", "#cc3a21", "#ffffff"),
    ("Orange", "#eaa041", "#000000"),
    ("Yellow", "#f2c960", "#000000"),
    ("Dark Green", "#149e60", "#ffffff"),
    ("Light Green", "#3dc789", "#000000"),
    ("Blue", "#3c78d8", "#ffffff"),
    ("Purple", "#8e63ce", "#ffffff"),
    ("Pink", "#e07798", "#000000"),
    ("Dark Red", "#ac2b16", "#ffffff"),
    ("Orange", "#cf8933", "#000000"),
    ("Yellow", "#d5ae49", "#000000"),
    ("Dark Green", "#0b804b", "#ffffff"),
    ("Green", "#2a9c68", "#ffffff"),
    ("Blue", "#285bac", "#ffffff"),
    ("Purple", "#653e9b", "#ffffff"),
    ("Pink", "#b65775", "#000000"),
    ("Dark Red", "#822111", "#ffffff"),
    ("Orange", "#a46a21", "#000000"),
    ("Yellow", "#aa8831", "#000000"),
    ("Dark Green", "#076239", "#ffffff"),
    ("Pink", "#83334c", "#ffffff"),
    ("Dark Gray", "#464646", "#ffffff"),
    ("Light Gray", "#e7e7e7", "#000000"),
    ("Dark Blue", "#0d3472", "#ffffff"),
    ("Light Blue", "#b6cff5", "#000000"),
    ("Dark Teal", "#0d3b44", "#ffffff"),
    ("Light Teal", "#98d7e4", "#000000"),
    ("Dark Purple", "#3d188e", "#ffffff"),
    ("Light Lavender", "#e3d7ff", "#000000"),
    ("Dark Pink", "#711a36", "#ffffff"),
    ("Light Pink", "#fbd3e0", "#000000"),
    ("Light Red", "#f2b2a8", "#000000"),
    ("Dark Brown", "#7a2e0b", "#ffffff"),
    ("Light Brown", "#ffc8af", "#000000"),
    ("Dark Orange", "#7a4706", "#ffffff"),
    ("Light Orange", "#ffdeb5", "#000000"),
    ("Dark Yellow", "#594c05", "#ffffff"),
    ("Light Yellow", "#fbe983", "#000000"),
    ("Dark Brown", "#684e07", "#ffffff"),
    ("Light Brown", "#fdedc1", "#000000"),
    ("Dark Green", "#0b4f30", "#ffffff"),
    ("Light Green", "#b3efd3", "#000000"),
    ("Dark Green", "#04502e", "#ffffff"),
    ("Light Green", "#a2dcc1", "#000000"),
    ("Gray", "#c2c2c2", "#000000"),
    ("Blue", "#4986e7", "#ffffff"),
    ("Lavender", "#b99aff", "#000000"),
    ("Dark Pink", "#994a64", "#ffffff"),
    ("Pink", "#f691b2", "#000000"),
    ("Golden Yellow", "#ffad46", "#000000"),
    ("Dark Red", "#662e37", "#ffffff"),
    ("Light Gray", "#ebdbde", "#000000"),
    ("Light Pink", "#cca6ac", "#000000"),
    ("Dark Green", "#094228", "#ffffff"),
    ("Light Green", "#42d692", "#000000"),
    ("Green", "#16a765", "#ffffff"),
    ("White", "#ffffff", "#000000"),
    ("Black", "#000000", "#ffffff"),
    ("Dark Gray", "#434343", "#ffffff"),
    ("Gray", "#666666", "#ffffff"),
    ("Light Gray", "#999999", "#000000"),
    ("Very Light Gray", "#cccccc", "#000000"),
    ("Pale Gray", "#efefef", "#000000"),
    ("Almost White", "#f3f3f3", "#000000"),
];

// fn get_label_color(_label: &str) -> LabelColor {
//     let color_map = Lazy::new(|| LABEL_COLORS.iter().cloned().collect::<HashMap<_, _>>());
//     let bg = color_map.get("Dark Gray").map(|c| c.to_string());
//     let text = color_map.get("White").map(|c| c.to_string());
//     LabelColor {
//         background_color: bg,
//         text_color: text,
//     }
// }

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
                content: "Advertisment".to_string(),
                mail_label: "mailclerk:ads".to_string(),
                gmail_categories: vec!["CATEGORY_PROMOTIONS".to_string()],
                important: None,
            },
            &HashMap::new(),
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
                        added: Some(vec![
                            "CATEGORY_PROMOTIONS".to_string(),
                            "mailclerk:ads".to_string()
                        ]),
                        removed: Some(vec!["CATEGORY_SOCIAL".to_string()])
                    }
                );
            }
            Err(e) => panic!("Error: {:?}", e),
        }
    }

    #[test]
    fn test_sanitize_message() {
        use super::*;
        use google_gmail1::api::Message;
        use std::fs;

        let root = env!("CARGO_MANIFEST_DIR");

        let path = format!("{root}/src/testing/data/jobot_message.json");
        let json = fs::read_to_string(path).expect("Unable to read file");

        let message = serde_json::from_str::<Message>(&json).expect("Unable to parse json");

        let sanitized = sanitize_message(message).expect("Unable to sanitize message");
        let test = EmailMessage {
                    id: "1921e8debe9a2256".to_string(),
        label_ids: vec![
            "Label_29".to_string(),
            "Label_5887327980780978551".to_string(),
            "CATEGORY_UPDATES".to_string(),
            "INBOX".to_string(),
        ],
        thread_id: "1921e8debe9a2256".to_string(),
        history_id: 12323499,
        internal_date: 1727089470000,
        from: Some(
            "jobs@alerts.jobot.com".to_string(),
        ),
        subject: Some(
            "Remote Sr. JavaScript Engineer openings are available. Apply Now.".to_string(),
        ),
        snippet: "Apply Now, Rachel and Charles are hiring for Remote Sr. JavaScript Engineer and Software Engineer roles! ".to_string(),
        body: Some(
            concat!(
                "Apply Now, Rachel and Charles are hiring for Remote Sr. JavaScript Engineer and Software Engineer roles! [Jobot logo] ",
            "12 New Jobs for your Job Search Recommended Apply -- Based on your resume Remote Sr. JavaScript Engineer [[LINK]] Growing health-tech startup seeks a Remote Sr. JavaScriptEngineer to join their team! [] REMOTE [] Washington, DC [] $130,000 - $155,000 1-Click Apply [[LINK]] [Rachel Hilton Berry] Rachel ", 
            "Also Consider -- Based on your resume Software Engineer [[LINK]] build out modern web applications and automated deployment pipelines 100% from home [] REMOTE [] McLean, VA [] $100,000 - $140,000 1-Click Apply [[LINK]] [Charles Simmons] Charles ", 
            "AlsoConsider -- Based on your resume Sr. Software Engineer [[LINK]] 100% Remote Role, Innovative Legal Software Company [] REMOTE [] Oklahoma City, OK [] $140,000 - $160,000 1-Click Apply [[LINK]] [Duran Workman] Duran ", 
            "Also Consider -- Based on your resume Senior Software Engineer [[LINK]] [] REMOTE [] Oklahoma City, OK +1 [] $115,000 - $155,000 1-Click Apply [[LINK]] [Dan Dungy] Dan ", 
            "AlsoConsider -- Based on your resume Frontend Developer - Remote [[LINK]] Growing tech company in the supply chain space is hiring for a Frontend Software Developer! [] REMOTE [] Chicago, IL [] $90,000 - $115,000 1-Click Apply [[LINK]] [Sydney Weaver] Sydney ",
            "Also Consider -- Based on your resume Flutter and Dart Engineer [[LINK]] 100% remote - Contract to Hire - Native Development [] REMOTE[] Cincinnati, OH +2 [] $45 - $55 1-Click Apply [[LINK]] [Chuck Wirtz] Chuck ", 
            "Also Consider -- Based on your resume Mobile Developer - Specializing in NFC Tech [[LINK]] [] REMOTE [] Austin, TX [] $50 - $80 1-Click Apply [[LINK]] [Ashley Elm] Ashley ", 
            "Also Consider -- Based on your resume Senior Software Engineer (Swift Integrations) [[LINK]] Remote Opportunity/AI Start Up/ Blockchain []REMOTE [] San Jose, CA [] $170,000 - $210,000 1-Click Apply [[LINK]] [Heather Burnach] Heather ", 
            "Also Consider -- Based on your resume Lead Growth Engineer [[LINK]] Lead Growth Engineer (PST, Remote) with scaling health/wellness startup- $90M, Series B [] REMOTE [] West Hollywood, CA [] $165,000 - $215,000 1-Click Apply [[LINK]] [Oliver Belkin] Oliver ",
            "Also Consider -- Based on your resumeSenior Software Engineer-(PHP, TypeScript, Node, AWS) [[LINK]] Senior Software Engineer-CONTRACT-REMOTE [] REMOTE [] Charlotte, NC [] $70 - $90 1-Click Apply [[LINK]] [Chris Chomic] Chris ",
            "Also Consider -- Based on your resume Senior React Native Developer [[LINK]] [] REMOTE [] San Francisco, CA [] $160,000 - $180,000 1-Click Apply [[LINK]] [Joe Lynch] Joe ",
            "Also Consider -- Based on yourresume Senior Backend Engineer [[LINK]] Build out modern platforms supporting the short term rental SaaS space 100% Remote [] REMOTE [] Philadelphia, PA +1 [] $130,000 - $175,000 1-Click Apply [[LINK]] [Charles Simmons] Charles [LinkedIn logo] [[LINK]][Instagram logo] [[LINK]] Jobot.com [[LINK]] | Unsubscribe [[LINK]] Copyright Jobot, LLC, All rights reserved. 3101 West Pacific Coast Hwy,Newport Beach, CA 92663"
        ).to_string())
        };
        assert_eq!(sanitized, test);
    }
}

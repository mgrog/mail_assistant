extern crate google_gmail1 as gmail1;
use futures::future::join_all;
use google_gmail1::{
    api::{Label, ListMessagesResponse, Message, MessagePart, Profile, Thread},
    hyper::{client::HttpConnector, Body, Response},
    hyper_rustls::HttpsConnector,
    oauth2::{AccessTokenAuthenticator, InstalledFlowAuthenticator},
};
use lazy_static::lazy_static;
use mail_parser::MessageParser;
use once_cell::sync::Lazy;
use regex::Regex;

pub struct EmailClient {
    http_client: reqwest::Client,
    access_token: String,
}

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub subject: Option<String>,
    pub snippet: String,
    pub body: Option<String>,
}

impl EmailClient {
    pub async fn new(
        http_client: reqwest::Client,
        access_token: String,
    ) -> anyhow::Result<EmailClient> {
        Ok(EmailClient {
            http_client,
            access_token,
        })
    }

    // pub async fn get_redirect_url(&self) -> anyhow::Result<String> {
    //     self.hub.auth.
    // }

    pub async fn get_messages(&self) -> anyhow::Result<Vec<EmailMessage>> {
        let resp = self
            .http_client
            .get("https://www.googleapis.com/gmail/v1/users/me/messages")
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        let messages = resp
            .json::<ListMessagesResponse>()
            .await?
            .messages
            .unwrap_or_default();

        let mut requests = vec![];
        for msg in messages {
            let id = msg.id;
            if id.is_none() {
                continue;
            }
            let task = async {
                let req = self
                    .http_client
                    .get(&format!(
                        "https://www.googleapis.com/gmail/v1/users/me/messages/{}",
                        id.unwrap()
                    ))
                    .bearer_auth(&self.access_token)
                    .query(&[("format", "RAW")])
                    .send()
                    .await?;

                req.json::<Message>().await
            };
            requests.push(task);
        }
        let responses = join_all(requests.into_iter().take(5))
            .await
            .into_iter()
            .map(|result| result.ok())
            .collect::<Vec<_>>();

        // println!("Responses {:?}", responses);

        static RE_WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\r\t\n]+").unwrap());
        static RE_NON_UNICODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^\x00-\x80]").unwrap());
        static RE_HTTP_LINK: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"https?:\/\/(www\.)?[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_\+.~#?&//=]*)").unwrap()
        });

        let messages = responses
            .into_iter()
            .filter_map(|resp| {
                resp.and_then(|msg| {
                    println!("Raw {:?}", msg.raw);
                    let snippet = msg.snippet.unwrap_or_default();
                    msg.raw.map(|input| {
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
                            let b = RE_HTTP_LINK.replace_all(&b, "[LINK_REMOVED]");
                            b.to_string()
                        });

                        EmailMessage {
                            subject,
                            snippet,
                            body,
                        }
                    })
                })
            })
            .collect();

        println!("Messages {:?}", messages);

        Ok(messages)
    }

    // pub async fn get_threads(&self) -> anyhow::Result<Vec<Thread>> {
    //     let (_, resp) = self.hub.users().threads_list("me").doit().await?;
    //     Ok(resp.threads.unwrap_or_default())
    // }

    // pub async fn get_labels(&self) -> anyhow::Result<Vec<Label>> {
    //     let (_, resp) = self.hub.users().labels_list("me").doit().await?;
    //     Ok(resp.labels.unwrap_or_default())
    // }

    // pub async fn configure_labels(&self) -> anyhow::Result<Response<Body>> {
    //     let existing_labels = self.get_labels().await?;
    //     // let required_labels =
    //     unimplemented!()
    // }

    pub async fn get_profile(&self) -> anyhow::Result<Profile> {
        let resp = self
            .http_client
            .get("https://www.googleapis.com/gmail/v1/users/me/profile")
            .bearer_auth(&self.access_token)
            .send()
            .await?;
        Ok(resp.json::<Profile>().await?)
    }
}

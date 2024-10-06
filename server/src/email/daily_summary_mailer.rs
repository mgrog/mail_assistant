use anyhow::Context;
use entity::{prelude::*, processed_email, user_session};
use google_gmail1::api::Message;
use minijinja::render;
use sea_orm::{entity::*, query::*};
use std::collections::{HashMap, VecDeque};

use sea_orm::DatabaseConnection;

use crate::{email::email_template::DAILY_SUMMARY_EMAIL_TEMPLATE, structs::error::AppResult};

pub struct DailySummaryMailer {
    conn: DatabaseConnection,
    users_to_send: VecDeque<user_session::Model>,
}

impl DailySummaryMailer {
    pub async fn new(conn: DatabaseConnection) -> AppResult<Self> {
        let user_sessions = UserSession::find().all(&conn).await?;

        Ok(Self {
            conn,
            users_to_send: user_sessions.into_iter().collect(),
        })
    }

    // fn send(&mut self, user_session: &user_session::Model, summary: &DailySummary) {
    //     // ...
    // }

    pub async fn send_to_all_users(&mut self) {
        while let Some(user_session) = self.users_to_send.pop_front() {
            match ProcessedEmail::find()
                .filter(processed_email::Column::UserSessionId.eq(user_session.id))
                .all(&self.conn)
                .await
            {
                Ok(processed_emails) if !processed_emails.is_empty() => {
                    tracing::info!("Sending daily email for user {}", user_session.email);
                    let summary = self
                        .construct_daily_summary(&user_session.email, processed_emails)
                        .expect("Could not construct daily summary");
                    tracing::info!(
                        "Daily email sent, {} users remaining",
                        self.users_to_send.len()
                    );
                }
                Ok(_) => {
                    // No emails to send
                    tracing::info!("No emails to send for user {}", user_session.email);
                }
                Err(e) => {
                    tracing::error!(
                        "Could not fetch emails for user {}: {:?}",
                        user_session.email,
                        e
                    );
                    self.users_to_send.push_back(user_session);
                }
            }

            // self.send(user, &summary);
        }
    }

    fn construct_daily_summary(
        &self,
        user_email: &str,
        processed_emails: Vec<processed_email::Model>,
    ) -> anyhow::Result<Message> {
        let email = lettre::Message::builder()
            .to(format!("<{user_email}>")
                .parse()
                .context("Could not parse to in daily summary message builder")?)
            .from("Mailclerk <noreply@mailclerk.io>".parse()?)
            .body({
                let mut category_counts = HashMap::new();
                for email in processed_emails {
                    let labels = email.labels_applied;
                    let label = labels
                        .iter()
                        .find(|label| label.contains("mailclerk:"))
                        .cloned();

                    if let Some(label) = label {
                        let category = label.split(":").last().unwrap().to_string();
                        let count = category_counts.entry(capitalize(&category)).or_insert(0);
                        *count += 1;
                    }
                }
                let category_counts = category_counts.into_iter().collect::<Vec<_>>();
                let r = render!(DAILY_SUMMARY_EMAIL_TEMPLATE, user_email, category_counts);
                // println!("Rendered email: {:?}", r);
                r
            })?;

        let msg = Message {
            id: None,
            thread_id: None,
            label_ids: Some(vec![]),
            snippet: None,
            history_id: None,
            internal_date: None,
            payload: None,
            size_estimate: None,
            raw: Some(email.formatted()),
        };

        Ok(msg)
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// struct

// struct DailySummary {

// }

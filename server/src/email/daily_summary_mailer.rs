use anyhow::Context;
use chrono::{Duration, Utc};
use entity::{prelude::*, processed_email, user_session};
use google_gmail1::api::Message;
use minijinja::render;
use sea_orm::{entity::*, query::*};
use std::collections::{HashMap, VecDeque};

use sea_orm::DatabaseConnection;

use crate::{
    email::{client::EmailClient, email_template::DAILY_SUMMARY_EMAIL_TEMPLATE},
    structs::error::AppResult,
    HttpClient,
};

pub struct DailySummaryMailer {
    conn: DatabaseConnection,
    http_client: HttpClient,
    users_to_send: VecDeque<user_session::Model>,
}

impl DailySummaryMailer {
    pub async fn new(
        conn: DatabaseConnection,
        http_client: HttpClient,
        active_user_sessions: Vec<user_session::Model>,
    ) -> AppResult<Self> {
        Ok(Self {
            conn,
            http_client,
            users_to_send: active_user_sessions.into_iter().collect(),
        })
    }

    async fn send(
        &mut self,
        user_session: &user_session::Model,
        processed_emails: Vec<processed_email::Model>,
    ) -> anyhow::Result<()> {
        tracing::info!("Sending daily email for user {}", user_session.email);
        let raw_email = self.construct_daily_summary(&user_session.email, processed_emails)?;

        let email_client = EmailClient::new(
            self.http_client.clone(),
            self.conn.clone(),
            user_session.clone(),
        )
        .await?;

        let daily_summary_label_id = email_client.get_daily_summary_label_id().await?;

        let message = Message {
            id: None,
            thread_id: None,
            label_ids: Some(vec!["INBOX".to_string(), daily_summary_label_id]),
            snippet: None,
            history_id: None,
            internal_date: None,
            payload: None,
            size_estimate: None,
            raw: Some(raw_email),
        };

        email_client.insert_message(message).await?;

        Ok(())
    }

    pub async fn send_to_all_users(&mut self) {
        let twenty_four_hours_ago = Utc::now() - Duration::hours(24);

        while let Some(user_session) = self.users_to_send.pop_front() {
            match ProcessedEmail::find()
                .filter(processed_email::Column::UserSessionId.eq(user_session.id))
                .filter(processed_email::Column::ProcessedAt.gt(twenty_four_hours_ago))
                .all(&self.conn)
                .await
            {
                Ok(processed_emails) if !processed_emails.is_empty() => {
                    match self.send(&user_session, processed_emails).await {
                        Ok(_) => {
                            tracing::info!(
                                "Daily email sent for user {}, {} users remaining",
                                user_session.email,
                                self.users_to_send.len()
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                "Could not send daily email for user {}: {:?}",
                                user_session.email,
                                e
                            );
                        }
                    }
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
                }
            }

            // self.send(user, &summary);
        }
    }

    fn construct_daily_summary(
        &self,
        user_email: &str,
        processed_emails: Vec<processed_email::Model>,
    ) -> anyhow::Result<Vec<u8>> {
        let email = lettre::Message::builder()
            .to(format!("<{user_email}>")
                .parse()
                .context("Could not parse to in daily summary message builder")?)
            .from("Mailclerk <noreply@mailclerk.io>".parse()?)
            .subject("Breakdown of your emails from the last 24 hours")
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

        Ok(email.formatted())
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

#[cfg(test)]
mod tests {
    use std::{env, str::FromStr};

    use chrono::DateTime;
    use sea_orm::{ConnectOptions, Database, DbBackend};

    use super::*;

    async fn setup_conn() -> DatabaseConnection {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
        let mut db_options = ConnectOptions::new(db_url);
        db_options.sqlx_logging(false);

        Database::connect(db_options)
            .await
            .expect("Database connection failed")
    }

    #[tokio::test]
    async fn test_query() {
        // let conn = setup_conn().await;
        let dt: DateTime<Utc> = DateTime::from_str("2024-10-07 20:04:19 +00:00").unwrap();

        let query = ProcessedEmail::find()
            .filter(processed_email::Column::UserSessionId.eq(1))
            .filter(processed_email::Column::ProcessedAt.gt(dt))
            .build(DbBackend::Postgres)
            .to_string();

        assert_eq!(query, "SELECT \"processed_email\".\"id\", \"processed_email\".\"user_session_id\", \"processed_email\".\"user_session_email\", \"processed_email\".\"processed_at\", \"processed_email\".\"labels_applied\", \"processed_email\".\"labels_removed\", \"processed_email\".\"ai_answer\" FROM \"processed_email\" WHERE \"processed_email\".\"user_session_id\" = 1 AND \"processed_email\".\"processed_at\" > '2024-10-07 20:04:19 +00:00'");
    }
}

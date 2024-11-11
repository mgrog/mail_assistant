#![allow(dead_code)]
#[macro_use]
mod macros;

mod api_quota;
mod auth_session_store;
mod cron_time_utils;
mod db_core;
mod email;
mod error;
mod model;
mod prompt;
mod rate_limiters;
mod request_tracing;
mod routes;
mod server_config;

use std::{
    env,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use anyhow::Context;
use auth_session_store::AuthSessionStore;
use axum::{extract::FromRef, http::StatusCode, response::IntoResponse, Router};
use cron_time_utils::parse_offset_str;
use db_core::prelude::*;
use email::{
    active_email_processors::ActiveProcessingQueue, tasks::email_processing_queue_cleanup,
};
use futures::future::join_all;
use mimalloc::MiMalloc;
use rate_limiters::RateLimiters;
use reqwest::Certificate;
use routes::AppRouter;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, EntityTrait, QueryFilter};
use server_config::get_cert;
use std::sync::atomic::Ordering::Relaxed;
use tokio::{signal, task::JoinHandle};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub type TokenCounter = Arc<AtomicU64>;
pub type HttpClient = reqwest::Client;
pub type PubsubClient = Arc<google_cloud_pubsub::client::Client>;

#[derive(Clone, FromRef)]
struct ServerState {
    http_client: HttpClient,
    conn: DatabaseConnection,
    token_count: TokenCounter,
    rate_limiters: RateLimiters,
    session_store: AuthSessionStore,
}

impl ServerState {
    fn add_global_token_count(&self, count: i64) {
        self.token_count.fetch_add(count as u64, Relaxed);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env::set_var("RUST_LOG", "info");
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
    let mut db_options = ConnectOptions::new(db_url);
    db_options.sqlx_logging(false);

    let conn = Database::connect(db_options)
        .await
        .expect("Database connection failed");

    let cert = get_cert();
    let http_client = reqwest::ClientBuilder::new()
        .use_rustls_tls()
        .add_root_certificate(Certificate::from_pem(&cert)?)
        .build()?;
    let session_store = AuthSessionStore::new();

    let state = ServerState {
        http_client,
        conn,
        token_count: Arc::new(AtomicU64::new(0)),
        rate_limiters: RateLimiters::from_env(),
        session_store,
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_env("RUST_LOG"))
        .with(tracing_subscriber::fmt::Layer::default().with_ansi(false))
        .init();

    let router = AppRouter::create(state.clone());
    let email_processing_queue = ActiveProcessingQueue::new(state.clone(), 1_000);
    let queue_watch_handle = email_processing_queue.watch().await;
    let processor_cleanup_handle = email_processing_queue_cleanup(email_processing_queue.clone());

    let mut scheduler = JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    {
        let state_clone = state.clone();
        let queue = email_processing_queue.clone();
        // Run full sync at startup
        scheduler
            .add(Job::new_one_shot_async(
                Duration::from_secs(0),
                move |uuid, _l| {
                    let state = state_clone.clone();
                    let queue = queue.clone();
                    tracing::info!("Running full sync job {}", uuid);
                    Box::pin(async move {
                        match email::tasks::process_unsynced_user_emails(state, queue).await {
                            Ok(_) => {
                                tracing::info!("Email processor job {} succeeded", uuid);
                            }
                            Err(e) => {
                                tracing::error!("Job failed: {:?}", e);
                            }
                        }
                    })
                },
            )?)
            .await?;

        let state_clone = state.clone();
        let queue = email_processing_queue.clone();
        scheduler
            .add(Job::new_async("0 * * * * *", move |uuid, mut l| {
                let state = state_clone.clone();
                let queue = queue.clone();
                tracing::info!("Running full sync job {}", uuid);
                Box::pin(async move {
                    match email::tasks::process_unsynced_user_emails(state, queue).await {
                        Ok(_) => {
                            tracing::info!("Email processor job {} succeeded", uuid);
                        }
                        Err(e) => {
                            tracing::error!("Job failed: {:?}", e);
                        }
                    }

                    // Query the next execution time for this job
                    let next_tick = l.next_tick_for_job(uuid).await;
                    match next_tick {
                        Ok(Some(ts)) => {
                            println!("Next time for email sync job is {:?}", ts)
                        }
                        _ => {
                            println!("Could not get next tick for email sync job")
                        }
                    }
                })
            })?)
            .await?;

        let user_settings_with_active_subscriptions = UserSettings::find()
            .find_also_related(User)
            .filter(user::Column::SubscriptionStatus.eq(SubscriptionStatus::Active))
            .all(&state.conn)
            .await?;

        for (user_setting, user) in user_settings_with_active_subscriptions {
            let state = state.clone();
            let offset = match parse_offset_str(&user_setting.user_time_zone_offset) {
                Ok(offset) => offset,
                Err(e) => {
                    tracing::error!("Failed to parse offset: {:?}", e);
                    continue;
                }
            };
            let user = user.context("User not found")?;

            tracing::info!(
                "Adding daily summary mailer job for user {} at {}{}",
                user_setting.user_email,
                user_setting.daily_summary_time,
                user_setting.user_time_zone_offset
            );
            let cron_time = format!("0 0 {} * * *", user_setting.daily_summary_time);
            scheduler
                .add(
                    Job::new_async_tz(cron_time, offset, move |uuid, mut l| {
                        let state = state.clone();
                        Box::pin(async move {
                            match email::tasks::send_user_daily_email_summary(&state, user.id).await
                            {
                                Ok(_) => {
                                    tracing::info!("Daily summary mailer job {} succeeded", uuid);
                                }
                                Err(e) => {
                                    tracing::error!("Job failed: {:?}", e);
                                }
                            };

                            // Query the next execution time for this job
                            let next_tick = l.next_tick_for_job(uuid).await;
                            match next_tick {
                                Ok(Some(ts)) => {
                                    println!("Next time for daily summary mailer job is {:?}", ts)
                                }
                                _ => {
                                    println!("Could not get next tick for daily summary mailer job")
                                }
                            }
                        })
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
        }

        // Cleanup session storage
        let state_clone = state.clone();
        scheduler
            .add(Job::new_repeated(
                Duration::from_secs(60),
                move |_uuid, _lock| {
                    tracing::info!("Running session storage cleanup job");
                    state_clone.session_store.clean_store();
                },
            )?)
            .await?;
    }

    scheduler.shutdown_on_ctrl_c();

    scheduler.set_shutdown_handler(Box::new(move || {
        Box::pin(async move {
            tracing::info!("Shutting down scheduler");
        })
    }));

    match scheduler.start().await {
        Ok(_) => {
            tracing::info!("Scheduler started");
        }
        Err(e) => {
            tracing::error!("Failed to start scheduler: {:?}", e);
        }
    }

    // Handle Ctrl+C
    let shutdown_handle = {
        tokio::spawn(async move {
            if env::var("NO_SHUTDOWN").unwrap_or("false".to_string()) == "true" {
                return;
            }
            signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
            tracing::info!("Received Ctrl+C, shutting down");
            scheduler.shutdown().await.unwrap();
            println!("Cleanups done, shutting down");
            std::process::exit(0);
        })
    };

    join_all(vec![
        run_server(router),
        // inbox_subscription_handle,
        shutdown_handle,
        queue_watch_handle,
        // process_emails_from_inbox_notifications_task,
        processor_cleanup_handle,
    ])
    .await;

    Ok(())
}

fn run_server(router: Router) -> JoinHandle<()> {
    tokio::spawn(async {
        // Start the server
        let port = env::var("PORT").unwrap_or("5006".to_string());
        tracing::info!("Auto email running on http://0.0.0.0:{}", port);
        // check config
        println!("{}", *server_config::cfg);

        // run it with hyper
        let addr = SocketAddr::from(([0, 0, 0, 0], port.parse::<u16>().unwrap()));
        tracing::debug!("listening on {addr}");
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        axum::serve(listener, router).await.unwrap();
    })
}

pub async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Route does not exist")
}

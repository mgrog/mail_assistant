#[macro_use]
mod macros;

mod api_quota;
mod cron_time_utils;
mod db_core;
mod email;
mod model;
mod prompt;
mod rate_limiters;
mod request_tracing;
mod routes;
mod server_config;
mod settings;

use std::{
    env,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc,
    },
    time::Duration,
};

use axum::{extract::FromRef, http::StatusCode, response::IntoResponse, routing::get, Router};
use cron_time_utils::parse_offset_str;
use email::{
    active_email_processors::ActiveProcessingQueue,
    tasks::{email_processing_queue_cleanup, process_emails_from_inbox_notifications},
};
use entity::prelude::*;
use futures::future::join_all;
use google_cloud_pubsub::{
    client::google_cloud_auth::credentials::CredentialsFile, subscription::SubscriptionConfig,
};
use mimalloc::MiMalloc;
use model::response::GmailWatchInboxPushNotification;
use rate_limiters::RateLimiters;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, EntityTrait};
use std::sync::atomic::Ordering::Relaxed;
use tokio::{signal, sync::mpsc, task::JoinHandle};
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;
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
}

impl ServerState {
    #[allow(dead_code)]
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

    let creds =
        CredentialsFile::new_from_file(format!("{}/google_creds.json", env!("CARGO_MANIFEST_DIR")))
            .await
            .expect("Failed to create credentials file");

    let pubsub_config = google_cloud_pubsub::client::ClientConfig::default()
        .with_credentials(creds)
        .await
        .expect("Failed to create pubsub client config");

    let pubsub_client = Arc::new(
        google_cloud_pubsub::client::Client::new(pubsub_config)
            .await
            .expect("Failed to create pubsub client"),
    );

    // Topic for user mailboxes
    let topic = pubsub_client.topic("mailclerk-user-inboxes");

    // Create subscription to user mailbox topic
    let subscription = pubsub_client.subscription("user-inboxes-subscription");
    if !subscription
        .exists(None)
        .await
        .expect("Failed to check if subscription exists")
    {
        match subscription
            .create(
                topic.fully_qualified_name(),
                SubscriptionConfig::default(),
                None,
            )
            .await
        {
            Ok(_) => {
                tracing::info!("Created inbox subscription");
            }
            Err(e) => {
                tracing::error!("Failed to create inbox subscription: {:?}", e);
            }
        };
    } else {
        tracing::info!("Inbox subscription already exists");
    }

    let (inbox_sender, inbox_receiver) = mpsc::channel::<GmailWatchInboxPushNotification>(100_000);

    let cancel_inbox_subscription = CancellationToken::new();
    let is_subscription_cleaned_up = Arc::new(AtomicBool::new(false));
    tracing::info!("Building inbox subscription listener");
    let inbox_subscription_handle = {
        let cancel = cancel_inbox_subscription.clone();
        let is_subscription_cleaned_up = is_subscription_cleaned_up.clone();
        tracing::info!("Starting inbox subscription listener");
        let inbox_sender = inbox_sender.clone();
        tokio::spawn(async move {
            tracing::info!("Listening for inbox messages...");
            let sub = subscription
                .receive(
                    move |message, _cancel| {
                        let data_result = serde_json::from_str::<GmailWatchInboxPushNotification>(
                            &String::from_utf8_lossy(message.message.data.as_slice()),
                        );
                        let inbox_sender = inbox_sender.clone();
                        async move {
                            match data_result {
                                Ok(data) => {
                                    tracing::info!("Received inbox message: {:?}", data);
                                    let _ = inbox_sender.send(data).await;
                                    let _ = message.ack().await;
                                }
                                Err(e) => {
                                    let raw =
                                        String::from_utf8_lossy(message.message.data.as_slice());
                                    tracing::error!(
                                        "Failed to parse inbox message: {}\n{:?}",
                                        raw,
                                        e
                                    );
                                }
                            }
                        }
                    },
                    cancel,
                    None,
                )
                .await;

            match sub {
                Ok(_) => {
                    tracing::info!("Subscription ended");
                }
                Err(e) => {
                    tracing::info!("Subscription error: {:?}", e);
                }
            };
            subscription.delete(None).await.unwrap();
            is_subscription_cleaned_up.store(true, Relaxed);
        })
    };

    let state = ServerState {
        http_client: reqwest::Client::new(),
        conn,
        token_count: Arc::new(AtomicU64::new(0)),
        rate_limiters: RateLimiters::from_env(),
    };

    // Watch user inboxes
    email::tasks::subscribe_to_inboxes(state.conn.clone(), state.http_client.clone())
        .await
        .expect("Failed to subscribe to inboxes");

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_env("RUST_LOG"))
        .with(tracing_subscriber::fmt::Layer::default().with_ansi(false))
        .init();

    let router = Router::new()
        .route("/", get(|| async { "Mailclerk server" }))
        .route("/auth", get(routes::auth::handler_auth_gmail))
        .route(
            "/auth/callback",
            get(routes::auth::handler_auth_gmail_callback),
        )
        .route(
            "/auth_token/callback",
            get(routes::auth::handler_auth_token_callback),
        )
        .layer(request_tracing::trace_with_request_id_layer())
        .layer(CorsLayer::permissive())
        .layer(CookieManagerLayer::new())
        .with_state(state.clone())
        .fallback(handler_404);

    let email_processing_queue = ActiveProcessingQueue::new(state.clone(), 1_000);
    let queue_watch_handle = email_processing_queue.watch().await;
    let process_emails_from_inbox_notifications_task =
        process_emails_from_inbox_notifications(inbox_receiver, email_processing_queue.clone());
    let processor_cleanup_handle = email_processing_queue_cleanup(email_processing_queue.clone());

    let mut scheduler = JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    {
        let state_clone = state.clone();
        // Run full sync at startup
        scheduler
            .add(Job::new_one_shot_async(
                Duration::from_secs(0),
                move |uuid, _l| {
                    let state = state_clone.clone();
                    let queue = email_processing_queue.clone();
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

        let user_settings = UserSettings::find()
            .all(&state.conn)
            .await
            .expect("Failed to fetch user settings");

        for user_setting in user_settings {
            let state = state.clone();
            let offset = match parse_offset_str(&user_setting.user_time_zone_offset) {
                Ok(offset) => offset,
                Err(e) => {
                    tracing::error!("Failed to parse offset: {:?}", e);
                    continue;
                }
            };

            tracing::info!(
                "Adding daily summary mailer job for user {} at {}{}",
                user_setting.user_session_id,
                user_setting.daily_summary_time,
                user_setting.user_time_zone_offset
            );
            let cron_time = format!("0 0 {} * * *", user_setting.daily_summary_time);
            dbg!(&cron_time);
            scheduler
                .add(
                    Job::new_async_tz(cron_time, offset, move |uuid, mut l| {
                        let state = state.clone();
                        Box::pin(async move {
                            match email::tasks::send_user_daily_email_summary(
                                &state,
                                user_setting.user_session_id,
                            )
                            .await
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

        // Resubscribe to inboxes every 8 hours
        let state_clone = state.clone();
        scheduler
            .add(Job::new_async("0 0 0,8,16 * * *", move |uuid, _l| {
                let conn = state_clone.conn.clone();
                let http_client = state_clone.http_client.clone();
                Box::pin(async move {
                    println!("Running subscribe to inboxes job {}", uuid);
                    match email::tasks::subscribe_to_inboxes(conn, http_client).await {
                        Ok(_) => {
                            tracing::info!("Subscribe to inboxes job {} succeeded", uuid);
                        }
                        Err(e) => {
                            tracing::error!("Job failed: {:?}", e);
                        }
                    }
                })
            })?)
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
            signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
            tracing::info!("Received Ctrl+C, shutting down");
            scheduler.shutdown().await.unwrap();
            cancel_inbox_subscription.cancel();
            println!("Waiting to clean inbox subscription...");
            while !is_subscription_cleaned_up.load(Relaxed) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            println!("Inbox subscription deleted");
            println!("Cleanups done, shutting down");
            std::process::exit(0);
        })
    };

    join_all(vec![
        run_server(router),
        inbox_subscription_handle,
        shutdown_handle,
        queue_watch_handle,
        process_emails_from_inbox_notifications_task,
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

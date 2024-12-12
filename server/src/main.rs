#![allow(dead_code)]
#[macro_use]
mod macros;

mod auth;
mod cron_time_utils;
mod db_core;
mod email;
mod error;
mod model;
mod notify;
mod prompt;
mod rate_limiters;
mod request_tracing;
mod routes;
mod server_config;
mod testing;

use std::{
    env,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

use auth::session_store::AuthSessionStore;
use axum::{extract::FromRef, Router};
use db_core::prelude::*;
use email::active_email_processors::ActiveEmailProcessorMap;
use futures::future::join_all;
use mimalloc::MiMalloc;
use prompt::priority_queue::PromptPriorityQueue;
use rate_limiters::RateLimiters;
use reqwest::Certificate;
use routes::AppRouter;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use server_config::get_cert;
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
    rate_limiters: RateLimiters,
    session_store: AuthSessionStore,
    pub priority_queue: PromptPriorityQueue,
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
        rate_limiters: RateLimiters::from_env(),
        session_store,
        priority_queue: PromptPriorityQueue::new(),
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_env("RUST_LOG"))
        .with(tracing_subscriber::fmt::Layer::default().with_ansi(false))
        .init();

    let router = AppRouter::create(state.clone());
    let email_processing_map = ActiveEmailProcessorMap::new(state.clone());
    let processing_watch_handle = email::tasks::watch(
        state.priority_queue.clone(),
        email_processing_map.clone(),
        state.rate_limiters.clone(),
    );

    let mut scheduler = JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    {
        let state_clone = state.clone();
        let map = email_processing_map.clone();
        scheduler
            .add(Job::new_one_shot_async(
                Duration::from_secs(1),
                move |uuid, l| {
                    create_processors_for_users(uuid, l, state_clone.clone(), map.clone())
                },
            )?)
            .await?;

        let queue = state.priority_queue.clone();
        let map = email_processing_map.clone();
        scheduler
            .add(Job::new_one_shot(
                Duration::from_secs(2),
                move |_uuid, _l| {
                    email::tasks::run_email_processing_loop(queue.clone(), map.clone());
                },
            )?)
            .await?;

        let queue = state.priority_queue.clone();
        let map = email_processing_map.clone();
        scheduler
            .add(Job::new_one_shot(
                chrono::Duration::minutes(30).to_std().unwrap(),
                move |_uuid, _l| {
                    email::tasks::run_email_processing_loop(queue.clone(), map.clone());
                },
            )?)
            .await?;

        let state_clone = state.clone();
        let map = email_processing_map.clone();
        scheduler
            .add(Job::new_async("0 * * * * *", move |uuid, l| {
                create_processors_for_users(uuid, l, state_clone.clone(), map.clone())
            })?)
            .await?;

        let http_client = state.http_client.clone();
        let conn = state.conn.clone();
        scheduler
            .add(Job::new_async("0 0 * * * *", move |uuid, mut l| {
                let http_client = http_client.clone();
                let conn = conn.clone();
                Box::pin(async move {
                    tracing::info!("Running auto cleanup job {}", uuid);
                    match email::tasks::run_auto_email_cleanup(http_client, conn).await {
                        Ok(_) => {
                            tracing::info!("Auto cleanup job {} succeeded", uuid);
                        }
                        Err(e) => {
                            tracing::error!("Failed to run auto cleanup: {:?}", e);
                        }
                    }

                    let next_tick = l.next_tick_for_job(uuid).await;
                    if let Ok(Some(ts)) = next_tick {
                        tracing::info!("Next time for auto cleanup job is {:?}", ts)
                    }
                })
            })?)
            .await?;

        // Cleanup session storage
        let state_clone = state.clone();
        scheduler
            .add(Job::new_repeated(
                Duration::from_secs(3 * 60),
                move |_uuid, _lock| {
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
        processing_watch_handle,
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

fn create_processors_for_users(
    uuid: Uuid,
    mut l: JobScheduler,
    state: ServerState,
    map: ActiveEmailProcessorMap,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    let state = state.clone();
    let map = map.clone();
    tracing::info!("Job: {}\n Creating processors for active users...", uuid);
    Box::pin(async move {
        match email::tasks::add_users_to_processing(state, map.clone()).await {
            Ok(_) => {
                tracing::info!("Processor Creation Job {} succeeded", uuid);
            }
            Err(e) => {
                tracing::error!("Job failed: {:?}", e);
            }
        }

        let next_tick = l.next_tick_for_job(uuid).await;
        if let Ok(Some(ts)) = next_tick {
            tracing::info!("Next time for processor creation job is {:?}", ts)
        }
    })
}

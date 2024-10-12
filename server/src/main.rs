#[macro_use]
mod macros;

mod api_quota;
mod email;
mod prompt;
mod rate_limiters;
mod request_tracing;
mod routes;
mod server_config;
mod structs;

use std::{
    env,
    net::SocketAddr,
    sync::{atomic::AtomicU64, Arc},
    time::Duration,
};

// use arl::RateLimiter;
use axum::{extract::FromRef, http::StatusCode, response::IntoResponse, routing::get, Router};
use futures::future::join_all;
use leaky_bucket::RateLimiter;
use mimalloc::MiMalloc;
use rate_limiters::RateLimiters;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use server_config::cfg;
use std::sync::atomic::Ordering::Relaxed;
use tokio::{signal, task::JoinHandle};
use tokio_cron_scheduler::{Job, JobScheduler};
use tower_cookies::CookieManagerLayer;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub type TokenCounter = Arc<AtomicU64>;
pub type HttpClient = reqwest::Client;

#[derive(Clone, FromRef)]
struct ServerState {
    http_client: HttpClient,
    conn: DatabaseConnection,
    token_count: TokenCounter,
    rate_limiters: RateLimiters,
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

    let state = ServerState {
        http_client: reqwest::Client::new(),
        conn,
        token_count: Arc::new(AtomicU64::new(0)),
        rate_limiters: RateLimiters::from_env(),
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_env("RUST_LOG"))
        .with(tracing_subscriber::fmt::Layer::default().with_ansi(false))
        .init();

    let router = Router::new()
        .route("/", get(|| async { "Auto email server" }))
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

    let mut scheduler = JobScheduler::new()
        .await
        .expect("Failed to create scheduler");

    {
        let state_clone = state.clone();
        // This is a one shot just for testing
        scheduler
            .add(Job::new_one_shot_async(
                Duration::from_secs(0),
                move |uuid, mut l: JobScheduler| {
                    let state = state_clone.clone();
                    Box::pin(async move {
                        match email::tasks::process_emails(state).await {
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
                                println!("Next time for email processor job is {:?}", ts)
                            }
                            _ => println!("Could not get next tick for email processor job"),
                        }
                    })
                },
            )?)
            .await?;

        let state_clone = state.clone();
        scheduler
            .add(Job::new_one_shot_async(
                Duration::from_secs(120),
                move |uuid, mut _l: JobScheduler| {
                    let conn = state_clone.conn.clone();
                    let http_client = state_clone.http_client.clone();
                    Box::pin(async move {
                        match email::tasks::send_daily_email_summaries(conn.clone(), http_client)
                            .await
                        {
                            Ok(_) => {
                                tracing::info!("Daily summary mailer job {} succeeded", uuid);
                            }
                            Err(e) => {
                                tracing::error!("Job failed: {:?}", e);
                            }
                        }
                    })
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

    scheduler.start().await.expect("Failed to start scheduler");

    // Handle Ctrl+C
    let shutdown_handle = {
        tokio::spawn(async move {
            signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
            tracing::info!("Received Ctrl+C, shutting down");
            scheduler.shutdown().await.unwrap();
            std::process::exit(0);
        })
    };

    join_all(vec![run_server(router), shutdown_handle]).await;

    Ok(())
}

fn run_server(router: Router) -> JoinHandle<()> {
    tokio::spawn(async {
        // Start the server
        let port = env::var("PORT").unwrap_or("5006".to_string());
        tracing::info!("Auto email running on http://0.0.0.0:{}", port);
        // check config
        tracing::info!("Config: {}", *server_config::cfg);

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

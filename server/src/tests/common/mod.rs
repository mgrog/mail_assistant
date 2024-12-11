use reqwest::Certificate;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::{env, path::PathBuf};

use crate::{server_config::get_cert, HttpClient};

pub async fn setup() -> (DatabaseConnection, HttpClient) {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
    let app_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .to_str()
        .expect("Failed to convert path to string")
        .to_string();

    env::set_var("APP_DIR", app_dir);
    let mut db_options = ConnectOptions::new(db_url);
    db_options.sqlx_logging(false);

    let conn = Database::connect(db_options)
        .await
        .expect("Database connection failed");

    let cert = get_cert();
    let http_client = reqwest::ClientBuilder::new()
        .use_rustls_tls()
        .add_root_certificate(Certificate::from_pem(&cert).unwrap())
        .build()
        .unwrap();
    (conn, http_client)
}

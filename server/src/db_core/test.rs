use std::env;

use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub async fn setup_conn() -> DatabaseConnection {
    dotenvy::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL is not set in .env file");
    let mut db_options = ConnectOptions::new(db_url);
    db_options.sqlx_logging(false);

    Database::connect(db_options)
        .await
        .expect("Database connection failed")
}

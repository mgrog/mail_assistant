use std::{env, path::PathBuf};

use reqwest::Certificate;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

use crate::routes::account_connection::*;
use crate::server_config::get_cert;
use common::setup;

mod common;

#[tokio::test]
async fn test_check_account_connection_ok() {
    // This test requires a valid email account mpgrospamacc@gmail.com to be present in the database
    const EMAIL: &str = "mpgrospamacc@gmail.com";
    let (conn, http_client) = setup().await;
    let query = AccountConnectionQuery {
        email: EMAIL.to_string(),
    };

    let check = check_account_connection(
        State(http_client.clone()),
        State(conn.clone()),
        Query(query),
    )
    .await
    .unwrap();

    assert_eq!(check.email, EMAIL);
    assert!(matches!(check.result, GmailAccountConnectionStatus::Good));
}

#[tokio::test]
async fn test_check_account_connection_missing_scopes() {
    // This test requires a valid email account
    const EMAIL: &str = "mtest4966@gmail.com";
    let (conn, http_client) = setup().await;
    let query = AccountConnectionQuery {
        email: EMAIL.to_string(),
    };

    let check = check_account_connection(
        State(http_client.clone()),
        State(conn.clone()),
        Query(query),
    )
    .await
    .unwrap();

    assert_eq!(check.email, EMAIL);
    assert!(matches!(
        check.result,
        GmailAccountConnectionStatus::MissingScopes { .. }
    ));
}

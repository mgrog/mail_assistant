use sea_orm_migration::{prelude::*, schema::*};

use crate::m20220101_000001_create_table::UserSession;

#[derive(DeriveMigrationName)]
pub struct Migration;

const IDX_USER_SESSION_INBOX_SETTINGS: &str = "idx_inbox_settings_user_session_id";
const IDX_USER_SESSION_INBOX_SETTINGS_CATEGORY: &str = "idx_user_session_inbox_settings_category";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(InboxSettings::Table)
                    .if_not_exists()
                    .col(pk_auto(InboxSettings::Id))
                    .col(integer(InboxSettings::UserSessionId).not_null())
                    .col(string(InboxSettings::Category).not_null())
                    .col(boolean(InboxSettings::SkipInbox).not_null())
                    .col(boolean(InboxSettings::MarkSpam).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_inbox_settings_user_session")
                            .from(InboxSettings::Table, InboxSettings::UserSessionId)
                            .to(UserSession::Table, UserSession::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(IDX_USER_SESSION_INBOX_SETTINGS)
                    .table(InboxSettings::Table)
                    .col(InboxSettings::UserSessionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(IDX_USER_SESSION_INBOX_SETTINGS_CATEGORY)
                    .table(InboxSettings::Table)
                    .col(InboxSettings::Category)
                    .col(InboxSettings::UserSessionId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        manager
            .drop_index(
                Index::drop()
                    .name(IDX_USER_SESSION_INBOX_SETTINGS)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name(IDX_USER_SESSION_INBOX_SETTINGS_CATEGORY)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .table(InboxSettings::Table)
                    .cascade()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum InboxSettings {
    Table,
    Id,
    UserSessionId,
    Category,
    SkipInbox,
    MarkSpam,
}

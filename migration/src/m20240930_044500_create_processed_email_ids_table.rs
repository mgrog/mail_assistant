use sea_orm_migration::{prelude::*, schema::*};

use crate::m20220101_000001_create_table::UserSession;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ProcessedEmail::Table)
                    .if_not_exists()
                    .col(string(ProcessedEmail::Id).primary_key())
                    .col(integer(ProcessedEmail::UserSessionId).not_null())
                    .col(
                        timestamp_with_time_zone(ProcessedEmail::ProcessedAt)
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(array(ProcessedEmail::LabelsApplied, ColumnType::Text).not_null())
                    .col(array(ProcessedEmail::LabelsRemoved, ColumnType::Text).not_null())
                    .col(string(ProcessedEmail::AiAnswer).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-processed_email-user_session_id")
                            .from(ProcessedEmail::Table, ProcessedEmail::UserSessionId)
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
                    .name("idx-processed_email-user_session_id")
                    .table(ProcessedEmail::Table)
                    .col(ProcessedEmail::UserSessionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-processed_email-user_session_id")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(ProcessedEmail::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum ProcessedEmail {
    Table,
    Id,
    UserSessionId,
    AiAnswer,
    LabelsApplied,
    LabelsRemoved,
    ProcessedAt,
}

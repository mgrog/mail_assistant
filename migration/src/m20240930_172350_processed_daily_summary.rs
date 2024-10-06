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
                    .table(ProcessedDailySummary::Table)
                    .if_not_exists()
                    .col(pk_auto(ProcessedDailySummary::Id))
                    .col(integer(ProcessedDailySummary::UserSessionId).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-processed_daily_summary-user_session_id")
                            .from(
                                ProcessedDailySummary::Table,
                                ProcessedDailySummary::UserSessionId,
                            )
                            .to(UserSession::Table, UserSession::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(
                        timestamp_with_time_zone(ProcessedDailySummary::CreatedAt)
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx-processed_daily_summary-user_session_id")
                    .table(ProcessedDailySummary::Table)
                    .col(ProcessedDailySummary::UserSessionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx-processed_daily_summary-user_session_id")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(ProcessedDailySummary::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum ProcessedDailySummary {
    Table,
    Id,
    UserSessionId,
    CreatedAt,
}

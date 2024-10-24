use sea_orm_migration::prelude::*;

use crate::m20220101_000001_create_table::UserSession;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .add_column_if_not_exists(
                        ColumnDef::new(Alias::new("last_sync")).timestamp_with_time_zone(),
                    )
                    .table(UserSession::Table)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UserSession::Table)
                    .drop_column(Alias::new("last_sync"))
                    .to_owned(),
            )
            .await
    }
}

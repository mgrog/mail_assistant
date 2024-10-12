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
                    .table(UserSession::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("active"))
                            .boolean()
                            .default(false)
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UserSession::Table)
                    .drop_column(Alias::new("active"))
                    .to_owned(),
            )
            .await
    }
}

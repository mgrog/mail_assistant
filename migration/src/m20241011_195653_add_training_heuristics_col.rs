use sea_orm_migration::prelude::*;

use crate::m20241009_192424_add_training_table::EmailTraining;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(EmailTraining::Table)
                    .add_column(
                        ColumnDef::new(Alias::new("heuristics_used"))
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(EmailTraining::Table)
                    .drop_column(Alias::new("heuristics_used"))
                    .to_owned(),
            )
            .await
    }
}

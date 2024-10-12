use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

const IDX_EMAIL_ID: &str = "idx_email_id";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EmailTraining::Table)
                    .if_not_exists()
                    .col(pk_auto(EmailTraining::Id))
                    .col(string(EmailTraining::UserEmail).not_null())
                    .col(string(EmailTraining::EmailId).not_null())
                    .col(string(EmailTraining::From).not_null())
                    .col(string(EmailTraining::Subject).not_null())
                    .col(text(EmailTraining::Body).not_null())
                    .col(string(EmailTraining::AiAnswer).not_null())
                    .col(float(EmailTraining::Confidence).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .table(EmailTraining::Table)
                    .name(IDX_EMAIL_ID)
                    .col(EmailTraining::EmailId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(Index::drop().name(IDX_EMAIL_ID).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(EmailTraining::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum EmailTraining {
    Table,
    Id,
    UserEmail,
    EmailId,
    From,
    Subject,
    Body,
    AiAnswer,
    Confidence,
}

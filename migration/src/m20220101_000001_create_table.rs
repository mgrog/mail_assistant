use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserSession::Table)
                    .if_not_exists()
                    .col(pk_auto(UserSession::Id))
                    .col(string(UserSession::Email))
                    .col(string(UserSession::AccessToken))
                    .col(string(UserSession::RefreshToken))
                    .col(timestamp_with_time_zone(UserSession::ExpiresAt))
                    .col(
                        timestamp_with_time_zone(UserSession::CreatedAt)
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        timestamp_with_time_zone(UserSession::UpdatedAt)
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
                    .name("idx-user_session_email")
                    .unique()
                    .table(UserSession::Table)
                    .col(UserSession::Email)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserSession::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserSession {
    Table,
    Id,
    Email,
    AccessToken,
    RefreshToken,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
}

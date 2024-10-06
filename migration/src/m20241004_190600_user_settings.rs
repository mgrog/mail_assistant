use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

const IDX_USER_SETTINGS_USER_SESSION_ID: &str = "idx-user_settings-user_session_id";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserSettings::Table)
                    .if_not_exists()
                    .col(pk_auto(UserSettings::Id))
                    .col(integer(UserSettings::UserSessionId).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserSettings::Table, UserSettings::UserSessionId)
                            .to(UserSettings::Table, UserSettings::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(
                        boolean(UserSettings::DailySummaryEnabled)
                            .not_null()
                            .default(true),
                    )
                    .col(
                        string(UserSettings::DailySummaryTime)
                            .not_null()
                            .default("06:00".to_owned()),
                    )
                    .col(
                        string(UserSettings::UserTimeZoneOffset)
                            .not_null()
                            .default("-08".to_owned()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(IDX_USER_SETTINGS_USER_SESSION_ID)
                    .table(UserSettings::Table)
                    .col(UserSettings::UserSessionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(IDX_USER_SETTINGS_USER_SESSION_ID)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(UserSettings::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum UserSettings {
    Table,
    Id,
    UserSessionId,
    DailySummaryTime,
    UserTimeZoneOffset,
    DailySummaryEnabled,
}

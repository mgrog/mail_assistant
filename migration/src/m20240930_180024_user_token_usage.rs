use sea_orm_migration::{prelude::*, schema::*};

use crate::m20220101_000001_create_table::UserSession;

#[derive(DeriveMigrationName)]
pub struct Migration;

const IDX_TOKEN_USAGE_STATS_USER_SESSION_ID: &str = "idx-user_token_usage_stats-user_session_id";
const IDX_TOKEN_USAGE_STATS_DATE: &str = "idx-user_token_usage_stats-date";
const IDX_TOKEN_USAGE_STATS_DATE_USER_SESSION_ID: &str =
    "idx-user_token_usage_stats-date-user_session_id";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // manager
        //     .create_type(
        //         Type::create()
        //             .as_enum(Month::Enum)
        //             .values([
        //                 Month::January,
        //                 Month::February,
        //                 Month::March,
        //                 Month::April,
        //                 Month::May,
        //                 Month::June,
        //                 Month::July,
        //                 Month::August,
        //                 Month::September,
        //                 Month::October,
        //                 Month::November,
        //                 Month::December,
        //             ])
        //             .to_owned(),
        //     )
        //     .await?;

        manager
            .create_table(
                Table::create()
                    .table(UserTokenUsageStats::Table)
                    .if_not_exists()
                    .col(pk_auto(UserTokenUsageStats::Id))
                    .col(
                        date(UserTokenUsageStats::Date)
                            .default(Expr::current_date())
                            .not_null(),
                    )
                    .col(
                        big_integer(UserTokenUsageStats::TokensConsumed)
                            .default(0)
                            .not_null(),
                    )
                    .col(
                        timestamp_with_time_zone(UserTokenUsageStats::CreatedAt)
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(
                        timestamp_with_time_zone(UserTokenUsageStats::UpdatedAt)
                            .default(Expr::current_timestamp())
                            .not_null(),
                    )
                    .col(integer(UserTokenUsageStats::UserSessionId).not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-user_token_usage_stats-user_session_id")
                            .from(
                                UserTokenUsageStats::Table,
                                UserTokenUsageStats::UserSessionId,
                            )
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
                    .name(IDX_TOKEN_USAGE_STATS_USER_SESSION_ID)
                    .table(UserTokenUsageStats::Table)
                    .col(UserTokenUsageStats::UserSessionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(IDX_TOKEN_USAGE_STATS_DATE)
                    .table(UserTokenUsageStats::Table)
                    .col(UserTokenUsageStats::Date)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name(IDX_TOKEN_USAGE_STATS_DATE_USER_SESSION_ID)
                    .table(UserTokenUsageStats::Table)
                    .col(UserTokenUsageStats::Date)
                    .col(UserTokenUsageStats::UserSessionId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name(IDX_TOKEN_USAGE_STATS_USER_SESSION_ID)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(Index::drop().name(IDX_TOKEN_USAGE_STATS_DATE).to_owned())
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name(IDX_TOKEN_USAGE_STATS_DATE_USER_SESSION_ID)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(UserTokenUsageStats::Table).to_owned())
            .await?;

        Ok(())
    }
}

// #[derive(DeriveIden)]
// #[sea_orm(enum_name = "month")]
// pub enum Month {
//     #[sea_orm(iden = "month")]
//     Enum,
//     #[sea_orm(iden = "january")]
//     January,
//     #[sea_orm(iden = "february")]
//     February,
//     #[sea_orm(iden = "march")]
//     March,
//     #[sea_orm(iden = "april")]
//     April,
//     #[sea_orm(iden = "may")]
//     May,
//     #[sea_orm(iden = "june")]
//     June,
//     #[sea_orm(iden = "july")]
//     July,
//     #[sea_orm(iden = "august")]
//     August,
//     #[sea_orm(iden = "september")]
//     September,
//     #[sea_orm(iden = "october")]
//     October,
//     #[sea_orm(iden = "november")]
//     November,
//     #[sea_orm(iden = "december")]
//     December,
// }

#[derive(DeriveIden)]
enum UserTokenUsageStats {
    Table,
    Id,
    UserSessionId,
    Date,
    TokensConsumed,
    CreatedAt,
    UpdatedAt,
}

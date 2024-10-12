pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20240930_044500_create_processed_email_ids_table;
mod m20240930_172350_processed_daily_summary;
mod m20240930_180024_user_token_usage;
mod m20241004_190600_user_settings;
mod m20241009_192424_add_training_table;
mod m20241011_180942_add_active_user_session_col;
mod m20241011_195653_add_training_heuristics_col;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20240930_044500_create_processed_email_ids_table::Migration),
            Box::new(m20240930_172350_processed_daily_summary::Migration),
            Box::new(m20240930_180024_user_token_usage::Migration),
            Box::new(m20241004_190600_user_settings::Migration),
            Box::new(m20241009_192424_add_training_table::Migration),
            Box::new(m20241011_180942_add_active_user_session_col::Migration),
            Box::new(m20241011_195653_add_training_heuristics_col::Migration),
        ]
    }
}

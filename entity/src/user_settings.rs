//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.0-rc.5

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "user_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub daily_summary_enabled: bool,
    pub daily_summary_time: String,
    pub user_time_zone_offset: String,
    #[sea_orm(unique)]
    pub user_email: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserEmail",
        to = "super::user::Column::Email",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

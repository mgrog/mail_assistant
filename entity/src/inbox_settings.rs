//! `SeaORM` Entity, @generated by sea-orm-codegen 1.0.0-rc.5

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "inbox_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_session_id: i32,
    pub category: String,
    pub skip_inbox: bool,
    pub mark_spam: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user_session::Entity",
        from = "Column::UserSessionId",
        to = "super::user_session::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    UserSession,
}

impl Related<super::user_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

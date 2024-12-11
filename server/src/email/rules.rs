use std::collections::HashMap;

use crate::db_core::prelude::*;
use anyhow::Context;
use futures::join;
use lazy_static::lazy_static;
use sea_orm::EntityTrait;

lazy_static! {
    static ref DEFAULT_EMAIL_RULES: Vec<EmailRule> = {
        use crate::server_config::cfg;
        let categories = &cfg.categories;
        categories
            .iter()
            .map(|c| EmailRule {
                prompt_content: c.content.clone(),
                mail_label: c.mail_label.clone(),
                associated_email_client_category: c.gmail_categories.first().map(|s| {
                    AssociatedEmailClientCategory::try_from_value(s)
                        .unwrap_or_else(|_| panic!("Invalid email client category: {s}"))
                }),
            })
            .collect()
    };
}

#[derive(Debug, Clone)]
pub struct EmailRule {
    pub prompt_content: String,
    pub mail_label: String,
    pub associated_email_client_category: Option<AssociatedEmailClientCategory>,
}

pub struct UserEmailRules {
    pub category_rules: Vec<EmailRule>,
}

impl UserEmailRules {
    pub async fn from_user(conn: &DatabaseConnection, user_id: i32) -> anyhow::Result<Self> {
        // let user_defined = None;
        let (default_rule_overrides, custom_email_rules) = join!(
            DefaultEmailRuleOverride::find()
                .filter(default_email_rule_override::Column::UserId.eq(user_id))
                .all(conn),
            CustomEmailRule::find()
                .filter(custom_email_rule::Column::UserId.eq(user_id))
                .all(conn)
        );
        let default_rule_overrides =
            default_rule_overrides.context("Failed to fetch default overrides")?;
        let custom_email_rules = custom_email_rules.context("Failed to fetch custom rules")?;
        let category_rules =
            Self::build_category_rules(default_rule_overrides.clone(), custom_email_rules.clone());

        Ok(Self { category_rules })
    }

    fn build_category_rules(
        default_rule_overrides: Vec<default_email_rule_override::Model>,
        custom_rules: Vec<custom_email_rule::Model>,
    ) -> Vec<EmailRule> {
        let mut default_rules = DEFAULT_EMAIL_RULES
            .iter()
            .map(|rule| (rule.mail_label.clone(), rule.clone()))
            .collect::<HashMap<_, _>>();

        for ro in default_rule_overrides {
            if ro.is_disabled {
                default_rules.remove(&ro.category);
                continue;
            }
            default_rules.entry(ro.category).and_modify(|rule| {
                rule.associated_email_client_category = ro.associated_email_client_category
            });
        }

        let custom_rules = custom_rules.into_iter().map(|rule| EmailRule {
            prompt_content: rule.prompt_content,
            mail_label: rule.category,
            associated_email_client_category: rule.associated_email_client_category,
        });

        custom_rules
            .chain(default_rules.values().cloned())
            .collect()
    }
}

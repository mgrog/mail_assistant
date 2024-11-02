use std::collections::HashMap;

#[derive(Debug, Copy, Clone)]
pub struct CategoryInboxSetting {
    pub skip_inbox: bool,
    pub mark_spam: bool,
}

pub type CategoryInboxSettingsMap = HashMap<String, CategoryInboxSetting>;

pub fn default_inbox_settings() -> CategoryInboxSettingsMap {
    HashMap::from([
        (
            "ads".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "political".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "notices".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "receipts".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "security_alerts".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "flights".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "finances".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
        (
            "social_media".to_string(),
            CategoryInboxSetting {
                skip_inbox: false,
                mark_spam: false,
            },
        ),
    ])
}

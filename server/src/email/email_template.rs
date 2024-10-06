pub const DAILY_SUMMARY_EMAIL_TEMPLATE: &str = r#"
  Hello, {{user_email}}! Here is your daily summary.

  <ul>
    {% for (category, count) in category_counts if not count == 0 %}
      <li>{{count}} emails went to {{category}}\\n</li>
    {% endfor %}
  </ul>
"#;

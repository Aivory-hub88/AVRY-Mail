use serde_json::Value;

/// One enabled filter row: (criteria_json, action_json).
pub struct FilterRule<'a> {
    pub criteria: &'a Value,
    pub action: &'a Value,
}

fn matches(criteria: &Value, from: &str, subject: &str, body: &str) -> bool {
    let Some(obj) = criteria.as_object() else { return false };
    if obj.is_empty() { return false; }
    obj.iter().all(|(key, needle)| {
        let Some(needle) = needle.as_str() else { return true };
        if needle.is_empty() { return true; }
        let haystack = match key.as_str() {
            "from" => from,
            "subject" => subject,
            "body" => body,
            _ => return true, // unknown criteria key: don't block the match on it
        };
        haystack.to_lowercase().contains(&needle.to_lowercase())
    })
}

/// Run enabled filters in order, return the target folder of the first match
/// (Gmail/Zoho semantics: first matching rule wins), or None to fall back to Inbox.
pub fn resolve_folder(rules: &[FilterRule], from: &str, subject: &str, body: &str) -> Option<String> {
    for rule in rules {
        if matches(rule.criteria, from, subject, body) {
            if let Some(folder) = rule.action.get("move").and_then(|v| v.as_str()) {
                return Some(folder.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_from_substring() {
        let criteria = json!({"from": "finance@"});
        let action = json!({"move": "Spam"});
        let rules = vec![FilterRule { criteria: &criteria, action: &action }];
        assert_eq!(resolve_folder(&rules, "finance@acme.com", "Invoice", "body"), Some("Spam".into()));
        assert_eq!(resolve_folder(&rules, "sales@acme.com", "Invoice", "body"), None);
    }

    #[test]
    fn first_match_wins() {
        let c1 = json!({"subject": "urgent"});
        let a1 = json!({"move": "Inbox"});
        let c2 = json!({"subject": "urgent"});
        let a2 = json!({"move": "Trash"});
        let rules = vec![
            FilterRule { criteria: &c1, action: &a1 },
            FilterRule { criteria: &c2, action: &a2 },
        ];
        assert_eq!(resolve_folder(&rules, "x@y.com", "Urgent: reply now", "b"), Some("Inbox".into()));
    }
}

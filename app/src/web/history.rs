use chrono::{DateTime, Utc};
use strum::{Display, EnumString};

use crate::models::{notification::Notification, record_change::LoggedChange};

use super::registry::RegistryField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum ChangeEvent {
    Add,
    Delete,
    Change,
    /// A registrar dropped a name that `[manual]` still tracks.
    Handover,
    Registry,
}

impl ChangeEvent {
    pub fn title(self) -> &'static str {
        match self {
            Self::Add => "Domain added",
            Self::Delete => "Domain removed",
            Self::Change => "Record changed",
            Self::Handover => "Moved to manual list",
            Self::Registry => "Registry changed",
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Self::Add => "Added",
            Self::Delete => "Removed",
            Self::Change | Self::Registry => "Changed",
            Self::Handover => "Moved to manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeLine {
    Field {
        field: String,
        before: String,
        after: String,
    },
    Note(String),
}

/// One line of a `diff_to_human` message: `- Auto Renew Changed: a => b` or `- key: (Value, Value)` in `Debug` form.
impl From<&str> for ChangeLine {
    fn from(raw: &str) -> Self {
        let line = raw.trim();
        let line = line.strip_prefix("- ").unwrap_or(line);
        let Some((field, rest)) = line.split_once(": ") else {
            return Self::Note(line.to_string());
        };
        let pair = rest
            .split_once(" => ")
            .map(|(before, after)| (before.to_string(), after.to_string()))
            .or_else(|| debug_value_pair(rest));
        match pair {
            Some((before, after)) => Self::Field {
                field: field.strip_suffix(" Changed").unwrap_or(field).to_string(),
                before,
                after,
            },
            None => Self::Note(line.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Change {
    pub event: ChangeEvent,
    pub at: DateTime<Utc>,
    pub lines: Vec<ChangeLine>,
}

impl From<&Notification> for Change {
    fn from(notification: &Notification) -> Self {
        let event = notification.event.parse().unwrap_or_else(|_| {
            panic!(
                "notification {} has event {:?}; only add, delete, change or handover are written",
                notification.id, notification.event
            )
        });
        Self {
            event,
            at: notification.created_at.and_utc(),
            lines: notification
                .message
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(ChangeLine::from)
                .collect(),
        }
    }
}

impl Change {
    pub fn registry(fields: &[LoggedChange]) -> Self {
        let at = fields
            .first()
            .expect("a snapshot only gets change rows when at least one field changed")
            .observed_at;
        Self {
            event: ChangeEvent::Registry,
            at,
            lines: fields.iter().map(ChangeLine::from).collect(),
        }
    }
}

/// A field this build does not know, from an older or newer schema, keeps its stored key and JSON values.
impl From<&LoggedChange> for ChangeLine {
    fn from(change: &LoggedChange) -> Self {
        match change.field.parse::<RegistryField>() {
            Ok(field) => Self::Field {
                field: field.label().to_string(),
                before: field.format(&change.before),
                after: field.format(&change.after),
            },
            Err(_) => Self::Field {
                field: change.field.clone(),
                before: change.before.to_string(),
                after: change.after.to_string(),
            },
        }
    }
}

/// Splits `(String("a"), Bool(true))`, the `Debug` form of a `(&Value, &Value)` tuple.
fn debug_value_pair(raw: &str) -> Option<(String, String)> {
    let inner = raw.strip_prefix('(')?.strip_suffix(')')?;
    match split_top_level(inner).as_slice() {
        [before, after] => Some((debug_value_text(before), debug_value_text(after))),
        _ => None,
    }
}

/// Splits at commas that sit outside quotes and brackets.
fn split_top_level(raw: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in raw.char_indices() {
        if in_string {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(raw[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    let last = raw[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// Renders one `serde_json::Value` `Debug` string as plain text; objects stay in `Debug` form.
fn debug_value_text(raw: &str) -> String {
    let raw = raw.trim();
    if raw == "Null" {
        return "none".to_string();
    }
    if let Some(items) = raw
        .strip_prefix("Array [")
        .and_then(|s| s.strip_suffix(']'))
    {
        return split_top_level(items)
            .into_iter()
            .map(debug_value_text)
            .collect::<Vec<_>>()
            .join(", ");
    }
    if let Some(quoted) = raw
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return quoted.replace("\\\"", "\"").replace("\\\\", "\\");
    }
    ["Bool(", "Number("]
        .iter()
        .find_map(|prefix| raw.strip_prefix(prefix).and_then(|s| s.strip_suffix(')')))
        .unwrap_or(raw)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::ChangeLine;

    fn field(field: &str, before: &str, after: &str) -> ChangeLine {
        ChangeLine::Field {
            field: field.to_string(),
            before: before.to_string(),
            after: after.to_string(),
        }
    }

    #[test]
    fn parses_diff_to_human_lines() {
        assert_eq!(
            ChangeLine::from(" - Auto Renew Changed: true => false"),
            field("Auto Renew", "true", "false")
        );
        assert_eq!(
            ChangeLine::from(
                r#" - ext_expiry_at: (String("2026-04-16T23:59:59Z"), String("2027-04-16T23:59:59Z"))"#
            ),
            field(
                "ext_expiry_at",
                "2026-04-16T23:59:59Z",
                "2027-04-16T23:59:59Z"
            )
        );
        assert_eq!(
            ChangeLine::from(r#" - registry_statuses: (String("a, \"b\""), Null)"#),
            field("registry_statuses", r#"a, "b""#, "none")
        );
        assert_eq!(
            ChangeLine::from(
                r#" - name_servers: (Array [String("a.ns"), String("b.ns")], Array [])"#
            ),
            field("name_servers", "a.ns, b.ns", "")
        );
        assert_eq!(
            ChangeLine::from("New domain detected"),
            ChangeLine::Note("New domain detected".to_string())
        );
    }
}

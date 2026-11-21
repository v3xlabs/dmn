use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, SubsecRound, Utc};
use serde::{Deserialize, Serialize};

/// Date-time layouts that port-43 servers use besides RFC 3339, all read as UTC.
const DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S",
    "%d.%m.%Y %H:%M:%S",
    "%Y%m%d %H:%M:%S",
];

/// Date-only layouts that port-43 servers use, read as midnight UTC.
const DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%d-%b-%Y", "%Y.%m.%d", "%Y/%m/%d"];

/// Registry state of one domain, normalized so RDAP and port-43 answers compare equal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryRecord {
    pub registered: bool,
    pub expiry_at: Option<DateTime<Utc>>,
    pub registered_at: Option<DateTime<Utc>>,
    /// RDAP status form, lower case and sorted: `client transfer prohibited`. `None` when the answer has no status member or line.
    pub statuses: Option<Vec<String>>,
    /// Lower case, no trailing dot, sorted. `None` when the answer has no nameserver member or line.
    pub nameservers: Option<Vec<String>>,
    pub registrar: Option<String>,
    pub dnssec: Option<bool>,
}

impl RegistryRecord {
    pub fn unregistered() -> Self {
        Self::default()
    }

    pub fn normalized(mut self) -> Self {
        for list in [&mut self.statuses, &mut self.nameservers]
            .into_iter()
            .flatten()
        {
            list.sort();
            list.dedup();
        }
        self
    }
}

/// Maps an EPP status (`clientTransferProhibited https://icann.org/epp#...`) or an RDAP status to the RDAP form of RFC 8056.
pub fn status(raw: &str) -> String {
    let code = raw.split(" http").next().unwrap_or(raw).trim();
    let words = if code.starts_with(|c: char| c.is_ascii_lowercase()) {
        let mut words = String::with_capacity(code.len() + 4);
        for c in code.chars() {
            if c.is_ascii_uppercase() && !words.is_empty() && !words.ends_with(' ') {
                words.push(' ');
            }
            words.push(c.to_ascii_lowercase());
        }
        words
    } else {
        code.to_ascii_lowercase()
    };
    if words == "ok" {
        "active".to_string()
    } else {
        words
    }
}

/// First token of a nameserver line, so DENIC's `ns1.denic.de. 77.67.63.106` becomes `ns1.denic.de`.
pub fn nameserver(raw: &str) -> Option<String> {
    let host = raw
        .split_whitespace()
        .next()?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Parses registry timestamps to whole seconds in UTC, so `.435Z` and `Z` forms of one instant compare equal.
pub fn timestamp(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    let parsed = DateTime::parse_from_rfc3339(raw)
        .map(|at| at.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DATETIME_FORMATS
                .iter()
                .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
                .map(|at| at.and_utc())
        })
        .or_else(|| {
            DATE_FORMATS
                .iter()
                .find_map(|format| NaiveDate::parse_from_str(raw, format).ok())
                .map(|day| day.and_time(NaiveTime::MIN).and_utc())
        })?;
    Some(parsed.trunc_subsecs(0))
}

#[cfg(test)]
mod tests {
    use super::status;

    #[test]
    fn ok_is_active_in_every_case() {
        for raw in ["ok", "OK", "Ok", "ok https://icann.org/epp#ok"] {
            assert_eq!(status(raw), "active", "{raw}");
        }
        assert_eq!(
            status("clientTransferProhibited"),
            "client transfer prohibited"
        );
        assert_eq!(
            status("CLIENT TRANSFER PROHIBITED"),
            "client transfer prohibited"
        );
    }
}

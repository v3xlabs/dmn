use std::{fmt, str::FromStr};

use serde::Serialize;

/// Longest domain name in presentation form that RFC 1035 allows.
const MAX_NAME_LEN: usize = 253;

/// Longest single label that RFC 1035 allows.
const MAX_LABEL_LEN: usize = 63;

/// A lower-case ASCII domain name with at least two labels, as registries expect it; IDN labels are A-labels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct DomainName(String);

impl DomainName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn tld(&self) -> &str {
        self.0
            .rsplit('.')
            .next()
            .expect("rsplit yields at least one item")
    }
}

impl FromStr for DomainName {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        let raw = raw.strip_suffix('.').unwrap_or(raw);
        let name = if raw.is_ascii() {
            raw.to_ascii_lowercase()
        } else {
            to_a_labels(raw).ok_or("name is not a valid internationalized domain name")?
        };
        if name.len() > MAX_NAME_LEN {
            return Err("name is longer than 253 characters");
        }
        if !name.contains('.') {
            return Err("name needs at least two labels");
        }
        let valid = name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= MAX_LABEL_LEN
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        });
        if !valid {
            return Err("labels must be letters, digits or hyphens");
        }
        Ok(Self(name))
    }
}

/// Maps Unicode labels to A-labels (`bücher.de` is `xn--bcher-kva.de`), with the IDNA mapping that the `url` crate applies to hosts.
fn to_a_labels(raw: &str) -> Option<String> {
    // The URL parser ends the host at a separator, so only text that can be a whole host reaches it.
    let host_char = |c: char| !c.is_ascii() || c.is_ascii_alphanumeric() || c == '-' || c == '.';
    if !raw.chars().all(host_char) {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://{raw}/")).ok()?;
    url.domain().map(str::to_string)
}

impl fmt::Display for DomainName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::DomainName;

    fn parse(raw: &str) -> Option<String> {
        raw.parse::<DomainName>().ok().map(|name| name.to_string())
    }

    #[test]
    fn only_one_root_dot_is_stripped() {
        assert_eq!(parse(" Example.dev. ").as_deref(), Some("example.dev"));
        assert_eq!(parse(" example.dev.. "), None);
        assert_eq!(parse(" bücher.de.. "), None);
        assert_eq!(parse(".example.dev"), None);
        assert_eq!(parse("example..dev"), None);
    }

    #[test]
    fn spellings_of_one_name_parse_to_one_name() {
        assert_eq!(parse("Example.COM.").as_deref(), Some("example.com"));
        assert_eq!(parse("bücher.de").as_deref(), Some("xn--bcher-kva.de"));
        assert_eq!(
            parse("XN--BCHER-KVA.de").as_deref(),
            Some("xn--bcher-kva.de")
        );
        assert_eq!(parse("example.com/path"), None);
        assert_eq!(parse("example.com\\example.org"), None);
        assert_eq!(parse("bücher.de\\example.org"), None);
        assert_eq!(parse("bücher.de\u{FF0F}example.org"), None);
        assert_eq!(parse(""), None);
    }
}

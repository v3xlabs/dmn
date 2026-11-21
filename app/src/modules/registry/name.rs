use std::{fmt, str::FromStr};

use serde::Serialize;

/// Longest domain name in presentation form that RFC 1035 allows.
const MAX_NAME_LEN: usize = 253;

/// Longest single label that RFC 1035 allows.
const MAX_LABEL_LEN: usize = 63;

/// A lower-case ASCII domain name with at least two labels, as registries expect it.
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
        let name = raw.strip_suffix('.').unwrap_or(raw).to_ascii_lowercase();
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
            return Err("labels must be ASCII letters, digits or hyphens, use punycode for IDNs");
        }
        Ok(Self(name))
    }
}

impl fmt::Display for DomainName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::DomainName;

    #[test]
    fn only_one_root_dot_is_stripped() {
        let parse = |raw: &str| raw.parse::<DomainName>().map(|name| name.to_string());
        assert_eq!(parse(" Example.dev. "), Ok("example.dev".to_string()));
        assert!(parse(" example.dev.. ").is_err());
        assert!(parse(".example.dev").is_err());
        assert!(parse("example..dev").is_err());
    }
}

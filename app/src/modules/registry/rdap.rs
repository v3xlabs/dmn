use std::{fmt, time::Duration};

use anyhow::{anyhow, bail, Error};
use chrono::{DateTime, Utc};
use reqwest::{
    header::{ACCEPT, LOCATION, RETRY_AFTER},
    StatusCode,
};
use serde::Deserialize;
use serde_json::Value;

use super::{
    name::DomainName,
    record::{self, RegistryRecord},
};

/// An RDAP server answered 429 or 503. `retry_after` holds its `Retry-After` seconds when it sent them.
#[derive(Debug)]
pub struct Throttled {
    pub url: String,
    pub retry_after: Option<Duration>,
}

/// RDAP hosts that leave out `secureDNS` for an unsigned delegation, so a missing member there means unsigned; elsewhere it means unknown.
/// Checked on 2026-11-21: `rdap.denic.de` sends `secureDNS` with `keyData` for `denic.de` and none for `google.de`, which has no DS record.
const SECURE_DNS_ONLY_WHEN_SIGNED: &[&str] = &["rdap.denic.de"];

/// An RDAP server answered with a redirect. `to` is the absolute URL from its `Location` header.
#[derive(Debug)]
pub struct Redirected {
    pub from: String,
    pub to: String,
}

impl fmt::Display for Redirected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RDAP server {} redirected to {}", self.from, self.to)
    }
}

impl std::error::Error for Redirected {}

impl Throttled {
    /// `Retry-After` holds either delay seconds or an HTTP date (RFC 9110, section 10.2.3).
    pub fn new(url: String, retry_after: Option<&str>, now: DateTime<Utc>) -> Self {
        let retry_after = retry_after.map(str::trim).and_then(|value| {
            value.parse().map(Duration::from_secs).ok().or_else(|| {
                DateTime::parse_from_rfc2822(value)
                    .ok()
                    .map(|at| (at.with_timezone(&Utc) - now).to_std().unwrap_or_default())
            })
        });
        Self { url, retry_after }
    }
}

impl fmt::Display for Throttled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RDAP server throttled {}", self.url)
    }
}

impl std::error::Error for Throttled {}

#[derive(Deserialize)]
struct RdapDomain {
    #[serde(rename = "objectClassName")]
    object_class_name: Option<String>,
    #[serde(rename = "ldhName")]
    ldh_name: Option<String>,
    status: Option<Vec<String>>,
    #[serde(default)]
    events: Vec<RdapEvent>,
    nameservers: Option<Vec<RdapNameserver>>,
    #[serde(default)]
    entities: Vec<RdapEntity>,
    #[serde(rename = "secureDNS")]
    secure_dns: Option<RdapSecureDns>,
}

#[derive(Deserialize)]
struct RdapEvent {
    #[serde(rename = "eventAction")]
    action: String,
    #[serde(rename = "eventDate")]
    date: Option<String>,
}

#[derive(Deserialize)]
struct RdapNameserver {
    #[serde(rename = "ldhName")]
    ldh_name: Option<String>,
}

#[derive(Deserialize)]
struct RdapEntity {
    #[serde(default)]
    roles: Vec<String>,
    #[serde(rename = "vcardArray")]
    vcard: Option<Value>,
}

#[derive(Deserialize)]
struct RdapSecureDns {
    #[serde(rename = "delegationSigned")]
    delegation_signed: Option<bool>,
    #[serde(default, rename = "dsData")]
    ds_data: Vec<Value>,
    #[serde(default, rename = "keyData")]
    key_data: Vec<Value>,
}

impl RdapSecureDns {
    /// DENIC omits `delegationSigned` but lists `keyData` for a signed delegation.
    fn signed(&self) -> Option<bool> {
        self.delegation_signed
            .or_else(|| (!self.ds_data.is_empty() || !self.key_data.is_empty()).then_some(true))
    }
}

impl RdapEntity {
    /// The `fn` property of a jCard, RFC 7095: `["vcard", [["fn", {}, "text", "Name"], ...]]`.
    fn full_name(&self) -> Option<&str> {
        self.vcard
            .as_ref()?
            .get(1)?
            .as_array()?
            .iter()
            .find(|property| property.get(0).and_then(Value::as_str) == Some("fn"))?
            .get(3)?
            .as_str()
    }
}

impl RegistryRecord {
    /// `host` is the `Route::host` key of the server that answered.
    ///
    /// # Errors
    /// When the body is not an RDAP domain object for `name`, so only an answer about this name counts as a registration.
    pub fn from_rdap(host: &str, name: &DomainName, body: &str) -> Result<Self, Error> {
        let domain: RdapDomain = serde_json::from_str(body)?;
        if domain.object_class_name.as_deref() != Some("domain") {
            bail!(
                "RDAP answer from {host} is a {:?} object, not a domain",
                domain.object_class_name
            );
        }
        let answered = domain
            .ldh_name
            .as_deref()
            .map(|ldh| ldh.trim_end_matches('.'));
        if !answered.is_some_and(|ldh| ldh.eq_ignore_ascii_case(name.as_str())) {
            bail!("RDAP answer from {host} is about {answered:?}, not {name}");
        }
        let event = |action: &str| {
            domain
                .events
                .iter()
                .find(|event| event.action == action)
                .and_then(|event| event.date.as_deref())
                .and_then(record::timestamp)
        };
        Ok(Self {
            registered: true,
            expiry_at: event("expiration"),
            registered_at: event("registration"),
            statuses: domain
                .status
                .map(|statuses| statuses.iter().map(|s| record::status(s)).collect()),
            nameservers: domain.nameservers.map(|nameservers| {
                nameservers
                    .iter()
                    .filter_map(|ns| ns.ldh_name.as_deref().and_then(record::nameserver))
                    .collect()
            }),
            registrar: domain
                .entities
                .iter()
                .find(|entity| entity.roles.iter().any(|role| role == "registrar"))
                .and_then(RdapEntity::full_name)
                .map(str::to_string),
            dnssec: match domain.secure_dns {
                Some(dns) => dns.signed(),
                None if SECURE_DNS_ONLY_WHEN_SIGNED.contains(&host) => Some(false),
                None => None,
            },
        }
        .normalized())
    }
}

/// Sends one RDAP domain request to `url` on the server with `Route::host` key `host`; a 404 is an unregistered name.
///
/// # Errors
/// `Redirected` for a 3xx, because the client follows no redirect: the caller paces the next host.
/// `Throttled` for a 429 or 503.
pub async fn query(
    client: &reqwest::Client,
    url: &str,
    host: &str,
    name: &DomainName,
) -> Result<(RegistryRecord, String), Error> {
    let response = client
        .get(url)
        .header(ACCEPT, "application/rdap+json")
        .send()
        .await?;
    match response.status() {
        // Verisign closes a 404 without a TLS close_notify, so its empty body reads as an error.
        StatusCode::NOT_FOUND => Ok((
            RegistryRecord::unregistered(),
            response.text().await.unwrap_or_default(),
        )),
        status if status.is_success() => {
            let raw = response.text().await?;
            Ok((RegistryRecord::from_rdap(host, name, &raw)?, raw))
        }
        status if status.is_redirection() => {
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| anyhow!("RDAP server {url} answered {status} without Location"))?;
            let to = response.url().join(location)?.to_string();
            Err(Redirected {
                from: url.to_string(),
                to,
            }
            .into())
        }
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok());
            Err(Throttled::new(url.to_string(), retry_after, Utc::now()).into())
        }
        status => bail!("RDAP server {url} answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn retry_after_reads_seconds_and_http_dates() {
        let now = Utc.with_ymd_and_hms(2026, 11, 21, 7, 0, 0).unwrap();
        let throttled = |value| Throttled::new(String::new(), Some(value), now).retry_after;

        assert_eq!(throttled("120"), Some(Duration::from_secs(120)));
        assert_eq!(
            throttled("Sat, 21 Nov 2026 09:00:00 GMT"),
            Some(Duration::from_secs(2 * 60 * 60))
        );
        assert_eq!(
            throttled("Sat, 21 Nov 2026 06:00:00 GMT"),
            Some(Duration::ZERO)
        );
        assert_eq!(throttled("soon"), None);
    }

    /// An RDAP domain object for `example.com` with `fields` added.
    fn answer(host: &str, fields: &str) -> Result<RegistryRecord, Error> {
        let comma = if fields.is_empty() { "" } else { "," };
        let body = format!(
            r#"{{"objectClassName": "domain", "ldhName": "EXAMPLE.com."{comma} {fields}}}"#
        );
        RegistryRecord::from_rdap(host, &"example.com".parse().unwrap(), &body)
    }

    #[test]
    fn only_a_domain_object_for_the_name_is_a_record() {
        let name: DomainName = "example.com".parse().unwrap();
        assert!(answer("", "").unwrap().registered);
        for body in [
            r#"{"objectClassName": "entity", "ldhName": "example.com"}"#,
            r#"{"ldhName": "example.com"}"#,
            r#"{"objectClassName": "domain", "ldhName": "example.org"}"#,
            r#"{"objectClassName": "domain"}"#,
            r#"{"errorCode": 200, "title": "OK"}"#,
        ] {
            assert!(
                RegistryRecord::from_rdap("", &name, body).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn missing_list_members_are_none_and_empty_lists_are_empty() {
        let omitted = answer("", r#""events": []"#).unwrap();
        assert_eq!((omitted.statuses, omitted.nameservers), (None, None));

        let empty = answer("", r#""status": [], "nameservers": []"#).unwrap();
        assert_eq!(
            (empty.statuses, empty.nameservers),
            (Some(vec![]), Some(vec![]))
        );
    }

    #[test]
    fn missing_secure_dns_is_unsigned_only_for_denic() {
        let fields = r#""status": ["active"]"#;

        assert_eq!(answer("rdap.denic.de", fields).unwrap().dnssec, Some(false));
        assert_eq!(answer("rdap.example", fields).unwrap().dnssec, None);
    }

    #[test]
    fn key_data_without_delegation_signed_is_signed() {
        let record = answer(
            "",
            r#""secureDNS": {"zoneSigned": true, "keyData": [{"flags": 257, "protocol": 3, "algorithm": 8, "publicKey": "AwEAAa"}]}"#,
        )
        .unwrap();

        assert_eq!(record.dnssec, Some(true));
    }

    #[test]
    fn rdap_answer_normalizes_case_dots_and_fractions() {
        let record = answer(
            "",
            r#"
                "status": ["client transfer prohibited", "active"],
                "events": [
                    {"eventAction": "registration", "eventDate": "2018-10-29T15:57:39.435Z"},
                    {"eventAction": "expiration", "eventDate": "2027-10-29T15:57:39.435Z"},
                    {"eventAction": "last update of RDAP database", "eventDate": "2026-11-21T19:53:20Z"}
                ],
                "nameservers": [{"ldhName": "NS2.EXAMPLE.COM"}, {"ldhName": "ns1.example.com."}],
                "entities": [
                    {"roles": ["registrant"], "vcardArray": ["vcard", [["fn", {}, "text", "Owner"]]]},
                    {"roles": ["registrar"], "vcardArray": ["vcard", [["version", {}, "text", "4.0"], ["fn", {}, "text", "Example Registrar"]]]}
                ],
                "secureDNS": {"delegationSigned": false}
            "#,
        )
        .unwrap();

        assert_eq!(
            record,
            RegistryRecord {
                registered: true,
                expiry_at: Some(Utc.with_ymd_and_hms(2027, 10, 29, 15, 57, 39).unwrap()),
                registered_at: Some(Utc.with_ymd_and_hms(2018, 10, 29, 15, 57, 39).unwrap()),
                statuses: Some(vec!["active".into(), "client transfer prohibited".into()]),
                nameservers: Some(vec!["ns1.example.com".into(), "ns2.example.com".into()]),
                registrar: Some("Example Registrar".into()),
                dnssec: Some(false),
            }
        );
    }
}

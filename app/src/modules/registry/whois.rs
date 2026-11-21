use std::time::Duration;

use anyhow::{anyhow, Error};
use async_std::{
    io::{self, ReadExt, WriteExt},
    net::TcpStream,
};

use super::{
    name::DomainName,
    record::{self, RegistryRecord},
    Lookup, Route,
};

/// IANA's WHOIS server names the WHOIS host of each TLD on its `whois:` line.
pub const IANA_WHOIS_HOST: &str = "whois.iana.org";

const WHOIS_PORT: u16 = 43;

/// Caps one port-43 exchange so a silent server cannot hold a lookup open.
const WHOIS_TIMEOUT_SECS: u64 = 20;

/// Real answers are a few KB; the cap only stops a server that never closes the stream.
const WHOIS_MAX_BYTES: u64 = 256 * 1024;

/// WHOIS hosts that print a DNSSEC line only for a signed delegation, so a missing line means unsigned.
/// Checked on 2026-11-21: `nic.at` has `DNSSEC: Signed`, and `google.at` has no DNSSEC line.
const DNSSEC_ONLY_WHEN_SIGNED: &[&str] = &["whois.nic.at"];

/// Phrases that registries print instead of a record when the name is free, matched on lower-case lines with single spaces.
const NOT_FOUND_MARKERS: &[&str] = &[
    "no match",
    "not found",
    "nothing found",
    "no data found",
    "no entries found",
    "status: free",
    "status: available",
];

/// Sends one query line and reads the answer until the server closes the stream.
pub async fn query(host: &str, line: &str) -> Result<String, Error> {
    let exchange = async {
        let mut stream = TcpStream::connect((host, WHOIS_PORT)).await?;
        stream.write_all(format!("{line}\r\n").as_bytes()).await?;
        let mut answer = Vec::new();
        stream
            .take(WHOIS_MAX_BYTES)
            .read_to_end(&mut answer)
            .await?;
        Ok(answer)
    };
    let answer = io::timeout(Duration::from_secs(WHOIS_TIMEOUT_SECS), exchange).await?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// The host on the `whois:` line of an IANA answer, `None` when IANA lists no WHOIS server.
pub fn referral(iana_answer: &str) -> Option<String> {
    iana_answer.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let host = value.trim();
        (key.trim().eq_ignore_ascii_case("whois") && !host.is_empty())
            .then(|| host.to_ascii_lowercase())
    })
}

impl RegistryRecord {
    /// Reads `Key: value` lines, JPRS `[Key] value` lines and indented sections such as `Name servers:` (EURid) or `Nameservers` (.it) from `host`.
    pub fn from_whois(host: &str, text: &str) -> Option<Self> {
        let mut record = Self {
            registered: true,
            ..Self::default()
        };
        let mut known = false;
        let mut section: Option<String> = None;
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                section = None;
                continue;
            }
            if line.starts_with(['%', '#']) {
                continue;
            }
            if let (Some(section), true) = (&section, raw_line.starts_with([' ', '\t'])) {
                known |= record.read_section_line(section, line);
                continue;
            }
            let (key, value) = match line.strip_prefix('[') {
                Some(rest) => rest.split_once(']').unwrap_or((rest, "")),
                None => line.split_once(':').unwrap_or((line, "")),
            };
            let key = squeeze(key);
            let value = value.trim();
            if value.is_empty() {
                section = Some(key);
                continue;
            }
            section = None;
            known |= record.read_field(&key, value);
        }

        let free = record.nameservers.as_ref().is_none_or(Vec::is_empty)
            && record.expiry_at.is_none()
            && text.lines().any(|line| {
                let line = squeeze(line.trim_start_matches(['%', '#']));
                NOT_FOUND_MARKERS.iter().any(|marker| line.contains(marker))
            });
        if free {
            return Some(Self::unregistered());
        }
        if record.dnssec.is_none() && DNSSEC_ONLY_WHEN_SIGNED.contains(&host) {
            record.dnssec = Some(false);
        }
        known.then(|| record.normalized())
    }

    fn read_field(&mut self, key: &str, value: &str) -> bool {
        match key {
            "registry expiry date"
            | "expiry date"
            | "expiration date"
            | "expiration time"
            | "expire date"
            | "expires"
            | "expires on"
            | "paid-till"
            | "有効期限" => {
                self.expiry_at = self.expiry_at.or_else(|| record::timestamp(value));
            }
            "creation date" | "created" | "created on" | "registration date"
            | "registration time" | "registered" | "registered on" | "登録年月日" => {
                self.registered_at = self.registered_at.or_else(|| record::timestamp(value));
            }
            "domain status" | "status" | "state" | "状態" => {
                self.statuses
                    .get_or_insert_with(Vec::new)
                    .extend(value.split(',').map(record::status));
            }
            "name server" | "nameserver" | "nserver" => {
                self.nameservers
                    .get_or_insert_with(Vec::new)
                    .extend(record::nameserver(value));
            }
            "registrar" | "sponsoring registrar" => {
                self.registrar.get_or_insert_with(|| value.to_string());
            }
            "dnssec" | "signed" => {
                self.dnssec = match value.to_ascii_lowercase().as_str() {
                    "unsigned" | "no" => Some(false),
                    "signeddelegation" | "signed" | "yes" => Some(true),
                    _ => self.dnssec,
                };
            }
            _ => return false,
        }
        true
    }

    fn read_section_line(&mut self, section: &str, line: &str) -> bool {
        match section {
            "nameservers" | "name servers" => {
                self.nameservers
                    .get_or_insert_with(Vec::new)
                    .extend(record::nameserver(line));
                true
            }
            "registrar" => match line.split_once(':') {
                Some((key, value))
                    if matches!(squeeze(key).as_str(), "organization" | "name")
                        && !value.trim().is_empty() =>
                {
                    self.registrar
                        .get_or_insert_with(|| value.trim().to_string());
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }
}

/// Lower case with single spaces, so `Status:             AVAILABLE` reads as `status: available`.
fn squeeze(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl Lookup {
    pub async fn whois(host: &str, name: &DomainName) -> Result<Self, Error> {
        let raw = query(host, name.as_str()).await?;
        Ok(Self {
            name: name.clone(),
            route: Route::Whois(host.to_string()),
            record: RegistryRecord::from_whois(host, &raw)
                .ok_or_else(|| anyhow!("WHOIS answer from {host} has no field dmn can read"))?,
            raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn epp_answer_matches_rdap_form() {
        let record = RegistryRecord::from_whois(
            "",
            "Domain Name: nic.io\r
Registry Expiry Date: 2027-05-01T00:00:02Z\r
Creation Date: 2003-09-15T11:13:53Z\r
Registrar: Registry Operator acts as Registrar (9999)\r
Domain Status: serverDeleteProhibited https://icann.org/epp#serverDeleteProhibited\r
Domain Status: ok https://icann.org/epp#ok\r
Name Server: B0.NIC.IO\r
Name Server: a0.nic.io\r
DNSSEC: signedDelegation\r
>>> Last update of WHOIS database: 2026-11-21T19:53:41Z <<<\r
",
        );

        assert_eq!(
            record,
            Some(RegistryRecord {
                registered: true,
                expiry_at: Some(Utc.with_ymd_and_hms(2027, 5, 1, 0, 0, 2).unwrap()),
                registered_at: Some(Utc.with_ymd_and_hms(2003, 9, 15, 11, 13, 53).unwrap()),
                statuses: Some(vec!["active".into(), "server delete prohibited".into()]),
                nameservers: Some(vec!["a0.nic.io".into(), "b0.nic.io".into()]),
                registrar: Some("Registry Operator acts as Registrar (9999)".into()),
                dnssec: Some(true),
            })
        );
    }

    /// Free-name answers of whois.nic.it, whois.nic.at, whois.cnnic.cn, whois.jprs.jp, whois.eu and Verisign, as served on 2026-11-21.
    #[test]
    fn free_names_are_unregistered() {
        let answers = [
            "Domain:             free.it\nStatus:             AVAILABLE\n",
            "% Copyright (c)2026 by NIC.AT (1)\n%\n% Restricted rights.\n\n% nothing found\n",
            "No matching record.\n",
            "[ JPRS database provides information on network administration. ]\nNo match!!\n\n    ・登録されているJPドメイン名\n",
            "% The WHOIS service offered by EURid\n\nDomain: free.eu\nScript: LATIN\nStatus: AVAILABLE\n",
            "No match for \"FREE-NAME.COM\".\r\n",
        ];

        for answer in answers {
            assert_eq!(
                RegistryRecord::from_whois("", answer),
                Some(RegistryRecord::unregistered()),
                "{answer}"
            );
        }
    }

    #[test]
    fn section_and_bracket_formats_are_read() {
        let eurid = RegistryRecord::from_whois(
            "",
            "Domain: eurid.eu\n\nRegistrar:\n        Name: EURid vzw\n        Website: https://www.eurid.eu\n\nName servers:\n        ns3.eurid.eu (185.36.4.253)\n        nsp.netnod.se\n\n",
        )
        .unwrap();
        assert_eq!(eurid.registrar.as_deref(), Some("EURid vzw"));
        assert_eq!(
            eurid.nameservers.unwrap(),
            vec!["ns3.eurid.eu", "nsp.netnod.se"]
        );

        let jprs = RegistryRecord::from_whois(
            "",
            "[Domain Name]                   JPRS.JP\n[Name Server]                   ns1.jprs.jp\n[登録年月日]                    2001/02/02\n[有効期限]                      2027/02/28\n[状態]                          Active\n",
        )
        .unwrap();
        assert_eq!(
            jprs.expiry_at,
            Some(Utc.with_ymd_and_hms(2027, 2, 28, 0, 0, 0).unwrap())
        );
        assert_eq!(jprs.nameservers.unwrap(), vec!["ns1.jprs.jp"]);
        assert_eq!(jprs.statuses.unwrap(), vec!["active"]);

        let tcinet = RegistryRecord::from_whois(
            "",
            "domain:        YANDEX.RU\nnserver:       ns1.yandex.ru. 213.180.193.1, 2a02:6b8::1\nstate:         REGISTERED, DELEGATED, VERIFIED\n",
        )
        .unwrap();
        assert_eq!(
            tcinet.statuses.unwrap(),
            vec!["delegated", "registered", "verified"]
        );
        assert_eq!(tcinet.nameservers.unwrap(), vec!["ns1.yandex.ru"]);
    }

    #[test]
    fn nic_at_without_dnssec_line_is_unsigned() {
        let answer = "domain:            google.at\nnserver:           ns1.google.com\n";

        assert_eq!(
            RegistryRecord::from_whois("whois.nic.at", answer)
                .unwrap()
                .dnssec,
            Some(false)
        );
        assert_eq!(
            RegistryRecord::from_whois("whois.example", answer)
                .unwrap()
                .dnssec,
            None
        );
    }

    #[test]
    fn unreadable_answer_is_none() {
        assert_eq!(
            RegistryRecord::from_whois("", "Query rate limit exceeded. Try again later.\n"),
            None
        );
    }

    #[test]
    fn iana_referral_reads_whois_line() {
        let answer =
            "domain:       CO\n\nwhois:        whois.registry.co\n\nstatus:       ACTIVE\n";

        assert_eq!(referral(answer), Some("whois.registry.co".into()));
        assert_eq!(referral("domain: ARPA\nwhois:\n"), None);
    }
}

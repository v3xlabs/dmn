use crate::Error;
use async_std::net::TcpStream;
use async_std::prelude::*;
use std::fmt;
use std::net::ToSocketAddrs;
use whois_rust::{Target, WhoIs, WhoIsHost, WhoIsLookupOptions};
use regex::Regex;
use colored::*;
use comfy_table::{Table, presets::UTF8_FULL, ContentArrangement, Cell, Row, Attribute, CellAlignment};

/// Struct to hold the result of a whois lookup
#[derive(Debug, Clone)]
pub struct WhoisResult {
    pub domain: String,
    pub raw: String,
}

impl fmt::Display for WhoisResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "\x1b[1;34mWhois Lookup for\x1b[0m: \x1b[1;33m{}\x1b[0m",
            self.domain
        )?;
        writeln!(f, "\x1b[1;34m--- Whois Data ---\x1b[0m")?;
        writeln!(f, "{}", self.raw.trim())
    }
}

const WHOIS_SERVERS: &str = include_str!("servers.json");

/// Perform a whois lookup on the domain
pub async fn whois(domain: String) -> Result<WhoisResult, Error> {
    // Default whois server for most TLDs
    let whois = WhoIs::from_string(WHOIS_SERVERS).unwrap();

    let raw = whois
        .lookup_async(WhoIsLookupOptions::from_str(domain.clone()).unwrap())
        .await?;

    // strip off everything behind `>>> Last update of WHOIS database: 2025-04-16T07:27:49Z <<<`
    let raw = raw.split(">>> Last update of WHOIS database:").next().unwrap();

    // example of raw:
    /*
Domain Name: v3x.sh
Registry Domain ID: eff74bbdf39c4e258f687c70c22df6a3-DONUTS
Registrar WHOIS Server: whois.porkbun.com
Registrar URL: http://porkbun.com
Updated Date: 2025-04-10T00:03:30Z
Creation Date: 2022-12-27T00:24:43Z
Registry Expiry Date: 2025-12-27T00:24:43Z
Registrar: Porkbun LLC
Registrar IANA ID: 1861
Registrar Abuse Contact Email: abuse@porkbun.com
Registrar Abuse Contact Phone: +1.5038508351
Domain Status: clientDeleteProhibited https://icann.org/epp#clientDeleteProhibited
Domain Status: clientTransferProhibited https://icann.org/epp#clientTransferProhibited
Registry Registrant ID: REDACTED
Registrant Name: REDACTED
Registrant Organization: Private by Design, LLC
Registrant Street: REDACTED
Registrant City: REDACTED
Registrant State/Province: NC
Registrant Postal Code: REDACTED
Registrant Country: US
Registrant Phone: REDACTED
Registrant Phone Ext: REDACTED
Registrant Fax: REDACTED
Registrant Fax Ext: REDACTED
Registrant Email: REDACTED
Registry Admin ID: REDACTED
Admin Name: REDACTED
Admin Organization: REDACTED
Admin Street: REDACTED
Admin City: REDACTED
Admin State/Province: REDACTED
Admin Postal Code: REDACTED
Admin Country: REDACTED
Admin Phone: REDACTED
Admin Phone Ext: REDACTED
Admin Fax: REDACTED
Admin Fax Ext: REDACTED
Admin Email: REDACTED
Registry Tech ID: REDACTED
Tech Name: REDACTED
Tech Organization: REDACTED
Tech Street: REDACTED
Tech City: REDACTED
Tech State/Province: REDACTED
Tech Postal Code: REDACTED
Tech Country: REDACTED
Tech Phone: REDACTED
Tech Phone Ext: REDACTED
Tech Fax: REDACTED
Tech Fax Ext: REDACTED
Tech Email: REDACTED
Name Server: thomas.ns.cloudflare.com
Name Server: simone.ns.cloudflare.com
DNSSEC: unsigned
URL of the ICANN Whois Inaccuracy Complaint Form: https://icann.org/wicf/    
     */

    let styled = style_raw(raw);

    Ok(WhoisResult {
        domain,
        raw: styled,
    })
}

fn style_raw(raw: &str) -> String {
    // Define regexes for the fields we care about
    let re_fields = [
        ("Registrar", Regex::new(r"Registrar:\s*(.+)").unwrap()),
        ("Registrar URL", Regex::new(r"Registrar URL:\s*(.+)").unwrap()),
        ("Registry Expiry Date", Regex::new(r"Registry Expiry Date:\s*(.+)").unwrap()),
        ("Updated Date", Regex::new(r"Updated Date:\s*(.+)").unwrap()),
        ("Creation Date", Regex::new(r"Creation Date:\s*(.+)").unwrap()),
        ("DNSSEC", Regex::new(r"DNSSEC:\s*(.+)").unwrap()),
    ];
    let ns_re = Regex::new(r"Name Server:\s*(.+)").unwrap();
    let field_re = Regex::new(r"^([A-Za-z0-9 /_-]+):\s*(.+)$").unwrap();

    let mut values = vec![];
    let mut redacted_fields = vec![];
    for (label, re) in &re_fields {
        let value = re.captures(raw).and_then(|cap| cap.get(1)).map(|m| m.as_str().trim()).unwrap_or("");
        let styled = if value == "REDACTED" {
            redacted_fields.push(label.to_string());
            value.truecolor(128,128,128).to_string()
        } else if *label == "DNSSEC" {
            if value.eq_ignore_ascii_case("unsigned") {
                format!("{} {}", "DNSSEC".bold(), "✗".red())
            } else {
                format!("{} {}", "DNSSEC".bold(), "✔".green())
            }
        } else {
            value.yellow().to_string()
        };
        values.push((label.to_string(), styled));
    }

    // Name Servers (can be multiple)
    let name_servers: Vec<_> = ns_re.captures_iter(raw)
        .filter_map(|cap| cap.get(1))
        .map(|m| {
            let v = m.as_str().trim();
            if v == "REDACTED" {
                redacted_fields.push("Name Server".to_string());
                v.truecolor(128,128,128).to_string()
            } else {
                v.cyan().to_string()
            }
        })
        .collect();
    let ns_val = if !name_servers.is_empty() {
        name_servers.join("\n")
    } else {
        "-".to_string()
    };
    values.push(("Name Servers".to_string(), ns_val));

    // Build table with better wrapping and improved colors
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("Field").add_attribute(Attribute::Bold).fg(comfy_table::Color::Blue),
        Cell::new("Value").add_attribute(Attribute::Bold).fg(comfy_table::Color::Yellow),
    ]);
    for (k, v) in values {
        let key_cell = Cell::new(k.clone()).add_attribute(Attribute::Bold).fg(comfy_table::Color::Blue).set_alignment(CellAlignment::Left);
        // DNSSEC and Name Servers already colored, others yellow
        let value_cell = if k.as_str() == "DNSSEC" || k.as_str() == "Name Servers" {
            Cell::new(v).set_alignment(CellAlignment::Left)
        } else {
            Cell::new(v).fg(comfy_table::Color::Yellow).set_alignment(CellAlignment::Left)
        };
        table.add_row(Row::from(vec![key_cell, value_cell]));
    }

    // Show the rest of the raw output (less important fields)
    let mut rest = String::new();
    for line in raw.lines() {
        if line.trim().is_empty() { continue; }
        // skip already shown fields
        if re_fields.iter().any(|(_, re)| re.is_match(line)) || ns_re.is_match(line) { continue; }
        // skip REDACTED lines, but collect field name
        if let Some(cap) = field_re.captures(line) {
            let field = cap.get(1).unwrap().as_str().trim();
            let value = cap.get(2).unwrap().as_str().trim();
            if value == "REDACTED" {
                redacted_fields.push(field.to_string());
                continue;
            }
        }
        rest.push_str(line);
        rest.push('\n');
    }

    // Aggregate and deduplicate redacted fields
    redacted_fields.sort();
    redacted_fields.dedup();
    let redacted_summary = if !redacted_fields.is_empty() {
        format!("\nREDACTED: {}", redacted_fields.join(", "))
    } else {
        String::new()
    };

    format!("{}\n\n{}{}", table, rest.trim(), redacted_summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn test_whois() {
        let result = whois("google.com".to_string()).await.unwrap();
        println!("{}", result);
    }
}

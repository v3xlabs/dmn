use crate::Error;
use async_std::prelude::*;
use chrono::{DateTime, Utc};
use colored::*;
use comfy_table::{
    presets::UTF8_FULL, Attribute, Cell, CellAlignment, ContentArrangement, Row, Table,
};
use regex::Regex;
use serde_json::json;
use tracing::info;
use std::{collections::HashMap, fmt};
use whois_rust::{Target, WhoIs, WhoIsHost, WhoIsLookupOptions};

/// Struct to hold the result of a whois lookup
#[derive(Debug, Clone)]
pub struct WhoisResult {
    pub domain: String,
    pub raw: String,
}

impl fmt::Display for WhoisResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.raw.trim())
    }
}

const WHOIS_SERVERS: &str = include_str!("servers.json");

/// Perform a whois lookup on the domain
pub async fn whois(domain: String, json: bool) -> Result<WhoisResult, Error> {
    // Default whois server for most TLDs
    let whois = WhoIs::from_string(WHOIS_SERVERS).unwrap();

    let raw = whois
        .lookup_async(WhoIsLookupOptions::from_str(domain.clone()).unwrap())
        .await?;

    // strip off everything behind `>>> Last update of WHOIS database: 2025-04-16T07:27:49Z <<<`
    let raw = raw
        .split(">>> Last update of WHOIS database:")
        .next()
        .unwrap();

    let styled = if json {
        json_raw(raw)
    } else {
        style_raw(raw)
    };

    Ok(WhoisResult {
        domain,
        raw: styled,
    })
}

fn style_raw(raw: &str) -> String {
    let now = Utc::now();
    let re_fields = [
        ("Registrar", Regex::new(r"Registrar:\s*(.+)").unwrap()),
        (
            "Registrar URL",
            Regex::new(r"Registrar URL:\s*(.+)").unwrap(),
        ),
        (
            "Expiry Date",
            Regex::new(r"Registry Expiry Date:\s*(.+)|Expiration date:\s*(.+)").unwrap(),
        ),
        (
            "Updated Date",
            Regex::new(r"Updated Date:\s*(.+)|Modification date:\s*(.+)").unwrap(),
        ),
        (
            "Creation Date",
            Regex::new(r"Creation Date:\s*(.+)|Registration date:\s*(.+)").unwrap(),
        ),
        ("DNSSEC", Regex::new(r"DNSSEC:\s*(.+)|DNSSEC signed:\s*(.+)").unwrap()),
    ];
    let ns_re = Regex::new(r"Name Server:\s*(.+)|DNS:\s*(.+)").unwrap();
    let field_re = Regex::new(r"^([A-Za-z0-9 /_-]+):\s*(.+)$").unwrap();

    let mut values = vec![];
    let mut redacted_fields = vec![];
    for (label, re) in &re_fields {
        let value = re
            .captures(raw)
            .and_then(|cap| {
                let mut i = cap.iter();
                i.next();
                for m in i {
                    if let Some(m) = m {
                        return Some(m.as_str().trim());
                    }
                }
                None
            })
            .unwrap_or("-");
        let mut display_value = value.to_string();

        info!("{}: {}", label, value);
        // For date fields, append humanized time with explicit 'in' or 'ago'
        if ["Expiry Date", "Updated Date", "Creation Date"].contains(&label)
            && !value.is_empty()
            && value != "REDACTED"
        {
            info!("z{}: {}", label, value);
            // Try to parse both RFC3339 and fallback to custom format (e.g. 02.05.2023 14:51:07)
            let dt_opt = DateTime::parse_from_rfc3339(value)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(value, "%d.%m.%Y %H:%M:%S")
                        .ok()
                        .map(|dt| chrono::DateTime::<Utc>::from_utc(dt, Utc))
                });
            if let Some(dt_utc) = dt_opt {
                let duration = dt_utc.signed_duration_since(now);
                let ht = chrono_humanize::HumanTime::from(duration).to_string();
                display_value = format!("{} ({})", value, ht);
            }
        }
        let styled = if value == "REDACTED" {
            redacted_fields.push(label.to_string());
            value.truecolor(128, 128, 128).to_string()
        } else if *label == "DNSSEC" {
            if value.eq_ignore_ascii_case("unsigned") {
                format!("{} {}", "DNSSEC".bold(), "✗".red())
            } else {
                format!("{} {}", "DNSSEC".bold(), "✔".green())
            }
        } else if *label == "Name Servers" {
            display_value.cyan().to_string()
        } else {
            display_value.yellow().to_string()
        };
        values.push((label.to_string(), styled));
    }

    // Name Servers (can be multiple)
    let name_servers: Vec<_> = ns_re
        .captures_iter(raw)
        .filter_map(|cap| cap.get(1))
        .map(|m| {
            let v = m.as_str().trim();
            if v == "REDACTED" {
                redacted_fields.push("Name Server".to_string());
                v.truecolor(128, 128, 128).to_string()
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
        Cell::new("Field")
            .add_attribute(Attribute::Bold)
            .fg(comfy_table::Color::Blue),
        Cell::new("Value")
            .add_attribute(Attribute::Bold)
            .fg(comfy_table::Color::Yellow),
    ]);
    for (k, v) in values {
        let key_cell = Cell::new(k.clone())
            .add_attribute(Attribute::Bold)
            .fg(comfy_table::Color::Blue)
            .set_alignment(CellAlignment::Left);
        // DNSSEC and Name Servers already colored, others yellow
        let value_cell = if k.as_str() == "DNSSEC" || k.as_str() == "Name Servers" {
            Cell::new(v).set_alignment(CellAlignment::Left)
        } else {
            Cell::new(v)
                .fg(comfy_table::Color::Yellow)
                .set_alignment(CellAlignment::Left)
        };
        table.add_row(Row::from(vec![key_cell, value_cell]));
    }

    // Show the rest of the raw output (less important fields)
    let mut rest = String::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // skip already shown fields
        if re_fields.iter().any(|(_, re)| re.is_match(line)) || ns_re.is_match(line) {
            continue;
        }
        // skip REDACTED lines, but collect field name
        if let Some(cap) = field_re.captures(line) {
            let field = cap.get(1).unwrap().as_str().trim();
            let value = cap.get(2).unwrap().as_str().trim();
            if value == "REDACTED" {
                redacted_fields.push(field.to_string());
                continue;
            }

            rest.push_str(&format!("{}: {}", field.bright_black(), value));
            rest.push('\n');
        } else {
            rest.push_str(line);
            rest.push('\n');
        }
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

fn json_raw(raw: &str) -> String {
    let field_re = Regex::new(r"^([A-Za-z0-9 /_-]+):\s*(.+)$").unwrap();

    // interpret the raw string (somehow)
    // some lines will be `(.+): (.+)`
    // lets extract the key and value and put them in a hashmap
    // add any extra lines to extra_lines vec<string>
    let mut map = HashMap::new();
    let mut extra_lines = vec![];

    for line in raw.lines() {
        if let Some(cap) = field_re.captures(line) {
            let key = cap.get(1).unwrap().as_str().trim();
            let value = cap.get(2).unwrap().as_str().trim();
            map.insert(sluggify_key(key), value.to_string());
        } else {
            extra_lines.push(line.to_string());
        }
    }
    
    // return the hashmap and the extra lines as a json object
    let json = json!({
        "data": map,
        "extra_lines": extra_lines
    });
    json.to_string()
}

fn sluggify_key(key: &str) -> String {
    key.replace(" ", "_").replace("-", "_").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn test_whois() {
        let result = whois("google.com".to_string(), false).await.unwrap();
        println!("{}", result);
    }
}

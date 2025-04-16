use crate::Error;
use std::fmt;
use std::net::ToSocketAddrs;
use async_std::net::TcpStream;
use async_std::prelude::*;

/// Struct to hold the result of a whois lookup
#[derive(Debug, Clone)]
pub struct WhoisResult {
    pub domain: String,
    pub server: String,
    pub raw: String,
}

impl fmt::Display for WhoisResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\x1b[1;34mWhois Lookup for\x1b[0m: \x1b[1;33m{}\x1b[0m", self.domain)?;
        writeln!(f, "\x1b[1;34mServer\x1b[0m: \x1b[1;32m{}\x1b[0m", self.server)?;
        writeln!(f, "\x1b[1;34m--- Whois Data ---\x1b[0m")?;
        writeln!(f, "{}", self.raw.trim())
    }
}

/// Perform a whois lookup on the domain
pub async fn whois(domain: String) -> Result<WhoisResult, Error> {
    // Default whois server for most TLDs
    let default_server = "whois.iana.org:43";
    let mut server = default_server.to_string();

    // First query IANA to get the authoritative whois server for the TLD
    let tld = domain.split('.').last().unwrap_or("");
    let iana_query = tld.to_string() + "\r\n";
    let mut iana_stream = TcpStream::connect(default_server).await?;
    iana_stream.write_all(iana_query.as_bytes()).await?;
    let mut iana_buf = Vec::new();
    iana_stream.read_to_end(&mut iana_buf).await?;
    let iana_response = String::from_utf8_lossy(&iana_buf);
    for line in iana_response.lines() {
        if line.to_lowercase().starts_with("whois:") {
            if let Some(srv) = line.split(':').nth(1) {
                server = srv.trim().to_string() + ":43";
                break;
            }
        }
    }

    // Now query the actual whois server
    let addr = server
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| Error::msg("Failed to resolve whois server address"))?;
    let mut stream = TcpStream::connect(addr).await?;
    let query = domain.clone() + "\r\n";
    stream.write_all(query.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let response = String::from_utf8_lossy(&buf).to_string();

    Ok(WhoisResult {
        domain,
        server: server.trim_end_matches(":43").to_string(),
        raw: response,
    })
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


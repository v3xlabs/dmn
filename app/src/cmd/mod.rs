use async_std::prelude::FutureExt;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Row, Table};
use csv::Writer;
use figment::Figment;
use porkbun::PorkbunCommands;
use crate::models::domain::Domain;
use crate::modules::{
    cloudflare::CloudflareService, whois::whois, DomainService,
};
use crate::state::{AppState, AppStateInner};
use crate::{server, util, Error};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::Arc;

mod porkbun;

#[derive(Parser)]
#[command(
    name = "dmn",
    about = "Domain management CLI",
    version,
    long_about = None,
    after_help = "Run 'dmn --version' to see the version."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web server
    Server,
    /// List all domains
    Ls {
        /// Show exact dates ("2025-04-16 12:00:00" instead of "2 years ago")
        #[arg(long)]
        exact_dates: bool,
        /// Output format (json, csv, table) (default: table)
        #[arg(long, default_value = "table")]
        output: String,
    },
    /// Porkbun related commands
    Porkbun {
        #[command(subcommand)]
        subcommand: PorkbunCommands,
    },
    /// Cloudflare related commands
    Cloudflare {
        #[command(subcommand)]
        subcommand: CloudflareCommands,
    },
    /// fzf extension
    Fzf,
    /// Whois related commands
    Whois {
        /// The domain name to query
        domain: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum CloudflareCommands {
    /// Index Cloudflare domains
    Index,
}

pub async fn handle_args() -> Result<(), Error> {
    let cli = Cli::parse();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Server => {
            dotenvy::dotenv().ok();

            let state: AppState = Arc::new(AppStateInner::init(true).await);
            let http = server::start_http(state.clone());
            let cache_size_notifier = state.cache.collect(&state);
            cache_size_notifier.race(http).await;
        }
        Commands::Ls {
            exact_dates,
            output,
        } => {
            let state: AppState = Arc::new(AppStateInner::init(false).await);
            let domains = Domain::get_all(&state).await?;

            match output.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string(&domains)?);
                }
                "csv" => {
                    let mut wtr = Writer::from_writer(vec![]);
                    // write header row
                    wtr.write_record(&[
                        "Name",
                        "Provider",
                        "Status",
                        "Expiry",
                        "Registered",
                        "Auto Renew",
                    ])?;
                    for domain in domains {
                        // Extract status from metadata
                        let status = domain
                            .metadata
                            .as_ref()
                            .and_then(|meta| {
                                if let Some(s) = meta.get("status").and_then(|v| v.as_str()) {
                                    Some(s.to_string())
                                } else {
                                    meta.get("last_known_status")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                }
                            })
                            .unwrap_or_else(|| "".to_string());
                        // Format expiry date
                        let expiry = match &domain.ext_expiry_at {
                            Some(dt) => {
                                if *exact_dates {
                                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                } else {
                                    chrono_humanize::HumanTime::from(*dt - chrono::Utc::now())
                                        .to_string()
                                }
                            }
                            None => "".to_string(),
                        };
                        // Format registered date
                        let registered = match &domain.ext_registered_at {
                            Some(dt) => {
                                if *exact_dates {
                                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                } else {
                                    chrono_humanize::HumanTime::from(*dt - chrono::Utc::now())
                                        .to_string()
                                }
                            }
                            None => "".to_string(),
                        };
                        // Format auto renew
                        let auto_renew = match domain.ext_auto_renew {
                            Some(true) => "Yes".to_string(),
                            Some(false) => "No".to_string(),
                            None => "".to_string(),
                        };
                        wtr.write_record(&[
                            &domain.name,
                            &domain.provider,
                            &status,
                            &expiry,
                            &registered,
                            &auto_renew,
                        ])?;
                    }
                    wtr.flush()?;
                    let data = String::from_utf8(wtr.into_inner()?)?;
                    println!("{}", data);
                }
                "table" => {
                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL);
                    table.set_content_arrangement(ContentArrangement::Dynamic);
                    table.set_header(vec![
                        "Name",
                        "Provider",
                        "Status",
                        "Expiry",
                        "Registered",
                        "Auto Renew",
                    ]);
                    for domain in domains {
                        // Extract status from metadata
                        let status = domain.metadata.as_ref().and_then(|meta| {
                            if let Some(s) = meta.get("status").and_then(|v| v.as_str()) {
                                Some(s)
                            } else {
                                meta.get("last_known_status").and_then(|v| v.as_str())
                            }
                        });
                        let status_cell = match status.map(|s| s.to_ascii_uppercase()) {
                            Some(ref s) if s == "ACTIVE" || s == "REGISTRATIONACTIVE" => {
                                Cell::new("ACTIVE").fg(Color::Green)
                            }
                            Some(ref s) if s == "AUCTION" => {
                                Cell::new(status.unwrap()).fg(Color::Red)
                            }
                            Some(s) => Cell::new(s).fg(Color::Yellow),
                            None => Cell::new("-").fg(Color::DarkGrey),
                        };
                        table.add_row(Row::from(vec![
                            Cell::new(&domain.name),
                            Cell::new(&util::color::colorize_provider(&domain.provider)),
                            status_cell,
                            Cell::new(match &domain.ext_expiry_at {
                                Some(dt) => {
                                    if *exact_dates {
                                        dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                    } else {
                                        chrono_humanize::HumanTime::from(*dt - chrono::Utc::now())
                                            .to_string()
                                    }
                                }
                                None => "-".to_string(),
                            }),
                            Cell::new(match &domain.ext_registered_at {
                                Some(dt) => {
                                    if *exact_dates {
                                        dt.format("%Y-%m-%d %H:%M:%S").to_string()
                                    } else {
                                        chrono_humanize::HumanTime::from(*dt - chrono::Utc::now())
                                            .to_string()
                                    }
                                }
                                None => "-".to_string(),
                            }),
                            Cell::new(match domain.ext_auto_renew {
                                Some(true) => "Yes",
                                Some(false) => "No",
                                None => "-",
                            }),
                        ]));
                    }
                    println!("{}", table);
                }
                _ => {
                    println!("Invalid output format: {}", output);
                }
            }
        }
        Commands::Fzf => {
            let state: AppState = Arc::new(AppStateInner::init(false).await);
            let domains = Domain::get_all(&state).await?;
            let domain_names: Vec<String> = domains.iter().map(|d| d.name.clone()).collect();
            let input_data = domain_names.join("\n");

            // run fzf on the domain names
            // in the preview window on the right, show the domain details
            // the selected domain should be printed to stdout
            let mut child = Command::new("fzf")
                .arg("--preview")
                .arg("dmn whois {}")
                .arg("--preview-window")
                .arg("right")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input_data.as_bytes())?;
            } // stdin is closed when `stdin` goes out of scope

            let output = child.wait_with_output()?;

            if output.status.success() {
                let selected_domain = String::from_utf8(output.stdout)?.trim().to_string();
                if !selected_domain.is_empty() {
                    println!("{}", selected_domain);
                }
            } else {
                // fzf returns non-zero exit code if cancelled (e.g., Esc)
                // Print nothing or an error message if desired
                // eprintln!("fzf command failed or was cancelled.");
            }
        }
        Commands::Porkbun { subcommand } => {
            subcommand.handle().await?;
        }
        Commands::Cloudflare { subcommand } => {
            let cloudflare = CloudflareService::try_init(&Figment::new())
                .await
                .ok_or(Error::msg("Failed to initialize Cloudflare service"))?;
            let state: AppState = Arc::new(AppStateInner::init(false).await);

            match subcommand {
                CloudflareCommands::Index => {
                    println!("Indexing Cloudflare domains");
                    cloudflare.ingest_domains(&state).await?;
                }
            }
        }
        Commands::Whois { domain, json } => {
            if !json {
                println!("Querying Whois for domain: {}", domain);
            }
            let result = whois(domain.clone(), *json).await?;
            println!("{}", result);
        }
    }

    Ok(())
}

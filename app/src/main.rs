use async_std::prelude::FutureExt;
use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Row, Table};
use figment::Figment;
use models::domain::Domain;
use modules::{
    cloudflare::CloudflareService, porkbun::PorkbunService, whois::whois, DomainService,
};
use state::{AppState, AppStateInner};
use std::sync::Arc;

pub mod cache;
pub mod database;
pub mod models;
pub mod modules;
pub mod server;
pub mod state;
pub mod util;
pub mod web;

pub type Error = anyhow::Error;

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
enum PorkbunCommands {
    /// List Porkbun domains
    // Ls,
    /// Index Porkbun domains & dns
    Index {
        /// Do not index DNS records
        #[arg(long)]
        no_dns: bool,
    },
}

#[derive(Subcommand)]
enum CloudflareCommands {
    /// Index Cloudflare domains
    Index,
}

#[async_std::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting dmn");

    let cli = Cli::parse();

    match &cli.command {
        Commands::Server => {
            dotenvy::dotenv().ok();

            let state: AppState = Arc::new(AppStateInner::init(true).await);
            let http = server::start_http(state.clone());
            let cache_size_notifier = state.cache.collect(&state);
            cache_size_notifier.race(http).await;
        }
        Commands::Ls { exact_dates } => {
            let state: AppState = Arc::new(AppStateInner::init(false).await);
            let domains = Domain::get_all(&state).await?;
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
                    Some(ref s) if s == "AUCTION" => Cell::new(status.unwrap()).fg(Color::Red),
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
        Commands::Porkbun { subcommand } => {
            let porkbun = PorkbunService::try_init(&Figment::new())
                .await
                .ok_or(Error::msg("Failed to initialize Porkbun service"))?;
            let state: AppState = Arc::new(AppStateInner::init(false).await);

            match subcommand {
                // PorkbunCommands::Ls => {
                //     println!("Listing Porkbun domains");
                //     // TODO: Implement actual listing logic
                // }
                PorkbunCommands::Index { no_dns } => {
                    println!("Indexing Porkbun domains");
                    // TODO: Implement actual indexing logic
                    porkbun.ingest_domains(&state).await?;

                    if !no_dns {
                        // TODO: Implement actual DNS indexing logic
                    }
                }
            }
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

use async_std::prelude::FutureExt;
use clap::{Parser, Subcommand};
use figment::Figment;
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

pub type Error = anyhow::Error;

#[derive(Parser)]
#[command(name = "dmn", about = "Domain management CLI")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web server
    Server,
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
        Commands::Whois { domain } => {
            println!("Querying Whois for domain: {}", domain);
            let result = whois(domain.clone()).await?;
            println!("{}", result);
        }
    }

    Ok(())
}

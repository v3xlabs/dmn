use std::sync::Arc;

use clap::{arg, Subcommand};
use figment::Figment;
use tracing::{info, warn};

use crate::{modules::{domains::diff_provider, porkbun::PorkbunService, DomainService}, state::{AppState, AppStateInner}, Error};

#[derive(Subcommand)]
pub enum PorkbunCommands {
    /// List Porkbun domains
    // Ls,
    /// Index Porkbun domains & dns
    Index {
        /// Do not index DNS records
        #[arg(long)]
        no_dns: bool,
        /// Skip ingesting domains
        #[arg(long)]
        no_domains: bool,
        /// Skip ingesting domain tld prices
        #[arg(long)]
        no_pricing: bool,
    },
}

impl PorkbunCommands {
    pub async fn handle(&self) -> Result<(), Error> {
        let porkbun = PorkbunService::try_init(&Figment::new())
            .await
            .ok_or(Error::msg("Failed to initialize Porkbun service"))?;
        let state: AppState = Arc::new(AppStateInner::init(false).await);

        match self {
            // PorkbunCommands::Ls => {
            //     println!("Listing Porkbun domains");
            //     // TODO: Implement actual listing logic
            // }
            PorkbunCommands::Index {
                no_dns,
                no_domains,
                no_pricing,
            } => {
                println!("Indexing Porkbun domains");
                // TODO: Implement actual indexing logic

                if !no_domains {
                    let notifications = diff_provider(&state, "porkbun", &porkbun).await?;

                    if let Some(ntfy) = &state.ntfy {
                        if !notifications.is_empty() {
                            info!("Sending notifications to Ntfy");
                            ntfy.send_notifications(notifications).await?;
                        } else {
                            info!("No notifications to send");
                        }
                    } else {
                        warn!("Ntfy service not initialized");
                    }
                }

                if !no_dns {
                    // TODO: Implement actual DNS indexing logic
                }

                if !no_pricing {
                    porkbun.ingest_domain_tld_prices_if_enabled(&state).await?;
                }

                Ok(())
            }
        }
    }
}

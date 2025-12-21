use std::sync::Arc;
use clap::Subcommand;
use figment::Figment;
use tracing::{info, warn};

use crate::{
    modules::{domains::diff_provider, ens::EnsService},
    state::{AppState, AppStateInner},
    Error,
};

#[derive(Subcommand)]
pub enum EnsCommands {
    /// Index ENS domains
    Index,
}

impl EnsCommands {
    pub async fn handle(&self) -> Result<(), Error> {
        let config = crate::state::get_config_file().unwrap_or_else(|| Figment::new());
        let ens = EnsService::try_init(&config)
            .await
            .ok_or(Error::msg("Failed to initialize ENS service"))?;
        let state: AppState = Arc::new(AppStateInner::init(false).await);

        match self {
            EnsCommands::Index => {
                println!("Indexing ENS domains");

                let notifications = diff_provider(&state, "ens", &ens).await?;

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
        }

        Ok(())
    }
}

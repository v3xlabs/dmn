use std::time::Duration;

use async_std::stream::{self, StreamExt};
use tracing::{info, warn};

use crate::{modules::{domains::diff_provider, DomainService}, state::AppState, Error};

pub async fn start_schedule(state: &AppState) {
    match do_loop(state).await {
        Ok(_) => (),
        Err(e) => {
            tracing::error!("Error in schedule: {}", e);
        }
    }

    // 1 hr by default
    let mut ticks = stream::interval(Duration::from_secs(60 * 60));

    while (ticks.next().await).is_some() {
        match do_loop(state).await {
            Ok(_) => (),
            Err(e) => {
                tracing::error!("Error in schedule: {}", e);
            }
        }
    }
}

async fn do_loop(state: &AppState) -> Result<(), Error> {
    if let Some(porkbun) = &state.porkbun {
        let notifications = diff_provider(&state, "porkbun", porkbun).await?;

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

        porkbun.ingest_domain_tld_prices_if_enabled(state).await?;
    }

    Ok(())
}

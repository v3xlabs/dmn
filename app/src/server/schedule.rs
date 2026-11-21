use std::time::Duration;

use async_std::{
    prelude::FutureExt,
    stream::{self, StreamExt},
};
use tracing::{error, info, warn};

use crate::{
    modules::{domains::diff_provider, DomainService},
    state::AppState,
    Error,
};

pub async fn start_schedule(state: &AppState) {
    let porkbun_loop = async {
        if let Err(e) = do_loop(state).await {
            error!("Error in schedule: {}", e);
        }

        // 1 hr by default
        let mut ticks = stream::interval(Duration::from_secs(60 * 60));
        while (ticks.next().await).is_some() {
            if let Err(e) = do_loop(state).await {
                error!("Error in schedule: {}", e);
            }
        }
    };
    let ens_loop = async {
        if let Some(ens) = &state.ens {
            ens.run_schedule(state).await;
        }
    };
    porkbun_loop.join(ens_loop).await;
}

async fn do_loop(state: &AppState) -> Result<(), Error> {
    if let Some(porkbun) = &state.porkbun {
        let notifications = diff_provider(state, "porkbun", porkbun).await?.notifications;

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

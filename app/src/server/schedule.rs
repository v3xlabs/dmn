use std::time::Duration;

use async_std::stream::{self, StreamExt};
use tracing::{info, warn};

use crate::{modules::domains::diff_provider, state::AppState, Error};

pub async fn start_schedule(state: &AppState) {
    let _ = do_loop(state).await;

    // 1 hr by default
    let mut ticks = stream::interval(Duration::from_secs(60 * 60));

    while (ticks.next().await).is_some() {
        let _ = do_loop(state).await;
    }
}

async fn do_loop(state: &AppState) -> Result<(), Error> {
    if let Some(porkbun) = &state.porkbun {
        let notifications = diff_provider(&state, "porkbun", porkbun).await?;

        if let Some(ntfy) = &state.ntfy {
            info!("Sending notifications to Ntfy");
            ntfy.send_notifications(notifications).await?;
        } else {
            warn!("Ntfy service not initialized");
        }
    }

    Ok(())
}

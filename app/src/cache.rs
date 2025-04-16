// use crate::models::party::PartyState;
use crate::state::AppState;

pub struct AppCache {}

impl AppCache {
    pub fn new() -> Self {
        Self {
            // bm_user_from_name: Cache::builder()
            //     .time_to_live(Duration::from_secs(5 * 60))
            //     .time_to_idle(Duration::from_secs(1 * 60))
            //     .max_capacity(1000)
            //     .build(),
        }
    }

    // pub async fn get_sizes(&self) -> (u64, u64, u64, u64, u64, u64) {
    //     (
    //         self.bm_user_from_name.weighted_size(),
    //         self.bm_recent_servers.weighted_size(),
    //         self.rm_search.weighted_size(),
    //         self.rm_map.weighted_size(),
    //         self.scmm_total_inventory.weighted_size(),
    //         self.party_state.weighted_size(),
    //     )
    // }

    pub async fn collect(&self, _state: &AppState) {
        loop {
            self.collect_all().await;

            // let (
            //     bm_user_from_name,
            //     bm_recent_servers,
            //     rm_search,
            //     rm_map,
            //     scmm_total_inventory,
            //     party_state,
            // ) = self.get_sizes().await;
            // tracing::info!(
            //     bm_user_from_name,
            //     bm_recent_servers,
            //     rm_search,
            //     rm_map,
            //     scmm_total_inventory,
            //     party_state
            // );
            async_std::task::sleep(std::time::Duration::from_secs(3 * 60)).await;
        }
    }

    pub async fn collect_all(&self) {
        // let tasks = vec![
        //     // async { self.bm_user_from_name.run_pending_tasks().await }.boxed(),
        //     // async { self.bm_recent_servers.run_pending_tasks().await }.boxed(),
        //     // async { self.rm_search.run_pending_tasks().await }.boxed(),
        //     // async { self.rm_map.run_pending_tasks().await }.boxed(),
        //     // async { self.scmm_total_inventory.run_pending_tasks().await }.boxed(),
        //     // async { self.party_state.run_pending_tasks().await }.boxed(),
        // ];

        // join_all(tasks).await;
    }
}

impl Default for AppCache {
    fn default() -> Self {
        Self::new()
    }
}

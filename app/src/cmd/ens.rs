use std::sync::Arc;

use clap::Subcommand;

use crate::{
    models::domain::Domain,
    modules::{
        domains::diff_provider,
        ens::{EnsService, EnsVersion},
    },
    state::{get_config_file, AppState, AppStateInner},
    Error,
};

#[derive(Subcommand)]
pub enum EnsCommands {
    /// Index configured ENS names directly via Ethereum RPC
    Index,
    /// Look up a single ENS name directly on-chain via Ethereum RPC
    Lookup {
        /// ENS name to query (for example: nick.eth)
        name: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

impl EnsCommands {
    pub async fn handle(&self) -> Result<(), Error> {
        let config = get_config_file(false).unwrap_or_default();

        match self {
            Self::Index => {
                let ens = EnsService::try_init(&config).await.ok_or_else(|| {
                    Error::msg(
                        "Failed to initialize ENS service (configure ENS_NAMES or [ens] in config.toml)",
                    )
                })?;
                let state: AppState = Arc::new(AppStateInner::init(false).await);

                println!("Indexing ENS domains via {}", ens.rpc_url());
                let _ = diff_provider(&state, "ens", &ens).await?;

                let stored = Domain::find_by_provider(&state, "ens").await?;
                let (mut v1, mut upgraded, mut v2) = (0, 0, 0);
                for domain in &stored {
                    match domain.metadata.as_ref().and_then(EnsVersion::from_metadata) {
                        Some(EnsVersion::V1) => v1 += 1,
                        Some(EnsVersion::Upgraded) => upgraded += 1,
                        Some(EnsVersion::V2) => v2 += 1,
                        None => {}
                    }
                }
                println!(
                    "Indexed {} ENS domains ({v1} v1, {upgraded} upgraded, {v2} v2)",
                    stored.len()
                );
            }
            Self::Lookup { name, json } => {
                let ens = EnsService::for_lookup(&config)?;
                match ens.lookup_name(name).await? {
                    Some(domain) if *json => println!("{}", serde_json::to_string_pretty(&domain)?),
                    Some(domain) => print!("{domain}"),
                    None => return Err(Error::msg(format!("ENS name not registered: {name}"))),
                }
            }
        }

        Ok(())
    }
}

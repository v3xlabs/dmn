use chrono::{DateTime, Duration, Utc};

use super::{
    abi::{u64_to_datetime, V2SlotStatus, V2State, MAX_SUPPORTED_TIMESTAMP},
    rpc::{GRAVEYARD_V2, NAME_WRAPPER_V1},
    EnsDomain, EnsMetadata, EnsName, EnsVersion,
};

/// Post-expiry renewal grace periods from `BaseRegistrarImplementation.sol` (90d) and `ETHRegistrar.sol` (28d).
const ENS_V1_GRACE_PERIOD_SECS: i64 = 90 * 24 * 60 * 60;
const ENS_V2_GRACE_PERIOD_SECS: i64 = 28 * 24 * 60 * 60;

#[derive(Debug)]
pub struct V2Probe {
    pub state: V2State,
    pub was_reserved: bool,
    pub resolver: Option<String>,
}

#[derive(Debug)]
pub struct V1Probe {
    pub registry_owner: Option<String>,
    pub resolver: Option<String>,
    pub base_registrar_owner: Option<String>,
    pub base_registrar_expiry: u64,
    pub wrapper_owner: Option<String>,
}

impl V1Probe {
    fn is_held_by_graveyard(&self) -> bool {
        [
            &self.registry_owner,
            &self.base_registrar_owner,
            &self.wrapper_owner,
        ]
        .into_iter()
        .any(|owner| {
            owner
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(GRAVEYARD_V2))
        })
    }

    /// Distinguishes v1->v2 migrations (finite v1 lease transferred to `GRAVEYARD_V2`) from `Graveyard.clear()` retired names (re-registered with near-`u64::MAX` expiry).
    fn is_migrated_to_graveyard(&self) -> bool {
        self.is_held_by_graveyard() && self.base_registrar_expiry <= MAX_SUPPORTED_TIMESTAMP
    }

    fn ownership(&self) -> (Option<String>, Option<String>, Option<String>) {
        let is_wrapper = |a: &str| a.eq_ignore_ascii_case(NAME_WRAPPER_V1);
        let wrapped = self.registry_owner.as_deref().is_some_and(is_wrapper)
            || self.base_registrar_owner.as_deref().is_some_and(is_wrapper);

        if wrapped {
            return match self
                .wrapper_owner
                .as_deref()
                .filter(|owner| !owner.eq_ignore_ascii_case(GRAVEYARD_V2))
            {
                Some(owner) => (
                    Some(owner.to_string()),
                    Some(owner.to_string()),
                    Some("wrapped".to_string()),
                ),
                None => (None, None, Some("lapsed".to_string())),
            };
        }

        let is_user = |addr: &&str| !addr.eq_ignore_ascii_case(GRAVEYARD_V2) && !is_wrapper(addr);
        let reg_owner = self
            .registry_owner
            .as_deref()
            .filter(is_user)
            .map(String::from);
        let base_owner = self
            .base_registrar_owner
            .as_deref()
            .filter(is_user)
            .map(String::from);
        let owner = base_owner.or_else(|| reg_owner.clone());
        let wrapper_state =
            (owner.is_some() || reg_owner.is_some()).then(|| "unwrapped".to_string());
        (owner, reg_owner, wrapper_state)
    }
}

impl EnsDomain {
    pub fn from_probes(
        name: &EnsName,
        v2: &V2Probe,
        v1: &V1Probe,
        now: DateTime<Utc>,
    ) -> Option<Self> {
        let namehash_hex = format!("0x{}", hex::encode(name.namehash()));
        let state = &v2.state;

        match state.status {
            V2SlotStatus::Registered => {
                let expiry_date = u64_to_datetime(state.expiry)?;
                let ens_version = if v2.was_reserved {
                    EnsVersion::Upgraded
                } else {
                    EnsVersion::V2
                };
                return Some(Self {
                    name: name.as_str().to_string(),
                    expiry_date,
                    registration_date: None,
                    metadata: EnsMetadata {
                        status: "active".to_string(),
                        ens_version,
                        authority: Some("ens_v2".to_string()),
                        namehash: Some(namehash_hex),
                        owner: state.latest_owner.clone(),
                        manager: state.latest_owner.clone(),
                        grace_ends_at: Some(
                            expiry_date + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS),
                        ),
                        v1_expiry_at: None,
                        wrapper_state: None,
                        resolver: v2.resolver.clone(),
                    },
                });
            }
            V2SlotStatus::Reserved => {
                // Unmigrated v1 names store `v1_expiry + 62d` in `state.expiry`, so `state.expiry + 28d` equals the 90d v1 grace deadline while `v1.base_registrar_expiry` is the real expiry.
                let v2_reserved_expiry = u64_to_datetime(state.expiry);
                let v1_expiry_at = u64_to_datetime(v1.base_registrar_expiry);
                let expiry_date = v1_expiry_at.or(v2_reserved_expiry)?;
                let grace_ends_at = v2_reserved_expiry.map_or(
                    expiry_date + Duration::seconds(ENS_V1_GRACE_PERIOD_SECS),
                    |e| e + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS),
                );
                let status = if now >= expiry_date {
                    "expired"
                } else {
                    "active"
                };
                let (owner, manager, wrapper_state) = v1.ownership();
                return Some(Self {
                    name: name.as_str().to_string(),
                    expiry_date,
                    registration_date: None,
                    metadata: EnsMetadata {
                        status: status.to_string(),
                        ens_version: EnsVersion::V1,
                        authority: Some("ens_v1".to_string()),
                        namehash: Some(namehash_hex),
                        owner,
                        manager,
                        grace_ends_at: Some(grace_ends_at),
                        v1_expiry_at,
                        wrapper_state,
                        resolver: v1.resolver.clone(),
                    },
                });
            }
            V2SlotStatus::Available => {
                // Expired ENSv2 names in 28d grace retain `latestOwner`, while `_constructResource` returns `eacVersionId + 1` so `roles()` returns 0 and we fall back to `v1.is_migrated_to_graveyard()`.
                if state.expiry > 0 && state.latest_owner.is_some() {
                    let expiry_date = u64_to_datetime(state.expiry)?;
                    let grace_ends_at = expiry_date + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS);
                    if now >= grace_ends_at {
                        return None;
                    }
                    let ens_version = if v2.was_reserved || v1.is_migrated_to_graveyard() {
                        EnsVersion::Upgraded
                    } else {
                        EnsVersion::V2
                    };
                    return Some(Self {
                        name: name.as_str().to_string(),
                        expiry_date,
                        registration_date: None,
                        metadata: EnsMetadata {
                            status: "expired".to_string(),
                            ens_version,
                            authority: Some("ens_v2".to_string()),
                            namehash: Some(namehash_hex),
                            owner: state.latest_owner.clone(),
                            manager: None,
                            grace_ends_at: Some(grace_ends_at),
                            v1_expiry_at: None,
                            wrapper_state: None,
                            resolver: v2.resolver.clone(),
                        },
                    });
                }
            }
        }

        if v1.is_held_by_graveyard() {
            return None;
        }

        let v1_expiry_at = u64_to_datetime(v1.base_registrar_expiry)?;
        let grace_ends_at = v1_expiry_at + Duration::seconds(ENS_V1_GRACE_PERIOD_SECS);
        if now >= grace_ends_at {
            return None;
        }
        let status = if now >= v1_expiry_at {
            "expired"
        } else {
            "active"
        };
        let (owner, manager, wrapper_state) = v1.ownership();
        Some(Self {
            name: name.as_str().to_string(),
            expiry_date: v1_expiry_at,
            registration_date: None,
            metadata: EnsMetadata {
                status: status.to_string(),
                ens_version: EnsVersion::V1,
                authority: Some("ens_v1".to_string()),
                namehash: Some(namehash_hex),
                owner,
                manager,
                grace_ends_at: Some(grace_ends_at),
                v1_expiry_at: Some(v1_expiry_at),
                wrapper_state,
                resolver: v1.resolver.clone(),
            },
        })
    }
}

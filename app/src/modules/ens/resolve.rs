use chrono::{DateTime, Duration, Utc};

use super::{
    abi::{u64_to_datetime, V2SlotStatus, V2State, WrapperData, MAX_SUPPORTED_TIMESTAMP},
    rpc::{GRAVEYARD_V2, NAME_WRAPPER_V1},
    EnsDomain, EnsMetadata, EnsName, EnsVersion,
};

/// Post-expiry renewal grace periods from `BaseRegistrarImplementation.sol` (90d) and `ETHRegistrar.sol` (28d).
const ENS_V1_GRACE_PERIOD_SECS: i64 = 90 * 24 * 60 * 60;
const ENS_V2_GRACE_PERIOD_SECS: i64 = 28 * 24 * 60 * 60;

#[derive(Debug, Default)]
pub struct V2Probe {
    pub state: Option<V2State>,
    pub was_reserved: bool,
    pub resolver: Option<String>,
    pub subregistry: Option<String>,
}

#[derive(Debug, Default)]
pub struct V1Probe {
    pub registry_owner: Option<String>,
    pub resolver: Option<String>,
    pub base_registrar_owner: Option<String>,
    pub base_registrar_expiry: u64,
    pub wrapper: Option<WrapperData>,
}

impl V1Probe {
    fn is_held_by_graveyard(&self) -> bool {
        self.registry_owner
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(GRAVEYARD_V2))
            || self
                .base_registrar_owner
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(GRAVEYARD_V2))
            || self
                .wrapper
                .as_ref()
                .and_then(|w| w.owner.as_deref())
                .is_some_and(|a| a.eq_ignore_ascii_case(GRAVEYARD_V2))
    }

    /// Distinguishes v1->v2 migrations (finite v1 lease transferred to `GRAVEYARD_V2`) from `Graveyard.clear()` retired names (re-registered with near-`u64::MAX` expiry).
    fn is_migrated_to_graveyard(&self) -> bool {
        self.is_held_by_graveyard() && self.base_registrar_expiry <= MAX_SUPPORTED_TIMESTAMP
    }

    fn ownership(
        &self,
    ) -> (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<DateTime<Utc>>,
    ) {
        let wrapped = self
            .registry_owner
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(NAME_WRAPPER_V1))
            || self
                .base_registrar_owner
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(NAME_WRAPPER_V1));

        if wrapped {
            if let Some(wrapper) = &self.wrapper {
                if let Some(owner) = &wrapper.owner {
                    if !owner.eq_ignore_ascii_case(GRAVEYARD_V2) {
                        return (
                            Some(owner.clone()),
                            Some(owner.clone()),
                            Some(wrapper.state_label().to_string()),
                            u64_to_datetime(wrapper.expiry),
                        );
                    }
                }
            }
            return (None, None, Some("lapsed".to_string()), None);
        }

        let is_user = |addr: &&str| {
            !addr.eq_ignore_ascii_case(GRAVEYARD_V2) && !addr.eq_ignore_ascii_case(NAME_WRAPPER_V1)
        };
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
        (owner, reg_owner, wrapper_state, None)
    }
}

impl EnsDomain {
    pub fn from_probes(
        name: &EnsName,
        v2: &V2Probe,
        v1: &V1Probe,
        now: DateTime<Utc>,
    ) -> Option<Self> {
        let eth_2ld = name.is_eth_2ld();
        let namehash_hex = format!("0x{}", hex::encode(name.namehash()));

        if let Some(state) = &v2.state {
            match state.status {
                V2SlotStatus::Registered => {
                    let expiry_date = u64_to_datetime(state.expiry);
                    let grace_ends_at = eth_2ld
                        .then(|| {
                            expiry_date.map(|e| e + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS))
                        })
                        .flatten();
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
                            grace_ends_at,
                            v1_expiry_at: None,
                            wrapper_state: None,
                            wrapper_expiry_at: None,
                            resolver: v2.resolver.clone(),
                            subregistry: v2.subregistry.clone(),
                        },
                    });
                }
                V2SlotStatus::Reserved => {
                    // Unmigrated v1 `.eth` names store `v1_expiry + 62d` in `state.expiry`, so `state.expiry + 28d` equals the 90d v1 grace deadline while `v1.base_registrar_expiry` is the real expiry.
                    let v2_reserved_expiry = u64_to_datetime(state.expiry);
                    let v1_expiry_at = eth_2ld
                        .then(|| u64_to_datetime(v1.base_registrar_expiry))
                        .flatten();
                    let expiry_date = v1_expiry_at.or(v2_reserved_expiry);
                    let grace_ends_at = eth_2ld
                        .then(|| {
                            v2_reserved_expiry
                                .map(|e| e + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS))
                                .or_else(|| {
                                    v1_expiry_at
                                        .map(|e| e + Duration::seconds(ENS_V1_GRACE_PERIOD_SECS))
                                })
                        })
                        .flatten();
                    let status = if expiry_date.is_some_and(|e| now >= e) {
                        "expired"
                    } else {
                        "active"
                    };
                    let (owner, manager, wrapper_state, wrapper_expiry_at) = v1.ownership();
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
                            grace_ends_at,
                            v1_expiry_at,
                            wrapper_state,
                            wrapper_expiry_at,
                            resolver: v1.resolver.clone(),
                            subregistry: None,
                        },
                    });
                }
                V2SlotStatus::Available => {
                    // Expired ENSv2 `.eth` names in 28d grace retain `latestOwner`, while `_constructResource` returns `eacVersionId + 1` so `roles()` returns 0 and we fall back to `v1.is_migrated_to_graveyard()`.
                    if eth_2ld && state.expiry > 0 && state.latest_owner.is_some() {
                        let expiry_date = u64_to_datetime(state.expiry);
                        let grace_ends_at =
                            expiry_date.map(|e| e + Duration::seconds(ENS_V2_GRACE_PERIOD_SECS));
                        if grace_ends_at.is_some_and(|g| now < g) {
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
                                    grace_ends_at,
                                    v1_expiry_at: None,
                                    wrapper_state: None,
                                    wrapper_expiry_at: None,
                                    resolver: v2.resolver.clone(),
                                    subregistry: v2.subregistry.clone(),
                                },
                            });
                        }
                        return None;
                    }
                }
            }
        }

        if v1.is_held_by_graveyard() {
            return None;
        }

        let (owner, manager, wrapper_state, wrapper_expiry_at) = v1.ownership();
        if eth_2ld {
            let v1_expiry_at = u64_to_datetime(v1.base_registrar_expiry);
            let grace_ends_at =
                v1_expiry_at.map(|e| e + Duration::seconds(ENS_V1_GRACE_PERIOD_SECS));
            if !grace_ends_at.is_some_and(|g| now < g) {
                return None;
            }
            let status = if v1_expiry_at.is_some_and(|e| now >= e) {
                "expired"
            } else {
                "active"
            };
            return Some(Self {
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
                    grace_ends_at,
                    v1_expiry_at,
                    wrapper_state,
                    wrapper_expiry_at,
                    resolver: v1.resolver.clone(),
                    subregistry: None,
                },
            });
        }

        if owner.is_none() && manager.is_none() {
            return None;
        }

        let status = if v1.wrapper.as_ref().is_some_and(WrapperData::is_emancipated)
            && wrapper_expiry_at.is_some_and(|e| now >= e)
        {
            "expired"
        } else {
            "active"
        };

        Some(Self {
            name: name.as_str().to_string(),
            expiry_date: wrapper_expiry_at,
            registration_date: None,
            metadata: EnsMetadata {
                status: status.to_string(),
                ens_version: EnsVersion::V1,
                authority: Some("ens_v1".to_string()),
                namehash: Some(namehash_hex),
                owner,
                manager,
                grace_ends_at: None,
                v1_expiry_at: None,
                wrapper_state,
                wrapper_expiry_at,
                resolver: v1.resolver.clone(),
                subregistry: None,
            },
        })
    }
}

use anyhow::{anyhow, Context, Error};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    abi::{keccak256, AbiWords, Calldata, V2State, WrapperData},
    resolve::{V1Probe, V2Probe},
    EnsDomain, EnsName, EvmAddress,
};

/// Default Ethereum Mainnet JSON-RPC endpoint (matches ENS Labs' public failover RPC).
pub const DEFAULT_ENS_RPC_URL: &str = "https://ethereum-rpc.publicnode.com";

/// Canonical Ethereum Mainnet ENS contract addresses from `docs.ens.domains/learn/deployments`.
pub const ROOT_REGISTRY_V2: &str = "0x2553c09be6447beffdb86fa622ad41fb8f77a91e";
pub const ETH_REGISTRY_V2: &str = "0xf60aba43e1f7f3dab1186ff03b561401c9df85e2";
pub const GRAVEYARD_V2: &str = "0xda021a2c1c2f99574d861ef8c6fa745851beb7f6";
pub const ENS_REGISTRY_V1: &str = "0x00000000000c2e074ec69a0dfb2997ba6c7d2e1e";
pub const BASE_REGISTRAR_V1: &str = "0x57f1887a8bf19b14fc0df6fd9b2acc9af147ea85";
pub const NAME_WRAPPER_V1: &str = "0xd4416b13d2b3a9abae7acd5d6c2bbdbe25686401";

/// Nybble 8 marker role set by `PermissionedRegistry._register()` when promoting a premigrated `RESERVED` v1 slot to `REGISTERED`, which fresh ENSv2 registrations never hold.
const ROLE_WAS_RESERVED_BIT: u64 = 1_u64 << 32;

/// Names with more than 16 labels skip the ENSv2 lookup and resolve through ENSv1 only, which bounds one lookup to 16 sequential `getSubregistry` calls.
const MAX_SUBREGISTRY_DEPTH: usize = 16;

#[derive(Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: (EthCallParam<'a>, &'static str),
}

#[derive(Serialize)]
struct EthCallParam<'a> {
    to: &'a str,
    data: String,
}

#[derive(Deserialize)]
struct RpcResponse {
    id: u64,
    result: Option<String>,
    error: Option<Value>,
}

pub struct RpcClient<'a> {
    client: &'a reqwest::Client,
    url: &'a str,
}

impl<'a> RpcClient<'a> {
    pub fn new(client: &'a reqwest::Client, url: &'a str) -> Self {
        Self { client, url }
    }

    pub async fn resolve_name(&self, name: &EnsName) -> Result<Option<EnsDomain>, Error> {
        let labels = name.labels();
        if name.is_eth_2ld() {
            return self.resolve_eth_2ld(name, labels[0]).await;
        }
        self.resolve_hierarchical(name, &labels).await
    }

    async fn batch_call(
        &self,
        calls: &[(&str, Calldata)],
    ) -> Result<Vec<Result<Vec<u8>, String>>, Error> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }

        let requests: Vec<RpcRequest<'_>> = calls
            .iter()
            .enumerate()
            .map(|(idx, (to, data))| RpcRequest {
                jsonrpc: "2.0",
                id: (idx + 1) as u64,
                method: "eth_call",
                params: (
                    EthCallParam {
                        to,
                        data: data.to_hex(),
                    },
                    "latest",
                ),
            })
            .collect();

        let response = self
            .client
            .post(self.url)
            .json(&requests)
            .send()
            .await
            .with_context(|| format!("Ethereum RPC request failed for {}", self.url))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("failed to read Ethereum RPC body from {}", self.url))?;
        if !status.is_success() {
            return Err(anyhow!("Ethereum RPC returned HTTP {status}: {body}"));
        }

        let mut responses: Vec<RpcResponse> = serde_json::from_str(&body)
            .with_context(|| format!("invalid JSON-RPC batch response: {body}"))?;
        if responses.len() != calls.len() {
            return Err(anyhow!(
                "Ethereum RPC batch size mismatch: expected {}, got {}",
                calls.len(),
                responses.len()
            ));
        }
        responses.sort_by_key(|r| r.id);

        let mut decoded = Vec::with_capacity(responses.len());
        for (idx, resp) in responses.into_iter().enumerate() {
            let expected_id = (idx + 1) as u64;
            if resp.id != expected_id {
                return Err(anyhow!(
                    "Ethereum RPC batch id mismatch at index {idx}: expected {expected_id}, got {}",
                    resp.id
                ));
            }
            if let Some(err) = resp.error {
                decoded.push(Err(err.to_string()));
            } else if let Some(hex_str) = resp.result {
                let raw = hex_str
                    .strip_prefix("0x")
                    .or_else(|| hex_str.strip_prefix("0X"))
                    .unwrap_or(&hex_str);
                let bytes = hex::decode(raw)
                    .with_context(|| format!("Ethereum RPC returned invalid hex: {hex_str}"))?;
                decoded.push(Ok(bytes));
            } else {
                decoded.push(Err("missing result in JSON-RPC response".to_string()));
            }
        }
        Ok(decoded)
    }

    async fn resolve_eth_2ld(
        &self,
        name: &EnsName,
        label: &str,
    ) -> Result<Option<EnsDomain>, Error> {
        let lh = keccak256(label.as_bytes());
        let nh = name.namehash();
        let calls = [
            (ETH_REGISTRY_V2, Calldata::bytes32("getState(uint256)", &lh)),
            (
                ETH_REGISTRY_V2,
                Calldata::string("getResolver(string)", label),
            ),
            (
                ETH_REGISTRY_V2,
                Calldata::string("getSubregistry(string)", label),
            ),
            (ENS_REGISTRY_V1, Calldata::bytes32("owner(bytes32)", &nh)),
            (ENS_REGISTRY_V1, Calldata::bytes32("resolver(bytes32)", &nh)),
            (
                BASE_REGISTRAR_V1,
                Calldata::bytes32("nameExpires(uint256)", &lh),
            ),
            (
                BASE_REGISTRAR_V1,
                Calldata::bytes32("ownerOf(uint256)", &lh),
            ),
            (NAME_WRAPPER_V1, Calldata::bytes32("getData(uint256)", &nh)),
        ];
        let results = self.batch_call(&calls).await?;

        let v2_state = results[0]
            .as_deref()
            .map_err(|err| anyhow!("ETHRegistry.getState reverted for {name}: {err}"))
            .and_then(|b| V2State::try_from(b).map_err(|e| anyhow!("{e}")))?;

        let v1_registry_owner = results[3]
            .as_deref()
            .map_err(|err| anyhow!("ENSRegistry.owner failed for {name}: {err}"))
            .and_then(|b| {
                AbiWords::new(b)
                    .address(0)
                    .ok_or_else(|| anyhow!("invalid owner ABI"))
            })?;
        let v1_expiry = results[5]
            .as_deref()
            .map_err(|err| anyhow!("BaseRegistrar.nameExpires failed for {name}: {err}"))
            .and_then(|b| {
                AbiWords::new(b)
                    .u64(0)
                    .ok_or_else(|| anyhow!("invalid expiry ABI"))
            })?;

        let was_reserved = match &v2_state.latest_owner {
            Some(owner) => {
                self.query_was_reserved(ETH_REGISTRY_V2, &v2_state.resource, owner)
                    .await?
            }
            None => false,
        };

        let v2 = V2Probe {
            state: Some(v2_state),
            was_reserved,
            resolver: results[1]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
            subregistry: results[2]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
        };
        let v1 = V1Probe {
            registry_owner: v1_registry_owner,
            resolver: results[4]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
            base_registrar_owner: results[6]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
            base_registrar_expiry: v1_expiry,
            wrapper: results[7]
                .as_deref()
                .ok()
                .and_then(|b| WrapperData::try_from(b).ok()),
        };

        Ok(EnsDomain::from_probes(name, &v2, &v1, Utc::now()))
    }

    async fn resolve_hierarchical(
        &self,
        name: &EnsName,
        labels: &[&str],
    ) -> Result<Option<EnsDomain>, Error> {
        let mut v2 = V2Probe::default();
        if labels.len() <= MAX_SUBREGISTRY_DEPTH {
            if let Some(parent_registry) = self.walk_v2_parent(labels).await? {
                let leaf = labels[0];
                let lh = keccak256(leaf.as_bytes());
                let calls = [
                    (
                        parent_registry.as_str(),
                        Calldata::bytes32("getState(uint256)", &lh),
                    ),
                    (
                        parent_registry.as_str(),
                        Calldata::string("getResolver(string)", leaf),
                    ),
                    (
                        parent_registry.as_str(),
                        Calldata::string("getSubregistry(string)", leaf),
                    ),
                ];
                let results = self.batch_call(&calls).await?;
                if let Some(state) = results[0]
                    .as_deref()
                    .ok()
                    .and_then(|b| V2State::try_from(b).ok())
                {
                    v2 = V2Probe {
                        state: Some(state),
                        // Subnames in UserRegistry were not batch-premigrated as RESERVED.
                        was_reserved: false,
                        resolver: results[1]
                            .as_deref()
                            .ok()
                            .and_then(|b| AbiWords::new(b).address(0))
                            .flatten(),
                        subregistry: results[2]
                            .as_deref()
                            .ok()
                            .and_then(|b| AbiWords::new(b).address(0))
                            .flatten(),
                    };
                }
            }
        }

        let nh = name.namehash();
        let v1_calls = [
            (ENS_REGISTRY_V1, Calldata::bytes32("owner(bytes32)", &nh)),
            (ENS_REGISTRY_V1, Calldata::bytes32("resolver(bytes32)", &nh)),
            (NAME_WRAPPER_V1, Calldata::bytes32("getData(uint256)", &nh)),
        ];
        let v1_results = self.batch_call(&v1_calls).await?;
        let v1 = V1Probe {
            registry_owner: v1_results[0]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
            resolver: v1_results[1]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten(),
            base_registrar_owner: None,
            base_registrar_expiry: 0,
            wrapper: v1_results[2]
                .as_deref()
                .ok()
                .and_then(|b| WrapperData::try_from(b).ok()),
        };

        Ok(EnsDomain::from_probes(name, &v2, &v1, Utc::now()))
    }

    async fn walk_v2_parent(&self, labels: &[&str]) -> Result<Option<String>, Error> {
        if labels.len() < 2 {
            return Ok(Some(ROOT_REGISTRY_V2.to_string()));
        }
        let (mut current, start) =
            if *labels.last().expect("EnsName has at least one label") == "eth" {
                (ETH_REGISTRY_V2.to_string(), labels.len() - 1)
            } else {
                (ROOT_REGISTRY_V2.to_string(), labels.len())
            };
        for idx in (1..start).rev() {
            let calls = [(
                current.as_str(),
                Calldata::string("getSubregistry(string)", labels[idx]),
            )];
            let results = self.batch_call(&calls).await?;
            match results[0]
                .as_deref()
                .ok()
                .and_then(|b| AbiWords::new(b).address(0))
                .flatten()
            {
                Some(next) => current = next,
                None => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    async fn query_was_reserved(
        &self,
        registry: &str,
        resource: &[u8; 32],
        owner: &str,
    ) -> Result<bool, Error> {
        let Ok(account) = owner.parse::<EvmAddress>() else {
            return Ok(false);
        };
        let calls = [(registry, Calldata::roles(resource, &account))];
        let results = self.batch_call(&calls).await?;
        Ok(results[0]
            .as_deref()
            .ok()
            .and_then(|b| AbiWords::new(b).has_role_bit(ROLE_WAS_RESERVED_BIT))
            .unwrap_or(false))
    }
}

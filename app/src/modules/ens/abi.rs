use chrono::{DateTime, Utc};
use tiny_keccak::{Hasher, Keccak};

use super::EvmAddress;

/// Upper bound on Unix timestamps converted into `DateTime<Utc>` (`9999-12-31T23:59:59Z`) because non-expiring ENSv2 entries use `u64::MAX`, which exceeds RFC3339 and SQLite date ranges.
pub const MAX_SUPPORTED_TIMESTAMP: u64 = 253_402_300_799;

const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
const FUSE_CANNOT_UNWRAP: u32 = 1 << 0;
const FUSE_PARENT_CANNOT_CONTROL: u32 = 1 << 16;

pub fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(input);
    let mut out = [0_u8; 32];
    keccak.finalize(&mut out);
    out
}

pub fn u64_to_datetime(ts: u64) -> Option<DateTime<Utc>> {
    if ts == 0 || ts > MAX_SUPPORTED_TIMESTAMP {
        return None;
    }
    DateTime::from_timestamp(ts as i64, 0)
}

fn selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

pub struct Calldata(Vec<u8>);

impl Calldata {
    pub fn bytes32(signature: &str, arg: &[u8; 32]) -> Self {
        let mut out = Vec::with_capacity(36);
        out.extend_from_slice(&selector(signature));
        out.extend_from_slice(arg);
        Self(out)
    }

    pub fn string(signature: &str, value: &str) -> Self {
        let bytes = value.as_bytes();
        let padded_len = bytes.len().div_ceil(32) * 32;
        let mut out = Vec::with_capacity(4 + 64 + padded_len);
        out.extend_from_slice(&selector(signature));
        let mut offset = [0_u8; 32];
        offset[31] = 32;
        out.extend_from_slice(&offset);
        let mut len_word = [0_u8; 32];
        len_word[24..].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
        out.extend_from_slice(&len_word);
        out.extend_from_slice(bytes);
        out.resize(4 + 64 + padded_len, 0);
        Self(out)
    }

    pub fn roles(resource: &[u8; 32], account: &EvmAddress) -> Self {
        let mut out = Vec::with_capacity(4 + 64);
        out.extend_from_slice(&selector("roles(uint256,address)"));
        out.extend_from_slice(resource);
        let mut addr_word = [0_u8; 32];
        addr_word[12..].copy_from_slice(&account.to_bytes());
        out.extend_from_slice(&addr_word);
        Self(out)
    }

    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.0))
    }
}

pub struct AbiWords<'a>(&'a [u8]);

impl<'a> AbiWords<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    fn word(&self, index: usize) -> Option<&'a [u8]> {
        let start = index.checked_mul(32)?;
        self.0.get(start..start.checked_add(32)?)
    }

    pub fn address(&self, index: usize) -> Option<Option<String>> {
        let w = self.word(index)?;
        let addr = format!("0x{}", hex::encode(&w[12..32]));
        Some((addr != ZERO_ADDRESS).then_some(addr))
    }

    pub fn u64(&self, index: usize) -> Option<u64> {
        let w = self.word(index)?;
        if w[..24].iter().any(|&b| b != 0) {
            return Some(u64::MAX);
        }
        Some(u64::from_be_bytes(w[24..32].try_into().ok()?))
    }

    pub fn u32(&self, index: usize) -> Option<u32> {
        let w = self.word(index)?;
        Some(u32::from_be_bytes(w[28..32].try_into().ok()?))
    }

    pub fn bytes32(&self, index: usize) -> Option<[u8; 32]> {
        self.word(index)?.try_into().ok()
    }

    pub fn has_role_bit(&self, role_bit: u64) -> Option<bool> {
        let w = self.word(0)?;
        let low64 = u64::from_be_bytes(w[24..32].try_into().ok()?);
        Some((low64 & role_bit) != 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V2SlotStatus {
    Available,
    Reserved,
    Registered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2State {
    pub status: V2SlotStatus,
    pub expiry: u64,
    pub latest_owner: Option<String>,
    pub resource: [u8; 32],
}

impl TryFrom<&[u8]> for V2State {
    type Error = &'static str;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let words = AbiWords::new(data);
        let status = match words.word(0).ok_or("missing status word")?[31] {
            0 => V2SlotStatus::Available,
            1 => V2SlotStatus::Reserved,
            2 => V2SlotStatus::Registered,
            _ => return Err("unknown ENSv2 status"),
        };
        let expiry = words.u64(1).ok_or("missing expiry word")?;
        let latest_owner = words.address(2).ok_or("missing latestOwner word")?;
        let resource = words.bytes32(4).ok_or("missing resource word")?;
        Ok(Self {
            status,
            expiry,
            latest_owner,
            resource,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WrapperData {
    pub owner: Option<String>,
    pub fuses: u32,
    pub expiry: u64,
}

impl WrapperData {
    pub fn state_label(&self) -> &'static str {
        let pcc = (self.fuses & FUSE_PARENT_CANNOT_CONTROL) != 0;
        let cannot_unwrap = (self.fuses & FUSE_CANNOT_UNWRAP) != 0;
        if pcc && cannot_unwrap {
            "locked"
        } else if pcc {
            "emancipated"
        } else {
            "wrapped"
        }
    }

    pub fn is_emancipated(&self) -> bool {
        (self.fuses & FUSE_PARENT_CANNOT_CONTROL) != 0
    }
}

impl TryFrom<&[u8]> for WrapperData {
    type Error = &'static str;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let words = AbiWords::new(data);
        Ok(Self {
            owner: words.address(0).ok_or("missing wrapper owner")?,
            fuses: words.u32(1).ok_or("missing wrapper fuses")?,
            expiry: words.u64(2).ok_or("missing wrapper expiry")?,
        })
    }
}

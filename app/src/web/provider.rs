use maud::PreEscaped;
use strum::{Display, EnumString};

use super::icons;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Display, EnumString)]
#[strum(serialize_all = "title_case", ascii_case_insensitive)]
pub enum ProviderKind {
    Porkbun,
    Cloudflare,
    #[strum(serialize = "ENS")]
    Ens,
    Manual,
    #[strum(default)]
    Other(String),
}

impl ProviderKind {
    pub fn icon(&self) -> PreEscaped<&'static str> {
        match self {
            Self::Porkbun => icons::PORKBUN,
            Self::Cloudflare => icons::CLOUDFLARE,
            Self::Ens => icons::ENS,
            Self::Manual | Self::Other(_) => icons::GLOBE,
        }
    }
}

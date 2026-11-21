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
    #[strum(default)]
    Other(String),
}

impl ProviderKind {
    pub fn icon(&self) -> PreEscaped<&'static str> {
        match self {
            Self::Porkbun => icons::PORKBUN,
            Self::Cloudflare => icons::CLOUDFLARE,
            Self::Ens => icons::ENS,
            Self::Other(_) => icons::GLOBE,
        }
    }

    /// ENS subnames under ENSv2 have no expiry by design; every other provider owes an expiry date.
    pub fn allows_no_expiry(&self) -> bool {
        matches!(self, Self::Ens)
    }
}

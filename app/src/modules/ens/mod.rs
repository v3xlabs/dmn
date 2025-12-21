use figment::{providers::Env, Figment, Provider};
use serde::{de::{self, Deserializer, SeqAccess, Visitor}, Deserialize, Serialize};
use tracing::{info, warn};
use chrono::{DateTime, Utc};

pub mod domains;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnsConfig {
    #[serde(deserialize_with = "deserialize_addresses")]
    pub addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnsDomain {
    pub name: String,
    pub expiry_date: Option<DateTime<Utc>>,
    pub registration_date: Option<DateTime<Utc>>,
}

pub struct EnsService {
    pub config: EnsConfig,
}

fn deserialize_addresses<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct AddressesVisitor;

    impl<'de> Visitor<'de> for AddressesVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let values = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect::<Vec<_>>();

            Ok(values)
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&v)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(AddressesVisitor)
}

impl EnsService {
    pub fn new(config: EnsConfig) -> Self {
        Self { config }
    }

    pub async fn try_init(provider: &impl Provider) -> Option<Self> {
        let figment = Figment::new()
            .merge(Env::prefixed("ENS_"))
            .merge(provider);

        let config = figment.focus("ens").extract::<EnsConfig>()
            .or_else(|_| figment.extract::<EnsConfig>());

        match config {
            Ok(config) => {
                if config.addresses.is_empty() {
                    warn!("ENS config found but addresses are empty");
                    return None;
                }
                info!("ENS config verified with {} addresses", config.addresses.len());
                Some(Self::new(config))
            }
            Err(e) => {
                warn!("ENS config extraction failed: {:?}", e);
                None
            }
        }
    }
}

use chrono::{DateTime, Utc};
use poem::{web::Data, Error};
use poem_openapi::{param::Path, payload::Json, ApiResponse, Object, OpenApi};
use reqwest::StatusCode;

use crate::{
    models::{domain::Domain, index_state::IndexKind, snapshot::Snapshot},
    modules::registry::name::DomainName,
    server::ApiTags,
    state::AppState,
};

pub struct DomainApi;

/// One registry answer for a name, with the raw RDAP JSON or WHOIS text.
#[derive(Object)]
pub struct RegistrySnapshot {
    pub source: String,
    pub server: String,
    pub observed_at: DateTime<Utc>,
    pub confirmed_at: DateTime<Utc>,
    pub raw: String,
}

impl From<Snapshot> for RegistrySnapshot {
    fn from(snapshot: Snapshot) -> Self {
        Self {
            source: snapshot.source.to_string(),
            server: snapshot.server,
            observed_at: snapshot.observed_at,
            confirmed_at: snapshot.confirmed_at,
            raw: snapshot.raw,
        }
    }
}

#[derive(ApiResponse)]
enum RegistryResponse {
    #[oai(status = 200)]
    Ok(Json<RegistrySnapshot>),
    /// No tracked name has this name, or the name has no snapshot with this id.
    #[oai(status = 404)]
    NotFound,
}

#[OpenApi]
impl DomainApi {
    #[oai(path = "/domains", method = "get", tag = "ApiTags::Domains")]
    async fn get_domains(&self, state: Data<&AppState>) -> Result<Json<Vec<Domain>>, Error> {
        let domains = Domain::get_all(&state).await.map_err(|x| {
            poem::error::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR)
        })?;
        Ok(Json(domains))
    }

    /// The raw answer of one registry snapshot, for a name that a provider tracks now.
    #[oai(
        path = "/domains/:name/registry/:snapshot",
        method = "get",
        tag = "ApiTags::Domains"
    )]
    async fn get_registry(
        &self,
        state: Data<&AppState>,
        name: Path<String>,
        snapshot: Path<i64>,
    ) -> poem::Result<RegistryResponse> {
        let Ok(name) = name.parse::<DomainName>() else {
            return Ok(RegistryResponse::NotFound);
        };
        let mut conn = state
            .database
            .pool
            .acquire()
            .await
            .map_err(poem::error::InternalServerError)?;
        let snapshot = Snapshot::find_tracked(&mut conn, snapshot.0, &name, IndexKind::Registry)
            .await
            .map_err(poem::error::InternalServerError)?;
        Ok(match snapshot {
            Some(snapshot) => RegistryResponse::Ok(Json(snapshot.into())),
            None => RegistryResponse::NotFound,
        })
    }
}

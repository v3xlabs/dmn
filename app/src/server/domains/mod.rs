use poem::{web::Data, Error};
use poem_openapi::{payload::Json, OpenApi};
use reqwest::StatusCode;

use crate::{models::domain::Domain, state::AppState, server::ApiTags};

pub struct DomainApi;

#[OpenApi]
impl DomainApi {
    #[oai(path = "/domains", method = "get", tag = "ApiTags::Domains")]
    async fn get_domains(&self, state: Data<&AppState>) -> Result<Json<Vec<Domain>>, Error> {
        let domains = Domain::get_all(&state).await.map_err(|x| {
            poem::error::Error::from_status(StatusCode::INTERNAL_SERVER_ERROR)
        })?;
        Ok(Json(domains))
    }
}

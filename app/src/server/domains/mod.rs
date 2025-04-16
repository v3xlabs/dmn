use poem::Error;
use poem_openapi::{payload::Json, OpenApi};

use crate::models::domain::Domain;

pub struct DomainApi;

#[OpenApi]
impl DomainApi {
    #[oai(path = "/domains", method = "get")]
    async fn get_domains(&self) -> Result<Json<Vec<Domain>>, Error> {
        Ok(Json(vec![]))
    }
}

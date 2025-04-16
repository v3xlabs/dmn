use anyhow::Error;

use crate::models::domain::Domain;

use super::PorkbunService;

impl PorkbunService {
    pub async fn get_domains(&self) -> Result<Vec<Domain>, Error> {
        let domains = vec![];
        Ok(domains)
    }
}


#[cfg(test)]
mod tests {

    use crate::state::AppStateInner;

    use super::*;

    #[async_std::test]
    async fn test_get_domains() {
        dotenvy::dotenv().ok();
        let state = AppStateInner::init().await;

        let service = PorkbunService::new("test".to_string());
        let domains = service.get_domains().await.unwrap();

        assert_eq!(domains.len(), 0);
    }
}

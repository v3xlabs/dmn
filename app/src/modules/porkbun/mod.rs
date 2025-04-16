pub mod domains;

pub struct PorkbunService {
    api_key: String,
}

impl PorkbunService {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

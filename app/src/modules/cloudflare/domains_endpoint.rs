use cloudflare::framework::{
    endpoint::{serialize_query, spec::EndpointSpec},
    response::ApiResult,
    response::ApiSuccess,
};
use reqwest::Method;
use serde::{Deserialize, Serialize};

use super::domains::CloudflareDomain;

#[derive(Debug)]
pub struct ListDomains {
    pub params: ListDomainsParams,
}

impl EndpointSpec for ListDomains {
    type JsonResponse = CloudflareVec;
    type ResponseType = ApiSuccess<Self::JsonResponse>;

    fn method(&self) -> Method {
        Method::GET
    }
    fn path(&self) -> String {
        format!("accounts/{}/registrar/domains", self.params.account)
    }
    #[inline]
    fn query(&self) -> Option<String> {
        serialize_query(&self.params)
    }

    fn body(&self) -> Option<cloudflare::framework::endpoint::RequestBody> {
        None
    }

    fn url(&self, environment: &cloudflare::framework::Environment) -> reqwest::Url {
        let mut url = reqwest::Url::from(environment).join(&self.path()).unwrap();
        url.set_query(self.query().as_deref());
        url
    }

    fn content_type(&self) -> Option<std::borrow::Cow<'static, str>> {
        match Self::body(self) {
            Some(cloudflare::framework::endpoint::RequestBody::Json(_)) => {
                Some(std::borrow::Cow::Borrowed("application/json"))
            }
            Some(cloudflare::framework::endpoint::RequestBody::Raw(_)) => {
                Some(std::borrow::Cow::Borrowed("application/octet-stream"))
            }
            Some(cloudflare::framework::endpoint::RequestBody::MultiPart(_)) => {
                Some(std::borrow::Cow::Borrowed("multipart/form-data"))
            }
            None => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CloudflareVec(serde_json::Value);

impl ApiResult for CloudflareVec {}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListDomainsParams {
    pub account: String,
}

impl CloudflareVec {
    pub fn into_vec(self) -> Vec<CloudflareDomain> {
        serde_json::from_value(self.0).unwrap()
    }
}

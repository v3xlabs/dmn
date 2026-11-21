use std::collections::HashSet;

use anyhow::{anyhow, bail};
use cloudflare::framework::endpoint::{serialize_query, spec::EndpointSpec, Method};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::Error;

use super::CloudflareService;

/// One page of a Cloudflare list endpoint, read raw because the `cloudflare` crate's envelope type drops `success` and fails a whole page on any field it cannot read.
#[derive(Debug, Serialize)]
struct ListPage {
    #[serde(skip)]
    path: String,
    page: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_page: Option<u32>,
}

impl EndpointSpec for ListPage {
    const IS_RAW_BODY: bool = true;

    type JsonResponse = ();
    type ResponseType = Vec<u8>;

    fn method(&self) -> Method {
        Method::GET
    }

    fn path(&self) -> String {
        self.path.clone()
    }

    fn query(&self) -> Option<String> {
        serialize_query(self)
    }
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<Value>,
    result: Option<Vec<T>>,
    result_info: Option<ResultInfo>,
}

#[derive(Debug, Deserialize)]
struct ResultInfo {
    page: u32,
    total_count: usize,
    total_pages: Option<u32>,
}

impl CloudflareService {
    /// Fails on a 2xx body that reports failure, carries errors, lacks paging info or answers another page, because any of them could make a partial listing look complete.
    async fn fetch_page<T: DeserializeOwned>(
        &self,
        path: &str,
        page: u32,
        per_page: Option<u32>,
    ) -> Result<(Vec<T>, ResultInfo), Error> {
        let request = ListPage {
            path: path.to_string(),
            page,
            per_page,
        };
        let body = self.client.request(&request).await?;
        let envelope: Envelope<T> = serde_json::from_slice(&body)?;
        if !envelope.success || !envelope.errors.is_empty() {
            bail!(
                "Cloudflare {path} listing failed: success {}, errors {:?}",
                envelope.success,
                envelope.errors
            );
        }
        let info = envelope
            .result_info
            .ok_or_else(|| anyhow!("Cloudflare {path} listing has no result_info"))?;
        if info.page != page {
            bail!(
                "Cloudflare {path} returned page {} for page {page}",
                info.page
            );
        }
        let items = envelope
            .result
            .ok_or_else(|| anyhow!("Cloudflare {path} listing has no result"))?;
        Ok((items, info))
    }

    pub async fn check_listing<T: DeserializeOwned>(
        &self,
        path: &str,
        per_page: Option<u32>,
    ) -> Result<(), Error> {
        self.fetch_page::<T>(path, 1, per_page).await.map(|_| ())
    }

    /// Returns every item only when each page reports the totals of page 1, no key repeats, and the item count equals `total_count`; when the endpoint reports `total_pages`, every page up to it is read and the count may not be reached before the last one.
    /// The checks cannot see an item deleted and another added between two pages, which keep the totals equal.
    pub async fn list_all<T: DeserializeOwned>(
        &self,
        path: &str,
        per_page: Option<u32>,
        key: impl Fn(&T) -> &str,
    ) -> Result<Vec<T>, Error> {
        let (mut batch, first) = self.fetch_page::<T>(path, 1, per_page).await?;
        let (total_count, total_pages) = (first.total_count, first.total_pages);
        if total_pages == Some(0) && (total_count != 0 || !batch.is_empty()) {
            bail!(
                "Cloudflare {path} reported 0 pages with {total_count} items and {} on page 1",
                batch.len()
            );
        }
        let mut items = Vec::new();
        let mut keys = HashSet::new();
        let mut page = 1;
        loop {
            if batch.is_empty() && items.len() < total_count {
                bail!(
                    "Cloudflare {path} page {page} is empty after {} of {total_count} items",
                    items.len()
                );
            }
            for item in batch {
                if !keys.insert(key(&item).to_string()) {
                    bail!("Cloudflare {path} listed {} twice", key(&item));
                }
                items.push(item);
            }
            let last_page = match total_pages {
                Some(last) => {
                    if page < last && items.len() >= total_count {
                        bail!(
                            "Cloudflare {path} reached all {total_count} items on page {page} of {last}"
                        );
                    }
                    page >= last
                }
                None => items.len() >= total_count,
            };
            if last_page {
                break;
            }
            page += 1;
            let (next, info) = self.fetch_page::<T>(path, page, per_page).await?;
            if (info.total_count, info.total_pages) != (total_count, total_pages) {
                bail!(
                    "Cloudflare {path} totals changed during the listing: {total_count} items in {total_pages:?} pages, then {} in {:?}",
                    info.total_count,
                    info.total_pages
                );
            }
            batch = next;
        }
        if items.len() != total_count {
            bail!(
                "Cloudflare {path} listed {} items but reported {total_count}",
                items.len()
            );
        }
        Ok(items)
    }
}

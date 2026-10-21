//! Provider for the (unidentified) "recomp" site.
//!
//! Step 0 of the spec found no verifiable public API. This provider is
//! therefore disabled by default and only enabled when the user supplies an
//! explicit HTTPS endpoint. If that endpoint returns the catalog JSON shape it
//! is parsed with the same validator as local manifests. HTML parsing is
//! intentionally not guessed: it will be added against saved fixtures once a
//! real URL is provided.

use async_trait::async_trait;
use tracing::warn;

use super::manifest::{CatalogValidator, MAX_CATALOG_BYTES};
use super::model::GameEntry;
use super::provider::{CatalogError, CatalogProvider, ProviderInfo};
use crate::net::http_client::HttpClient;

pub struct RecompSiteProvider {
    info: ProviderInfo,
    client: HttpClient,
    endpoint: Option<String>,
}

impl RecompSiteProvider {
    pub fn new(client: HttpClient, endpoint: Option<String>) -> Self {
        Self {
            info: ProviderInfo {
                id: "recomp-site".to_string(),
                name: "Site recomp".to_string(),
                url: endpoint.clone().unwrap_or_default(),
            },
            client,
            endpoint,
        }
    }

    pub fn disabled(client: HttpClient) -> Self {
        Self::new(client, None)
    }
}

#[async_trait]
impl CatalogProvider for RecompSiteProvider {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    async fn fetch_all(&self) -> Result<Vec<GameEntry>, CatalogError> {
        let endpoint = self.endpoint.as_deref().ok_or_else(|| {
            CatalogError::NotConfigured(
                "no verified recomp site URL; using the embedded manifest".to_string(),
            )
        })?;

        let response = self.client.get_limited(endpoint, MAX_CATALOG_BYTES).await?;

        // Only JSON endpoints are supported until a real HTML source is verified.
        let parsed =
            CatalogValidator::new()?.parse_catalog_str(&response.text().map_err(|error| {
                CatalogError::InvalidDocument(format!("response is not UTF-8: {error}"))
            })?)?;

        let mut entries = parsed.entries;
        for entry in &mut entries {
            entry.provider = Some(self.info.id.clone());
        }
        warn!(
            count = entries.len(),
            "recomp site provider returned JSON; HTML sources are not enabled"
        );
        Ok(entries)
    }
}

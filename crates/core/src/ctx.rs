//! Application context: owns every long-lived service.
//!
//! Created once in `main`, wrapped in `Arc` and shared with the backend task.
//! The eframe thread only sees commands/events; it never calls these methods
//! directly except for pure lookups (paths, settings handle).

use std::sync::Arc;

use url::Url;

use crate::adapters::AdapterRegistry;
use crate::catalog::manifest::ManifestProvider;
use crate::catalog::recomp_site::RecompSiteProvider;
use crate::catalog::service::{CatalogService, CatalogSnapshot};
use crate::catalog::CatalogProvider;
use crate::db::{Db, DbError};
use crate::error::Result;
use crate::launch::GameLauncher;
use crate::library::install::InstallManager;
use crate::library::Library;
use crate::metadata::{MetadataService, WikipediaProvider};
use crate::mods::ModManager;
use crate::net::http_client::{HttpClient, HttpConfig};
use crate::online::{
    FriendsStore, JsonServerProvider, OnlineService, ServerProvider, DEFAULT_LAN_SERVICE,
};
use crate::paths::AppPaths;
use crate::settings::{
    Settings, KEY_CATALOG_MANIFEST_URL, KEY_CATALOG_TTL_HOURS, KEY_LAN_SERVICE, KEY_PROXY,
    KEY_RECOMP_SITE_URL, KEY_SERVERS_URL,
};

/// Shared services used by the backend.
pub struct AppContext {
    paths: AppPaths,
    db: Db,
    settings: Settings,
    http: HttpClient,
    catalog: CatalogService,
    library: Library,
    installer: InstallManager,
    mods: ModManager,
    launcher: GameLauncher,
    adapters: AdapterRegistry,
    metadata: MetadataService,
    online: OnlineService,
    friends: FriendsStore,
}

impl AppContext {
    /// Opens the database, applies migrations, reads network settings and
    /// registers the default catalog providers.
    pub async fn new(paths: AppPaths) -> Result<Self> {
        paths.ensure_base_dirs()?;
        let db = Db::open(paths.db_path()).await?;
        let settings = Settings::new(db.clone());

        let manifest_url = non_empty(settings.get(KEY_CATALOG_MANIFEST_URL).await?);
        let recomp_url = non_empty(settings.get(KEY_RECOMP_SITE_URL).await?);
        let servers_url = non_empty(settings.get(KEY_SERVERS_URL).await?);
        let proxy = non_empty(settings.get(KEY_PROXY).await?);
        let ttl_hours = settings.get_i64(KEY_CATALOG_TTL_HOURS, 6).await?;
        let lan_service = settings
            .get_or(KEY_LAN_SERVICE, DEFAULT_LAN_SERVICE)
            .await?;

        let mut extra_hosts = Vec::new();
        for candidate in [
            manifest_url.as_deref(),
            recomp_url.as_deref(),
            servers_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(host) = Url::parse(candidate)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
            {
                extra_hosts.push(host);
            }
        }

        let http = HttpClient::new(
            HttpConfig {
                proxy,
                ..HttpConfig::default()
            },
            extra_hosts,
        )?;

        let mut catalog = CatalogService::new(db.clone());
        catalog.set_ttl_hours(ttl_hours);
        catalog.add_provider(Arc::new(ManifestProvider::embedded()) as Arc<dyn CatalogProvider>);
        if let Some(url) = manifest_url {
            catalog
                .add_provider(Arc::new(ManifestProvider::remote(http.clone(), url))
                    as Arc<dyn CatalogProvider>);
        }
        if let Some(url) = recomp_url {
            catalog.add_provider(Arc::new(RecompSiteProvider::new(http.clone(), Some(url)))
                as Arc<dyn CatalogProvider>);
        }

        let library = Library::new(db.clone());
        let installer = InstallManager::new(paths.clone(), http.clone(), library.clone());
        let mods = ModManager::new(paths.clone(), library.clone());
        let adapters = AdapterRegistry::new();
        let launcher =
            GameLauncher::new(library.clone(), mods.clone()).with_adapters(adapters.clone());
        let metadata =
            MetadataService::new(db.clone(), Arc::new(WikipediaProvider::new(http.clone())));
        let server_provider = servers_url.map(|url| {
            Arc::new(JsonServerProvider::new(http.clone(), url)) as Arc<dyn ServerProvider>
        });
        let online = OnlineService::new(db.clone(), server_provider, lan_service);
        let friends = FriendsStore::new(db.clone());

        Ok(Self {
            paths,
            db,
            settings,
            http,
            catalog,
            library,
            installer,
            mods,
            launcher,
            adapters,
            metadata,
            online,
            friends,
        })
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    pub fn catalog(&self) -> &CatalogService {
        &self.catalog
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn installer(&self) -> &InstallManager {
        &self.installer
    }

    pub fn mods(&self) -> &ModManager {
        &self.mods
    }

    pub fn launcher(&self) -> &GameLauncher {
        &self.launcher
    }

    pub fn adapters(&self) -> &AdapterRegistry {
        &self.adapters
    }

    pub fn metadata(&self) -> &MetadataService {
        &self.metadata
    }

    pub fn online(&self) -> &OnlineService {
        &self.online
    }

    pub fn friends(&self) -> &FriendsStore {
        &self.friends
    }

    pub async fn load_cached_catalog(&self) -> std::result::Result<CatalogSnapshot, DbError> {
        self.catalog.load_cached().await
    }

    pub async fn refresh_catalog(&self) -> std::result::Result<CatalogSnapshot, DbError> {
        self.catalog.refresh().await
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

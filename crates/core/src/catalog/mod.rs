//! Catalog domain: manifest model, providers, validation and cache service.

pub mod filter;
pub mod github_releases;
pub mod manifest;
pub mod model;
pub mod provider;
pub mod recomp_site;
pub mod service;

#[cfg(test)]
pub(crate) mod test_support;

pub use manifest::{
    parse_catalog_str, CatalogValidator, ParsedCatalog, GAME_SCHEMA_JSON, SEED_CATALOG_JSON,
};
pub use model::{
    AcceptedHash, Arch, AssetPattern, GameEntry, GameKind, GameStatus, HashAlgo, LaunchSpec,
    MetadataRefs, ModStrategy, ModsSpec, OnlineMode, OnlineSpec, Os, Project, ReleaseSource,
    Releases, Slug, UserFiles,
};
pub use provider::{CatalogError, CatalogProvider, ProviderInfo};

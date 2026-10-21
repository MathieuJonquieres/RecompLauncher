//! Strongly typed game manifest model (spec §4.1).
//!
//! Unknown JSON fields are ignored for forward compatibility. Enums reject
//! unknown values; newtypes validate slugs, hashes and relative paths.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// Maximum length of a catalog slug.
pub const MAX_SLUG_LEN: usize = 80;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ManifestError {
    #[error("unsupported manifest schema {0}")]
    UnsupportedSchema(u32),

    #[error("invalid slug: {0}")]
    InvalidSlug(String),

    #[error("invalid field `{field}`: {reason}")]
    InvalidField { field: String, reason: String },

    #[error("invalid project URL: {0}")]
    InvalidUrl(String),

    #[error("invalid hash `{value}` for {algo}")]
    InvalidHash { algo: String, value: String },

    #[error("invalid relative path: {0}")]
    InvalidPath(String),
}

impl ManifestError {
    fn field(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

/// Validated catalog identifier (`[a-z0-9][a-z0-9-]*`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    pub fn new(value: impl Into<String>) -> Result<Self, ManifestError> {
        let value = value.into();
        let mut chars = value.chars();
        let valid_start = chars
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit());
        let valid_rest = value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-');
        if !valid_start
            || !valid_rest
            || value.len() > MAX_SLUG_LEN
            || value.ends_with('-')
            || value.contains("--")
        {
            return Err(ManifestError::InvalidSlug(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Hash algorithms accepted for user-provided files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum HashAlgo {
    Sha1,
    Sha256,
    Crc32,
}

impl HashAlgo {
    pub const fn expected_hex_len(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
            Self::Crc32 => 8,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Crc32 => "crc32",
        }
    }
}

impl fmt::Display for HashAlgo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Lowercase hexadecimal hash value, length-checked against its algorithm.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashHex(String);

impl HashHex {
    pub fn new(value: impl Into<String>, algo: HashAlgo) -> Result<Self, ManifestError> {
        let value = value.into().to_ascii_lowercase();
        let valid = value.len() == algo.expected_hex_len()
            && value.chars().all(|ch| ch.is_ascii_hexdigit());
        if !valid {
            return Err(ManifestError::InvalidHash {
                algo: algo.to_string(),
                value,
            });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One accepted dump hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedHash {
    pub algo: HashAlgo,
    pub value: HashHex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Origin platform of the original game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GameKind {
    Recomp,
    Decomp,
    #[serde(rename = "native")]
    NativePort,
}

impl GameKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Recomp => "Recomp",
            Self::Decomp => "Décomp",
            Self::NativePort => "Port natif",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GameStatus {
    Playable,
    Beta,
    Wip,
}

impl GameStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Playable => "Jouable",
            Self::Beta => "Bêta",
            Self::Wip => "WIP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Os {
    Windows,
    Linux,
    Macos,
}

impl Os {
    pub const fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "linux")]
        {
            Self::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Self::Linux
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::Macos => "macOS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Arch {
    X86_64,
    Arm64,
    Any,
}

impl Arch {
    pub const fn current() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self::X86_64
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self::Arm64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self::Any
        }
    }

    pub const fn is_compatible(self) -> bool {
        matches!(self, Self::Any)
            || matches!(
                (self, Self::current()),
                (Self::X86_64, Self::X86_64) | (Self::Arm64, Self::Arm64)
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ReleaseSource {
    #[default]
    None,
    Github,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetPattern {
    pub os: Os,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<Arch>,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<HashHex>,
}

impl AssetPattern {
    pub fn arch_matches(&self) -> bool {
        self.arch.map_or(true, |arch| arch.is_compatible())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Releases {
    #[serde(default)]
    pub source: ReleaseSource,
    #[serde(default)]
    pub assets: Vec<AssetPattern>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserFiles {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub accepted_hashes: Vec<AcceptedHash>,
    #[serde(default)]
    pub prompt_ext: Vec<String>,
}

impl UserFiles {
    /// Hash pairs in the form `(algo, hex)` expected by `hash::verify_file`.
    pub fn hash_pairs(&self) -> Vec<(&str, &str)> {
        self.accepted_hashes
            .iter()
            .map(|hash| (hash.algo.as_str(), hash.value.as_str()))
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    #[serde(default)]
    pub exe: BTreeMap<Os, String>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl LaunchSpec {
    pub fn exe_for(&self, os: Os) -> Option<&str> {
        self.exe.get(&os).map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModStrategy {
    #[default]
    NativeDir,
    Args,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModsSpec {
    #[serde(default)]
    pub supported: bool,
    #[serde(default = "default_mods_dir")]
    pub dir_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_hint: Option<String>,
    #[serde(default)]
    pub strategy: ModStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

impl Default for ModsSpec {
    fn default() -> Self {
        Self {
            supported: false,
            dir_name: default_mods_dir(),
            format_hint: None,
            strategy: ModStrategy::NativeDir,
            args_template: None,
            docs_url: None,
        }
    }
}

fn default_mods_dir() -> String {
    "mods".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OnlineMode {
    LocalServer,
    PublicServer,
    P2p,
}

impl OnlineMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LocalServer => "Serveur local",
            Self::PublicServer => "Serveur public",
            Self::P2p => "Pair à pair",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineSpec {
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub modes: Vec<OnlineMode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataRefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wikipedia_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub igdb_id: Option<i64>,
}

/// One complete game manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEntry {
    pub schema: u32,
    pub slug: Slug,
    pub title: String,
    pub original_platform: String,
    pub kind: GameKind,
    pub status: GameStatus,
    #[serde(default)]
    pub description: String,
    pub project: Project,
    #[serde(default)]
    pub releases: Releases,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_files: Option<UserFiles>,
    #[serde(default)]
    pub launch: LaunchSpec,
    #[serde(default)]
    pub mods: ModsSpec,
    #[serde(default)]
    pub online: OnlineSpec,
    #[serde(default)]
    pub metadata: MetadataRefs,
    /// Injected by the provider, persisted in the cache, absent from manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl GameEntry {
    /// Checks invariants that JSON Schema cannot express.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != 1 {
            return Err(ManifestError::UnsupportedSchema(self.schema));
        }
        if self.title.trim().is_empty() {
            return Err(ManifestError::field("title", "must not be empty"));
        }
        if self.project.name.trim().is_empty() {
            return Err(ManifestError::field("project.name", "must not be empty"));
        }
        validate_https_url(&self.project.repo)?;

        if self.releases.source == ReleaseSource::Github && self.releases.assets.is_empty() {
            return Err(ManifestError::field(
                "releases.assets",
                "a GitHub release source needs at least one asset pattern",
            ));
        }
        for (index, asset) in self.releases.assets.iter().enumerate() {
            if asset.pattern.trim().is_empty() {
                return Err(ManifestError::field(
                    format!("releases.assets[{index}].pattern"),
                    "must not be empty",
                ));
            }
        }

        if let Some(user_files) = &self.user_files {
            if user_files.required && user_files.description.trim().is_empty() {
                return Err(ManifestError::field(
                    "user_files.description",
                    "required user files need an explanation",
                ));
            }
            for ext in &user_files.prompt_ext {
                if !ext.starts_with('.') || ext.len() < 2 || ext.contains(['/', '\\', '?']) {
                    return Err(ManifestError::field(
                        "user_files.prompt_ext",
                        format!("`{ext}` is not a file extension"),
                    ));
                }
            }
        }

        if self.launch.exe.is_empty() {
            return Err(ManifestError::field(
                "launch.exe",
                "at least one platform executable is required",
            ));
        }
        for (os, exe) in &self.launch.exe {
            validate_relative_path(exe).map_err(|reason| {
                ManifestError::field("launch.exe", format!("{}: {reason}", os.label()))
            })?;
        }

        if self.mods.supported && self.mods.strategy == ModStrategy::Args {
            let template = self.mods.args_template.as_deref().unwrap_or_default();
            if !template.contains("{mod}") {
                return Err(ManifestError::field(
                    "mods.args_template",
                    "must contain the {mod} placeholder for the `args` strategy",
                ));
            }
        }
        validate_relative_path(&self.mods.dir_name)
            .map_err(|reason| ManifestError::field("mods.dir_name", reason))?;

        if self.online.supported && self.online.modes.is_empty() {
            return Err(ManifestError::field(
                "online.modes",
                "online support is declared without any mode",
            ));
        }

        Ok(())
    }

    pub fn supports_os(&self, os: Os) -> bool {
        self.releases.assets.iter().any(|asset| asset.os == os) && self.launch.exe_for(os).is_some()
    }

    pub fn incompatible_reason(&self, os: Os) -> Option<&'static str> {
        if self.launch.exe_for(os).is_none() {
            return Some("Aucun exécutable publié pour ce système.");
        }
        if !self.releases.assets.iter().any(|asset| asset.os == os) {
            return Some("Aucun binaire publié pour ce système.");
        }
        None
    }

    pub fn best_asset(&self, os: Os) -> Option<&AssetPattern> {
        self.releases
            .assets
            .iter()
            .find(|asset| asset.os == os && asset.arch_matches())
    }
}

fn validate_https_url(value: &str) -> Result<(), ManifestError> {
    let url = Url::parse(value).map_err(|_| ManifestError::InvalidUrl(value.to_string()))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(ManifestError::InvalidUrl(value.to_string()));
    }
    Ok(())
}

/// Rejects absolute paths, drive prefixes, `..`, and empty components.
pub fn validate_relative_path(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("path must not be empty".to_string());
    }
    if value.contains('\0') {
        return Err("path contains a NUL byte".to_string());
    }
    let normalised = value.replace('\\', "/");
    if normalised.starts_with('/') || normalised.starts_with("//") {
        return Err("absolute paths are not allowed".to_string());
    }
    if normalised.as_bytes().get(1) == Some(&b':') {
        return Err("drive prefixes are not allowed".to_string());
    }
    for component in normalised.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(format!("invalid path component `{component}`"));
        }
        let stem = component.split('.').next().unwrap_or(component);
        let upper = stem.to_ascii_uppercase();
        let reserved = matches!(
            upper.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        );
        if reserved {
            return Err(format!("reserved Windows name `{component}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::minimal_entry;

    #[test]
    fn slug_accepts_valid_values() {
        assert!(Slug::new("zelda64-recomp").is_ok());
        assert!(Slug::new("a").is_ok());
        assert!(Slug::new("game-2").is_ok());
    }

    #[test]
    fn slug_rejects_invalid_values() {
        for value in ["", "Zelda", "-zelda", "zelda-", "a--b", "a/b", "é"] {
            assert!(Slug::new(value).is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn hash_lengths_are_enforced() {
        assert!(HashHex::new("a".repeat(40), HashAlgo::Sha1).is_ok());
        assert!(HashHex::new("a".repeat(39), HashAlgo::Sha1).is_err());
        assert!(HashHex::new("z".repeat(40), HashAlgo::Sha1).is_err());
        assert!(HashHex::new("a".repeat(8), HashAlgo::Crc32).is_ok());
    }

    #[test]
    fn minimal_entry_is_valid() {
        let entry = minimal_entry("test-game");
        entry.validate().expect("valid entry");
        assert!(entry.supports_os(Os::Windows));
        assert!(!entry.supports_os(Os::Linux));
        assert!(entry.incompatible_reason(Os::Linux).is_some());
    }

    #[test]
    fn path_validation_rejects_escapes() {
        for value in ["../evil", "/etc/passwd", "C:\\Windows", "a/../../b", "CON"] {
            assert!(validate_relative_path(value).is_err(), "{value}");
        }
        assert!(validate_relative_path("bin/game.exe").is_ok());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let mut value = serde_json::to_value(minimal_entry("test-game")).expect("json");
        value["future_field"] = serde_json::json!({ "anything": true });
        let entry: GameEntry = serde_json::from_value(value).expect("unknown fields ignored");
        assert_eq!(entry.slug.as_str(), "test-game");
    }
}

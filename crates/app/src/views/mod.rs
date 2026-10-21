//! UI views. Each view is presentation-only: it renders state and sends
//! `Command`s through the backend bridge.

pub mod catalog;
pub mod library;

use std::path::PathBuf;

use recomp_core::library::ModRecord;
use recomp_core::mods::ModsSnapshot;

/// Mods state for one game, shared by the catalog detail and library pages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModsUiState {
    pub supported: bool,
    pub strategy: String,
    pub dir_name: String,
    pub dir: PathBuf,
    pub mods: Vec<ModRecord>,
    pub error: Option<String>,
    pub loading: bool,
}

impl ModsUiState {
    pub fn from_snapshot(snapshot: ModsSnapshot) -> Self {
        Self {
            supported: snapshot.supported,
            strategy: snapshot.strategy.label().to_string(),
            dir_name: snapshot.dir_name,
            dir: snapshot.dir,
            mods: snapshot.mods,
            error: None,
            loading: false,
        }
    }

    pub fn loading() -> Self {
        Self {
            loading: true,
            ..Self::default()
        }
    }
}

/// Download progress displayed by the catalog and library pages.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InstallProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

impl InstallProgress {
    pub fn fraction(self) -> Option<f32> {
        match self.total {
            Some(total) if total > 0 => {
                Some((self.downloaded as f32 / total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

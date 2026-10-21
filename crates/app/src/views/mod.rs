//! UI views. Each view is presentation-only: it renders state and sends
//! `Command`s through the backend bridge.

pub mod catalog;
pub mod library;

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

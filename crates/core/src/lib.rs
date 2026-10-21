//! RecompLauncher business logic.
//!
//! This crate contains no UI dependency. All I/O (network, disk, processes,
//! SQLite) lives here and is exposed through async APIs or channels. The
//! eframe binary in `crates/app` only sends commands and renders events.
//!
//! Threading: functions are `async` unless documented otherwise. SQLite calls
//! are executed through `tokio::task::spawn_blocking`; no lock is held across
//! an `.await`.

#![forbid(unsafe_code)]

pub mod archive;
pub mod catalog;
pub mod ctx;
pub mod db;
pub mod error;
pub mod hash;
pub mod launch;
pub mod library;
pub mod logging;
pub mod mods;
pub mod net;
pub mod paths;
pub mod settings;

pub use ctx::AppContext;
pub use error::{CoreError, Result};
pub use paths::AppPaths;
pub use settings::Settings;

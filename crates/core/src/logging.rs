//! Tracing setup: rotating daily file + console.
//!
//! `init` is called once from `main`; the returned guard must be kept alive for
//! the duration of the process, otherwise buffered log lines are dropped.

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::error::Result;
use crate::paths::AppPaths;

/// Keeps the non-blocking log writer alive.
pub struct LogGuard {
    _guard: WorkerGuard,
}

/// Initialises global tracing subscribers.
pub fn init(paths: &AppPaths) -> Result<LogGuard> {
    paths.ensure_base_dirs()?;

    let file_appender = tracing_appender::rolling::daily(paths.logs_dir(), "launcher.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,recomp_core=debug"));

    let console = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_ansi(true)
        .with_filter(filter.clone());

    let file = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(console)
        .with(file)
        .try_init()
        .map_err(|error| crate::error::CoreError::Config(format!("tracing init: {error}")))?;

    Ok(LogGuard { _guard: guard })
}

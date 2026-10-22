//! RecompLauncher desktop binary.
//!
//! The UI thread never performs I/O. It renders state and sends `Command`s to
//! the backend; backend `Event`s are drained once per frame.

mod app;
mod backend_bridge;
mod ui;
mod views;

use anyhow::Context;
use recomp_core::logging;
use recomp_core::paths::AppPaths;
use recomp_core::AppContext;

fn main() -> anyhow::Result<()> {
    let paths = AppPaths::discover().context("resolving application directories")?;
    let _log_guard = logging::init(&paths).context("initialising logging")?;

    tracing::info!(
        root = %paths.root().display(),
        portable = paths.is_portable(),
        "starting RecompLauncher"
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("recomp-backend")
        .build()
        .context("building tokio runtime")?;
    let context = runtime
        .block_on(AppContext::new(paths))
        .context("initialising application context")?;

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_resizable(true)
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1180.0, 720.0])
            .with_title("RecompLauncher"),
        ..Default::default()
    };

    eframe::run_native(
        "RecompLauncher",
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::LauncherApp::new(
                creation_context,
                context,
                runtime,
            )))
        }),
    )
    .map_err(|error| anyhow::anyhow!("eframe failed: {error}"))
}

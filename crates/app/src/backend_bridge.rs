//! Command/Event bridge between the egui thread and the tokio backend.
//!
//! The UI thread sends `Command`s and drains `Event`s once per frame. The
//! backend owns the runtime and performs every blocking/network operation off
//! the render thread.

use std::sync::Arc;

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use recomp_core::catalog::service::CatalogSnapshot;
use recomp_core::AppContext;

/// Requests sent by the UI.
#[derive(Debug, Clone)]
pub enum Command {
    /// Load the cache immediately, then refresh if stale.
    Bootstrap,
    RefreshCatalog,
    OpenUrl(String),
}

/// Results pushed back to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Catalog(CatalogSnapshot),
    CatalogRefreshing,
    Error(String),
}

/// Owns the backend runtime and channels.
pub struct BackendBridge {
    _runtime: Runtime,
    commands: UnboundedSender<Command>,
    events: UnboundedReceiver<Event>,
}

impl BackendBridge {
    pub fn new(
        runtime: Runtime,
        context: Arc<AppContext>,
        egui_ctx: eframe::egui::Context,
    ) -> Self {
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        runtime.spawn(backend_loop(context, command_rx, event_tx, egui_ctx));
        Self {
            _runtime: runtime,
            commands: command_tx,
            events: event_rx,
        }
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::error!("backend command channel is closed");
        }
    }

    /// Borrowed sender for views that only need to emit commands.
    pub fn sender(&self) -> &UnboundedSender<Command> {
        &self.commands
    }

    /// Drains every pending event without blocking.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}

async fn backend_loop(
    context: Arc<AppContext>,
    mut commands: UnboundedReceiver<Command>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    let emit = |event: Event| {
        if events.send(event).is_err() {
            tracing::debug!("UI event channel closed");
        }
        egui_ctx.request_repaint();
    };

    while let Some(command) = commands.recv().await {
        match command {
            Command::Bootstrap => {
                let cached = context.load_cached_catalog().await;
                let stale = match &cached {
                    Ok(snapshot) => context.catalog().is_stale(snapshot),
                    Err(error) => {
                        emit(Event::Error(format!("Chargement du cache: {error}")));
                        true
                    }
                };
                if let Ok(snapshot) = cached {
                    emit(Event::Catalog(snapshot));
                }
                if stale {
                    emit(Event::CatalogRefreshing);
                    refresh(&context, &emit).await;
                }
            }
            Command::RefreshCatalog => {
                emit(Event::CatalogRefreshing);
                refresh(&context, &emit).await;
            }
            Command::OpenUrl(url) => {
                if !url.starts_with("https://") {
                    emit(Event::Error(format!("URL refusée (HTTPS requis): {url}")));
                    continue;
                }
                let result = tokio::task::spawn_blocking(move || opener::open(url)).await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => emit(Event::Error(format!("Ouverture du lien: {error}"))),
                    Err(error) => emit(Event::Error(format!("Tâche d'ouverture: {error}"))),
                }
            }
        }
    }
}

async fn refresh(context: &Arc<AppContext>, emit: &impl Fn(Event)) {
    match context.refresh_catalog().await {
        Ok(snapshot) => {
            for error in &snapshot.errors {
                tracing::warn!(%error, "catalog provider error");
            }
            emit(Event::Catalog(snapshot));
        }
        Err(error) => emit(Event::Error(format!("Actualisation du catalogue: {error}"))),
    }
}

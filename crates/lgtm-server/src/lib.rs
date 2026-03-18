pub mod routes;
pub mod ws;
pub mod watcher;
pub mod test_helpers;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use lgtm_git::DiffProvider;
use tokio::sync::{RwLock, broadcast};

use lgtm_session::Session;

pub struct AppState {
    pub session: RwLock<Session>,
    pub session_path: PathBuf,
    pub diff_provider: Box<dyn DiffProvider>,
    pub repo_path: PathBuf,
    pub broadcast_tx: broadcast::Sender<ws::WsMessage>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws::ws_handler))
        .nest("/api", routes::api_routes())
        .fallback(routes::assets::serve_asset)
        .with_state(state)
}

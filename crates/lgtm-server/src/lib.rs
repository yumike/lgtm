pub mod routes;
pub mod test_helpers;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use lgtm_git::DiffProvider;
use tokio::sync::RwLock;

use lgtm_session::Session;

pub struct AppState {
    pub session: RwLock<Session>,
    pub session_path: PathBuf,
    pub diff_provider: Box<dyn DiffProvider>,
    pub repo_path: PathBuf,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api", routes::api_routes())
        .fallback(routes::assets::serve_asset)
        .with_state(state)
}

pub mod assets;
pub mod diff;
pub mod files;
pub mod session;
pub mod submit;
pub mod threads;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::AppState;

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/session", get(session::get_session).patch(session::patch_session))
        .route("/diff", get(diff::get_diff))
        .route("/threads", post(threads::create_thread))
        .route("/threads/{id}/comments", post(threads::add_comment))
        .route("/threads/{id}", patch(threads::patch_thread))
        .route("/files", patch(files::patch_file))
        .route("/submit", post(submit::post_submit).get(submit::get_submit))
}

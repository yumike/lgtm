pub mod diff;
pub mod files;
pub mod sessions;
pub mod submit;
pub mod threads;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, patch, post};

use crate::AppState;

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/sessions", post(sessions::create_session).get(sessions::list_sessions))
        .route("/sessions/{id}", get(sessions::get_session).patch(sessions::patch_session).delete(sessions::delete_session))
        .route("/sessions/{id}/diff", get(diff::get_diff))
        .route("/sessions/{id}/threads", post(threads::create_thread))
        .route("/sessions/{id}/threads/{tid}/comments", post(threads::add_comment))
        .route("/sessions/{id}/threads/{tid}", patch(threads::patch_thread))
        .route("/sessions/{id}/files", patch(files::patch_file))
        .route("/sessions/{id}/submit", post(submit::post_submit).get(submit::get_submit))
}

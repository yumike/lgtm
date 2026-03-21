pub mod lockfile;
pub mod routes;
pub mod ws;
pub mod watcher;
pub mod test_helpers;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use lgtm_git::DiffProvider;
use tokio::sync::broadcast;
use ulid::Ulid;

use lgtm_session::SessionStore;

pub struct AppState {
    pub store: Arc<SessionStore>,
    pub diff_providers: std::sync::RwLock<HashMap<Ulid, Box<dyn DiffProvider>>>,
    pub broadcast_channels: std::sync::RwLock<HashMap<Ulid, broadcast::Sender<ws::WsMessage>>>,
    pub submit_pending: std::sync::RwLock<HashMap<Ulid, bool>>,
}

impl AppState {
    pub fn new(store: Arc<SessionStore>) -> Self {
        Self {
            store,
            diff_providers: std::sync::RwLock::new(HashMap::new()),
            broadcast_channels: std::sync::RwLock::new(HashMap::new()),
            submit_pending: std::sync::RwLock::new(HashMap::new()),
        }
    }

    pub fn register_session(&self, id: Ulid, diff_provider: Box<dyn DiffProvider>) {
        let (tx, _) = broadcast::channel(32);
        self.diff_providers.write().unwrap().insert(id, diff_provider);
        self.broadcast_channels.write().unwrap().insert(id, tx);
        self.submit_pending.write().unwrap().insert(id, false);
    }

    pub fn unregister_session(&self, id: Ulid) {
        self.diff_providers.write().unwrap().remove(&id);
        self.broadcast_channels.write().unwrap().remove(&id);
        self.submit_pending.write().unwrap().remove(&id);
    }

    pub fn broadcast(&self, id: Ulid, msg: ws::WsMessage) {
        let channels = self.broadcast_channels.read().unwrap();
        if let Some(tx) = channels.get(&id) {
            let _ = tx.send(msg);
        }
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws/{id}", get(ws::ws_handler))
        .nest("/api", routes::api_routes())
        .with_state(state)
}

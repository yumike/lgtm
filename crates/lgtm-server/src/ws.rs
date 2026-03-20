use std::sync::Arc;

use axum::extract::{State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::Response;
use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum WsMessage {
    SessionUpdated(lgtm_session::Session),
    DiffUpdated(Vec<lgtm_git::DiffFile>),
    SubmitStatus(SubmitStatusData),
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitStatusData {
    pub pending: bool,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<crate::AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<crate::AppState>) {
    let mut rx = state.broadcast_tx.subscribe();

    // Send initial session state
    let session = state.session.read().await.clone();
    let msg = WsMessage::SessionUpdated(session);
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(json.into())).await;
    }

    // Forward broadcast messages to this client
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

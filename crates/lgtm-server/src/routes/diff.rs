use std::sync::Arc;
use axum::extract::{Path, Query, State};
use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;
use crate::AppState;
use crate::routes::sessions::parse_id;

#[derive(Deserialize)]
pub struct DiffQuery {
    pub file: Option<String>,
}

pub async fn get_diff(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Vec<lgtm_git::DiffFile>>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
    let session = state.store.get(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let merge_base = &session.merge_base;
    let head = &session.head;

    let providers = state.diff_providers.read().unwrap();
    let provider = providers.get(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "diff provider not found for session" })),
        )
    })?;

    match &query.file {
        Some(path) => {
            match provider.diff_file(merge_base, head, path) {
                Ok(Some(file)) => Ok(Json(vec![file])),
                Ok(None) => Err((
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "file not found in diff" })),
                )),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )),
            }
        }
        None => {
            match provider.diff_files(merge_base, head) {
                Ok(files) => {
                    let mut result = Vec::new();
                    for mut file in files {
                        if let Ok(Some(detailed)) = provider.diff_file(merge_base, head, &file.path) {
                            file.hunks = detailed.hunks;
                        }
                        result.push(file);
                    }
                    Ok(Json(result))
                }
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )),
            }
        }
    }
}

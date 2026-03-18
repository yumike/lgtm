use std::sync::Arc;
use axum::extract::{Query, State};
use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;
use crate::AppState;

#[derive(Deserialize)]
pub struct DiffQuery {
    pub file: Option<String>,
}

pub async fn get_diff(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Vec<lgtm_git::DiffFile>>, StatusCode> {
    let session = state.session.read().await;
    let merge_base = &session.merge_base;
    let head = &session.head;

    match &query.file {
        Some(path) => {
            match state.diff_provider.diff_file(merge_base, head, path) {
                Ok(Some(file)) => Ok(Json(vec![file])),
                Ok(None) => Err(StatusCode::NOT_FOUND),
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        None => {
            match state.diff_provider.diff_files(merge_base, head) {
                Ok(files) => {
                    let mut result = Vec::new();
                    for mut file in files {
                        if let Ok(Some(detailed)) = state.diff_provider.diff_file(merge_base, head, &file.path) {
                            file.hunks = detailed.hunks;
                        }
                        result.push(file);
                    }
                    Ok(Json(result))
                }
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
    }
}

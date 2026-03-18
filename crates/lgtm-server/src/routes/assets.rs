use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

pub async fn serve_asset(req: Request) -> Response {
    let path = req.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match lgtm_assets::get(path) {
        Some(data) => {
            let mime = lgtm_assets::mime_for(path);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime)],
                data.into_owned(),
            )
                .into_response()
        }
        None => {
            match lgtm_assets::get("index.html") {
                Some(data) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html".to_string())],
                    data.into_owned(),
                )
                    .into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

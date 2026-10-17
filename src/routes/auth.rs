use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;

use super::types::{AppState, ErrorResponse};
use crate::config::Config;

pub fn check_auth(headers: &HeaderMap, config: &Config) -> bool {
    let Some(ref expected_key) = config.server.auth_key else {
        return true;
    };

    if expected_key.is_empty() {
        return true;
    }

    if let Some(auth_val) = headers.get("authorization") {
        if let Ok(auth_str) = auth_val.to_str() {
            let token = auth_str.strip_prefix("Bearer ").unwrap_or(auth_str);
            return token == expected_key;
        }
    }

    false
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (StatusCode, Json<ErrorResponse>)> {
    if !check_auth(&headers, &state.config) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "unauthorized".to_string(),
            }),
        ));
    }
    Ok(next.run(req).await)
}

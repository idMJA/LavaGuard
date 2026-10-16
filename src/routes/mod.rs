use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::cipher::CipherManager;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub cipher: Arc<CipherManager>,
    pub config: Arc<Config>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveUrlRequest {
    pub stream_url: String,
    pub player_url: String,
    pub encrypted_signature: Option<String>,
    pub signature_key: Option<String>,
    pub n_param: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolveUrlResponse {
    pub resolved_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GetStsRequest {
    pub player_url: String,
}

#[derive(Debug, Deserialize)]
pub struct DecryptSignatureRequest {
    pub player_url: String,
    pub encrypted_signature: Option<String>,
    pub n_param: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DecryptSignatureResponse {
    pub decrypted_signature: Option<String>,
    pub decrypted_n_sig: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetStsResponse {
    pub sts: String,
}

#[derive(Debug, Deserialize)]
pub struct GeneratePotRequest {
    pub content_binding: Option<String>,
    #[serde(rename = "coldToken")]
    pub cold_token: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct GeneratePotResponse {
    #[serde(rename = "poToken")]
    pub po_token: String,
    #[serde(rename = "contentBinding")]
    pub content_binding: String,
    #[serde(rename = "coldStartToken", skip_serializing_if = "Option::is_none")]
    pub cold_start_token: Option<String>,
    pub ttl: u64,
}

#[derive(Debug, Deserialize)]
pub struct DecodeColdStartRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

fn check_auth(headers: &HeaderMap, config: &Config) -> bool {
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

async fn auth_middleware(
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

async fn handle_resolve_url(
    State(state): State<AppState>,
    Json(payload): Json<ResolveUrlRequest>,
) -> Result<Json<ResolveUrlResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .cipher
        .resolve_stream_url(
            &payload.player_url,
            &payload.stream_url,
            payload.encrypted_signature,
            payload.signature_key,
            payload.n_param,
        )
        .await
    {
        Ok((resolved, mode)) => {
            tracing::info!(mode, player_url = %payload.player_url, "resolved stream url");
            Ok(Json(ResolveUrlResponse {
                resolved_url: resolved,
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

async fn handle_get_sts(
    State(state): State<AppState>,
    Json(payload): Json<GetStsRequest>,
) -> Result<Json<GetStsResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.cipher.extract_sts(&payload.player_url).await {
        Ok(sts) => Ok(Json(GetStsResponse { sts })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

async fn handle_decrypt_signature(
    State(state): State<AppState>,
    Json(payload): Json<DecryptSignatureRequest>,
) -> Result<Json<DecryptSignatureResponse>, (StatusCode, Json<ErrorResponse>)> {
    if payload.encrypted_signature.is_none() && payload.n_param.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "either encrypted_signature or n_param must be provided".to_string(),
            }),
        ));
    }

    match state
        .cipher
        .solve_cipher(
            &payload.player_url,
            payload.n_param,
            payload.encrypted_signature,
        )
        .await
    {
        Ok(res) => Ok(Json(DecryptSignatureResponse {
            decrypted_signature: res.sig,
            decrypted_n_sig: res.n,
        })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

async fn handle_generate(
    State(state): State<AppState>,
    payload: Option<Json<GeneratePotRequest>>,
) -> Result<Json<GeneratePotResponse>, (StatusCode, Json<ErrorResponse>)> {
    let binding = payload
        .as_ref()
        .and_then(|Json(p)| p.content_binding.clone())
        .unwrap_or_else(|| "default".to_string());
    let cold_token = payload
        .as_ref()
        .and_then(|Json(p)| p.cold_token)
        .unwrap_or(false);

    match state.cipher.generate_pot(&binding, cold_token).await {
        Ok(res) => Ok(Json(GeneratePotResponse {
            po_token: res.po_token,
            content_binding: res.content_binding,
            cold_start_token: res.cold_start_token,
            ttl: res.ttl,
        })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

async fn handle_decode_cold_start(
    State(state): State<AppState>,
    Json(payload): Json<DecodeColdStartRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    match state.cipher.decode_cold_start(&payload.token).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/resolve_url", post(handle_resolve_url))
        .route("/get_sts", post(handle_get_sts))
        .route("/decrypt_signature", post(handle_decrypt_signature))
        .route("/generate", post(handle_generate))
        .route("/decode_cold_start", post(handle_decode_cold_start))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

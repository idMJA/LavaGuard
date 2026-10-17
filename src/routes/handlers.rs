use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use super::types::{
    ApiResponse, AppState, DecodeColdStartRequest, DecryptSignatureRequest,
    DecryptSignatureResponse, ErrorResponse, GeneratePotRequest, GeneratePotResponse,
    GetStsRequest, GetStsResponse, ResolveUrlRequest, ResolveUrlResponse,
};

pub async fn handle_resolve_url(
    State(state): State<AppState>,
    Json(payload): Json<ResolveUrlRequest>,
) -> ApiResponse<ResolveUrlResponse> {
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

pub async fn handle_get_sts(
    State(state): State<AppState>,
    Json(payload): Json<GetStsRequest>,
) -> ApiResponse<GetStsResponse> {
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

pub async fn handle_decrypt_signature(
    State(state): State<AppState>,
    Json(payload): Json<DecryptSignatureRequest>,
) -> ApiResponse<DecryptSignatureResponse> {
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

pub async fn handle_generate(
    State(state): State<AppState>,
    payload: Option<Json<GeneratePotRequest>>,
) -> ApiResponse<GeneratePotResponse> {
    let binding = payload
        .as_ref()
        .and_then(|Json(p)| p.content_binding.as_deref())
        .filter(|b| !b.trim().is_empty());
    let cold_token = payload
        .as_ref()
        .and_then(|Json(p)| p.cold_token)
        .unwrap_or(false);

    match state.cipher.generate_pot(binding, cold_token).await {
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

pub async fn handle_decode_cold_start(
    State(state): State<AppState>,
    Json(payload): Json<DecodeColdStartRequest>,
) -> ApiResponse<serde_json::Value> {
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

use axum::Json;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize)]
pub struct GetStsResponse {
    pub sts: String,
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

pub type ApiResponse<T> = Result<Json<T>, (axum::http::StatusCode, Json<ErrorResponse>)>;

#[derive(Clone)]
pub struct AppState {
    pub cipher: std::sync::Arc<crate::cipher::CipherManager>,
    pub config: std::sync::Arc<crate::config::Config>,
}

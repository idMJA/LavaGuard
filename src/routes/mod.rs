mod auth;
mod handlers;
pub mod types;

pub use types::AppState;

use axum::routing::post;
use axum::Router;

use auth::auth_middleware;
use handlers::{
    handle_decode_cold_start, handle_decrypt_signature, handle_generate, handle_get_sts,
    handle_resolve_url,
};

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

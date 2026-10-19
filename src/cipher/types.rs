use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;

#[derive(Debug, Serialize, Deserialize)]
pub struct SolveResult {
    pub mode: String,
    pub n: Option<String>,
    pub sig: Option<String>,
    pub resolved_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotguardResult {
    #[serde(rename = "poToken")]
    pub po_token: String,
    #[serde(rename = "contentBinding")]
    pub content_binding: String,
    #[serde(rename = "coldStartToken", skip_serializing_if = "Option::is_none")]
    pub cold_start_token: Option<String>,
    pub ttl: u64,
}

#[derive(Debug)]
pub enum WorkerMessage {
    Cipher {
        player_code: Arc<String>,
        n_param: Option<String>,
        sig: Option<String>,
        respond_to: oneshot::Sender<anyhow::Result<SolveResult>>,
    },
    Eval {
        player_code: String,
        stream_url: String,
        respond_to: oneshot::Sender<anyhow::Result<String>>,
    },
    Botguard {
        content_binding: String,
        cold_token: bool,
        respond_to: oneshot::Sender<anyhow::Result<BotguardResult>>,
    },
}

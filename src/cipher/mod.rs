mod bridge;
pub mod cold_start;
pub mod player;
pub mod types;
pub mod url_solver;
mod worker;

pub use types::{BotguardResult, SolveResult};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

use crate::config::Config;
use player::{normalize_player_url, parse_sts_timestamp};
use types::WorkerMessage;
use url_solver::assemble_stream_url;
use worker::start_worker_thread;

#[derive(Clone)]
pub struct CipherManager {
    http_client: reqwest::Client,
    player_cache: Arc<RwLock<HashMap<String, Arc<String>>>>,
    sts_cache: Arc<RwLock<HashMap<String, String>>>,
    pot_cache: Arc<RwLock<HashMap<String, (BotguardResult, std::time::Instant)>>>,
    worker_tx: mpsc::Sender<WorkerMessage>,
    config: Arc<Config>,
}

impl CipherManager {
    /// Creates a new `CipherManager` instance and spawns background JS worker threads.
    ///
    /// # Errors
    ///
    /// Returns an error if the `QuickJS` isolate thread runtime initialization fails.
    pub fn new(config: Arc<Config>) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel::<WorkerMessage>(64);
        start_worker_thread(rx)?;

        Ok(Self {
            http_client: reqwest::Client::builder().build()?,
            player_cache: Arc::new(RwLock::new(HashMap::new())),
            sts_cache: Arc::new(RwLock::new(HashMap::new())),
            pot_cache: Arc::new(RwLock::new(HashMap::new())),
            worker_tx: tx,
            config,
        })
    }

    #[must_use]
    pub fn normalize_player_url(&self, raw: &str) -> String {
        normalize_player_url(raw, &self.config)
    }

    async fn get_player(&self, player_url: &str) -> anyhow::Result<Arc<String>> {
        let normalized = self.normalize_player_url(player_url);

        {
            let cache = self.player_cache.read().await;
            if let Some(cached) = cache.get(&normalized) {
                return Ok(Arc::clone(cached));
            }
        }

        let resp = self
            .http_client
            .get(&normalized)
            .header(
                "user-agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            )
            .send()
            .await?
            .error_for_status()?;

        let body = Arc::new(resp.text().await?);
        self.player_cache
            .write()
            .await
            .insert(normalized, Arc::clone(&body));
        Ok(body)
    }

    /// Extracts the STS signature timestamp from a remote player script URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP fetch fails or no STS timestamp is present.
    pub async fn extract_sts(&self, player_url: &str) -> anyhow::Result<String> {
        let normalized = self.normalize_player_url(player_url);

        {
            let cache = self.sts_cache.read().await;
            if let Some(sts) = cache.get(&normalized) {
                return Ok(sts.clone());
            }
        }

        let player = self.get_player(&normalized).await?;
        let sts = parse_sts_timestamp(&player)?;

        self.sts_cache.write().await.insert(normalized, sts.clone());
        Ok(sts)
    }

    /// Solves `YouTube` cipher signatures and n-parameters via EJS or isolate fallback.
    ///
    /// # Errors
    ///
    /// Returns an error if the player script fetch fails or evaluation returns an exception.
    pub async fn solve_cipher(
        &self,
        player_url: &str,
        n_param: Option<String>,
        sig: Option<String>,
    ) -> anyhow::Result<SolveResult> {
        let player_code = self.get_player(player_url).await?;
        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::Cipher {
                player_code,
                n_param,
                sig,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))?
    }

    async fn solve_eval(&self, player_url: &str, stream_url: &str) -> anyhow::Result<String> {
        let player_code = self.get_player(player_url).await?;
        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::Eval {
                player_code: (*player_code).clone(),
                stream_url: stream_url.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))?
    }

    /// Fetches fresh `visitorData` directly from `YouTube` `InnerTube`.
    ///
    /// # Errors
    ///
    /// Returns an error if the network request fails or response does not contain `visitorData`.
    pub async fn fetch_visitor_data(&self) -> anyhow::Result<String> {
        let resp = self
            .http_client
            .post("https://www.youtube.com/youtubei/v1/visitor_id")
            .header("content-type", "application/json")
            .header(
                "user-agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
            )
            .json(&serde_json::json!({
                "context": {
                    "client": {
                        "clientName": "WEB",
                        "clientVersion": "2.20260227.01.00"
                    }
                }
            }))
            .send()
            .await?
            .error_for_status()?;

        let json: serde_json::Value = resp.json().await?;
        let visitor = json
            .pointer("/responseContext/visitorData")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("visitorData not found in response"))?;

        Ok(visitor.to_string())
    }

    /// Generates or retrieves a cached `BotGuard` Proof of Origin Token (PO Token).
    ///
    /// # Errors
    ///
    /// Returns an error if the `BotGuard` isolate runner fails or `visitorData` fetch fails.
    pub async fn generate_pot(
        &self,
        content_binding: Option<&str>,
        cold_token: bool,
    ) -> anyhow::Result<BotguardResult> {
        let binding = match content_binding {
            Some(b) if !b.is_empty() => b.to_string(),
            _ => self.fetch_visitor_data().await?,
        };

        if !cold_token {
            let cache = self.pot_cache.read().await;
            if let Some((cached, instant)) = cache.get(&binding) {
                if instant.elapsed().as_secs() < cached.ttl.saturating_sub(60) {
                    return Ok(cached.clone());
                }
            }
        }

        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::Botguard {
                content_binding: binding.clone(),
                cold_token,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        let res = rx
            .await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))??;

        if !cold_token {
            self.pot_cache
                .write()
                .await
                .insert(binding, (res.clone(), std::time::Instant::now()));
        }

        Ok(res)
    }

    /// Decodes a client cold-start token payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the token cannot be decoded or deserialized.
    pub fn decode_cold_start(&self, token: &str) -> anyhow::Result<serde_json::Value> {
        let decoded = cold_start::decode_cold_start_token(token)?;
        Ok(serde_json::to_value(decoded)?)
    }

    /// Resolves an entire streaming URL by executing cipher transformations.
    ///
    /// # Errors
    ///
    /// Returns an error if deciphering fails or URL re-assembly fails.
    pub async fn resolve_stream_url(
        &self,
        player_url: &str,
        stream_url: &str,
        enc_sig: Option<String>,
        sig_key: Option<String>,
        n_param: Option<String>,
    ) -> anyhow::Result<(String, &'static str)> {
        let parsed_url = url::Url::parse(stream_url)?;

        let target_n = n_param.or_else(|| {
            parsed_url
                .query_pairs()
                .find(|(k, _)| k == "n")
                .map(|(_, v)| v.into_owned())
        });

        let target_sig = enc_sig.or_else(|| {
            parsed_url
                .query_pairs()
                .find(|(k, _)| k == "s")
                .map(|(_, v)| v.into_owned())
        });

        let solved = self.solve_cipher(player_url, target_n, target_sig).await?;

        if solved.mode == "ejs" {
            let assembled = assemble_stream_url(stream_url, sig_key.as_deref(), &solved)?;
            return Ok((assembled, "ejs"));
        }

        let fallback_url = self.solve_eval(player_url, stream_url).await?;
        Ok((fallback_url, "eval"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_player_url_defaults() {
        let config = Arc::new(crate::config::Config::default());
        let mgr = CipherManager {
            http_client: reqwest::Client::new(),
            player_cache: Arc::new(RwLock::new(HashMap::new())),
            sts_cache: Arc::new(RwLock::new(HashMap::new())),
            pot_cache: Arc::new(RwLock::new(HashMap::new())),
            worker_tx: mpsc::channel(1).0,
            config,
        };

        assert_eq!(
            mgr.normalize_player_url(
                "https://www.youtube.com/s/player/2574220e/player_ias.vflset/en_US/base.js"
            ),
            "https://www.youtube.com/s/player/2574220e/player_ias.vflset/en_US/base.js"
        );

        assert_eq!(
            mgr.normalize_player_url("/s/player/2574220e/player_ias.vflset/en_US/base.js"),
            "https://www.youtube.com/s/player/2574220e/player_ias.vflset/en_US/base.js"
        );
    }

    #[test]
    fn test_normalize_player_url_overrides() {
        let mut cfg = crate::config::Config::default();
        cfg.youtube.override_player_id = Some("override123".to_string());
        cfg.youtube.override_player_variant = Some("IAS".to_string());

        let mgr = CipherManager {
            http_client: reqwest::Client::new(),
            player_cache: Arc::new(RwLock::new(HashMap::new())),
            sts_cache: Arc::new(RwLock::new(HashMap::new())),
            pot_cache: Arc::new(RwLock::new(HashMap::new())),
            worker_tx: mpsc::channel(1).0,
            config: Arc::new(cfg),
        };

        assert_eq!(
            mgr.normalize_player_url(
                "https://www.youtube.com/s/player/original/player_tcl.vflset/en_US/base.js"
            ),
            "https://www.youtube.com/s/player/override123/player_ias.vflset/en_US/base.js"
        );
    }
}

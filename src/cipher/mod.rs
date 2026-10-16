use rquickjs::{Context, Function, Object, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, oneshot, RwLock};

static PLAYER_ID_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static PLAYER_VARIANT_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static STS_REGEX: OnceLock<regex::Regex> = OnceLock::new();

const SOLVER_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/cipher.bundle.js"));

#[derive(Debug, Serialize, Deserialize)]
pub struct SolveResult {
    pub mode: String,
    pub n: Option<String>,
    pub sig: Option<String>,
    pub resolved_url: Option<String>,
}

#[allow(clippy::enum_variant_names)]
enum WorkerMessage {
    SolveCipher {
        player_code: Arc<String>,
        n_param: Option<String>,
        sig: Option<String>,
        respond_to: oneshot::Sender<anyhow::Result<SolveResult>>,
    },
    SolveEval {
        player_code: String,
        stream_url: String,
        respond_to: oneshot::Sender<anyhow::Result<String>>,
    },
    SolveBotguard {
        content_binding: String,
        cold_token: bool,
        respond_to: oneshot::Sender<anyhow::Result<BotguardResult>>,
    },
    DecodeColdStart {
        token: String,
        respond_to: oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
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

#[derive(Clone)]
pub struct CipherManager {
    http_client: reqwest::Client,
    player_cache: Arc<RwLock<HashMap<String, Arc<String>>>>,
    sts_cache: Arc<RwLock<HashMap<String, String>>>,
    pot_cache: Arc<RwLock<HashMap<String, (BotguardResult, std::time::Instant)>>>,
    worker_tx: mpsc::Sender<WorkerMessage>,
    config: Arc<crate::config::Config>,
}

impl CipherManager {
    pub fn new(config: Arc<crate::config::Config>) -> anyhow::Result<Self> {
        let (tx, mut rx) = mpsc::channel::<WorkerMessage>(64);

        std::thread::Builder::new()
            .name("quickjs-worker".into())
            .spawn(move || {
                let runtime = match Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        tracing::error!("failed creating quickjs runtime: {e}");
                        return;
                    }
                };

                let context = match Context::full(&runtime) {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        tracing::error!("failed creating quickjs context: {e}");
                        return;
                    }
                };

                let eval_res = context.with(|ctx| {
                    if let Err(e) = ctx.eval::<(), _>(
                        r#"
                        if (typeof globalThis.structuredClone !== 'function') {
                            globalThis.structuredClone = function(obj) {
                                return JSON.parse(JSON.stringify(obj));
                            };
                        }
                        if (typeof globalThis.setTimeout !== 'function') {
                            globalThis.setTimeout = function(fn, ms) {
                                return 1;
                            };
                            globalThis.clearTimeout = function(id) {};
                        }
                    "#,
                    ) {
                        return Err(format!("failed installing polyfills: {e}"));
                    }

                    let globals = ctx.globals();
                    let fetch_ctx = ctx.clone();
                    let native_fetch = Function::new(
                        ctx.clone(),
                        move |url: String, opts: Option<Object>| -> rquickjs::Result<Object> {
                            let mut method = "GET".to_string();
                            let mut req_body: Option<String> = None;
                            let mut req_headers: Vec<(String, String)> = Vec::new();

                            if let Some(o) = opts {
                                if let Ok(m) = o.get::<_, String>("method") {
                                    method = m.to_uppercase();
                                }
                                if let Ok(b) = o.get::<_, String>("body") {
                                    req_body = Some(b);
                                }
                                if let Ok(h) = o.get::<_, Object>("headers") {
                                    for k in h.keys::<String>().flatten() {
                                        if let Ok(v) = h.get::<_, String>(&k) {
                                            req_headers.push((k, v));
                                        }
                                    }
                                }
                            }

                            let mut request = match method.as_str() {
                                "POST" => ureq::post(&url),
                                "PUT" => ureq::put(&url),
                                "HEAD" => ureq::head(&url),
                                _ => ureq::get(&url),
                            };

                            for (k, v) in req_headers {
                                request = request.set(&k, &v);
                            }

                            let resp_result = if let Some(b) = req_body {
                                request.send_string(&b)
                            } else {
                                request.call()
                            };

                            let (status, status_text, body_text) = match resp_result {
                                Ok(resp) => {
                                    let status = resp.status();
                                    let status_text = resp.status_text().to_string();
                                    let text = resp.into_string().unwrap_or_default();
                                    (status, status_text, text)
                                }
                                Err(ureq::Error::Status(code, resp)) => {
                                    let text = resp.into_string().unwrap_or_default();
                                    (code, format!("HTTP Error {code}"), text)
                                }
                                Err(_) => {
                                    return Err(rquickjs::Error::Exception);
                                }
                            };

                            let resp_obj = Object::new(fetch_ctx.clone())?;
                            resp_obj.set("status", status)?;
                            resp_obj.set("statusText", status_text)?;
                            resp_obj.set("ok", (200..300).contains(&status))?;
                            resp_obj.set("bodyText", body_text)?;

                            Ok(resp_obj)
                        },
                    )
                    .map_err(|e| format!("failed creating native fetch function: {e}"))?;
                    globals
                        .set("__native_fetch_sync", native_fetch)
                        .map_err(|e| format!("failed setting native fetch global: {e}"))?;

                    if let Err(e) = ctx.eval::<(), _>(
                        r#"
                        globalThis.fetch = async function(url, options) {
                            const res = globalThis.__native_fetch_sync(String(url), options || {});
                            return {
                                status: res.status,
                                statusText: res.statusText,
                                ok: res.ok,
                                text: async () => res.bodyText,
                                json: async () => JSON.parse(res.bodyText)
                            };
                        };
                    "#,
                    ) {
                        return Err(format!("failed installing fetch polyfill: {e}"));
                    }

                    if let Err(e) = ctx.eval::<(), _>(SOLVER_BUNDLE) {
                        let err_msg = if let Some(exc) = ctx.catch().as_exception() {
                            format!("{e}: {exc}")
                        } else {
                            format!("{e}")
                        };
                        return Err(err_msg);
                    }
                    Ok(())
                });

                if let Err(e) = eval_res {
                    tracing::error!("failed evaluating solver bundle: {e}");
                    return;
                }

                while let Some(msg) = rx.blocking_recv() {
                    match msg {
                        WorkerMessage::SolveCipher {
                            player_code,
                            n_param,
                            sig,
                            respond_to,
                        } => {
                            let res = context.with(|ctx| -> anyhow::Result<SolveResult> {
                                let solve_cipher_fn: Function =
                                    ctx.globals().get("solve_cipher")?;
                                let out: Object =
                                    solve_cipher_fn.call((player_code.as_str(), n_param, sig))?;

                                let mode: String = out.get("mode")?;
                                let n: Option<String> = out.get("n")?;
                                let sig: Option<String> = out.get("sig")?;
                                let resolved_url: Option<String> = out.get("resolved_url")?;

                                Ok(SolveResult {
                                    mode,
                                    n,
                                    sig,
                                    resolved_url,
                                })
                            });

                            let _ = respond_to.send(res);
                        }
                        WorkerMessage::SolveEval {
                            player_code,
                            stream_url,
                            respond_to,
                        } => {
                            let res = context.with(|ctx| -> anyhow::Result<String> {
                                let solve_eval_fn: Function = ctx.globals().get("solve_eval")?;
                                let out: Object = solve_eval_fn.call((player_code, stream_url))?;
                                let resolved: String = out.get("resolved_url")?;
                                Ok(resolved)
                            });

                            let _ = respond_to.send(res);
                        }
                        WorkerMessage::SolveBotguard {
                            content_binding,
                            cold_token,
                            respond_to,
                        } => {
                            let res = context.with(|ctx| -> anyhow::Result<BotguardResult> {
                                let solve_bg_fn: Function = ctx.globals().get("solve_botguard")?;
                                let promise: rquickjs::Promise =
                                    solve_bg_fn.call((content_binding, cold_token))?;
                                let out: Object = promise.finish().map_err(|e| {
                                    if let Some(err) = ctx.catch().as_exception() {
                                        anyhow::anyhow!("QuickJS exception: {:?}", err.message())
                                    } else {
                                        anyhow::anyhow!("QuickJS error: {e}")
                                    }
                                })?;
                                let po_token: String = out.get("poToken")?;
                                let content_binding: String = out.get("contentBinding")?;
                                let cold_start_token: Option<String> = out.get("coldStartToken")?;
                                let ttl: u64 = out.get("ttl")?;
                                Ok(BotguardResult {
                                    po_token,
                                    content_binding,
                                    cold_start_token,
                                    ttl,
                                })
                            });

                            let _ = respond_to.send(res);
                        }
                        WorkerMessage::DecodeColdStart { token, respond_to } => {
                            let res = context.with(|ctx| -> anyhow::Result<serde_json::Value> {
                                let decode_fn: Function = ctx.globals().get("decode_cold_start")?;
                                let out: Object = decode_fn.call((token,))?;
                                let json_str: String = ctx
                                    .globals()
                                    .get::<_, Object>("JSON")?
                                    .get::<_, Function>("stringify")?
                                    .call((out,))?;
                                let val: serde_json::Value = serde_json::from_str(&json_str)?;
                                Ok(val)
                            });

                            let _ = respond_to.send(res);
                        }
                    }
                }
            })?;

        Ok(Self {
            http_client: reqwest::Client::builder().build()?,
            player_cache: Arc::new(RwLock::new(HashMap::new())),
            sts_cache: Arc::new(RwLock::new(HashMap::new())),
            pot_cache: Arc::new(RwLock::new(HashMap::new())),
            worker_tx: tx,
            config,
        })
    }

    pub fn normalize_player_url(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        let base = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else if trimmed.starts_with('/') {
            format!("https://www.youtube.com{trimmed}")
        } else {
            format!("https://www.youtube.com/{trimmed}")
        };

        let mut result = base;

        if let Some(ref override_id) = self.config.youtube.override_player_id {
            if !override_id.is_empty() {
                let re = PLAYER_ID_REGEX
                    .get_or_init(|| regex::Regex::new(r#"/player/[a-zA-Z0-9_-]+/"#).unwrap());
                result = re
                    .replace(&result, format!("/player/{override_id}/"))
                    .into_owned();
            }
        }

        if let Some(ref override_variant) = self.config.youtube.override_player_variant {
            if !override_variant.is_empty() {
                let variant_str = if override_variant.ends_with(".vflset") {
                    override_variant.clone()
                } else {
                    format!("player_{}.vflset", override_variant.to_lowercase())
                };
                let re = PLAYER_VARIANT_REGEX
                    .get_or_init(|| regex::Regex::new(r#"/([a-zA-Z0-9_-]+\.vflset)/"#).unwrap());
                result = re.replace(&result, format!("/{variant_str}/")).into_owned();
            }
        }

        result
    }

    pub async fn get_player(&self, player_url: &str) -> anyhow::Result<Arc<String>> {
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

        let mut cache = self.player_cache.write().await;
        cache.insert(normalized, Arc::clone(&body));
        Ok(body)
    }

    pub async fn extract_sts(&self, player_url: &str) -> anyhow::Result<String> {
        let normalized = self.normalize_player_url(player_url);

        {
            let cache = self.sts_cache.read().await;
            if let Some(sts) = cache.get(&normalized) {
                return Ok(sts.clone());
            }
        }

        let player = self.get_player(&normalized).await?;

        let re = STS_REGEX
            .get_or_init(|| regex::Regex::new(r#"(?:signatureTimestamp|sts):(\d+)"#).unwrap());
        let sts = match re.captures(&player) {
            Some(caps) => caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .ok_or_else(|| anyhow::anyhow!("sts regex group missing"))?,
            None => {
                return Err(anyhow::anyhow!(
                    "could not find sts timestamp in player script"
                ))
            }
        };

        let mut cache = self.sts_cache.write().await;
        cache.insert(normalized, sts.clone());
        Ok(sts)
    }

    pub async fn solve_cipher(
        &self,
        player_url: &str,
        n_param: Option<String>,
        sig: Option<String>,
    ) -> anyhow::Result<SolveResult> {
        let player_code = self.get_player(player_url).await?;
        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::SolveCipher {
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

    pub async fn solve_eval(&self, player_url: &str, stream_url: &str) -> anyhow::Result<String> {
        let player_code = self.get_player(player_url).await?;
        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::SolveEval {
                player_code: (*player_code).clone(),
                stream_url: stream_url.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))?
    }

    pub async fn generate_pot(
        &self,
        content_binding: &str,
        cold_token: bool,
    ) -> anyhow::Result<BotguardResult> {
        if !cold_token {
            let cache = self.pot_cache.read().await;
            if let Some((cached, instant)) = cache.get(content_binding) {
                if instant.elapsed().as_secs() < cached.ttl.saturating_sub(60) {
                    return Ok(cached.clone());
                }
            }
        }

        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::SolveBotguard {
                content_binding: content_binding.to_string(),
                cold_token,
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        let res = rx
            .await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))??;

        if !cold_token {
            let mut cache = self.pot_cache.write().await;
            cache.insert(
                content_binding.to_string(),
                (res.clone(), std::time::Instant::now()),
            );
        }

        Ok(res)
    }

    pub async fn decode_cold_start(&self, token: &str) -> anyhow::Result<serde_json::Value> {
        let (tx, rx) = oneshot::channel();

        self.worker_tx
            .send(WorkerMessage::DecodeColdStart {
                token: token.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("quickjs worker thread is dead"))?;

        rx.await
            .map_err(|_| anyhow::anyhow!("cancelled from quickjs worker"))?
    }

    pub async fn resolve_stream_url(
        &self,
        player_url: &str,
        stream_url: &str,
        enc_sig: Option<String>,
        sig_key: Option<String>,
        n_param: Option<String>,
    ) -> anyhow::Result<(String, &'static str)> {
        let mut parsed_url = url::Url::parse(stream_url)?;

        let pairs: Vec<(String, String)> = parsed_url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        let target_n =
            n_param.or_else(|| pairs.iter().find(|(k, _)| k == "n").map(|(_, v)| v.clone()));

        let target_sig =
            enc_sig.or_else(|| pairs.iter().find(|(k, _)| k == "s").map(|(_, v)| v.clone()));

        let solved = self.solve_cipher(player_url, target_n, target_sig).await?;

        if solved.mode == "ejs" {
            parsed_url.set_query(None);
            let mut query = parsed_url.query_pairs_mut();
            let sig_param_key = sig_key.as_deref().unwrap_or("sig");

            for (k, v) in pairs {
                if k == "s" {
                    continue;
                }
                if k == "n" {
                    if let Some(ref resolved_n) = solved.n {
                        query.append_pair("n", resolved_n);
                        continue;
                    }
                }
                query.append_pair(&k, &v);
            }

            if let Some(ref resolved_sig) = solved.sig {
                query.append_pair(sig_param_key, resolved_sig);
            }

            drop(query);
            return Ok((parsed_url.to_string(), "ejs"));
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
            mgr.normalize_player_url("https://www.youtube.com/s/player/2574220e/player_ias.vflset/en_US/base.js"),
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
            mgr.normalize_player_url("https://www.youtube.com/s/player/original/player_tcl.vflset/en_US/base.js"),
            "https://www.youtube.com/s/player/override123/player_ias.vflset/en_US/base.js"
        );
    }
}

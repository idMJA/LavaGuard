use rquickjs::{Context, Function, Object, Runtime};
use tokio::sync::mpsc;

use super::bridge::setup_polyfills;
use super::types::{BotguardResult, SolveResult, WorkerMessage};

const SOLVER_BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/cipher.bundle.js"));

pub fn start_worker_thread(mut rx: mpsc::Receiver<WorkerMessage>) -> anyhow::Result<()> {
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
                if let Err(e) = setup_polyfills(&ctx) {
                    return Err(format!("failed installing polyfills: {e}"));
                }

                if let Err(e) = ctx.eval::<(), _>(SOLVER_BUNDLE) {
                    let err_msg = ctx
                        .catch()
                        .as_exception()
                        .map_or_else(|| format!("{e}"), |exc| format!("{e}: {exc}"));
                    return Err(err_msg);
                }
                Ok(())
            });

            if let Err(e) = eval_res {
                tracing::error!("failed evaluating solver bundle: {e}");
                return;
            }

            while let Some(msg) = rx.blocking_recv() {
                dispatch_message(&context, msg);
            }
        })?;

    Ok(())
}

fn dispatch_message(context: &Context, msg: WorkerMessage) {
    match msg {
        WorkerMessage::SolveCipher {
            player_code,
            n_param,
            sig,
            respond_to,
        } => {
            let res = context.with(|ctx| -> anyhow::Result<SolveResult> {
                let solve_cipher_fn: Function = ctx.globals().get("solve_cipher")?;
                let out: Object = solve_cipher_fn.call((player_code.as_str(), n_param, sig))?;

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
                let promise: rquickjs::Promise = solve_bg_fn.call((content_binding, cold_token))?;
                let out: Object = promise.finish().map_err(|e| {
                    ctx.catch().as_exception().map_or_else(
                        || anyhow::anyhow!("QuickJS error: {e}"),
                        |err| anyhow::anyhow!("QuickJS exception: {:?}", err.message()),
                    )
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

use rquickjs::{Ctx, Function, Object, Result};

pub fn setup_polyfills(ctx: &Ctx<'_>) -> Result<()> {
    ctx.eval::<(), _>(
        r"
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
    ",
    )?;

    setup_native_fetch(ctx)?;

    Ok(())
}

fn setup_native_fetch(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();
    let fetch_ctx = ctx.clone();

    let native_fetch = Function::new(
        ctx.clone(),
        move |url: String, opts: Option<Object>| -> Result<Object> {
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
    )?;

    globals.set("__native_fetch_sync", native_fetch)?;

    ctx.eval::<(), _>(
        r"
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
    ",
    )?;

    Ok(())
}

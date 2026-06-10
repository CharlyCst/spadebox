use std::collections::HashMap;
use std::sync::Arc;

use boa_engine::{
    Context, JsNativeError, JsResult, JsValue, NativeFunction,
    job::NativeAsyncJob,
    js_string,
    object::{ObjectInitializer, builtins::JsPromise},
    property::{Attribute, PropertyKey},
};

use crate::sandbox::substitute_credentials;
use crate::tools::fetch::{http_client, validate_request};
use crate::{Sandbox, ToolError};

use super::SandboxCaptures;

/// Registers the global `fetch(url, options?)` function.
pub(super) fn register(ctx: &mut Context, sandbox: Arc<Sandbox>) {
    ctx.register_global_builtin_callable(
        js_string!("fetch"),
        2,
        NativeFunction::from_copy_closure_with_captures(fetch_fn, SandboxCaptures { sandbox }),
    )
    .expect("failed to register fetch");
}

fn fetch_fn(
    _this: &JsValue,
    args: &[JsValue],
    captures: &SandboxCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    // --- Parse URL ---
    let url_str = args
        .first()
        .and_then(|v| v.as_string())
        .ok_or_else(|| {
            JsNativeError::typ().with_message("fetch: first argument must be a URL string")
        })?
        .to_std_string_lossy();

    // --- Parse options (method, body, headers) ---
    let method_str;
    let body: Option<String>;
    let headers: HashMap<String, String>;
    if let Some(opts) = args.get(1).and_then(|v| v.as_object()) {
        method_str = opts
            .get(js_string!("method"), ctx)
            .ok()
            .filter(|v| !v.is_undefined())
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_lossy().to_uppercase())
            .unwrap_or_else(|| "GET".to_owned());
        body = opts
            .get(js_string!("body"), ctx)
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_lossy());
        headers = opts
            .get(js_string!("headers"), ctx)
            .ok()
            .filter(|v| !v.is_undefined() && !v.is_null())
            .and_then(|v| v.as_object())
            .map(|obj| {
                let mut map = HashMap::new();
                if let Ok(keys) = obj.own_property_keys(ctx) {
                    for key in keys {
                        if let PropertyKey::String(k) = key {
                            let k_str = k.to_std_string_lossy();
                            if let Ok(val) = obj.get(js_string!(k_str.as_str()), ctx)
                                && let Some(v) = val.as_string()
                            {
                                map.insert(k_str, v.to_std_string_lossy());
                            }
                        }
                    }
                }
                map
            })
            .unwrap_or_default();
    } else {
        method_str = "GET".to_owned();
        body = None;
        headers = HashMap::new();
    }

    // --- Security check (synchronous, before enqueuing any async work) ---
    let sandbox = Arc::clone(&captures.sandbox);

    let (validated_url, user_agent) =
        validate_request(&sandbox, &url_str, &method_str).map_err(|e| match e {
            ToolError::PermissionDenied(msg) | ToolError::InvalidUrl(msg) => {
                JsNativeError::error().with_message(format!("fetch: {msg}"))
            }
            e => JsNativeError::error().with_message(format!("fetch: {e}")),
        })?;

    let (url, body, headers) = substitute_credentials(&sandbox, validated_url, body, headers);

    // --- Start the request, create a pending promise, enqueue an async job ---

    // Spawning on the SpadeBox runtime starts the request immediately on the
    // runtime's worker threads: multiple in-flight fetches run concurrently,
    // and progress even while the engine thread is evaluating code.
    let request =
        crate::runtime::handle().spawn(do_request(url, method_str, body, user_agent, headers));

    let (promise, resolvers) = JsPromise::new_pending(ctx);
    let resolve = resolvers.resolve;
    let reject = resolvers.reject;

    ctx.enqueue_job(
        NativeAsyncJob::with_realm(
            async move |ctx: &std::cell::RefCell<&mut Context>| {
                let result = match request.await {
                    Ok(result) => result,
                    Err(join_err) => Err(join_err.to_string()),
                };
                let ctx = &mut ctx.borrow_mut();
                match result {
                    Ok((status, body_text)) => {
                        let response = build_response(status, body_text, ctx);
                        resolve.call(&JsValue::undefined(), &[JsValue::from(response)], ctx)?;
                    }
                    Err(e) => {
                        let err = JsNativeError::error()
                            .with_message(format!("fetch: {e}"))
                            .to_opaque(ctx);
                        reject.call(&JsValue::undefined(), &[JsValue::from(err)], ctx)?;
                    }
                }
                Ok(JsValue::undefined())
            },
            ctx.realm().clone(),
        )
        .into(),
    );

    Ok(JsValue::from(promise))
}

// ---------------------------------------------------------------------------
// HTTP execution
// ---------------------------------------------------------------------------

/// Performs the HTTP request. Must run on the SpadeBox runtime.
async fn do_request(
    url: reqwest::Url,
    method: String,
    body: Option<String>,
    user_agent: String,
    headers: HashMap<String, String>,
) -> Result<(u16, String), String> {
    let method_parsed =
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = http_client()
        .request(method_parsed, url)
        .header(reqwest::header::USER_AGENT, &user_agent);
    if let Some(b) = body {
        req = req.body(b);
    }
    for (key, value) in &headers {
        req = req.header(key.as_str(), value.as_str());
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok((status, text))
}

// ---------------------------------------------------------------------------
// Response object
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ResponseCaptures {
    body: String,
}

impl boa_engine::gc::Finalize for ResponseCaptures {}

// SAFETY: `String` holds no GC-managed objects; nothing to trace.
unsafe impl boa_engine::gc::Trace for ResponseCaptures {
    boa_engine::gc::empty_trace!();
}

fn text_method(
    _this: &JsValue,
    _args: &[JsValue],
    captures: &ResponseCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let body = js_string!(captures.body.as_str());
    Ok(JsValue::from(JsPromise::resolve(body, ctx)))
}

fn json_method(
    _this: &JsValue,
    _args: &[JsValue],
    captures: &ResponseCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    // Delegate to JS's built-in JSON.parse — no serde_json interop needed.
    let body_js = JsValue::from(js_string!(captures.body.as_str()));
    let json_global = ctx.global_object().get(js_string!("JSON"), ctx)?;
    let parsed = json_global
        .as_object()
        .ok_or_else(|| JsNativeError::error().with_message("JSON not found"))?
        .get(js_string!("parse"), ctx)?
        .as_callable()
        .ok_or_else(|| JsNativeError::error().with_message("JSON.parse not callable"))?
        .call(&JsValue::undefined(), &[body_js], ctx)?;
    Ok(JsValue::from(JsPromise::resolve(parsed, ctx)))
}

fn build_response(status: u16, body: String, ctx: &mut Context) -> boa_engine::JsObject {
    let ok = (200u16..300).contains(&status);
    ObjectInitializer::new(ctx)
        .property(js_string!("status"), f64::from(status), Attribute::all())
        .property(js_string!("ok"), ok, Attribute::all())
        .function(
            NativeFunction::from_copy_closure_with_captures(
                text_method,
                ResponseCaptures { body: body.clone() },
            ),
            js_string!("text"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(json_method, ResponseCaptures { body }),
            js_string!("json"),
            0,
        )
        .build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        Sandbox,
        js_runtime::JsContext,
        sandbox::{DomainRule, HttpVerb},
    };

    fn sandbox_with_http() -> Arc<Sandbox> {
        let s = Arc::new(Sandbox::new());
        s.enable_http()
            .allow(DomainRule::new("*", vec![HttpVerb::Get, HttpVerb::Post]).unwrap());
        s
    }

    #[test]
    fn fetch_permissions() {
        // HTTP not enabled — fetch throws synchronously.
        let sandbox = Arc::new(Sandbox::new());
        let mut ctx = JsContext::new(sandbox);
        assert!(
            ctx.eval(r#"fetch("https://example.com")"#).is_err(),
            "should throw when HTTP disabled"
        );

        // Invalid URL.
        let mut ctx = JsContext::new(sandbox_with_http());
        assert!(
            ctx.eval(r#"fetch("not a url")"#).is_err(),
            "should throw on invalid URL"
        );

        // Unsupported scheme.
        assert!(
            ctx.eval(r#"fetch("file:///etc/passwd")"#).is_err(),
            "should throw on non-http scheme"
        );

        // Domain not matched by any rule.
        let sandbox = Arc::new(Sandbox::new());
        sandbox
            .enable_http()
            .allow(DomainRule::new("allowed.com", vec![HttpVerb::Get]).unwrap());
        let mut ctx = JsContext::new(sandbox);
        assert!(
            ctx.eval(r#"fetch("https://blocked.com")"#).is_err(),
            "should throw for blocked domain"
        );

        // Verb not allowed for domain.
        assert!(
            ctx.eval(r#"fetch("https://allowed.com", { method: "POST" })"#)
                .is_err(),
            "should throw for disallowed verb"
        );
    }

    /// End-to-end check of the async fetch plumbing: requests are spawned on
    /// the SpadeBox runtime, the promise resolves through the job executor,
    /// and concurrent fetches both complete within a single eval.
    #[test]
    fn fetch_resolves_through_async_executor() {
        use std::io::{Read, Write};

        // Minimal HTTP server answering two requests with "hello".
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\nconnection: close\r\n\r\nhello",
                    )
                    .unwrap();
            }
        });

        let mut ctx = JsContext::new(sandbox_with_http());
        ctx.eval(&format!(
            r#"
            let a, b;
            fetch("http://{addr}/").then(r => r.text()).then(t => {{ a = t; }});
            fetch("http://{addr}/").then(r => r.text()).then(t => {{ b = t; }});
            "#
        ))
        .unwrap();
        // eval drains the job queue, so both promises have settled by now.
        assert_eq!(ctx.eval("a + b").unwrap().value, "\"hellohello\"");
        server.join().unwrap();
    }
}

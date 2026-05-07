use std::sync::Arc;

use boa_engine::{
    js_string,
    job::{Job, PromiseJob},
    object::{builtins::JsPromise, ObjectInitializer},
    property::Attribute,
    Context, JsNativeError, JsResult, JsValue, NativeFunction,
};
use reqwest::Client;

use crate::{Sandbox, ToolError};
use crate::tools::fetch::validate_request;

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

    // --- Parse options (method, body) ---
    let method_str;
    let body: Option<String>;
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
    } else {
        method_str = "GET".to_owned();
        body = None;
    }

    // --- Security check (synchronous, before enqueuing any async work) ---
    let sandbox = Arc::clone(&captures.sandbox);

    let (url, user_agent) = validate_request(&sandbox, &url_str, &method_str)
        .map_err(|e| match e {
            ToolError::PermissionDenied(msg) | ToolError::InvalidUrl(msg) => {
                JsNativeError::error().with_message(format!("fetch: {msg}"))
            }
            e => JsNativeError::error().with_message(format!("fetch: {e}")),
        })?;

    // --- Create pending promise, enqueue HTTP job ---
    let (promise, resolvers) = JsPromise::new_pending(ctx);
    let resolve = resolvers.resolve;
    let reject = resolvers.reject;

    ctx.enqueue_job(Job::PromiseJob(PromiseJob::new(move |ctx| {
        match do_request(url, &method_str, body.as_deref(), &user_agent) {
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
    })));

    Ok(JsValue::from(promise))
}

// ---------------------------------------------------------------------------
// HTTP execution — sync wrapper around async reqwest
// ---------------------------------------------------------------------------

fn do_request(
    url: reqwest::Url,
    method: &str,
    body: Option<&str>,
    user_agent: &str,
) -> Result<(u16, String), String> {
    let method = method.to_owned();
    let body = body.map(str::to_owned);
    let user_agent = user_agent.to_owned();

    let fut = async move {
        let client = Client::builder()
            .user_agent(user_agent)
            .build()
            .map_err(|e| e.to_string())?;
        let method_parsed =
            reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
        let mut req = client.request(method_parsed, url);
        if let Some(b) = body {
            req = req.body(b);
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        Ok((status, text))
    };

    // Always create a fresh runtime. Using Handle::block_on on a borrowed handle
    // deadlocks inside current_thread runtimes (e.g. #[tokio::test]), because
    // block_on needs to drive the scheduler but the owning thread is waiting on
    // spawn_blocking. A new runtime is safe from a spawn_blocking thread and
    // panics loudly if eval() is ever (incorrectly) called from an async task.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(fut)
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
            NativeFunction::from_copy_closure_with_captures(
                json_method,
                ResponseCaptures { body },
            ),
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
            ctx.eval(r#"fetch("https://allowed.com", { method: "POST" })"#).is_err(),
            "should throw for disallowed verb"
        );
    }
}

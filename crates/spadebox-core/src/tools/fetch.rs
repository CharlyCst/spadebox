//! Fetch tool — sandboxed HTTP requests.
//!
//! # Security model
//!
//! Access is controlled by [`HttpConfig`] on the [`Sandbox`]:
//!
//! - When `HttpConfig::enabled` is `false`, all requests are rejected.
//! - [`DomainRule`]s are matched in order against the request hostname; the
//!   first matching rule's `allowed_verbs` applies. Domains not matched by any
//!   rule are rejected.
//! - URL scheme must be `http` or `https`; all other schemes are rejected.

use std::collections::HashMap;

use reqwest::Client;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tool_utils::{DEFAULT_MAX_BYTES, deserialize_bool_flexible, truncate_bytes};
use crate::{
    AsArc, Sandbox, ToolError, ToolResult,
    sandbox::{HttpVerb, substitute_credentials},
};

use super::Tool;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FetchParams {
    /// The URL to fetch (must use `http` or `https` scheme).
    pub url: String,
    /// HTTP method to use (e.g. `"GET"`, `"POST"`).
    pub method: String,
    /// Optional request body (for POST, PUT, PATCH).
    pub body: Option<String>,
    /// Optional HTTP headers to include in the request (e.g. `{"Authorization": "Bearer token"}`).
    pub headers: Option<HashMap<String, String>>,
    /// When `true` return the raw response body, otherwise process the content for efficient LLM
    /// consumption (e.g. convert HTML to markdown). Default to `false`
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub raw: bool,
    /// Maximum number of bytes to return. Defaults to 20 000. Set to 0 to disable.
    pub max_bytes: Option<u64>,
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

pub struct FetchTool;

impl Tool for FetchTool {
    type Params = FetchParams;
    const NAME: &'static str = "fetch";
    const DESCRIPTION: &'static str = "\
        Perform an HTTP request and return the response body as text. \
        The URL must use the http or https scheme. \
        Available methods and domains are determined by the sandbox configuration.";

    async fn run(sandbox: impl AsArc<Sandbox> + Send, params: FetchParams) -> ToolResult<String> {
        let sandbox = sandbox.as_arc();
        let (validated_url, user_agent) = validate_request(&sandbox, &params.url, &params.method)?;
        let method_upper = params.method.to_uppercase();

        let (url, body, headers) = substitute_credentials(
            &sandbox,
            validated_url,
            params.body,
            params.headers.unwrap_or_default(),
        );

        // Build and send the request.
        let client = Client::builder()
            .user_agent(user_agent)
            .build()
            .map_err(|e| ToolError::HttpError(e.to_string()))?;
        let method = reqwest::Method::from_bytes(method_upper.as_bytes())
            .map_err(|e| ToolError::InvalidUrl(format!("invalid method: {}", e)))?;

        let mut req = client.request(method, url);
        if let Some(body) = body {
            req = req.body(body);
        }
        for (key, value) in headers {
            req = req.header(&key, &value);
        }

        let response = req
            .send()
            .await
            .map_err(|e| ToolError::HttpError(e.to_string()))?;

        let status = response.status();
        let content_type = parse_content_type(&response);
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::HttpError(e.to_string()))?;

        if body.is_empty() {
            return Ok(format!("HTTP {}", status.as_u16()));
        }

        let result = if params.raw {
            body
        } else {
            process_body(content_type.as_deref(), &body)?
        };

        Ok(truncate_bytes(
            result,
            params.max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
        ))
    }
}

// ---------------------------------------------------------------------------
// Content processing
// ---------------------------------------------------------------------------

/// Return the content-type value from a response header, stripped of parameters
/// (e.g. `"text/html; charset=utf-8"` → `"text/html"`).
fn parse_content_type(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.split(';').next().unwrap_or(ct).trim().to_lowercase())
}

/// Process a response body for LLM consumption based on its MIME type.
/// Unknown types are returned as-is.
fn process_body(mime: Option<&str>, body: &str) -> ToolResult<String> {
    match mime {
        Some("text/html") => html_to_markdown(body),
        _ => Ok(body.to_owned()),
    }
}

fn html_to_markdown(html: &str) -> ToolResult<String> {
    htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style"])
        .build()
        .convert(html)
        .map_err(|e| ToolError::HttpError(format!("HTML to Markdown conversion failed: {e}")))
}

// ---------------------------------------------------------------------------
// Security Check
// ---------------------------------------------------------------------------

/// Validates an HTTP request against the sandbox policy.
///
/// Checks that HTTP is enabled, the URL is valid, the scheme is http/https,
/// and the method is permitted for the target host. Returns the parsed URL and
/// the configured user-agent string on success.
pub(crate) fn validate_request(
    sandbox: &Sandbox,
    url_str: &str,
    method: &str,
) -> ToolResult<(reqwest::Url, String)> {
    if !sandbox.http_is_enabled() {
        return Err(ToolError::PermissionDenied(
            "HTTP fetch is disabled".to_string(),
        ));
    }

    let url = reqwest::Url::parse(url_str)
        .map_err(|e| ToolError::InvalidUrl(format!("invalid URL '{url_str}': {e}")))?;

    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(ToolError::InvalidUrl(format!(
            "unsupported scheme '{scheme}': only http and https are allowed"
        )));
    }

    let host = url
        .host_str()
        .ok_or_else(|| ToolError::InvalidUrl("URL has no host".to_string()))?;

    let method_upper = method.to_uppercase();
    let user_agent = {
        let http_config = sandbox.http.read().unwrap();
        let allowed_verbs = http_config.allowed_verbs_for(host)?;
        let verb = HttpVerb::parse(&method_upper).ok_or_else(|| {
            ToolError::PermissionDenied(format!("unknown HTTP method '{method}'"))
        })?;
        if !allowed_verbs.contains(&verb) {
            return Err(ToolError::PermissionDenied(format!(
                "method '{method_upper}' is not allowed for host '{host}'"
            )));
        }
        http_config.user_agent.clone()
    };

    Ok((url, user_agent))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use crate::sandbox::{DomainRule, HttpVerb};
    use std::sync::Arc;

    fn setup_sandbox() -> Arc<Sandbox> {
        Arc::new(Sandbox::new())
    }

    // --- Permission checks (no network) ---

    #[tokio::test]
    async fn rejects_when_disabled() {
        let sandbox = setup_sandbox(); // http disabled by default
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://example.com".into(),
                method: "GET".into(),
                body: None,
                headers: None,
                raw: false,
                max_bytes: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_unknown_scheme() {
        let sandbox = setup_sandbox();
        sandbox
            .enable_http()
            .allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "file:///etc/passwd".into(),
                method: "GET".into(),
                body: None,
                headers: None,
                raw: false,
                max_bytes: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn rejects_unmatched_domain() {
        let sandbox = setup_sandbox();
        sandbox
            .enable_http()
            .allow(DomainRule::new("*.example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://evil.com".into(),
                method: "GET".into(),
                body: None,
                headers: None,
                raw: false,
                max_bytes: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_disallowed_verb() {
        let sandbox = setup_sandbox();
        sandbox
            .enable_http()
            .allow(DomainRule::new("example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://example.com".into(),
                method: "POST".into(),
                body: None,
                headers: None,
                raw: false,
                max_bytes: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }
}

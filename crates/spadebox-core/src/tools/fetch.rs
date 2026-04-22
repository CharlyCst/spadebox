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

use reqwest::Client;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Sandbox, ToolError, ToolResult, sandbox::HttpVerb};

use super::{Tool, deserialize_bool_flexible};

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
    /// When `true` return the raw response body, otherwise process the content for efficient LLM
    /// consumption (e.g. convert HTML to markdown). Default to `false`
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub raw: bool,
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

    async fn run(sandbox: &Sandbox, params: FetchParams) -> ToolResult<String> {
        if !sandbox.http.enabled {
            return Err(ToolError::PermissionDenied(
                "HTTP fetch is disabled".to_string(),
            ));
        }

        // Parse and validate the URL.
        let url = reqwest::Url::parse(&params.url)
            .map_err(|e| ToolError::InvalidUrl(format!("invalid URL '{}': {}", params.url, e)))?;

        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(ToolError::InvalidUrl(format!(
                "unsupported scheme '{}': only http and https are allowed",
                scheme
            )));
        }

        let host = url
            .host_str()
            .ok_or_else(|| ToolError::InvalidUrl("URL has no host".to_string()))?;

        // Find the first matching domain rule.
        let allowed_verbs = sandbox.http.allowed_verbs_for(host)?;

        // Validate the requested method against the rule.
        let method_upper = params.method.to_uppercase();
        let verb = HttpVerb::parse(&method_upper).ok_or_else(|| {
            ToolError::PermissionDenied(format!("unknown HTTP method '{}'", params.method))
        })?;

        if !allowed_verbs.contains(&verb) {
            return Err(ToolError::PermissionDenied(format!(
                "method '{}' is not allowed for host '{}'",
                method_upper, host
            )));
        }

        // Build and send the request.
        let client = Client::builder()
            .user_agent(&sandbox.http.user_agent)
            .build()
            .map_err(|e| ToolError::HttpError(e.to_string()))?;
        let method = reqwest::Method::from_bytes(method_upper.as_bytes())
            .map_err(|e| ToolError::InvalidUrl(format!("invalid method: {}", e)))?;

        let mut req = client.request(method, url);
        if let Some(body) = params.body {
            req = req.body(body);
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

        if params.raw {
            Ok(body)
        } else {
            process_body(content_type.as_deref(), &body)
        }
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
    htmd::convert(html)
        .map_err(|e| ToolError::HttpError(format!("HTML to Markdown conversion failed: {e}")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use crate::sandbox::{DomainRule, HttpVerb};

    fn setup_sandbox() -> Sandbox {
        Sandbox::new()
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
                raw: false,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_unknown_scheme() {
        let mut sandbox = setup_sandbox();
        sandbox
            .http
            .enable()
            .allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "file:///etc/passwd".into(),
                method: "GET".into(),
                body: None,
                raw: false,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn rejects_unmatched_domain() {
        let mut sandbox = setup_sandbox();
        sandbox
            .http
            .enable()
            .allow(DomainRule::new("*.example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://evil.com".into(),
                method: "GET".into(),
                body: None,
                raw: false,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_disallowed_verb() {
        let mut sandbox = setup_sandbox();
        sandbox
            .http
            .enable()
            .allow(DomainRule::new("example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://example.com".into(),
                method: "POST".into(),
                body: None,
                raw: false,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }
}

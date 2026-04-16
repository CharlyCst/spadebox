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

use super::Tool;

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FetchParams {
    /// The URL to fetch (must use `http` or `https` scheme).
    pub url: String,
    /// HTTP method to use (e.g. `"GET"`, `"POST"`). Case-insensitive.
    pub method: String,
    /// Optional request body (for POST, PUT, PATCH).
    pub body: Option<String>,
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
        let verb = HttpVerb::from_str(&method_upper).ok_or_else(|| {
            ToolError::PermissionDenied(format!("unknown HTTP method '{}'", params.method))
        })?;

        if !allowed_verbs.contains(&verb) {
            return Err(ToolError::PermissionDenied(format!(
                "method '{}' is not allowed for host '{}'",
                method_upper, host
            )));
        }

        // Build and send the request.
        let client = Client::new();
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
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::HttpError(e.to_string()))?;

        if body.is_empty() {
            Ok(format!("HTTP {}", status.as_u16()))
        } else {
            Ok(body)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sandbox;
    use crate::sandbox::{DomainRule, HttpVerb};
    use tempfile::TempDir;

    fn setup_sandbox() -> (TempDir, Sandbox) {
        let dir = TempDir::new().unwrap();
        let sandbox = Sandbox::new(dir.path()).unwrap();
        (dir, sandbox)
    }

    // --- Permission checks (no network) ---

    #[tokio::test]
    async fn rejects_when_disabled() {
        let (_dir, sandbox) = setup_sandbox(); // http disabled by default
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://example.com".into(),
                method: "GET".into(),
                body: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_unknown_scheme() {
        let (_dir, mut sandbox) = setup_sandbox();
        sandbox.http.enable().allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "file:///etc/passwd".into(),
                method: "GET".into(),
                body: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::InvalidUrl(_))));
    }

    #[tokio::test]
    async fn rejects_unmatched_domain() {
        let (_dir, mut sandbox) = setup_sandbox();
        sandbox.http.enable().allow(DomainRule::new("*.example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://evil.com".into(),
                method: "GET".into(),
                body: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn rejects_disallowed_verb() {
        let (_dir, mut sandbox) = setup_sandbox();
        sandbox.http.enable().allow(DomainRule::new("example.com", vec![HttpVerb::Get]).unwrap());
        let result = FetchTool::run(
            &sandbox,
            FetchParams {
                url: "https://example.com".into(),
                method: "POST".into(),
                body: None,
            },
        )
        .await;
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }
}

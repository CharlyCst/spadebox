use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::tool_utils::Registry;
use crate::{AsArc, ToolError, ToolResult};
use cap_std::ambient_authority;
use cap_std::fs::Dir;

// ---------------------------------------------------------------------------
// HTTP configuration
// ---------------------------------------------------------------------------

/// HTTP verb allowed in a [`DomainRule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl std::str::FromStr for HttpVerb {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        HttpVerb::parse(s).ok_or(())
    }
}

impl HttpVerb {
    /// Returns the uppercase string representation (e.g. `"GET"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpVerb::Get => "GET",
            HttpVerb::Post => "POST",
            HttpVerb::Put => "PUT",
            HttpVerb::Patch => "PATCH",
            HttpVerb::Delete => "DELETE",
            HttpVerb::Head => "HEAD",
        }
    }

    /// Parses an uppercase HTTP method string into an [`HttpVerb`].
    /// Returns `None` if the method is not recognised.
    pub fn parse(method: &str) -> Option<Self> {
        match method {
            "GET" => Some(HttpVerb::Get),
            "POST" => Some(HttpVerb::Post),
            "PUT" => Some(HttpVerb::Put),
            "PATCH" => Some(HttpVerb::Patch),
            "DELETE" => Some(HttpVerb::Delete),
            "HEAD" => Some(HttpVerb::Head),
            _ => None,
        }
    }
}

/// Returns `true` if `pattern` matches `host`.
///
/// Patterns follow the same rules as [`DomainRule`]:
/// - Exact hostname: `"api.example.com"`
/// - Subdomain wildcard: `"*.example.com"`
/// - Catch-all: `"*"`
fn domain_pattern_matches(pattern: &str, host: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(suffix) => host.ends_with(suffix),
        None => host == pattern,
    }
}

/// A domain rule that maps a pattern to a set of allowed HTTP verbs.
///
/// Patterns may be:
/// - An exact hostname: `"api.example.com"`
/// - A wildcard prefix: `"*.example.com"` (matches any subdomain)
/// - A catch-all: `"*"` (matches any host)
///
/// `'*'` may only appear as the first character. When multiple rules match a
/// request, the most specific one wins (longest literal suffix).
pub struct DomainRule {
    /// Pattern string to match against.
    pub(crate) pattern: String,
    /// HTTP verbs permitted for domains matching this rule.
    pub allowed_verbs: Vec<HttpVerb>,
    /// Number of literal characters (pattern length minus any leading `*`).
    /// Used to pick the most specific matching rule.
    pub(crate) specificity: usize,
}

impl DomainRule {
    /// Creates a new `DomainRule`.
    ///
    /// Returns [`ToolError::InvalidPattern`] if `'*'` appears anywhere other
    /// than the first character.
    /// Creates a new `DomainRule`.
    ///
    /// Returns [`ToolError::InvalidPattern`] if the pattern is not one of:
    /// - An exact hostname: `"api.example.com"`
    /// - A subdomain wildcard: `"*.example.com"`
    /// - A catch-all: `"*"`
    pub fn new(pattern: impl Into<String>, allowed_verbs: Vec<HttpVerb>) -> ToolResult<Self> {
        let pattern = pattern.into();
        let invalid = pattern.contains('*') && pattern != "*" && !pattern.starts_with("*.");
        if invalid || pattern.chars().skip(1).any(|c| c == '*') {
            return Err(ToolError::InvalidPattern(format!(
                "domain pattern must be an exact hostname, '*', or '*.suffix', got: '{pattern}'"
            )));
        }
        let specificity = pattern.trim_start_matches('*').len();
        Ok(DomainRule {
            pattern,
            allowed_verbs,
            specificity,
        })
    }

    /// Returns `true` if this rule matches `host`.
    pub(crate) fn matches(&self, host: &str) -> bool {
        domain_pattern_matches(&self.pattern, host)
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// A credential that can be injected into fetch requests.
///
/// Agents hold an opaque token returned by [`Sandbox::add_credential`]. At
/// fetch time, any occurrence of the token in the URL or body is replaced with
/// the real value, provided the target host matches one of `domains`.
pub struct Credential {
    /// Human-readable label; not part of the token.
    pub name: String,
    /// The real secret value substituted at fetch time.
    pub value: String,
    /// Domain patterns that allow substitution (same syntax as [`DomainRule`]).
    pub domains: Vec<String>,
}

impl Credential {
    fn matches_host(&self, host: &str) -> bool {
        self.domains
            .iter()
            .any(|pattern| domain_pattern_matches(pattern, host))
    }
}

/// Returns `true` if `text` contains a credential token that is allowed for `host`.
fn has_credential(text: &str, host: &str, http: &HttpConfig) -> bool {
    if http.credentials.is_empty() || !text.contains("SPADB-") {
        return false;
    }
    http.credentials
        .iter()
        .any(|(token, cred)| cred.matches_host(host) && text.contains(token.as_str()))
}

/// Replaces credential tokens in `text` in-place for all credentials allowed for `host`.
fn replace_credentials(text: &mut String, host: &str, http: &HttpConfig) {
    for (token, cred) in &http.credentials {
        if text.contains(token.as_str()) && cred.matches_host(host) {
            *text = text.replace(token.as_str(), &cred.value);
        }
    }
}

/// Substitutes credentials in a validated URL, optional body, and headers.
///
/// Acquires and releases the [`HttpConfig`] lock internally, so callers must
/// not hold it already. Returns the updated URL, body, and headers ready for
/// the request. When no credentials match, all are returned as-is.
pub(crate) fn substitute_credentials(
    sandbox: &Sandbox,
    url: reqwest::Url,
    body: Option<String>,
    headers: HashMap<String, String>,
) -> (reqwest::Url, Option<String>, HashMap<String, String>) {
    let http = sandbox.http.read().unwrap();
    let host = url.host_str().unwrap_or("").to_owned();

    let url = if has_credential(url.as_str(), &host, &http) {
        let mut s = url.to_string();
        replace_credentials(&mut s, &host, &http);
        reqwest::Url::parse(&s).unwrap_or(url)
    } else {
        url
    };

    let body = body.map(|mut b| {
        if has_credential(&b, &host, &http) {
            replace_credentials(&mut b, &host, &http);
        }
        b
    });

    let headers = headers
        .into_iter()
        .map(|(k, mut v)| {
            if has_credential(&v, &host, &http) {
                replace_credentials(&mut v, &host, &http);
            }
            (k, v)
        })
        .collect();

    (url, body, headers)
}

// ---------------------------------------------------------------------------
// HTTP configuration
// ---------------------------------------------------------------------------

/// Configuration for the `fetch` tool.
///
/// When `enabled` is `false`, all fetch calls return a permission error
/// regardless of the domain rules.
///
/// # Example
///
/// ```
/// use spadebox_core::Sandbox;
/// use spadebox_core::{DomainRule, HttpVerb};
///
/// let sandbox = Sandbox::new();
/// sandbox.enable_http()
///     .allow(DomainRule::new("api.example.com", vec![HttpVerb::Get, HttpVerb::Post]).unwrap())
///     .allow(DomainRule::new("*.cdn.example.com", vec![HttpVerb::Get]).unwrap());
/// ```
pub struct HttpConfig {
    pub enabled: bool,
    /// Domain rules evaluated in order; first match wins.
    pub domain_rules: Vec<DomainRule>,
    /// Value sent as the `User-Agent` header on every request.
    pub user_agent: String,
    /// Registered credentials keyed by their opaque token.
    pub credentials: HashMap<String, Credential>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            domain_rules: Vec::new(),
            user_agent: "spadebox/0.0.0 (AI-agent)".to_string(),
            credentials: HashMap::new(),
        }
    }
}

impl HttpConfig {
    /// Returns the allowed verbs for `host` from the most specific matching rule.
    /// Returns `Err(PermissionDenied)` if no rule matches.
    pub(crate) fn allowed_verbs_for(&self, host: &str) -> crate::ToolResult<&[HttpVerb]> {
        self.domain_rules
            .iter()
            .filter(|rule| rule.matches(host))
            .max_by_key(|rule| rule.specificity)
            .map(|rule| rule.allowed_verbs.as_slice())
            .ok_or_else(|| {
                crate::ToolError::PermissionDenied(format!(
                    "host '{}' is not allowed by any domain rule",
                    host
                ))
            })
    }
}

// ---------------------------------------------------------------------------
// Filesystem configuration
// ---------------------------------------------------------------------------

/// Configuration for filesystem tools (`read_file`, `write_file`, `edit_file`,
/// `glob`, `grep`).
///
/// When `root` is `None`, all filesystem tool calls return a permission error.
///
/// # Example
///
/// ```
/// use spadebox_core::Sandbox;
/// # use tempfile::TempDir;
/// # let dir = TempDir::new().unwrap();
///
/// let sandbox = Sandbox::new();
/// sandbox.enable_fs(dir.path()).unwrap();
/// ```
#[derive(Default)]
pub struct FilesConfig {
    pub(crate) root: Option<Dir>,
    pub(crate) read_registry: Registry,
}

impl FilesConfig {
    /// Opens `root` as the sandbox root path and resets the read registry.
    ///
    /// Resets the registry so stale read records from a previous root do not
    /// carry over.
    pub fn set_root(&mut self, root: impl AsRef<Path>) -> ToolResult<()> {
        let root = Dir::open_ambient_dir(&root, ambient_authority())
            .map_err(|e| map_io_err(&root.as_ref().to_string_lossy(), e))?;
        self.root = Some(root);
        self.read_registry = HashMap::new();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// JavaScript configuration
// ---------------------------------------------------------------------------

/// A request to evaluate JavaScript code, sent to the dedicated REPL thread.
type JsEvalRequest = (
    String,
    tokio::sync::oneshot::Sender<ToolResult<crate::js_runtime::JsOutput>>,
);

/// Live handle to the dedicated JS REPL thread.
///
/// # Why a dedicated thread?
///
/// Boa's `JsContext` is `!Send`: it cannot be moved across threads. We spawn a
/// dedicated OS thread that owns the `JsContext` for its entire lifetime and
/// processes evaluation requests through a channel. This keeps `JsConfig` —
/// and therefore `Sandbox` — `Send + Sync`, while naturally preserving JS session
/// state (variables, loaded modules, …) across tool calls.
///
/// [`tokio::sync::mpsc::UnboundedSender`] is used instead of
/// `std::sync::mpsc::Sender` because only the former is `Send + Sync`, which is
/// required for `Sandbox` to be `Sync`.
struct JsReplHandle {
    tx: tokio::sync::mpsc::UnboundedSender<JsEvalRequest>,
    /// Kept alive so the thread is joined on drop rather than detached.
    _thread: std::thread::JoinHandle<()>,
}

/// A native function stored in [`JsConfig::funcs`] and shared with JS contexts.
pub(crate) type JsFunc =
    Arc<dyn Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static>;

/// Configuration and handle for the JavaScript tools.
pub struct JsConfig {
    /// `None` until first use of the JavaScript REPL.
    //
    //  TODO: on drop we will need to signal the REPL thread to shut down, but this would require
    //  support for interrupting Boa, which is not available yet.
    repl_handle: RwLock<Option<JsReplHandle>>,
    /// Native functions registered via [`Sandbox::expose_js_func`].
    ///
    /// Append-only. The REPL thread tracks a cursor and registers newly appended
    /// functions before each evaluation. Fresh `js_exec` contexts register all
    /// entries when they are constructed.
    ///
    /// Each entry is `(name, params, func)` where `params` lists the positional
    /// parameter names. The runtime maps JS positional arguments to a JSON object
    /// `{ paramName: value, ... }` before calling `func`.
    pub(crate) funcs: RwLock<Vec<(String, Vec<String>, JsFunc)>>,
}

impl Default for JsConfig {
    fn default() -> Self {
        Self {
            repl_handle: RwLock::new(None),
            funcs: RwLock::new(Vec::new()),
        }
    }
}

impl JsConfig {
    /// Spawns the dedicated JavaScript REPL thread, if not already started. No-op otherwise.
    fn init_repl_handle(&self, sandbox: impl AsArc<Sandbox>) {
        let sandbox = sandbox.as_arc();
        let handle = self.repl_handle.read().unwrap();
        if handle.is_some() {
            return; // Already initialized
        }

        // We need to spawn a new thread. Start by dropping the read lock to acquire a write lock.
        drop(handle);
        let mut handle = self.repl_handle.write().unwrap();
        if handle.is_some() {
            return; // Someone created the thread in between the drop and re-lock
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<JsEvalRequest>();
        let thread = std::thread::spawn(move || {
            let mut ctx = crate::js_runtime::JsContext::new(&sandbox);
            let mut registered = 0usize;
            // `blocking_recv` parks this thread until a message arrives.
            // Loop exits when all senders are dropped (i.e., when `JsConfig` is
            // dropped), cleanly destroying the context.
            while let Some((code, reply)) = rx.blocking_recv() {
                // Register any functions appended since the last evaluation, then eval.
                // Always advance the cursor even on error to avoid retrying a broken entry.
                let result = {
                    let funcs = sandbox.js.funcs.read().unwrap();
                    let reg = ctx.register_funcs(&funcs[registered..]);
                    registered = funcs.len();
                    reg
                }
                .and_then(|()| ctx.eval(&code));
                let _ = reply.send(result);
            }
        });
        *handle = Some(JsReplHandle {
            tx,
            _thread: thread,
        });
    }

    /// Sends `code` to the JS context thread and awaits the result.
    pub(crate) async fn repl_eval(
        &self,
        sandbox: Arc<Sandbox>,
        code: String,
    ) -> ToolResult<crate::js_runtime::JsOutput> {
        self.init_repl_handle(sandbox);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        // Lock the handle, send the message, and release the lock before awaiting the answer.
        {
            let handle = self.repl_handle.read().unwrap();
            let tx = &handle
                .as_ref()
                .expect("init_repl_handle guarantees Some")
                .tx;
            tx.send((code, reply_tx))
                .map_err(|_| ToolError::JsError("JS repl thread has shut down".into()))?;
        }

        // Now that we release the lock we can safely await.
        reply_rx
            .await
            .map_err(|_| ToolError::JsError("JS repl thread has shut down".into()))?
    }

    /// Appends `func` to the shared function table under `name`.
    ///
    /// The REPL thread picks up new entries before each evaluation; fresh
    /// `js_exec` contexts register all entries on construction.
    pub(crate) fn expose_js_func(&self, name: String, params: Vec<String>, func: JsFunc) {
        self.funcs.write().unwrap().push((name, params, func));
    }
}

// ---------------------------------------------------------------------------
// Sandbox
// ---------------------------------------------------------------------------

/// The sandbox configuration.
///
/// The inner `RwLock` fields use `std::sync::RwLock` (not `tokio::sync::RwLock`). They
/// may be locked in async code but must never be held across an `.await` point —
/// that would block the async executor.
pub struct Sandbox {
    pub files: RwLock<FilesConfig>,
    pub http: RwLock<HttpConfig>,
    pub js: JsConfig,
    /// File operations are enabled.
    fs_is_enabled: AtomicBool,
    /// HTTP operations are enabled.
    http_is_enabled: AtomicBool,
    /// JS operations are enabled.
    js_is_enabled: AtomicBool,
}

impl Sandbox {
    /// Creates a new `Sandbox` with all tools disabled.
    ///
    /// Activate individual tool groups with:
    /// - [`Sandbox::enable_fs`] — filesystem tools
    /// - [`Sandbox::enable_http`] — HTTP fetch tool
    /// - [`Sandbox::enable_js`] — JavaScript tools
    pub fn new() -> Self {
        static INIT_TLS: std::sync::Once = std::sync::Once::new();
        INIT_TLS.call_once(|| {
            rustls::crypto::ring::default_provider()
                .install_default()
                .expect("failed to install ring crypto provider");
        });

        Sandbox {
            files: RwLock::new(FilesConfig::default()),
            http: RwLock::new(HttpConfig::default()),
            js: JsConfig::default(),
            fs_is_enabled: AtomicBool::new(false),
            http_is_enabled: AtomicBool::new(false),
            js_is_enabled: AtomicBool::new(false),
        }
    }

    /// File operations are enabled.
    pub fn fs_is_enabled(&self) -> bool {
        self.fs_is_enabled.load(Ordering::Acquire)
    }

    /// HTTP operations are enabled.
    pub fn http_is_enabled(&self) -> bool {
        self.http_is_enabled.load(Ordering::Acquire)
    }

    /// JavaScript operations are enabled.
    pub fn js_is_enabled(&self) -> bool {
        self.js_is_enabled.load(Ordering::Acquire)
    }

    /// Enables file operation tools and opens `root` as the sandbox root path.
    ///
    /// Resets the registry so stale read records from a previous root do not
    /// carry over.
    pub fn enable_fs(&self, root: impl AsRef<Path>) -> ToolResult<&Self> {
        self.files.write().unwrap().set_root(root)?;
        self.fs_is_enabled.store(true, Ordering::Release);
        Ok(self)
    }

    /// Enables HTTP tools.
    pub fn enable_http(&self) -> &Self {
        self.http_is_enabled.store(true, Ordering::Release);
        self
    }

    /// Enables JavaScript tools.
    pub fn enable_js(&self) -> &Self {
        self.js_is_enabled.store(true, Ordering::Release);
        self
    }

    /// Sets the `User-Agent` header sent with every request.
    pub fn set_user_agent(&self, user_agent: impl Into<String>) -> &Self {
        let mut http_config = self.http.write().unwrap();
        http_config.user_agent = user_agent.into();
        self
    }

    /// Registers a credential and returns a stable opaque token the agent can use as a placeholder.
    ///
    /// The token is deterministic: the same `name` always produces the same token across process
    /// restarts. Security relies on the `domains` allowlist — the token is substituted only when
    /// the fetch target matches one of the supplied domain patterns (same syntax as [`DomainRule`]).
    ///
    /// # Example
    ///
    /// ```
    /// use spadebox_core::Sandbox;
    ///
    /// let sandbox = Sandbox::new();
    /// let token = sandbox.add_credential("github-token", "secret", ["api.github.com"]);
    /// // token is something like "SPADB-a3f7..."
    /// // The agent can now pass `token` as a Bearer value; it will be substituted at fetch time.
    /// ```
    pub fn add_credential(
        &self,
        name: impl Into<String>,
        value: impl Into<String>,
        domains: impl IntoIterator<Item = impl Into<String>>,
    ) -> String {
        let name = name.into();
        let value = value.into();
        let domains: Vec<String> = domains.into_iter().map(Into::into).collect();

        let mut hasher = DefaultHasher::new();
        "spadebox".hash(&mut hasher);
        name.hash(&mut hasher);
        let token = format!("SPADB-{:016x}", hasher.finish());

        self.http.write().unwrap().credentials.insert(
            token.clone(),
            Credential {
                name,
                value,
                domains,
            },
        );
        token
    }

    /// Appends a domain rule.
    pub fn allow(&self, rule: DomainRule) -> &Self {
        let mut http_config = self.http.write().unwrap();
        http_config.domain_rules.push(rule);
        self
    }

    /// Registers a native function as a JavaScript global, available to both the
    /// persistent REPL session and fresh `js_exec` contexts.
    ///
    /// `params` declares the positional parameter names. When the function is called
    /// from JavaScript, positional arguments are mapped to a JSON object
    /// `{ "paramName": value, … }` and passed to `func`. The return value is
    /// converted back to a JS value, or a JS `Error` is thrown if `func` returns
    /// `Err`.
    ///
    /// Returns [`ToolError::PermissionDenied`] if JavaScript has not been enabled.
    ///
    /// # Security
    ///
    /// Exposed functions execute as trusted host code outside SpadeBox's JavaScript
    /// runtime and outside the SpadeBox sandbox. Only expose callbacks that
    /// intentionally provide capabilities you want JavaScript code to have.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::sync::Arc;
    /// # use spadebox_core::Sandbox;
    /// let sandbox = Arc::new(Sandbox::new());
    /// sandbox.enable_js();
    /// sandbox.expose_js_func("add", ["a", "b"], |args| {
    ///     let a = args["a"].as_i64().unwrap_or(0);
    ///     let b = args["b"].as_i64().unwrap_or(0);
    ///     Ok(serde_json::Value::Number((a + b).into()))
    /// }).unwrap();
    /// ```
    pub fn expose_js_func(
        &self,
        name: impl Into<String>,
        params: impl IntoIterator<Item = impl Into<String>>,
        func: impl Fn(serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync + 'static,
    ) -> ToolResult<()> {
        if !self.js_is_enabled() {
            return Err(ToolError::PermissionDenied("JS is disabled".to_string()));
        }
        let params: Vec<String> = params.into_iter().map(Into::into).collect();
        self.js.expose_js_func(name.into(), params, Arc::new(func));
        Ok(())
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps a raw `io::Error` from cap-std into a structured `ToolError`.
///
/// On Linux 5.6+, `cap-std` uses `openat2` with `RESOLVE_BENEATH`. The kernel
/// returns `EXDEV` (errno 18) when any path component (including symlinks)
/// attempts to escape the jail root. On older kernels and macOS, cap-std's
/// userspace resolver returns `EACCES` / `PermissionDenied` for escapes.
pub(crate) fn map_io_err(path: &str, e: std::io::Error) -> ToolError {
    const EXDEV: i32 = 18;
    if e.raw_os_error() == Some(EXDEV) {
        return ToolError::EscapeAttempt(path.to_string());
    }
    match e.kind() {
        std::io::ErrorKind::NotFound => ToolError::NotFound(path.to_string()),
        std::io::ErrorKind::PermissionDenied => ToolError::PermissionDenied(path.to_string()),
        _ => ToolError::IoError(e),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_match_wins() {
        // Rule order is deliberately reversed to confirm it's specificity, not insertion order.
        let sandbox = Sandbox::new();
        sandbox
            .allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap())
            .allow(DomainRule::new("*.example.com", vec![HttpVerb::Post]).unwrap())
            .allow(DomainRule::new("api.example.com", vec![HttpVerb::Delete]).unwrap());
        let http_config = sandbox.http.read().unwrap();

        // Exact match is most specific
        let verbs = http_config.allowed_verbs_for("api.example.com").unwrap();
        assert_eq!(verbs, &[HttpVerb::Delete]);

        // Subdomain wildcard beats catch-all
        let verbs = http_config.allowed_verbs_for("other.example.com").unwrap();
        assert_eq!(verbs, &[HttpVerb::Post]);

        // Only catch-all matches
        let verbs = http_config.allowed_verbs_for("unrelated.com").unwrap();
        assert_eq!(verbs, &[HttpVerb::Get]);
    }

    #[test]
    fn rejects_wildcard_not_at_start() {
        assert!(DomainRule::new("api.*.com", vec![]).is_err());
        assert!(DomainRule::new("*.*.com", vec![]).is_err());
        assert!(DomainRule::new("api*", vec![]).is_err());
        // '*' without a following '.' would match across domain boundaries
        // (e.g. "*test.com" would match "mytest.com" via ends_with).
        assert!(DomainRule::new("*test.com", vec![]).is_err());
    }

    #[test]
    fn parse_verb_recognizes_known_verbs() {
        assert_eq!(HttpVerb::parse("GET"), Some(HttpVerb::Get));
        assert_eq!(HttpVerb::parse("POST"), Some(HttpVerb::Post));
        assert_eq!(HttpVerb::parse("PUT"), Some(HttpVerb::Put));
        assert_eq!(HttpVerb::parse("PATCH"), Some(HttpVerb::Patch));
        assert_eq!(HttpVerb::parse("DELETE"), Some(HttpVerb::Delete));
        assert_eq!(HttpVerb::parse("HEAD"), Some(HttpVerb::Head));
        assert_eq!(HttpVerb::parse("UNKNOWN"), None);
    }

    #[test]
    fn credential_tokens() {
        let sandbox = Sandbox::new();

        // Same name → same token regardless of value or domains.
        let t1 = sandbox.add_credential("my-key", "secret1", ["api.example.com"]);
        let t2 = sandbox.add_credential("my-key", "secret2", ["other.com"]);
        assert_eq!(t1, t2);
        assert!(t1.starts_with("SPADB-"));

        let token = sandbox.add_credential("api-key", "real-secret", ["api.example.com"]);
        let http = sandbox.http.read().unwrap();

        // Substitutes when host is in the allowlist.
        assert!(has_credential(&token, "api.example.com", &http));
        let mut s = token.clone();
        replace_credentials(&mut s, "api.example.com", &http);
        assert_eq!(s, "real-secret");

        // No substitution for unlisted domains.
        assert!(!has_credential(&token, "evil.com", &http));

        // Wildcard domain patterns.
        drop(http);
        let token = sandbox.add_credential("wildcard-key", "secret", ["*.example.com"]);
        let http = sandbox.http.read().unwrap();
        assert!(has_credential(&token, "api.example.com", &http));
        assert!(has_credential(&token, "cdn.example.com", &http));
        assert!(!has_credential(&token, "evil.com", &http));

        // Token embedded in a larger string.
        let mut body = format!("{{\"Authorization\": \"Bearer {token}\"}}");
        assert!(has_credential(&body, "api.example.com", &http));
        replace_credentials(&mut body, "api.example.com", &http);
        assert_eq!(body, r#"{"Authorization": "Bearer secret"}"#);

        // No token present → no match.
        let text = "https://api.example.com/data?foo=bar";
        assert!(!has_credential(text, "api.example.com", &http));
    }
}

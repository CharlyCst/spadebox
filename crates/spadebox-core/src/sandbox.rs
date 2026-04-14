use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_std::time::SystemTime;
use globset::{Glob, GlobMatcher};

use crate::{ToolError, ToolResult};

/// Registry mapping relative file paths to the mtime recorded at last read.
///
/// Used to enforce read-before-write and detect external modifications.
/// The inner `Mutex` is a `std::sync::Mutex` (not `tokio::sync::Mutex`) because it
/// is only ever locked on blocking threads inside `spawn_blocking`. Never lock it
/// across an `.await` point — that would block the async executor.
pub(crate) type Registry = Arc<Mutex<HashMap<String, SystemTime>>>;

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
    pub(crate) fn from_str(method: &str) -> Option<Self> {
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

/// A domain rule that maps a glob pattern to a set of allowed HTTP verbs.
///
/// Rules are evaluated in order; the first matching rule wins.
/// Use `"*"` as the pattern to create a catch-all default.
/// Domains not matched by any rule are rejected.
///
/// Construct with [`DomainRule::new`] — the glob pattern is compiled once at
/// creation time and reused on every request.
pub struct DomainRule {
    /// Original glob pattern string, kept for display and diagnostic purposes.
    pub(crate) _pattern: String,
    /// HTTP verbs permitted for domains matching this rule.
    pub allowed_verbs: Vec<HttpVerb>,
    /// Pre-compiled matcher derived from `pattern`.
    pub(crate) matcher: GlobMatcher,
}

impl DomainRule {
    /// Creates a new `DomainRule`, compiling the glob pattern eagerly.
    ///
    /// Returns [`ToolError::InvalidPattern`] if `pattern` is not a valid glob.
    pub fn new(pattern: impl Into<String>, allowed_verbs: Vec<HttpVerb>) -> ToolResult<Self> {
        let pattern = pattern.into();
        let matcher = Glob::new(&pattern)
            .map_err(|e| ToolError::InvalidPattern(e.to_string()))?
            .compile_matcher();
        Ok(DomainRule { _pattern: pattern, allowed_verbs, matcher })
    }
}

/// Configuration for the `fetch` tool.
///
/// When `enabled` is `false`, all fetch calls return a permission error
/// regardless of the domain rules.
///
/// # Example
///
/// ```
/// use spadebox_core::{HttpConfig, DomainRule, HttpVerb};
///
/// let config = HttpConfig::new()
///     .allow(DomainRule::new("api.example.com", vec![HttpVerb::Get, HttpVerb::Post]).unwrap())
///     .allow(DomainRule::new("*.cdn.example.com", vec![HttpVerb::Get]).unwrap());
/// ```
#[derive(Default)]
pub struct HttpConfig {
    pub enabled: bool,
    /// Domain rules evaluated in order; first match wins.
    pub domain_rules: Vec<DomainRule>,
}

impl HttpConfig {
    /// Creates an enabled `HttpConfig` with no domain rules.
    ///
    /// Add rules with [`HttpConfig::allow`].
    pub fn new() -> Self {
        HttpConfig {
            enabled: true,
            domain_rules: Vec::new(),
        }
    }

    /// Appends a domain rule and returns `self` for chaining.
    pub fn allow(mut self, rule: DomainRule) -> Self {
        self.domain_rules.push(rule);
        self
    }

    /// Finds the allowed verbs for `host` by matching domain rules in order.
    /// Returns `Err(PermissionDenied)` if no rule matches.
    pub(crate) fn allowed_verbs_for(&self, host: &str) -> crate::ToolResult<&[HttpVerb]> {
        for rule in &self.domain_rules {
            if rule.matcher.is_match(host) {
                return Ok(&rule.allowed_verbs);
            }
        }
        Err(crate::ToolError::PermissionDenied(format!(
            "host '{}' is not allowed by any domain rule",
            host
        )))
    }
}

// ---------------------------------------------------------------------------
// Sandbox
// ---------------------------------------------------------------------------

pub struct Sandbox {
    pub(crate) root: Dir,
    pub(crate) read_registry: Registry,
    pub(crate) http: HttpConfig,
}

impl Sandbox {
    /// Opens `path` as the jail root. All subsequent tool operations are
    /// confined to this directory — no ambient filesystem access occurs.
    ///
    /// HTTP fetching is disabled by default; configure [`Sandbox::http`] to enable it.
    pub fn new(path: impl AsRef<Path>) -> ToolResult<Self> {
        let root = Dir::open_ambient_dir(&path, ambient_authority())
            .map_err(|e| map_io_err(&path.as_ref().to_string_lossy(), e))?;
        Ok(Sandbox {
            root,
            read_registry: Arc::new(Mutex::new(HashMap::new())),
            http: HttpConfig::default(),
        })
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
    fn first_matching_rule_wins() {
        let config = HttpConfig::new()
            .allow(DomainRule::new("api.example.com", vec![HttpVerb::Get, HttpVerb::Post]).unwrap())
            .allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap());

        // Specific rule matches first — POST is allowed
        let verbs = config.allowed_verbs_for("api.example.com").unwrap();
        assert!(verbs.contains(&HttpVerb::Post));

        // Catch-all matches — POST is not in the catch-all verbs
        let verbs = config.allowed_verbs_for("other.com").unwrap();
        assert!(!verbs.contains(&HttpVerb::Post));
        assert!(verbs.contains(&HttpVerb::Get));
    }

    #[test]
    fn parse_verb_recognizes_known_verbs() {
        assert_eq!(HttpVerb::from_str("GET"), Some(HttpVerb::Get));
        assert_eq!(HttpVerb::from_str("POST"), Some(HttpVerb::Post));
        assert_eq!(HttpVerb::from_str("PUT"), Some(HttpVerb::Put));
        assert_eq!(HttpVerb::from_str("PATCH"), Some(HttpVerb::Patch));
        assert_eq!(HttpVerb::from_str("DELETE"), Some(HttpVerb::Delete));
        assert_eq!(HttpVerb::from_str("HEAD"), Some(HttpVerb::Head));
        assert_eq!(HttpVerb::from_str("UNKNOWN"), None);
    }
}

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::{ToolError, ToolResult};
use cap_std::ambient_authority;
use cap_std::fs::Dir;
use cap_std::time::SystemTime;

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
        match self.pattern.strip_prefix('*') {
            Some(suffix) => host.ends_with(suffix),
            None => host == self.pattern,
        }
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
/// use spadebox_core::Sandbox;
/// use spadebox_core::{DomainRule, HttpVerb};
///
/// let mut sandbox = Sandbox::new();
/// sandbox.http
///     .enable()
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
    /// Returns `true` if HTTP fetching has been enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enables HTTP fetching and returns `&mut self` for chaining.
    pub fn enable(&mut self) -> &mut Self {
        self.enabled = true;
        self
    }

    /// Appends a domain rule and returns `&mut self` for chaining.
    pub fn allow(&mut self, rule: DomainRule) -> &mut Self {
        self.domain_rules.push(rule);
        self
    }

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
/// let mut sandbox = Sandbox::new();
/// sandbox.files.enable(dir.path()).unwrap();
/// ```
#[derive(Default)]
pub struct FilesConfig {
    pub(crate) root: Option<Dir>,
    pub(crate) read_registry: Registry,
}

impl FilesConfig {
    /// Opens `path` as the sandbox root and resets the read registry.
    ///
    /// Resets the registry so stale read records from a previous root do not
    /// carry over. Returns `&mut self` for consistency; filesystem tools are
    /// enabled as soon as this call succeeds.
    pub fn enable(&mut self, path: impl AsRef<Path>) -> ToolResult<&mut Self> {
        let root = Dir::open_ambient_dir(&path, ambient_authority())
            .map_err(|e| map_io_err(&path.as_ref().to_string_lossy(), e))?;
        self.root = Some(root);
        self.read_registry = Arc::new(Mutex::new(HashMap::new()));
        Ok(self)
    }

    /// Returns `true` if a sandbox root has been set.
    pub fn is_enabled(&self) -> bool {
        self.root.is_some()
    }

    /// Returns a clone of the root `Dir`, or `Err(PermissionDenied)` if
    /// filesystem tools are disabled.
    pub(crate) fn try_clone_root(&self) -> ToolResult<Dir> {
        self.root
            .as_ref()
            .ok_or_else(|| ToolError::PermissionDenied("filesystem access is disabled".into()))?
            .try_clone()
            .map_err(ToolError::IoError)
    }
}

// ---------------------------------------------------------------------------
// Sandbox
// ---------------------------------------------------------------------------

pub struct Sandbox {
    pub files: FilesConfig,
    pub http: HttpConfig,
}

impl Sandbox {
    /// Creates a new `Sandbox` with filesystem tools and HTTP fetching both
    /// disabled. Call `sandbox.files.enable(path)` and/or
    /// `sandbox.http.enable()` to activate them.
    pub fn new() -> Self {
        Sandbox {
            files: FilesConfig::default(),
            http: HttpConfig::default(),
        }
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
        let mut config = HttpConfig::default();
        config
            .allow(DomainRule::new("*", vec![HttpVerb::Get]).unwrap())
            .allow(DomainRule::new("*.example.com", vec![HttpVerb::Post]).unwrap())
            .allow(DomainRule::new("api.example.com", vec![HttpVerb::Delete]).unwrap());

        // Exact match is most specific
        let verbs = config.allowed_verbs_for("api.example.com").unwrap();
        assert_eq!(verbs, &[HttpVerb::Delete]);

        // Subdomain wildcard beats catch-all
        let verbs = config.allowed_verbs_for("other.example.com").unwrap();
        assert_eq!(verbs, &[HttpVerb::Post]);

        // Only catch-all matches
        let verbs = config.allowed_verbs_for("unrelated.com").unwrap();
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
}

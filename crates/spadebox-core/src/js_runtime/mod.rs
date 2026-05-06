use std::sync::Arc;

use boa_engine::{Context, Source};

use crate::{Sandbox, ToolError, ToolResult};

mod files;

/// A JavaScript execution context with a persistent session.
///
/// Wraps a Boa [`Context`] and exposes a simple [`eval`](JsContext::eval) method.
/// All state — variables, functions, loaded modules — is preserved across calls,
/// giving tools a REPL-like experience.
///
/// `JsContext` is the single place that imports `boa_engine`; callers never
/// interact with Boa types directly.
pub struct JsContext {
    ctx: Context,
    #[allow(dead_code)]
    sandbox: Arc<Sandbox>,
}

impl JsContext {
    /// Creates a new `JsContext` with all runtime APIs registered.
    pub fn new(sandbox: Arc<Sandbox>) -> Self {
        let mut ctx = Context::default();
        files::register(&mut ctx, Arc::clone(&sandbox));
        Self { ctx, sandbox }
    }

    /// Evaluates `code` and returns the result as a string.
    pub fn eval(&mut self, code: &str) -> ToolResult<String> {
        self.ctx
            .eval(Source::from_bytes(code.as_bytes()))
            .map(|v| v.display().to_string())
            .map_err(|e| ToolError::JsError(e.to_string()))
    }
}

/// Captures for native JS functions — wraps `Arc<Sandbox>` in a GC-traceable struct.
///
/// `Arc<Sandbox>` contains no GC-managed values, so all trace methods are no-ops.
struct SandboxCaptures {
    sandbox: Arc<Sandbox>,
}

impl boa_engine::gc::Finalize for SandboxCaptures {}

// SAFETY: `Arc<Sandbox>` holds no GC-managed objects; nothing to trace.
unsafe impl boa_engine::gc::Trace for SandboxCaptures {
    unsafe fn trace(&self, _tracer: &mut boa_engine::gc::Tracer) {}
    unsafe fn trace_non_roots(&self) {}
    fn run_finalizer(&self) {
        boa_engine::gc::Finalize::finalize(self);
    }
}

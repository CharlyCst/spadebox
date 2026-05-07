use std::{cell::RefCell, rc::Rc, sync::Arc};

use boa_engine::{Context, Source};

use crate::{Sandbox, ToolError, ToolResult};

mod console;
mod files;
mod loader;

/// Output from a JavaScript evaluation: the expression value plus any captured console lines.
#[derive(Debug)]
pub(crate) struct JsOutput {
    /// String representation of the last evaluated expression.
    pub value: String,
    /// Lines emitted via `console.log`, `console.warn`, etc., in order.
    pub console: Vec<String>,
}

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
    console_output: Rc<RefCell<Vec<String>>>,
    #[allow(dead_code)]
    sandbox: Arc<Sandbox>,
}

impl JsContext {
    /// Creates a new `JsContext` with all runtime APIs registered.
    pub fn new(sandbox: Arc<Sandbox>) -> Self {
        let mut ctx = Context::builder()
            .module_loader(Rc::new(loader::SpadeboxModuleLoader {
                sandbox: Arc::clone(&sandbox),
            }))
            .build()
            .expect("failed to build JS context");

        // Inject runtime functions and objects
        files::register(&mut ctx, Arc::clone(&sandbox));
        loader::register_require(&mut ctx, Arc::clone(&sandbox));
        let console_output = console::register(&mut ctx);

        Self { ctx, console_output, sandbox }
    }

    /// Evaluates `code` and returns the result along with any captured console output.
    pub fn eval(&mut self, code: &str) -> ToolResult<JsOutput> {
        let result = self.ctx.eval(Source::from_bytes(code.as_bytes()));
        let console = self.console_output.borrow_mut().drain(..).collect();
        result
            .map(|v| JsOutput { value: v.display().to_string(), console })
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

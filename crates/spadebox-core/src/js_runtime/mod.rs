use std::{cell::RefCell, rc::Rc, sync::Arc};

use boa_engine::{Context, JsNativeError, JsValue, NativeFunction, Source, js_string};

use crate::{Sandbox, ToolError, ToolResult};

mod console;
mod fetch;
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
        fetch::register(&mut ctx, Arc::clone(&sandbox));
        loader::register_require(&mut ctx, Arc::clone(&sandbox));
        let console_output = console::register(&mut ctx);

        Self {
            ctx,
            console_output,
            sandbox,
        }
    }

    /// Registers `func` as a synchronous JavaScript global named `name`.
    ///
    /// JS arguments are converted to strings via `display()` and passed to `func`.
    /// The return value is converted back to a JS string, or a JS `Error` is thrown
    /// if `func` returns `Err`.
    pub fn register_func(
        &mut self,
        name: &str,
        func: Box<dyn Fn(Vec<String>) -> Result<String, String> + 'static>,
    ) {
        let native = NativeFunction::from_copy_closure_with_captures(
            |_this, args, captures, ctx| {
                let str_args: Vec<String> = args
                    .iter()
                    .map(|v| {
                        v.to_string(ctx)
                            .map(|s| s.to_std_string_lossy())
                            .unwrap_or_else(|_| v.display().to_string())
                    })
                    .collect();
                (captures.func)(str_args)
                    .map(|s| JsValue::from(js_string!(s.as_str())))
                    .map_err(|e| JsNativeError::error().with_message(e).into())
            },
            UserFuncCaptures { func },
        );
        self.ctx
            .register_global_callable(js_string!(name), 0, native)
            .expect("failed to register global function");
    }

    /// Evaluates `code` and returns the result along with any captured console output.
    ///
    /// After evaluation, the job queue is drained so that Promises and `async/await`
    /// settle before returning. Eval errors take priority over job errors.
    pub fn eval(&mut self, code: &str) -> ToolResult<JsOutput> {
        let eval_result = self.ctx.eval(Source::from_bytes(code.as_bytes()));
        // Always drain the job queue — even on eval error, pending microtasks
        // should be flushed to keep the context in a consistent state.
        let job_result = self.ctx.run_jobs();
        let console = self.console_output.borrow_mut().drain(..).collect();

        let value = eval_result.map_err(|e| ToolError::JsError(e.to_string()))?;
        job_result.map_err(|e| ToolError::JsError(e.to_string()))?;

        Ok(JsOutput {
            value: value.display().to_string(),
            console,
        })
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
    boa_engine::gc::empty_trace!();
}

/// Captures for user-provided native functions exposed via `expose_js_func`.
///
/// The boxed closure contains no GC-managed values, so trace is a no-op.
struct UserFuncCaptures {
    func: Box<dyn Fn(Vec<String>) -> Result<String, String> + 'static>,
}

impl boa_engine::gc::Finalize for UserFuncCaptures {}

// SAFETY: The closure holds no GC-managed objects; nothing to trace.
unsafe impl boa_engine::gc::Trace for UserFuncCaptures {
    boa_engine::gc::empty_trace!();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::JsContext;
    use crate::Sandbox;
    use std::sync::Arc;

    fn ctx() -> JsContext {
        JsContext::new(Arc::new(Sandbox::new()))
    }

    #[test]
    fn register_func_callable_from_js() {
        let mut ctx = ctx();
        ctx.register_func(
            "double",
            Box::new(|args| {
                let n: i64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(0);
                Ok((n * 2).to_string())
            }),
        );
        assert_eq!(ctx.eval("double(21)").unwrap().value, "\"42\"");
    }

    #[test]
    fn register_func_error_becomes_js_error() {
        let mut ctx = ctx();
        ctx.register_func(
            "fail",
            Box::new(|_| Err("something went wrong".to_string())),
        );
        let err = ctx.eval("fail()").unwrap_err().to_string();
        assert!(err.contains("something went wrong"), "got: {err}");
    }

    #[test]
    fn jobs() {
        let mut ctx = ctx();

        // Promise .then() callbacks are settled before eval() returns.
        ctx.eval("let x = 0; Promise.resolve(1).then(v => { x = v; });")
            .unwrap();
        assert_eq!(ctx.eval("x").unwrap().value, "1", ".then callback ran");

        // async/await inside an IIFE resolves through the job queue.
        ctx.eval(
            r#"
            let y;
            (async () => { y = await Promise.resolve("done"); })();
        "#,
        )
        .unwrap();
        assert_eq!(
            ctx.eval("y").unwrap().value,
            r#""done""#,
            "async/await settled"
        );

        // console.log inside an async callback is captured.
        let out = ctx
            .eval(r#"(async () => { console.log("async log"); })();"#)
            .unwrap();
        assert_eq!(
            out.console,
            vec!["async log"],
            "console captured from async"
        );
    }
}

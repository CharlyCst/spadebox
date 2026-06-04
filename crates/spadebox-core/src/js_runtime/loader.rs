use std::{cell::RefCell, collections::HashMap, io::Read, path::PathBuf, rc::Rc, sync::Arc};

use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsString, JsValue, Module, NativeFunction, Source,
    builtins::promise::PromiseState,
    js_string,
    module::{ModuleLoader, Referrer, SyntheticModuleInitializer, resolve_module_specifier},
    property::PropertyKey,
};

use super::files;
use crate::{AsArc, Sandbox, tool_utils};

// ————————————————————————————————— Types —————————————————————————————————— //

/// Module loader that resolves built-in SpadeBox modules (`fs`, `node:fs`) and
/// ES module files from the sandbox filesystem.
pub(super) struct SpadeboxModuleLoader {
    pub sandbox: Arc<Sandbox>,
    module_cache: RefCell<HashMap<PathBuf, Module>>,
}

/// The closure captures for the `require` native function implementation.
struct RequireCaptures {
    loader: Rc<SpadeboxModuleLoader>,
}

// SAFETY: We reach through the `Rc<SpadeboxModuleLoader>` to trace each `Module` in the
// cache. `Module` implements `Trace`, so the GC correctly accounts for these references
// and keeps modules alive as long as `RequireCaptures` (and thus the require closure) is
// reachable. No `Gc<T>` cycles are introduced: `Module`s in the cache do not reference
// back to `RequireCaptures`.
unsafe impl boa_engine::gc::Trace for RequireCaptures {
    boa_engine::gc::custom_trace!(this, mark, {
        for module in this.loader.module_cache.borrow().values() {
            mark(module);
        }
    });
}

impl boa_engine::gc::Finalize for RequireCaptures {}

// ————————————————————————— Loader Implementation —————————————————————————— //

impl SpadeboxModuleLoader {
    pub fn new(sandbox: impl AsArc<Sandbox>) -> Self {
        let sandbox = sandbox.as_arc();
        Self {
            sandbox,
            module_cache: RefCell::new(HashMap::new()),
        }
    }
}

impl ModuleLoader for SpadeboxModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let spec = specifier.to_std_string_lossy();

        // Built-in modules take priority over file resolution.
        // Split into two statements so the RefMut from borrow_mut() is dropped before
        // into_es_module takes its own borrow (if let extends scrutinee temporaries).
        let builtin = self.builtin_module_value(&spec, &mut context.borrow_mut());
        if let Some(value) = builtin {
            return into_es_module(value, &mut context.borrow_mut());
        }

        // Resolve the specifier relative to the referrer, producing a sandbox-root-relative path.
        // `base = None` keeps all resolved paths relative. We then strip any leading `/` with
        // normalize_path before handing to cap-std, which enforces the RESOLVE_BENEATH boundary.
        let resolved = {
            let raw = resolve_module_specifier(
                None,
                &specifier,
                referrer.path(),
                &mut context.borrow_mut(),
            )?;
            let raw_str = raw.to_string_lossy();
            PathBuf::from(tool_utils::normalize_path(&raw_str))
        };

        self.load_module(resolved, &spec, &mut context.borrow_mut())
    }
}

impl SpadeboxModuleLoader {
    /// Inserts a pre-parsed module into the cache under its normalized path.
    ///
    /// Called by `eval_module` before `load_link_evaluate` so that circular imports
    /// back to the entry module resolve to the same instance instead of loading a
    /// second copy.
    pub(super) fn insert_in_cache(&self, path: &std::path::Path, module: Module) {
        let key = PathBuf::from(tool_utils::normalize_path(&path.to_string_lossy()));
        self.module_cache.borrow_mut().insert(key, module);
    }

    /// Returns the `JsValue` for a built-in module specifier, or `None` if not a built-in.
    fn builtin_module_value(&self, spec: &str, ctx: &mut Context) -> Option<JsValue> {
        match spec {
            "fs" | "node:fs" => Some(JsValue::from(files::build_fs_object(
                ctx,
                Arc::clone(&self.sandbox),
            ))),
            _ => None,
        }
    }

    /// Returns a module for `path`: the cached instance if available, otherwise reads,
    /// parses, and caches it.
    fn load_module(&self, path: PathBuf, spec: &str, ctx: &mut Context) -> JsResult<Module> {
        if let Some(module) = self.module_cache.borrow().get(&path) {
            return Ok(module.clone());
        }
        let content = self.read_from_sandbox(&path, spec)?;
        let module = Module::parse(
            Source::from_bytes(content.as_bytes()).with_path(&path),
            None,
            ctx,
        )
        .map_err(|e| {
            JsNativeError::syntax()
                .with_message(format!("failed to parse module '{spec}'"))
                .with_cause(e)
        })?;
        self.module_cache.borrow_mut().insert(path, module.clone());
        Ok(module)
    }

    fn read_from_sandbox(&self, path: &PathBuf, spec: &str) -> JsResult<String> {
        if !self.sandbox.fs_is_enabled() {
            return Err(JsNativeError::error()
                .with_message("file system access is not enabled")
                .into());
        }

        let files = self.sandbox.files.read().unwrap();
        let root = files.root.as_ref().ok_or_else(|| -> JsError {
            JsNativeError::error()
                .with_message("no file system root configured")
                .into()
        })?;

        let mut file = root.open(path).map_err(|e| -> JsError {
            JsNativeError::error()
                .with_message(format!("Cannot find module '{spec}': {e}"))
                .into()
        })?;

        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| -> JsError {
            JsNativeError::error()
                .with_message(format!("failed to read module '{spec}': {e}"))
                .into()
        })?;

        Ok(content)
    }
}

/// Wraps any JS object into a synthetic ES module.
///
/// The object's own string-keyed properties become named exports; the object
/// itself is the default export.
fn into_es_module(value: JsValue, context: &mut Context) -> JsResult<Module> {
    let obj = value.as_object().expect("module value must be a JsObject");

    let mut export_names = vec![js_string!("default")];
    for key in obj.own_property_keys(context)? {
        if let PropertyKey::String(name) = key {
            export_names.push(name);
        }
    }

    Ok(Module::synthetic(
        &export_names,
        SyntheticModuleInitializer::from_copy_closure_with_captures(
            |module, value, ctx| {
                module.set_export(&js_string!("default"), value.clone())?;
                let obj = value.as_object().expect("module value must be a JsObject");
                for key in obj.own_property_keys(ctx)? {
                    if let PropertyKey::String(name) = key {
                        let value = obj.get(name.clone(), ctx)?;
                        module.set_export(&name, value)?;
                    }
                }
                Ok(())
            },
            value,
        ),
        None,
        None,
        context,
    ))
}

/// Injects a CommonJS-style `require` shim.
///
/// Built-in modules (e.g. `node:fs`) are resolved to their object representations.
/// File paths are loaded from the sandbox and evaluated as CJS (no ES module syntax).
/// Paths are normalized by stripping leading `./` and `/`; cap-std enforces the
/// sandbox boundary for anything that reaches the filesystem.
pub(super) fn register_require(ctx: &mut Context, loader: Rc<SpadeboxModuleLoader>) {
    ctx.register_global_builtin_callable(
        js_string!("require"),
        1,
        NativeFunction::from_copy_closure_with_captures(
            |_this, args, captures, ctx| {
                let spec = args
                    .get(0)
                    .and_then(|v| v.as_string())
                    .ok_or_else(|| {
                        JsNativeError::typ().with_message("require argument must be a string")
                    })?
                    .to_std_string_lossy();

                if let Some(value) = captures.loader.builtin_module_value(&spec, ctx) {
                    return Ok(value);
                }

                let path_str = spec.strip_prefix("./").unwrap_or(&spec);
                let path_str = tool_utils::normalize_path(path_str);
                let path = PathBuf::from(path_str);
                require_file(&captures.loader, path, &spec, ctx)
            },
            RequireCaptures { loader },
        ),
    )
    .unwrap();
}

/// Loads a file as an ES module and returns its namespace object.
///
/// Delegates cache lookup, reading, and parsing to `load_module`, then drives
/// module initialization via `load_link_evaluate` + `run_jobs`. Calling
/// `load_link_evaluate` on an already-evaluated module is idempotent (ES spec
/// §16.2.1.5.2). `run_jobs` is safe here because Boa releases all internal
/// borrows before invoking a `NativeFunction`, so there is no `RefCell`
/// re-entrancy.
fn require_file(
    loader: &SpadeboxModuleLoader,
    path: PathBuf,
    spec: &str,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let module = loader.load_module(path, spec, ctx)?;
    let promise = module.load_link_evaluate(ctx);
    ctx.run_jobs()
        .map_err(|e| -> JsError { JsNativeError::error().with_message(e.to_string()).into() })?;

    match promise.state() {
        PromiseState::Fulfilled(_) => Ok(module.namespace(ctx).into()),
        PromiseState::Rejected(reason) => Err(JsNativeError::error()
            .with_message(reason.display().to_string())
            .into()),
        PromiseState::Pending => Err(JsNativeError::error()
            .with_message(format!("module '{spec}' did not finish loading"))
            .into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use super::super::JsContext;
    use crate::Sandbox;

    fn setup() -> (JsContext, TempDir) {
        let dir = TempDir::new().unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();
        let ctx = JsContext::new(sandbox);
        (ctx, dir)
    }

    #[test]
    fn require() {
        let (mut ctx, _dir) = setup();
        // require('fs') returns an object with all fs functions
        ctx.eval(r#"const fs = require('fs')"#).unwrap();
        ctx.eval(r#"fs.writeFileSync("a.txt", "hi")"#).unwrap();
        assert_eq!(
            ctx.eval(r#"fs.readFileSync("a.txt")"#).unwrap().value,
            r#""hi""#
        );
        // node:fs prefix and destructuring work too
        ctx.eval(r#"const { readFileSync, writeFileSync } = require('node:fs')"#)
            .unwrap();
        ctx.eval(r#"writeFileSync("b.txt", "hello")"#).unwrap();
        assert_eq!(
            ctx.eval(r#"readFileSync("b.txt")"#).unwrap().value,
            r#""hello""#
        );
        // unknown modules throw
        assert!(
            ctx.eval(r#"require('os')"#)
                .unwrap_err()
                .to_string()
                .contains("JS error")
        );
    }

    #[test]
    fn require_file() {
        let (mut ctx, dir) = setup();
        std::fs::write(
            dir.path().join("utils.js"),
            "export function double(x) { return x * 2; }",
        )
        .unwrap();

        // bare path
        ctx.eval(r#"const u = require('utils.js');"#).unwrap();
        assert_eq!(ctx.eval("u.double(21)").unwrap().value, "42");

        // ./ prefix is stripped
        ctx.eval(r#"const v = require('./utils.js');"#).unwrap();
        assert_eq!(ctx.eval("v.double(10)").unwrap().value, "20");
    }
}

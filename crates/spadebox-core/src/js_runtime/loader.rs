use std::{cell::RefCell, rc::Rc, sync::Arc};

use boa_engine::{
    Context, JsNativeError, JsResult, JsString, JsValue, Module, NativeFunction, js_string,
    module::{ModuleLoader, Referrer, SyntheticModuleInitializer},
    property::PropertyKey,
};

use super::{SandboxCaptures, files};
use crate::Sandbox;

/// Module loader that resolves built-in SpadeBox modules (`fs`, `node:fs`).
pub(super) struct SpadeboxModuleLoader {
    pub sandbox: Arc<Sandbox>,
}

impl ModuleLoader for SpadeboxModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        _referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let spec = specifier.to_std_string_lossy();
        let value = resolve(
            spec.as_str(),
            Arc::clone(&self.sandbox),
            &mut context.borrow_mut(),
        )?;
        into_es_module(value, &mut context.borrow_mut())
    }
}

/// Resolves a module specifier to its JS value, or errors if the module is unknown.
fn resolve(spec: &str, sandbox: Arc<Sandbox>, ctx: &mut Context) -> JsResult<JsValue> {
    match spec {
        "fs" | "node:fs" => Ok(JsValue::from(files::build_fs_object(ctx, sandbox))),
        _ => Err(JsNativeError::error()
            .with_message(format!("Cannot find module '{spec}'"))
            .into()),
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
            |module, fs, ctx| {
                module.set_export(&js_string!("default"), fs.clone())?;
                let obj = fs.as_object().expect("module value must be a JsObject");
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

/// Injects a CommonJS-style `require` shim that resolves built-in modules.
pub(super) fn register_require(ctx: &mut Context, sandbox: Arc<Sandbox>) {
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

                resolve(spec.as_str(), Arc::clone(&captures.sandbox), ctx)
            },
            SandboxCaptures { sandbox },
        ),
    )
    .unwrap();
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
}

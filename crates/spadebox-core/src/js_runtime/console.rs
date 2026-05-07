use std::{cell::RefCell, rc::Rc};

use boa_engine::{
    Context, JsResult, JsValue, NativeFunction, js_string, object::ObjectInitializer,
    property::Attribute,
};

pub(super) type ConsoleBuffer = Rc<RefCell<Vec<String>>>;

/// Captures for a single console method: the shared output buffer and the level prefix.
///
/// `Rc<RefCell<Vec<String>>>` holds no GC-managed values, so all trace methods are no-ops.
#[derive(Clone)]
struct ConsoleCaptures {
    output: ConsoleBuffer,
    prefix: Option<&'static str>,
}

impl boa_engine::gc::Finalize for ConsoleCaptures {}

// SAFETY: `Rc<RefCell<Vec<String>>>` holds no GC-managed objects; nothing to trace.
unsafe impl boa_engine::gc::Trace for ConsoleCaptures {
    boa_engine::gc::empty_trace!();
}

fn console_method(
    _this: &JsValue,
    args: &[JsValue],
    captures: &ConsoleCaptures,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let msg = formatter(args, ctx);
    let line = match captures.prefix {
        Some(p) => format!("[{p}] {msg}"),
        None => msg,
    };
    captures.output.borrow_mut().push(line);
    Ok(JsValue::undefined())
}

/// Formats console arguments into a single string, handling `%s`, `%d`, `%i`, `%f`, `%o` specifiers.
fn formatter(args: &[JsValue], ctx: &mut Context) -> String {
    match args {
        [] => String::new(),
        [val] => val_to_string(val),
        [fmt_val, rest @ ..] => {
            let fmt = val_to_string(fmt_val);
            let mut result = String::new();
            let mut arg_iter = rest.iter();
            let mut chars = fmt.chars();

            while let Some(c) = chars.next() {
                if c != '%' {
                    result.push(c);
                    continue;
                }
                match chars.next() {
                    Some('s') => match arg_iter.next() {
                        Some(arg) => result.push_str(&val_to_string(arg)),
                        None => result.push_str("%s"),
                    },
                    Some('d') | Some('i') => match arg_iter.next() {
                        Some(arg) => {
                            let n = arg.to_number(ctx).unwrap_or(f64::NAN);
                            result.push_str(&(n as i64).to_string());
                        }
                        None => result.push_str("%d"),
                    },
                    Some('f') => match arg_iter.next() {
                        Some(arg) => {
                            let n = arg.to_number(ctx).unwrap_or(f64::NAN);
                            result.push_str(&format!("{n:.6}"));
                        }
                        None => result.push_str("%f"),
                    },
                    Some('o') | Some('O') => match arg_iter.next() {
                        Some(arg) => result.push_str(&arg.display().to_string()),
                        None => result.push_str("%o"),
                    },
                    Some('%') => result.push('%'),
                    Some(other) => {
                        result.push('%');
                        result.push(other);
                    }
                    None => result.push('%'),
                }
            }

            for remaining in arg_iter {
                result.push(' ');
                result.push_str(&val_to_string(remaining));
            }
            result
        }
    }
}

fn val_to_string(val: &JsValue) -> String {
    if let Some(s) = val.as_string() {
        s.to_std_string_lossy()
    } else {
        val.display().to_string()
    }
}

/// Registers the global `console` object and returns a handle to its output buffer.
///
/// Drain the buffer after each `eval` to collect captured output.
pub(super) fn register(ctx: &mut Context) -> ConsoleBuffer {
    let output: ConsoleBuffer = Rc::new(RefCell::new(Vec::new()));

    let cap = |prefix| ConsoleCaptures {
        output: Rc::clone(&output),
        prefix,
    };
    let console = ObjectInitializer::new(ctx)
        .function(
            NativeFunction::from_copy_closure_with_captures(console_method, cap(None)),
            js_string!("log"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(console_method, cap(Some("info"))),
            js_string!("info"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(console_method, cap(Some("debug"))),
            js_string!("debug"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(console_method, cap(Some("warn"))),
            js_string!("warn"),
            0,
        )
        .function(
            NativeFunction::from_copy_closure_with_captures(console_method, cap(Some("error"))),
            js_string!("error"),
            0,
        )
        .build();

    ctx.register_global_property(
        js_string!("console"),
        console,
        Attribute::WRITABLE | Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
    )
    .unwrap();

    output
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{Sandbox, js_runtime::JsContext};

    fn ctx() -> JsContext {
        JsContext::new(Arc::new(Sandbox::new()))
    }

    #[test]
    fn log_levels() {
        let mut ctx = ctx();

        // log: no prefix, returns undefined
        let out = ctx.eval(r#"console.log("hello")"#).unwrap();
        assert_eq!(out.console, vec!["hello"], "log");
        assert_eq!(out.value, "undefined", "log returns undefined");

        // warn / error: prefixed
        let out = ctx
            .eval(r#"console.warn("careful"); console.error("oops")"#)
            .unwrap();
        assert_eq!(
            out.console,
            vec!["[warn] careful", "[error] oops"],
            "warn/error"
        );

        // info / debug: prefixed
        let out = ctx
            .eval(r#"console.info("fyi"); console.debug("verbose")"#)
            .unwrap();
        assert_eq!(
            out.console,
            vec!["[info] fyi", "[debug] verbose"],
            "info/debug"
        );

        // no console call: empty, value is the expression result
        let out = ctx.eval("1 + 1").unwrap();
        assert!(out.console.is_empty(), "no console call");
        assert_eq!(out.value, "2", "expression value");

        // buffer is drained between evals
        ctx.eval(r#"console.log("first")"#).unwrap();
        let out = ctx.eval(r#"console.log("second")"#).unwrap();
        assert_eq!(out.console, vec!["second"], "buffer drained between evals");
    }

    #[test]
    fn formatter() {
        let mut ctx = ctx();

        // multiple args without specifiers are space-joined
        let out = ctx.eval(r#"console.log("a", "b", "c")"#).unwrap();
        assert_eq!(out.console, vec!["a b c"], "space-join");

        // %s substitution
        let out = ctx.eval(r#"console.log("hello %s!", "world")"#).unwrap();
        assert_eq!(out.console, vec!["hello world!"], "%s");

        // %d substitution
        let out = ctx.eval(r#"console.log("count: %d", 7)"#).unwrap();
        assert_eq!(out.console, vec!["count: 7"], "%d");

        // %f substitution (6 decimal places)
        let out = ctx.eval(r#"console.log("pi: %f", 3.14159)"#).unwrap();
        assert_eq!(out.console, vec!["pi: 3.141590"], "%f");

        // mixed specifiers
        let out = ctx.eval(r#"console.log("x=%d y=%s", 42, "hi")"#).unwrap();
        assert_eq!(out.console, vec!["x=42 y=hi"], "mixed specifiers");

        // single arg: no substitution, %% is verbatim
        let out = ctx.eval(r#"console.log("100%%")"#).unwrap();
        assert_eq!(out.console, vec!["100%%"], "%% single arg verbatim");

        // with extra args: format string active, %% becomes %
        let out = ctx.eval(r#"console.log("%d%%", 100)"#).unwrap();
        assert_eq!(out.console, vec!["100%"], "%% escape");
    }
}

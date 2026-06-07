use std::ffi::{CStr, CString, c_char};
use std::sync::{Arc, OnceLock};
use spadebox_core::{Sandbox, call_tool, enabled_tools};

static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn rt() -> &'static tokio::runtime::Runtime {
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

pub struct Sb {
    sandbox: Arc<Sandbox>,
}

#[unsafe(no_mangle)]
pub extern "C" fn sb_create() -> *mut Sb {
    Box::into_raw(Box::new(Sb {
        sandbox: Arc::new(Sandbox::new()),
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_destroy(sb: *mut Sb) {
    if !sb.is_null() {
        drop(unsafe { Box::from_raw(sb) });
    }
}

/// Returns NULL on success, or an error string that the caller must free with sb_free_str.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_enable_files(sb: *mut Sb, path: *const c_char) -> *mut c_char {
    let sb = unsafe { &*sb };
    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return to_cstr("invalid UTF-8 path"),
    };
    match sb.sandbox.enable_fs(path) {
        Ok(_) => std::ptr::null_mut(),
        Err(e) => to_cstr(&e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_enable_http(sb: *mut Sb) {
    let sb = unsafe { &*sb };
    sb.sandbox.enable_http();
}

/// Returns a JSON array of tool definitions. Caller must free with sb_free_str.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_tools_json(sb: *const Sb) -> *mut c_char {
    let sb = unsafe { &*sb };
    let tools: Vec<serde_json::Value> = enabled_tools(&sb.sandbox)
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.schema,
            })
        })
        .collect();
    to_cstr(&serde_json::to_string(&tools).expect("JSON is infallible"))
}

/// Returns JSON `{"isError": bool, "output": string}`. Caller must free with sb_free_str.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_call_tool(
    sb: *const Sb,
    name: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let sb = unsafe { &*sb };
    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => return tool_result(true, "invalid UTF-8 tool name"),
    };
    let params = match unsafe { CStr::from_ptr(params_json) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => return tool_result(true, "invalid UTF-8 params"),
    };
    let result = rt().block_on(call_tool(sb.sandbox.clone(), &name, params));
    match result {
        Err(e) => tool_result(true, &e),
        Ok(Ok(out)) => tool_result(false, &out),
        Ok(Err(e)) => tool_result(true, &e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_free_str(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

fn to_cstr(s: &str) -> *mut c_char {
    // Replace any embedded NULs so CString::new never fails.
    let safe = s.replace('\0', "\u{FFFD}");
    CString::new(safe).expect("no NUL after replacement").into_raw()
}

fn tool_result(is_error: bool, output: &str) -> *mut c_char {
    to_cstr(&serde_json::json!({"isError": is_error, "output": output}).to_string())
}

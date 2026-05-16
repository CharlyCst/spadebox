use spadebox::SpadeBox;

// --- Builder ---

#[test]
fn builder() {
    let dir = tempfile::TempDir::new().unwrap();
    let sb = SpadeBox::new()
        .enable_files(dir.path())
        .unwrap()
        .enable_http()
        .set_user_agent("test-agent/1.0")
        .allow("api.example.com", &["GET", "POST"])
        .unwrap();
    let names: Vec<_> = sb.tools().into_iter().map(|t| t.name).collect();
    assert!(names.contains(&"read_file".to_owned()));
    assert!(names.contains(&"fetch".to_owned()));

    // Unknown verb is rejected
    let err = SpadeBox::new()
        .enable_http()
        .allow("api.example.com", &["DANCE"]);
    assert!(err.is_err());
}

// --- File operations ---

#[tokio::test]
async fn files() {
    let dir = tempfile::TempDir::new().unwrap();
    let sb = SpadeBox::new().enable_files(dir.path()).unwrap();

    // write + read round-trip
    sb.write_file("hello.txt", Some("hello world"), false)
        .await
        .unwrap();
    let content = sb.read_file("hello.txt", None, None, None).await.unwrap();
    assert_eq!(content, "hello world");

    // edit replaces a string
    sb.write_file("greet.txt", Some("hello world"), false)
        .await
        .unwrap();
    sb.edit_file("greet.txt", "world", "spadebox", false)
        .await
        .unwrap();
    let content = sb.read_file("greet.txt", None, None, None).await.unwrap();
    assert_eq!(content, "hello spadebox");

    // edit with replace_all replaces every occurrence
    sb.write_file("rep.txt", Some("a a a"), false)
        .await
        .unwrap();
    sb.edit_file("rep.txt", "a", "b", true).await.unwrap();
    let content = sb.read_file("rep.txt", None, None, None).await.unwrap();
    assert_eq!(content, "b b b");

    // missing file errors with "not found"
    let err = sb
        .read_file("nope.txt", None, None, None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not found"),
        "unexpected error: {err}"
    );

    // path traversal is rejected
    let err = sb
        .read_file("../etc/passwd", None, None, None)
        .await
        .unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("escape") || msg.contains("permission"),
        "unexpected error: {err}"
    );
}

// --- Grep ---

#[tokio::test]
async fn grep() {
    let dir = tempfile::TempDir::new().unwrap();
    let sb = SpadeBox::new().enable_files(dir.path()).unwrap();

    sb.write_file(
        "src.ts",
        Some("const x = 1\nconst y = 2\nconst z = 3\n"),
        false,
    )
    .await
    .unwrap();
    sb.write_file("note.txt", Some("const needle = 1\n"), false)
        .await
        .unwrap();

    // matches include file:line and exclude non-matching lines
    let result = sb.grep("const y", None, 0).await.unwrap();
    assert!(
        result.contains("src.ts:2"),
        "expected file:line in result: {result}"
    );
    assert!(result.contains("const y = 2"));
    assert!(!result.contains("const x"));

    // glob restricts search to matching file types
    sb.write_file("code.ts", Some("const needle = 1\n"), false)
        .await
        .unwrap();
    let result = sb.grep("needle", Some("**/*.ts"), 0).await.unwrap();
    assert!(result.contains("code.ts"));
    assert!(!result.contains("note.txt"));

    // no matches returns a sentinel message
    let result = sb.grep("xyzzy", None, 0).await.unwrap();
    assert_eq!(result, "No matches found.");
}

// --- JS REPL ---

#[tokio::test]
async fn js_repl() {
    let sb = SpadeBox::new().enable_js();

    // basic evaluation
    let result = sb.js_repl("1 + 1").await.unwrap();
    assert_eq!(result, "2");

    // session is persistent across calls
    sb.js_repl("let x = 42;").await.unwrap();
    let result = sb.js_repl("x").await.unwrap();
    assert_eq!(result, "42");

    // JS errors are surfaced
    let err = sb.js_repl("throw new Error('oops')").await.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("js error"),
        "unexpected: {err}"
    );
}

// --- expose_js_func ---

#[tokio::test]
async fn expose_js_func() {
    let sb = SpadeBox::new().enable_js();

    sb.expose_js_func("double", ["n"], |args| {
        let n = args["n"].as_i64().unwrap_or(0);
        Ok(serde_json::Value::Number((n * 2).into()))
    })
    .unwrap();

    // callable from the REPL
    let result = sb.js_repl("double(21)").await.unwrap();
    assert_eq!(result, "42");
}

#[tokio::test]
async fn expose_js_func_string_return() {
    let sb = SpadeBox::new().enable_js();

    sb.expose_js_func("greet", ["name"], |args| {
        let name = args["name"].as_str().unwrap_or("").to_owned();
        Ok(serde_json::Value::String(format!("hello, {name}")))
    })
    .unwrap();

    let result = sb.js_repl("greet('world')").await.unwrap();
    assert_eq!(result, r#""hello, world""#);
}

#[tokio::test]
async fn expose_js_func_error_surfaces_as_js_error() {
    let sb = SpadeBox::new().enable_js();

    sb.expose_js_func("boom", std::iter::empty::<&str>(), |_| {
        Err("intentional failure".to_owned())
    })
    .unwrap();

    let err = sb
        .js_repl("try { boom() } catch(e) { e.message }")
        .await
        .unwrap();
    assert!(
        err.contains("intentional failure"),
        "unexpected result: {err}"
    );
}

#[tokio::test]
async fn expose_js_func_persists_across_repl_calls() {
    let sb = SpadeBox::new().enable_js();

    sb.expose_js_func("add", ["a", "b"], |args| {
        let a = args["a"].as_i64().unwrap_or(0);
        let b = args["b"].as_i64().unwrap_or(0);
        Ok(serde_json::Value::Number((a + b).into()))
    })
    .unwrap();

    sb.js_repl("let sum = add(3, 4);").await.unwrap();
    let result = sb.js_repl("sum").await.unwrap();
    assert_eq!(result, "7");
}

#[tokio::test]
async fn expose_js_func_available_in_js_exec() {
    let dir = tempfile::TempDir::new().unwrap();
    let sb = SpadeBox::new()
        .enable_js()
        .enable_files(dir.path())
        .unwrap();

    sb.expose_js_func("triple", ["n"], |args| {
        let n = args["n"].as_i64().unwrap_or(0);
        Ok(serde_json::Value::Number((n * 3).into()))
    })
    .unwrap();

    std::fs::write(
        dir.path().join("script.js"),
        r#"var r = triple(7); if (r !== 21) throw new Error("got " + r);"#,
    )
    .unwrap();

    sb.js_exec("script.js").await.unwrap();
}

#[tokio::test]
async fn expose_js_func_requires_js_enabled() {
    let sb = SpadeBox::new(); // JS not enabled
    let err = sb.expose_js_func("f", std::iter::empty::<&str>(), |_| {
        Ok(serde_json::Value::Null)
    });
    assert!(err.is_err());
}

// --- call_tool ---

#[tokio::test]
async fn call_tool() {
    let dir = tempfile::TempDir::new().unwrap();
    let sb = SpadeBox::new().enable_files(dir.path()).unwrap();
    sb.write_file("hello.txt", Some("hi from call_tool"), false)
        .await
        .unwrap();

    // successful dispatch returns output with is_error = false
    let result = sb
        .call_tool("read_file", r#"{"path":"hello.txt"}"#)
        .await
        .unwrap();
    assert!(!result.is_error);
    assert_eq!(result.output, "hi from call_tool");

    // tool-level errors set is_error = true instead of returning Err
    let result = sb
        .call_tool("read_file", r#"{"path":"missing.txt"}"#)
        .await
        .unwrap();
    assert!(result.is_error);
    assert!(result.output.to_lowercase().contains("not found"));

    // sandbox escape also surfaces as is_error = true
    let result = sb
        .call_tool("read_file", r#"{"path":"../etc/passwd"}"#)
        .await
        .unwrap();
    assert!(result.is_error);
    let msg = result.output.to_lowercase();
    assert!(
        msg.contains("escape") || msg.contains("permission"),
        "unexpected: {}",
        result.output
    );

    // unknown tool name is a protocol error (Err variant)
    let err = sb.call_tool("no_such_tool", "{}").await.unwrap_err();
    assert!(
        err.to_string().contains("unknown tool"),
        "unexpected: {err}"
    );
}

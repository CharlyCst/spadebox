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

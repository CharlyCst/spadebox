//! The global SpadeBox Tokio runtime.
//!
//! A single multi-thread Tokio runtime owned by SpadeBox, shared by every
//! [`Sandbox`] instance and independent from any runtime the host may be
//! running. It hosts all work that needs an executor: HTTP requests, JS job
//! draining, and `js_exec` evaluation. Plain blocking file I/O (read, write,
//! glob, …) instead runs inline on the calling thread — it is short enough
//! that dispatching would cost more than it saves.
//!
//! As a result no tool requires the host to provide an async runtime — Tokio,
//! other async runtimes, and fully synchronous code all work. The runtime also
//! gives the JS event loop a place to dispatch `Send` futures (e.g. fetches)
//! so they make progress on other threads while the engine thread is busy.
//!
//! The runtime is initialized lazily on first use rather than at `Sandbox`
//! creation: this keeps process-wide thread spawning out of import/constructor
//! paths, which matters for hosts that fork after initialization (e.g. Python
//! `multiprocessing`).
//!
//! [`Sandbox`]: crate::Sandbox

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

/// Returns a handle to the global SpadeBox runtime, initializing it on first use.
pub fn handle() -> &'static Handle {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_multi_thread()
                .thread_name("spadebox")
                .enable_all()
                .build()
                .expect("failed to build the SpadeBox tokio runtime")
        })
        .handle()
}

/// Runs a future to completion on the global SpadeBox runtime, blocking the
/// current thread until it resolves.
///
/// Intended for synchronous hosts (e.g. the Python bindings). Must not be
/// called from within an async context: like [`Handle::block_on`], it panics
/// if the current thread is already driving an async executor.
pub fn block_on<F: Future>(future: F) -> F::Output {
    handle().block_on(future)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::tools::{ReadFileTool, ReadParams, Tool};
    use crate::{Sandbox, ToolError};
    use std::sync::Arc;

    /// Tools must work without any host runtime: a plain `#[test]` has no
    /// ambient Tokio context, so this passes only if all async work runs on
    /// the global SpadeBox runtime.
    #[test]
    fn tools_run_without_host_runtime() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello").unwrap();
        let sandbox = Arc::new(Sandbox::new());
        sandbox.enable_fs(dir.path()).unwrap();

        let content = super::block_on(ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "hello.txt".into(),
                offset: None,
                limit: None,
                max_bytes: None,
            },
        ))
        .unwrap();
        assert_eq!(content, "hello");

        let err = super::block_on(ReadFileTool::run(
            &sandbox,
            ReadParams {
                path: "missing.txt".into(),
                offset: None,
                limit: None,
                max_bytes: None,
            },
        ))
        .unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }
}

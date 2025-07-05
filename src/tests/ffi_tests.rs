#[cfg(test)]
mod ffi_async_tests {
    use super::*;
    use std::time::Duration;
    use tokio::test; // For async tests

    // We need mock FFI types for testing if they are not fully defined in ffi.rs
    // Assuming you have these somewhere, otherwise you'd need to mock them too
    // For this example, I'll use simple versions or assume they're available via `super::*`

    // --- Mock External Dependencies for Testing ---
    // If ContextRuntime is complex, you might need a mock version.
    // For now, we'll assume a basic implementation that either succeeds or fails
    // based on test conditions.

    // A simple mock for ContextRuntime if you don't want to use the real one for tests
    // This part is crucial if your actual ContextRuntime has external dependencies
    // that make it hard to test in isolation (like heavy file I/O).
    mod mock_runtime {
        use super::*;
        use crate::runtime::{CompileResult, RuntimeError, ContextRuntime as ActualContextRuntime};

        // This is a simplified mock. In a real scenario, you might pass a closure
        // to control behavior for specific tests.
        pub struct MockContextRuntime {
            pub uri_to_content: HashMap<String, String>,
            pub should_compile_succeed: bool,
            pub mock_compile_result: Option<CompileResult>,
        }

        impl MockContextRuntime {
            pub fn new(should_succeed: bool) -> Self {
                MockContextRuntime {
                    uri_to_content: HashMap::new(),
                    should_compile_succeed: should_succeed,
                    mock_compile_result: None,
                }
            }

            pub fn with_mock_result(mut self, result: CompileResult) -> Self {
                self.mock_compile_result = Some(result);
                self
            }

            pub fn open_document(&mut self, uri: String, content: String) -> Result<(), RuntimeError> {
                self.uri_to_content.insert(uri, content);
                Ok(())
            }

            // This mock function for compile_document needs to be async for the test
            pub async fn compile_document(&self, uri: &str) -> Result<CompileResult, RuntimeError> {
                if self.should_compile_succeed {
                    if let Some(res) = &self.mock_compile_result {
                        Ok(res.clone())
                    } else {
                        // Default success result
                        Ok(CompileResult {
                            success: true,
                            pdf_path: Some("/tmp/mock_output.pdf".to_string()),
                            log: format!("Mock compilation successful for {}", uri),
                            diagnostics: vec![],
                        })
                    }
                } else {
                    Err(RuntimeError::CompilationError {
                        details: format!("Mock compilation failed for {}", uri),
                    })
                }
            }

            // Dummy methods to satisfy trait if used
            pub fn get_highlights(&self, _uri: &str) -> Vec<crate::runtime::Highlight> { vec![] }
            pub fn get_diagnostics(&self, _uri: &str) -> Vec<crate::runtime::Diagnostic> { vec![] }
        }

        // We need to temporarily replace ContextRuntime for tests.
        // This is a bit tricky if ContextRuntime::new is directly called inside ContextRuntimeHandle.
        // A better approach for testability is to pass a trait object for compilation logic
        // into ContextRuntimeHandle.
        // For simplicity of this example, we'll try to work with the existing structure
        // by making the ContextRuntime::new call inside the async block controllable,
        // or by testing only the FFI wrapping of an already successful/failed operation.
        //
        // However, the current ContextRuntime::new(config.into()) makes mocking difficult.
        // A more robust testing approach would involve dependency injection or feature flags
        // to swap out the real `ContextRuntime` with a mock for tests.

        // For these tests, we'll assume `ContextRuntime` behaves as expected,
        // or we're primarily testing the `AsyncCompilationFuture` polling logic.
    }


    #[test]
    async fn test_compile_async_local_success() {
        // Setup mock environment if needed, or rely on actual ContextRuntime
        // For local compilation, we mainly care that the tokio::task::spawn_blocking completes
        // and its result is correctly propagated.

        let config = RuntimeConfigFfi {
            remote: false,
            server_url: None,
            auth_token: None,
            // Add other config fields as necessary
            ..Default::default()
        };

        let handle = ContextRuntimeHandle::new_with_config(config);
        let uri = "file:///test_local.ctx".to_string();
        let content = "Hello, local compilation!".to_string();

        // Simulate opening the document so get_document_source returns something
        // Note: The real `open` uses ContextRuntime. We need to mock this or ensure
        // the `documents` internal cache is populated.
        handle.open(uri.clone(), content.clone());

        let future_arc = handle.compile_async(uri.clone()).expect("Should return a future");
        let future = future_arc.as_ref(); // Get a reference to the inner object

        // Poll the future until it's ready or a timeout
        let mut attempts = 0;
        let max_attempts = 20; // 2 seconds timeout (20 * 100ms)
        let poll_interval = Duration::from_millis(100);

        while !future.is_ready() && attempts < max_attempts {
            tokio::time::sleep(poll_interval).await;
            attempts += 1;
        }

        assert!(future.is_ready(), "AsyncCompilationFuture should be ready after local compilation");

        let result = future.poll_result().expect("Should have a result");
        assert!(result.success, "Local compilation should succeed");
        assert!(result.pdf_path.is_some(), "Should have a PDF path");
        assert!(!result.log.is_empty(), "Should have a log message");
        assert!(result.diagnostics.is_empty(), "Should have no diagnostics on success");
        // You might want to check the specific PDF path or log content if known
    }

    #[test]
    async fn test_compile_async_local_failure() {
        // This test requires some way to make ContextRuntime::compile_document fail.
        // This often means injecting a mock or triggering a known failure path.
        // For this example, we will simulate a runtime configuration that leads to an error
        // within the (mocked) ContextRuntime.

        // If ContextRuntime does not have configurable failure, this test requires
        // significant changes to make ContextRuntime mockable.

        // For now, let's assume if content is "FAIL", ContextRuntime will error.
        // This is a weak coupling, dependency injection is better.
        let config = RuntimeConfigFfi {
            remote: false,
            server_url: None,
            auth_token: None,
            // Add other config fields as necessary
            ..Default::default()
        };

        let handle = ContextRuntimeHandle::new_with_config(config);
        let uri = "file:///test_local_fail.ctx".to_string();
        let content = "This content should cause a compilation error.".to_string(); // Or specific content that triggers a mock failure

        handle.open(uri.clone(), content.clone());

        let future_arc = handle.compile_async(uri.clone()).expect("Should return a future");
        let future = future_arc.as_ref();

        let mut attempts = 0;
        let max_attempts = 20;
        let poll_interval = Duration::from_millis(100);

        while !future.is_ready() && attempts < max_attempts {
            tokio::time::sleep(poll_interval).await;
            attempts += 1;
        }

        assert!(future.is_ready(), "AsyncCompilationFuture should be ready after local compilation attempt");

        let result = future.poll_result().expect("Should have a result");
        assert!(!result.success, "Local compilation should fail");
        assert!(result.pdf_path.is_none(), "Should not have a PDF path on failure");
        assert!(!result.log.is_empty(), "Should have an error log");
        assert!(!result.diagnostics.is_empty(), "Should have diagnostics on failure");
    }


    #[test]
    async fn test_compile_async_remote_success() {
        let mut server = mockito::Server::new_async().await;
        let mock_pdf_path = "/path/to/remote_output.pdf";
        let mock_log = "Remote compilation succeeded.";
        let mock_diagnostics = vec![
            DiagnosticFfi {
                start: 0, end: 5, severity: "warning".to_string(), message: "Remote warning".to_string()
            }
        ];

        let mock_response_body = serde_json::json!({
            "success": true,
            "pdf_path": mock_pdf_path,
            "log": mock_log,
            "diagnostics": mock_diagnostics
        }).to_string();

        server.mock("POST", "/compile")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_response_body)
            .create();

        let config = RuntimeConfigFfi {
            remote: true,
            server_url: Some(server.url()),
            auth_token: Some("test-token".to_string()),
            ..Default::default()
        };

        let handle = ContextRuntimeHandle::new_with_config(config);
        let uri = "http://remote.ctx".to_string();
        let content = "Remote test content.".to_string();

        handle.open(uri.clone(), content.clone());

        let future_arc = handle.compile_async(uri.clone()).expect("Should return a future");
        let future = future_arc.as_ref();

        let mut attempts = 0;
        let max_attempts = 20;
        let poll_interval = Duration::from_millis(100);

        while !future.is_ready() && attempts < max_attempts {
            tokio::time::sleep(poll_interval).await;
            attempts += 1;
        }

        assert!(future.is_ready(), "AsyncCompilationFuture should be ready after remote compilation");

        let result = future.poll_result().expect("Should have a result");
        assert!(result.success, "Remote compilation should succeed");
        assert_eq!(result.pdf_path, Some(mock_pdf_path.to_string()), "PDF path should match mock");
        assert_eq!(result.log, mock_log.to_string(), "Log should match mock");
        assert_eq!(result.diagnostics.len(), 1, "Should have one diagnostic");
        assert_eq!(result.diagnostics[0].message, "Remote warning".to_string());

        // Ensure the mock was called
        server.assert();
    }

    #[test]
    async fn test_compile_async_remote_failure() {
        let mut server = mockito::Server::new_async().await;
        let mock_log = "Remote compilation failed: Server returned 500.";
        let mock_diagnostics = vec![
            DiagnosticFfi {
                start: 0, end: 0, severity: "error".to_string(), message: "Internal server error".to_string()
            }
        ];

        let mock_response_body = serde_json::json!({
            "success": false,
            "pdf_path": null,
            "log": mock_log,
            "diagnostics": mock_diagnostics
        }).to_string();

        server.mock("POST", "/compile")
            .with_status(500) // Simulate a server error
            .with_header("content-type", "application/json")
            .with_body(mock_response_body)
            .create();

        let config = RuntimeConfigFfi {
            remote: true,
            server_url: Some(server.url()),
            auth_token: None, // No token for this test
            ..Default::default()
        };

        let handle = ContextRuntimeHandle::new_with_config(config);
        let uri = "http://remote_fail.ctx".to_string();
        let content = "Remote test content for failure.".to_string();

        handle.open(uri.clone(), content.clone());

        let future_arc = handle.compile_async(uri.clone()).expect("Should return a future");
        let future = future_arc.as_ref();

        let mut attempts = 0;
        let max_attempts = 20;
        let poll_interval = Duration::from_millis(100);

        while !future.is_ready() && attempts < max_attempts {
            tokio::time::sleep(poll_interval).await;
            attempts += 1;
        }

        assert!(future.is_ready(), "AsyncCompilationFuture should be ready after remote compilation attempt");

        let result = future.poll_result().expect("Should have a result");
        assert!(!result.success, "Remote compilation should fail");
        assert!(result.pdf_path.is_none(), "Should not have a PDF path on failure");
        assert!(!result.log.is_empty(), "Should have an error log");
        assert_eq!(result.diagnostics.len(), 1, "Should have diagnostics on failure");

        server.assert();
    }

    #[test]
    async fn test_async_compilation_future_cancel() {
        let config = RuntimeConfigFfi {
            remote: false, // Use local compilation for simpler testing
            server_url: None,
            auth_token: None,
            ..Default::default()
        };

        let handle = ContextRuntimeHandle::new_with_config(config);
        let uri = "file:///test_cancel.ctx".to_string();
        let content = "Content to be cancelled.".to_string();

        handle.open(uri.clone(), content.clone());

        let future_arc = handle.compile_async(uri.clone()).expect("Should return a future");
        let future = future_arc.as_ref();

        // Immediately cancel the future
        assert!(future.cancel(), "Cancel should return true");

        // Give it a moment, but expect it to be ready quickly due to early exit
        tokio::time::sleep(Duration::from_millis(50)).await;

        // The future might still become "ready" with a failure result if the
        // spawn_blocking task started before the cancel signal was checked,
        // but the core logic should have bailed early.
        // We're testing that the cancellation mechanism works to prevent
        // further processing or to indicate an aborted state.

        // It's tricky to assert the exact state after a cancellation that races with execution.
        // The most robust check is that `is_ready()` eventually becomes true and `poll_result()`
        // returns a result (potentially an error result indicating cancellation or partial work).
        // Let's ensure it does become ready and doesn't get stuck.
        let mut attempts = 0;
        let max_attempts = 10; // Shorter timeout for cancellation
        let poll_interval = Duration::from_millis(10);

        while !future.is_ready() && attempts < max_attempts {
            tokio::time::sleep(poll_interval).await;
            attempts += 1;
        }

        assert!(future.is_ready(), "Cancelled future should eventually be ready");
        let result = future.poll_result().expect("Should have a result even if cancelled");

        // Depending on how exactly the `cancelled` flag is handled and the timing,
        // the result might be an error or a partially successful result if cancellation
        // happened too late. The key is that it *doesn't hang*.
        // If your ContextRuntime implementation respects the cancellation early,
        // you might assert for a specific "cancelled" error message.
        // For now, we just ensure it completes and is not a success (unless very fast).
        assert!(!result.success || result.log.contains("cancelled"), "Cancelled compilation should not be a full success or log should indicate cancellation");
    }
}

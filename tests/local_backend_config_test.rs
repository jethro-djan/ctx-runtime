use context_runtime::runtime::{RuntimeConfig, ContextRuntime};
use std::path::{PathBuf, Path};
use tempfile::TempDir;
use tokio::fs;
use tokio::process::Command;
use std::env;

use std::os::unix::fs::PermissionsExt;
use temp_env;

// Helper function to create a dummy executable (simulates mtxrun)
async fn create_dummy_executable(dir: &Path, name: &str) -> PathBuf {
    let dummy_path = dir.join(name);
    // On Unix, ensure it's executable. On Windows, just creating a file is enough for Command::new.
    #[cfg(unix)]
    {
        fs::write(&dummy_path, "#!/bin/sh\necho 'dummy mtxrun output'\nexit 0")
            .await
            .expect("Failed to write dummy executable");
        // Make it executable
        let mut perms = fs::metadata(&dummy_path).await.unwrap().permissions();
        perms.set_mode(0o755); // rwxr-xr-x
        fs::set_permissions(&dummy_path, perms)
            .await
            .expect("Failed to set executable permissions");
    }
    #[cfg(windows)]
    {
        fs::write(&dummy_path, "@echo off\necho dummy mtxrun output\nexit 0")
            .await
            .expect("Failed to write dummy executable");
    }
    dummy_path
}

// Helper to create a dummy mtxrun.exe if on Windows for PATH testing
// Windows doesn't easily let you add a temp dir to PATH for just one process.
// For Windows, often easier to put a dummy exe in a temp dir and explicitly pass its path.
#[cfg(windows)]
async fn create_dummy_mtxrun_in_path(temp_dir: &TempDir) -> PathBuf {
    let dummy_path = temp_dir.path().join("mtxrun.exe");
    fs::write(&dummy_path, "@echo off\necho dummy mtxrun output\nexit 0")
        .await
        .expect("Failed to write dummy mtxrun.exe");
    dummy_path
}

#[tokio::test]
async fn test_local_backend_with_explicit_mtxrun_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let mtxrun_path = create_dummy_executable(temp_dir.path(), "my_mtxrun").await;

    let config = RuntimeConfig {
        remote: false,
        server_url: None,
        auth_token: None,
        local_executable: Some(mtxrun_path.clone()), // Explicit absolute path
    };

    let result = tokio::spawn(async move {
        let config_clone = config.clone();
        tokio::task::spawn_blocking(move || {
            let runtime = ContextRuntime::new(config_clone);
            let uri = "test_explicit_path.tex".to_string();
            let content = r"\starttext Test Explicit \stoptext".to_string();

            runtime.open_document(uri.clone(), content).expect("Failed to open document");

            // Create a new tokio runtime to block on the async compile_document call
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            let compile_result = rt.block_on(runtime.compile_document(&uri)).expect("Failed to compile");

            assert!(compile_result.success);
            assert!(compile_result.log.contains("dummy mtxrun output")); // Check dummy output
            assert!(compile_result.pdf_path.is_some());
            assert!(compile_result.errors.is_empty());
            assert!(compile_result.warnings.is_empty());
            Ok::<(), String>(())
        }).await.expect("Blocking task failed")
    }).await;
    assert!(result.is_ok(), "Test failed: {:?}", result.err());
}

#[tokio::test]
async fn test_local_backend_with_non_existent_explicit_path() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let non_existent_path = temp_dir.path().join("non_existent_mtxrun"); // A path that doesn't exist

    let config = RuntimeConfig {
        remote: false,
        server_url: None,
        auth_token: None,
        local_executable: Some(non_existent_path.clone()), // Non-existent path
    };

    let result = tokio::spawn(async move {
        let config_clone = config.clone();
        tokio::task::spawn_blocking(move || {
            // We expect LocalBackend::new to fail here
            let runtime_creation_result = std::panic::catch_unwind(|| {
                ContextRuntime::new(config_clone)
            });

            assert!(runtime_creation_result.is_err(), "Runtime creation should have panicked");
            let panic_payload = runtime_creation_result.unwrap_err();
            let err_msg = panic_payload.downcast_ref::<String>()
                                        .expect("Panic payload not a String");

            // The error message from BackendError::Unavailable
            assert!(err_msg.contains("Configured mtxrun executable not found"));
            assert!(err_msg.contains(&non_existent_path.to_string_lossy().to_string()));

            Ok::<(), String>(())
        }).await.expect("Blocking task failed")
    }).await;
    assert!(result.is_ok(), "Test failed: {:?}", result.err());
}


#[tokio::test]
async fn test_local_backend_with_path_lookup_success() {
    // This test is harder because we need to modify the PATH for the test process.
    // temp-env crate is good for this, but it requires a careful approach
    // with `tokio::spawn` and the fact that env vars are process-wide.

    // On Unix-like systems, you can inject a temp dir at the start of PATH
    // On Windows, it's generally more difficult to temporarily modify PATH for a subprocess
    // and rely on it, so often explicit paths are preferred for tests.

    // Let's create a dummy mtxrun and put it in a temp directory.
    let temp_dir = TempDir::new().expect("Failed to create temp dir for PATH test");
    let dummy_mtxrun_path = create_dummy_executable(temp_dir.path(), "mtxrun").await;

    // Use `temp-env` to temporarily modify the PATH for this test.
    // This MUST be done carefully to avoid interfering with other tests,
    // especially when using `tokio::spawn`.
    // It's often safer to use a separate test binary for env var manipulation tests.
    // For simplicity, we'll use `temp_env::with_var` which attempts to revert.

    // Because `temp_env::with_var` operates on the current thread's environment,
    // and `tokio::spawn` moves the async block to a new task (which might run
    // on a different thread in a multi-threaded runtime), this pattern can be tricky.
    // The most reliable way for env var tests is often:
    // 1. Ensure `tokio::test` uses a single-threaded runtime if you're not careful (default for `#[tokio::test]`).
    // 2. Or, use `std::process::Command` to launch a *new process* for this test,
    //    setting its environment variables explicitly, rather than trying to
    //    manipulate the current process's environment for concurrent async tests.

    // For demonstration, let's try the `temp_env::with_var` in a single-threaded context.
    // If your `tokio::test` uses the default `current_thread` runtime, this should work.

    temp_env::with_var("PATH", Some(format!("{}:{}", temp_dir.path().to_string_lossy(), env!("PATH"))), || {
        let config = RuntimeConfig {
            remote: false,
            server_url: None,
            auth_token: None,
            local_executable: None, // Rely on PATH lookup
        };

        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let config_clone = config.clone();
                tokio::task::spawn_blocking(move || {
                    let runtime = ContextRuntime::new(config_clone); // Should find it in PATH
                    let uri = "test_path_success.tex".to_string();
                    let content = r"\starttext Test Path Success \stoptext".to_string();

                    runtime.open_document(uri.clone(), content).expect("Failed to open document");

                    // Create a new tokio runtime to block on the async compile_document call
                    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
                    let compile_result = rt.block_on(runtime.compile_document(&uri)).expect("Failed to compile");

                    assert!(compile_result.success);
                    assert!(compile_result.log.contains("dummy mtxrun output")); // Check dummy output
                    assert!(compile_result.pdf_path.is_some());
                    assert!(compile_result.errors.is_empty());
                    assert!(compile_result.warnings.is_empty());
                    Ok::<(), String>(())
                }).await.expect("Blocking task failed")
            });
        assert!(result.is_ok(), "Test failed: {:?}", result.err());
    });
}


#[tokio::test]
async fn test_local_backend_with_path_lookup_failure() {
    // This test aims to confirm that if mtxrun is NOT in PATH and not explicitly provided,
    // `LocalBackend::new` or `compile` fails as expected.

    // To ensure mtxrun is *not* found, we can temporarily clear/manipulate PATH.
    // This is also tricky with `tokio::spawn` and process-wide env.

    // Let's create a temporary directory and ensure it's the *only* thing in PATH.
    // This guarantees mtxrun won't be found unless we put it there.
    let temp_dir = TempDir::new().expect("Failed to create temp dir for PATH failure test");
    let empty_path_dir = temp_dir.path().join("empty_path");
    fs::create_dir(&empty_path_dir).await.unwrap();

    temp_env::with_var("PATH", Some(empty_path_dir.to_string_lossy().to_string()), || {
        let config = RuntimeConfig {
            remote: false,
            server_url: None,
            auth_token: None,
            local_executable: None, // Rely on PATH lookup, which should fail
        };

        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let config_clone = config.clone();
                tokio::task::spawn_blocking(move || {
                    // Expect LocalBackend::new to fail with BackendError::Unavailable
                    let runtime_creation_result = std::panic::catch_unwind(|| {
                        ContextRuntime::new(config_clone)
                    });

                    assert!(runtime_creation_result.is_err(), "Runtime creation should have panicked");
                    let panic_payload = runtime_creation_result.unwrap_err();
                    let err_msg = panic_payload.downcast_ref::<String>()
                                                .expect("Panic payload not a String");

                    // Check for the error message from `which` crate (or similar)
                    assert!(err_msg.contains("mtxrun executable not found in system PATH"));

                    Ok::<(), String>(())
                }).await.expect("Blocking task failed")
            });
        assert!(result.is_ok(), "Test failed: {:?}", result.err());
    });
}

// Add necessary dev-dependencies in your Cargo.toml:
/*
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full"] } # Or just "macros", "rt-current-thread"
temp-env = "0.3" # For manipulating environment variables in tests
*/

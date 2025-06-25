use context_runtime::runtime::{Runtime, RuntimeError};
use std::fs;

use context_runtime::runtime::ConTeXtCompiler;
use std::path::PathBuf;

#[test]
fn test_context_binary_path() {
    let expected_path = PathBuf::from("/Users/jethro-world/context/tex/texmf-osx-arm64/bin/mtxrun");
    
    let compiler = ConTeXtCompiler::new()
        .expect("Failed to create ConTeXt compiler");
    
    assert_eq!(
        compiler.executable, 
        expected_path,
        "ConTeXt binary not at expected location.\nFound: {:?}\nExpected: {:?}",
        compiler.executable,
        expected_path
    );
}

#[test]
fn test_compile_simple_document() {
    // Initialize logging for tests
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Debug) // Increase log level
        .try_init();
    
    log::info!("Starting compilation test");
    
    let runtime = Runtime::new().expect("Failed to create runtime");
    let content = r#"
        \starttext
        Hello World!
        \stoptext
    "#;
    
    log::debug!("Opening document 'simple.tex'");
    runtime.open_document("simple.tex".into(), content.into())
        .expect("Failed to open document");
    
    log::debug!("Compiling document");
    let result = runtime.compile_document("simple.tex");
    
    match result {
        Ok(result) => {
            log::info!("Compilation result: success={}", result.success);
            log::debug!("Output path: {:?}", result.output_path);
            log::trace!("Full log:\n{}", result.log);
            
            if !result.success {
                panic!("Compilation failed. Log:\n{}", result.log);
            }
            
            if result.output_path.is_none() {
                panic!("No PDF path generated despite successful compilation. Log:\n{}", result.log);
            }
            
            let pdf_path = result.output_path.unwrap();
            log::debug!("Checking PDF at: {:?}", pdf_path);
            
            if !pdf_path.exists() {
                panic!("PDF not found at {:?}. Log:\n{}", pdf_path, result.log);
            }
            
            log::info!("Test passed! PDF found at: {:?}", pdf_path);
        }
        Err(e) => {
            log::error!("Compilation failed: {}", e);
            panic!("Compilation failed: {}", e);
        }
    }
}
#[test]
fn test_compile_error_detection() {
    let runtime = Runtime::new().unwrap();
    let bad_content = r#"
        \starttext
        \unknowncommand
        \stoptext
    "#;
    
    runtime.open_document("error.tex".into(), bad_content.into()).unwrap();
    let result = runtime.compile_document("error.tex").unwrap();
    
    assert!(!result.success, "Compilation should fail");
    assert!(!result.errors.is_empty(), "Should detect errors");
    assert!(result.log.contains("unknowncommand"), "Log should contain error");
}

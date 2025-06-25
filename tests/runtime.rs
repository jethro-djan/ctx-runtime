use context_runtime::runtime::{ConTeXtCompiler, Runtime, RuntimeError};
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn test_compiler_new() {
    let compiler = ConTeXtCompiler::new();
    assert!(compiler.is_ok(), "Compiler should initialize");
}

#[test]
fn test_compiler_with_executable() {
    let temp_dir = tempdir().unwrap();
    let fake_exec = temp_dir.path().join("context");
    std::fs::File::create(&fake_exec).unwrap();
    
    let compiler = ConTeXtCompiler::with_executable(fake_exec);
    assert!(compiler.is_ok());
}

#[test]
fn test_compiler_with_invalid_executable() {
    let compiler = ConTeXtCompiler::with_executable(PathBuf::from("/nonexistent/path"));
    assert!(matches!(compiler, Err(RuntimeError::ParseError(_))));
}

#[test]
fn test_runtime_document_lifecycle() {
    let runtime = Runtime::new().unwrap();
    
    // Test open
    assert!(runtime.open_document("test.tex".into(), "\\starttext\nHello\n\\stoptext".into()).is_ok());
    
    // Test get source
    assert_eq!(runtime.get_document_source("test.tex"), Some("\\starttext\nHello\n\\stoptext".into()));
    
    // Test update
    assert!(runtime.update_document("test.tex".into(), "\\starttext\nUpdated\n\\stoptext".into()).is_ok());
    assert_eq!(runtime.get_document_source("test.tex"), Some("\\starttext\nUpdated\n\\stoptext".into()));
    
    // Test close
    runtime.close_document("test.tex");
    assert!(runtime.get_document_source("test.tex").is_none());
}

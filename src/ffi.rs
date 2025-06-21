use std::sync::Mutex;

use crate::{
    runtime::{Runtime, CompilationResult, RuntimeError},
    highlight::{Highlight, HighlightKind},
    diagnostic::Diagnostic,
};

#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    inner: Mutex<Runtime>,
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Runtime::new()), 
        }
    }

    pub fn open(&self, uri: String, text: String) -> bool {
        let runtime = self.inner.lock().unwrap(); 
        runtime.open_document(uri, text).is_ok()
    }

    pub fn update(&self, uri: String, text: String) -> bool {
        let runtime = self.inner.lock().unwrap(); 
        runtime.open_document(uri, text).is_ok()
    }

    pub fn close(&self, uri: String) {
        let runtime = self.inner.lock().unwrap(); 
        runtime.close_document(&uri);
    }

    pub fn get_document_source(&self, uri: String) -> Option<String> {
        let runtime = self.inner.lock().unwrap(); 
        runtime.get_document_source(&uri).map(|s| s.to_string())
    }

    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        let runtime = self.inner.lock().unwrap(); 
        runtime.get_highlights(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        let runtime = self.inner.lock().unwrap(); 
        runtime.get_diagnostics(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn compile(&self, uri: String) -> CompileResultFfi {
        let runtime = self.inner.lock().unwrap(); 
        runtime.compile_document(&uri).into()
    }
}

#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct CompileResultFfi {
    pub success: bool,
    pub pdf_path: Option<String>,
    pub log: String,
    pub errors: Vec<DiagnosticFfi>,
    pub warnings: Vec<DiagnosticFfi>,
}

impl From<Result<CompilationResult, RuntimeError>> for CompileResultFfi {
    fn from(result: Result<CompilationResult, RuntimeError>) -> Self {
        match result {
            Ok(compilation_result) => compilation_result.into(),
            Err(error) => CompileResultFfi {
                success: false,
                pdf_path: None,
                log: format!("Compilation failed: {:?}", error),
                errors: vec![DiagnosticFfi {
                    range: FfiRange { start: 0, end: 0 },
                    severity: "error".to_string(),
                    message: format!("{:?}", error),
                    source: "runtime".to_string(),
                }],
                warnings: vec![],
            }
        }
    }
}

impl From<CompilationResult> for CompileResultFfi {
    fn from(result: CompilationResult) -> Self {
        CompileResultFfi {
            success: result.success,
            pdf_path: result.output_path.and_then(|p| p.to_str().map(|s| s.to_string())),
            log: result.log,
            errors: result.errors.into_iter().map(|e| DiagnosticFfi {
                range: FfiRange {
                    start: e.column,
                    end: e.column + 1,
                },
                severity: "error".to_string(),
                message: e.message,
                source: e.file,
            }).collect(),
            warnings: result.warnings.into_iter().map(|w| DiagnosticFfi {
                range: FfiRange {
                    start: w.column,
                    end: w.column + 1,
                },
                severity: "warning".to_string(),
                message: w.message,
                source: w.file,
            }).collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HighlightFfi {
    pub range: FfiRange,
    pub kind: String, 
}

impl From<Highlight> for HighlightFfi {
    fn from(h: Highlight) -> Self {
        HighlightFfi {
            range: FfiRange {
                start: h.range.start as u32,
                end: h.range.end as u32,
            },
            kind: h.kind.to_string(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DiagnosticFfi {
    pub range: FfiRange,
    pub severity: String,
    pub message: String,
    pub source: String,
}

impl From<Diagnostic> for DiagnosticFfi {
    fn from(d: Diagnostic) -> Self {
        DiagnosticFfi {
            range: FfiRange {
                start: d.span.start_byte.unwrap_or(0) as u32,
                end: d.span.end_byte.unwrap_or(0) as u32,
            },
            severity: d.severity.to_string(),
            message: d.message,
            source: d.source,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiRange {
    pub start: u32,
    pub end: u32,
}




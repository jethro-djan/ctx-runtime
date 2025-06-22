use crate::{
    runtime::{Runtime, CompilationResult, RuntimeError},
    highlight::{Highlight, HighlightKind},
    diagnostic::Diagnostic,
};

use std::sync::Mutex;

#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    inner: Mutex<Runtime>,
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Self {
        let runtime = Runtime::new().unwrap_or_else(|e| {
            log::error!("Failed to create Runtime: {}", e);
            panic!("Failed to create Runtime: {}", e);
        });
        
        Self {
            inner: Mutex::new(runtime),
        }
    }

    pub fn open(&self, uri: String, text: String) -> bool {
        self.inner.lock().unwrap().open_document(uri, text).is_ok()
    }

    pub fn update(&self, uri: String, text: String) -> bool {
        self.inner.lock().unwrap().open_document(uri, text).is_ok()
    }

    pub fn close(&self, uri: String) {
        self.inner.lock().unwrap().close_document(&uri);
    }

    pub fn get_document_source(&self, uri: String) -> Option<String> {
        self.inner.lock().unwrap().get_document_source(&uri)
    }

    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        self.inner.lock().unwrap()
            .get_highlights(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        self.inner.lock().unwrap()
            .get_diagnostics(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    pub fn compile(&self, uri: String) -> CompileResultFfi {
        self.inner.lock().unwrap()
            .compile_document(&uri)
            .into()
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




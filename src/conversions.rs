use crate::ffi_types::{TextRangeFfi, RuntimeErrorFfi, CompileResultFfi, HighlightFfi, DiagnosticFfi, FfiRange};
use crate::runtime::{RuntimeError, CompilationResult};
use crate::diagnostic::{Diagnostic, DiagnosticSeverity};
use crate::highlight::Highlight;
use rowan::TextRange;

impl From<TextRange> for TextRangeFfi {
    fn from(range: TextRange) -> Self {
        Self {
            start: range.start().into(),
            end: range.end().into(),
        }
    }
}

impl From<RuntimeError> for RuntimeErrorFfi {
    fn from(err: RuntimeError) -> Self {
        match err {
            RuntimeError::DocumentNotFound(uri) => Self::DocumentNotFound { uri },
            RuntimeError::LockPoisoned => Self::LockPoisoned,
            RuntimeError::DocumentAccess(details) => Self::DocumentAccess { details },
            RuntimeError::ParseError(details) => Self::ParseError { details },
            RuntimeError::CompilationError(details) => Self::CompilationError { details },
            RuntimeError::IoError(e) => Self::IoError { 
                details: e.to_string() 
            },
        }
    }
}

impl From<std::io::Error> for RuntimeErrorFfi {
    fn from(err: std::io::Error) -> Self {
        Self::IoError { 
            details: err.to_string() 
        }
    }
}

impl CompileResultFfi {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            pdf_path: None,
            log: message.clone(),
            errors: vec![DiagnosticFfi {
                range: FfiRange { start: 0, end: 0 },
                severity: "error".to_string(),
                message,
                source: "runtime".to_string(),
            }],
            warnings: vec![],
        }
    }
}

impl From<TextRangeFfi> for std::ops::Range<usize> {
    fn from(range: TextRangeFfi) -> Self {
        range.start as usize..range.end as usize
    }
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

impl From<Diagnostic> for DiagnosticFfi {
    fn from(d: Diagnostic) -> Self {
        DiagnosticFfi {
            range: FfiRange {
                start: d.range.start as u32,
                end: d.range.end as u32,
            },
            severity: d.severity.to_string(), // Use the enum's to_string method
            message: d.message,
            source: d.source,
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
                    start: e.column as u32,
                    end: (e.column + 1) as u32,
                },
                severity: "error".to_string(),
                message: e.message,
                source: e.file,
            }).collect(),
            warnings: result.warnings.into_iter().map(|w| DiagnosticFfi {
                range: FfiRange {
                    start: w.column as u32,
                    end: (w.column + 1) as u32,
                },
                severity: "warning".to_string(),
                message: w.message,
                source: w.file,
            }).collect(),
        }
    }
}

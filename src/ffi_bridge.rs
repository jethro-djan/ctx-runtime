use crate::backend_traits::CompilationResult;
use crate::runtime::{RuntimeError, RuntimeConfig};
use crate::diagnostic::Diagnostic;
use crate::highlight::Highlight;
use rowan::TextRange;
use std::path::PathBuf;
use uniffi;

// ============================================================================
// FFI Types
// ============================================================================

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiRange {
    pub start: u32,
    pub end: u32,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct TextRangeFfi {
    pub start: u32,
    pub end: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, uniffi::Record)]
pub struct CompileResultFfi {
    pub success: bool,
    pub pdf_path: Option<String>,
    pub log: String,
    pub diagnostics: Vec<DiagnosticFfi>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum RuntimeErrorFfi {
    DocumentNotFound { uri: String },
    LockPoisoned,
    CompilationError { details: String },
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, uniffi::Record)]
pub struct DiagnosticFfi {
    pub start: u32,
    pub end: u32,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HighlightFfi {
    pub range: FfiRange,
    pub kind: String,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct RuntimeConfigFfi {
    pub remote: bool,
    pub server_url: Option<String>,
    pub auth_token: Option<String>,
    pub local_executable: Option<String>,
}

// ============================================================================
// Conversions: From Rust Types to FFI Types
// ============================================================================

impl From<TextRange> for TextRangeFfi {
    fn from(range: TextRange) -> Self {
        Self {
            start: range.start().into(),
            end: range.end().into(),
        }
    }
}

impl From<TextRange> for FfiRange {
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
            RuntimeError::CompilationError { message, .. } => Self::CompilationError { 
                details: message 
            },
        }
    }
}

impl From<std::io::Error> for RuntimeErrorFfi {
    fn from(err: std::io::Error) -> Self {
        Self::CompilationError { 
            details: format!("IO Error: {}", err)
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
            start: d.range.start as u32,
            end: d.range.end as u32,
            severity: d.severity.to_string(),
            message: d.message,
        }
    }
}

impl From<CompilationResult> for CompileResultFfi {
    fn from(result: CompilationResult) -> Self {
        let mut diagnostics = Vec::new();
        
        // Add errors as diagnostics
        for error in result.errors {
            diagnostics.push(DiagnosticFfi {
                start: error.line as u32,
                end: (error.line + 1) as u32,
                severity: "error".to_string(),
                message: error.message,
            });
        }

        // Add warnings as diagnostics
        for warning in result.warnings {
            diagnostics.push(DiagnosticFfi {
                start: warning.line as u32,
                end: (warning.line + 1) as u32,
                severity: "warning".to_string(),
                message: warning.message,
            });
        }

        CompileResultFfi {
            success: result.success,
            pdf_path: result.pdf_path.and_then(|p| p.to_str().map(|s| s.to_string())),
            log: result.log,
            diagnostics,
        }
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
                diagnostics: vec![DiagnosticFfi {
                    start: 0, 
                    end: 0,
                    severity: "error".to_string(),
                    message: format!("{:?}", error),
                }],
            }
        }
    }
}

impl From<RuntimeConfigFfi> for RuntimeConfig {
    fn from(config: RuntimeConfigFfi) -> Self {
        Self {
            remote: config.remote,
            server_url: config.server_url,
            auth_token: config.auth_token,
            local_executable: config.local_executable.map(PathBuf::from),
        }
    }
}

// ============================================================================
// Conversions: From FFI Types to Rust Types
// ============================================================================

impl From<TextRangeFfi> for std::ops::Range<usize> {
    fn from(range: TextRangeFfi) -> Self {
        range.start as usize..range.end as usize
    }
}

impl From<FfiRange> for std::ops::Range<usize> {
    fn from(range: FfiRange) -> Self {
        range.start as usize..range.end as usize
    }
}

impl Default for RuntimeConfigFfi {
    fn default() -> Self {
        Self {
            remote: true,
            server_url: None,
            auth_token: None,
            local_executable: None,
        }
    }
}

// ============================================================================
// Utility Methods
// ============================================================================

impl CompileResultFfi {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            pdf_path: None,
            log: message.clone(),
            diagnostics: vec![DiagnosticFfi {
                start: 0, 
                end: 0,
                severity: "error".to_string(),
                message,
            }],
        }
    }

    pub fn success(pdf_path: Option<String>, log: String) -> Self {
        Self {
            success: true,
            pdf_path,
            log,
            diagnostics: vec![],
        }
    }

    pub fn errors(&self) -> Vec<&DiagnosticFfi> {
        self.diagnostics.iter().filter(|d| d.severity == "error").collect()
    }

    pub fn warnings(&self) -> Vec<&DiagnosticFfi> {
        self.diagnostics.iter().filter(|d| d.severity == "warning").collect()
    }
}

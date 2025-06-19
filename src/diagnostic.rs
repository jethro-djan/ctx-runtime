use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub span: SourceSpan,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

impl DiagnosticSeverity {
    pub fn to_string(&self) -> String {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub start_byte: Option<u32>,
    pub end_byte: Option<u32>,
}

impl Diagnostic {
    pub fn error(
        line: u32,
        column: u32,
        length: u32,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            span: SourceSpan {
                start_line: line,
                start_col: column,
                end_line: line,
                end_col: column + length,
                start_byte: None,
                end_byte: None,
            },
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            source: source.into(),
        }
    }

    pub fn warning(
        line: u32,
        column: u32,
        length: u32,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            span: SourceSpan {
                start_line: line,
                start_col: column,
                end_line: line,
                end_col: column + length,
                start_byte: None,
                end_byte: None,
            },
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            source: source.into(),
        }
    }
}

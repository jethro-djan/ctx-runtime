use serde::{Serialize, Deserialize};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range<usize>,
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

impl Diagnostic {
    pub fn error(start: usize, length: usize, message: String, source: String) -> Self {
        Self {
            range: start..(start + length),
            severity: DiagnosticSeverity::Error,  // Use the enum variant directly
            message,
            source,
        }
    }

    pub fn warning(start: usize, length: usize, message: String, source: String) -> Self {
        Self {
            range: start..(start + length),
            severity: DiagnosticSeverity::Warning,  // Use the enum variant directly
            message,
            source,
        }
    }
}

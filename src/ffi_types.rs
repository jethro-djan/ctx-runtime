use uniffi;

#[derive(uniffi::Record, Debug, Clone)]
pub struct TextRangeFfi {
    pub start: u32,
    pub end: u32,
}

// #[derive(uniffi::Record, Debug, Clone)]
// pub struct RuntimeErrorFfi {
//     pub kind: String,
//     pub message: String,
// }

#[derive(Debug, Clone, Default, uniffi::Record)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct CompileResultFfi {
    pub success: bool,
    pub pdf_path: Option<String>,
    pub log: String,
    pub errors: Vec<DiagnosticFfi>,
    pub warnings: Vec<DiagnosticFfi>,
}

#[derive(Debug, Clone, uniffi::Record)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct HighlightFfi {
    pub range: FfiRange,
    pub kind: String,
}

#[derive(Debug, Clone, uniffi::Record)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticFfi {
    pub range: FfiRange,
    pub severity: String,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct FfiRange {
    pub start: u32,
    pub end: u32,
}

// #[derive(uniffi::Record)]
// pub struct TextRangeFfi {
//     pub start: usize,
//     pub end: usize,
// }

#[derive(Debug, Clone, uniffi::Enum)]
pub enum RuntimeErrorFfi {
    DocumentNotFound { uri: String },
    LockPoisoned,
    DocumentAccess { details: String },
    ParseError { details: String },
    CompilationError { details: String },
    IoError { details: String },
}

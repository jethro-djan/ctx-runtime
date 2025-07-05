use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use bumpalo::Bump;
use crate::{
    highlight::{Highlight, highlight},
    diagnostic::Diagnostic, // This is your internal Diagnostic struct
    syntax::{SyntaxKind, SyntaxTree},
    parser::parse_text,
};

// Corrected import to match your backend_traits.rs
use crate::backend_traits::{
    BackendError, CompilationBackend, CompilationRequest, CompilationResult,
    LocalBackend, RemoteBackend, CompilationError, 
};

#[derive(Debug)]
pub struct ContextRuntime {
    backend: Arc<RwLock<Box<dyn CompilationBackend>>>,
    config: RuntimeConfig,
    documents: RwLock<HashMap<String, Document>>,
    diagnostics: RwLock<HashMap<String, Vec<Diagnostic>>>, // This is `crate::diagnostic::Diagnostic`
}

// ... Document, RuntimeConfig, Default for RuntimeConfig unchanged ...
#[derive(Debug)]
pub struct Document {
    source: String,
    syntax_tree: SyntaxTree,
    arena: Box<Bump>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub remote: bool,
    pub server_url: Option<String>,
    pub auth_token: Option<String>,
    pub local_executable: Option<PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            remote: true,
            server_url: None,
            auth_token: None,
            local_executable: None,
        }
    }
}


impl ContextRuntime {
    pub fn new_with_backend(config: RuntimeConfig, backend: Box<dyn CompilationBackend>) -> Arc<Self> {
        Arc::new(Self {
            backend: Arc::new(RwLock::new(backend)),
            config,
            documents: RwLock::new(HashMap::new()),
            diagnostics: RwLock::new(HashMap::new()),
        })
    }

    pub fn new(config: RuntimeConfig) -> Arc<Self> {
        let backend = Self::create_backend(&config);
        Self::new_with_backend(config, backend)
    }

    fn create_backend(config: &RuntimeConfig) -> Box<dyn CompilationBackend> {
        if config.remote {
            Box::new(RemoteBackend::new(
                config.server_url.clone().unwrap_or_default(),
                config.auth_token.clone(),
            ))
        } else {
            let local_backend = LocalBackend::new(config.local_executable.clone())
                .expect("Failed to create local backend"); // This unwrap will panic on `BackendError::Unavailable`
            Box::new(local_backend)
        }
    }

    pub fn set_backend(&self, backend: Box<dyn CompilationBackend>) {
        let mut write_guard = self.backend.write().unwrap();
        *write_guard = backend;
    }

    pub fn with_document<F, R>(&self, uri: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Document) -> R
    {
        let docs = self.documents.read().ok()?;
        docs.get(uri).map(f)
    }

    pub fn open_document(&self, uri: String, content: String) -> Result<(), RuntimeError> {
        let arena = Box::new(Bump::new());
        let syntax_tree = parse_text(&content);

        let document = Document {
            source: content,
            syntax_tree,
            arena,
        };

        self.documents.write()
            .map_err(|_| RuntimeError::LockPoisoned)?
            .insert(uri.clone(), document);

        self.update_diagnostics(&uri)?;
        Ok(())
    }

    pub fn update_document(
        &self,
        uri: &str,
        edit_range: std::ops::Range<usize>,
        new_text: &str,
    ) -> Result<(), RuntimeError> {
        let mut documents = self.documents.write()
            .map_err(|_| RuntimeError::LockPoisoned)?;

        if let Some(document) = documents.get_mut(uri) {
            let mut new_source = document.source.clone();
            new_source.replace_range(edit_range.clone(), new_text);

            let new_tree = parse_text(&new_source);

            document.source = new_source;
            document.syntax_tree = new_tree;

            self.update_diagnostics(uri)?;
        }

        Ok(())
    }

    pub fn close_document(&self, uri: &str) {
        self.documents.write().unwrap().remove(uri);
        self.diagnostics.write().unwrap().remove(uri);
    }

    pub fn get_highlights(&self, uri: &str) -> Vec<Highlight> {
        self.with_document(uri, |doc| highlight(&doc.syntax_tree.root()))
            .unwrap_or_default()
    }

    pub fn get_document_source(&self, uri: &str) -> Option<String> {
        self.with_document(uri, |doc| doc.source.clone())
    }

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> { // Uses crate::diagnostic::Diagnostic
        self.diagnostics.read()
            .ok()
            .and_then(|diags| diags.get(uri).cloned())
            .unwrap_or_default()
    }

    fn update_diagnostics(&self, uri: &str) -> Result<(), RuntimeError> {
        let mut diagnostics = Vec::new();

        if let Some(doc) = self.documents.read().unwrap().get(uri) {
            self.collect_syntax_diagnostics(&doc.syntax_tree, &mut diagnostics);
        }

        let mut diag_map = self.diagnostics.write()
            .map_err(|_| RuntimeError::LockPoisoned)?;
        diag_map.insert(uri.to_string(), diagnostics);

        Ok(())
    }

    fn collect_syntax_diagnostics(&self, tree: &SyntaxTree, diagnostics: &mut Vec<Diagnostic>) {
        for node in tree.root().descendants() {
            match node.kind() {
                SyntaxKind::Command => {
                    if let Some(name_token) = node.first_token() {
                        let name = name_token.text().trim_start_matches('\\');
                        if !self.is_known_command(name) {
                            diagnostics.push(Diagnostic::warning( // Uses crate::diagnostic::Diagnostic
                                name_token.text_range().start().into(),
                                name_token.text_range().len().into(),
                                format!("Unknown command: \\{}", name),
                            ));
                        }
                    }
                }
                SyntaxKind::Environment => {
                    if let Some(name_token) = node.first_token() {
                        let name = name_token.text().trim_start_matches(r"\start");
                        if !self.is_known_environment(name) {
                            diagnostics.push(Diagnostic::warning( // Uses crate::diagnostic::Diagnostic
                                name_token.text_range().start().into(),
                                name_token.text_range().len().into(),
                                format!("Unknown environment: {}", name),
                            ));
                        }
                    }
                }
                SyntaxKind::Error => {
                    if let Some(token) = node.first_token() {
                        diagnostics.push(Diagnostic::error( // Uses crate::diagnostic::Diagnostic
                            token.text_range().start().into(),
                            token.text_range().len().into(),
                            "Syntax error".to_string(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn is_known_command(&self, name: &str) -> bool {
        matches!(name,
            "setupbodyfont" | "setuppapersize" | "setupmargins" | "setuphead" |
            "setuplist" | "setupitemize" | "setupenumerate" | "setupdescription" |
            "definefont" | "definecolor" | "definelayout" | "setupcolor" |
            "input" | "component" | "product" | "environment" | "project" |
            "em" | "bf" | "it" | "tt" | "rm" | "sf" | "sc" | "sl" |
            "item" | "head" | "subhead" | "subsubhead" | "title" | "subject" |
            "page" | "blank" | "space" | "par" | "break" | "hfill" | "vfill" |
            "starttext" | "stoptext" | "startdocument" | "stopdocument"
        )
    }

    fn is_known_environment(&self, name: &str) -> bool {
        matches!(name,
            "document" | "text" | "itemize" | "enumerate" | "description" |
            "table" | "tabulate" | "figure" | "float" | "framed" |
            "typing" | "verbatim" | "quote" | "quotation" | "lines" |
            "formula" | "math" | "alignment" | "combinations" | "columns"
        )
    }

    pub async fn compile_document(&self, uri: &str) -> Result<CompilationResult, RuntimeError> {
        let content = self.get_document_source(uri)
            .ok_or(RuntimeError::DocumentNotFound(uri.to_string()))?;

        let backend_guard = self.backend.read().map_err(|_| RuntimeError::LockPoisoned)?;
        let backend = backend_guard.as_ref();

        let compilation_result = backend.compile(CompilationRequest {
            content,
            job_id: uri.to_string(),
        })
        .await
        .map_err(|e: BackendError| { // Explicitly map BackendError to RuntimeError
            match e {
                BackendError::Network(msg) => RuntimeError::Unavailable(format!("Network error: {}", msg)),
                BackendError::Compilation(msg) => RuntimeError::CompilationError {
                    line: 0, // No line/column from generic BackendError::Compilation
                    column: 0,
                    message: msg,
                },
                BackendError::Unavailable(msg) => RuntimeError::Unavailable(format!("Backend unavailable: {}", msg)),
                BackendError::Setup(msg) => RuntimeError::Unavailable(format!("Backend setup error: {}", msg)),
                BackendError::IO(msg) => RuntimeError::CompilationError {
                    line: 0,
                    column: 0,
                    message: format!("IO error during compilation: {}", msg),
                },
            }
        })?; // Apply the mapping and then unwrap

        // If compilation was successful (Backend returned Ok(CompilationResult)),
        // update the diagnostics based on the compilation result
        self.update_compilation_diagnostics(uri, &compilation_result)?;

        Ok(compilation_result)
    }


    fn update_compilation_diagnostics(
        &self,
        uri: &str,
        result: &CompilationResult
    ) -> Result<(), RuntimeError> {
        let mut diag_map = self.diagnostics.write()
            .map_err(|_| RuntimeError::LockPoisoned)?;

        let diagnostics = diag_map.entry(uri.to_string())
            .or_default();

        diagnostics.retain(|d| {
            // Keep syntax diagnostics, remove compilation errors/warnings (if they have a specific tag/source)
            // Or, for now, just append. If you have "source" in Diagnostic, you could filter by source.
            // For this example, let's assume we just append, and the client will handle duplicates if necessary,
            // or you add a `source` field to Diagnostic.
            true // Keep all existing diagnostics. New ones will be added.
        });


        if let Some(document) = self.documents.read().unwrap().get(uri) {
            for error in &result.errors {
                if let Some(offset) = self.line_column_to_offset(&document.source, error.line, error.column) {
                    diagnostics.push(Diagnostic::error( // Uses crate::diagnostic::Diagnostic
                        offset,
                        // FIX: Explicitly cast to usize
                        (error.column.saturating_sub(error.line).max(1)) as usize, // Basic attempt to derive length from start/end if available, otherwise 1
                        error.message.clone(),
                    ));
                }
            }

            for warning in &result.warnings {
                if let Some(offset) = self.line_column_to_offset(&document.source, warning.line, warning.column) {
                    diagnostics.push(Diagnostic::warning( // Uses crate::diagnostic::Diagnostic
                        offset,
                        // FIX: Explicitly cast to usize
                        (warning.column.saturating_sub(warning.line).max(1)) as usize, // Same as above
                        warning.message.clone(),
                    ));
                }
            }
        }

        Ok(())
    }


    fn line_column_to_offset(&self, text: &str, line: u32, column: u32) -> Option<usize> {
        let mut current_line = 1;
        let mut byte_offset_at_start_of_current_line = 0;

        for (byte_idx, char_val) in text.char_indices() {
            if current_line == line {
                // We are on the target line. Now find the column.
                // column is 1-indexed for the user, convert to 0-indexed for string slicing
                let target_char_idx_on_line = (column.saturating_sub(1)) as usize;

                // Iterate over characters on the current line to find the byte offset for the column
                let mut current_char_idx_on_line = 0;
                for (char_byte_idx_in_line, c) in text[byte_offset_at_start_of_current_line..].char_indices() {
                    if current_char_idx_on_line == target_char_idx_on_line {
                        return Some(byte_offset_at_start_of_current_line + char_byte_idx_in_line);
                    }
                    current_char_idx_on_line += 1;
                    // If we hit a newline character, this is the end of the current line
                    if c == '\n' {
                        break;
                    }
                }
                // If column is beyond line length, return the end of the line (or the whole document for simplicity)
                // or None if it's truly out of bounds. For simplicity, let's say the end of the line.
                // A better approach might be to return the last character's offset or None.
                // For now, if we didn't find the exact column on the line, assume end of line segment.
                // This will effectively point to the end of the line if column is too high.
                return Some(byte_offset_at_start_of_current_line + text[byte_offset_at_start_of_current_line..]
                    .find('\n')
                    .unwrap_or(text[byte_offset_at_start_of_current_line..].len()));
            }

            if char_val == '\n' {
                current_line += 1;
                byte_offset_at_start_of_current_line = byte_idx + char_val.len_utf8();
            }
        }

        // Handle the case where the target line is the last line and might not end with a newline
        if current_line == line {
            let target_char_idx_on_line = (column.saturating_sub(1)) as usize;
            let mut current_char_idx_on_line = 0;
            for (char_byte_idx_in_line, _) in text[byte_offset_at_start_of_current_line..].char_indices() {
                if current_char_idx_on_line == target_char_idx_on_line {
                    return Some(byte_offset_at_start_of_current_line + char_byte_idx_in_line);
                }
                current_char_idx_on_line += 1;
            }
            // If the target column is beyond the actual characters on the last line,
            // return the end of the line (which is text.len() if it's the very end).
            return Some(text.len());
        }
        None
    }
}

// ... RuntimeError enum remains the same ...
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Deadlock error")]
    LockPoisoned,
    #[error("Compilation error: line {line:?}, column {column:?}, message{message:?}")]
    CompilationError {
        line: u32,
        column: u32,
        message: String,
    },
    #[error("Document not found: {0}")]
    DocumentNotFound(String),
    #[error("Backend unavailable: {0}")]
    Unavailable(String),
}

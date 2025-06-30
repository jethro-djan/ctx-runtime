use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use bumpalo::Bump;
use crate::{
    highlight::{Highlight, highlight},
    diagnostic::Diagnostic,
    syntax::{SyntaxKind, SyntaxTree},
    parser::parse_text,
};

use crate::backend_traits::*;

pub struct ContextRuntime {
    backend: Arc<RwLock<Box<dyn CompilationBackend>>>,
    config: RuntimeConfig,
    documents: RwLock<HashMap<String, Document>>,
    diagnostics: RwLock<HashMap<String, Vec<Diagnostic>>>,
}

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
    pub fn new(config: RuntimeConfig) -> Arc<Self> {
        let backend = Self::create_backend(&config);

        Arc::new(Self {
            backend: Arc::new(RwLock::new(backend)),
            config,
            documents: RwLock::new(HashMap::new()),
            diagnostics: RwLock::new(HashMap::new()),
        })
    }

    fn create_backend(config: &RuntimeConfig) -> Box<dyn CompilationBackend> {
        if config.remote {
            Box::new(RemoteBackend::new(
                config.server_url.clone().unwrap_or_default(),
                config.auth_token.clone(),
            ))
        } else {
            let local_backend = LocalBackend::new(config.local_executable.clone())
                .expect("Failed to create local backend");
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

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
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
                            diagnostics.push(Diagnostic::warning(
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
                            diagnostics.push(Diagnostic::warning(
                                name_token.text_range().start().into(),
                                name_token.text_range().len().into(),
                                format!("Unknown environment: {}", name),
                            ));
                        }
                    }
                }
                SyntaxKind::Error => {
                    if let Some(token) = node.first_token() {
                        diagnostics.push(Diagnostic::error(
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

        let result = backend.compile(CompilationRequest {
            content,
            job_id: uri.to_string(),
        })
        .await;

        result.map_err(|e| {
            let parsed = backend.as_any().downcast_ref::<LocalBackend>().unwrap().parse_compiler_output(&e.to_string());
            RuntimeError::CompilationError {
                line: parsed.errors.first().map(|e| e.line).unwrap_or(0),
                column: parsed.errors.first().map(|e| e.column).unwrap_or(0),
                message: parsed.errors.first()
                    .map(|e| e.message.clone())
                    .unwrap_or_else(|| e.to_string()),
            }
        })
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
        
        diagnostics.retain(|d| d.message != "compiler");
        
        if let Some(document) = self.documents.read().unwrap().get(uri) {
            for error in &result.errors {
                if let Some(offset) = self.line_column_to_offset(&document.source, error.line, error.column) {
                    diagnostics.push(Diagnostic::error(
                        offset,
                        1,
                        error.message.clone(),
                    ));
                }
            }
            
            for warning in &result.warnings {
                if let Some(offset) = self.line_column_to_offset(&document.source, warning.line, warning.column) {
                    diagnostics.push(Diagnostic::warning(
                        offset,
                        1, 
                        warning.message.clone(),
                    ));
                }
            }
        }
        
        Ok(())
    }

    fn line_column_to_offset(&self, text: &str, line: u32, column: u32) -> Option<usize> {
        let mut current_line = 1;
        let mut current_offset = 0;
        
        for (offset, c) in text.char_indices() {
            if current_line == line as usize {
                let col = column as usize;
                if current_offset + col < offset {
                    return Some(offset + col);
                }
            }
            
            if c == '\n' {
                current_line += 1;
                current_offset = offset;
            }
        }
        
        None
    }
}

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
}

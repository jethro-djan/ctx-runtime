use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::ops::Range;
use std::io::Write;
use bumpalo::Bump;
use async_trait::async_trait;
use crate::{
    highlight::{Highlight, highlight},
    diagnostic::Diagnostic,
    syntax::{SyntaxKind, SyntaxTree},
    parser::parse_text,
};

use crate::backend_traits::*;
use crate::runtime_config::*;

pub struct ContextRuntime {
    backend: Mutex<Arc<dyn CompilationBackend>>,
    config: RuntimeConfig,
    documents: Mutex<HashMap<String, Document>>,
    diagnostics: Mutex<HashMap<String, Vec<Diagnostic>>>,
    active_compilations: Mutex<HashMap<String, CompilationJob>>,
}

struct Document {
    source: String,
    syntax_tree: SyntaxTree,
    arena: Box<Bump>,
}

#[derive(Clone)]
struct CompilationJob {
    id: String,
    uri: String,
    backend_id: String,
}

impl ContextRuntime {
    pub fn new(config: RuntimeConfig) -> Arc<Self> {
        let backend: Arc<dyn CompilationBackend> = if config.is_mobile() {
            let remote_backend = RemoteBackend::new(
                config.remote_endpoint.clone().expect("Mobile requires remote endpoint"), 
                config.auth_token.clone()
            );
            Arc::new(remote_backend)
        } else {
            let local_backend = LocalBackend::new(config.local_executable.clone())
                .expect("Failed to create local backend");
            Arc::new(local_backend)
        };

        Arc::new(Self {
            backend: Mutex::new(backend),
            config,    
            documents: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            active_compilations: Mutex::new(HashMap::new()),
        })
    }

    pub fn with_document<F, R>(&self, uri: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Document) -> R
    {
        let docs = self.documents.lock().ok()?;
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

        self.documents.lock()
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
        let mut documents = self.documents.lock()
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
        self.documents.lock().unwrap().remove(uri);
        self.diagnostics.lock().unwrap().remove(uri);
    }

    pub fn get_highlights(&self, uri: &str) -> Vec<Highlight> {
        self.with_document(uri, |doc| highlight(&doc.syntax_tree.root()))
            .unwrap_or_default()
    }

    pub fn get_document_source(&self, uri: &str) -> Option<String> {
        self.with_document(uri, |doc| doc.source.clone())
    }

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.lock()
            .ok()
            .and_then(|diags| diags.get(uri).cloned())
            .unwrap_or_default()
    }

    fn update_diagnostics(&self, uri: &str) -> Result<(), RuntimeError> {
        let mut diagnostics = Vec::new();
        
        if let Some(doc) = self.documents.lock().unwrap().get(uri) {
            self.collect_syntax_diagnostics(&doc.syntax_tree, &mut diagnostics);
        }
        
        let mut diag_map = self.diagnostics.lock()
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
                                "parser".to_string(),
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
                                "parser".to_string(),
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
                            "parser".to_string(),
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

    fn create_backend(config: &RuntimeConfig) -> Arc<dyn CompilationBackend> {
        if config.is_mobile() {
            Arc::new(RemoteBackend::new(
                config.remote_endpoint.clone().expect("Mobile requires remote endpoint"),
                config.auth_token.clone(),
            ))
        } else {
            let local_backend = LocalBackend::new(config.local_executable.clone())
                .expect("Failed to create local backend");
            Arc::new(local_backend)
        }
    }

    pub async fn compile_document(&self, uri: &str) -> Result<CompilationResult, RuntimeError> {
        let content = self.get_document_source(uri)
            .ok_or(RuntimeError::DocumentNotFound(uri.to_string()))?;
        let backend = self.backend.lock().unwrap().clone();
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
        let mut diag_map = self.diagnostics.lock()
            .map_err(|_| RuntimeError::LockPoisoned)?;
        
        let diagnostics = diag_map.entry(uri.to_string())
            .or_default();
        
        diagnostics.retain(|d| d.source != "compiler");
        
        if let Some(document) = self.documents.lock().unwrap().get(uri) {
            for error in &result.errors {
                if let Some(offset) = self.line_column_to_offset(&document.source, error.line, error.column) {
                    diagnostics.push(Diagnostic::error(
                        offset,
                        1,
                        error.message.clone(),
                        "compiler".to_string(),
                    ));
                }
            }
            
            for warning in &result.warnings {
                if let Some(offset) = self.line_column_to_offset(&document.source, warning.line, warning.column) {
                    diagnostics.push(Diagnostic::warning(
                        offset,
                        1, 
                        warning.message.clone(),
                        "compiler".to_string(),
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




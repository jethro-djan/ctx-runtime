use std::collections::HashMap;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use crate::highlight::{Highlight, highlight};
use crate::diagnostic::Diagnostic;

pub struct Runtime {
    documents: RefCell<HashMap<String, DocumentData>>,
    compiler: ConTeXtCompiler,
    diagnostics: RefCell<HashMap<String, Vec<Diagnostic>>>,
}

struct DocumentData {
    source: String,
}

#[derive(Debug, Clone)]
pub struct CompilationResult {
    pub success: bool,
    pub output_path: Option<PathBuf>,
    pub errors: Vec<CompilationError>,
    pub warnings: Vec<CompilationWarning>,
    pub log: String,
}

#[derive(Debug, Clone)]
pub struct CompilationError {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CompilationWarning {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

pub struct ConTeXtCompiler {
    executable: PathBuf,
    working_dir: PathBuf,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            documents: RefCell::new(HashMap::new()),
            compiler: ConTeXtCompiler::new(),
            diagnostics: RefCell::new(HashMap::new()),
        }
    }

    pub fn open_document(&self, uri: String, content: String) -> Result<(), RuntimeError> {
        self.documents.borrow_mut().insert(uri.clone(), DocumentData {
            source: content,
        });
        self.update_parse_diagnostics(&uri);
        Ok(())
    }

    pub fn update_document(&self, uri: String, content: String) -> Result<(), RuntimeError> {
        self.open_document(uri, content)
    }

    pub fn close_document(&self, uri: &str) {
        self.documents.borrow_mut().remove(uri);
        self.diagnostics.borrow_mut().remove(uri);
    }

    pub fn get_document_source(&self, uri: &str) -> Option<String> {
        self.documents.borrow().get(uri).map(|d| d.source.clone())
    }

    pub fn get_document_ast(&self, uri: &str) -> Option<crate::ast::ConTeXtNode> {
        self.documents.borrow().get(uri).and_then(|d| {
            crate::parser::parse_document(&d.source)
                .map_err(|e| {
                    log::warn!("Failed to parse document {}: {:?}", uri, e);
                    e
                })
                .ok()
        })
    }

    pub fn get_highlights(&self, uri: &str) -> Vec<Highlight> {
        self.get_document_ast(uri)
            .map(|ast| highlight(&ast))
            .unwrap_or_default()
    }

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.borrow()
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    fn update_parse_diagnostics(&self, uri: &str) {
        let mut diagnostics = Vec::new();
        
        if let Some(ast) = self.get_document_ast(uri) {
            self.collect_ast_diagnostics(&ast, &mut diagnostics);
        }
        
        self.diagnostics.borrow_mut().insert(uri.to_string(), diagnostics);
    }

    fn collect_ast_diagnostics(&self, node: &crate::ast::ConTeXtNode, diagnostics: &mut Vec<Diagnostic>) {
        use crate::ast::ConTeXtNode;
        
        match node {
            ConTeXtNode::Command { name, span, .. } => {
                if !self.is_known_command(name) {
                    diagnostics.push(Diagnostic::warning(
                        span.start_line as u32,
                        span.start_col as u32,
                        span.len() as u32,
                        format!("Unknown command: \\{}", name),
                        "parser".to_string(),
                    ));
                }
            }
            ConTeXtNode::StartStop { name, content, span, .. } => {
                if !self.is_known_environment(name) {
                    diagnostics.push(Diagnostic::warning(
                        span.start_line as u32,
                        span.start_col as u32,
                        span.len() as u32,
                        format!("Unknown environment: {}", name),
                        "parser".to_string(),
                    ));
                }
                
                for child in content {
                    self.collect_ast_diagnostics(child, diagnostics);
                }
            }
            ConTeXtNode::Document { preamble, body } => {
                for node in preamble.iter().chain(body.iter()) {
                    self.collect_ast_diagnostics(node, diagnostics);
                }
            }
            _ => {}
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

    pub fn compile_document(&self, uri: &str) -> Result<CompilationResult, RuntimeError> {
        let content = self.get_document_source(uri)
            .ok_or_else(|| RuntimeError::CompilationError("Document not found".to_string()))?;
        
        let temp_file = self.create_temp_file(uri, &content)?;
        let result = self.compiler.compile(&temp_file)?;
        
        // Clean up temp file
        let _ = std::fs::remove_file(&temp_file);
        
        // Update diagnostics with compilation errors
        self.update_compilation_diagnostics(uri, &result);
        
        Ok(result)
    }

    fn create_temp_file(&self, uri: &str, content: &str) -> Result<PathBuf, RuntimeError> {
        use std::io::Write;
        
        let temp_dir = std::env::temp_dir();
        let file_name = format!("context_temp_{}.tex", 
            uri.replace(['/', '\\', ':'], "_"));
        let temp_path = temp_dir.join(file_name);
        
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.flush()?;
        
        Ok(temp_path)
    }

    fn update_compilation_diagnostics(&self, uri: &str, result: &CompilationResult) {
        let mut diagnostics = self.diagnostics.borrow()
            .get(uri)
            .cloned()
            .unwrap_or_default();
        
        diagnostics.retain(|d| d.source != "compiler");
        
        for error in &result.errors {
            diagnostics.push(Diagnostic::error(
                error.line,
                error.column,
                1,
                error.message.clone(),
                "compiler".to_string(),
            ));
        }
        
        for warning in &result.warnings {
            diagnostics.push(Diagnostic::warning(
                warning.line,
                warning.column,
                1,
                warning.message.clone(),
                "compiler".to_string(),
            ));
        }
        
        self.diagnostics.borrow_mut().insert(uri.to_string(), diagnostics);
    }
}

impl ConTeXtCompiler {
    pub fn new() -> Self {
        ConTeXtCompiler {
            executable: PathBuf::from("context"),
            working_dir: std::env::temp_dir(),
        }
    }

    pub fn compile(&self, input_file: &Path) -> Result<CompilationResult, RuntimeError> {
        use std::process::Command;
        
        let output = Command::new(&self.executable)
            .arg("--batchmode")
            .arg("--nonstopmode")
            .arg(input_file)
            .current_dir(&self.working_dir)
            .output()
            .map_err(|e| RuntimeError::CompilationError(format!("Failed to execute ConTeXt: {}", e)))?;

        let log = String::from_utf8_lossy(&output.stdout).to_string();
        let success = output.status.success();
        
        let (errors, warnings) = self.parse_log(&log);
        
        let output_path = if success {
            let mut path = input_file.to_path_buf();
            path.set_extension("pdf");
            Some(path)
        } else {
            None
        };

        Ok(CompilationResult {
            success,
            output_path,
            errors,
            warnings,
            log,
        })
    }

    fn parse_log(&self, log: &str) -> (Vec<CompilationError>, Vec<CompilationWarning>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        for line in log.lines() {
            if line.contains("! ") {
                errors.push(CompilationError {
                    file: String::new(),
                    line: 0,
                    column: 0,
                    message: line.to_string(),
                });
            } else if line.contains("Warning:") {
                warnings.push(CompilationWarning {
                    file: String::new(),
                    line: 0,
                    column: 0,
                    message: line.to_string(),
                });
            }
        }
        
        (errors, warnings)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Compilation error: {0}")]
    CompilationError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

use std::collections::HashMap;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use crate::highlight::{Highlight, highlight};
use crate::diagnostic::Diagnostic;
use std::process::Command;

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
    pub executable: PathBuf,
    working_dir: PathBuf,
}

impl Runtime {
    pub fn new() -> Result<Self, RuntimeError> {
        let compiler = ConTeXtCompiler::new()?;
        
        Ok(Runtime {
            documents: RefCell::new(HashMap::new()),
            compiler,
            diagnostics: RefCell::new(HashMap::new()),
        })
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
    pub fn new() -> Result<Self, RuntimeError> {
        let executable = which::which("mtxrun")
            // .or_else(|_| which::which("texlua"))
            // .or_else(|_| which::which("context-lmtx"))
            .map_err(|_| RuntimeError::ParseError("Could not find ConTeXt executable".to_string()))?;

        Ok(ConTeXtCompiler {
            executable,
            working_dir: std::env::current_dir()?,
        })
    }

    pub fn with_executable(executable: PathBuf) -> Result<Self, RuntimeError> {
        if !executable.exists() {
            return Err(RuntimeError::ParseError("Specified executable does not exist".to_string()));
        }

        Ok(ConTeXtCompiler {
            executable,
            working_dir: std::env::current_dir()?,
        })
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Result<Self, RuntimeError> {
        if !dir.exists() {
            return Err(RuntimeError::ParseError("Working directory does not exist".to_string()));
        }
        self.working_dir = dir;
        Ok(self)
    }

pub fn compile(&self, input_file: &Path) -> Result<CompilationResult, RuntimeError> {
    if !input_file.exists() {
        return Err(RuntimeError::ParseError("Input file does not exist".to_string()));
    }
    
    let input_file = input_file.canonicalize()?;
    log::debug!("Compiling: {:?}", input_file);
    
    let output = Command::new(&self.executable)
        .arg("--script")
        .arg("context")
        .arg("--run")
        .arg("--synctex")
        .arg("--nonstopmode")
        .arg("--purgeall")
        .arg("--pattern={.log,.tex.tuc}")
        .arg(&input_file)
        .current_dir(&self.working_dir)
        .output()
        .map_err(|e| RuntimeError::CompilationError(format!("Failed to execute ConTeXt: {}", e)))?;
    
    let log = String::from_utf8_lossy(&output.stdout).to_string();
    let success = output.status.success();
    
    let (errors, warnings) = self.parse_log(&log);
    
    let output_path = if success {
        self.find_output_pdf(&input_file)
    } else {
        None
    };
    
    // Clean up any auxiliary files created during compilation
    self.cleanup_auxiliary_files(&input_file);
    
    Ok(CompilationResult {
        success,
        output_path,
        errors,
        warnings,
        log,
    })
}

fn find_output_pdf(&self, input_file: &Path) -> Option<PathBuf> {
    // Strategy 1: Same location as input file
    let mut path = input_file.to_path_buf();
    path.set_extension("pdf");
    if path.exists() {
        return Some(path);
    }
    
    // Strategy 2: Working directory with input file stem
    let pdf_name = input_file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string()) + ".pdf";
    
    let possible_paths = vec![
        self.working_dir.join(&pdf_name),
        self.working_dir.join(input_file.file_name().unwrap_or_default()).with_extension("pdf"),
    ];
    
    for path in possible_paths {
        if path.exists() {
            return Some(path);
        }
    }
    
    None
}

fn cleanup_auxiliary_files(&self, input_file: &Path) {
    let base_name = input_file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "temp".to_string());
    
    // Common ConTeXt auxiliary file extensions
    let aux_extensions = ["aux", "log", "fls", "fdb_latexmk", "synctex.gz", "tuc"];
    
    for ext in &aux_extensions {
        let aux_file = self.working_dir.join(format!("{}.{}", base_name, ext));
        if aux_file.exists() {
            let _ = std::fs::remove_file(&aux_file);
        }
        
        // Also check in the input file's directory
        if let Some(input_dir) = input_file.parent() {
            let aux_file = input_dir.join(format!("{}.{}", base_name, ext));
            if aux_file.exists() {
                let _ = std::fs::remove_file(&aux_file);
            }
        }
    }
}

    fn parse_log(&self, log: &str) -> (Vec<CompilationError>, Vec<CompilationWarning>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        for line in log.lines() {
            if line.starts_with("! ") {
                // Error line
                let message = line.trim_start_matches("! ").to_string();
                errors.push(CompilationError {
                    file: String::new(),
                    line: 0,
                    column: 0,
                    message,
                });
            } else if let Some(pos) = line.find("Warning:") {
                // Warning line
                let message = line[pos..].to_string();
                warnings.push(CompilationWarning {
                    file: String::new(),
                    line: 0,
                    column: 0,
                    message,
                });
            } else if line.contains("error:") {
                // Alternative error format
                errors.push(CompilationError {
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


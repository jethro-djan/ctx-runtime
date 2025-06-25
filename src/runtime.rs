use std::collections::HashMap;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::Write;
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
        
        // Remove old compiler diagnostics
        diagnostics.retain(|d| d.source != "compiler");
        
        // Add new compilation errors
        for error in &result.errors {
            diagnostics.push(Diagnostic::error(
                error.line,
                error.column,
                1,
                error.message.clone(),
                "compiler".to_string(),
            ));
        }
        
        // Add new compilation warnings
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

    // NEW: Method to set custom context executable path
    pub fn set_context_executable(&mut self, path: PathBuf) {
        self.compiler.executable = path;
    }

    // NEW: Method to set working directory
    pub fn set_working_directory(&mut self, path: PathBuf) {
        self.compiler.working_dir = path;
    }

    // NEW: Method to check if context executable exists
    pub fn context_executable_exists(&self) -> bool {
        self.compiler.executable_exists()
    }

    // NEW: Method to get current executable path
    pub fn get_context_executable(&self) -> &PathBuf {
        &self.compiler.executable
    }
}

impl ConTeXtCompiler {
    pub fn new() -> Result<Self, RuntimeError> {
        let executable = which::which("mtxrun")
            // .or_else(|_| which::which("texlua"))
            // .or_else(|_| which::which("context-lmtx"))
            .map_err(|_| RuntimeError::ParseError("Could not find ConTeXt executable".to_string()))?;

<<<<<<< HEAD
    pub fn new_with_executable(executable: PathBuf) -> Self {
        ConTeXtCompiler {
            executable,
            working_dir: std::env::temp_dir(),
        }
    }

    pub fn executable_exists(&self) -> bool {
        // Check if the executable exists and is executable
        if self.executable.is_absolute() {
            self.executable.exists()
        } else {
            // For relative paths, try to find in PATH
            self.find_in_path().is_some()
        }
    }

    fn find_in_path(&self) -> Option<PathBuf> {
        if let Ok(path_var) = std::env::var("PATH") {
            for path in std::env::split_paths(&path_var) {
                let full_path = path.join(&self.executable);
                if full_path.exists() {
                    return Some(full_path);
                }
                
                // On Windows, also try with .exe extension
                #[cfg(windows)]
                {
                    let exe_path = path.join(format!("{}.exe", self.executable.display()));
                    if exe_path.exists() {
                        return Some(exe_path);
                    }
                }
            }
        }
        None
    }

    pub fn compile(&self, input_file: &Path) -> Result<CompilationResult, RuntimeError> {
        // Ensure the executable exists before attempting compilation
        if !self.executable_exists() {
            return Err(RuntimeError::CompilationError(
                format!("ConTeXt executable not found: {}", self.executable.display())
            ));
        }

        let executable_path = if self.executable.is_absolute() {
            self.executable.clone()
        } else {
            self.find_in_path().unwrap_or_else(|| self.executable.clone())
        };

        let output = Command::new(&executable_path)
            .arg("--batchmode")
            .arg("--nonstopmode")
            .arg("--purgeall") // Clean up auxiliary files
            .arg(input_file)
            .current_dir(&self.working_dir)
            .output()
            .map_err(|e| RuntimeError::CompilationError(
                format!("Failed to execute ConTeXt ({}): {}", executable_path.display(), e)
            ))?;

        let log = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let success = output.status.success();
        
        // Combine stdout and stderr for comprehensive log
        let full_log = if stderr.is_empty() {
            log
        } else {
            format!("{}\n--- stderr ---\n{}", log, stderr)
        };
        
        let (errors, warnings) = self.parse_log(&full_log);
        
        let output_path = if success {
            let mut path = input_file.to_path_buf();
            path.set_extension("pdf");
            // Only return the path if the PDF actually exists
            if path.exists() {
                Some(path)
            } else {
                None
            }
        } else {
            None
        };

        Ok(CompilationResult {
            success,
            output_path,
            errors,
            warnings,
            log: full_log,
=======
        Ok(ConTeXtCompiler {
            executable,
            working_dir: std::env::current_dir()?,
>>>>>>> aac4d207ea2836c1cc79c5c33ee117f783681446
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
        
<<<<<<< HEAD
        // ConTeXt log parsing - this is a simplified version
        // You may need to enhance this based on actual ConTeXt log format
        for (line_num, line) in log.lines().enumerate() {
            let line = line.trim();
            
            // Error patterns
            if line.starts_with("! ") || line.contains("error") {
=======
        for line in log.lines() {
            if line.starts_with("! ") {
                // Error line
                let message = line.trim_start_matches("! ").to_string();
>>>>>>> aac4d207ea2836c1cc79c5c33ee117f783681446
                errors.push(CompilationError {
                    file: String::new(), // ConTeXt logs don't always clearly indicate file
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
                    column: 0,
                    message,
                });
<<<<<<< HEAD
            }
            // Warning patterns
            else if line.contains("warning") || line.contains("Warning") {
=======
            } else if let Some(pos) = line.find("Warning:") {
                // Warning line
                let message = line[pos..].to_string();
>>>>>>> aac4d207ea2836c1cc79c5c33ee117f783681446
                warnings.push(CompilationWarning {
                    file: String::new(),
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
                    column: 0,
                    message: line.to_string(),
                });
            }
            // TeX error patterns
            else if line.starts_with("tex error") {
                errors.push(CompilationError {
                    file: String::new(),
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
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

    fn extract_line_number(&self, line: &str) -> Option<u32> {
        // Try to extract line numbers from various ConTeXt error formats
        // This is a simplified implementation - you may need to enhance this
        
        // Look for patterns like "line 42" or "l.42"
        if let Some(pos) = line.find("line ") {
            let after_line = &line[pos + 5..];
            if let Some(space_pos) = after_line.find(' ') {
                if let Ok(num) = after_line[..space_pos].parse::<u32>() {
                    return Some(num);
                }
            }
        }
        
        if let Some(pos) = line.find("l.") {
            let after_l = &line[pos + 2..];
            let mut end_pos = 0;
            for (i, c) in after_l.char_indices() {
                if !c.is_ascii_digit() {
                    end_pos = i;
                    break;
                }
            }
            if end_pos > 0 {
                if let Ok(num) = after_l[..end_pos].parse::<u32>() {
                    return Some(num);
                }
            }
        }
        
        None
    }

    // NEW: Method to test if compilation works
    pub fn test_compilation(&self) -> Result<bool, RuntimeError> {
        // Create a minimal test document
        let test_content = r#"\starttext
Hello, ConTeXt!
\stoptext"#;
        
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("context_test.tex");
        
        // Write test file
        std::fs::write(&test_file, test_content)?;
        
        // Try to compile
        let result = self.compile(&test_file);
        
        // Clean up
        let _ = std::fs::remove_file(&test_file);
        if let Ok(ref comp_result) = result {
            if let Some(ref pdf_path) = comp_result.output_path {
                let _ = std::fs::remove_file(pdf_path);
            }
        }
        
        match result {
            Ok(comp_result) => Ok(comp_result.success),
            Err(_) => Ok(false),
        }
    }

    // NEW: Get version information
    pub fn get_version(&self) -> Result<String, RuntimeError> {
        if !self.executable_exists() {
            return Err(RuntimeError::CompilationError(
                format!("ConTeXt executable not found: {}", self.executable.display())
            ));
        }

        let executable_path = if self.executable.is_absolute() {
            self.executable.clone()
        } else {
            self.find_in_path().unwrap_or_else(|| self.executable.clone())
        };

        let output = Command::new(&executable_path)
            .arg("--version")
            .output()
            .map_err(|e| RuntimeError::CompilationError(
                format!("Failed to get ConTeXt version: {}", e)
            ))?;

        let version_info = String::from_utf8_lossy(&output.stdout);
        Ok(version_info.trim().to_string())
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

<<<<<<< HEAD
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ConTeXtCompiler {
    fn default() -> Self {
        Self::new()
    }
}
=======
>>>>>>> aac4d207ea2836c1cc79c5c33ee117f783681446

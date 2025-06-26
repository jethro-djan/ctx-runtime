use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ops::Range;
use std::io::Write;
use bumpalo::Bump;
use std::sync::{Mutex, Arc};
use crate::highlight::{Highlight, highlight};
use crate::diagnostic::Diagnostic;
use crate::syntax::{SyntaxKind, SyntaxTree};
use crate::parser::parse_text;

pub struct Runtime {
    documents: Mutex<HashMap<String, DocumentData>>,
    compiler: ConTeXtCompiler,
    diagnostics: Mutex<HashMap<String, Vec<Diagnostic>>>,
}

struct DocumentData {
    source: String,
    syntax_tree: SyntaxTree,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            documents: Mutex::new(HashMap::new()),
            compiler: ConTeXtCompiler::new(),
            diagnostics: Mutex::new(HashMap::new()),
        }
    }

    pub fn with_document<F, R>(&self, uri: &str, f: F) -> Option<R>
    where
        F: FnOnce(&DocumentData) -> R
    {
        let docs = self.documents.lock().ok()?;
        docs.get(uri).map(f)
    }

    pub fn open_document(&self, uri: String, content: String) -> Result<(), RuntimeError> {
        let syntax_tree = parse_text(&content);

        let document = DocumentData {
            source: content,
            syntax_tree,
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
        edit_range: Range<usize>, 
        new_text: &str,
    ) -> Result<(), RuntimeError> {
        let mut documents = self.documents.lock()
            .map_err(|_| RuntimeError::LockPoisoned)?;

        if let Some(document) = documents.get_mut(uri) {
            let mut new_source = document.source.clone();
            new_source.replace_range(edit_range.clone(), new_text);
            
            let new_tree = parse_text(&new_source);

            *document = DocumentData {
                source: new_source,
                syntax_tree: new_tree,
            };
            
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

    pub fn compile_document(&self, uri: &str) -> Result<CompilationResult, RuntimeError> {
        let content = self.get_document_source(uri)
            .ok_or_else(|| RuntimeError::CompilationError("Document not found".to_string()))?;
        
        let temp_file = self.create_temp_file(uri, &content)?;
        let result = self.compiler.compile(&temp_file)?;
        
        let _ = std::fs::remove_file(&temp_file);
        
        self.update_compilation_diagnostics(uri, &result)?;
        
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
        
        // Convert line/column positions to absolute offsets
        if let Some(document) = self.documents.lock().unwrap().get(uri) {
            for error in &result.errors {
                if let Some(offset) = self.line_column_to_offset(&document.source, error.line, error.column) {
                    diagnostics.push(Diagnostic::error(
                        offset,
                        1,  // Length of 1 for compiler errors
                        error.message.clone(),
                        "compiler".to_string(),
                    ));
                }
            }
            
            for warning in &result.warnings {
                if let Some(offset) = self.line_column_to_offset(&document.source, warning.line, warning.column) {
                    diagnostics.push(Diagnostic::warning(
                        offset,
                        1,  // Length of 1 for compiler warnings
                        warning.message.clone(),
                        "compiler".to_string(),
                    ));
                }
            }
        }
        
        Ok(())
    }

    /// Helper function to convert line/column to absolute offset
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

    pub fn set_context_executable(&self, path: PathBuf) {
        self.compiler.set_executable(path);
    }

    pub fn set_working_directory(&self, path: PathBuf) {
        self.compiler.set_working_directory(path);
    }

    pub fn context_executable_exists(&self) -> bool {
        self.compiler.executable_exists()
    }

    pub fn get_context_executable(&self) -> PathBuf {
        self.compiler.get_executable()
    }
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
    executable: Mutex<PathBuf>,
    working_dir: Mutex<PathBuf>,
}

impl ConTeXtCompiler {
    pub fn new() -> Self {
        ConTeXtCompiler {
            executable: Mutex::new(PathBuf::from("context")),
            working_dir: Mutex::new(std::env::temp_dir()),
        }
    }

    pub fn executable_exists(&self) -> bool {
        let executable = self.executable.lock().unwrap();
        if executable.is_absolute() {
            executable.exists()
        } else {
            self.find_in_path(&executable).is_some()
        }
    }

    pub fn set_executable(&self, path: PathBuf) {
        *self.executable.lock().unwrap() = path;
    }

    pub fn get_executable(&self) -> PathBuf {
        self.executable.lock().unwrap().clone()
    }

    pub fn set_working_directory(&self, path: PathBuf) {
        *self.working_dir.lock().unwrap() = path;
    }

    fn find_in_path(&self, executable: &Path) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let full_path = dir.join(executable);
                if full_path.exists() {
                    Some(full_path)
                } else {
                    #[cfg(windows)]
                    {
                        let with_exe = dir.join(format!("{}.exe", executable.display()));
                        with_exe.exists().then_some(with_exe)
                    }
                    #[cfg(not(windows))]
                    {
                        None
                    }
                }
            })
        })
    }

    pub fn compile(&self, input_file: &Path) -> Result<CompilationResult, RuntimeError> {
        let executable = self.get_executable_path()?;
        let working_dir = self.working_dir.lock().unwrap().clone();

        let output = Command::new(&executable)
            .arg("--batchmode")
            .arg("--nonstopmode")
            .arg("--purgeall")
            .arg(input_file)
            .current_dir(&working_dir)
            .output()
            .map_err(|e| RuntimeError::CompilationError(
                format!("Failed to execute ConTeXt ({}): {}", executable.display(), e)
            ))?;

        self.process_output(output, input_file)
    }

    fn get_executable_path(&self) -> Result<PathBuf, RuntimeError> {
        let executable = self.executable.lock().unwrap();
        if executable.is_absolute() {
            if executable.exists() {
                Ok(executable.clone())
            } else {
                Err(RuntimeError::CompilationError(
                    format!("ConTeXt executable not found: {}", executable.display())
                ))
            }
        } else {
            self.find_in_path(&executable)
                .ok_or_else(|| RuntimeError::CompilationError(
                    format!("ConTeXt executable not found in PATH: {}", executable.display())
                ))
        }
    }

    fn process_output(&self, output: std::process::Output, input_file: &Path) -> Result<CompilationResult, RuntimeError> {
        let log = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let success = output.status.success();

        let full_log = if stderr.is_empty() {
            log.into_owned()
        } else {
            format!("{}\n--- stderr ---\n{}", log, stderr)
        };

        let (errors, warnings) = self.parse_log(&full_log);

        let output_path = if success {
            let mut path = input_file.to_path_buf();
            path.set_extension("pdf");
            path.exists().then_some(path)
        } else {
            None
        };

        Ok(CompilationResult {
            success,
            output_path,
            errors,
            warnings,
            log: full_log,
        })
    }

    fn parse_log(&self, log: &str) -> (Vec<CompilationError>, Vec<CompilationWarning>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        for (line_num, line) in log.lines().enumerate() {
            let line = line.trim();
            
            if line.starts_with("! ") || line.contains("error") {
                errors.push(CompilationError {
                    file: String::new(),
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
                    column: 0,
                    message: line.to_string(),
                });
            }
            else if line.contains("warning") || line.contains("Warning") {
                warnings.push(CompilationWarning {
                    file: String::new(),
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
                    column: 0,
                    message: line.to_string(),
                });
            }
            else if line.starts_with("tex error") {
                errors.push(CompilationError {
                    file: String::new(),
                    line: self.extract_line_number(line).unwrap_or(line_num as u32 + 1),
                    column: 0,
                    message: line.to_string(),
                });
            }
        }
        
        (errors, warnings)
    }

    fn extract_line_number(&self, line: &str) -> Option<u32> {
        line.find("line ")
            .and_then(|pos| {
                let rest = &line[pos + 5..];
                let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
                rest[..num_end].parse().ok()
            })
            .or_else(|| {
                line.find("l.")
                    .and_then(|pos| {
                        let rest = &line[pos + 2..];
                        let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
                        rest[..num_end].parse().ok()
                    })
            })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Document not found: {0}")]
    DocumentNotFound(String),
    
    #[error("Mutex/RwLock poisoned")]
    LockPoisoned,
    
    #[error("Document access failed: {0}")]
    DocumentAccess(String),

    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Compilation error: {0}")]
    CompilationError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

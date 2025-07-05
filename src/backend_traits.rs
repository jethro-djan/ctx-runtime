use std::path::PathBuf;
use std::any::Any;
use std::path::Path;
use async_trait::async_trait;
use thiserror::Error;
use tempfile::TempDir;
use regex::Regex;
use serde::Deserialize;
use reqwest::Client;

#[derive(Debug)]
pub struct CompilationRequest {
    pub content: String,
    pub job_id: String,
}

#[derive(Debug)]
pub struct CompilationResult {
    pub success: bool,
    pub pdf_path: Option<PathBuf>,
    pub log: String,   
    pub errors: Vec<CompilationError>, 
    pub warnings: Vec<CompilationError>
}

#[derive(Debug, Deserialize)]
pub struct CompileResponse {
    pub success: bool,
    pub log: String,
    pub output_url: Option<String>,
    pub diagnostics: Vec<RemoteDiagnostic>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteDiagnostic {
    pub message: String,
    pub severity: String,
    pub range: Option<RemoteRange>,
}

#[derive(Debug, Deserialize)]
pub struct RemoteRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct CompilationError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Compilation failed: {0}")]
    Compilation(String),
    #[error("Backend unavailable: {0}")]
    Unavailable(String),
    #[error("Something went wrong with the setup: {0}")]
    Setup(String),
    #[error("IO Error: {0}")]
    IO(String),
}

#[async_trait]
pub trait CompilationBackend: Send + Sync + std::fmt::Debug + Any {
    fn as_any(&self) -> &dyn Any;
    async fn compile(&self, request: CompilationRequest) -> Result<CompilationResult, BackendError>;
}

#[derive(Debug)]
pub struct LocalBackend {
    executable_path: PathBuf,
    working_dir: TempDir,
}

impl LocalBackend {
    pub fn new(executable_path: Option<PathBuf>) -> Result<Self, BackendError> {
        let executable_path = executable_path.unwrap_or_else(|| PathBuf::from("context"));

        let working_dir = tempfile::tempdir()
            .map_err(|e| BackendError::Setup(e.to_string()))?;

        if !executable_path.exists() {
            return Err(BackendError::Unavailable("ConTeXt not found in PATH".into()));
        }
        Ok(Self { 
            executable_path,
            working_dir,
        })
    }

    async fn create_temp_file(&self, job_id: &str, content: &str) -> Result<PathBuf, BackendError> {
        let file_path = self.working_dir.path().join(format!("{}.tex", job_id));
        
        tokio::fs::write(&file_path, content)
            .await
            .map_err(|e| BackendError::IO(e.to_string()))?;
            
        Ok(file_path)
    }

    async fn process_output(
        &self,
        output: std::process::Output,
        source_file: &Path,
    ) -> Result<CompilationResult, BackendError> {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let full_log = format!("{}\n\nSTDERR:\n{}", stdout, stderr);
        
        // Check for PDF output
        let pdf_path = if output.status.success() {
            let pdf_path = source_file.with_extension("pdf");
            pdf_path.exists().then_some(pdf_path)
        } else {
            None
        };

        // Parse errors and warnings from output
        let result = self.parse_compiler_output(&full_log);

        Ok(CompilationResult {
            success: output.status.success(),
            pdf_path,
            log: full_log,
            errors: result.errors,
            warnings: result.warnings,
        })
    }


    pub fn parse_compiler_output(&self, output: &str) -> CompilationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        
        for line in output.lines() {
            if let Some(error) = self.parse_compiler_line(line) {
                if line.to_lowercase().contains("warning") {
                    warnings.push(error);
                } else {
                    errors.push(error);
                }
            }
        }
        
        CompilationResult {
            success: errors.is_empty(),
            pdf_path: None,
            log: output.to_string(),
            errors,
            warnings,
        }
    }

    fn parse_compiler_line(&self, line: &str) -> Option<CompilationError> {
        // Example parser for lines like "main.tex:12:5 Error: Missing $"
        let re = Regex::new(r"(?x)
            ^(?:.*?):?      # Optional filename
            (\d+)           # Line number
            :
            (\d+)           # Column number
            \s+
            (?:error|warning):?
            \s+
            (.+)           # Message
        ").unwrap();
        
        re.captures(line).map(|caps| CompilationError {
            line: caps[1].parse().unwrap_or(0),
            column: caps[2].parse().unwrap_or(0),
            message: caps[3].trim().to_string(),
        })
    }
}

#[async_trait]
impl CompilationBackend for LocalBackend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn compile(&self, request: CompilationRequest) -> Result<CompilationResult, BackendError> {
         use tokio::process::Command;
        
        let temp_file = self.create_temp_file(&request.job_id, &request.content).await?;
        
        let output = Command::new(&self.executable_path)
            .arg("--batchmode")
            .arg("--nonstopmode") 
            .arg("--purgeall")
            .arg(&temp_file)
            .current_dir(&self.working_dir)
            .output()
            .await
            .map_err(|e| BackendError::Compilation(e.to_string()))?;
            
        self.process_output(output, &temp_file).await
    }
}

#[derive(Debug)]
pub struct RemoteBackend {
    endpoint: String,
    client: Client,
    auth_token: Option<String>,
}

impl RemoteBackend {
    pub fn new(endpoint: String, auth_token: Option<String>) -> Self {
        let client = Client::new();
        Self { endpoint, auth_token, client }
    }
}

#[async_trait]
impl CompilationBackend for RemoteBackend {
    fn as_any(&self) -> &dyn Any {
        self
    }

    async fn compile(&self, request: CompilationRequest) -> Result<CompilationResult, BackendError> {
        let mut req = self.client
            .post(&format!("{}/compile", self.endpoint))
            .json(&serde_json::json!({
                "uri": request.job_id,    
                "content": request.content,
                "format": "pdf",         
            }));

        // Add auth header if token present
        if let Some(token) = &self.auth_token {
            req = req.bearer_auth(token);
        }

        let response = req.send().await
            .map_err(|e| BackendError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(BackendError::Compilation(format!(
                "Server returned {}",
                response.status()
            )));
        }

        let remote_result: CompileResponse = response.json().await
            .map_err(|e| BackendError::Network(e.to_string()))?;

        Ok(CompilationResult {
            success: remote_result.success,
            pdf_path: remote_result.output_url.map(PathBuf::from),
            log: remote_result.log,
            errors: remote_result.diagnostics.iter().filter_map(|d| {
                if d.severity == "error" {
                    Some(CompilationError {
                        line: d.range.as_ref().map_or(0, |r| r.start),
                        column: d.range.as_ref().map_or(0, |r| r.end),
                        message: d.message.clone(),
                    })
                } else {
                    None
                }
            }).collect(),
            warnings: remote_result.diagnostics.iter().filter_map(|d| {
                if d.severity == "warning" {
                    Some(CompilationError {
                        line: d.range.as_ref().map_or(0, |r| r.start),
                        column: d.range.as_ref().map_or(0, |r| r.end),
                        message: d.message.clone(),
                    })
                } else {
                    None
                }
            }).collect(),
        })
    }
}

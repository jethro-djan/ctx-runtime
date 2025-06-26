use std::collections::HashMap;
use std::path::PathBuf;
use std::ops::Range;
use uuid::Uuid;
use std::thread;
use std::sync::{Arc, RwLock, Mutex};

use crate::runtime::{Runtime, CompilationResult, RuntimeError};
use crate::highlight::Highlight;
use crate::diagnostic::Diagnostic;
use crate::ffi_types::*;
use crate::conversions::*;

use std::sync::mpsc;
use tokio::sync::oneshot;
use uniffi::{self, Object};

#[uniffi::export(with_foreign)]
pub trait LiveUpdateCallback: Send + Sync + std::fmt::Debug {
    fn on_diagnostics_updated(&self, uri: String, diagnostics: Vec<DiagnosticFfi>);
    fn on_highlights_updated(&self, uri: String, highlights: Vec<HighlightFfi>);
    
    fn on_error(&self, error: RuntimeErrorFfi) {
        log::warn!("Unhandled error in callback: {:?}", error);
    }
}

#[uniffi::export(with_foreign)]
pub trait CompilationCallback: Send + Sync + std::fmt::Debug {
    fn on_progress(&self, progress: f32);
    fn on_compilation_complete(&self, result: CompileResultFfi);
    
    fn on_error(&self, error: RuntimeErrorFfi) {
        log::warn!("Unhandled compilation error: {:?}", error);
    }
}

#[derive(Debug)]
enum RuntimeCommand {
    OpenDocument { 
        uri: String, 
        content: String, 
        reply: oneshot::Sender<Result<(), RuntimeError>> 
    },
    UpdateDocument { 
        uri: String, 
        edit_range: Range<usize>, 
        new_text: String, 
        reply: oneshot::Sender<Result<(), RuntimeError>> 
    },
    CloseDocument { 
        uri: String 
    },
    GetDocumentSource { 
        uri: String, 
        reply: oneshot::Sender<Option<String>> 
    },
    GetHighlights { 
        uri: String, 
        reply: oneshot::Sender<Vec<Highlight>> 
    },
    GetDiagnostics { 
        uri: String, 
        reply: oneshot::Sender<Vec<Diagnostic>> 
    },
    CompileDocument { 
        uri: String, 
        reply: oneshot::Sender<Result<CompilationResult, RuntimeError>> 
    },
    SetContextExecutable { 
        path: PathBuf 
    },
    SetWorkingDirectory { 
        path: PathBuf 
    },
    CheckExecutableExists { 
        reply: oneshot::Sender<bool> 
    },
    GetExecutablePath { 
        reply: oneshot::Sender<PathBuf> 
    },
    Shutdown,
}

#[derive(Debug)]
enum BackgroundTask {
    ParseDocument { 
        uri: String,
    },
    CompileDocument { 
        job: CompilationJob 
    },
    Shutdown,
}

#[derive(Debug, Clone)]
struct CompilationJob {
    id: String,
    uri: String,
    callback: Option<Arc<dyn CompilationCallback>>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct CompilationConfig {
    pub backend: CompilationBackend,
    pub timeout_seconds: u64,
    pub auth_token: Option<String>,
}

#[derive(uniffi::Enum, Debug, Clone)]
pub enum CompilationBackend {
    Auto,
    Local { 
        executable_path: Option<String>
    },
    #[cfg(feature = "http-compilation")]
    Remote { 
        endpoint: String 
    },
    LocalWithInstall { 
        install_path: String 
    },
}

impl Default for CompilationConfig {
    fn default() -> Self {
        Self {
            backend: CompilationBackend::Auto,
            timeout_seconds: 60,
            auth_token: None,
        }
    }
}

#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    runtime_sender: mpsc::Sender<RuntimeCommand>,
    task_sender: tokio::sync::mpsc::UnboundedSender<BackgroundTask>,
    _task_worker: tokio::task::JoinHandle<()>,
    _runtime_thread: thread::JoinHandle<()>,
    live_callback: Arc<RwLock<Option<Arc<dyn LiveUpdateCallback>>>>,
    active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
    compilation_config: Arc<RwLock<CompilationConfig>>,
    
    #[cfg(feature = "http-compilation")]
    http_client: Arc<reqwest::Client>,
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::new_with_config(CompilationConfig::default())
    }
    
    #[uniffi::constructor]
    pub fn new_with_config(config: CompilationConfig) -> Self {
        let (runtime_sender, runtime_receiver) = mpsc::channel::<RuntimeCommand>();
        let (task_sender, task_receiver) = tokio::sync::mpsc::unbounded_channel::<BackgroundTask>();
        
        let live_callback = Arc::new(RwLock::new(None));
        let active_jobs = Arc::new(Mutex::new(HashMap::new()));
        let compilation_config = Arc::new(RwLock::new(config));
        
        #[cfg(feature = "http-compilation")]
        let http_client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client")
        );
        
        let runtime_thread = thread::spawn(move || {
            let mut runtime = Runtime::new();
            
            while let Ok(command) = runtime_receiver.recv() {
                match command {
                    RuntimeCommand::OpenDocument { uri, content, reply } => {
                        let _ = reply.send(runtime.open_document(uri, content));
                    }
                    RuntimeCommand::UpdateDocument { uri, edit_range, new_text, reply } => {
                        let result = runtime.update_document(&uri, edit_range, &new_text);
                        let _ = reply.send(result);
                    }
                    RuntimeCommand::CloseDocument { uri } => {
                        runtime.close_document(&uri);
                    }
                    RuntimeCommand::GetDocumentSource { uri, reply } => {
                        let source = runtime.get_document_source(&uri);
                        let _ = reply.send(source);
                    }
                    RuntimeCommand::GetHighlights { uri, reply } => {
                        let highlights = runtime.get_highlights(&uri);
                        let _ = reply.send(highlights);
                    }
                    RuntimeCommand::GetDiagnostics { uri, reply } => {
                        let diagnostics = runtime.get_diagnostics(&uri);
                        let _ = reply.send(diagnostics);
                    }
                    RuntimeCommand::CompileDocument { uri, reply } => {
                        let result = runtime.compile_document(&uri);
                        let _ = reply.send(result);
                    }
                    RuntimeCommand::SetContextExecutable { path } => {
                        runtime.set_context_executable(path);
                    }
                    RuntimeCommand::SetWorkingDirectory { path } => {
                        runtime.set_working_directory(path);
                    }
                    RuntimeCommand::CheckExecutableExists { reply } => {
                        let exists = runtime.context_executable_exists();
                        let _ = reply.send(exists);
                    }
                    RuntimeCommand::GetExecutablePath { reply } => {
                        let path = runtime.get_context_executable().clone();
                        let _ = reply.send(path);
                    }
                    RuntimeCommand::Shutdown => break,
                }
            }
        });
        
        let callback_clone = live_callback.clone();
        let jobs_clone = active_jobs.clone();
        let config_clone = compilation_config.clone();
        let runtime_sender_clone = runtime_sender.clone();
        
        #[cfg(feature = "http-compilation")]
        let http_client_clone = http_client.clone();
        
        let task_worker = tokio::spawn(async move {
            Self::task_worker(
                task_receiver,
                callback_clone,
                jobs_clone,
                config_clone,
                runtime_sender_clone,
                #[cfg(feature = "http-compilation")]
                http_client_clone,
            ).await;
        });
        
        Self {
            runtime_sender,
            task_sender,
            _task_worker: task_worker,
            _runtime_thread: runtime_thread,
            live_callback,
            active_jobs,
            compilation_config,
            #[cfg(feature = "http-compilation")]
            http_client,
        }
    }
    
    pub fn set_live_callback(&self, callback: Option<Arc<dyn LiveUpdateCallback>>) {
        if let Ok(mut live_callback) = self.live_callback.write() {
            *live_callback = callback;
        }
    }
    
    pub fn open(&self, uri: String, text: String) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::OpenDocument { 
            uri: uri.clone(), 
            content: text, 
            reply: reply_tx 
        };
        
        if self.runtime_sender.send(cmd).is_err() {
            return false;
        }
        
        match reply_rx.blocking_recv() {
            Ok(Ok(())) => {
                let _ = self.task_sender.send(BackgroundTask::ParseDocument { uri });
                true
            },
            _ => false
        }
    }
    
    pub fn update(&self, uri: String, start: u32, end: u32, text: String) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let edit_range = (start as usize)..(end as usize);
        let cmd = RuntimeCommand::UpdateDocument { 
            uri: uri.clone(), 
            edit_range, 
            new_text: text, 
            reply: reply_tx 
        };
        
        if self.runtime_sender.send(cmd).is_err() {
            return false;
        }
        
        match reply_rx.blocking_recv() {
            Ok(Ok(())) => {
                let _ = self.task_sender.send(BackgroundTask::ParseDocument { uri });
                true
            },
            _ => false
        }
    }
    
    pub fn close(&self, uri: String) {
        let _ = self.runtime_sender.send(RuntimeCommand::CloseDocument { uri });
    }
    
    pub fn get_document_source(&self, uri: String) -> Option<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::GetDocumentSource { uri, reply: reply_tx };
        
        if self.runtime_sender.send(cmd).is_err() {
            return None;
        }
        
        reply_rx.blocking_recv().unwrap_or(None)
    }
    
    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::GetHighlights { uri, reply: reply_tx };
        
        if self.runtime_sender.send(cmd).is_err() {
            return Vec::new();
        }
        
        match reply_rx.blocking_recv() {
            Ok(highlights) => highlights.into_iter().map(|h| HighlightFfi {
                range: FfiRange {
                    start: h.range.start as u32,
                    end: h.range.end as u32,
                },
                kind: h.kind.to_string(),
            }).collect(),
            Err(_) => Vec::new()
        }
    }
    
    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::GetDiagnostics { uri, reply: reply_tx };
        
        if self.runtime_sender.send(cmd).is_err() {
            return Vec::new();
        }
        
        match reply_rx.blocking_recv() {
            Ok(diagnostics) => diagnostics.into_iter().map(Into::into).collect(),
            Err(_) => Vec::new()
        }
    }
    
    pub fn compile(&self, uri: String, callback: Option<Arc<dyn CompilationCallback>>) -> String {
        let job_id = Uuid::new_v4().to_string();
        let job = CompilationJob {
            id: job_id.clone(),
            uri,
            callback,
        };
        
        {
            let mut jobs = self.active_jobs.lock().unwrap();
            jobs.insert(job_id.clone(), job.clone());
        }
        
        let _ = self.task_sender.send(BackgroundTask::CompileDocument { job });
        job_id
    }
    
    pub fn cancel_compilation(&self, job_id: String) -> bool {
        let mut jobs = self.active_jobs.lock().unwrap();
        jobs.remove(&job_id).is_some()
    }
    
       pub fn set_context_executable(&self, path: String) {
        let cmd = RuntimeCommand::SetContextExecutable { 
            path: PathBuf::from(path)  // Convert String to PathBuf
        };
        let _ = self.runtime_sender.send(cmd);
    }
    
    pub fn set_working_directory(&self, path: String) {
        let cmd = RuntimeCommand::SetWorkingDirectory { 
            path: PathBuf::from(path)  // Convert String to PathBuf
        };
        let _ = self.runtime_sender.send(cmd);
    } 

    pub fn context_executable_exists(&self) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::CheckExecutableExists { reply: reply_tx };
        
        if self.runtime_sender.send(cmd).is_err() {
            return false;
        }
        
        reply_rx.blocking_recv().unwrap_or(false)
    }
    
    pub fn get_context_executable_path(&self) -> String {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = RuntimeCommand::GetExecutablePath { reply: reply_tx };
        
        if self.runtime_sender.send(cmd).is_err() {
            return String::new();
        }
        
        reply_rx.blocking_recv()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
    
    pub fn shutdown(&self) {
        let _ = self.runtime_sender.send(RuntimeCommand::Shutdown);
        let _ = self.task_sender.send(BackgroundTask::Shutdown);
    }
}

impl ContextRuntimeHandle {
    async fn task_worker(
        mut receiver: tokio::sync::mpsc::UnboundedReceiver<BackgroundTask>,
        live_callback: Arc<RwLock<Option<Arc<dyn LiveUpdateCallback>>>>,
        active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
        config: Arc<RwLock<CompilationConfig>>,
        runtime_sender: mpsc::Sender<RuntimeCommand>,
        #[cfg(feature = "http-compilation")]
        http_client: Arc<reqwest::Client>,
    ) {
        while let Some(task) = receiver.recv().await {
            match task {
                BackgroundTask::ParseDocument { uri } => {
                    Self::process_parse_task(
                        runtime_sender.clone(),
                        live_callback.clone(),
                        uri,
                    ).await;
                }
                BackgroundTask::CompileDocument { job } => {
                    Self::process_compile_task(
                        runtime_sender.clone(),
                        active_jobs.clone(),
                        config.clone(),
                        #[cfg(feature = "http-compilation")]
                        http_client.clone(),
                        job,
                    ).await;
                }
                BackgroundTask::Shutdown => break,
            }
        }
    }
    
    async fn process_parse_task(
        runtime_sender: mpsc::Sender<RuntimeCommand>,
        live_callback: Arc<RwLock<Option<Arc<dyn LiveUpdateCallback>>>>,
        uri: String,
    ) {
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        
        let (diag_tx, diag_rx) = oneshot::channel();
        let (high_tx, high_rx) = oneshot::channel();
        
        let diag_cmd = RuntimeCommand::GetDiagnostics { 
            uri: uri.clone(), 
            reply: diag_tx 
        };
        let high_cmd = RuntimeCommand::GetHighlights { 
            uri: uri.clone(), 
            reply: high_tx 
        };
        
        if runtime_sender.send(diag_cmd).is_ok() && runtime_sender.send(high_cmd).is_ok() {
            let diagnostics = diag_rx.await.unwrap_or_default();
            let highlights = high_rx.await.unwrap_or_default();
            
            if let Ok(callback_guard) = live_callback.read() {
                if let Some(callback) = callback_guard.as_ref() {
                    let diagnostics_ffi: Vec<DiagnosticFfi> = diagnostics.into_iter().map(Into::into).collect();
                    let highlights_ffi: Vec<HighlightFfi> = highlights.into_iter().map(|h| HighlightFfi {
                        range: FfiRange {
                            start: h.range.start as u32,
                            end: h.range.end as u32,
                        },
                        kind: h.kind.to_string(),
                    }).collect();
                    
                    callback.on_diagnostics_updated(uri.clone(), diagnostics_ffi);
                    callback.on_highlights_updated(uri, highlights_ffi);
                }
            }
        }
    } 

    async fn process_compile_task(
        runtime_sender: mpsc::Sender<RuntimeCommand>,
        active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
        config: Arc<RwLock<CompilationConfig>>,
        #[cfg(feature = "http-compilation")]
        http_client: Arc<reqwest::Client>,
        job: CompilationJob,
    ) {
        // Early exit if job was cancelled
        {
            let jobs = active_jobs.lock().unwrap();
            if !jobs.contains_key(&job.id) {
                return;
            }
        }
        
        let config = config.read().unwrap().clone();
        let result = {
            #[cfg(feature = "http-compilation")]
            {
                if let CompilationBackend::Remote { ref endpoint } = config.backend {
                    match Self::do_remote_compile(
                        http_client.clone(),
                        &job.uri,
                        endpoint,
                        &config
                    ).await {
                        Ok(result) => result,
                        Err(e) => CompileResultFfi::error(format!("Remote compilation failed: {}", e)),
                    }
                } else {
                    // Handle non-Remote cases when http-compilation is enabled
                    match config.backend {
                        CompilationBackend::Auto |
                        CompilationBackend::Local { .. } |
                        CompilationBackend::LocalWithInstall { .. } => {
                            let (reply_tx, reply_rx) = oneshot::channel();
                            let cmd = RuntimeCommand::CompileDocument { 
                                uri: job.uri.clone(), 
                                reply: reply_tx 
                            };
                            
                            if runtime_sender.send(cmd).is_ok() {
                                match reply_rx.await {
                                    Ok(Ok(result)) => result.into(),
                                    Ok(Err(e)) => CompileResultFfi::error(e.to_string()),
                                    Err(_) => CompileResultFfi::error("Communication error".to_string()),
                                }
                            } else {
                                CompileResultFfi::error("Failed to send compile command".to_string())
                            }
                        }
                        CompilationBackend::Remote { .. } => {
                            // This case is already handled above
                            unreachable!()
                        }
                    }
                }
            }
            
            #[cfg(not(feature = "http-compilation"))]
            {
                // When http-compilation is disabled, Remote variant doesn't exist
                match config.backend {
                    CompilationBackend::Auto |
                    CompilationBackend::Local { .. } |
                    CompilationBackend::LocalWithInstall { .. } => {
                        let (reply_tx, reply_rx) = oneshot::channel();
                        let cmd = RuntimeCommand::CompileDocument { 
                            uri: job.uri.clone(), 
                            reply: reply_tx 
                        };
                        
                        if runtime_sender.send(cmd).is_ok() {
                            match reply_rx.await {
                                Ok(Ok(result)) => result.into(),
                                Ok(Err(e)) => CompileResultFfi::error(e.to_string()),
                                Err(_) => CompileResultFfi::error("Communication error".to_string()),
                            }
                        } else {
                            CompileResultFfi::error("Failed to send compile command".to_string())
                        }
                    }
                    // No Remote variant exists when http-compilation feature is disabled
                }
            }
        };
        
        // Remove job and notify
        if let Some(callback) = active_jobs.lock().unwrap().remove(&job.id).and_then(|j| j.callback) {
            callback.on_compilation_complete(result);
        }
    }

    #[cfg(feature = "http-compilation")]
    async fn do_remote_compile(
        http_client: Arc<reqwest::Client>,
        uri: &str,
        endpoint: &str,
        config: &CompilationConfig,
    ) -> Result<CompileResultFfi, Box<dyn std::error::Error + Send + Sync>> {
        let mut request = http_client
            .post(endpoint)
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .json(&serde_json::json!({
                "uri": uri,
                "action": "compile"
            }));
        
        if let Some(token) = &config.auth_token {
            request = request.bearer_auth(token);
        }
        
        let response = request.send().await?;
        let result: CompileResultFfi = response.json().await?;
        Ok(result)
    }
}

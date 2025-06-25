use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, RwLock};
use std::collections::HashMap;
use uuid::Uuid;
use std::path::PathBuf;
use std::time::Duration;

#[uniffi::export(callback_interface)]
pub trait CompilationCallback: Send + Sync {
    fn on_compilation_complete(&self, result: CompileResultFfi);
}

#[uniffi::export(callback_interface)]
pub trait LiveUpdateCallback: Send + Sync {
    fn on_diagnostics_updated(&self, uri: String, diagnostics: Vec<DiagnosticFfi>);
    fn on_highlights_updated(&self, uri: String, highlights: Vec<HighlightFfi>);
}

#[derive(Debug, Clone)]
pub enum CompilationBackend {
    Auto,                           
    Local(Option<String>),          
    Remote(String),                 
    LocalWithInstall {              
        install_path: Option<String>,
        download_url: String,
        fallback_remote: Option<String>,
    }
}

#[derive(Debug, Clone)]
pub struct CompilationConfig {
    pub backend: CompilationBackend,
    pub timeout_seconds: u64,
    pub auth_token: Option<String>,
    pub auto_install: bool,
}

impl Default for CompilationConfig {
    fn default() -> Self {
        Self {
            backend: CompilationBackend::Auto,
            timeout_seconds: 30,
            auth_token: None,
            auto_install: true,
        }
    }
}

#[derive(Clone)]
pub struct CompilationJob {
    pub id: String,
    pub uri: String,
    pub callback: Option<Arc<dyn CompilationCallback>>,
}

enum BackgroundTask {
    ParseDocument { uri: String, text: String },
    CompileDocument { job: CompilationJob },
}

#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    sync_runtime: Arc<Mutex<Runtime>>,
    
    task_sender: mpsc::UnboundedSender<BackgroundTask>,
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
    
    pub fn new_with_config(config: CompilationConfig) -> Self {
        let sync_runtime = Arc::new(Mutex::new(Runtime::new()));
        let (task_sender, mut task_receiver) = mpsc::unbounded_channel::<BackgroundTask>();
        let live_callback = Arc::new(RwLock::new(None));
        let active_jobs = Arc::new(Mutex::new(HashMap::new()));
        let compilation_config = Arc::new(RwLock::new(config));
        
        #[cfg(feature = "http-compilation")]
        let http_client = Arc::new(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client")
        );
        
        let runtime_clone = sync_runtime.clone();
        let callback_clone = live_callback.clone();
        let jobs_clone = active_jobs.clone();
        let config_clone = compilation_config.clone();
        
        #[cfg(feature = "http-compilation")]
        let http_client_clone = http_client.clone();
        
        tokio::spawn(async move {
            while let Some(task) = task_receiver.recv().await {
                match task {
                    BackgroundTask::ParseDocument { uri, text } => {
                        Self::process_parse_task(
                            runtime_clone.clone(),
                            callback_clone.clone(),
                            uri,
                            text,
                        ).await;
                    }
                    BackgroundTask::CompileDocument { job } => {
                        Self::process_compile_task(
                            runtime_clone.clone(),
                            jobs_clone.clone(),
                            config_clone.clone(),
                            #[cfg(feature = "http-compilation")]
                            http_client_clone.clone(),
                            job,
                        ).await;
                    }
                }
            }
        });
        
        Self {
            sync_runtime,
            task_sender,
            live_callback,
            active_jobs,
            compilation_config,
            #[cfg(feature = "http-compilation")]
            http_client,
        }
    }
    
    #[cfg(all(feature = "local-install", not(target_os = "ios"), not(target_os = "android")))]
    pub fn new_desktop() -> Self {
        let config = CompilationConfig {
            backend: CompilationBackend::LocalWithInstall {
                install_path: None,
                download_url: "https://releases.context.com/latest/context".to_string(),
                fallback_remote: Some("https://api.context.com/compile".to_string()),
            },
            auto_install: true,
            ..Default::default()
        };
        Self::new_with_config(config)
    }
    
    #[cfg(feature = "http-compilation")]
    pub fn new_mobile(api_endpoint: String) -> Self {
        let config = CompilationConfig {
            backend: CompilationBackend::Remote(api_endpoint),
            auto_install: false,
            ..Default::default()
        };
        Self::new_with_config(config)
    }
    
    pub fn set_compilation_config(&self, config: CompilationConfig) {
        tokio::spawn({
            let compilation_config = self.compilation_config.clone();
            async move {
                *compilation_config.write().await = config;
            }
        });
    }
    
    pub fn set_compilation_backend(&self, backend: CompilationBackend) {
        tokio::spawn({
            let compilation_config = self.compilation_config.clone();
            async move {
                compilation_config.write().await.backend = backend;
            }
        });
    }
    
    pub fn set_auth_token(&self, token: Option<String>) {
        tokio::spawn({
            let compilation_config = self.compilation_config.clone();
            async move {
                compilation_config.write().await.auth_token = token;
            }
        });
    }
    
    pub fn set_live_update_callback(&self, callback: Option<Arc<dyn LiveUpdateCallback>>) {
        tokio::spawn({
            let live_callback = self.live_callback.clone();
            async move {
                *live_callback.write().await = callback;
            }
        });
    }
    
    pub fn open(&self, uri: String, text: String) -> bool {
        let result = {
            let runtime = self.sync_runtime.lock().unwrap();
            runtime.open_document(uri.clone(), text.clone()).is_ok()
        };
        
        if result {
            let _ = self.task_sender.send(BackgroundTask::ParseDocument { uri, text });
        }
        
        result
    }
    
    pub fn update(&self, uri: String, text: String) -> bool {
        let result = {
            let runtime = self.sync_runtime.lock().unwrap();
            runtime.open_document(uri.clone(), text.clone()).is_ok()
        };
        
        let _ = self.task_sender.send(BackgroundTask::ParseDocument { uri, text });
        
        result
    }
    
    pub fn close(&self, uri: String) {
        let runtime = self.sync_runtime.lock().unwrap();
        runtime.close_document(&uri);
    }
    
    pub fn get_document_source(&self, uri: String) -> Option<String> {
        let runtime = self.sync_runtime.lock().unwrap();
        runtime.get_document_source(&uri).map(|s| s.to_string())
    }
    
    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        let runtime = self.sync_runtime.lock().unwrap();
        runtime.get_highlights(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }
    
    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        let runtime = self.sync_runtime.lock().unwrap();
        runtime.get_diagnostics(&uri)
            .into_iter()
            .map(Into::into)
            .collect()
    }
    
    pub fn compile(&self, uri: String) -> CompileResultFfi {
        let runtime = self.sync_runtime.lock().unwrap();
        runtime.compile_document(&uri).into()
    }
    
    pub fn compile_async(&self, uri: String, callback: Option<Arc<dyn CompilationCallback>>) -> String {
        let job_id = Uuid::new_v4().to_string();
        let job = CompilationJob {
            id: job_id.clone(),
            uri: uri.clone(),
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
    
    pub fn get_active_compilations(&self) -> Vec<String> {
        let jobs = self.active_jobs.lock().unwrap();
        jobs.keys().cloned().collect()
    }
    
    async fn process_parse_task(
        runtime: Arc<Mutex<Runtime>>,
        live_callback: Arc<RwLock<Option<Arc<dyn LiveUpdateCallback>>>>,
        uri: String,
        text: String,
    ) {
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        
        let (diagnostics, highlights) = {
            let runtime = runtime.lock().unwrap();
            let diagnostics = runtime.get_diagnostics(&uri)
                .into_iter()
                .map(Into::into)
                .collect();
            let highlights = runtime.get_highlights(&uri)
                .into_iter()
                .map(Into::into)
                .collect();
            (diagnostics, highlights)
        };
        
        if let Some(callback) = live_callback.read().await.as_ref() {
            callback.on_diagnostics_updated(uri.clone(), diagnostics);
            callback.on_highlights_updated(uri, highlights);
        }
    }
    
       async fn process_compile_task(
        runtime: Arc<Mutex<Runtime>>,
        active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
        config: Arc<RwLock<CompilationConfig>>,
        #[cfg(feature = "http-compilation")]
        http_client: Arc<reqwest::Client>,
        job: CompilationJob,
    ) {
        {
            let jobs = active_jobs.lock().unwrap();
            if !jobs.contains_key(&job.id) {
                return;
            }
        }
        
        let config = config.read().await.clone();
        
        let result = match &config.backend {
            CompilationBackend::Auto => {
                if let Some(local_path) = Self::find_local_binary().await {
                    Self::compile_local(runtime.clone(), &job.uri, &local_path).await
                } else {
                    #[cfg(feature = "http-compilation")]
                    {
                        if let Some(endpoint) = Self::get_default_remote_endpoint() {
                            Self::compile_remote(http_client, &job.uri, &endpoint, &config).await
                        } else {
                            CompileResultFfi::error("No compilation backend available".to_string())
                        }
                    }
                    #[cfg(not(feature = "http-compilation"))]
                    CompileResultFfi::error("Local binary not found and HTTP compilation not enabled".to_string())
                }
            }
            
            CompilationBackend::Local(path) => {
                let binary_path = if let Some(path) = path {
                    PathBuf::from(path)
                } else {
                    match Self::find_local_binary().await {
                        Some(path) => path,
                        None => {
                            return Self::complete_job_with_error(
                                active_jobs,
                                job,
                                "Context binary not found".to_string()
                            ).await;
                        }
                    }
                };
                Self::compile_local(runtime.clone(), &job.uri, &binary_path).await
            }
            
            CompilationBackend::Remote(endpoint) => {
                #[cfg(feature = "http-compilation")]
                {
                    Self::compile_remote(http_client, &job.uri, endpoint, &config).await
                }
                #[cfg(not(feature = "http-compilation"))]
                {
                    CompileResultFfi::error("HTTP compilation not enabled".to_string())
                }
            }
            
            CompilationBackend::LocalWithInstall { install_path, download_url, fallback_remote } => {
                #[cfg(feature = "local-install")]
                {
                    match Self::ensure_local_binary(install_path, download_url).await {
                        Ok(binary_path) => {
                            Self::compile_local(runtime.clone(), &job.uri, &binary_path).await
                        }
                        Err(_) => {
                            #[cfg(feature = "http-compilation")]
                            if let Some(remote_endpoint) = fallback_remote {
                                Self::compile_remote(http_client, &job.uri, remote_endpoint, &config).await
                            } else {
                                CompileResultFfi::error("Failed to install Context binary and no fallback remote specified".to_string())
                            }
                            #[cfg(not(feature = "http-compilation"))]
                            CompileResultFfi::error("Failed to install Context binary".to_string())
                        }
                    }
                }
                #[cfg(not(feature = "local-install"))]
                {
                    CompileResultFfi::error("Local installation not supported in this build".to_string())
                }
            }
        };
        
        Self::complete_job(active_jobs, job, result).await;
    } 

    async fn complete_job(
        active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
        job: CompilationJob,
        result: CompileResultFfi,
    ) {
        {
            let mut jobs = active_jobs.lock().unwrap();
            jobs.remove(&job.id);
        }
        
        if let Some(callback) = job.callback {
            callback.on_compilation_complete(result);
        }
    }
    
    async fn complete_job_with_error(
        active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
        job: CompilationJob,
        error: String,
    ) {
        Self::complete_job(active_jobs, job, CompileResultFfi::error(error)).await;
    }
    
    async fn compile_local(
        runtime: Arc<Mutex<Runtime>>,
        uri: &str,
        binary_path: &PathBuf,
    ) -> CompileResultFfi {
        let result = {
            let mut runtime = runtime.lock().unwrap();
            runtime.set_context_executable(binary_path.clone());
            runtime.compile_document(uri)
        };
        result.into()
    }
    
    #[cfg(feature = "http-compilation")]
    async fn compile_remote(
        http_client: Arc<reqwest::Client>,
        uri: &str,
        endpoint: &str,
        config: &CompilationConfig,
    ) -> CompileResultFfi {
        // This is a placeholder - you'll need to implement the actual HTTP protocol
        // based on your server's API
        match Self::do_remote_compile(http_client, uri, endpoint, config).await {
            Ok(result) => result,
            Err(e) => CompileResultFfi::error(format!("Remote compilation failed: {}", e)),
        }
    }
    
    #[cfg(feature = "http-compilation")]
    async fn do_remote_compile(
        http_client: Arc<reqwest::Client>,
        uri: &str,
        endpoint: &str,
        config: &CompilationConfig,
    ) -> Result<CompileResultFfi, Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation - customize based on your API
        let mut request = http_client
            .post(endpoint)
            .timeout(Duration::from_secs(config.timeout_seconds))
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
    
    // Utility methods
    async fn find_local_binary() -> Option<PathBuf> {
        let candidates = vec![
            PathBuf::from("context"),
            PathBuf::from("/usr/local/bin/context"),
            PathBuf::from("/usr/bin/context"),
            PathBuf::from("C:\\context\\context.exe"),
        ];
        
        for path in candidates {
            if tokio::fs::metadata(&path).await.is_ok() {
                return Some(path);
            }
        }
        
        None
    }
    
    #[cfg(feature = "local-install")]
    async fn ensure_local_binary(
        install_path: &Option<String>,
        download_url: &str,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let install_dir = if let Some(path) = install_path {
            PathBuf::from(path)
        } else {
            Self::default_install_path()
        };
        
        let binary_path = install_dir.join(if cfg!(windows) { "context.exe" } else { "context" });
        
        if tokio::fs::metadata(&binary_path).await.is_ok() {
            return Ok(binary_path);
        }
        
        // Download and install
        println!("Installing Context compiler...");
        Self::download_and_install(download_url, &install_dir).await?;
        
        Ok(binary_path)
    }
    
    #[cfg(feature = "local-install")]
    fn default_install_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\context")
        } else {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".context")
        }
    }
    
    #[cfg(feature = "local-install")]
    async fn download_and_install(
        download_url: &str,
        install_dir: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Placeholder implementation
        tokio::fs::create_dir_all(install_dir).await?;
        
        // Download binary
        let client = reqwest::Client::new();
        let response = client.get(download_url).send().await?;
        let bytes = response.bytes().await?;
        
        // Write to install directory
        let binary_name = if cfg!(windows) { "context.exe" } else { "context" };
        let binary_path = install_dir.join(binary_name);
        tokio::fs::write(&binary_path, bytes).await?;
        
        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&binary_path).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&binary_path, perms).await?;
        }
        
        Ok(())
    }
    
    #[cfg(feature = "http-compilation")]
    fn get_default_remote_endpoint() -> Option<String> {
        // You can set this to your default remote endpoint
        Some("https://api.context.com/compile".to_string())
    }
}

#[derive(Debug, Clone, Default, uniffi::Record)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct CompileResultFfi {
    pub success: bool,
    pub pdf_path: Option<String>,
    pub log: String,
    pub errors: Vec<DiagnosticFfi>,
    pub warnings: Vec<DiagnosticFfi>,
}

impl CompileResultFfi {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            pdf_path: None,
            log: message.clone(),
            errors: vec![DiagnosticFfi {
                range: FfiRange { start: 0, end: 0 },
                severity: "error".to_string(),
                message,
                source: "runtime".to_string(),
            }],
            warnings: vec![],
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct HighlightFfi {
    pub range: FfiRange,
    pub kind: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticFfi {
    pub range: FfiRange,
    pub severity: String,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "http-compilation", derive(serde::Serialize, serde::Deserialize))]
pub struct FfiRange {
    pub start: u32,
    pub end: u32,
}

impl From<Result<CompilationResult, RuntimeError>> for CompileResultFfi {
    fn from(result: Result<CompilationResult, RuntimeError>) -> Self {
        match result {
            Ok(compilation_result) => compilation_result.into(),
            Err(error) => CompileResultFfi {
                success: false,
                pdf_path: None,
                log: format!("Compilation failed: {:?}", error),
                errors: vec![DiagnosticFfi {
                    range: FfiRange { start: 0, end: 0 },
                    severity: "error".to_string(),
                    message: format!("{:?}", error),
                    source: "runtime".to_string(),
                }],
                warnings: vec![],
            }
        }
    }
}

impl From<Highlight> for HighlightFfi {
    fn from(h: Highlight) -> Self {
        HighlightFfi {
            range: FfiRange {
                start: h.range.start as u32,
                end: h.range.end as u32,
            },
            kind: h.kind.to_string(),
        }
    }
}

impl From<Diagnostic> for DiagnosticFfi {
    fn from(d: Diagnostic) -> Self {
        DiagnosticFfi {
            range: FfiRange {
                start: d.span.start_byte.unwrap_or(0) as u32,
                end: d.span.end_byte.unwrap_or(0) as u32,
            },
            severity: d.severity.to_string(),
            message: d.message,
            source: d.source,
        }
    }
}

impl From<CompilationResult> for CompileResultFfi {
    fn from(result: CompilationResult) -> Self {
        CompileResultFfi {
            success: result.success,
            pdf_path: result.output_path.and_then(|p| p.to_str().map(|s| s.to_string())),
            log: result.log,
            errors: result.errors.into_iter().map(|e| DiagnosticFfi {
                range: FfiRange {
                    start: e.column,
                    end: e.column + 1,
                },
                severity: "error".to_string(),
                message: e.message,
                source: e.file,
            }).collect(),
            warnings: result.warnings.into_iter().map(|w| DiagnosticFfi {
                range: FfiRange {
                    start: w.column,
                    end: w.column + 1,
                },
                severity: "warning".to_string(),
                message: w.message,
                source: w.file,
            }).collect(),
        }
    }
}

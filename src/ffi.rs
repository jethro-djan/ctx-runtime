use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::runtime::ContextRuntime;
use crate::ffi_bridge::*; // This import is crucial for your FFI types like HighlightFfi, DiagnosticFfi, CompileResultFfi, etc.

use uniffi::{self};

#[derive(Debug, Clone)]
struct DocumentState {
    uri: String,
    content: String,
    highlights: Vec<HighlightFfi>,
    diagnostics: Vec<DiagnosticFfi>,
}

// Callback trait for live updates
#[uniffi::export(callback_interface)]
pub trait LiveUpdateCallback: Send + Sync {
    fn on_highlights_updated(&self, uri: String, highlights: Vec<HighlightFfi>);
    fn on_diagnostics_updated(&self, uri: String, diagnostics: Vec<DiagnosticFfi>);
    fn on_compilation_completed(&self, uri: String, result: CompileResultFfi);
    fn on_error(&self, error: RuntimeErrorFfi);
}

// Job tracking for async operations
#[derive(Debug, Clone)]
struct CompilationJob {
    uri: String,
    content: String,
    config: RuntimeConfigFfi,
}

#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    config: RuntimeConfigFfi,
    documents: RwLock<HashMap<String, DocumentState>>,
    // FIX 2: Correct type for the callback storage
    live_callback: Arc<RwLock<Option<Box<dyn LiveUpdateCallback>>>>,
    active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
    tokio_runtime: Arc<tokio::runtime::Runtime>,
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::new_with_config(RuntimeConfigFfi::default())
    }

    #[uniffi::constructor]
    pub fn new_with_config(config: RuntimeConfigFfi) -> Arc<Self> {
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime"));

        Arc::new(Self {
            config,
            documents: RwLock::new(HashMap::new()),
            // FIX 2 (continued): Initialize with the new type
            live_callback: Arc::new(RwLock::new(None)),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            tokio_runtime,
        })
    }

    pub fn set_live_callback(&self, callback: Option<Box<dyn LiveUpdateCallback>>) {
        if let Ok(mut cb) = self.live_callback.write() {
            *cb = callback;
        }
    }

    pub fn open(&self, uri: String, content: String) -> bool {
        let runtime = ContextRuntime::new(self.config.clone().into());

        match runtime.open_document(uri.clone(), content.clone()) {
            Ok(_) => {
                let highlights: Vec<HighlightFfi> = runtime.get_highlights(&uri)
                    .into_iter()
                    .map(Into::into)
                    .collect();

                let diagnostics: Vec<DiagnosticFfi> = runtime.get_diagnostics(&uri)
                    .into_iter()
                    .map(Into::into)
                    .collect();

                let doc_state = DocumentState {
                    uri: uri.clone(),
                    content,
                    highlights: highlights.clone(),
                    diagnostics: diagnostics.clone(),
                };

                if let Ok(mut docs) = self.documents.write() {
                    docs.insert(uri.clone(), doc_state);
                }

                self.notify_highlights_updated(&uri, highlights);
                self.notify_diagnostics_updated(&uri, diagnostics);
                true
            }
            Err(e) => {
                self.notify_error(e.into());
                false
            }
        }
    }

    pub fn update(&self, uri: String, start: u32, end: u32, new_text: String) -> bool {
        let mut updated_content = None;

        if let Ok(docs) = self.documents.read() {
            if let Some(doc) = docs.get(&uri) {
                let mut content = doc.content.clone();
                let range = (start as usize)..(end as usize);

                // Ensure range is valid
                if range.end <= content.len() && range.start <= range.end {
                    content.replace_range(range, &new_text);
                    updated_content = Some(content);
                }
            }
        }

        if let Some(content) = updated_content {
            let runtime = ContextRuntime::new(self.config.clone().into());

            match runtime.open_document(uri.clone(), content.clone()) {
                Ok(_) => {
                    let highlights: Vec<HighlightFfi> = runtime.get_highlights(&uri)
                        .into_iter()
                        .map(Into::into)
                        .collect();

                    let diagnostics: Vec<DiagnosticFfi> = runtime.get_diagnostics(&uri)
                        .into_iter()
                        .map(Into::into)
                        .collect();

                    if let Ok(mut docs) = self.documents.write() {
                        if let Some(doc) = docs.get_mut(&uri) {
                            doc.content = content;
                            doc.highlights = highlights.clone();
                            doc.diagnostics = diagnostics.clone();
                        }
                    }

                    self.notify_highlights_updated(&uri, highlights);
                    self.notify_diagnostics_updated(&uri, diagnostics);
                    true
                }
                Err(e) => {
                    self.notify_error(e.into());
                    false
                }
            }
        } else {
            // FIX 3: Correct constructor for RuntimeErrorFfi::DocumentNotFound
            self.notify_error(RuntimeErrorFfi::DocumentNotFound { uri });
            false
        }
    }

    pub fn close(&self, uri: String) {
        if let Ok(mut docs) = self.documents.write() {
            docs.remove(&uri);
        }
    }

    pub fn get_document_source(&self, uri: String) -> Option<String> {
        self.documents.read()
            .ok()
            .and_then(|docs| docs.get(&uri).map(|doc| doc.content.clone()))
    }

    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        self.documents.read()
            .ok()
            .and_then(|docs| docs.get(&uri).map(|doc| doc.highlights.clone()))
            .unwrap_or_default()
    }

    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        self.documents.read()
            .ok()
            .and_then(|docs| docs.get(&uri).map(|doc| doc.diagnostics.clone()))
            .unwrap_or_default()
    }

    pub fn compile(&self, uri: String) -> String {
        // Create job_id first and clone it for the async block
        let job_id = format!("compile_{}", uuid::Uuid::new_v4());
        let job_id_for_async = job_id.clone();

        let content = match self.get_document_source(uri.clone()) {
            Some(content) => content,
            None => {
                // FIX 3: Correct constructor for RuntimeErrorFfi::DocumentNotFound
                self.notify_error(RuntimeErrorFfi::DocumentNotFound { uri });
                return job_id;
            }
        };

        let job = CompilationJob {
            uri: uri.clone(),
            content: content.clone(),
            config: self.config.clone(),
        };

        if let Ok(mut jobs) = self.active_jobs.lock() {
            jobs.insert(job_id.clone(), job.clone());
        }

        // Clone all necessary Arc references
        let active_jobs = Arc::clone(&self.active_jobs);
        // FIX 2 (continued): Clone the correct type of live_callback
        let live_callback = Arc::clone(&self.live_callback);
        let config = self.config.clone();
        let cancelled = Arc::new(AtomicBool::new(false));

        // Spawn the async task using the cloned job_id_for_async
        self.tokio_runtime.spawn(async move {
            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            println!("Starting async compilation for job: {}", job_id_for_async);

            let ffi_result = if config.remote {
                match perform_remote_compilation(&config, &job.uri, &job.content).await {
                    Ok(result) => result,
                    Err(e) => {
                        println!("Remote compilation failed: {}", e);
                        CompileResultFfi::error(e)
                    }
                }
            } else {
                match perform_local_compilation(&job).await {
                    Ok(result) => result,
                    Err(e) => {
                        println!("Local compilation failed: {}", e);
                        CompileResultFfi::error(e)
                    }
                }
            };

            println!("Compilation completed for job {}: success={}", job_id_for_async, ffi_result.success);

            // Clean up job using the cloned ID
            if let Ok(mut jobs) = active_jobs.lock() {
                jobs.remove(&job_id_for_async);
            }

            if let Ok(cb) = live_callback.read() {
                if let Some(callback) = &*cb {
                    callback.on_compilation_completed(job.uri.clone(), ffi_result);
                }
            }

        });

        // Return the original job_id
        job_id
    }

    pub fn cancel_compilation(&self, job_id: String) -> bool {
        if let Ok(mut jobs) = self.active_jobs.lock() {
            jobs.remove(&job_id).is_some()
        } else {
            false
        }
    }

    pub fn get_active_jobs(&self) -> Vec<String> {
        self.active_jobs.lock()
            .map(|jobs| jobs.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_document_uris(&self) -> Vec<String> {
        self.documents.read()
            .map(|docs| docs.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn compile_async(&self, uri: String) -> Option<Arc<AsyncCompilationFuture>> {
        let content = self.get_document_source(uri.clone())?;
        let future = AsyncCompilationFuture::new(
            self.tokio_runtime.clone(),
            self.config.clone(),
            uri,
            content,
            Arc::clone(&self.live_callback), // This now passes the Box version
        );
        Some(Arc::new(future))
    }

    // Helper methods for notifications
    fn notify_highlights_updated(&self, uri: &str, highlights: Vec<HighlightFfi>) {
        if let Ok(cb) = self.live_callback.read() {
            if let Some(callback) = &*cb {
                callback.on_highlights_updated(uri.to_string(), highlights);
            }
        }
    }

    fn notify_diagnostics_updated(&self, uri: &str, diagnostics: Vec<DiagnosticFfi>) {
        if let Ok(cb) = self.live_callback.read() {
            if let Some(callback) = &*cb {
                callback.on_diagnostics_updated(uri.to_string(), diagnostics);
            }
        }
    }

    fn notify_error(&self, error: RuntimeErrorFfi) {
        if let Ok(cb) = self.live_callback.read() {
            if let Some(callback) = &*cb {
                callback.on_error(error);
            }
        }
    }
}


async fn perform_remote_compilation(
    config: &RuntimeConfigFfi,
    uri: &str,
    content: &str,
) -> Result<CompileResultFfi, String> {
    let server_url = config.server_url.as_ref().ok_or("No server URL configured")?;
    let request_body = CompileRequestFfi {
        uri: uri.to_string(),
        content: content.to_string(),
        format: Some("pdf".to_string()),
    };

    println!("Sending async request to: {}/compile", server_url);
    println!("Request body: uri={}, content_length={}", request_body.uri, request_body.content.len());

    let client = reqwest::Client::new();
    let mut request = client.post(&format!("{}/compile", server_url))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(30));

    if let Some(token) = &config.auth_token {
        request = request.bearer_auth(token);
        println!("Using authentication token for async request");
    }

    let response = request.send().await.map_err(|e| format!("Failed to send request: {}", e))?;
    let status = response.status();

    if !status.is_success() {
        let error_details = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Server error: {} - {}", status, error_details));
    }

    let mut result = response.json::<CompileResultFfi>().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    if let Some(pdf_path) = result.pdf_path.take() {
        println!("Original PDF path from server: {}", pdf_path);
        
    let final_url = if pdf_path.starts_with("http://") || pdf_path.starts_with("https://") {
            println!("Server returned complete URL");
            pdf_path
        } else {
            let server_url = config.server_url.as_deref().unwrap_or("").trim_end_matches('/');
            let pdf_path_trimmed = pdf_path.trim_start_matches('/');
            let constructed_url = format!("{}/{}", server_url, pdf_path_trimmed);
            println!("Constructed URL from relative path: {}", constructed_url);
            constructed_url
        };
        
        println!("Final PDF URL: {}", final_url);
        result.pdf_path = Some(final_url);
    }

    // Ensure all diagnostics have valid ranges
    result.diagnostics = result.diagnostics.into_iter().map(|d| {
        DiagnosticFfi {
            // FIX 1: Use `start`, `end`, and `severity`
            start: d.start.or(Some(0)), // Use .or(Some(0)) as they are Option<u32>
            end: d.end.or(Some(0)),     // Use .or(Some(0)) as they are Option<u32>
            severity: d.severity,
            message: d.message,
        }
    }).collect();

    Ok(result)
}

async fn perform_local_compilation(job: &CompilationJob) -> Result<CompileResultFfi, String> {
    println!("Performing local compilation");
    let runtime = ContextRuntime::new(job.config.clone().into());

    runtime.open_document(job.uri.clone(), job.content.clone())
        .map_err(|e| format!("Failed to open document: {}", e))?;

    let rt_inner = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;

    let result = rt_inner.block_on(runtime.compile_document(&job.uri))
        .map_err(|e| format!("Compilation failed: {}", e))?;

    println!("Local compilation successful");
    Ok(result.into())
}

// Async compilation future
#[derive(uniffi::Object)]
pub struct AsyncCompilationFuture {
    result: Arc<Mutex<Option<CompileResultFfi>>>,
    ready: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    // Change this to Box as well
    live_callback: Arc<RwLock<Option<Box<dyn LiveUpdateCallback>>>>,
}

impl AsyncCompilationFuture {
    fn new(
        tokio_runtime: Arc<tokio::runtime::Runtime>,
        config: RuntimeConfigFfi,
        uri: String,
        content: String,
        // Change parameter type
        live_callback: Arc<RwLock<Option<Box<dyn LiveUpdateCallback>>>>,
    ) -> Self {
        let result = Arc::new(Mutex::new(None));
        let ready = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));

        let result_clone = Arc::clone(&result);
        let ready_clone = Arc::clone(&ready);
        let cancelled_clone = Arc::clone(&cancelled);
        // FIX 2 (continued): Clone for the async move block
        let live_callback_clone = Arc::clone(&live_callback);

        let uri_for_callback = uri.clone(); // Keep original URI for callback

        tokio_runtime.spawn(async move {
            if cancelled_clone.load(Ordering::Relaxed) {
                return;
            }

            println!("Starting async compilation for URI: {}", uri);

            let ffi_result = if config.remote {
                println!("Performing remote async compilation");
                // It's generally better to call the shared `perform_remote_compilation` here
                // but adapting the existing duplicated logic as per your request for minimal changes
                let server_url = config.server_url.clone().unwrap_or_default();
                let auth_token = config.auth_token.clone();
                let request_body = CompileRequestFfi {
                    uri: uri.clone(),
                    content: content.clone(),
                    format: Some("pdf".to_string()),
                };

                println!("Sending async request to: {}/compile", server_url);
                println!("Request body: uri={}, content_length={}", request_body.uri, request_body.content.len());

                let client = reqwest::Client::new();
                let mut request = client.post(&format!("{}/compile", server_url))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .timeout(std::time::Duration::from_secs(30));

                if let Some(token) = auth_token {
                    request = request.bearer_auth(token);
                    println!("Using authentication token for async request");
                }

                match request.send().await {
                    Ok(response) => {
                        let status = response.status();
                        println!("Async compilation response status: {}", status);

                        if status.is_success() {
                            match response.json::<CompileResultFfi>().await {
                                Ok(mut result) => { // Make result mutable to potentially modify pdf_path
                                    if let Some(pdf_path) = result.pdf_path.take() {
                                        let server_url = config.server_url.as_deref().unwrap_or("").trim_end_matches('/');
                                        let pdf_path_trimmed = pdf_path.trim_start_matches('/');
                                        let full_url = if server_url.is_empty() {
                                            pdf_path_trimmed.to_string()
                                        } else {
                                            format!("{}/{}", server_url, pdf_path_trimmed)
                                        };
                                        result.pdf_path = Some(full_url);
                                    }

                                    result.diagnostics = result.diagnostics.into_iter().map(|d| {
                                        DiagnosticFfi {
                                            // FIX 1: Use `start`, `end`, and `severity`
                                            start: d.start.or(Some(0)),
                                            end: d.end.or(Some(0)),
                                            severity: d.severity,
                                            message: d.message,
                                        }
                                    }).collect();
                                    println!("Successfully parsed async compilation result: success={}", result.success);
                                    result
                                },
                                Err(e) => {
                                    let error_msg = format!("Failed to parse remote async compilation response: {}", e);
                                    println!("{}", error_msg);
                                    CompileResultFfi::error(error_msg)
                                },
                            }
                        } else {
                            let error_details = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                            let error_msg = format!("Remote async compilation failed with status: {} - {}", status, error_details);
                            println!("{}", error_msg);
                            CompileResultFfi::error(error_msg)
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Failed to send remote async compilation request: {}", e);
                        println!("{}", error_msg);
                        CompileResultFfi::error(error_msg)
                    },
                }
            } else {
                println!("Performing local async compilation");
                let compilation_result = tokio::task::spawn_blocking(move || {
                    if cancelled_clone.load(Ordering::Relaxed) {
                        // FIX: Return a proper CompileResultFfi for cancellation, perhaps with a specific message
                        return Ok(CompileResultFfi::error("Compilation cancelled".to_string()));
                    }

                    let runtime = ContextRuntime::new(config.into());
                    runtime.open_document(uri.clone(), content)
                        .and_then(|_| {
                            let rt_inner = tokio::runtime::Runtime::new()
                                .expect("Failed to create tokio runtime for local async compilation");
                            rt_inner.block_on(runtime.compile_document(&uri))
                        })
                        .map(|res| res.into()) // Convert CompilationResult to CompileResultFfi
                        .map_err(|e| format!("{}", e)) // Convert RuntimeErrors to String
                }).await;

                match compilation_result {
                    Ok(Ok(compile_result)) => {
                        println!("Local async compilation successful");
                        compile_result
                    },
                    Ok(Err(error_msg)) => {
                        let error_msg = format!("Local async compilation failed: {}", error_msg);
                        println!("{}", error_msg);
                        CompileResultFfi {
                            success: false,
                            pdf_path: None,
                            log: error_msg.clone(),
                            diagnostics: vec![DiagnosticFfi {
                                start: Some(0),
                                end: Some(0),
                                severity: "error".to_string(),
                                message: error_msg,
                            }],
                        }
                    },
                    Err(join_err) => {
                        let error_msg = format!("Async compilation task failed: {}", join_err);
                        println!("{}", error_msg);
                        CompileResultFfi {
                            success: false,
                            pdf_path: None,
                            log: error_msg.clone(),
                            diagnostics: vec![DiagnosticFfi {
                                start: Some(0),
                                end: Some(0),
                                severity: "error".to_string(),
                                message: error_msg,
                            }],
                        }
                    }
                }
            };

            println!("Async compilation completed: success={}", ffi_result.success);

            if let Ok(mut result_guard) = result_clone.lock() {
                *result_guard = Some(ffi_result.clone()); // Clone for storage
            }

            // Notify the live callback
            if let Ok(cb) = live_callback_clone.read() {
                if let Some(callback) = &*cb {
                    callback.on_compilation_completed(uri_for_callback, ffi_result); // Use uri_for_callback
                }
            }

            ready_clone.store(true, Ordering::Relaxed);
        });

        Self { result, ready, cancelled, live_callback } // Initialize live_callback
    }
}

#[uniffi::export]
impl AsyncCompilationFuture {
    pub fn poll_result(&self) -> Option<CompileResultFfi> {
        if self.is_ready() {
            self.result.lock().ok().and_then(|r| r.clone())
        } else {
            None
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) -> bool {
        self.cancelled.store(true, Ordering::Relaxed);
        true
    }
}

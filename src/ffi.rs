use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::runtime::ContextRuntime;
use crate::ffi_bridge::*; // Assuming these are defined elsewhere, e.g., in ffi_bridge.rs

use uniffi::{self};

// Document state that we can safely store (without the problematic Bump allocator)
#[derive(Debug, Clone)]
struct DocumentState {
    uri: String,
    content: String,
    // Store processed results instead of the raw syntax tree
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
    live_callback: Arc<RwLock<Option<Box<dyn LiveUpdateCallback>>>>,
    active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
    tokio_runtime: Arc<tokio::runtime::Runtime>, // Central Tokio runtime
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::new_with_config(RuntimeConfigFfi::default())
    }

    #[uniffi::constructor]
    pub fn new_with_config(config: RuntimeConfigFfi) -> Arc<Self> {
        // Create or get tokio runtime
        let tokio_runtime = Arc::new(tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime"));

        Arc::new(Self {
            config,
            documents: RwLock::new(HashMap::new()),
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
        // Create a temporary runtime to process the document
        // Consider if ContextRuntime should be persistent or managed differently
        let runtime = ContextRuntime::new(self.config.clone().into());
        
        match runtime.open_document(uri.clone(), content.clone()) {
            Ok(_) => {
                // Extract the processed data we need
                let highlights: Vec<HighlightFfi> = runtime.get_highlights(&uri)
                    .into_iter()
                    .map(Into::into)
                    .collect();
                
                let diagnostics: Vec<DiagnosticFfi> = runtime.get_diagnostics(&uri)
                    .into_iter()
                    .map(Into::into)
                    .collect();

                // Store only the safe data
                let doc_state = DocumentState {
                    uri: uri.clone(),
                    content,
                    highlights: highlights.clone(),
                    diagnostics: diagnostics.clone(),
                };

                if let Ok(mut docs) = self.documents.write() {
                    docs.insert(uri.clone(), doc_state);
                }

                // Notify callback
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
        
        // Get current content and update it
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
            // Create temporary runtime to reprocess the document
            // Consider if ContextRuntime should be persistent or managed differently
            let runtime = ContextRuntime::new(self.config.clone().into());
            
            match runtime.open_document(uri.clone(), content.clone()) {
                Ok(_) => {
                    // Get updated highlights and diagnostics
                    let highlights: Vec<HighlightFfi> = runtime.get_highlights(&uri)
                        .into_iter()
                        .map(Into::into)
                        .collect();
                    
                    let diagnostics: Vec<DiagnosticFfi> = runtime.get_diagnostics(&uri)
                        .into_iter()
                        .map(Into::into)
                        .collect();

                    // Update stored state
                    if let Ok(mut docs) = self.documents.write() {
                        if let Some(doc) = docs.get_mut(&uri) {
                            doc.content = content;
                            doc.highlights = highlights.clone();
                            doc.diagnostics = diagnostics.clone();
                        }
                    }

                    // Notify callback
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
        let job_id = format!("compile_{}", uuid::Uuid::new_v4());
        
        // Get document content
        let content = match self.get_document_source(uri.clone()) {
            Some(content) => content,
            None => {
                self.notify_error(RuntimeErrorFfi::DocumentNotFound { uri });
                return job_id;
            }
        };

        let job = CompilationJob {
            uri: uri.clone(),
            content,
            config: self.config.clone(),
        };

        // Store job
        if let Ok(mut jobs) = self.active_jobs.lock() {
            jobs.insert(job_id.clone(), job.clone());
        }

        let job_id_clone = job_id.clone();
        let active_jobs = Arc::clone(&self.active_jobs);
        let live_callback_clone = Arc::clone(&self.live_callback);
        let config_clone = self.config.clone(); // Clone config for use in the async block

        // Spawn a blocking task for the compilation
        self.tokio_runtime.spawn_blocking(move || {
            let ffi_result = if config_clone.remote {
                // Remote compilation
                let server_url = config_clone.server_url.clone().unwrap_or_default();
                let auth_token = config_clone.auth_token.clone();
                let request_body = CompileRequestFfi {
                    file_name: job.uri.clone(),
                    content: job.content.clone(),
                };

                let client = reqwest::blocking::Client::new();
                let mut request = client.post(&format!("{}/compile", server_url))
                    .json(&request_body);

                if let Some(token) = auth_token {
                    request = request.bearer_auth(token);
                }

                match request.send() {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.json::<CompileResultFfi>() {
                                Ok(result) => result,
                                Err(e) => CompileResultFfi::error(format!("Failed to parse remote compilation response: {}", e)),
                            }
                        } else {
                            CompileResultFfi::error(format!("Remote compilation failed with status: {}", response.status()))
                        }
                    },
                    Err(e) => CompileResultFfi::error(format!("Failed to send remote compilation request: {}", e)),
                }
            } else {
                // Local compilation (blocking, as ContextRuntime may not be Send/Sync)
                // Create ContextRuntime within this blocking task
                let runtime = ContextRuntime::new(job.config.into());
                let compilation_result = runtime.open_document(job.uri.clone(), job.content)
                    .and_then(|_| {
                        // For the `compile_document` which is async, we need to block on it
                        // within this blocking task, using a temporary mini-runtime or
                        // ensuring the ContextRuntime itself handles its async ops internally.
                        // Given you have `self.tokio_runtime` already, we should ideally
                        // run this on that main runtime, but since `ContextRuntime` is not Send,
                        // this is the common pattern.
                        let rt_inner = tokio::runtime::Runtime::new()
                            .expect("Failed to create tokio runtime for local compilation");
                        rt_inner.block_on(runtime.compile_document(&job.uri))
                    });

                match compilation_result {
                    Ok(res) => res.into(),
                    Err(e) => CompileResultFfi {
                        success: false,
                        pdf_path: None,
                        log: format!("{}", e),
                        diagnostics: vec![DiagnosticFfi {
                            start: 0,
                            end: 0,
                            severity: "error".to_string(),
                            message: format!("{}", e),
                        }],
                    }
                }
            };

            // Clean up job
            if let Ok(mut jobs) = active_jobs.lock() {
                jobs.remove(&job_id_clone);
            }
            
            // Notify callback
            if let Ok(cb) = live_callback_clone.read() {
                if let Some(callback) = &*cb {
                    callback.on_compilation_completed(job.uri, ffi_result);
                }
            }
        });

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

// Simplified async compilation that doesn't store problematic state
#[derive(uniffi::Object)]
pub struct AsyncCompilationFuture {
    result: Arc<Mutex<Option<CompileResultFfi>>>,
    ready: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

impl AsyncCompilationFuture {
    // Modified signature to accept tokio_runtime
    fn new(tokio_runtime: Arc<tokio::runtime::Runtime>, config: RuntimeConfigFfi, uri: String, content: String) -> Self {
        let result = Arc::new(Mutex::new(None));
        let ready = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        
        let result_clone = Arc::clone(&result);
        let ready_clone = Arc::clone(&ready);
        let cancelled_clone = Arc::clone(&cancelled);

        // Use the passed tokio_runtime for spawning
        tokio_runtime.spawn(async move {
            if cancelled_clone.load(Ordering::Relaxed) {
                return;
            }

            let ffi_result = if config.remote {
                // Remote compilation: can be fully async
                let server_url = config.server_url.clone().unwrap_or_default();
                let auth_token = config.auth_token.clone();
                let request_body = CompileRequestFfi {
                    file_name: uri.clone(),
                    content: content.clone(),
                };

                let client = reqwest::Client::new();
                let mut request = client.post(&format!("{}/compile", server_url))
                    .json(&request_body);

                if let Some(token) = auth_token {
                    request = request.bearer_auth(token);
                }

                match request.send().await {
                    Ok(response) => {
                        if response.status().is_success() {
                            match response.json::<CompileResultFfi>().await {
                                Ok(result) => result,
                                Err(e) => CompileResultFfi::error(format!("Failed to parse remote compilation response: {}", e)),
                            }
                        } else {
                            CompileResultFfi::error(format!("Remote compilation failed with status: {}", response.status()))
                        }
                    },
                    Err(e) => CompileResultFfi::error(format!("Failed to send remote compilation request: {}", e)),
                }
            } else {
                // Local compilation: needs to be spawned into a blocking task if ContextRuntime
                // is not Send and its methods are not pure async, or if they do heavy blocking I/O.
                let compilation_result = tokio::task::spawn_blocking(move || {
                    if cancelled_clone.load(Ordering::Relaxed) {
                        return Err("Compilation cancelled".to_string()); // Return a string error
                    }

                    // Create ContextRuntime in the blocking task
                    let runtime = ContextRuntime::new(config.into());
                    
                    // Open document, then compile. Both might return Result.
                    runtime.open_document(uri.clone(), content)
                        .and_then(|_| {
                            // Since compile_document is async, block on it within this blocking task.
                            // A new mini-runtime here is okay as it's isolated to this blocking task.
                            let rt_inner = tokio::runtime::Runtime::new()
                                .expect("Failed to create tokio runtime for local compilation in blocking task");
                            rt_inner.block_on(runtime.compile_document(&uri))
                        })
                        .map_err(|e| format!("{}", e)) // Convert any error to string
                }).await; // Await the join handle from spawn_blocking

                match compilation_result {
                    Ok(Ok(compile_result)) => compile_result.into(), // Outer Ok for spawn_blocking, inner Ok for actual result
                    Ok(Err(error_msg)) => { // Outer Ok for spawn_blocking, inner Err for compilation error
                        CompileResultFfi {
                            success: false,
                            pdf_path: None,
                            log: error_msg.clone(),
                            diagnostics: vec![DiagnosticFfi {
                                start: 0,
                                end: 0,
                                severity: "error".to_string(),
                                message: error_msg,
                            }],
                        }
                    }
                    Err(join_err) => { // Err from spawn_blocking (e.g., task panicked)
                        let error_msg = format!("Compilation task failed: {}", join_err);
                        CompileResultFfi {
                            success: false,
                            pdf_path: None,
                            log: error_msg.clone(),
                            diagnostics: vec![DiagnosticFfi {
                                start: 0,
                                end: 0,
                                severity: "error".to_string(),
                                message: error_msg,
                            }],
                        }
                    }
                }
            };
            
            if let Ok(mut result_guard) = result_clone.lock() {
                *result_guard = Some(ffi_result);
            }
            ready_clone.store(true, Ordering::Relaxed);
        });

        Self { result, ready, cancelled }
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

#[uniffi::export]
impl ContextRuntimeHandle {
    pub fn compile_async(&self, uri: String) -> Option<Arc<AsyncCompilationFuture>> {
        let content = self.get_document_source(uri.clone())?;
        // Pass self.tokio_runtime to AsyncCompilationFuture::new
        let future = AsyncCompilationFuture::new(self.tokio_runtime.clone(), self.config.clone(), uri, content);
        Some(Arc::new(future))
    }
}

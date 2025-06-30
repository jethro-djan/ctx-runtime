use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};

use crate::runtime::ContextRuntime;
use crate::ffi_bridge::*;

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
    live_callback: RwLock<Option<Box<dyn LiveUpdateCallback>>>,
    active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
    tokio_handle: tokio::runtime::Handle,
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
        let tokio_handle = tokio::runtime::Handle::try_current()
            .unwrap_or_else(|_| {
                // Create a new runtime if we're not in a tokio context
                let rt = tokio::runtime::Runtime::new()
                    .expect("Failed to create tokio runtime");
                let handle = rt.handle().clone();
                // Keep runtime alive by leaking it (or store it properly in a global)
                std::mem::forget(rt);
                handle
            });

        Arc::new(Self {
            config,
            documents: RwLock::new(HashMap::new()),
            live_callback: RwLock::new(None),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
            tokio_handle,
        })
    }

    pub fn set_live_callback(&self, callback: Option<Box<dyn LiveUpdateCallback>>) {
        if let Ok(mut cb) = self.live_callback.write() {
            *cb = callback;
        }
    }

    pub fn open(&self, uri: String, content: String) -> bool {
        // Create a temporary runtime to process the document
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
        
        // Use spawn_blocking since ContextRuntime is not Send
        self.tokio_handle.spawn_blocking(move || {
            // Create fresh runtime for this compilation
            let runtime = ContextRuntime::new(job.config.into());
            
            // Open document in the fresh runtime and compile synchronously
            let compilation_result = match runtime.open_document(job.uri.clone(), job.content) {
                Ok(_) => {
                    // Create new tokio runtime for the async compile call
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => return Err(format!("Failed to create runtime: {}", e)),
                    };
                    rt.block_on(runtime.compile_document(&job.uri))
                        .map_err(|e| format!("Compilation failed: {}", e))
                },
                Err(e) => Err(format!("Failed to open document: {}", e)),
            };
            
            // Convert result to FFI type
            let ffi_result = match compilation_result {
                Ok(compilation_result) => compilation_result.into(),
                Err(error_msg) => CompileResultFfi {
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
            };

            // Clean up job
            if let Ok(mut jobs) = active_jobs.lock() {
                jobs.remove(&job_id_clone);
            }
            
            Ok(ffi_result)
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

// Remove the duplicate From implementation - this should be in conversions.rs only
// impl From<RuntimeError> for RuntimeErrorFfi {
//     fn from(error: RuntimeError) -> Self {
//         match error {
//             RuntimeError::LockPoisoned => RuntimeErrorFfi::LockPoisoned,
//             RuntimeError::CompilationError { line, column, message } => {
//                 RuntimeErrorFfi::CompilationError {
//                     details: format!("Line {}, Column {}: {}", line, column, message)
//                 }
//             }
//             RuntimeError::DocumentNotFound(uri) => RuntimeErrorFfi::DocumentNotFound { uri },
//         }
//     }
// }

// Simplified async compilation that doesn't store problematic state
#[derive(uniffi::Object)]
pub struct AsyncCompilationFuture {
    result: Arc<Mutex<Option<CompileResultFfi>>>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl AsyncCompilationFuture {
    fn new(config: RuntimeConfigFfi, uri: String, content: String) -> Self {
        let result = Arc::new(Mutex::new(None));
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        
        let result_clone = Arc::clone(&result);
        let ready_clone = Arc::clone(&ready);
        let cancelled_clone = Arc::clone(&cancelled);

        // Use spawn_blocking to handle non-Send ContextRuntime
        tokio::spawn(async move {
            if cancelled_clone.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            // Create and use runtime in blocking task
            let compilation_result = tokio::task::spawn_blocking(move || {
                if cancelled_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err("Cancelled".to_string());
                }

                // Create runtime in blocking context
                let runtime = ContextRuntime::new(config.into());
                
                // Since we're in blocking context, we need to handle async calls
                // Option A: If compile_document is async, create a new tokio runtime
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| format!("Failed to create runtime: {}", e))?;
                
                let _open_result = runtime.open_document(uri.clone(), content)
                    .map_err(|e| format!("Failed to open document: {}", e))?;
                
                // Block on the async compilation
                let compile_result = rt.block_on(runtime.compile_document(&uri))
                    .map_err(|e| format!("Compilation failed: {}", e))?;
                
                Ok(compile_result)
            }).await;

            let ffi_result = match compilation_result {
                Ok(Ok(compile_result)) => compile_result.into(),
                Ok(Err(error_msg)) => {
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
                Err(join_err) => {
                    let error_msg = format!("Task failed: {}", join_err);
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
            };
            
            if let Ok(mut result_guard) = result_clone.lock() {
                *result_guard = Some(ffi_result);
            }
            ready_clone.store(true, std::sync::atomic::Ordering::Relaxed);
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
        self.ready.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn cancel(&self) -> bool {
        self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }
}

impl ContextRuntimeHandle {
    pub fn compile_async(&self, uri: String) -> Option<Arc<AsyncCompilationFuture>> {
        let content = self.get_document_source(uri.clone())?;
        let future = AsyncCompilationFuture::new(self.config.clone(), uri, content);
        Some(Arc::new(future))
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::ops::Range;
use uuid::Uuid;
use std::thread;
use std::sync::{Arc, RwLock, Mutex};

use crate::runtime_config::*;
use crate::backend_traits::*;
use crate::runtime::{ContextRuntime, RuntimeError};
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
    fn on_error(&self, error: RuntimeErrorFfi);
}

#[uniffi::export(with_foreign)]
pub trait CompilationCallback: Send + Sync + std::fmt::Debug {
    fn on_progress(&self, progress: f32);
    fn on_compilation_complete(&self, result: CompileResultFfi);
    fn on_error(&self, error: RuntimeErrorFfi);
}


#[derive(uniffi::Object)]
pub struct ContextRuntimeHandle {
    runtime: Arc<ContextRuntime>,
    live_callback: Arc<RwLock<Option<Arc<dyn LiveUpdateCallback>>>>,
    active_jobs: Arc<Mutex<HashMap<String, CompilationJob>>>,
}

#[uniffi::export]
impl ContextRuntimeHandle {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Self::new_with_config(RuntimeConfig::default())
    }

    #[uniffi::constructor]
    pub fn new_with_config(config: RuntimeConfig) -> Arc<Self> {
        Arc::new(Self {
            runtime: Arc::new(ContextRuntime::new(config)),
            live_callback: Arc::new(RwLock::new(None)),
            active_jobs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn set_live_callback(&self, callback: Option<Arc<dyn LiveUpdateCallback>>) {
        if let Ok(mut cb) = self.live_callback.write() {
            *cb = callback;
        }
    }

    pub fn open(&self, uri: String, content: String) -> bool {
        match self.runtime.open_document(uri.clone(), content) {
            Ok(_) => {
                if let Ok(cb) = self.live_callback.read() {
                    if let Some(callback) = &*cb {
                        callback.on_highlights_updated(
                            uri.clone(), 
                            self.runtime.get_highlights(&uri).into_iter().map(Into::into).collect()
                        );
                        callback.on_diagnostics_updated(
                            uri,
                            self.runtime.get_diagnostics(&uri).into_iter().map(Into::into).collect()
                        );
                    }
                }
                true
            }
            Err(e) => {
                if let Ok(cb) = self.live_callback.read() {
                    if let Some(callback) = &*cb {
                        callback.on_error(e.into());
                    }
                }
                false
            }
        }
    }

    pub fn update(&self, uri: String, start: u32, end: u32, new_text: String) -> bool {
        let range = (start as usize)..(end as usize);
        match self.runtime.update_document(&uri, range, &new_text) {
            Ok(_) => {
                if let Ok(cb) = self.live_callback.read() {
                    if let Some(callback) = &*cb {
                        callback.on_highlights_updated(
                            uri.clone(),
                            self.runtime.get_highlights(&uri).into_iter().map(Into::into).collect()
                        );
                        callback.on_diagnostics_updated(
                            uri,
                            self.runtime.get_diagnostics(&uri).into_iter().map(Into::into).collect()
                        );
                    }
                }
                true
            }
            Err(e) => {
                if let Ok(cb) = self.live_callback.read() {
                    if let Some(callback) = &*cb {
                        callback.on_error(e.into());
                    }
                }
                false
            }
        }
    }

    pub fn close(&self, uri: String) {
        self.runtime.close_document(&uri);
    }

    pub fn get_document_source(&self, uri: String) -> Option<String> {
        self.runtime.get_document_source(&uri)
    }

    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFfi> {
        self.runtime.get_highlights(&uri).into_iter().map(Into::into).collect()
    }

    pub fn get_diagnostics(&self, uri: String) -> Vec<DiagnosticFfi> {
        self.runtime.get_diagnostics(&uri).into_iter().map(Into::into).collect()
    }

    pub fn compile(&self, uri: String, callback: Option<Arc<dyn CompilationCallback>>) -> String {
        let job_id = Uuid::new_v4().to_string();
        let runtime = Arc::clone(&self.runtime);
        let active_jobs = Arc::clone(&self.active_jobs);

        tokio::spawn(async move {
            let result = runtime.compile_document(&uri).await;
            active_jobs.lock().unwrap().remove(&job_id);
            
            if let Some(cb) = callback {
                match result {
                    Ok(result) => cb.on_compilation_complete(result.into()),
                    Err(e) => cb.on_error(e.into()),
                }
            }
        });

        self.active_jobs.lock().unwrap().insert(
            job_id.clone(),
            CompilationJob { uri, callback },
        );
        job_id
    }

    pub fn cancel_compilation(&self, job_id: String) -> bool {
        self.active_jobs.lock().unwrap().remove(&job_id).is_some()
    }

    pub fn set_context_executable(&self, path: String) {
        self.runtime.set_executable(PathBuf::from(path));
    }

    pub fn set_working_directory(&self, path: String) {
        self.runtime.set_working_directory(PathBuf::from(path));
    }

    pub fn context_executable_exists(&self) -> bool {
        self.runtime.executable_exists()
    }

    pub fn get_context_executable_path(&self) -> String {
        self.runtime.get_executable().to_string_lossy().to_string()
    }

    pub fn shutdown(&self) {
        self.active_jobs.lock().unwrap().clear();
    }
}

struct CompilationJob {
    uri: String,
    callback: Option<Arc<dyn CompilationCallback>>,
}

// FFI-compatible result type
#[derive(uniffi::Record)]
pub struct CompileResultFfi {
    pub success: bool,
    pub pdf_path: Option<String>,
    pub log: String,
    pub diagnostics: Vec<DiagnosticFfi>,
}

impl From<CompilationResult> for CompileResultFfi {
    fn from(result: CompilationResult) -> Self {
        Self {
            success: result.success,
            pdf_path: result.pdf_path.map(|p| p.to_string_lossy().to_string()),
            log: result.log,
            diagnostics: result.errors.into_iter().map(|e| DiagnosticFfi {
                range: None,
                severity: "error".to_string(),
                message: e.message,
                source: "compiler".to_string(),
            }).chain(result.warnings.into_iter().map(|w| DiagnosticFfi {
                range: None,
                severity: "warning".to_string(),
                message: w.message,
                source: "compiler".to_string(),
            })).collect(),
        }
    }
}

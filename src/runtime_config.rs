use std::path::PathBuf;

// #[derive(Debug, Clone, PartialEq, Eq, Default)]
// pub struct RuntimeConfig {
//     pub remote: bool,
//     pub server_url: Option<String>,
// }

// #[derive(Clone)]
// pub struct RuntimeConfig {
//     pub platform: PlatformType,
//     pub local_executable: Option<PathBuf>,
//     pub remote_endpoint: Option<String>, 
//     pub auth_token: Option<String>,  
// }
// 
// #[derive(Clone)]
// pub enum PlatformType {
//     Desktop,
//     Mobile,
// }

impl RuntimeConfig {
    pub fn is_mobile(&self) -> bool {
        matches!(self.platform, PlatformType::Mobile)
    }

    pub fn desktop_default() -> Self {
        Self {
            platform: PlatformType::Desktop,
            local_executable: None, // Auto-detect
            remote_endpoint: None,
            auth_token: None,
        }
    }

    pub fn mobile_default(endpoint: String, auth_token: Option<String>) -> Self {
        Self {
            platform: PlatformType::Mobile,
            local_executable: None,
            remote_endpoint: Some(endpoint),
            auth_token,
        }
    }
}

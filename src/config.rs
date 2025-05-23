use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Default summarization prompt
    pub default_summarization_prompt: String,
    
    /// Maximum file size to process (in bytes)
    pub max_file_size: usize,
    
    /// Project root directory
    pub project_root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_summarization_prompt: "Provide a concise summary of this code, focusing on its purpose, main functionality, and key components.".to_string(),
            max_file_size: 10 * 1024 * 1024, // 10MB
            project_root: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }
}

impl Config {
    /// Load configuration from environment variables with defaults
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();
        
        if let Ok(val) = env::var("LLM_CONTEXT_DEFAULT_PROMPT") {
            config.default_summarization_prompt = val;
        }
        
        if let Ok(val) = env::var("LLM_CONTEXT_MAX_FILE_SIZE") {
            config.max_file_size = val.parse()?;
        }
        
        if let Ok(val) = env::var("LLM_CONTEXT_PROJECT_ROOT") {
            config.project_root = PathBuf::from(val);
        }
        
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
        assert!(!config.default_summarization_prompt.is_empty());
    }
    
    #[test]
    fn test_from_env() {
        unsafe {
            env::set_var("LLM_CONTEXT_MAX_FILE_SIZE", "5242880");
        }
        
        let config = Config::from_env().unwrap();
        assert_eq!(config.max_file_size, 5242880);
        
        // Clean up
        unsafe {
            env::remove_var("LLM_CONTEXT_MAX_FILE_SIZE");
        }
    }
}
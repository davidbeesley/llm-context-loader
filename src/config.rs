use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Rate limit for API requests (requests per minute)
    pub rate_limit: u32,
    
    /// Backoff multiplier for retry logic
    pub backoff_multiplier: f64,
    
    /// Maximum number of retries for failed requests
    pub max_retries: u32,
    
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: u64,
    
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
            rate_limit: 60,
            backoff_multiplier: 2.0,
            max_retries: 3,
            cache_ttl_seconds: 300,
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
        
        if let Ok(val) = env::var("LLM_CONTEXT_RATE_LIMIT") {
            config.rate_limit = val.parse()?;
        }
        
        if let Ok(val) = env::var("LLM_CONTEXT_BACKOFF_MULTIPLIER") {
            config.backoff_multiplier = val.parse()?;
        }
        
        if let Ok(val) = env::var("LLM_CONTEXT_MAX_RETRIES") {
            config.max_retries = val.parse()?;
        }
        
        if let Ok(val) = env::var("LLM_CONTEXT_CACHE_TTL") {
            config.cache_ttl_seconds = val.parse()?;
        }
        
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
        assert_eq!(config.rate_limit, 60);
        assert_eq!(config.backoff_multiplier, 2.0);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.cache_ttl_seconds, 300);
        assert_eq!(config.max_file_size, 10 * 1024 * 1024);
    }
    
    #[test]
    fn test_from_env() {
        unsafe {
            env::set_var("LLM_CONTEXT_RATE_LIMIT", "120");
            env::set_var("LLM_CONTEXT_MAX_RETRIES", "5");
        }
        
        let config = Config::from_env().unwrap();
        assert_eq!(config.rate_limit, 120);
        assert_eq!(config.max_retries, 5);
        
        // Clean up
        unsafe {
            env::remove_var("LLM_CONTEXT_RATE_LIMIT");
            env::remove_var("LLM_CONTEXT_MAX_RETRIES");
        }
    }
}
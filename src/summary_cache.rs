use anyhow::{Context, Result};
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Cache of file summaries
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SummaryCache {
    /// Map from file path hash to summary info
    entries: HashMap<String, SummaryEntry>,
}

/// Entry in the summary cache
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SummaryEntry {
    /// Content hash of the file when it was summarized
    pub content_hash: String,
    /// Timestamp when the summary was created
    pub timestamp: u64,
    /// The generated summary
    pub summary: String,
}

impl SummaryCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Get a summary from the cache
    pub fn get_summary(&self, file_path: &Path, content_hash: &str) -> Option<&str> {
        let path_hash = hash_path(file_path);
        match self.entries.get(&path_hash) {
            Some(entry) if entry.content_hash == content_hash => Some(&entry.summary),
            _ => None,
        }
    }


    /// Cleans up summaries that no longer exist in the filesystem
    /// This preserves all valid summaries regardless of age
    pub fn cleanup(&mut self, base_dir: &Path) -> Result<()> {
        // Get all files in the project
        let mut existing_files = HashSet::new();
        for entry in walkdir::WalkDir::new(base_dir).follow_links(true) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            
            if entry.file_type().is_file() {
                existing_files.insert(hash_path(entry.path()));
            }
        }
        
        // Keep only the summaries that correspond to existing files
        let to_remove: Vec<String> = self.entries.keys()
            .filter(|path_hash| !existing_files.contains(*path_hash))
            .cloned()
            .collect();
            
        // Remove the orphaned summaries
        for path_hash in to_remove {
            self.entries.remove(&path_hash);
        }
        
        Ok(())
    }
}

/// Calculate a hash for any hashable value
fn calculate_hash<T: Hash>(value: T) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Calculate a hash for a file path
fn hash_path(path: &Path) -> String {
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    calculate_hash(canonical_path)
}

/// Calculate a hash for file content
pub fn hash_content(content: &str) -> String {
    calculate_hash(content)
}

/// Load summary cache from disk
pub fn load_summary_cache(base_dir: &Path) -> Result<SummaryCache> {
    let cache_path = base_dir.join(".claude-summaries");

    if cache_path.exists() {
        let cache_content = fs::read_to_string(&cache_path).context("Failed to read summary cache file")?;

        match serde_json::from_str(&cache_content) {
            Ok(cache) => {
                info!("Loaded summary cache from {}", cache_path.display());
                Ok(cache)
            }
            Err(e) => {
                warn!("Invalid summary cache file format. Creating a new one: {}", e);
                Ok(SummaryCache::new())
            }
        }
    } else {
        Ok(SummaryCache::new())
    }
}

/// Save summary cache to disk
pub fn save_summary_cache(base_dir: &Path, cache: &SummaryCache) -> Result<()> {
    let cache_path = base_dir.join(".claude-summaries");

    let cache_content = serde_json::to_string_pretty(cache).context("Failed to serialize summary cache")?;

    fs::write(&cache_path, cache_content).context("Failed to write summary cache file")?;

    info!("Summary cache saved to {}", cache_path.display());

    Ok(())
}
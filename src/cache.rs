use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
//use serde::Deserialize;
use log::{info, warn};

pub type CacheMap = HashMap<PathBuf, String>;

/// Load the .claude_include cache file if it exists
pub fn load_cache(directory: &Path) -> Result<CacheMap> {
    let cache_path = directory.join(".claude_include");

    if cache_path.exists() {
        let cache_content = fs::read_to_string(&cache_path).context("Failed to read cache file")?;

        match serde_json::from_str(&cache_content) {
            Ok(cache) => {
                info!("Loaded cache from {}", cache_path.display());
                Ok(cache)
            }
            Err(e) => {
                warn!("Invalid cache file format. Creating a new one: {}", e);
                Ok(CacheMap::new())
            }
        }
    } else {
        Ok(CacheMap::new())
    }
}

/// Save the cache of file actions to .claude_include
pub fn save_cache(directory: &Path, cache: &CacheMap) -> Result<()> {
    let cache_path = directory.join(".claude_include");

    let cache_content = serde_json::to_string_pretty(cache).context("Failed to serialize cache")?;

    fs::write(&cache_path, cache_content).context("Failed to write cache file")?;

    info!("Cache saved to {}", cache_path.display());

    Ok(())
}

/// Get the cached action for a path if it exists in the cache
pub fn get_action_for_path(path: &Path, cache: &CacheMap) -> Option<String> {
    cache.get(path).cloned()
}

/// Determine if we should prompt for a directory or use cached actions
pub fn should_prompt_for_directory(
    directory: &Path,
    dir_info: &HashMap<PathBuf, crate::file_analysis::DirInfo>,
    cache: &CacheMap,
) -> bool {
    if !dir_info.contains_key(directory) {
        return true;
    }

    // Check if all files in the directory and subdirectories are in the cache
    let mut all_files = Vec::new();
    let mut all_dirs = vec![directory.to_path_buf()];

    // Collect all files in this directory and its subdirectories
    let mut i = 0;
    while i < all_dirs.len() {
        let current_dir = &all_dirs[i];
        if let Some(dir_info) = dir_info.get(current_dir) {
            // Add all files in this directory
            all_files.extend(dir_info.files.iter().map(|f| f.path.clone()));
            // Add all subdirectories
            all_dirs.extend(dir_info.subdirs.clone());
        }
        i += 1;
    }

    // Check if all files are in the cache
    for file_path in all_files {
        if !cache.contains_key(&file_path) {
            return true;
        }
    }

    false
}

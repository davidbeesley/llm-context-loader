use llm_context_loader::cache::{load_cache, save_cache, get_action_for_path, should_prompt_for_directory};
use llm_context_loader::file_analysis::DirInfo;
use tempfile::TempDir;
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

#[test]
fn test_save_and_load_cache() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_file.txt");
    
    // Create a test file
    File::create(&file_path).unwrap();
    
    // Create a cache with one entry
    let mut cache = HashMap::new();
    cache.insert(file_path.clone(), "read".to_string());
    
    // Save the cache
    save_cache(temp_dir.path(), &cache).unwrap();
    
    // Verify cache file exists
    let cache_path = temp_dir.path().join(".claude_include");
    assert!(cache_path.exists());
    
    // Load the cache
    let loaded_cache = load_cache(temp_dir.path()).unwrap();
    
    // Verify content
    assert_eq!(loaded_cache.len(), 1);
    assert_eq!(loaded_cache.get(&file_path).unwrap(), "read");
}

#[test]
fn test_get_action_for_path() {
    let mut cache = HashMap::new();
    let path = PathBuf::from("/test/file.txt");
    
    // No action initially
    assert_eq!(get_action_for_path(&path, &cache), None);
    
    // Add action and test
    cache.insert(path.clone(), "summarize".to_string());
    assert_eq!(get_action_for_path(&path, &cache), Some("summarize".to_string()));
}

#[test]
fn test_should_prompt_for_directory() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_path_buf();
    let file_path = dir_path.join("file.txt");
    
    // Create a file
    File::create(&file_path).unwrap();
    
    // Setup directory info
    let mut dir_info = HashMap::new();
    let mut info = DirInfo::default();
    let file_info = llm_context_loader::file_analysis::FileInfo {
        path: file_path.clone(),
        binary: false,
        tokens: 100,
        size: 10,
        ext: ".txt".to_string(),
    };
    info.files.push(file_info);
    dir_info.insert(dir_path.clone(), info);
    
    // Create cache
    let mut cache = HashMap::new();
    
    // Should prompt because file is not in cache
    assert!(should_prompt_for_directory(&dir_path, &dir_info, &cache));
    
    // Add file to cache
    cache.insert(file_path.clone(), "read".to_string());
    
    // Should not prompt because all files are in cache
    assert!(!should_prompt_for_directory(&dir_path, &dir_info, &cache));
}
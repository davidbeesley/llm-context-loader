use llm_context_loader::summary_cache::{SummaryCache, hash_content};
use tempfile::TempDir;
use std::fs::{self, File};
use std::io::Write;

#[test]
fn test_summary_cache_get_summary() {
    let mut cache = SummaryCache::new();
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_file.txt");
    
    // Create a test file
    let content = "Test content for cache";
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", content).unwrap();
    
    // Calculate hash
    let content_hash = hash_content(content);
    
    // Insert into cache
    cache.insert_summary(&file_path, &content_hash, "Test summary".to_string());
    
    // Test retrieval
    let summary = cache.get_summary(&file_path, &content_hash);
    assert_eq!(summary, Some("Test summary"));
    
    // Test with wrong content hash
    let different_hash = hash_content("Different content");
    let summary = cache.get_summary(&file_path, &different_hash);
    assert_eq!(summary, None);
}

#[test]
fn test_summary_cache_cleanup() {
    let mut cache = SummaryCache::new();
    let temp_dir = TempDir::new().unwrap();
    
    // Create test files
    let file1_path = temp_dir.path().join("file1.txt");
    let file2_path = temp_dir.path().join("file2.txt");
    
    File::create(&file1_path).unwrap();
    File::create(&file2_path).unwrap();
    
    // Add both files to cache
    let content_hash = hash_content("test");
    cache.insert_summary(&file1_path, &content_hash, "Summary 1".to_string());
    cache.insert_summary(&file2_path, &content_hash, "Summary 2".to_string());
    
    // Delete one file
    fs::remove_file(&file2_path).unwrap();
    
    // Run cleanup
    cache.cleanup(temp_dir.path()).unwrap();
    
    // Verify file1 is still in cache but file2 is gone
    assert!(cache.get_summary(&file1_path, &content_hash).is_some());
    assert!(cache.get_summary(&file2_path, &content_hash).is_none());
}
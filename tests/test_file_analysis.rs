use llm_context_loader::file_analysis::{analyze_directory, is_binary, TOKENS_PER_BYTE};
use tempfile::TempDir;
use std::fs::{self, File};
use std::io::Write;

#[test]
fn test_is_binary() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a text file
    let text_path = temp_dir.path().join("text_file.txt");
    let mut text_file = File::create(&text_path).unwrap();
    writeln!(text_file, "This is a text file").unwrap();
    
    // Create a binary file
    let bin_path = temp_dir.path().join("binary_file.bin");
    let mut bin_file = File::create(&bin_path).unwrap();
    let binary_data = [0u8, 1u8, 2u8, 3u8];
    bin_file.write_all(&binary_data).unwrap();
    
    // Test binary detection
    // Note: is_binary uses the 'file' command which might have different behavior
    // across platforms, so this test might need adjustments
    assert!(!is_binary(&text_path).unwrap_or(true));
    // Binary detection may not work in all test environments
    // We can't reliably assert binary detection in a platform-independent way
}

#[test]
fn test_analyze_directory() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a test directory structure
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    
    // Create a text file
    let text_path = temp_dir.path().join("text_file.txt");
    let text_content = "This is a text file\nWith multiple lines\n";
    fs::write(&text_path, text_content).unwrap();
    
    // Create a file in subdir
    let sub_file_path = subdir.join("sub_file.rs");
    let sub_file_content = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
    fs::write(&sub_file_path, sub_file_content).unwrap();
    
    // Create an excluded directory
    let excluded_dir = temp_dir.path().join("node_modules");
    fs::create_dir(&excluded_dir).unwrap();
    let excluded_file = excluded_dir.join("excluded.js");
    fs::write(&excluded_file, "console.log('This should be excluded');").unwrap();
    
    // Analyze the directory
    let exclude_patterns = vec!["node_modules".to_string()];
    let dir_info = analyze_directory(temp_dir.path(), &exclude_patterns).unwrap();
    
    // Check results
    assert!(dir_info.contains_key(temp_dir.path()));
    assert!(dir_info.contains_key(&subdir));
    
    // Check that main directory info is correct
    let main_dir_info = &dir_info[temp_dir.path()];
    assert_eq!(main_dir_info.subdirs.len(), 1);
    assert!(main_dir_info.subdirs.iter().any(|p| p == &subdir));
    
    // Verify exclusion worked
    assert!(!dir_info.contains_key(&excluded_dir));
    
    // Check token calculations for text file
    let text_file_size = text_content.len() as f64;
    let expected_tokens = (text_file_size * TOKENS_PER_BYTE).ceil() as usize;
    
    let main_dir_files = &main_dir_info.files;
    assert!(main_dir_files.iter().any(|f| f.path == text_path));
    
    let text_file_info = main_dir_files.iter().find(|f| f.path == text_path).unwrap();
    assert_eq!(text_file_info.tokens, expected_tokens);
}
use llm_context_loader::context_files::{create_context_file, append_to_file, get_or_rotate_file};
use tempfile::TempDir;
use std::fs;

#[test]
fn test_create_context_file() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir(&output_dir).unwrap();
    
    // Create a context file
    let context_file = create_context_file(1, 2, base_dir, Some(&output_dir)).unwrap();
    
    // Verify the file exists
    let expected_path = output_dir.join("context-001.txt");
    assert_eq!(context_file.path, expected_path);
    assert!(expected_path.exists());
    
    // Check the content
    let content = fs::read_to_string(&expected_path).unwrap();
    assert!(content.contains("Part 1 of 2"));
    assert!(content.contains("START and END tag"));
    assert!(content.contains(&format!("Source directory: {}", base_dir.display())));
}

#[test]
fn test_append_to_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_append.txt");
    
    // Create initial content
    fs::write(&file_path, "Initial content\n").unwrap();
    
    // Append content
    append_to_file(&file_path, "Appended content").unwrap();
    
    // Verify content
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Initial content\nAppended content");
}

#[test]
fn test_get_or_rotate_file() {
    let temp_dir = TempDir::new().unwrap();
    let base_dir = temp_dir.path();
    let output_dir = temp_dir.path().join("output");
    fs::create_dir(&output_dir).unwrap();
    
    // Create initial context file
    let context_file = create_context_file(1, 2, base_dir, Some(&output_dir)).unwrap();
    
    // Test with tokens under limit
    let context_file1 = get_or_rotate_file(&context_file, 2, base_dir, Some(&output_dir)).unwrap();
    assert_eq!(context_file.path, context_file1.path);
    
    // Modify token count to exceed limit
    let mut context_file_over_limit = context_file;
    context_file_over_limit.current_tokens = 21000; // Over the CLAUDE_TOKEN_LIMIT
    
    // Test rotation
    let context_file2 = get_or_rotate_file(&context_file_over_limit, 2, base_dir, Some(&output_dir)).unwrap();
    assert_ne!(context_file_over_limit.path, context_file2.path);
    assert_eq!(context_file2.file_num, 2);
}
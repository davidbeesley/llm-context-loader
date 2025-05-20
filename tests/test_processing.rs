use llm_context_loader::processing::{Action, process_directory_content};
use llm_context_loader::context_files::ContextFile;
use llm_context_loader::file_analysis::{DirInfo, FileInfo};
use tempfile::TempDir;
use std::collections::HashMap;
use std::fs::{self, File};

#[test]
fn test_action_parse_str() {
    assert_eq!(Action::parse_str("read"), Some(Action::Read));
    assert_eq!(Action::parse_str("exclude"), Some(Action::Exclude));
    assert_eq!(Action::parse_str("enter"), Some(Action::Enter));
    assert_eq!(Action::parse_str("summarize"), Some(Action::Summarize));
    assert_eq!(Action::parse_str("stats"), Some(Action::Stats));
    assert_eq!(Action::parse_str("invalid"), None);
}

#[test]
fn test_process_directory_content() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path().to_path_buf();
    
    // Create a test context file
    let context_file_path = temp_dir.path().join("context.txt");
    File::create(&context_file_path).unwrap();
    let mut context_file = ContextFile {
        path: context_file_path,
        file_num: 1,
        current_tokens: 0,
    };
    
    // Create directory info
    let mut dir_info = HashMap::new();
    let mut info = DirInfo::default();
    info.total_files = 2;
    info.binary_files = 1;
    info.tokens = 500;
    
    // Add file info
    let file_info = FileInfo {
        path: dir_path.join("file.txt"),
        binary: false,
        tokens: 300,
        size: 1000,
        ext: ".txt".to_string(),
    };
    info.files.push(file_info);
    
    // Add binary file info
    let bin_file_info = FileInfo {
        path: dir_path.join("image.png"),
        binary: true,
        tokens: 0,
        size: 5000,
        ext: ".png".to_string(),
    };
    info.files.push(bin_file_info);
    
    dir_info.insert(dir_path.clone(), info);
    
    // Process directory with Stats action
    let result = process_directory_content(
        &dir_path,
        &dir_info,
        &mut context_file,
        &Action::Stats,
        1,
        temp_dir.path(),
        None
    ).unwrap();
    
    // Verify result
    assert_eq!(result.len(), 1);
    
    // Read content from context file
    let content = fs::read_to_string(&context_file.path).unwrap();
    
    // Verify content contains stats
    assert!(content.contains("DIRECTORY"));
    assert!(content.contains("Files: 2 (1 text)"));
    assert!(content.contains("Tokens: ~500"));
}

// More comprehensive processing tests would need some mocking
// of file system operations that are performed during processing
#[test]
fn test_action_from_str() {
    use std::str::FromStr;
    
    assert_eq!(Action::from_str("read").unwrap(), Action::Read);
    assert_eq!(Action::from_str("exclude").unwrap(), Action::Exclude);
    assert!(Action::from_str("invalid").is_err());
}
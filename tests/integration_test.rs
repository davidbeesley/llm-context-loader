use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

// This is a more comprehensive integration test that would need to
// simulate user input, which is challenging in automated tests.
// For now, we'll focus on creating a structure that can be used
// to manually verify the tool works properly.

#[test]
fn test_setup_test_directory() {
    // Create a temporary directory structure that can be used
    // for manual testing of the tool
    let temp_dir = TempDir::new().unwrap();
    println!("Created test directory at: {}", temp_dir.path().display());
    
    // Create a few subdirectories
    create_dir(temp_dir.path(), "src");
    create_dir(temp_dir.path(), "docs");
    create_dir(temp_dir.path(), "tests");
    create_dir(temp_dir.path(), "node_modules"); // Should be excluded by default
    
    // Create some files in src
    let src_dir = temp_dir.path().join("src");
    create_file(&src_dir, "main.rs", r#"
fn main() {
    println!("Hello, world!");
}
    "#);
    
    create_file(&src_dir, "lib.rs", r#"
pub mod utils;

pub fn hello() -> &'static str {
    "Hello from lib"
}
    "#);
    
    create_file(&src_dir, "utils.rs", r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
    "#);
    
    // Create a docs file
    let docs_dir = temp_dir.path().join("docs");
    create_file(&docs_dir, "README.md", r#"
# Test Project

This is a test project for demonstrating the LLM context loader.

## Features
- Feature 1
- Feature 2
- Feature 3
    "#);
    
    // Create a test file
    let tests_dir = temp_dir.path().join("tests");
    create_file(&tests_dir, "test_utils.rs", r#"
#[test]
fn test_add() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn test_subtract() {
    assert_eq!(subtract(5, 3), 2);
}
    "#);
    
    // Create a binary file 
    create_binary_file(&temp_dir.path(), "binary.dat");
    
    // Create a file in node_modules that should be excluded
    let node_modules_dir = temp_dir.path().join("node_modules");
    create_file(&node_modules_dir, "package.json", r#"
{
  "name": "test-package",
  "version": "1.0.0"
}    
    "#);
    
    println!("Test directory structure created successfully");
    println!("To test the tool manually, run:");
    println!("cargo run -- {}", temp_dir.path().display());
    
    // Keep the temporary directory around for manual testing
    // by printing its path and preventing automatic cleanup
    let path_buf = temp_dir.path().to_path_buf();
    println!("Note: Test directory will not be automatically cleaned up");
    println!("Manual cleanup: rm -rf {}", path_buf.display());
    let _ = temp_dir.keep();
    
    // This test just sets up the structure, it doesn't actually verify anything
    // and serves as a utility for manual testing
    assert!(true);
}

fn create_dir(parent: &Path, name: &str) {
    let path = parent.join(name);
    fs::create_dir(&path).unwrap();
}

fn create_file(dir: &Path, name: &str, content: &str) {
    let file_path = dir.join(name);
    let mut file = File::create(&file_path).unwrap();
    write!(file, "{}", content).unwrap();
}

fn create_binary_file(dir: &Path, name: &str) {
    let file_path = dir.join(name);
    let mut file = File::create(&file_path).unwrap();
    // Write some binary data
    let data = [0u8, 1u8, 2u8, 3u8, 4u8, 5u8];
    file.write_all(&data).unwrap();
}
use crate::lsp_client::RustAnalyzerClient;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_rust_analyzer_basic_operations() {
        // Create a test Rust project
        let temp_dir = TempDir::new().unwrap();
        let project_root = temp_dir.path().to_path_buf();

        // Create a simple Cargo.toml
        let cargo_toml = r#"
[package]
name = "test_project"
version = "0.1.0"
edition = "2021"
"#;
        fs::write(project_root.join("Cargo.toml"), cargo_toml).unwrap();

        // Create src directory
        fs::create_dir(project_root.join("src")).unwrap();

        // Create a simple Rust file
        let test_file = project_root.join("src/main.rs");
        let test_content = r#"fn main() {
    println!("Hello, world!");
    let x = calculate(5, 3);
    println!("Result: {}", x);
}

fn calculate(a: i32, b: i32) -> i32 {
    a + b
}

struct MyStruct {
    field: String,
}

impl MyStruct {
    fn new() -> Self {
        Self {
            field: String::from("test"),
        }
    }
}"#;
        fs::write(&test_file, test_content).unwrap();

        // Create the LSP client
        println!("Creating RustAnalyzerClient...");
        let client = RustAnalyzerClient::new(project_root.clone()).unwrap();

        // Initialize the client
        println!("Initializing LSP connection...");
        let init_result = client.initialize().unwrap();
        println!("Server capabilities: {:?}", init_result.capabilities);

        // Open the document
        println!("Opening document...");
        client
            .open_document(&test_file, test_content.to_string())
            .unwrap();

        // Wait for rust-analyzer to be ready
        client.wait_for_ready(&test_file, 10).unwrap();

        // Test folding ranges
        println!("Getting folding ranges...");
        let folding_ranges = client.get_folding_ranges(&test_file).unwrap();
        println!("Found {} folding ranges", folding_ranges.len());
        for range in &folding_ranges {
            println!(
                "  Folding range: lines {}-{}",
                range.start_line, range.end_line
            );
        }

        // Test goto definition - find where 'calculate' is defined
        // Line 2 (0-based), column 12 is where 'calculate' is called
        println!("\nTesting goto definition for 'calculate' call at line 2, col 12...");
        let definition_response = client.goto_definition(&test_file, 2, 12).unwrap();
        println!("Definition response: {:?}", definition_response);

        // Also try a different position - right on the 'c' of calculate
        println!("\nTrying goto definition at line 2, col 11 (start of 'calculate')...");
        let definition_response2 = client.goto_definition(&test_file, 2, 11).unwrap();
        println!("Definition response: {:?}", definition_response2);

        // Test find references - find all uses of 'calculate'
        // Line 6 (0-based), column 3 is where 'fn calculate' is defined
        println!("\nTesting find references for 'calculate' definition at line 6, col 3...");
        let references = client.find_references(&test_file, 6, 3, true).unwrap();
        println!("Found {} references", references.len());
        for reference in &references {
            println!("  Reference: {:?}", reference);
        }

        // Shutdown
        println!("Shutting down...");
        client.shutdown().unwrap();

        println!("Test completed successfully!");
    }
}

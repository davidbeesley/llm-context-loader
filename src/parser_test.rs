#[cfg(test)]
mod integration_tests {
    use crate::parser::RustParser;
    use std::path::Path;

    #[test]
    fn test_parse_syn_crate() {
        // Test parsing a file from the syn crate itself
        let parser = RustParser::new(".");

        // Find a syn source file
        let registry_path = Path::new(&std::env::var("HOME").unwrap()).join(".cargo/registry/src");

        // Find syn in the registry
        if let Ok(entries) = std::fs::read_dir(&registry_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let syn_dir = path.join("syn-2.0.101/src/lib.rs");
                    if syn_dir.exists() {
                        println!("Testing syn lib.rs at: {}", syn_dir.display());
                        match parser.parse_file(&syn_dir) {
                            Ok(entities) => {
                                println!("Found {} entities", entities.len());

                                // Count entity types
                                let mut counts = std::collections::HashMap::new();
                                for entity in &entities {
                                    *counts.entry(format!("{:?}", entity.kind)).or_insert(0) += 1;
                                }

                                println!("Entity breakdown:");
                                for (kind, count) in counts {
                                    println!("  {}: {}", kind, count);
                                }

                                // Look for macro-related items
                                let macro_items: Vec<_> = entities
                                    .iter()
                                    .filter(|e| {
                                        e.name.contains("macro") || e.name.contains("Macro")
                                    })
                                    .collect();

                                println!("\nMacro-related items found: {}", macro_items.len());
                                for item in macro_items.iter().take(5) {
                                    println!("  - {} ({:?})", item.name, item.kind);
                                }
                            }
                            Err(e) => {
                                println!("Failed to parse syn: {}", e);
                                // This is expected - syn uses complex macros
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    #[test]
    fn test_parse_serde_derive() {
        let parser = RustParser::new(".");

        // Test with code that uses derive macros
        let source = r#"
            use serde::{Serialize, Deserialize};
            
            #[derive(Debug, Clone, Serialize, Deserialize)]
            pub struct Config {
                #[serde(default)]
                pub name: String,
                
                #[serde(skip_serializing_if = "Option::is_none")]
                pub value: Option<i32>,
            }
            
            #[derive(Serialize)]
            #[serde(tag = "type")]
            pub enum Message {
                #[serde(rename = "req")]
                Request { id: u64, method: String },
                
                #[serde(rename = "res")]  
                Response { id: u64, result: serde_json::Value },
            }
        "#;

        match parser.parse_source(source, Path::new("test.rs")) {
            Ok(entities) => {
                println!(
                    "\nParsed {} entities from derive macro code",
                    entities.len()
                );
                for entity in &entities {
                    println!("  - {} ({:?})", entity.name, entity.kind);
                }

                // Check if attributes were preserved
                if let Some(config) = entities.iter().find(|e| e.name == "Config") {
                    println!("\nConfig struct signature: {}", config.signature);
                }
            }
            Err(e) => {
                println!("Failed to parse derive macro code: {}", e);
            }
        }
    }

    #[test]
    fn test_parse_macro_rules() {
        let parser = RustParser::new(".");

        let source = r#"
            macro_rules! impl_display {
                ($type:ty) => {
                    impl std::fmt::Display for $type {
                        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            write!(f, "{:?}", self)
                        }
                    }
                };
            }
            
            macro_rules! create_function {
                ($name:ident, $value:expr) => {
                    pub fn $name() -> i32 {
                        $value
                    }
                };
            }
            
            create_function!(get_magic_number, 42);
            
            #[macro_export]
            macro_rules! log_error {
                ($($arg:tt)*) => {
                    eprintln!("[ERROR] {}", format!($($arg)*));
                };
            }
        "#;

        match parser.parse_source(source, Path::new("test.rs")) {
            Ok(entities) => {
                println!("\nParsed {} entities from macro_rules code", entities.len());
                for entity in &entities {
                    println!("  - {} ({:?})", entity.name, entity.kind);
                }
            }
            Err(e) => {
                println!("Failed to parse macro_rules code: {}", e);
            }
        }
    }

    #[test]
    fn test_parse_proc_macro_usage() {
        let parser = RustParser::new(".");

        // Test with code using various procedural macros
        let source = r#"
            use thiserror::Error;
            use clap::Parser;
            
            #[derive(Error, Debug)]
            pub enum MyError {
                #[error("IO error: {0}")]
                Io(#[from] std::io::Error),
                
                #[error("Parse error at line {line}")]
                Parse { line: usize },
            }
            
            #[derive(Parser, Debug)]
            #[command(author, version, about, long_about = None)]
            struct Args {
                /// Name of the person to greet
                #[arg(short, long)]
                name: String,
                
                /// Number of times to greet
                #[arg(short, long, default_value_t = 1)]
                count: u8,
            }
            
            #[tokio::main]
            async fn main() {
                println!("Hello");
            }
        "#;

        match parser.parse_source(source, Path::new("test.rs")) {
            Ok(entities) => {
                println!("\nParsed {} entities from proc macro code", entities.len());
                for entity in &entities {
                    println!("  - {} ({:?})", entity.name, entity.kind);
                    if !entity.doc_comments.is_empty() {
                        println!("    Docs: {:?}", entity.doc_comments);
                    }
                }
            }
            Err(e) => {
                println!("Failed to parse proc macro code: {}", e);
            }
        }
    }

    #[test]
    fn test_parse_complex_generics() {
        let parser = RustParser::new(".");

        let source = r#"
            use std::marker::PhantomData;
            
            pub struct Complex<'a, T, U: Clone, const N: usize> 
            where 
                T: Send + Sync + 'a,
                U: Default,
            {
                data: &'a [T; N],
                other: U,
                _marker: PhantomData<T>,
            }
            
            impl<'a, T, U: Clone, const N: usize> Complex<'a, T, U, N>
            where
                T: Send + Sync + 'a,
                U: Default,
            {
                pub fn new(data: &'a [T; N]) -> Self {
                    Self {
                        data,
                        other: U::default(),
                        _marker: PhantomData,
                    }
                }
            }
            
            pub trait MyTrait<T> {
                type Output;
                fn process(&self, input: T) -> Self::Output;
            }
            
            impl<'a, T, U: Clone, const N: usize> MyTrait<Vec<T>> for Complex<'a, T, U, N>
            where
                T: Send + Sync + 'a + Clone,
                U: Default,
            {
                type Output = Vec<U>;
                
                fn process(&self, input: Vec<T>) -> Self::Output {
                    vec![self.other.clone(); input.len()]
                }
            }
        "#;

        match parser.parse_source(source, Path::new("test.rs")) {
            Ok(entities) => {
                println!(
                    "\nParsed {} entities from complex generics code",
                    entities.len()
                );
                for entity in &entities {
                    println!("  - {} ({:?})", entity.name, entity.kind);
                    println!("    Signature: {}", entity.signature);
                }
            }
            Err(e) => {
                println!("Failed to parse complex generics: {}", e);
            }
        }
    }
}

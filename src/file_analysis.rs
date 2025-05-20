use anyhow::{Context, Result};
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

// Token estimation constants
pub const TOKENS_PER_BYTE: f64 = 0.3;
pub const MAX_TOKENS: usize = 100000;
pub const CLAUDE_TOKEN_LIMIT: usize = 20000;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub binary: bool,
    pub tokens: usize,
    pub size: u64,
    pub ext: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct DirInfo {
    pub files: Vec<FileInfo>,
    pub tokens: usize,
    pub subdirs: Vec<PathBuf>,
    pub total_files: usize,
    pub binary_files: usize,
}

pub type DirectoryMap = HashMap<PathBuf, DirInfo>;

/// Check if file is binary using the 'file' command
pub fn is_binary(filepath: &Path) -> Result<bool> {
    let output = Command::new("file")
        .args(["--mime", "--brief", filepath.to_str().unwrap_or("")])
        .output()
        .context("Failed to execute 'file' command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains("charset=binary") || stdout.contains("application/octet-stream"))
}

/// Recursively analyze directory structure
pub fn analyze_directory(directory: &Path, exclude_patterns: &[String]) -> Result<DirectoryMap> {
    let mut result = DirectoryMap::new();

    for entry in WalkDir::new(directory)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            !exclude_patterns
                .iter()
                .any(|p| e.path().to_string_lossy().contains(p))
        })
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                error!("Error accessing entry: {}", e);
                continue;
            }
        };

        let path = entry.path();

        if entry.file_type().is_dir() {
            let dir_path = path.to_path_buf();
            result.entry(dir_path.clone()).or_default();

            // Get all immediate subdirectories and add them to subdirs
            if let Ok(entries) = fs::read_dir(path) {
                for subdir_entry in entries.filter_map(Result::ok) {
                    let subdir_path = subdir_entry.path();
                    if subdir_path.is_dir()
                        && !exclude_patterns
                            .iter()
                            .any(|p| subdir_path.to_string_lossy().contains(p))
                    {
                        if let Some(dir_info) = result.get_mut(&dir_path) {
                            dir_info.subdirs.push(subdir_path);
                        }
                    }
                }
            }
        } else if entry.file_type().is_file() {
            // Process file
            let filepath = path.to_path_buf();
            let parent_dir = match path.parent() {
                Some(parent) => parent.to_path_buf(),
                None => {
                    error!("Failed to get parent dir for: {}", path.display());
                    continue;
                }
            };

            match process_file_info(&filepath) {
                Ok(file_info) => {
                    let dir_info = result.entry(parent_dir).or_default();

                    dir_info.files.push(file_info.clone());
                    dir_info.total_files += 1;

                    if file_info.binary {
                        dir_info.binary_files += 1;
                    } else {
                        dir_info.tokens += file_info.tokens;
                    }
                }
                Err(e) => {
                    error!("Error processing file {}: {}", filepath.display(), e);
                }
            }
        }
    }

    Ok(result)
}

fn process_file_info(filepath: &Path) -> Result<FileInfo> {
    let binary = is_binary(filepath)?;
    let metadata = fs::metadata(filepath).context("Failed to get file metadata")?;
    let size = metadata.len();
    let tokens = if binary {
        0
    } else {
        (size as f64 * TOKENS_PER_BYTE).ceil() as usize
    };

    let ext = filepath
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    Ok(FileInfo {
        path: filepath.to_path_buf(),
        binary,
        tokens,
        size,
        ext: format!(".{}", ext),
    })
}

/// Display directory information
pub fn show_dir_info(dir_path: &Path, info: &DirInfo) {
    println!("\n{}", "=".repeat(60));
    println!("DIR: {}", dir_path.display());
    println!("{}", "=".repeat(60));
    println!(
        "Files: {} ({} text)",
        info.total_files,
        info.total_files - info.binary_files
    );
    println!("Tokens: ~{}", info.tokens);
    println!("Subdirs: {}", info.subdirs.len());

    // Count file extensions
    let mut exts: HashMap<String, usize> = HashMap::new();
    for f in &info.files {
        let entry = exts.entry(f.ext.clone()).or_insert(0);
        *entry += 1;
    }

    if !exts.is_empty() {
        println!("\nExtensions:");
        let mut ext_counts: Vec<(String, usize)> = exts.into_iter().collect();
        ext_counts.sort_by(|a, b| b.1.cmp(&a.1));

        for (ext, count) in ext_counts.iter().take(5) {
            println!("  {}: {}", ext, count);
        }
    }
}

use anyhow::{Context, Result};
use log::info;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Summary information for a file
pub struct FileSummaryInfo {
    /// Path of the summarized file 
    pub path: PathBuf,
    /// Content hash of the file
    pub content_hash: String,
    /// Generated summary
    pub summary: String,
}

/// Result of processing a node in the filesystem
pub struct NodeProcessingResult {
    /// Total token count
    pub total_tokens: usize,
    /// Set of processed paths
    pub processed: HashSet<PathBuf>,
    /// Set of included file paths
    pub included_files: HashSet<PathBuf>,
    /// Context files used for the results
    pub context_files: Vec<ContextFile>,
    /// Newly generated file summaries
    pub file_summaries: Vec<FileSummaryInfo>,
}

use crate::context_files::{ContextFile, append_to_file, get_or_rotate_file};
use crate::file_analysis::{CLAUDE_TOKEN_LIMIT, DirectoryMap, is_binary};
use crate::summary_cache::{SummaryCache, hash_content};

// Common code file extensions
pub const CODE_EXTENSIONS: [&str; 9] = [
    ".py", ".rs", ".js", ".ts", ".c", ".cpp", ".go", ".java", ".rb",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Read,
    Exclude,
    Enter,
    Summarize,
    Stats,
}

impl Action {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Action::Read),
            "exclude" => Some(Action::Exclude),
            "enter" => Some(Action::Enter),
            "summarize" => Some(Action::Summarize),
            "stats" => Some(Action::Stats),
            _ => None,
        }
    }
}

use std::str::FromStr;

impl FromStr for Action {
    type Err = &'static str;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s).ok_or("Invalid action")
    }
}

/// Process directory contents based on action type
pub fn process_directory_content(
    dir_path: &Path,
    dir_info: &DirectoryMap,
    context_file: &mut ContextFile,
    action: &Action,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
) -> Result<Vec<ContextFile>> {
    // Check if adding directory header would exceed token limit
    let header_tokens = 200; // Rough estimate

    let mut current_file = context_file.clone();
    let mut used_files = vec![current_file.clone()];

    if current_file.current_tokens + header_tokens > CLAUDE_TOKEN_LIMIT {
        current_file = get_or_rotate_file(&current_file, total_files, base_dir, output_dir)?;
        used_files.push(current_file.clone());
    }

    let rel_path = dir_path
        .strip_prefix(std::env::current_dir()?)
        .unwrap_or(dir_path);
    let mut content = format!("\n\n## DIRECTORY: {}\n", rel_path.display());

    if action == &Action::Stats {
        if let Some(info) = dir_info.get(dir_path) {
            content.push_str(&format!(
                "Files: {} ({} text)\n",
                info.total_files,
                info.total_files - info.binary_files
            ));
            content.push_str(&format!("Tokens: ~{}\n", info.tokens));

            // Add extension stats
            let mut exts: HashMap<String, usize> = HashMap::new();
            for file in &info.files {
                let entry = exts.entry(file.ext.clone()).or_insert(0);
                *entry += 1;
            }

            if !exts.is_empty() {
                content.push_str("\nExtensions:\n");
                let mut ext_counts: Vec<(String, usize)> = exts.into_iter().collect();
                ext_counts.sort_by(|a, b| b.1.cmp(&a.1));

                for (ext, count) in ext_counts.iter().take(5) {
                    content.push_str(&format!("  {}: {}\n", ext, count));
                }
            }
        }
    }

    append_to_file(&current_file.path, &content)?;
    current_file.current_tokens += header_tokens;

    // Update the original context file with the latest state
    *context_file = current_file;

    Ok(used_files)
}

/// Process a single file based on the action
#[allow(clippy::too_many_arguments)]
fn process_file(
    path: &Path,
    context_file: &mut ContextFile,
    action: &Action,
    total_tokens: usize,
    included_files: &HashSet<PathBuf>,
    processed: &HashSet<PathBuf>,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
    summary_cache: Option<&SummaryCache>,
) -> Result<NodeProcessingResult> {
    let mut total_tokens = total_tokens;
    let mut included_files = included_files.clone();
    let mut processed = processed.clone();
    let mut context_files = vec![context_file.clone()];

    let rel_path = path.strip_prefix(std::env::current_dir()?).unwrap_or(path);
    let metadata = fs::metadata(path).context("Failed to get file metadata")?;
    let size = metadata.len();
    let tokens = (size as f64 * crate::file_analysis::TOKENS_PER_BYTE).ceil() as usize;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match action {
        Action::Read => {
            // Check if adding this file would exceed token limit
            if context_file.current_tokens + tokens > CLAUDE_TOKEN_LIMIT {
                *context_file =
                    get_or_rotate_file(context_file, total_files, base_dir, output_dir)?;
                context_files.push(context_file.clone());
            }

            info!("Reading: {}", rel_path.display());

            let mut content = format!("\n\n===== FILE START: {} =====\n", rel_path.display());

            if CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str()) {
                content.push_str(&format!("```{}\n", ext));
            }

            match fs::read_to_string(path) {
                Ok(file_content) => {
                    content.push_str(&file_content);
                }
                Err(e) => {
                    content.push_str(&format!("Error reading file: {}\n", e));
                }
            }

            if CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str()) {
                content.push_str("\n```\n");
            }

            content.push_str(&format!("===== FILE END: {} =====\n", rel_path.display()));

            append_to_file(&context_file.path, &content)?;

            // Update tracking variables
            total_tokens += tokens;
            context_file.current_tokens += tokens;
            included_files.insert(path.to_path_buf());
            processed.insert(path.to_path_buf());
        }
        Action::Summarize => {
            // Check if adding summary would exceed token limit
            let summary_tokens = tokens / 4; // Rough estimate

            if context_file.current_tokens + summary_tokens > CLAUDE_TOKEN_LIMIT {
                *context_file =
                    get_or_rotate_file(context_file, total_files, base_dir, output_dir)?;
                context_files.push(context_file.clone());
            }

            info!("Summarizing: {}", rel_path.display());

            let content = format!("\n\n# Summary of {}\n", rel_path.display());
            append_to_file(&context_file.path, &content)?;

            // Get file content
            let file_content = match fs::read_to_string(path) {
                Ok(content) => content,
                Err(e) => {
                    let error_msg = format!("Error reading file: {}\n", e);
                    append_to_file(&context_file.path, &error_msg)?;
                    return Ok(NodeProcessingResult {
                        total_tokens,
                        processed,
                        included_files,
                        context_files,
                        file_summaries: Vec::new(),
                    });
                }
            };
            
            // Calculate content hash
            let content_hash = hash_content(&file_content);
            
            // Check if we have a cached summary
            let summary = if let Some(cache) = summary_cache {
                if let Some(cached_summary) = cache.get_summary(path, &content_hash) {
                    info!("Using cached summary for: {}", rel_path.display());
                    format!("{}\n(Cached summary)\n", cached_summary)
                } else {
                    // No cached summary, generate a new one
                    let mut temp_file = NamedTempFile::new()?;
                    writeln!(temp_file, "Summarize this file concisely:\n\n")?;
                    
                    if CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str()) {
                        writeln!(temp_file, "```{}\n{}\n```\n", ext, file_content)?;
                    } else {
                        write!(temp_file, "{}", file_content)?;
                    }
                    
                    temp_file.flush()?;
                    
                    // Run Claude if available (in real implementation)
                    // For now, use a placeholder
                    let new_summary = "Summary would be generated by claude if available.\n".to_string();
                    
                    // The summary will be stored in the cache later
                    
                    new_summary
                }
            } else {
                // No cache available, generate a summary without caching
                let mut temp_file = NamedTempFile::new()?;
                writeln!(temp_file, "Summarize this file concisely:\n\n")?;
                
                if CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str()) {
                    writeln!(temp_file, "```{}\n{}\n```\n", ext, file_content)?;
                } else {
                    write!(temp_file, "{}", file_content)?;
                }
                
                temp_file.flush()?;
                
                "Summary would be generated by claude if available.\n".to_string()
            };
            
            // Add the summary to the context file
            append_to_file(&context_file.path, &summary)?;

            // Update tracking variables
            total_tokens += summary_tokens;
            context_file.current_tokens += summary_tokens;
            included_files.insert(path.to_path_buf());
            processed.insert(path.to_path_buf());
        }
        Action::Stats => {
            // Check if adding stats would exceed token limit
            let stats_tokens = 100; // Rough estimate

            if context_file.current_tokens + stats_tokens > CLAUDE_TOKEN_LIMIT {
                *context_file =
                    get_or_rotate_file(context_file, total_files, base_dir, output_dir)?;
                context_files.push(context_file.clone());
            }

            info!("Stats for: {}", rel_path.display());

            let content = format!(
                "\n\n# File: {}\nSize: {} bytes\nEstimated tokens: {}\nExtension: .{}\n",
                rel_path.display(),
                size,
                tokens,
                ext
            );

            append_to_file(&context_file.path, &content)?;
            context_file.current_tokens += stats_tokens;
            processed.insert(path.to_path_buf());
        }
        _ => {}
    }

    // Create vector to store any new summaries created
    let mut file_summaries = Vec::new();
    
    // If we created a new summary, add it to the file_summaries
    if let Action::Summarize = action {
        if let Ok(file_content) = fs::read_to_string(path) {
            let content_hash = hash_content(&file_content);
            
            // Only add if it wasn't already in the cache
            if let Some(cache) = summary_cache {
                if cache.get_summary(path, &content_hash).is_none() {
                    // We generated a new summary that wasn't in the cache
                    file_summaries.push(FileSummaryInfo {
                        path: path.to_path_buf(),
                        content_hash,
                        summary: "Summary would be generated by claude if available.".to_string(),
                    });
                }
            } else {
                // No cache provided, always add the summary 
                file_summaries.push(FileSummaryInfo {
                    path: path.to_path_buf(),
                    content_hash,
                    summary: "Summary would be generated by claude if available.".to_string(),
                });
            }
        }
    }
    
    Ok(NodeProcessingResult {
        total_tokens,
        processed,
        included_files,
        context_files,
        file_summaries,
    })
}

/// Process a node (file or directory) based on the chosen action
#[allow(clippy::too_many_arguments)]
pub fn process_node(
    path: &Path,
    dir_info: &DirectoryMap,
    context_file: &mut ContextFile,
    _max_tokens: usize,
    total_tokens: usize,
    included_files: &HashSet<PathBuf>,
    processed: &HashSet<PathBuf>,
    action: Action,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
    summary_cache: Option<&SummaryCache>,
) -> Result<NodeProcessingResult> {
    let mut total_tokens = total_tokens;
    let mut processed = processed.clone();
    let mut included_files = included_files.clone();
    let mut all_context_files = vec![context_file.clone()];
    let mut file_summaries = Vec::new();

    if path.is_file() {
        // Skip if already processed
        if included_files.contains(path) {
            return Ok(NodeProcessingResult {
                total_tokens,
                processed,
                included_files,
                context_files: all_context_files,
                file_summaries: Vec::new(),
            });
        }

        // Skip binary files
        if is_binary(path)? {
            processed.insert(path.to_path_buf());
            return Ok(NodeProcessingResult {
                total_tokens,
                processed,
                included_files,
                context_files: all_context_files,
                file_summaries: Vec::new(),
            });
        }

        // Process file based on action
        match action {
            Action::Read | Action::Summarize | Action::Stats => {
                let result = process_file(
                    path,
                    context_file,
                    &action,
                    total_tokens,
                    &included_files,
                    &processed,
                    total_files,
                    base_dir,
                    output_dir,
                    summary_cache,
                )?;
                total_tokens = result.total_tokens;
                included_files = result.included_files;
                processed = result.processed;
                all_context_files.extend(result.context_files.into_iter().skip(1)); // Skip first as it's already in the list
                file_summaries.extend(result.file_summaries);
            }
            Action::Exclude => {
                info!(
                    "Excluding: {}",
                    path.strip_prefix(std::env::current_dir()?)
                        .unwrap_or(path)
                        .display()
                );
                processed.insert(path.to_path_buf());
            }
            _ => {}
        }
    } else {
        // It's a directory
        if processed.contains(path) {
            return Ok(NodeProcessingResult {
                total_tokens,
                processed,
                included_files,
                context_files: all_context_files,
                file_summaries: Vec::new(),
            });
        }

        if !dir_info.contains_key(path) {
            processed.insert(path.to_path_buf());
            return Ok(NodeProcessingResult {
                total_tokens,
                processed,
                included_files,
                context_files: all_context_files,
                file_summaries: Vec::new(),
            });
        }

        // Process directory based on action
        match action {
            Action::Read | Action::Summarize | Action::Stats => {
                info!("Processing directory: {}", path.display());

                // Add directory header
                let used_files = process_directory_content(
                    path,
                    dir_info,
                    context_file,
                    &action,
                    total_files,
                    base_dir,
                    output_dir,
                )?;

                all_context_files.extend(used_files.into_iter().skip(1)); // Skip first as it's already in the list

                // Process all files in the directory
                if let Some(info) = dir_info.get(path) {
                    for file in &info.files {
                        if !file.binary && !processed.contains(&file.path) {
                            let result = process_node(
                                &file.path,
                                dir_info,
                                context_file,
                                _max_tokens,
                                total_tokens,
                                &included_files,
                                &processed,
                                action.clone(),
                                total_files,
                                base_dir,
                                output_dir,
                                summary_cache,
                            )?;
                            total_tokens = result.total_tokens;
                            processed = result.processed;
                            included_files = result.included_files;
                            all_context_files.extend(result.context_files.into_iter().skip(1)); // Skip first as it's already in the list
                            file_summaries.extend(result.file_summaries);
                        }
                    }

                    // Process all subdirectories
                    for subdir in &info.subdirs {
                        if !processed.contains(subdir) {
                            let result = process_node(
                                subdir,
                                dir_info,
                                context_file,
                                _max_tokens,
                                total_tokens,
                                &included_files,
                                &processed,
                                action.clone(),
                                total_files,
                                base_dir,
                                output_dir,
                                summary_cache,
                            )?;
                            total_tokens = result.total_tokens;
                            processed = result.processed;
                            included_files = result.included_files;
                            all_context_files.extend(result.context_files.into_iter().skip(1)); // Skip first as it's already in the list
                            file_summaries.extend(result.file_summaries);
                        }
                    }
                }

                processed.insert(path.to_path_buf());
            }
            Action::Exclude => {
                info!("Excluding directory: {}", path.display());
                // Mark all subdirectories and files as processed
                let mut to_exclude = vec![path.to_path_buf()];
                while let Some(exclude_path) = to_exclude.pop() {
                    processed.insert(exclude_path.clone());

                    if let Some(info) = dir_info.get(&exclude_path) {
                        // Add all files to processed
                        for file in &info.files {
                            processed.insert(file.path.clone());
                        }

                        // Add all subdirectories to exclude queue
                        to_exclude.extend(info.subdirs.clone());
                    }
                }
            }
            _ => {}
        }
    }

    // Deduplicate context files
    let mut unique_context_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for file in all_context_files {
        if seen_paths.insert(file.path.clone()) {
            unique_context_files.push(file);
        }
    }
    
    // Collect all file summaries from child nodes
    let file_summaries = Vec::new();
    
    if path.is_file() && matches!(action, Action::Summarize) {
        // This has already been handled in the file processing
        // The file_summaries will be populated by the file processing code
    }

    Ok(NodeProcessingResult {
        total_tokens,
        processed,
        included_files,
        context_files: unique_context_files,
        file_summaries,
    })
}

/// Apply actions from the cache to matching files
#[allow(clippy::too_many_arguments)]
pub fn apply_cached_actions(
    dir_info: &DirectoryMap,
    context_file: &mut ContextFile,
    _max_tokens: usize,
    cache: &HashMap<PathBuf, String>,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
    summary_cache: Option<&SummaryCache>,
) -> Result<NodeProcessingResult> {
    let mut processed = HashSet::new();
    let mut included_files = HashSet::new();
    let mut total_tokens = 0;
    let mut all_context_files = vec![context_file.clone()];
    let mut file_summaries = Vec::new();

    // Sort cached paths by directories first (helps processing in hierarchical order)
    let mut paths: Vec<_> = cache.keys().collect();
    paths.sort_by(|a, b| {
        if a.is_file() && b.is_dir() {
            Ordering::Greater
        } else if a.is_dir() && b.is_file() {
            Ordering::Less
        } else {
            a.cmp(b)
        }
    });

    for path in paths {
        if !path.exists() {
            info!(
                "Skipping cached path that no longer exists: {}",
                path.display()
            );
            continue;
        }

        if let Some(action_str) = cache.get(path) {
            if let Some(action) = Action::parse_str(action_str) {
                info!(
                    "Applying cached action '{}' to {}",
                    action_str,
                    path.display()
                );
                let result = process_node(
                    path,
                    dir_info,
                    context_file,
                    _max_tokens,
                    total_tokens,
                    &included_files,
                    &processed,
                    action,
                    total_files,
                    base_dir,
                    output_dir,
                    summary_cache,
                )?;

                total_tokens = result.total_tokens;
                processed = result.processed;
                included_files = result.included_files;
                all_context_files.extend(result.context_files.into_iter().skip(1)); // Skip first as it's already in the list
                file_summaries.extend(result.file_summaries);
            }
        }
    }

    // Deduplicate context files
    let mut unique_context_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for file in all_context_files {
        if seen_paths.insert(file.path.clone()) {
            unique_context_files.push(file);
        }
    }
    
    // Collect file summaries from all child processes
    let file_summaries = Vec::new();

    Ok(NodeProcessingResult {
        total_tokens,
        processed, 
        included_files,
        context_files: unique_context_files,
        file_summaries,
    })
}

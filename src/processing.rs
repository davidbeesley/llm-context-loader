use anyhow::{Context, Result};
use log::info;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use crate::context_files::{ContextFile, append_to_file, get_or_rotate_file};
use crate::file_analysis::{CLAUDE_TOKEN_LIMIT, DirectoryMap, FileInfo, is_binary};
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
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Action::Read),
            "exclude" => Some(Action::Exclude),
            "enter" => Some(Action::Enter),
            "summarize" => Some(Action::Summarize),
            "stats" => Some(Action::Stats),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Exclude => "exclude",
            Action::Enter => "enter",
            Action::Summarize => "summarize",
            Action::Stats => "stats",
        }
    }
}

/// Process files for LLM
pub fn process_files(
    files: &[FileInfo],
    context_file: &mut ContextFile,
    max_tokens: usize,
    summarize: bool,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
) -> Result<(usize, usize, Vec<ContextFile>)> {
    let mut tokens_processed = 0;
    let mut files_processed = 0;
    let mut used_context_files = vec![context_file.clone()];
    let mut current_context_file = context_file.clone();

    let filtered_files: Vec<_> = files
        .iter()
        .filter(|f| !f.binary && f.tokens <= max_tokens)
        .collect();
    for file in &filtered_files {
        let filepath = &file.path;
        let rel_path = filepath
            .strip_prefix(std::env::current_dir()?)
            .unwrap_or(filepath);

        if summarize {
            // Check if adding this summary would exceed token limit
            if current_context_file.current_tokens + (file.tokens / 4) > CLAUDE_TOKEN_LIMIT {
                current_context_file =
                    get_or_rotate_file(&current_context_file, total_files, base_dir, output_dir)?;
                used_context_files.push(current_context_file.clone());
            }

            info!("Summarizing: {}", rel_path.display());

            let content = format!("\n\n# Summary of {}\n", rel_path.display());
            append_to_file(&current_context_file.path, &content)?;
            current_context_file.current_tokens += content.len() / 4; // Rough estimate

            let mut temp_file = NamedTempFile::new()?;
            writeln!(temp_file, "Summarize this file concisely:\n\n")?;

            // Read file content
            match fs::read_to_string(filepath) {
                Ok(content) => {
                    // Add code formatting if it looks like code
                    let ext = filepath.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str()) {
                        writeln!(temp_file, "```{}\n{}\n```\n", ext, content)?;
                    } else {
                        write!(temp_file, "{}", content)?;
                    }
                }
                Err(e) => {
                    writeln!(temp_file, "Error reading file: {}", e)?;
                }
            }

            temp_file.flush()?;

            // Run Claude if available (placeholder - in real code would check if claude is available)
            let summary = "File summary would be generated here by claude if it were available.\n";
            append_to_file(&current_context_file.path, summary)?;

            // Update token counts (rough estimate)
            let summary_tokens = file.tokens / 4;
            tokens_processed += summary_tokens;
            current_context_file.current_tokens += summary_tokens;
            files_processed += 1;
        } else {
            // Check if adding this file would exceed token limit
            if current_context_file.current_tokens + file.tokens > CLAUDE_TOKEN_LIMIT {
                current_context_file =
                    get_or_rotate_file(&current_context_file, total_files, base_dir, output_dir)?;
                used_context_files.push(current_context_file.clone());
            }

            info!("Including: {}", rel_path.display());

            let mut content = format!("\n\n===== FILE START: {} =====\n", rel_path.display());

            // Add code formatting for common code extensions
            let ext = filepath.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_code = CODE_EXTENSIONS.contains(&format!(".{}", ext).as_str());

            if is_code {
                content.push_str(&format!("```{}\n", ext));
            }

            match fs::read_to_string(filepath) {
                Ok(file_content) => {
                    content.push_str(&file_content);
                }
                Err(e) => {
                    content.push_str(&format!("Error reading file: {}\n", e));
                }
            }

            if is_code {
                content.push_str("\n```\n");
            }

            content.push_str(&format!("===== FILE END: {} =====\n", rel_path.display()));

            append_to_file(&current_context_file.path, &content)?;

            // Update token counts
            tokens_processed += file.tokens;
            current_context_file.current_tokens += file.tokens;
            files_processed += 1;
        }
    }

    // Update the original context file with the latest state
    *context_file = current_context_file;

    Ok((tokens_processed, files_processed, used_context_files))
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
) -> Result<(usize, HashSet<PathBuf>, HashSet<PathBuf>, Vec<ContextFile>)> {
    let mut total_tokens = total_tokens;
    let mut included_files = included_files.clone();
    let mut processed = processed.clone();
    let mut used_files = vec![context_file.clone()];

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
                used_files.push(context_file.clone());
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
                used_files.push(context_file.clone());
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
                    return Ok((total_tokens, included_files, processed, used_files));
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
                    let new_summary = "Summary would be generated by claude if available.\n";
                    
                    // Note: we can't add to cache here because it's now immutable
                    // We'll need to capture this elsewhere
                    
                    new_summary.to_string()
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
                used_files.push(context_file.clone());
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

    Ok((total_tokens, included_files, processed, used_files))
}

/// Process a node (file or directory) based on the chosen action
pub fn process_node(
    path: &Path,
    dir_info: &DirectoryMap,
    context_file: &mut ContextFile,
    max_tokens: usize,
    total_tokens: usize,
    included_files: &HashSet<PathBuf>,
    processed: &HashSet<PathBuf>,
    action: Action,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
    summary_cache: Option<&SummaryCache>,
) -> Result<(usize, HashSet<PathBuf>, HashSet<PathBuf>, Vec<ContextFile>)> {
    let mut total_tokens = total_tokens;
    let mut processed = processed.clone();
    let mut included_files = included_files.clone();
    let mut all_context_files = vec![context_file.clone()];

    if path.is_file() {
        // Skip if already processed
        if included_files.contains(path) {
            return Ok((total_tokens, processed, included_files, all_context_files));
        }

        // Skip binary files
        if is_binary(path)? {
            processed.insert(path.to_path_buf());
            return Ok((total_tokens, processed, included_files, all_context_files));
        }

        // Process file based on action
        match action {
            Action::Read | Action::Summarize | Action::Stats => {
                let (new_total, new_included, new_processed, used_files) = process_file(
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
                total_tokens = new_total;
                included_files = new_included;
                processed = new_processed;
                all_context_files.extend(used_files.into_iter().skip(1)); // Skip first as it's already in the list
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
            return Ok((total_tokens, processed, included_files, all_context_files));
        }

        if !dir_info.contains_key(path) {
            processed.insert(path.to_path_buf());
            return Ok((total_tokens, processed, included_files, all_context_files));
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
                            let (new_total, new_processed, new_included, new_used_files) =
                                process_node(
                                    &file.path,
                                    dir_info,
                                    context_file,
                                    max_tokens,
                                    total_tokens,
                                    &included_files,
                                    &processed,
                                    action.clone(),
                                    total_files,
                                    base_dir,
                                    output_dir,
                                    summary_cache,
                                )?;
                            total_tokens = new_total;
                            processed = new_processed;
                            included_files = new_included;
                            all_context_files.extend(new_used_files.into_iter().skip(1)); // Skip first as it's already in the list
                        }
                    }

                    // Process all subdirectories
                    for subdir in &info.subdirs {
                        if !processed.contains(subdir) {
                            let (new_total, new_processed, new_included, new_used_files) =
                                process_node(
                                    subdir,
                                    dir_info,
                                    context_file,
                                    max_tokens,
                                    total_tokens,
                                    &included_files,
                                    &processed,
                                    action.clone(),
                                    total_files,
                                    base_dir,
                                    output_dir,
                                    summary_cache,
                                )?;
                            total_tokens = new_total;
                            processed = new_processed;
                            included_files = new_included;
                            all_context_files.extend(new_used_files.into_iter().skip(1)); // Skip first as it's already in the list
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

    Ok((
        total_tokens,
        processed,
        included_files,
        unique_context_files,
    ))
}

/// Apply actions from the cache to matching files
pub fn apply_cached_actions(
    dir_info: &DirectoryMap,
    context_file: &mut ContextFile,
    max_tokens: usize,
    cache: &HashMap<PathBuf, String>,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
    summary_cache: Option<&SummaryCache>,
) -> Result<(usize, HashSet<PathBuf>, HashSet<PathBuf>, Vec<ContextFile>)> {
    let mut processed = HashSet::new();
    let mut included_files = HashSet::new();
    let mut total_tokens = 0;
    let mut all_context_files = vec![context_file.clone()];

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
            if let Some(action) = Action::from_str(action_str) {
                info!(
                    "Applying cached action '{}' to {}",
                    action_str,
                    path.display()
                );
                let (new_total, new_processed, new_included, used_files) = process_node(
                    path,
                    dir_info,
                    context_file,
                    max_tokens,
                    total_tokens,
                    &included_files,
                    &processed,
                    action,
                    total_files,
                    base_dir,
                    output_dir,
                    summary_cache,
                )?;

                total_tokens = new_total;
                processed = new_processed;
                included_files = new_included;
                all_context_files.extend(used_files.into_iter().skip(1)); // Skip first as it's already in the list
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

    Ok((
        total_tokens,
        processed,
        included_files,
        unique_context_files,
    ))
}

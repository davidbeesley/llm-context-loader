mod cache;
mod context_files;
mod file_analysis;
mod processing;
mod summary_cache;

use anyhow::{Context, Result};
use clap::Parser;
use llm_context_loader::logging;
use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cache::{get_action_for_path, load_cache, save_cache, should_prompt_for_directory};
use crate::context_files::{
    ContextFile, append_to_file, create_context_file, finalize_context_files,
    get_default_context_dir, get_or_rotate_file,
};
use crate::file_analysis::{CLAUDE_TOKEN_LIMIT, analyze_directory, is_binary, show_dir_info};
use crate::processing::{Action, apply_cached_actions, process_node};
use crate::summary_cache::{SummaryCache, load_summary_cache, save_summary_cache};

#[derive(Parser)]
#[command(
    name = "llm-context-loader",
    about = "Process directory structure for LLM loading",
    version
)]
struct Cli {
    /// Starting directory (default: current directory)
    #[arg(default_value_t = String::from("."))]
    start_dir: String,

    /// Patterns to exclude
    #[arg(short, long, action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Maximum tokens to process
    #[arg(short, long, default_value_t = 100000)]
    max_tokens: usize,

    /// Ignore existing cache file
    #[arg(long)]
    no_cache: bool,

    /// Directory to store output files (default: .claude-context in current directory)
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    /// Create context files in a subdirectory of current working directory
    #[arg(long, default_value_t = true)]
    local_context: bool,
}

fn main() -> Result<()> {
    // Initialize logger
    logging::debug();

    // Parse command line arguments
    let args = Cli::parse();

    let start_dir = PathBuf::from(&args.start_dir)
        .canonicalize()
        .context("Failed to resolve start directory")?;

    let mut excludes = vec![
        ".git".to_string(),
        "node_modules".to_string(),
        "__pycache__".to_string(),
        ".env".to_string(),
        "venv".to_string(),
        "target".to_string(),
        ".claude-context".to_string(),
        ".claude-summaries".to_string(),
        ".claude_include".to_string(),
        ".claude".to_string(),
    ];
    excludes.extend(args.exclude);

    info!("Analyzing directory: {}", start_dir.display());
    info!("Excluding: {}", excludes.join(", "));

    // Analyze directory structure
    let dir_info = analyze_directory(&start_dir, &excludes)?;

    // Estimate total tokens and files needed
    let total_tokens: usize = dir_info.values().map(|dir| dir.tokens).sum();

    let estimated_files = std::cmp::max(
        1,
        (total_tokens as f64 / CLAUDE_TOKEN_LIMIT as f64).ceil() as usize,
    );

    info!("Estimated total tokens: {}", total_tokens);
    info!("Estimated context files needed: {}", estimated_files);

    // Determine the output directory
    let output_dir = if args.output_dir.is_some() {
        args.output_dir.clone()
    } else if args.local_context {
        Some(get_default_context_dir()?)
    } else {
        None
    };

    info!(
        "Context files will be stored in: {}",
        output_dir.as_ref().map_or_else(
            || "temporary directory".to_string(),
            |p| p.display().to_string()
        )
    );

    // Create the first output file
    let mut context_file =
        create_context_file(1, estimated_files, &start_dir, output_dir.as_deref())?;

    // Keep track of all used context files
    let mut all_context_files = vec![context_file.clone()];

    // Initialize file action cache
    let mut cache = if args.no_cache {
        HashMap::new()
    } else {
        load_cache(&start_dir)?
    };
    let use_cache = !cache.is_empty() && !args.no_cache;

    // Initialize summary cache
    let mut summary_cache = if args.no_cache {
        SummaryCache::new()
    } else {
        load_summary_cache(&start_dir)?
    };

    // Clean up orphaned summaries (files that no longer exist)
    if let Err(e) = summary_cache.cleanup(&start_dir) {
        warn!("Failed to clean up summary cache: {}", e);
    }

    if use_cache {
        info!("Found existing cache with {} entries", cache.len());

        // Display cache summary
        let mut action_paths: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for (path, action) in &cache {
            action_paths
                .entry(action.clone())
                .or_default()
                .push(path.clone());
        }

        println!("\nCache summary:");
        for (action, paths) in action_paths.iter() {
            println!("\n  {}: {} items", action, paths.len());
            for path in paths.iter().take(5) {
                // Only show first 5 for brevity
                println!(
                    "    - {}",
                    path.strip_prefix(&start_dir).unwrap_or(path).display()
                );
            }
            if paths.len() > 5 {
                println!("    - ... and {} more", paths.len() - 5);
            }
        }

        // Ask if user wants to use the cache
        print!("\nUse existing cache file? [Y/n]: ");
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        let use_cache = response.trim().to_lowercase() != "n";

        if use_cache {
            // Apply cached actions first if needed
            info!("Applying actions from cache...");
            let result = apply_cached_actions(
                &dir_info,
                &mut context_file,
                args.max_tokens,
                &cache,
                estimated_files,
                &start_dir,
                output_dir.as_deref(),
                Some(&summary_cache),
            )?;

            let total_tokens = result.total_tokens;
            let processed = result.processed;
            let included_files = result.included_files;
            all_context_files.extend(result.context_files.into_iter().skip(1)); // Skip first since it's already in the list

            // Add any new summaries to the cache
            for summary_info in result.file_summaries {
                summary_cache.insert_summary(
                    &summary_info.path,
                    &summary_info.content_hash,
                    summary_info.summary,
                );
            }

            process_interactive_loop(
                start_dir,
                dir_info,
                &mut context_file,
                args.max_tokens,
                &mut cache,
                &mut summary_cache,
                use_cache,
                total_tokens,
                processed,
                included_files,
                estimated_files,
                output_dir.as_deref(),
                &mut all_context_files,
            )?;
        } else {
            process_interactive_loop(
                start_dir,
                dir_info,
                &mut context_file,
                args.max_tokens,
                &mut cache,
                &mut summary_cache,
                false,
                0,
                HashSet::new(),
                HashSet::new(),
                estimated_files,
                output_dir.as_deref(),
                &mut all_context_files,
            )?;
        }
    } else {
        process_interactive_loop(
            start_dir,
            dir_info,
            &mut context_file,
            args.max_tokens,
            &mut cache,
            &mut summary_cache,
            false,
            0,
            HashSet::new(),
            HashSet::new(),
            estimated_files,
            output_dir.as_deref(),
            &mut all_context_files,
        )?;
    }

    // Finalize all context files
    // This is also done in the interactive loop to handle ctrl+c, but we do it again here to make sure

    // Check if claude command is available
    let claude_available = Command::new("which")
        .arg("claude")
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    // Ask to start Claude with instructions to read the context files (defaulting to Yes)
    if claude_available && !all_context_files.is_empty() {
        print!("\nStart Claude with context files? [Y/n]: ");
        io::stdout().flush()?;

        let mut response = String::new();
        io::stdin().read_line(&mut response)?;

        if response.trim().to_lowercase() != "n" {
            // Create a list of all context file paths
            let context_files_paths: Vec<_> = all_context_files
                .iter()
                .map(|f| f.path.display().to_string())
                .collect();

            info!("Starting Claude with all context files...");

            // Start Claude with instructions to read all context files
            let message = format!(
                "The context files are at {}. Read the first each file in its entirety, then say 'Ready'.",
                context_files_paths.join(", ")
            );

            match Command::new("claude")
                .args(["-d", "--verbose", &message])
                .status()
            {
                Ok(status) => {
                    if status.success() {
                        info!("Claude session completed");
                    } else {
                        error!("Claude exited with status: {}", status);
                    }
                }
                Err(e) => {
                    error!("Error starting Claude: {}", e);
                    println!("\nContext files are available at:");
                    for file in &all_context_files {
                        println!("  {}", file.path.display());
                    }
                    println!(
                        "\nYou can try starting manually with: claude -d --verbose \"The context files are at {}. Read the first file in its entirety, then say 'Ready'.\"",
                        context_files_paths.join(", ")
                    );
                }
            }
        } else {
            // Create a list of all context file paths
            let context_files_paths: Vec<_> = all_context_files
                .iter()
                .map(|f| f.path.display().to_string())
                .collect();

            println!("\nContext files are available at:");
            for file in &all_context_files {
                println!("  {}", file.path.display());
            }
            println!(
                "\nStart Claude manually with: claude -d --verbose \"The context files are at {}. Read the each file in its entirety, then say 'Ready'.\"",
                context_files_paths.join(", ")
            );
        }
    } else {
        // Create a list of all context file paths
        let context_files_paths: Vec<_> = all_context_files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect();

        println!("\nContext files are available at:");
        for file in &all_context_files {
            println!("  {}", file.path.display());
        }

        if claude_available {
            println!(
                "\nStart Claude manually with: claude -d --verbose \"The context files are at {}. Read the first file in its entirety, then say 'Ready'.\"",
                context_files_paths.join(", ")
            );
        } else {
            println!("\nClaude CLI not found. You can view the context files directly.");
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_interactive_loop(
    start_dir: PathBuf,
    dir_info: HashMap<PathBuf, file_analysis::DirInfo>,
    context_file: &mut ContextFile,
    max_tokens: usize,
    cache: &mut HashMap<PathBuf, String>,
    summary_cache: &mut SummaryCache,
    use_cache: bool,
    initial_tokens: usize,
    initial_processed: HashSet<PathBuf>,
    initial_included_files: HashSet<PathBuf>,
    total_files: usize,
    output_dir: Option<&Path>,
    all_context_files: &mut Vec<ContextFile>,
) -> Result<()> {
    // Interactive processing setup
    let mut to_process = vec![start_dir.clone()];
    let mut processed = initial_processed;
    let mut included_files = initial_included_files;
    let mut total_tokens = initial_tokens;

    // Interactive processing loop
    let result: Result<()> = (|| {
        while let Some(current) = to_process.pop() {
            // Skip if already processed
            if processed.contains(&current) {
                continue;
            }

            let is_file = current.is_file();

            // If it's a directory we need the dir_info
            if !is_file && !dir_info.contains_key(&current) {
                continue;
            }

            // Check if we should use the cached action
            let cached_action = if use_cache {
                get_action_for_path(&current, cache)
            } else {
                None
            };

            // For directories with cached "enter" action, apply it automatically without prompting
            if !is_file && use_cache && cached_action.as_deref() == Some("enter") {
                println!(
                    "\nAutomatically entering directory (from cache): {}",
                    current.display()
                );

                // Add a header for the directory - check token limit first
                if context_file.current_tokens + 200 > CLAUDE_TOKEN_LIMIT {
                    // Rough estimate
                    *context_file =
                        get_or_rotate_file(context_file, total_files, &start_dir, output_dir)?;
                    all_context_files.push(context_file.clone());
                }

                let rel_path = current.strip_prefix(&start_dir).unwrap_or(&current);
                let content = format!("\n\n## DIRECTORY: {}\n", rel_path.display());
                append_to_file(&context_file.path, &content)?;
                context_file.current_tokens += 200; // Rough estimate

                // Mark directory as processed but add all its child nodes to the queue
                processed.insert(current.clone());

                // Add all files in this directory to the processing queue
                if let Some(dir_info) = dir_info.get(&current) {
                    for file in &dir_info.files {
                        if !processed.contains(&file.path) {
                            to_process.push(file.path.clone());
                        }
                    }

                    // Add all subdirectories to the processing queue
                    for subdir in &dir_info.subdirs {
                        if !processed.contains(subdir) {
                            to_process.push(subdir.clone());
                        }
                    }
                }

                continue;
            }

            // For other directories, check if we need to prompt or can use cached actions
            if !is_file && use_cache && !should_prompt_for_directory(&current, &dir_info, cache) {
                println!(
                    "\nUsing cached actions for directory: {}",
                    current.display()
                );
                processed.insert(current.clone()); // Mark as processed and skip prompting
                continue;
            }

            // Show information about the current node
            if is_file {
                if is_binary(&current)? {
                    println!("\n{}", "-".repeat(60));
                    println!("BINARY FILE: {}", current.display());
                    println!("{}", "-".repeat(60));
                    println!("Binary files are not processed.");
                    processed.insert(current.clone());
                    continue;
                }

                let metadata = std::fs::metadata(&current)?;
                let size = metadata.len();
                let tokens = (size as f64 * file_analysis::TOKENS_PER_BYTE).ceil() as usize;
                let ext = current
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                println!("\n{}", "-".repeat(60));
                println!("FILE: {}", current.display());
                println!("{}", "-".repeat(60));
                println!("Size: {} bytes", size);
                println!("Estimated tokens: {}", tokens);
                println!("Extension: .{}", ext);
            } else if let Some(info) = dir_info.get(&current) {
                show_dir_info(&current, info);
            }

            // Display appropriate options based on node type
            println!("\nOptions:");
            println!("  1. Read (include full content)");
            println!("  2. Exclude (skip this node)");
            if !is_file {
                println!("  3. Enter (process each child node separately)");
            }
            println!("  4. Summarize (create summary)");
            println!("  5. Stats only (just include statistics)");
            println!("  q. Quit");

            // Show cached action if it exists
            let choice = if let Some(cached_action) = cached_action {
                println!("\nCached action: {}", cached_action);
                print!("Use cached action '{}'? [Y/n]: ", cached_action);
                io::stdout().flush()?;

                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                let auto_apply = response.trim().to_lowercase() != "n";

                if auto_apply {
                    // For files, don't allow 'enter' choice
                    if is_file && cached_action == "enter" {
                        println!("Invalid cached action: Files don't have child nodes to enter.");
                        print!("\nEnter choice [1-5, q]: ");
                        io::stdout().flush()?;
                        let mut choice = String::new();
                        io::stdin().read_line(&mut choice)?;
                        choice.trim().to_string()
                    } else {
                        match cached_action.as_str() {
                            "read" => "1".to_string(),
                            "exclude" => "2".to_string(),
                            "enter" => "3".to_string(),
                            "summarize" => "4".to_string(),
                            "stats" => "5".to_string(),
                            _ => {
                                print!("\nEnter choice [1-5, q]: ");
                                io::stdout().flush()?;
                                let mut choice = String::new();
                                io::stdin().read_line(&mut choice)?;
                                choice.trim().to_string()
                            }
                        }
                    }
                } else {
                    print!("\nEnter choice [1-5, q]: ");
                    io::stdout().flush()?;
                    let mut choice = String::new();
                    io::stdin().read_line(&mut choice)?;
                    choice.trim().to_string()
                }
            } else {
                print!("\nEnter choice [1-5, q]: ");
                io::stdout().flush()?;
                let mut choice = String::new();
                io::stdin().read_line(&mut choice)?;
                choice.trim().to_string()
            };

            if choice == "q" {
                break;
            }

            match choice.as_str() {
                "1" => {
                    // Read
                    // Update cache
                    cache.insert(current.clone(), "read".to_string());
                    let result = process_node(
                        &current,
                        &dir_info,
                        context_file,
                        max_tokens,
                        total_tokens,
                        &included_files,
                        &processed,
                        Action::Read,
                        total_files,
                        &start_dir,
                        output_dir,
                        Some(summary_cache),
                    )?;
                    total_tokens = result.total_tokens;
                    processed = result.processed;
                    included_files = result.included_files;

                    // Add any new summaries to the cache
                    for summary_info in result.file_summaries {
                        summary_cache.insert_summary(
                            &summary_info.path,
                            &summary_info.content_hash,
                            summary_info.summary,
                        );
                    }

                    // Add any new context files to our tracking list
                    for file in result.context_files.into_iter().skip(1) {
                        // Skip first as it's the updated original
                        if !all_context_files.iter().any(|f| f.path == file.path) {
                            all_context_files.push(file);
                        }
                    }
                }
                "2" => {
                    // Exclude
                    // Update cache
                    cache.insert(current.clone(), "exclude".to_string());
                    let result = process_node(
                        &current,
                        &dir_info,
                        context_file,
                        max_tokens,
                        total_tokens,
                        &included_files,
                        &processed,
                        Action::Exclude,
                        total_files,
                        &start_dir,
                        output_dir,
                        Some(summary_cache),
                    )?;
                    total_tokens = result.total_tokens;
                    processed = result.processed;
                    included_files = result.included_files;

                    // Add any new summaries to the cache
                    for summary_info in result.file_summaries {
                        summary_cache.insert_summary(
                            &summary_info.path,
                            &summary_info.content_hash,
                            summary_info.summary,
                        );
                    }
                }
                "3" => {
                    // Enter
                    if is_file {
                        println!("Invalid option: Files don't have child nodes to enter.");
                        // Don't mark as processed so it will be prompted again
                        to_process.insert(0, current.clone());
                    } else {
                        // Check if adding directory header would exceed token limit
                        if context_file.current_tokens + 200 > CLAUDE_TOKEN_LIMIT {
                            // Rough estimate
                            *context_file = get_or_rotate_file(
                                context_file,
                                total_files,
                                &start_dir,
                                output_dir,
                            )?;
                            all_context_files.push(context_file.clone());
                        }

                        // Add a header for the directory
                        let rel_path = current.strip_prefix(&start_dir).unwrap_or(&current);
                        let content = format!("\n\n## DIRECTORY: {}\n", rel_path.display());
                        append_to_file(&context_file.path, &content)?;
                        context_file.current_tokens += 200; // Rough estimate

                        // Mark directory as processed but add all its child nodes to the queue
                        processed.insert(current.clone());
                        cache.insert(current.clone(), "enter".to_string()); // Record that we entered this directory

                        // Add all files in this directory to the processing queue
                        if let Some(dir_info) = dir_info.get(&current) {
                            for file in &dir_info.files {
                                if !processed.contains(&file.path) {
                                    to_process.push(file.path.clone());
                                }
                            }

                            // Add all subdirectories to the processing queue
                            for subdir in &dir_info.subdirs {
                                if !processed.contains(subdir) {
                                    to_process.push(subdir.clone());
                                }
                            }
                        }
                    }
                }
                "4" => {
                    // Summarize
                    // Update cache
                    cache.insert(current.clone(), "summarize".to_string());
                    let result = process_node(
                        &current,
                        &dir_info,
                        context_file,
                        max_tokens,
                        total_tokens,
                        &included_files,
                        &processed,
                        Action::Summarize,
                        total_files,
                        &start_dir,
                        output_dir,
                        Some(summary_cache),
                    )?;
                    total_tokens = result.total_tokens;
                    processed = result.processed;
                    included_files = result.included_files;

                    // Add any new summaries to the cache
                    for summary_info in result.file_summaries {
                        summary_cache.insert_summary(
                            &summary_info.path,
                            &summary_info.content_hash,
                            summary_info.summary,
                        );
                    }

                    // Add any new context files to our tracking list
                    for file in result.context_files.into_iter().skip(1) {
                        // Skip first as it's the updated original
                        if !all_context_files.iter().any(|f| f.path == file.path) {
                            all_context_files.push(file);
                        }
                    }
                }
                "5" => {
                    // Stats
                    // Update cache
                    cache.insert(current.clone(), "stats".to_string());
                    let result = process_node(
                        &current,
                        &dir_info,
                        context_file,
                        max_tokens,
                        total_tokens,
                        &included_files,
                        &processed,
                        Action::Stats,
                        total_files,
                        &start_dir,
                        output_dir,
                        Some(summary_cache),
                    )?;
                    total_tokens = result.total_tokens;
                    processed = result.processed;
                    included_files = result.included_files;

                    // Add any new summaries to the cache
                    for summary_info in result.file_summaries {
                        summary_cache.insert_summary(
                            &summary_info.path,
                            &summary_info.content_hash,
                            summary_info.summary,
                        );
                    }

                    // Add any new context files to our tracking list
                    for file in result.context_files.into_iter().skip(1) {
                        // Skip first as it's the updated original
                        if !all_context_files.iter().any(|f| f.path == file.path) {
                            all_context_files.push(file);
                        }
                    }
                }
                _ => {
                    println!("Invalid choice");
                    to_process.insert(0, current.clone());
                }
            }
        }

        Ok(())
    })();

    // Finalize all context files - we do this regardless of whether the loop completed normally or was interrupted
    finalize_context_files(all_context_files, included_files.len())?;

    // Save the cache files
    save_cache(&start_dir, cache)?;

    // We need to do this outside the closure to avoid ownership issues
    save_summary_cache(&start_dir, summary_cache)?;

    // Display information
    println!(
        "\nProcessed {} nodes, {} files included.",
        processed.len(),
        included_files.len()
    );
    println!("Estimated tokens: {}", total_tokens);
    println!(
        "Created {} context files (limited to ~{} tokens each):",
        all_context_files.len(),
        CLAUDE_TOKEN_LIMIT
    );

    for (i, file) in all_context_files.iter().enumerate() {
        println!("  {}. {}", i + 1, file.path.display());
    }

    // Display summary of cached actions
    let mut action_paths: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (path, action) in cache {
        action_paths
            .entry(action.clone())
            .or_default()
            .push(path.clone());
    }

    println!("\nCache summary:");
    for (action, paths) in action_paths.iter() {
        println!("\n  {}: {} items", action, paths.len());
        for path in paths.iter().take(5) {
            // Only show first 5 for brevity
            println!(
                "    - {}",
                path.strip_prefix(&start_dir).unwrap_or(path).display()
            );
        }
        if paths.len() > 5 {
            println!("    - ... and {} more", paths.len() - 5);
        }
    }

    // Handle the result from the processing loop
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            println!("\nInterrupted: {}", e);
            Ok(())
        }
    }
}

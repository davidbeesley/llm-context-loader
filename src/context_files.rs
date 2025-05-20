use anyhow::{Context, Result};
use log::info;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use crate::file_analysis::CLAUDE_TOKEN_LIMIT;

/// Information about the current context file
#[derive(Debug, Clone)]
pub struct ContextFile {
    pub path: PathBuf,
    pub file_num: usize,
    pub current_tokens: usize,
}

/// Creates a new context file with proper header
pub fn create_context_file(
    file_num: usize,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
) -> Result<ContextFile> {
    let file_path = if let Some(out_dir) = output_dir {
        fs::create_dir_all(out_dir).context("Failed to create output directory")?;
        out_dir.join(format!("context-{:03}.txt", file_num))
    } else {
        let temp_file = NamedTempFile::new().context("Failed to create temporary file")?;
        let path = temp_file.path().to_path_buf();
        // Keep the temp file in scope so it's not deleted
        std::mem::forget(temp_file);
        path
    };

    let mut file = File::create(&file_path).context("Failed to create context file")?;

    writeln!(
        file,
        "The following content is a collection of files and directories (Part {} of {}).",
        file_num, total_files
    )?;
    writeln!(
        file,
        "Each file is clearly marked with a START and END tag."
    )?;
    writeln!(
        file,
        "After reading these files, you will respond with 'Ready' and await further instructions."
    )?;
    writeln!(file, "===== DIRECTORY CONTENT=====")?;
    writeln!(file, "Source directory: {}", base_dir.display())?;

    info!("Created new context file at: {}", file_path.display());

    Ok(ContextFile {
        path: file_path,
        file_num,
        current_tokens: 0,
    })
}

/// Get current file or create a new one if token limit reached
pub fn get_or_rotate_file(
    current_file: &ContextFile,
    total_files: usize,
    base_dir: &Path,
    output_dir: Option<&Path>,
) -> Result<ContextFile> {
    // If we're under token limit, just return current file
    if current_file.current_tokens < CLAUDE_TOKEN_LIMIT {
        return Ok(current_file.clone());
    }

    // Create a new file because we've hit token limit
    let file_num = current_file.file_num + 1;
    info!(
        "\nCreating new context file {} (token limit of {} reached)",
        file_num, CLAUDE_TOKEN_LIMIT
    );

    let new_file = create_context_file(file_num, total_files, base_dir, output_dir)?;

    Ok(new_file)
}

/// Append content to a context file
pub fn append_to_file(path: &Path, content: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .context("Failed to open file for appending")?;

    write!(file, "{}", content).context("Failed to write to file")?;

    Ok(())
}

/// Finalize all context files
pub fn finalize_context_files(
    context_files: &[ContextFile],
    included_files_count: usize,
) -> Result<()> {
    for (idx, file) in context_files.iter().enumerate() {
        let mut output = OpenOptions::new()
            .append(true)
            .open(&file.path)
            .context("Failed to open context file for finalizing")?;

        writeln!(output, "\n\n===== END OF FILE COLLECTION =====\n")?;
        writeln!(output, "Total files included: {}", included_files_count)?;
        writeln!(output, "This is the complete source code for your review.")?;
        
        // Add pointer to the next file if this isn't the last file
        if idx < context_files.len() - 1 {
            writeln!(output, "\nIMPORTANT: continue reading the next context file at: {}", context_files[idx + 1].path.display())?;
        } else {
        writeln!(
            output,
            "You have now read all the files. Respond only with 'Ready' and await further instructions."
        )?;

        }

        
    }

    Ok(())
}

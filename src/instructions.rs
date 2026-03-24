/// Shared instruction file detection logic used by init and doctor.
///
/// Handles detecting which instruction file (AGENTS.md or CLAUDE.md) is canonical,
/// ensuring the <aiki> block is present, and managing symlinks between them.
use crate::commands::agents_template::{aiki_block_template, AIKI_BLOCK_VERSION};
use crate::error::{AikiError, Result};
use anyhow::Context;
use std::fs;
use std::path::Path;

pub const AGENTS_MD: &str = "AGENTS.md";
pub const CLAUDE_MD: &str = "CLAUDE.md";

/// Determine which instruction file is canonical based on priority logic:
///
/// 1. Both exist, one is symlink -> canonical = the real file (symlink target)
/// 2. Both exist, both real -> conflict error (unless `preferred` overrides)
/// 3. Only CLAUDE.md exists (real) -> canonical = CLAUDE.md
/// 4. Only AGENTS.md exists (real) -> canonical = AGENTS.md
/// 5. Neither exists -> use `preferred` if given, else AGENTS.md
pub fn detect_canonical(repo_root: &Path, preferred: Option<&str>) -> Result<&'static str> {
    let agents_path = repo_root.join(AGENTS_MD);
    let claude_path = repo_root.join(CLAUDE_MD);

    let agents_exists = agents_path.exists();
    let claude_exists = claude_path.exists();

    match (agents_exists, claude_exists) {
        (true, true) => {
            let agents_is_symlink = agents_path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            let claude_is_symlink = claude_path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            match (agents_is_symlink, claude_is_symlink) {
                // One is symlink -> canonical is the real file
                (true, false) => Ok(CLAUDE_MD),
                (false, true) => Ok(AGENTS_MD),
                // Both real files or both symlinks -> conflict
                _ => {
                    if let Some(pref) = preferred {
                        if pref == AGENTS_MD {
                            Ok(AGENTS_MD)
                        } else if pref == CLAUDE_MD {
                            Ok(CLAUDE_MD)
                        } else {
                            Err(AikiError::InvalidPreferred(pref.to_string()))
                        }
                    } else {
                        Err(AikiError::InstructionsConflict)
                    }
                }
            }
        }
        (true, false) => Ok(AGENTS_MD),
        (false, true) => Ok(CLAUDE_MD),
        (false, false) => {
            if let Some(pref) = preferred {
                if pref == AGENTS_MD {
                    Ok(AGENTS_MD)
                } else if pref == CLAUDE_MD {
                    Ok(CLAUDE_MD)
                } else {
                    Err(AikiError::InvalidPreferred(pref.to_string()))
                }
            } else {
                Ok(AGENTS_MD)
            }
        }
    }
}

/// Ensure the <aiki> block is present in the given instruction file.
///
/// - If file exists and has current block -> no-op, print checkmark
/// - If file exists with outdated block -> replace with current block
/// - If file exists without block -> prepend block
/// - If file doesn't exist -> create it with block
pub fn ensure_aiki_block(repo_root: &Path, filename: &str, quiet: bool) -> Result<()> {
    let file_path = repo_root.join(filename);

    // Remove dangling symlink so we can create a fresh file
    if !file_path.exists() {
        if let Ok(meta) = file_path.symlink_metadata() {
            if meta.file_type().is_symlink() {
                fs::remove_file(&file_path)
                    .with_context(|| format!("Failed to remove dangling symlink {}", filename))?;
            }
        }
    }

    if file_path.exists() {
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {}", filename))?;

        if !content.contains("<aiki version=") {
            // Prepend block
            let updated = format!("{}\n{}", aiki_block_template(), content);
            fs::write(&file_path, updated)
                .with_context(|| format!("Failed to update {}", filename))?;
            if !quiet {
                println!("✓ Added <aiki> block to {}", filename);
            }
        } else if !content.contains(&format!("<aiki version=\"{}\">", AIKI_BLOCK_VERSION)) {
            // Version is outdated — replace the old block
            let start = content
                .find("<aiki version=")
                .expect("already checked content contains <aiki version=");
            let end_tag = "</aiki>";
            let end = content[start..]
                .find(end_tag)
                .map(|pos| start + pos + end_tag.len())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Malformed <aiki> block in {}: missing </aiki> closing tag",
                        filename
                    )
                })?;
            // Skip a trailing newline if present
            let end = if content[end..].starts_with("\r\n") {
                end + 2
            } else if content[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            let updated = format!(
                "{}{}{}",
                &content[..start],
                aiki_block_template(),
                &content[end..]
            );
            fs::write(&file_path, updated)
                .with_context(|| format!("Failed to update {}", filename))?;
            if !quiet {
                println!("✓ Updated <aiki> block in {}", filename);
            }
        } else if !quiet {
            println!("✓ {} already has <aiki> block", filename);
        }
    } else {
        // Create new file with just the block
        fs::write(&file_path, aiki_block_template())
            .with_context(|| format!("Failed to create {}", filename))?;
        if !quiet {
            println!("✓ Created {} with task system instructions", filename);
        }
    }

    Ok(())
}

/// Create a symlink from `link_name` -> `target_name` in repo_root.
///
/// - If symlink already exists pointing to correct target -> no-op, print checkmark
/// - If symlink exists with wrong target -> remove and recreate
/// - If path exists as real file -> warn and skip
pub fn ensure_symlink(
    repo_root: &Path,
    target_name: &str,
    link_name: &str,
    quiet: bool,
) -> Result<()> {
    let link_path = repo_root.join(link_name);

    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        let metadata = link_path
            .symlink_metadata()
            .with_context(|| format!("Failed to read metadata for {}", link_name))?;

        if metadata.file_type().is_symlink() {
            // Check if it points to the correct target
            let current_target = fs::read_link(&link_path)
                .with_context(|| format!("Failed to read symlink {}", link_name))?;

            if current_target.to_string_lossy() == target_name {
                if !quiet {
                    println!("✓ {} already symlinked to {}", link_name, target_name);
                }
                return Ok(());
            }

            // Wrong target -> remove and recreate
            fs::remove_file(&link_path)
                .with_context(|| format!("Failed to remove old symlink {}", link_name))?;
        } else {
            // Real file exists — can't create symlink
            if !quiet {
                println!("⚠ {} exists as a regular file, skipping symlink", link_name);
            }
            return Ok(());
        }
    }

    // Create the symlink
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(target_name, &link_path).with_context(|| {
            format!("Failed to create symlink {} -> {}", link_name, target_name)
        })?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        symlink_file(target_name, &link_path).with_context(|| {
            format!("Failed to create symlink {} -> {}", link_name, target_name)
        })?;
    }

    if !quiet {
        println!("✓ Created symlink {} -> {}", link_name, target_name);
    }

    Ok(())
}

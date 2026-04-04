//! Resolve CLI text values that may come from inline text, a file path, or stdin.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::error::{AikiError, Result};
use crate::tasks::id::{is_valid_slug, looks_like_task_id, TaskRef};

/// A classified CLI input — either a task reference, plan file (.md), or text file (.txt).
#[derive(Debug, Clone, PartialEq)]
pub enum RefKind {
    Task(TaskRef),
    Plan(PathBuf),
    File(PathBuf),
}

/// Classify a single input string as a task reference or file path.
///
/// Resolution order:
/// 1. If it looks like a task ID, prefix, or slug ref → `Task(TaskRef)`
/// 2. If `cwd` is `Some` and input has a file extension:
///    a. Unsupported extension (not .md/.txt) → error (unsupported file extension)
///    b. Supported extension and file exists → `Plan` (.md) or `File` (.txt)
///    c. Supported extension but file doesn't exist → error (file not found)
/// 3. Otherwise → error (unrecognized input)
///
/// Does NOT resolve task refs against the graph — the caller does that.
pub fn resolve_ref(input: &str, cwd: Option<&Path>) -> Result<RefKind> {
    // 1. Task ID, prefix, or slug ref (parent_prefix:slug)
    if looks_like_task_id(input) || looks_like_slug_ref(input) {
        return Ok(RefKind::Task(TaskRef(input.to_string())));
    }

    // 2-3. File path detection (only when cwd is provided)
    if let Some(cwd) = cwd {
        if has_any_extension(input) {
            if !has_supported_extension(input) {
                let ext = Path::new(input)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown");
                return Err(AikiError::InvalidArgument(format!(
                    "Unsupported file extension '.{ext}': only .md and .txt files are accepted"
                )));
            }
            let path = if Path::new(input).is_absolute() {
                PathBuf::from(input)
            } else {
                cwd.join(input)
            };
            if path.exists() {
                if path.extension().is_some_and(|ext| ext == "md") {
                    return Ok(RefKind::Plan(path));
                }
                return Ok(RefKind::File(path));
            }
            return Err(AikiError::InvalidArgument(format!(
                "File not found: {}",
                input
            )));
        }
    }

    // 4. Unrecognized
    Err(AikiError::InvalidArgument(format!(
        "Target not found: {}",
        input
    )))
}

/// Check if input looks like a slug ref (`parent_prefix:slug`).
fn looks_like_slug_ref(input: &str) -> bool {
    if let Some((parent, slug)) = input.split_once(':') {
        looks_like_task_id(parent) && is_valid_slug(slug)
    } else {
        false
    }
}

/// Check if input has any file extension (contains a dot with non-empty suffix).
fn has_any_extension(val: &str) -> bool {
    Path::new(val).extension().is_some_and(|ext| !ext.is_empty())
}

/// Check if input has a supported text-file extension (`.md` or `.txt`).
fn has_supported_extension(val: &str) -> bool {
    Path::new(val)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "txt"))
}

/// Resolve a CLI text value that may come from inline text, a file path, or stdin.
///
/// - `None` → flag was omitted → `Ok(None)`
/// - `Some("")` → bare flag (clap `default_missing_value`) → read stdin
/// - `Some("-")` → explicit stdin sentinel → read stdin
/// - `Some(val)` where val looks like a readable text file (.md/.txt, or explicit path prefix) → file contents
/// - `Some(val)` otherwise → literal text
pub fn resolve_text(value: Option<&str>) -> Result<Option<String>> {
    let val = match value {
        None => return Ok(None),
        Some(v) => v,
    };

    // Empty string or "-" → read stdin
    if val.is_empty() || val == "-" {
        if std::io::stdin().is_terminal() {
            eprintln!("Reading from stdin… paste your text, then press Ctrl-D to finish.");
        }
        let content = std::io::read_to_string(std::io::stdin())?;
        return Ok(Some(content.trim_end().to_string()));
    }

    // Check if value looks like a text file path
    if looks_like_text_file(val) {
        let expanded = expand_tilde(val);
        if let Ok(content) = std::fs::read_to_string(&expanded) {
            return Ok(Some(content.trim_end().to_string()));
        }
        // Path doesn't exist or isn't readable → fall through to literal
    }

    Ok(Some(val.to_string()))
}

/// Check if a value looks like a text file path.
///
/// Heuristic: starts with `/`, `./`, `../`, `~` (explicit path prefix), or
/// has a `.md` or `.txt` extension. Other extensions are NOT matched to avoid
/// accidentally reading source files passed as literal text.
fn looks_like_text_file(val: &str) -> bool {
    if val.starts_with('/')
        || val.starts_with("./")
        || val.starts_with("../")
        || val.starts_with('~')
    {
        return true;
    }

    val.ends_with(".md") || val.ends_with(".txt")
}

/// Expand leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            if rest.is_empty() {
                return home.to_string();
            }
            if rest.starts_with('/') {
                return format!("{}{}", home, rest);
            }
        }
    }
    path.to_string()
}

/// Resolve a list of task references from CLI positional args, falling back to stdin if empty.
///
/// - Non-empty `ids` → wrapped as `TaskRef`s
/// - Empty `ids` → read lines from stdin, extract via `extract_fn`, wrap as `TaskRef`s
///
/// The `extract_fn` processes each stdin line (e.g., `extract_task_id` to
/// handle "Added: <id> — name" output from piped commands).
pub fn resolve_ref_list(
    ids: Vec<String>,
    extract_fn: fn(&str) -> String,
) -> Result<Vec<crate::tasks::TaskRef>> {
    use crate::tasks::TaskRef;

    if !ids.is_empty() {
        return Ok(ids.into_iter().map(|s| TaskRef(s)).collect());
    }

    // Read from stdin, processing each line individually
    use std::io::{self, BufRead};
    let stdin = io::stdin();
    let mut refs: Vec<TaskRef> = Vec::new();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| {
            AikiError::InvalidArgument(format!("Failed to read from stdin: {e}"))
        })?;
        let extracted = extract_fn(&line);
        if !extracted.is_empty() {
            refs.push(TaskRef(extracted));
        }
    }

    if refs.is_empty() {
        return Err(AikiError::InvalidArgument(
            "No task ID provided. Pass as argument or pipe from another command.".to_string(),
        ));
    }

    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_none_returns_none() {
        assert!(resolve_text(None).unwrap().is_none());
    }

    #[test]
    fn test_literal_text() {
        let result = resolve_text(Some("hello world")).unwrap();
        assert_eq!(result, Some("hello world".to_string()));
    }

    #[test]
    fn test_file_path_reads_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.md");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            write!(f, "file contents\n").unwrap();
        }
        let result = resolve_text(Some(file_path.to_str().unwrap())).unwrap();
        assert_eq!(result, Some("file contents".to_string()));
    }

    #[test]
    fn test_nonexistent_path_falls_back_to_literal() {
        let result = resolve_text(Some("./nonexistent.md")).unwrap();
        assert_eq!(result, Some("./nonexistent.md".to_string()));
    }

    #[test]
    fn test_tilde_expansion() {
        let expanded = expand_tilde("~/foo/bar");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.ends_with("/foo/bar"));
    }

    #[test]
    fn test_looks_like_text_file() {
        assert!(looks_like_text_file("/absolute/path"));
        assert!(looks_like_text_file("./relative"));
        assert!(looks_like_text_file("../parent"));
        assert!(looks_like_text_file("~/home"));
        assert!(looks_like_text_file("file.md"));
        assert!(looks_like_text_file("file.txt"));
        assert!(!looks_like_text_file("auth.rs"));
        assert!(!looks_like_text_file("config.toml"));
        assert!(!looks_like_text_file("hello world"));
        assert!(!looks_like_text_file("just text"));
    }

    // --- resolve_ref_list tests ---

    fn identity(s: &str) -> String {
        s.trim().to_string()
    }

    #[test]
    fn test_resolve_ref_list_non_empty_vec() {
        use crate::tasks::TaskRef;
        let ids = vec!["abc".to_string(), "def".to_string()];
        let result = resolve_ref_list(ids, identity).unwrap();
        assert_eq!(result, vec![TaskRef("abc".to_string()), TaskRef("def".to_string())]);
    }

    #[test]
    fn test_resolve_ref_list_single_id() {
        use crate::tasks::TaskRef;
        let ids = vec!["mvslrsp".to_string()];
        let result = resolve_ref_list(ids, identity).unwrap();
        assert_eq!(result, vec![TaskRef("mvslrsp".to_string())]);
    }

    // resolve_ref tests

    #[test]
    fn test_resolve_ref_full_task_id() {
        let input = "mvslrspmoynoxyyywqyutmovxpvztkls";
        let result = resolve_ref(input, None).unwrap();
        assert_eq!(result, RefKind::Task(TaskRef(input.to_string())));
    }

    #[test]
    fn test_resolve_ref_task_prefix() {
        let input = "mvslrsp";
        let result = resolve_ref(input, None).unwrap();
        assert_eq!(result, RefKind::Task(TaskRef(input.to_string())));
    }

    #[test]
    fn test_resolve_ref_slug_ref() {
        let input = "mvslrsp:build";
        let result = resolve_ref(input, None).unwrap();
        assert_eq!(result, RefKind::Task(TaskRef(input.to_string())));
    }

    #[test]
    fn test_resolve_ref_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("plan.md");
        std::fs::write(&file_path, "content").unwrap();
        let result = resolve_ref("plan.md", Some(dir.path())).unwrap();
        assert_eq!(result, RefKind::Plan(dir.path().join("plan.md")));
    }

    #[test]
    fn test_resolve_ref_non_md_file_exists() {
        // Non-text extensions (.py, .rs, etc.) are not recognized — only .md/.txt are
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("script.py");
        std::fs::write(&file_path, "print('hello')").unwrap();
        let result = resolve_ref("script.py", Some(dir.path()));
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Unsupported file extension '.py'"),
            "Expected unsupported extension error, got: {err}"
        );
    }

    #[test]
    fn test_resolve_ref_txt_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("notes.txt");
        std::fs::write(&file_path, "some notes").unwrap();
        let result = resolve_ref("notes.txt", Some(dir.path())).unwrap();
        assert_eq!(result, RefKind::File(dir.path().join("notes.txt")));
    }

    #[test]
    fn test_resolve_ref_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let result = resolve_ref("missing.md", Some(dir.path()));
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_ref_cwd_none_skips_file_check() {
        // Without cwd, a non-task-id with extension is an error, not a file check
        let result = resolve_ref("plan.md", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_ref_unrecognized_input() {
        let result = resolve_ref("hello world", None);
        assert!(result.is_err());
    }
}

//! `aiki plugin` CLI subcommand — install, update, list, and remove plugins.

use clap::Subcommand;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;

use crate::error::{AikiError, Result};
use crate::plugins::deps::{install_with_deps, resolve_deps, InstallReport};
use crate::plugins::graph::PluginGraph;
use crate::plugins::git::{pull_plugin, remove_plugin};
use crate::plugins::scanner::derive_plugin_refs;
use crate::plugins::{
    check_install_status, list_installed_plugins, plugins_base_dir, InstallStatus, PluginRef,
};
use crate::tasks::templates::TASKS_DIR_NAME;

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum PluginCommands {
    /// Install a plugin from GitHub (and its dependencies)
    Install {
        /// Plugin reference (e.g., aiki/way, somecorp/security). Omit to install from project.
        reference: Option<String>,
    },
    /// Update an installed plugin (and reconcile dependencies)
    Update {
        /// Plugin reference to update. Omit to update all installed.
        reference: Option<String>,
    },
    /// List installed plugins
    List {
        /// Limit the number of results shown
        #[arg(long, short = 'n')]
        number: Option<usize>,
    },
    /// Remove an installed plugin
    Remove {
        /// Plugin reference to remove (e.g., aiki/way)
        reference: String,
        /// Remove even if other plugins depend on this one
        #[arg(long)]
        force: bool,
    },
}

pub fn run(command: PluginCommands) -> Result<()> {
    match command {
        PluginCommands::Install { reference } => run_install(reference),
        PluginCommands::Update { reference } => run_update(reference),
        PluginCommands::List { number } => run_list(number),
        PluginCommands::Remove { reference, force } => run_remove(reference, force),
    }
}

fn run_install(reference: Option<String>) -> Result<()> {
    let plugins_base = plugins_base_dir()?;

    match reference {
        Some(ref_str) => {
            let plugin: PluginRef = ref_str.parse()?;
            let report = install_with_deps(&plugin, &plugins_base)?;
            print_install_report(&report, &plugin);

            if !report.failed.is_empty() {
                Err(anyhow::anyhow!("Some plugins failed to install"))?;
            }
        }
        None => {
            // Scan current project for plugin references
            let cwd = env::current_dir()?;
            let aiki_dir = find_aiki_dir(&cwd)?;

            let refs = derive_plugin_refs(&aiki_dir, None);
            if refs.is_empty() {
                println!("No plugin references found in project.");
                return Ok(());
            }

            let mut any_failed = false;
            for plugin in &refs {
                let report = install_with_deps(plugin, &plugins_base)?;
                print_install_report(&report, plugin);
                if !report.failed.is_empty() {
                    any_failed = true;
                }
            }

            if any_failed {
                Err(anyhow::anyhow!("Some plugins failed to install"))?;
            }
        }
    }

    Ok(())
}

fn run_update(reference: Option<String>) -> Result<()> {
    let plugins_base = plugins_base_dir()?;

    match reference {
        Some(ref_str) => {
            let plugin: PluginRef = ref_str.parse()?;
            update_single(&plugin, &plugins_base)?;
        }
        None => {
            // Update all installed plugins
            let installed = list_installed_plugins(&plugins_base);
            if installed.is_empty() {
                println!("No plugins installed.");
                return Ok(());
            }

            let mut any_failed = false;
            for plugin in &installed {
                if let Err(e) = update_single(plugin, &plugins_base) {
                    eprintln!("Error: failed to update {} — {}", plugin, e);
                    any_failed = true;
                }
            }

            if any_failed {
                Err(anyhow::anyhow!("Some plugins failed to update"))?;
            }
        }
    }

    Ok(())
}

fn update_single(plugin: &PluginRef, plugins_base: &Path) -> Result<()> {
    // 1. Pull the root plugin
    pull_plugin(plugin, plugins_base)?;
    println!("Updated: {}", plugin.display_name(plugins_base));

    let mut failures: Vec<String> = Vec::new();

    // 2. Resolve deps after pulling root, then pull existing deps
    let deps = resolve_deps(plugin, plugins_base);
    let mut pulled: HashSet<PluginRef> = HashSet::new();
    for dep in &deps {
        if check_install_status(dep, plugins_base) == InstallStatus::Installed {
            if let Err(e) = pull_plugin(dep, plugins_base) {
                eprintln!("Error: failed to update dependency {}: {}", dep, e);
                // Don't record failure yet — dep may be retried in second pass
            } else {
                pulled.insert(dep.clone());
            }
        }
    }

    // 3. Re-resolve deps after pulls — pulled deps may now reference new plugins
    let deps_after = resolve_deps(plugin, plugins_base);
    for dep in &deps_after {
        match check_install_status(dep, plugins_base) {
            InstallStatus::Installed => {
                // Pull deps discovered or retried after re-resolve (not yet pulled)
                if !pulled.contains(dep) {
                    if let Err(e) = pull_plugin(dep, plugins_base) {
                        eprintln!("Error: failed to update dependency {}: {}", dep, e);
                        failures.push(format!("{}: {}", dep, e));
                    }
                }
            }
            _ => {
                // New dep — install with its own deps
                let report = install_with_deps(dep, plugins_base)?;
                for (installed, _manifest) in &report.installed {
                    println!("Installed (dependency): {}", installed.display_name(plugins_base));
                }
                for (failed, err) in &report.failed {
                    eprintln!("Error: failed to install {}: {}", failed, err);
                    failures.push(format!("{}: {}", failed, err));
                }
            }
        }
    }

    if !failures.is_empty() {
        return Err(AikiError::PluginOperationFailed {
            plugin: plugin.to_string(),
            details: format!(
                "{} dependency failure(s): {}",
                failures.len(),
                failures.join("; ")
            ),
        });
    }

    Ok(())
}

fn run_list(number: Option<usize>) -> Result<()> {
    let plugins_base = plugins_base_dir()?;
    let cwd = env::current_dir()?;

    // Check if we're in a project
    let aiki_dir = find_aiki_dir_optional(&cwd);

    match aiki_dir {
        Some(aiki_dir) => {
            // In repo: show project context
            let mut project_refs = derive_plugin_refs(&aiki_dir, None);
            if let Some(n) = number {
                project_refs.truncate(n);
            }

            if project_refs.is_empty() {
                println!("No plugin references found in project.");
            } else {
                let graph = PluginGraph::build(&plugins_base);
                println!("Plugins:");
                for plugin in &project_refs {
                    let status = check_install_status(plugin, &plugins_base);
                    match status {
                        InstallStatus::Installed => {
                            println!("  {}    installed", graph.display_name(plugin));
                            // Show deps
                            for dep in &graph.dependencies(plugin) {
                                let dep_status = check_install_status(dep, &plugins_base);
                                match dep_status {
                                    InstallStatus::Installed => {
                                        println!("    └ {}    installed (dependency)", dep);
                                    }
                                    _ => {
                                        println!("    └ {}    not installed", dep);
                                    }
                                }
                            }
                        }
                        InstallStatus::PartialInstall => {
                            println!(
                                "  {}    partial install    ← aiki plugin install {}",
                                plugin, plugin
                            );
                        }
                        InstallStatus::NotInstalled => {
                            println!(
                                "  {}    not installed    ← aiki plugin install {}",
                                plugin, plugin
                            );
                        }
                    }
                }
            }

            // Check for overrides
            let templates_dir = aiki_dir.join(TASKS_DIR_NAME);
            if templates_dir.is_dir() {
                let overrides = find_overrides(&templates_dir, &project_refs);
                if !overrides.is_empty() {
                    println!();
                    println!("Overrides:");
                    for (ref_path, local_path) in &overrides {
                        println!("  {} → {}", ref_path, local_path);
                    }
                }
            }
        }
        None => {
            // Outside repo: list all installed
            let installed = list_installed_plugins(&plugins_base);
            if installed.is_empty() {
                println!("No plugins installed.");
                return Ok(());
            }

            let graph = PluginGraph::build(&plugins_base);

            // Build (plugin, deps) pairs from the graph
            let mut plugin_deps: Vec<(PluginRef, Vec<PluginRef>)> = installed
                .iter()
                .map(|p| (p.clone(), graph.dependencies(p)))
                .collect();

            // A plugin is a root if nothing depends on it.
            // If no roots exist (all in cycles), show everything as top-level.
            let roots: HashSet<&PluginRef> = installed
                .iter()
                .filter(|p| graph.dependents(p).is_empty())
                .collect();

            let hidden: HashSet<PluginRef> = if roots.is_empty() {
                HashSet::new()
            } else {
                plugin_deps
                    .iter()
                    .filter(|(p, _)| roots.contains(p))
                    .flat_map(|(_, deps)| deps.iter().cloned())
                    .collect()
            };

            if let Some(n) = number {
                plugin_deps.truncate(n);
            }
            println!("Installed ({}):", plugins_base.display());
            for (plugin, deps) in &plugin_deps {
                if hidden.contains(plugin) {
                    continue; // Only show as dependency under its parent
                }
                println!("  {}", graph.display_name(plugin));
                for dep in deps {
                    println!("    └ {}    (dependency)", dep);
                }
            }
        }
    }

    Ok(())
}

fn run_remove(reference: String, force: bool) -> Result<()> {
    let plugins_base = plugins_base_dir()?;
    let plugin: PluginRef = reference.parse()?;

    // Capture display name before removing (removal deletes the directory)
    let display_name = plugin.display_name(&plugins_base);

    // Error if other installed plugins depend on this one (unless --force)
    let dependents = find_dependents(&plugin, &plugins_base);
    if !dependents.is_empty() {
        let names: Vec<String> = dependents.iter().map(|d| d.to_string()).collect();
        if force {
            eprintln!(
                "Warning: {} is a dependency of: {}",
                plugin,
                names.join(", ")
            );
        } else {
            return Err(AikiError::PluginHasDependents {
                plugin: plugin.to_string(),
                dependents: names.join(", "),
            });
        }
    }

    remove_plugin(&plugin, &plugins_base)?;

    // Remove from hooks.yml include references in the current project (if in one)
    let cwd = env::current_dir()?;
    if let Some(aiki_dir) = find_aiki_dir_optional(&cwd) {
        remove_from_hooks_yml(&aiki_dir, &plugin);
    }

    println!("Removed: {}", display_name);
    Ok(())
}

// --- Helpers ---

fn find_aiki_dir(start: &Path) -> Result<std::path::PathBuf> {
    find_aiki_dir_optional(start).ok_or_else(|| AikiError::InvalidArgument(
        "Not in an aiki project. Use 'aiki plugin install <reference>' to install a specific plugin.".to_string(),
    ))
}

fn find_aiki_dir_optional(start: &Path) -> Option<std::path::PathBuf> {
    let mut current = start;
    loop {
        let aiki_dir = current.join(".aiki");
        if aiki_dir.is_dir() {
            return Some(aiki_dir);
        }
        current = current.parent()?;
    }
}

/// Find installed plugins that depend on the given plugin (direct dependents only).
pub(crate) fn find_dependents(plugin: &PluginRef, plugins_base: &Path) -> Vec<PluginRef> {
    PluginGraph::build(plugins_base).dependents(plugin)
}

/// Print install report.
///
/// Validation (plugin.yaml check + cleanup) is handled by `install_single` in
/// `deps.rs`, so this function only formats the output.
fn print_install_report(report: &InstallReport, root: &PluginRef) {
    for (plugin, manifest) in &report.installed {
        let label = if plugin == root {
            "Installed"
        } else {
            "Installed (dependency)"
        };
        let plugin_str = plugin.to_string();
        match manifest.name.as_deref() {
            Some(name) if !name.is_empty() && name != plugin_str => {
                println!("{}: {} ({})", label, plugin, name);
            }
            _ => println!("{}: {}", label, plugin),
        }
    }
    for (failed, err) in &report.failed {
        eprintln!("Error: failed to install {} — {}", failed, err);
    }
}

/// Remove a plugin reference from `include:` lists in the project's `hooks.yml`.
///
/// Scans both top-level `include:` and `before:/after:` composition block includes.
/// Writes the file back only if changes were made.
fn remove_from_hooks_yml(aiki_dir: &Path, plugin: &PluginRef) {
    let hooks_path = aiki_dir.join("hooks.yml");
    if !hooks_path.is_file() {
        return;
    }

    let content = match fs::read_to_string(&hooks_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let plugin_str = plugin.to_string();

    // Filter out lines that are include references to this plugin.
    // We operate on raw lines to preserve formatting/comments.
    // TODO: This removes ALL list items matching the plugin name regardless of
    // which YAML key they appear under (not just `include:` blocks). Should be
    // replaced with YAML-aware removal when PluginGraph lands
    // (see ops/now/plugins/dependency-graph.md).
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
    let mut changed = false;

    for line in &lines {
        let trimmed = line.trim();
        // Match list items like "- aiki/way" or "- aiki/way  # comment"
        if trimmed.starts_with("- ") {
            let value = trimmed[2..].trim();
            // Strip trailing comment
            let value = value.split('#').next().unwrap_or(value).trim();
            if value == plugin_str {
                changed = true;
                continue; // Skip this line
            }
        }
        new_lines.push(line);
    }

    if changed {
        let new_content = new_lines.join("\n");
        // Preserve trailing newline if original had one
        let new_content = if content.ends_with('\n') && !new_content.ends_with('\n') {
            format!("{}\n", new_content)
        } else {
            new_content
        };
        if let Err(e) = fs::write(&hooks_path, new_content) {
            eprintln!("Warning: failed to update hooks.yml: {}", e);
        }
    }
}


/// Find project-level template overrides of plugin templates.
fn find_overrides(templates_dir: &Path, project_refs: &[PluginRef]) -> Vec<(String, String)> {
    let mut overrides = Vec::new();

    // For each plugin ref, check if there's a local template override
    let ref_set: HashSet<String> = project_refs.iter().map(|r| r.to_string()).collect();

    // Walk the templates directory looking for three-part paths matching known plugins
    collect_overrides(templates_dir, templates_dir, &ref_set, &mut overrides);

    overrides
}

fn collect_overrides(
    base: &Path,
    current: &Path,
    known_plugins: &HashSet<String>,
    overrides: &mut Vec<(String, String)>,
) {
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_overrides(base, &path, known_plugins, overrides);
        } else if path.is_file() && path.extension().map_or(false, |e| e == "md") {
            let relative = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .with_extension("")
                .display()
                .to_string()
                .replace('\\', "/");

            // Check if this is a three-part path matching a known plugin
            let parts: Vec<&str> = relative.split('/').collect();
            if parts.len() >= 3 {
                let plugin_ref = format!("{}/{}", parts[0], parts[1]);
                if known_plugins.contains(&plugin_ref) {
                    overrides.push((
                        relative,
                        path.strip_prefix(base.parent().unwrap_or(base))
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_remove_from_hooks_yml_removes_matching_line() {
        let tmp = TempDir::new().unwrap();
        let aiki_dir = tmp.path().join(".aiki");
        fs::create_dir_all(&aiki_dir).unwrap();

        let hooks_content = "include:\n  - eslint/standard\n  - aiki/way\n";
        fs::write(aiki_dir.join("hooks.yml"), hooks_content).unwrap();

        let plugin: PluginRef = "eslint/standard".parse().unwrap();
        remove_from_hooks_yml(&aiki_dir, &plugin);

        let result = fs::read_to_string(aiki_dir.join("hooks.yml")).unwrap();
        assert!(!result.contains("eslint/standard"));
        assert!(result.contains("aiki/way"));
    }

    #[test]
    fn test_remove_from_hooks_yml_preserves_trailing_newline() {
        let tmp = TempDir::new().unwrap();
        let aiki_dir = tmp.path().join(".aiki");
        fs::create_dir_all(&aiki_dir).unwrap();

        let hooks_content = "include:\n  - eslint/standard\n  - aiki/way\n";
        fs::write(aiki_dir.join("hooks.yml"), hooks_content).unwrap();

        let plugin: PluginRef = "eslint/standard".parse().unwrap();
        remove_from_hooks_yml(&aiki_dir, &plugin);

        let result = fs::read_to_string(aiki_dir.join("hooks.yml")).unwrap();
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn test_remove_from_hooks_yml_no_file() {
        let tmp = TempDir::new().unwrap();
        let aiki_dir = tmp.path().join(".aiki");
        fs::create_dir_all(&aiki_dir).unwrap();

        let plugin: PluginRef = "eslint/standard".parse().unwrap();
        // Should not panic when hooks.yml doesn't exist
        remove_from_hooks_yml(&aiki_dir, &plugin);
    }

    #[test]
    fn test_remove_from_hooks_yml_no_match() {
        let tmp = TempDir::new().unwrap();
        let aiki_dir = tmp.path().join(".aiki");
        fs::create_dir_all(&aiki_dir).unwrap();

        let original = "include:\n  - aiki/way\n";
        fs::write(aiki_dir.join("hooks.yml"), original).unwrap();

        let plugin: PluginRef = "eslint/standard".parse().unwrap();
        remove_from_hooks_yml(&aiki_dir, &plugin);

        let result = fs::read_to_string(aiki_dir.join("hooks.yml")).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn test_remove_from_hooks_yml_with_comment() {
        let tmp = TempDir::new().unwrap();
        let aiki_dir = tmp.path().join(".aiki");
        fs::create_dir_all(&aiki_dir).unwrap();

        let hooks_content = "include:\n  - eslint/standard  # linting\n  - aiki/way\n";
        fs::write(aiki_dir.join("hooks.yml"), hooks_content).unwrap();

        let plugin: PluginRef = "eslint/standard".parse().unwrap();
        remove_from_hooks_yml(&aiki_dir, &plugin);

        let result = fs::read_to_string(aiki_dir.join("hooks.yml")).unwrap();
        assert!(!result.contains("eslint/standard"));
        assert!(result.contains("aiki/way"));
    }

    /// Helper: create a fake installed plugin with optional deps declared via hooks.yaml.
    fn create_fake_plugin(base: &Path, ns: &str, name: &str, deps: &[&str]) {
        let dir = base.join(ns).join(name);
        fs::create_dir_all(dir.join(".git")).unwrap();

        if !deps.is_empty() {
            let yaml: Vec<String> = deps
                .iter()
                .enumerate()
                .map(|(i, d)| format!("hook{}:\n  template: {}", i, d))
                .collect();
            fs::write(dir.join("hooks.yaml"), yaml.join("\n")).unwrap();
        }
    }

    #[test]
    fn test_find_dependents_no_dependents() {
        let tmp = TempDir::new().unwrap();
        create_fake_plugin(tmp.path(), "aiki", "alpha", &[]);

        let plugin: PluginRef = "aiki/alpha".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        assert!(deps.is_empty());
    }

    #[test]
    fn test_find_dependents_direct_dependent() {
        let tmp = TempDir::new().unwrap();
        create_fake_plugin(tmp.path(), "aiki", "alpha", &[]);
        create_fake_plugin(tmp.path(), "aiki", "beta", &["aiki/alpha/tmpl"]);

        let plugin: PluginRef = "aiki/alpha".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].to_string(), "aiki/beta");
    }

    #[test]
    fn test_find_dependents_not_a_dependent() {
        let tmp = TempDir::new().unwrap();
        create_fake_plugin(tmp.path(), "aiki", "alpha", &[]);
        create_fake_plugin(tmp.path(), "aiki", "beta", &["aiki/gamma/tmpl"]);
        create_fake_plugin(tmp.path(), "aiki", "gamma", &[]);

        let plugin: PluginRef = "aiki/alpha".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        assert!(deps.is_empty());
    }

    #[test]
    fn test_find_dependents_multiple_dependents() {
        let tmp = TempDir::new().unwrap();
        create_fake_plugin(tmp.path(), "aiki", "alpha", &[]);
        create_fake_plugin(tmp.path(), "aiki", "beta", &["aiki/alpha/tmpl"]);
        create_fake_plugin(tmp.path(), "aiki", "gamma", &["aiki/alpha/tmpl"]);

        let plugin: PluginRef = "aiki/alpha".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        let names: Vec<String> = deps.iter().map(|d| d.to_string()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"aiki/beta".to_string()));
        assert!(names.contains(&"aiki/gamma".to_string()));
    }

    #[test]
    fn test_find_dependents_direct_only() {
        let tmp = TempDir::new().unwrap();
        // A depends on B, B depends on C
        create_fake_plugin(tmp.path(), "aiki", "a", &["aiki/b/tmpl"]);
        create_fake_plugin(tmp.path(), "aiki", "b", &["aiki/c/tmpl"]);
        create_fake_plugin(tmp.path(), "aiki", "c", &[]);

        // find_dependents(C) should return only B (direct dependent)
        let plugin: PluginRef = "aiki/c".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        let names: Vec<String> = deps.iter().map(|d| d.to_string()).collect();
        assert_eq!(names, vec!["aiki/b".to_string()]);
    }

    #[test]
    fn test_find_dependents_plugin_not_installed() {
        let tmp = TempDir::new().unwrap();
        // No plugins installed at all
        let plugin: PluginRef = "aiki/nonexistent".parse().unwrap();
        let deps = find_dependents(&plugin, tmp.path());
        assert!(deps.is_empty());
    }
}

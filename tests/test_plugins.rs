//! Integration tests for the plugin system.
//!
//! These tests exercise reference scanning, dependency resolution, template
//! resolution with plugins, and CLI subcommand wiring — all without network
//! access (no actual git clones).

use std::fs;
use std::path::Path;
use tempfile::TempDir;

use aiki::plugins::scanner::derive_plugin_refs;
use aiki::plugins::{check_install_status, InstallStatus, PluginRef};

// ---------------------------------------------------------------------------
// Helper: create a fake installed plugin (dir + .git/ marker)
// ---------------------------------------------------------------------------
fn fake_install(plugins_base: &Path, ns: &str, name: &str) {
    let dir = plugins_base.join(ns).join(name);
    fs::create_dir_all(dir.join(".git")).unwrap();
}

/// Create a fake installed plugin that itself references other plugins
/// (via a hooks.yaml with template: lines under unique parent keys).
fn fake_install_with_deps(plugins_base: &Path, ns: &str, name: &str, dep_refs: &[&str]) {
    let dir = plugins_base.join(ns).join(name);
    fs::create_dir_all(dir.join(".git")).unwrap();
    let mut yaml = String::new();
    for (i, dep) in dep_refs.iter().enumerate() {
        yaml.push_str(&format!("hook{}:\n  template: {}\n", i, dep));
    }
    fs::write(dir.join("hooks.yaml"), yaml).unwrap();
}

// ---------------------------------------------------------------------------
// Reference scanning — end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_scan_project_finds_yaml_and_markdown_refs() {
    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");

    // Create hooks.yaml with a three-part template ref
    fs::create_dir_all(&aiki_dir).unwrap();
    fs::write(
        aiki_dir.join("hooks.yaml"),
        "on_review:\n  template: acme/security/scan\n",
    )
    .unwrap();

    // Create a template with a partial ref
    let tpl_dir = aiki_dir.join("tasks");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(
        tpl_dir.join("review.md"),
        "# Review\n\n{{> corp/lint/check}}\n",
    )
    .unwrap();

    let refs = derive_plugin_refs(&aiki_dir, None);

    let ref_strs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
    assert!(ref_strs.contains(&"acme/security".to_string()));
    assert!(ref_strs.contains(&"corp/lint".to_string()));
    assert_eq!(refs.len(), 2);
}

#[test]
fn test_scan_excludes_code_blocks_and_inline_code() {
    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");
    let tpl_dir = aiki_dir.join("tasks");
    fs::create_dir_all(&tpl_dir).unwrap();

    // This template has refs in code blocks (should be excluded) and real refs
    let content = r#"# Template

Real partial: {{> real/plugin/template}}

```markdown
Fenced code: {{> fake/plugin/excluded}}
```

Inline code: `{{> inline/plugin/also_excluded}}`

<!-- HTML comment: {{> comment/plugin/hidden}} -->
"#;
    fs::write(tpl_dir.join("mixed.md"), content).unwrap();

    let refs = derive_plugin_refs(&aiki_dir, None);
    let ref_strs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();

    assert!(ref_strs.contains(&"real/plugin".to_string()));
    assert!(!ref_strs.contains(&"fake/plugin".to_string()));
    assert!(!ref_strs.contains(&"inline/plugin".to_string()));
    assert!(!ref_strs.contains(&"comment/plugin".to_string()));
    assert_eq!(refs.len(), 1);
}

#[test]
fn test_scan_deduplicates_across_files() {
    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");
    let tpl_dir = aiki_dir.join("tasks");
    fs::create_dir_all(&tpl_dir).unwrap();

    // Same plugin referenced from two different templates
    fs::write(tpl_dir.join("a.md"), "{{> shared/plugin/one}}\n").unwrap();
    fs::write(tpl_dir.join("b.md"), "{{> shared/plugin/two}}\n").unwrap();

    let refs = derive_plugin_refs(&aiki_dir, None);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].to_string(), "shared/plugin");
}

#[test]
fn test_scan_filters_self_ref() {
    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");
    let tpl_dir = aiki_dir.join("tasks");
    fs::create_dir_all(&tpl_dir).unwrap();

    fs::write(
        tpl_dir.join("a.md"),
        "{{> myself/plugin/tpl}}\n{{> other/plugin/tpl}}\n",
    )
    .unwrap();

    let self_ref: PluginRef = "myself/plugin".parse().unwrap();
    let refs = derive_plugin_refs(&aiki_dir, Some(&self_ref));

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].to_string(), "other/plugin");
}

// ---------------------------------------------------------------------------
// Dependency resolution — end-to-end with temp plugin tree
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_deps_transitive_chain() {
    use aiki::plugins::deps::resolve_deps;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path();

    // A depends on B, B depends on C
    fake_install_with_deps(base, "ns", "a", &["ns/b/tpl"]);
    fake_install_with_deps(base, "ns", "b", &["ns/c/tpl"]);
    fake_install(base, "ns", "c");

    let root: PluginRef = "ns/a".parse().unwrap();
    let deps = resolve_deps(&root, base);

    let dep_strs: Vec<String> = deps.iter().map(|r| r.to_string()).collect();
    assert!(dep_strs.contains(&"ns/b".to_string()));
    assert!(dep_strs.contains(&"ns/c".to_string()));
    // Root should not be in deps
    assert!(!dep_strs.contains(&"ns/a".to_string()));
}

#[test]
fn test_resolve_deps_diamond() {
    use aiki::plugins::deps::resolve_deps;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path();

    // A depends on B and C; both B and C depend on D
    fake_install_with_deps(base, "ns", "a", &["ns/b/tpl", "ns/c/tpl"]);
    fake_install_with_deps(base, "ns", "b", &["ns/d/tpl"]);
    fake_install_with_deps(base, "ns", "c", &["ns/d/tpl"]);
    fake_install(base, "ns", "d");

    let root: PluginRef = "ns/a".parse().unwrap();
    let deps = resolve_deps(&root, base);

    let dep_strs: Vec<String> = deps.iter().map(|r| r.to_string()).collect();
    assert!(dep_strs.contains(&"ns/b".to_string()));
    assert!(dep_strs.contains(&"ns/c".to_string()));
    assert!(dep_strs.contains(&"ns/d".to_string()));

    // D should appear exactly once (dedup)
    assert_eq!(dep_strs.iter().filter(|s| *s == "ns/d").count(), 1);
}

#[test]
fn test_resolve_deps_cycle_does_not_hang() {
    use aiki::plugins::deps::resolve_deps;

    let tmp = TempDir::new().unwrap();
    let base = tmp.path();

    // A depends on B, B depends on A (cycle)
    fake_install_with_deps(base, "ns", "a", &["ns/b/tpl"]);
    fake_install_with_deps(base, "ns", "b", &["ns/a/tpl"]);

    let root: PluginRef = "ns/a".parse().unwrap();
    let deps = resolve_deps(&root, base);

    // Should complete without hanging; B is a dep of A
    let dep_strs: Vec<String> = deps.iter().map(|r| r.to_string()).collect();
    assert!(dep_strs.contains(&"ns/b".to_string()));
}

// ---------------------------------------------------------------------------
// Install status checking
// ---------------------------------------------------------------------------

#[test]
fn test_install_status_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path();
    let plugin: PluginRef = "test/myplugin".parse().unwrap();

    // Not installed
    assert_eq!(
        check_install_status(&plugin, base),
        InstallStatus::NotInstalled
    );

    // Create directory without .git/ → partial
    let dir = plugin.install_dir(base);
    fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        check_install_status(&plugin, base),
        InstallStatus::PartialInstall
    );

    // Add .git/ → installed
    fs::create_dir_all(dir.join(".git")).unwrap();
    assert_eq!(
        check_install_status(&plugin, base),
        InstallStatus::Installed
    );
}

// ---------------------------------------------------------------------------
// Template resolution with plugin fallback (via public load_template API)
// ---------------------------------------------------------------------------

/// Mutex to serialize tests that modify the AIKI_HOME env var.
static AIKI_HOME_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run a closure with AIKI_HOME pointing to a temp directory.
/// Restores the original value (or removes it) when done, even on panic.
fn with_temp_aiki_home<F, R>(aiki_home: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    struct RestoreEnv(Option<String>);
    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => std::env::set_var("AIKI_HOME", v),
                None => std::env::remove_var("AIKI_HOME"),
            }
        }
    }

    let _lock = AIKI_HOME_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let _restore = RestoreEnv(std::env::var("AIKI_HOME").ok());
    std::env::set_var("AIKI_HOME", aiki_home);
    f()
}

#[test]
fn test_template_resolution_three_part_ref_falls_back_to_plugin() {
    use aiki::tasks::templates::resolver::load_template;

    let tmp = TempDir::new().unwrap();
    let aiki_home = TempDir::new().unwrap();

    // Create project .aiki/tasks/ (empty — no override)
    let project_templates = tmp.path().join(".aiki").join("tasks");
    fs::create_dir_all(&project_templates).unwrap();

    // Create a fake plugin with a template under the temp AIKI_HOME
    let plugins_base = aiki_home.path().join("plugins");
    let plugin_tpl_dir = plugins_base
        .join("testns")
        .join("testplug")
        .join("tasks");
    fs::create_dir_all(&plugin_tpl_dir).unwrap();
    fs::create_dir_all(plugins_base.join("testns").join("testplug").join(".git")).unwrap();
    fs::write(
        plugin_tpl_dir.join("review.md"),
        "---\nname: Plugin Review\n---\n# Plugin review template\n",
    )
    .unwrap();

    // Point AIKI_HOME to our temp dir so plugins_base_dir() resolves there
    with_temp_aiki_home(aiki_home.path(), || {
        let result = load_template("testns/testplug/review", &project_templates);
        assert!(
            result.is_ok(),
            "Should resolve three-part ref to plugin: {:?}",
            result
        );
    });
}

#[test]
fn test_template_resolution_project_override_wins() {
    use aiki::tasks::templates::resolver::load_template;

    let tmp = TempDir::new().unwrap();
    let aiki_home = TempDir::new().unwrap();

    // Create project templates dir
    let project_templates = tmp.path().join(".aiki").join("tasks");

    let plugins_base = aiki_home.path().join("plugins");

    // Create plugin template
    let plugin_tpl_dir = plugins_base
        .join("testns2")
        .join("overplug")
        .join("tasks");
    fs::create_dir_all(&plugin_tpl_dir).unwrap();
    fs::create_dir_all(plugins_base.join("testns2").join("overplug").join(".git")).unwrap();
    fs::write(
        plugin_tpl_dir.join("task.md"),
        "---\nname: Plugin Version\n---\n# Plugin version\n",
    )
    .unwrap();

    // Create project override at .aiki/tasks/testns2/overplug/task.md
    let project_tpl = project_templates.join("testns2").join("overplug");
    fs::create_dir_all(&project_tpl).unwrap();
    fs::write(
        project_tpl.join("task.md"),
        "---\nname: Project Override\n---\n# Project override\n",
    )
    .unwrap();

    // Point AIKI_HOME to our temp dir so plugins_base_dir() resolves there
    with_temp_aiki_home(aiki_home.path(), || {
        let result = load_template("testns2/overplug/task", &project_templates);
        assert!(
            result.is_ok(),
            "Should resolve to project override: {:?}",
            result
        );

        let template = result.unwrap();
        // The project version should win — source_path should point to the project dir, not plugins
        let source = template.source_path.unwrap_or_default();
        assert!(
            source.starts_with(tmp.path().to_str().unwrap()),
            "Project override should win over plugin. source_path: {}",
            source
        );
    });
}

// ---------------------------------------------------------------------------
// CLI wiring tests (assert_cmd — no network)
// ---------------------------------------------------------------------------

#[test]
fn test_plugin_help() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_plugin_remove_invalid_ref() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "remove", "not-a-valid-ref"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("namespace/name"));
}

#[test]
fn test_plugin_remove_with_dot_in_namespace() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "remove", "github.com/repo"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Only GitHub plugins"));
}

/// Outside-repo `plugin list` reports no project plugin references.
#[test]
fn test_plugin_list_outside_repo_cycle_shows_all_top_level() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let aiki_home = TempDir::new().unwrap();
    let plugins_base = aiki_home.path().join("plugins");

    // A→B→A (cycle: every plugin is someone's dependency)
    fake_install_with_deps(plugins_base.as_path(), "ns", "alpha", &["ns/beta/tmpl"]);
    fake_install_with_deps(plugins_base.as_path(), "ns", "beta", &["ns/alpha/tmpl"]);

    // Run from a temp dir without .aiki (outside-repo path)
    let workdir = TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "list"])
        .current_dir(workdir.path())
        .env("AIKI_HOME", aiki_home.path());

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "plugin list should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No plugin references found in project."),
        "expected outside-repo plugin list message, got: {}",
        stdout
    );
}

/// Outside-repo `plugin list` reports no project plugin references.
#[test]
fn test_plugin_list_outside_repo_hides_deps_of_roots() {
    use std::process::Command;

    let aiki_home = TempDir::new().unwrap();
    let plugins_base = aiki_home.path().join("plugins");

    // root→dep (root is not a dependency of anyone)
    fake_install_with_deps(plugins_base.as_path(), "ns", "root", &["ns/dep/tmpl"]);
    fake_install(plugins_base.as_path(), "ns", "dep");

    let workdir = TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "list"])
        .current_dir(workdir.path())
        .env("AIKI_HOME", aiki_home.path());

    let output = cmd.output().unwrap();
    assert!(output.status.success(), "plugin list should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No plugin references found in project."),
        "expected outside-repo plugin list message, got: {}",
        stdout
    );
}

// ---------------------------------------------------------------------------
// Update-all continue-on-error
// ---------------------------------------------------------------------------

/// `plugin update` (update-all) attempts every installed plugin even when
/// one fails, then exits non-zero.  This ensures the continue-on-error
/// semantics are not regressed.
#[test]
fn test_plugin_update_all_continues_on_error() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let aiki_home = TempDir::new().unwrap();
    let plugins_base = aiki_home.path().join("plugins");

    // Create two fake plugins with .git/ dirs (count as "installed") but
    // without real git repos — `git pull` will fail for both.
    fake_install(&plugins_base, "org", "alpha");
    fake_install(&plugins_base, "org", "beta");

    let workdir = TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "update"])
        .current_dir(workdir.path())
        .env("AIKI_HOME", aiki_home.path());

    // Both plugins should be attempted (per-plugin error messages) and
    // the command should exit non-zero.
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("org/alpha"))
        .stderr(predicate::str::contains("org/beta"));
}

// ---------------------------------------------------------------------------
// Project-level plugin operations
// ---------------------------------------------------------------------------

#[test]
fn test_check_project_plugins_empty_project() {
    use aiki::plugins::project::derive_project_plugin_refs;

    let tmp = TempDir::new().unwrap();
    // No .aiki dir → no refs
    let refs = derive_project_plugin_refs(tmp.path());
    assert!(refs.is_empty());
}

#[test]
fn test_check_project_plugins_with_refs() {
    use aiki::plugins::project::derive_project_plugin_refs;

    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");
    fs::create_dir_all(&aiki_dir).unwrap();
    fs::write(
        aiki_dir.join("hooks.yaml"),
        "hook:\n  template: vendor/tool/scan\n",
    )
    .unwrap();

    let refs = derive_project_plugin_refs(tmp.path());
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].to_string(), "vendor/tool");
}

// ---------------------------------------------------------------------------
// Scanner — tasks/ directory (not templates/)
// ---------------------------------------------------------------------------

#[test]
fn test_scan_uses_tasks_dir_not_templates() {
    let tmp = TempDir::new().unwrap();
    let aiki_dir = tmp.path().join(".aiki");

    // Put a ref in the old templates/ dir — should NOT be found
    let old_dir = aiki_dir.join("templates");
    fs::create_dir_all(&old_dir).unwrap();
    fs::write(
        old_dir.join("review.md"),
        "{{> old/plugin/ref}}\n",
    )
    .unwrap();

    // Put a ref in the new tasks/ dir — should be found
    let new_dir = aiki_dir.join("tasks");
    fs::create_dir_all(&new_dir).unwrap();
    fs::write(
        new_dir.join("review.md"),
        "{{> new/plugin/ref}}\n",
    )
    .unwrap();

    let refs = derive_plugin_refs(&aiki_dir, None);
    let ref_strs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();

    assert!(ref_strs.contains(&"new/plugin".to_string()));
    assert!(!ref_strs.contains(&"old/plugin".to_string()));
    assert_eq!(refs.len(), 1);
}

// ---------------------------------------------------------------------------
// Hook resolver — installed plugin and repo-root plugin lookup
// ---------------------------------------------------------------------------

/// Installed-plugin lookup uses ~/.aiki/plugins/{ns}/{name}/hooks.yaml.
/// This requires manipulating HOME which is unreliable in parallel tests,
/// so we test the repo-root plugin path (step 4) as a proxy — both use
/// the same resolution logic in resolve_namespaced_flow().
/// The unit tests in hook_resolver.rs cover the full search order.

#[test]
fn test_hook_resolver_repo_root_plugin_lookup() {
    use aiki::flows::hook_resolver::HookResolver;

    let tmp = TempDir::new().unwrap();

    // Create minimal .aiki dir
    fs::create_dir_all(tmp.path().join(".aiki/hooks")).unwrap();

    // Create repo-root plugin: {project}/plugins/myns/myplugin/hooks.yaml
    let plugin_dir = tmp.path().join("plugins/myns/myplugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    let hooks_path = plugin_dir.join("hooks.yaml");
    fs::write(&hooks_path, "name: test\nversion: \"1\"\n").unwrap();

    let resolver = HookResolver::with_start_dir(tmp.path()).unwrap();
    let resolved = resolver.resolve("myns/myplugin").unwrap();

    assert_eq!(
        resolved.canonicalize().unwrap(),
        hooks_path.canonicalize().unwrap()
    );
}

#[test]
fn test_hook_resolver_priority_project_over_repo_root() {
    use aiki::flows::hook_resolver::HookResolver;

    let tmp = TempDir::new().unwrap();

    // Create project hook
    fs::create_dir_all(tmp.path().join(".aiki/hooks/myns")).unwrap();
    let project_path = tmp.path().join(".aiki/hooks/myns/myplugin.yml");
    fs::write(&project_path, "name: project\nversion: \"1\"\n").unwrap();

    // Create repo-root plugin
    let plugin_dir = tmp.path().join("plugins/myns/myplugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("hooks.yaml"),
        "name: repo-root\nversion: \"1\"\n",
    )
    .unwrap();

    let resolver = HookResolver::with_start_dir(tmp.path()).unwrap();
    let resolved = resolver.resolve("myns/myplugin").unwrap();

    // Project should win
    assert_eq!(
        resolved.canonicalize().unwrap(),
        project_path.canonicalize().unwrap()
    );
}

#[test]
fn test_hook_resolver_not_found_error() {
    use aiki::flows::hook_resolver::HookResolver;

    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join(".aiki/hooks")).unwrap();

    let resolver = HookResolver::with_start_dir(tmp.path()).unwrap();
    let result = resolver.resolve("nonexistent/plugin");
    assert!(result.is_err(), "Should return error for missing plugin");
}

// ---------------------------------------------------------------------------
// Template resolver — three-part ref, TemplateNotFound
// ---------------------------------------------------------------------------

#[test]
fn test_template_resolution_three_part_ref_uses_tasks_dir() {
    use aiki::tasks::templates::resolver::load_template;

    let tmp = TempDir::new().unwrap();
    let aiki_home = TempDir::new().unwrap();

    // Create project .aiki/tasks/ (empty)
    let project_templates = tmp.path().join(".aiki").join("tasks");
    fs::create_dir_all(&project_templates).unwrap();

    // Create plugin with template in tasks/ dir (not templates/)
    let plugins_base = aiki_home.path().join("plugins");
    let plugin_tasks_dir = plugins_base.join("ns").join("plug").join("tasks");
    fs::create_dir_all(&plugin_tasks_dir).unwrap();
    fs::create_dir_all(plugins_base.join("ns").join("plug").join(".git")).unwrap();
    fs::write(
        plugin_tasks_dir.join("mytpl.md"),
        "---\nname: Plugin Template\n---\n# From plugin tasks dir\n",
    )
    .unwrap();

    with_temp_aiki_home(aiki_home.path(), || {
        let result = load_template("ns/plug/mytpl", &project_templates);
        assert!(
            result.is_ok(),
            "Should resolve three-part ref from plugin tasks/ dir: {:?}",
            result
        );
    });
}

#[test]
fn test_template_not_found_for_missing_plugin_template() {
    use aiki::tasks::templates::resolver::load_template;

    let tmp = TempDir::new().unwrap();
    let aiki_home = TempDir::new().unwrap();

    let project_templates = tmp.path().join(".aiki").join("tasks");
    fs::create_dir_all(&project_templates).unwrap();

    with_temp_aiki_home(aiki_home.path(), || {
        let result = load_template("nonexist/noplugin/notemplate", &project_templates);
        assert!(result.is_err(), "Should return TemplateNotFound");
    });
}

#[test]
fn test_template_short_names_and_two_part_refs_still_work() {
    use aiki::tasks::templates::resolver::load_template;

    let tmp = TempDir::new().unwrap();
    let project_templates = tmp.path().join(".aiki").join("tasks");
    fs::create_dir_all(&project_templates).unwrap();

    // Create a simple template
    fs::write(
        project_templates.join("review.md"),
        "---\nname: Review\n---\n# Review template\n",
    )
    .unwrap();

    // Create a namespaced (two-part) template
    let ns_dir = project_templates.join("myns");
    fs::create_dir_all(&ns_dir).unwrap();
    fs::write(
        ns_dir.join("custom.md"),
        "---\nname: Custom\n---\n# Custom template\n",
    )
    .unwrap();

    // Short name should work
    let result = load_template("review", &project_templates);
    assert!(result.is_ok(), "Short name should work: {:?}", result);

    // Two-part ref should work
    let result = load_template("myns/custom", &project_templates);
    assert!(result.is_ok(), "Two-part ref should work: {:?}", result);
}

// ---------------------------------------------------------------------------
// Plugin manifest — integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_manifest_display_name_from_plugin_yaml() {
    use aiki::plugins::manifest::load_manifest;

    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("plugin.yaml"),
        "name: My Awesome Plugin\n",
    )
    .unwrap();

    let manifest = load_manifest(tmp.path()).unwrap();
    assert_eq!(manifest.name.as_deref(), Some("My Awesome Plugin"));
}

#[test]
fn test_manifest_missing_plugin_yaml_error() {
    use aiki::plugins::manifest::load_manifest;

    let tmp = TempDir::new().unwrap();
    // No plugin.yaml — should error
    let err = load_manifest(tmp.path()).unwrap_err();
    assert!(
        err.to_string().contains("Missing plugin.yaml"),
        "Should report missing plugin.yaml: {}",
        err
    );
}

#[test]
fn test_manifest_invalid_plugin_yaml_error() {
    use aiki::plugins::manifest::load_manifest;

    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("plugin.yaml"),
        "totally: not\nthe: right\nformat: at all\n",
    )
    .unwrap();

    let err = load_manifest(tmp.path()).unwrap_err();
    assert!(
        err.to_string().contains("Invalid plugin.yaml"),
        "Should report invalid plugin.yaml: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// Plugin install/remove — validates plugin.yaml
// ---------------------------------------------------------------------------

#[test]
fn test_plugin_remove_nonexistent_fails() {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::process::Command;

    let aiki_home = TempDir::new().unwrap();
    let workdir = TempDir::new().unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("aiki"));
    cmd.args(["plugin", "remove", "nonexist/plugin"])
        .current_dir(workdir.path())
        .env("AIKI_HOME", aiki_home.path());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not installed"));
}


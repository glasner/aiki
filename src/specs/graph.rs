//! SpecGraph — first-class spec management with O(1) reverse index
//!
//! The SpecGraph builds a reverse index from spec file paths to the tasks
//! that implement them. It unifies the duplicate `find_plan_for_spec()`
//! functions that existed in `plan.rs` and `build.rs`.

use std::path::Path;

use crate::tasks::graph::TaskGraph;
use crate::tasks::types::{FastHashMap, Task, TaskOutcome, TaskStatus};

use super::parser::{parse_spec_metadata, SpecMetadata};

/// Status of a spec in the lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStatus {
    /// No plan exists (or plan closed as wont_do)
    Draft,
    /// Plan exists and is open (not started)
    Planned,
    /// Plan is in_progress
    Implementing,
    /// Plan is closed (successfully)
    Implemented,
}

impl std::fmt::Display for SpecStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecStatus::Draft => write!(f, "draft"),
            SpecStatus::Planned => write!(f, "planned"),
            SpecStatus::Implementing => write!(f, "implementing"),
            SpecStatus::Implemented => write!(f, "implemented"),
        }
    }
}

/// A spec entry with parsed metadata and derived status.
#[derive(Debug, Clone)]
pub struct Spec {
    /// Canonical path key (e.g., "file:ops/now/foo.md")
    pub path: String,
    /// Parsed metadata from the markdown file
    pub metadata: SpecMetadata,
    /// Derived status based on implementing tasks
    pub status: SpecStatus,
}

/// SpecGraph: indexes specs and their implementing tasks.
///
/// Built from a `TaskGraph` and optionally from filesystem spec files.
/// Provides O(1) lookups for common spec queries.
pub struct SpecGraph {
    /// Reverse index: spec_path (normalized "file:..." URI) → implementing task IDs
    spec_to_tasks: FastHashMap<String, Vec<String>>,
    /// Spec metadata: spec_path → Spec
    specs: FastHashMap<String, Spec>,
}

impl SpecGraph {
    /// Build a SpecGraph from a TaskGraph.
    ///
    /// Scans the `implements` edges in the TaskGraph to build the reverse index.
    /// Does not scan the filesystem — call `with_filesystem_specs()` for that.
    pub fn build(task_graph: &TaskGraph) -> Self {
        let mut spec_to_tasks: FastHashMap<String, Vec<String>> = FastHashMap::default();

        // Build reverse index from implements edges.
        // The TaskGraph already materializes implements edges from both:
        // - Explicit LinkAdded events
        // - Backward-compat data.spec synthesis in graph.rs
        for (task_id, _task) in &task_graph.tasks {
            // Check if this task has an implements edge
            let targets = task_graph.edges.targets(task_id, "implements");
            for target in targets {
                if target.starts_with("file:") {
                    let entry = spec_to_tasks.entry(target.clone()).or_default();
                    if !entry.contains(task_id) {
                        entry.push(task_id.clone());
                    }
                }
            }
        }

        SpecGraph {
            spec_to_tasks,
            specs: FastHashMap::default(),
        }
    }

    /// Scan the filesystem for spec files and populate metadata.
    ///
    /// Recursively scans the given directories (relative to `cwd`) for `.md`
    /// files and parses their metadata. Merges with existing spec entries from
    /// the task graph.
    pub fn with_filesystem_specs(mut self, cwd: &Path, dirs: &[&str]) -> Self {
        for dir in dirs {
            let dir_path = cwd.join(dir);
            if !dir_path.is_dir() {
                continue;
            }
            self.scan_dir_recursive(cwd, &dir_path);
        }
        self
    }

    /// Recursively scan a directory for .md files and add them as specs.
    fn scan_dir_recursive(&mut self, cwd: &Path, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir_recursive(cwd, &path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Ok(rel) = path.strip_prefix(cwd) {
                    let spec_path = format!("file:{}", rel.display());
                    let metadata = parse_spec_metadata(&path);
                    self.specs.entry(spec_path.clone()).or_insert(Spec {
                        path: spec_path,
                        metadata,
                        status: SpecStatus::Draft,
                    });
                }
            }
        }
    }

    /// Infer and update status for all known specs.
    pub fn infer_statuses(mut self, task_graph: &TaskGraph) -> Self {
        let paths: Vec<String> = self
            .specs
            .keys()
            .chain(self.spec_to_tasks.keys())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for path in paths {
            let status = self.infer_status(&path, task_graph);
            if let Some(spec) = self.specs.get_mut(&path) {
                spec.status = status;
            }
        }
        self
    }

    /// Get all task IDs that implement a given spec.
    ///
    /// O(1) lookup via the reverse index.
    pub fn implementing_task_ids(&self, spec_path: &str) -> &[String] {
        let normalized = normalize_spec_path(spec_path);
        self.spec_to_tasks
            .get(&normalized)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all tasks that implement a given spec.
    pub fn implementing_tasks<'a>(
        &self,
        spec_path: &str,
        task_graph: &'a TaskGraph,
    ) -> Vec<&'a Task> {
        self.implementing_task_ids(spec_path)
            .iter()
            .filter_map(|id| task_graph.tasks.get(id))
            .collect()
    }

    /// Find the most recent valid plan for a spec.
    ///
    /// Filters out:
    /// - Planning tasks (type "plan") — ephemeral tasks that run the planning agent
    /// - Orchestrator tasks (type "orchestrator") — build tasks that coordinate execution
    ///
    /// Returns the most recently created task that implements the spec and
    /// is neither a planning task nor an orchestrator.
    pub fn find_plan_for_spec<'a>(
        &self,
        spec_path: &str,
        task_graph: &'a TaskGraph,
    ) -> Option<&'a Task> {
        let normalized = normalize_spec_path(spec_path);
        self.spec_to_tasks
            .get(&normalized)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| task_graph.tasks.get(id))
            .filter(|t| {
                t.task_type.as_deref() != Some("plan")
                    && t.task_type.as_deref() != Some("orchestrator")
            })
            .max_by_key(|t| t.created_at)
    }

    /// Find specs that have no implementing tasks.
    pub fn unimplemented(&self) -> Vec<&Spec> {
        self.specs
            .values()
            .filter(|spec| {
                self.spec_to_tasks
                    .get(&spec.path)
                    .map_or(true, |ids| ids.is_empty())
            })
            .collect()
    }

    /// Get a spec by its path.
    #[allow(dead_code)]
    pub fn get_spec(&self, spec_path: &str) -> Option<&Spec> {
        let normalized = normalize_spec_path(spec_path);
        self.specs.get(&normalized)
    }

    /// Get all known specs.
    #[allow(dead_code)]
    pub fn all_specs(&self) -> Vec<&Spec> {
        self.specs.values().collect()
    }

    /// Infer the status of a spec based on its draft flag and implementing tasks.
    fn infer_status(&self, spec_path: &str, task_graph: &TaskGraph) -> SpecStatus {
        // Check draft flag first — always Draft if explicitly marked
        if let Some(spec) = self.specs.get(spec_path) {
            if spec.metadata.draft {
                return SpecStatus::Draft;
            }
        }

        let plan = self.find_plan_for_spec(spec_path, task_graph);

        match plan {
            None => SpecStatus::Draft,
            Some(plan) => match plan.status {
                TaskStatus::Closed => {
                    if plan.closed_outcome == Some(TaskOutcome::WontDo) {
                        SpecStatus::Draft
                    } else {
                        SpecStatus::Implemented
                    }
                }
                TaskStatus::InProgress => SpecStatus::Implementing,
                _ => SpecStatus::Planned,
            },
        }
    }

    /// Check if a spec is a draft.
    pub fn is_draft(&self, spec_path: &str) -> bool {
        let normalized = normalize_spec_path(spec_path);
        self.specs
            .get(&normalized)
            .map_or(false, |spec| spec.metadata.draft)
    }
}

/// Normalize a spec path to the canonical "file:..." URI format.
///
/// Handles variations like `./ops/now/foo.md`, `file:./ops/now/foo.md`,
/// and `ops/now/foo.md` — all normalize to `file:ops/now/foo.md`.
pub fn normalize_spec_path(spec_path: &str) -> String {
    let path = spec_path
        .strip_prefix("file:")
        .unwrap_or(spec_path);

    // Strip leading "./" to normalize relative paths
    let path = path.strip_prefix("./").unwrap_or(path);

    format!("file:{}", path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::graph::{materialize_graph, EdgeStore};
    use crate::tasks::types::{TaskEvent, TaskPriority};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_task_graph(tasks: FastHashMap<String, Task>, edges: EdgeStore) -> TaskGraph {
        TaskGraph {
            tasks,
            edges,
            slug_index: FastHashMap::default(),
        }
    }

    fn make_task(id: &str, name: &str, status: TaskStatus) -> Task {
        Task {
            id: id.to_string(),
            name: name.to_string(),
            slug: None,
            task_type: None,
            status,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: None,
            summary: None,
            turn_started: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn make_created(id: &str, name: &str) -> TaskEvent {
        TaskEvent::Created {
            task_id: id.to_string(),
            name: name.to_string(),
            slug: None,
            task_type: None,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    fn make_link(from: &str, to: &str, kind: &str) -> TaskEvent {
        TaskEvent::LinkAdded {
            from: from.to_string(),
            to: to.to_string(),
            kind: kind.to_string(),
            timestamp: Utc::now(),
        }
    }

    // --- normalize_spec_path tests ---

    #[test]
    fn test_normalize_bare_path() {
        assert_eq!(
            normalize_spec_path("ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_normalize_dot_slash_prefix() {
        assert_eq!(
            normalize_spec_path("./ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_normalize_file_dot_slash_prefix() {
        assert_eq!(
            normalize_spec_path("file:./ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    #[test]
    fn test_normalize_already_prefixed() {
        assert_eq!(
            normalize_spec_path("file:ops/now/feature.md"),
            "file:ops/now/feature.md"
        );
    }

    // --- SpecGraph::build tests ---

    #[test]
    fn test_build_empty_graph() {
        let tg = make_task_graph(FastHashMap::default(), EdgeStore::new());
        let sg = SpecGraph::build(&tg);
        assert!(sg.implementing_task_ids("ops/now/feature.md").is_empty());
    }

    #[test]
    fn test_build_indexes_implements_edges() {
        let mut tasks = FastHashMap::default();
        tasks.insert("plan1".to_string(), make_task("plan1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        assert_eq!(
            sg.implementing_task_ids("ops/now/feature.md"),
            &["plan1"]
        );
        assert_eq!(
            sg.implementing_task_ids("file:ops/now/feature.md"),
            &["plan1"]
        );
    }

    #[test]
    fn test_build_multiple_implementors() {
        let mut tasks = FastHashMap::default();
        tasks.insert("p1".to_string(), make_task("p1", "Plan 1", TaskStatus::Closed));
        tasks.insert("p2".to_string(), make_task("p2", "Plan 2", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");
        edges.add("p2", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        let ids = sg.implementing_task_ids("ops/now/feature.md");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"p1".to_string()));
        assert!(ids.contains(&"p2".to_string()));
    }

    // --- find_plan_for_spec tests ---

    #[test]
    fn test_find_plan_none() {
        let tg = make_task_graph(FastHashMap::default(), EdgeStore::new());
        let sg = SpecGraph::build(&tg);
        assert!(sg.find_plan_for_spec("ops/now/feature.md", &tg).is_none());
    }

    #[test]
    fn test_find_plan_basic() {
        let mut tasks = FastHashMap::default();
        tasks.insert("plan1".to_string(), make_task("plan1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_excludes_planning_task() {
        let mut tasks = FastHashMap::default();

        let mut planning = make_task("planning1", "Planning task", TaskStatus::Closed);
        planning.task_type = Some("plan".to_string());
        tasks.insert("planning1".to_string(), planning);

        tasks.insert("plan1".to_string(), make_task("plan1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("planning1", "file:ops/now/feature.md", "implements");
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_excludes_orchestrator() {
        let mut tasks = FastHashMap::default();

        let mut orch = make_task("orch1", "Build task", TaskStatus::InProgress);
        orch.task_type = Some("orchestrator".to_string());
        tasks.insert("orch1".to_string(), orch);

        tasks.insert("plan1".to_string(), make_task("plan1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("orch1", "file:ops/now/feature.md", "implements");
        edges.add("plan1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_find_plan_returns_most_recent() {
        let mut tasks = FastHashMap::default();

        let mut old = make_task("old_plan", "Old Plan", TaskStatus::Closed);
        old.created_at = Utc::now() - chrono::Duration::hours(1);
        tasks.insert("old_plan".to_string(), old);

        let new = make_task("new_plan", "New Plan", TaskStatus::Open);
        tasks.insert("new_plan".to_string(), new);

        let mut edges = EdgeStore::new();
        edges.add("old_plan", "file:ops/now/feature.md", "implements");
        edges.add("new_plan", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert_eq!(result.unwrap().id, "new_plan");
    }

    // --- build from events (integration) ---

    #[test]
    fn test_build_from_events_with_data_spec() {
        // Older tasks use data.spec which gets synthesized into implements edges
        let mut data = HashMap::new();
        data.insert("spec".to_string(), "ops/now/feature.md".to_string());

        let events = vec![TaskEvent::Created {
            task_id: "plan1".to_string(),
            name: "Plan: Feature".to_string(),
            slug: None,
            task_type: None,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            working_copy: None,
            instructions: None,
            data,
            timestamp: Utc::now(),
        }];

        let tg = materialize_graph(&events);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    #[test]
    fn test_build_from_events_with_link() {
        let events = vec![
            make_created("plan1", "Plan: Feature"),
            make_link("plan1", "file:ops/now/feature.md", "implements"),
        ];

        let tg = materialize_graph(&events);
        let sg = SpecGraph::build(&tg);

        let result = sg.find_plan_for_spec("ops/now/feature.md", &tg);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "plan1");
    }

    // --- infer_status tests ---

    #[test]
    fn test_infer_status_draft_no_plan() {
        let tg = make_task_graph(FastHashMap::default(), EdgeStore::new());
        let sg = SpecGraph::build(&tg);
        assert_eq!(sg.infer_status("file:ops/now/feature.md", &tg), SpecStatus::Draft);
    }

    #[test]
    fn test_infer_status_planned() {
        let mut tasks = FastHashMap::default();
        tasks.insert("p1".to_string(), make_task("p1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);
        assert_eq!(
            sg.infer_status("file:ops/now/feature.md", &tg),
            SpecStatus::Planned
        );
    }

    #[test]
    fn test_infer_status_implementing() {
        let mut tasks = FastHashMap::default();
        tasks.insert(
            "p1".to_string(),
            make_task("p1", "Plan", TaskStatus::InProgress),
        );

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);
        assert_eq!(
            sg.infer_status("file:ops/now/feature.md", &tg),
            SpecStatus::Implementing
        );
    }

    #[test]
    fn test_infer_status_implemented() {
        let mut tasks = FastHashMap::default();
        let mut t = make_task("p1", "Plan", TaskStatus::Closed);
        t.closed_outcome = Some(TaskOutcome::Done);
        tasks.insert("p1".to_string(), t);

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);
        assert_eq!(
            sg.infer_status("file:ops/now/feature.md", &tg),
            SpecStatus::Implemented
        );
    }

    #[test]
    fn test_infer_status_draft_wont_do() {
        let mut tasks = FastHashMap::default();
        let mut t = make_task("p1", "Plan", TaskStatus::Closed);
        t.closed_outcome = Some(TaskOutcome::WontDo);
        tasks.insert("p1".to_string(), t);

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg);
        assert_eq!(
            sg.infer_status("file:ops/now/feature.md", &tg),
            SpecStatus::Draft
        );
    }

    #[test]
    fn test_infer_status_draft_overrides_plan() {
        // Even with an implementing task, a draft spec should stay Draft
        let dir = tempfile::TempDir::new().unwrap();
        let ops_dir = dir.path().join("ops/now");
        std::fs::create_dir_all(&ops_dir).unwrap();
        std::fs::write(
            ops_dir.join("feature.md"),
            "---\ndraft: true\n---\n\n# Feature\n\nDesc.\n",
        )
        .unwrap();

        let mut tasks = FastHashMap::default();
        tasks.insert(
            "p1".to_string(),
            make_task("p1", "Plan", TaskStatus::InProgress),
        );

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg)
            .with_filesystem_specs(dir.path(), &["ops/now"])
            .infer_statuses(&tg);

        let spec = sg.get_spec("file:ops/now/feature.md").unwrap();
        assert_eq!(spec.status, SpecStatus::Draft);
        assert!(sg.is_draft("ops/now/feature.md"));
    }

    // --- SpecStatus display ---

    #[test]
    fn test_spec_status_display() {
        assert_eq!(SpecStatus::Draft.to_string(), "draft");
        assert_eq!(SpecStatus::Planned.to_string(), "planned");
        assert_eq!(SpecStatus::Implementing.to_string(), "implementing");
        assert_eq!(SpecStatus::Implemented.to_string(), "implemented");
    }

    // --- unimplemented ---

    #[test]
    fn test_unimplemented_with_filesystem_specs() {
        let dir = tempfile::TempDir::new().unwrap();
        let ops_dir = dir.path().join("ops/now");
        std::fs::create_dir_all(&ops_dir).unwrap();
        std::fs::write(ops_dir.join("feature.md"), "# Feature\n\nDesc.\n").unwrap();
        std::fs::write(ops_dir.join("other.md"), "# Other\n\nOther desc.\n").unwrap();

        // Only feature.md has an implementing task
        let mut tasks = FastHashMap::default();
        tasks.insert("p1".to_string(), make_task("p1", "Plan", TaskStatus::Open));

        let mut edges = EdgeStore::new();
        edges.add("p1", "file:ops/now/feature.md", "implements");

        let tg = make_task_graph(tasks, edges);
        let sg = SpecGraph::build(&tg)
            .with_filesystem_specs(dir.path(), &["ops/now"]);

        let unimplemented = sg.unimplemented();
        assert_eq!(unimplemented.len(), 1);
        assert_eq!(unimplemented[0].path, "file:ops/now/other.md");
    }

    #[test]
    fn test_filesystem_specs_recursive() {
        let dir = tempfile::TempDir::new().unwrap();
        let ops_dir = dir.path().join("ops/now");
        let sub_dir = dir.path().join("ops/now/subdir");
        std::fs::create_dir_all(&sub_dir).unwrap();
        std::fs::write(ops_dir.join("top.md"), "# Top\n\nTop level.\n").unwrap();
        std::fs::write(sub_dir.join("nested.md"), "# Nested\n\nNested spec.\n").unwrap();

        let tg = make_task_graph(FastHashMap::default(), EdgeStore::new());
        let sg = SpecGraph::build(&tg)
            .with_filesystem_specs(dir.path(), &["ops/now"]);

        assert!(sg.get_spec("file:ops/now/top.md").is_some());
        assert!(sg.get_spec("file:ops/now/subdir/nested.md").is_some());
        assert_eq!(sg.all_specs().len(), 2);
    }
}

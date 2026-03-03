use crate::tui::widgets::lane_dag::DagLayout;

/// State of a workflow stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    Pending,
    Starting,
    Active,
    Done,
    Failed,
}

/// Status for subtask lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtaskStatus {
    Pending,
    Starting,
    Active,
    Done,
    Failed,
}

/// A subtask line in the epic tree or inside a stage.
#[derive(Debug, Clone)]
pub struct SubtaskLine {
    pub name: String,
    pub status: SubtaskStatus,
    pub agent: Option<String>,
    pub elapsed: Option<String>,
    pub error: Option<String>,
}

/// A child of a fix stage.
#[derive(Debug, Clone)]
pub enum FixChild {
    Subtask(SubtaskLine),
    ReviewFix {
        number: Option<u32>,
        state: StageState,
        result: Option<String>,
        agent: Option<String>,
        elapsed: Option<String>,
    },
}

/// A sub-stage within a group stage.
#[derive(Debug, Clone)]
pub struct SubStageView {
    pub name: String,
    pub state: StageState,
    pub progress: Option<String>,
    pub elapsed: Option<String>,
}

/// Children under a stage.
#[derive(Debug, Clone)]
pub enum StageChild {
    Subtask(SubtaskLine),
    Fix(FixChild),
}

/// A workflow stage.
#[derive(Debug, Clone)]
pub struct StageView {
    pub name: String,
    pub state: StageState,
    pub progress: Option<String>,
    pub elapsed: Option<String>,
    pub sub_stages: Vec<SubStageView>,
    pub children: Vec<StageChild>,
}

/// The epic header and subtask list.
#[derive(Debug, Clone)]
pub struct EpicView {
    pub short_id: String,
    pub name: String,
    pub subtasks: Vec<SubtaskLine>,
    pub collapsed: bool,
    pub collapsed_summary: Option<String>,
}

/// Top-level workflow view data model.
#[derive(Debug, Clone)]
pub struct WorkflowView {
    pub plan_path: String,
    pub epic: EpicView,
    pub stages: Vec<StageView>,
    pub lane_dag: Option<DagLayout>,
}

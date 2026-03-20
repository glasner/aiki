use crate::tui::widgets::lane_dag::DagLayout;

/// State of a workflow stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    Pending,
    Starting,
    Active,
    Done,
    Skipped,
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
    pub children: Vec<StageChild>,
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
    /// Repository folder name shown as `[name]` prefix in the path line.
    pub repo_name: String,
    pub plan_path: String,
    pub epic: EpicView,
    pub stages: Vec<StageView>,
    pub lane_dag: Option<DagLayout>,
}

// ── Chat data model ─────────────────────────────────────────────────

/// Pipeline stage — used for progressive dimming.
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub enum Stage {
    Plan,
    Build,
    Review,
    Fix,
    ReReview,
    Summary,
}

/// A pipeline rendered as a conversation.
#[derive(Debug, Clone)]
pub struct Chat {
    pub messages: Vec<Message>,
}

/// One line (or multi-line block) in the chat.
#[derive(Debug, Clone)]
pub struct Message {
    pub stage: Stage,
    pub kind: MessageKind,
    pub text: String,
    /// Right-aligned metadata: e.g. "1m54" or "42s  claude".
    pub meta: Option<String>,
    pub children: Vec<ChatChild>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    /// Green, past tense.
    Done,
    /// Yellow, present tense.
    Active,
    /// Dim, future tense.
    Pending,
    /// Yellow, review found issues.
    Attention,
    /// Red, failure.
    Error,
    /// Dim, plan created/edited/summary.
    Meta,
}

/// Children of a message.
#[derive(Debug, Clone)]
pub enum ChatChild {
    Subtask {
        name: String,
        status: MessageKind,
        elapsed: Option<String>,
        agent: Option<String>,
        error: Option<String>,
    },
    AgentBlock {
        task_name: String,
        footer: BlockFooter,
    },
    LaneBlock {
        subtasks: Vec<LaneSubtask>,
        footer: BlockFooter,
    },
    Issue {
        number: usize,
        title: String,
        location: Option<String>,
        description: Option<String>,
    },
}

/// A subtask within a LaneBlock.
#[derive(Debug, Clone)]
pub struct LaneSubtask {
    pub name: String,
    pub status: MessageKind,
    pub elapsed: Option<String>,
    pub error: Option<String>,
}

/// Footer line for active blocks.
#[derive(Debug, Clone)]
pub struct BlockFooter {
    pub agent: String,
    pub model: String,
    pub context_pct: u8,
    pub cost: f64,
    pub elapsed: Option<String>,
}

/// Status for subtask lines (used by epic_show).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtaskStatus {
    Pending,
    Starting,
    Active,
    Done,
    Failed,
}

/// A subtask line in the epic tree (used by epic_show).
#[derive(Debug, Clone)]
pub struct SubtaskLine {
    pub name: String,
    pub status: SubtaskStatus,
    pub agent: Option<String>,
    pub elapsed: Option<String>,
    pub error: Option<String>,
}

/// The epic header and subtask list (used by epic_show).
#[derive(Debug, Clone)]
pub struct EpicView {
    pub short_id: String,
    pub name: String,
    pub subtasks: Vec<SubtaskLine>,
    pub collapsed: bool,
    pub collapsed_summary: Option<String>,
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
    /// Green text, no symbol. Used for the final summary line.
    Summary,
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

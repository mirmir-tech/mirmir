use libmir::ProgressStage;

use crate::catalog::TransferPhase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityKind {
    Pull,
    Generate,
    Load,
    Unload,
    Restore,
    Recovery,
    Remove,
}

impl ActivityKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pull => "pull",
            Self::Generate => "generate",
            Self::Load => "load",
            Self::Unload => "unload",
            Self::Restore => "restore",
            Self::Recovery => "recovery",
            Self::Remove => "remove",
        }
    }

    #[must_use]
    pub const fn changes_models(self) -> bool {
        matches!(self, Self::Pull | Self::Load | Self::Unload | Self::Restore | Self::Remove)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityState {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    NotFound,
}

impl ActivityState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::NotFound => "not_found",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStage {
    State(ActivityState),
    Resolving,
    CheckingMemory,
    Runtime(ProgressStage),
    Transfer(TransferPhase),
}

impl ActivityStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::State(state) => state.as_str(),
            Self::Resolving => "resolving",
            Self::CheckingMemory => "checking_memory",
            Self::Runtime(stage) => stage.as_str(),
            Self::Transfer(phase) => phase.as_str(),
        }
    }
}

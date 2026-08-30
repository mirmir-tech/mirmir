use libmir::ProgressStage;

use crate::application::ports::TransferPhase;

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
pub enum ActivityStage {
    Queued,
    Starting,
    Resolving,
    CheckingMemory,
    Runtime(ProgressStage),
    Transfer(TransferPhase),
}

impl ActivityStage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Starting => "running",
            Self::Resolving => "resolving",
            Self::CheckingMemory => "checking_memory",
            Self::Runtime(stage) => stage.as_str(),
            Self::Transfer(phase) => phase.as_str(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityOutcome {
    Completed,
    Failed,
    Cancelled,
}

impl ActivityOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityProgress {
    current: u64,
    total: Option<u64>,
}

impl ActivityProgress {
    #[must_use]
    pub const fn new(current: u64, total: Option<u64>) -> Self {
        Self { current, total }
    }

    #[must_use]
    pub const fn current(self) -> u64 {
        self.current
    }

    #[must_use]
    pub const fn total(self) -> Option<u64> {
        self.total
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityStatus {
    Queued,
    Running {
        stage: ActivityStage,
        progress: Option<ActivityProgress>,
    },
    Cancelling {
        stage: ActivityStage,
        progress: Option<ActivityProgress>,
    },
    Finished(ActivityOutcome),
}

impl ActivityStatus {
    #[must_use]
    pub const fn state_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running { .. } => "running",
            Self::Cancelling { .. } => "cancelling",
            Self::Finished(outcome) => outcome.as_str(),
        }
    }

    #[must_use]
    pub const fn stage_str(self) -> &'static str {
        match self {
            Self::Queued => ActivityStage::Queued.as_str(),
            Self::Running { stage, .. } | Self::Cancelling { stage, .. } => stage.as_str(),
            Self::Finished(outcome) => outcome.as_str(),
        }
    }

    #[must_use]
    pub const fn progress(self) -> Option<ActivityProgress> {
        match self {
            Self::Running { progress, .. } | Self::Cancelling { progress, .. } => progress,
            Self::Queued | Self::Finished(_) => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Finished(_))
    }
}

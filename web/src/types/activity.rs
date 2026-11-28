use serde::Deserialize;

/// Backend or overall compatibility status parsed once at JSON ingress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    Supported,
    Partial,
    Unsupported,
    #[default]
    #[serde(other)]
    Unknown,
}

impl Compatibility {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Pull,
    Generate,
    Load,
    Unload,
    Restore,
    Recovery,
    Remove,
    #[default]
    #[serde(other)]
    Unknown,
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
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn changes_models(self) -> bool {
        matches!(self, Self::Load | Self::Restore | Self::Unload | Self::Pull | Self::Remove)
    }

    #[must_use]
    pub const fn operation_state(self) -> &'static str {
        match self {
            Self::Pull => "downloading",
            Self::Unload => "unloading",
            Self::Remove => "removing",
            _ => "loading",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    #[default]
    #[serde(other)]
    Unknown,
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
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Cancelling)
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct Activity {
    pub operation_id: String,
    #[serde(default)]
    pub kind: ActivityKind,
    pub target: String,
    #[serde(default)]
    pub state: ActivityState,
    pub detail: String,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

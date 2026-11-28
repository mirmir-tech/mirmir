//! Typed TUI local-model and activity rows parsed once at gRPC ingress.

use super::ModelFeatures;
use crate::{
    application::{ActivityKind, LocalModelState},
    rpc::proto,
};

/// A local model's state inside the TUI, combining server states and local
/// optimistic lifecycle transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalRowState {
    Server(LocalModelState),
    Unloading,
    Removing,
    Partial,
    Failed,
    Unknown,
}

impl LocalRowState {
    #[must_use]
    pub fn from_protocol(value: &str) -> Self {
        if let Some(state) = LocalModelState::from_protocol(value) {
            return Self::Server(state);
        }
        match value {
            "unloading" => Self::Unloading,
            "removing" => Self::Removing,
            "partial" => Self::Partial,
            "failed" => Self::Failed,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server(state) => state.as_str(),
            Self::Unloading => "unloading",
            Self::Removing => "removing",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Server(LocalModelState::Ready) => "READY",
            Self::Server(LocalModelState::Loading) => "LOADING",
            Self::Server(LocalModelState::Available) => "AVAILABLE",
            Self::Server(LocalModelState::Missing) => "MISSING",
            Self::Server(LocalModelState::Paused) => "PAUSED",
            Self::Server(LocalModelState::Error) => "ERROR",
            Self::Unloading => "UNLOADING",
            Self::Removing => "REMOVING",
            Self::Partial => "PARTIAL",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }

    #[must_use]
    pub const fn on_disk(self) -> bool {
        matches!(self, Self::Server(state) if state.on_disk())
    }

    #[must_use]
    pub const fn is_ready(self) -> bool {
        matches!(self, Self::Server(LocalModelState::Ready))
    }

    #[must_use]
    pub const fn is_missing(self) -> bool {
        matches!(self, Self::Server(LocalModelState::Missing))
    }

    #[must_use]
    pub const fn is_resumable(self) -> bool {
        matches!(self, Self::Server(LocalModelState::Paused) | Self::Partial | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRow {
    pub id: String,
    pub repo_id: String,
    pub revision: String,
    pub commit: String,
    pub path: String,
    pub state: LocalRowState,
    pub selector: String,
    pub managed: bool,
    pub loadable: bool,
    pub load_unavailable_reason: String,
    pub library: String,
    pub ecosystem: String,
    pub encoding: Option<String>,
    pub size_bytes: u64,
    pub features: ModelFeatures,
}

impl From<proto::LocalModelInfo> for LocalRow {
    fn from(model: proto::LocalModelInfo) -> Self {
        Self {
            id: model.id,
            repo_id: model.repo_id,
            revision: model.revision,
            commit: model.commit,
            path: model.path,
            state: LocalRowState::from_protocol(&model.state),
            selector: model.selector,
            managed: model.managed,
            loadable: model.loadable,
            load_unavailable_reason: model.load_unavailable_reason,
            library: model.library,
            ecosystem: model.ecosystem,
            encoding: model.encoding,
            size_bytes: model.size_bytes,
            features: ModelFeatures {
                tool_use: model.tool_use,
                thinking: model.thinking,
                vision: model.vision,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityRowState {
    Queued,
    Running,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

impl ActivityRowState {
    #[must_use]
    pub fn from_protocol(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "cancelling" => Self::Cancelling,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Cancelling)
    }

    #[must_use]
    pub const fn is_cancellable_state(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    pub operation_id: String,
    pub kind: Option<ActivityKind>,
    pub kind_label: String,
    pub target: String,
    pub state: ActivityRowState,
    pub stage: String,
    pub updated_at_unix_ms: u64,
    pub cancellable: bool,
}

impl From<proto::ActivityEvent> for ActivityRow {
    fn from(event: proto::ActivityEvent) -> Self {
        Self {
            operation_id: event.operation_id,
            kind: ActivityKind::from_protocol(&event.kind),
            kind_label: event.kind,
            target: event.target,
            state: ActivityRowState::from_protocol(&event.state),
            stage: event.stage,
            updated_at_unix_ms: event.updated_at_unix_ms,
            cancellable: event.cancellable,
        }
    }
}

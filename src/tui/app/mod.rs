mod actions;
mod activity;
mod chat;
mod configuration;
mod events;
mod load;
mod models;
mod navigation;
mod removal;
mod restore;
mod telemetry;
mod transfer;
mod viewport;

use std::collections::VecDeque;

use chat::settings::ChatSettingsEvent;
pub use chat::{
    ChatLiveMetrics, ChatParameters, ChatSettingsDialog, ChatSettingsStatus, ChatStatus, Message,
};
pub use configuration::{ConfigurationEdit, ConfigurationTarget};
pub use load::{LoadDialog, LoadStatus};
#[cfg(test)]
pub use load::{LoadTarget, RestorePosition};
pub use navigation::{NavigationState, WORKSPACE_PREFIX, WORKSPACE_TABS};
pub use removal::RemoveDialog;
pub(super) use telemetry::RefreshSnapshot;
pub use telemetry::TelemetryPoint;
use tokio::sync::mpsc;
pub use viewport::ListView;

use crate::{media::AttachedImage, rpc::proto};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Models,
    Chat,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogFilter {
    Compatible,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigurationView {
    Table,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningView {
    Collapsed,
    Expanded,
}

impl ReasoningView {
    pub const fn expanded(self) -> bool {
        matches!(self, Self::Expanded)
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Collapsed => Self::Expanded,
            Self::Expanded => Self::Collapsed,
        }
    }
}

pub struct App {
    pub screen: Screen,
    pub navigation: NavigationState,
    pub help_open: bool,
    pub list_view: Option<ListView>,
    pub animation_tick: u64,
    pub server_reused: bool,
    pub server_version: String,
    pub protocol_version: String,
    pub models: Vec<proto::ModelInfo>,
    pub local_models: Vec<proto::LocalModelInfo>,
    pub local_selected: usize,
    pub last_error: Option<String>,
    pub search_query: String,
    pub editing_search: bool,
    pub catalog: Vec<proto::CatalogModel>,
    pub catalog_selected: usize,
    pub catalog_filter: CatalogFilter,
    pub catalog_error: Option<String>,
    pub catalog_next_cursor: Option<String>,
    pub memory_source: String,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub transfer_repo: Option<String>,
    pub transfer_phase: Option<String>,
    pub transfer_downloaded_bytes: u64,
    pub transfer_total_bytes: Option<u64>,
    pub action_message: Option<String>,
    pub remove_dialog: Option<RemoveDialog>,
    pub load_dialog: Option<LoadDialog>,
    pub chat_input: String,
    pub chat_image: Option<AttachedImage>,
    pub chat_scroll: usize,
    pub chat_reasoning: ReasoningView,
    pub chat_model_index: usize,
    pub chat_messages: Vec<Message>,
    pub chat_metrics: Option<proto::Completion>,
    pub chat_live_metrics: Option<ChatLiveMetrics>,
    pub chat_error: Option<String>,
    pub chat_status: ChatStatus,
    pub chat_operation_id: Option<String>,
    pub chat_parameters: Option<ChatParameters>,
    pub chat_settings_dialog: Option<ChatSettingsDialog>,
    pub activities: Vec<proto::ActivityEvent>,
    pub activity_selected: usize,
    pub activity_error: Option<String>,
    pub telemetry: Option<proto::TelemetrySnapshot>,
    pub configuration: Option<proto::ConfigurationSnapshot>,
    pub configuration_selected: usize,
    pub configuration_view: ConfigurationView,
    pub configuration_edit: Option<ConfigurationEdit>,
    pub configuration_message: Option<String>,
    pub telemetry_history: VecDeque<TelemetryPoint>,
    restore_queue: VecDeque<String>,
    restore_total: usize,
    restore_completed: usize,
    transfer_tx: mpsc::Sender<Result<proto::ModelTransferEvent, String>>,
    transfer_rx: mpsc::Receiver<Result<proto::ModelTransferEvent, String>>,
    catalog_search_rx: Option<mpsc::Receiver<models::CatalogSearchEvent>>,
    removal_rx: Option<mpsc::Receiver<Result<proto::RemoveModelResponse, String>>>,
    pending_removal: Option<RemoveDialog>,
    lifecycle_rx: Option<mpsc::Receiver<Result<proto::ModelLifecycleEvent, String>>>,
    settings_rx: Option<mpsc::Receiver<Result<proto::InspectModelResponse, String>>>,
    chat_rx: Option<mpsc::Receiver<Result<proto::GenerateEvent, String>>>,
    chat_settings_rx: Option<mpsc::Receiver<Result<ChatSettingsEvent, String>>>,
    activity_rx: Option<mpsc::Receiver<Result<proto::ActivityEvent, String>>>,
    activity_task: Option<activity::ActivityTask>,
}

impl App {
    pub fn new(server_reused: bool) -> Self {
        let (transfer_tx, transfer_rx) = mpsc::channel(256);
        Self {
            screen: Screen::Dashboard,
            navigation: NavigationState::default(),
            help_open: false,
            list_view: None,
            animation_tick: 0,
            server_reused,
            server_version: "connecting".to_owned(),
            protocol_version: "-".to_owned(),
            models: Vec::new(),
            local_models: Vec::new(),
            local_selected: 0,
            last_error: None,
            search_query: String::new(),
            editing_search: false,
            catalog: Vec::new(),
            catalog_selected: 0,
            catalog_filter: CatalogFilter::Compatible,
            catalog_error: None,
            catalog_next_cursor: None,
            memory_source: "not sampled".to_owned(),
            total_memory_bytes: None,
            available_memory_bytes: None,
            transfer_repo: None,
            transfer_phase: None,
            transfer_downloaded_bytes: 0,
            transfer_total_bytes: None,
            action_message: None,
            remove_dialog: None,
            load_dialog: None,
            chat_input: String::new(),
            chat_image: None,
            chat_scroll: 0,
            chat_reasoning: ReasoningView::Collapsed,
            chat_model_index: 0,
            chat_messages: Vec::new(),
            chat_metrics: None,
            chat_live_metrics: None,
            chat_error: None,
            chat_status: ChatStatus::Idle,
            chat_operation_id: None,
            chat_parameters: None,
            chat_settings_dialog: None,
            activities: Vec::new(),
            activity_selected: 0,
            activity_error: None,
            telemetry: None,
            configuration: None,
            configuration_selected: 0,
            configuration_view: ConfigurationView::Table,
            configuration_edit: None,
            configuration_message: None,
            telemetry_history: VecDeque::new(),
            restore_queue: VecDeque::new(),
            restore_total: 0,
            restore_completed: 0,
            transfer_tx,
            transfer_rx,
            catalog_search_rx: None,
            removal_rx: None,
            pending_removal: None,
            lifecycle_rx: None,
            settings_rx: None,
            chat_rx: None,
            chat_settings_rx: None,
            activity_rx: None,
            activity_task: None,
        }
    }
}

use super::{App, ChatStatus};
use crate::rpc::proto;

#[derive(Debug, Clone)]
pub struct ChatLiveMetrics {
    pub stage: String,
    pub elapsed_ms: f64,
    pub ttft_ms: Option<f64>,
    pub ttft_pending_ms: Option<f64>,
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl Default for ChatLiveMetrics {
    fn default() -> Self {
        Self {
            stage: "starting".to_owned(),
            elapsed_ms: 0.0,
            ttft_ms: None,
            ttft_pending_ms: None,
            prefill_tokens_per_second: None,
            decode_tokens_per_second: None,
            prompt_tokens: 0,
            completion_tokens: 0,
        }
    }
}

impl App {
    pub(super) fn update_chat_telemetry(&mut self, telemetry: &proto::TelemetrySnapshot) {
        if self.chat_status != ChatStatus::Generating || telemetry.active_requests == 0 {
            return;
        }
        let metrics = self.chat_live_metrics.get_or_insert_default();
        metrics.stage.clone_from(&telemetry.active_stage);
        metrics.elapsed_ms = telemetry.active_elapsed_ms;
        metrics.ttft_pending_ms = telemetry.current_ttft_ms;
        if telemetry.active_completion_tokens > 0 {
            metrics.ttft_ms = telemetry.current_ttft_ms;
        }
        if let Some(rate) = telemetry.current_prefill_tokens_per_second {
            metrics.prefill_tokens_per_second = Some(rate);
        }
        if let Some(rate) = telemetry.current_decode_tokens_per_second {
            metrics.decode_tokens_per_second = Some(rate);
        }
        metrics.prompt_tokens = telemetry.active_prompt_tokens;
        metrics.completion_tokens = telemetry.active_completion_tokens;
    }
}

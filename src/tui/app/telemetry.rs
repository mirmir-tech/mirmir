use std::collections::VecDeque;

use super::App;
use crate::rpc::{Client, PROTOCOL_VERSION, proto};

const HISTORY_LENGTH: u32 = 60;

impl App {
    pub async fn refresh(&mut self, client: &mut Client) {
        let health = client.health(proto::HealthRequest {}).await;
        let models = client.list_models(proto::ListModelsRequest {}).await;
        let local = client.list_local_models(proto::ListLocalModelsRequest {}).await;
        let telemetry = client.telemetry(proto::TelemetryRequest {}).await;
        let history = client
            .telemetry_history(proto::TelemetryHistoryRequest { limit: HISTORY_LENGTH })
            .await;
        let configuration = client.get_configuration(proto::GetConfigurationRequest {}).await;
        match (health, models, local, telemetry, history, configuration) {
            (Ok(health), Ok(models), Ok(local), Ok(telemetry), Ok(history), Ok(configuration)) => {
                let health = health.into_inner();
                if health.protocol_version != PROTOCOL_VERSION {
                    self.last_error = Some(format!(
                        "server protocol {} is incompatible with client protocol {PROTOCOL_VERSION}",
                        health.protocol_version
                    ));
                    return;
                }
                self.server_version = health.server_version;
                self.protocol_version = health.protocol_version;
                self.models = models.into_inner().models;
                self.local_models = local.into_inner().models;
                self.local_selected =
                    self.local_selected.min(self.local_model_count().saturating_sub(1));
                self.apply_history(&history.into_inner());
                self.apply_telemetry(telemetry.into_inner());
                self.configuration = Some(configuration.into_inner());
                self.last_error = None;
            },
            (Err(error), _, _, _, _, _)
            | (_, Err(error), _, _, _, _)
            | (_, _, Err(error), _, _, _)
            | (_, _, _, Err(error), _, _)
            | (_, _, _, _, Err(error), _)
            | (_, _, _, _, _, Err(error)) => self.last_error = Some(error.to_string()),
        }
    }

    pub async fn refresh_chat_telemetry(&mut self, client: &mut Client) {
        match client.telemetry(proto::TelemetryRequest {}).await {
            Ok(telemetry) => self.apply_telemetry(telemetry.into_inner()),
            Err(error) => self.last_error = Some(error.to_string()),
        }
    }

    fn apply_telemetry(&mut self, telemetry: proto::TelemetrySnapshot) {
        self.update_chat_telemetry(&telemetry);
        self.telemetry = Some(telemetry);
    }

    fn apply_history(&mut self, history: &proto::TelemetryHistoryResponse) {
        self.throughput_history = history
            .samples
            .iter()
            .map(|sample| rate_sample(sample.e2e_tokens_per_second))
            .collect::<VecDeque<_>>();
        self.memory_history = history
            .samples
            .iter()
            .map(|sample| percentage(sample.memory_total_bytes, sample.memory_available_bytes))
            .collect::<VecDeque<_>>();
    }
}

fn rate_sample(value: Option<f64>) -> u64 {
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .and_then(|value| format!("{value:.0}").parse().ok())
        .unwrap_or(0)
}

fn percentage(total: Option<u64>, available: Option<u64>) -> u64 {
    match (total, available) {
        (Some(total), Some(available)) if total > 0 => {
            let used = total.saturating_sub(available);
            u64::try_from(u128::from(used) * 100 / u128::from(total)).unwrap_or(100)
        },
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_bounded_memory_percentage() {
        assert_eq!(percentage(Some(100), Some(25)), 75);
        assert_eq!(percentage(Some(100), Some(120)), 0);
        assert_eq!(percentage(None, None), 0);
    }

    #[test]
    fn replaces_local_charts_with_server_history() {
        let mut app = App::new(true);
        app.throughput_history.push_back(999);
        app.apply_history(&proto::TelemetryHistoryResponse {
            samples: vec![proto::TelemetryHistorySample {
                e2e_tokens_per_second: Some(24.4),
                memory_total_bytes: Some(100),
                memory_available_bytes: Some(25),
                ..Default::default()
            }],
            retention_limit: 900,
            sampling_interval_ms: 1_000,
        });
        assert_eq!(app.throughput_history.iter().copied().collect::<Vec<_>>(), [24]);
        assert_eq!(app.memory_history.iter().copied().collect::<Vec<_>>(), [75]);
    }
}

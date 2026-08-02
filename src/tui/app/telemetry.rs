use std::collections::VecDeque;

use super::{App, InitialRefresh};
use crate::rpc::{Client, PROTOCOL_VERSION, proto};

const HISTORY_LENGTH: u32 = 900;

#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetryPoint {
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub memory_percent: Option<f64>,
    pub gpu_percent: Option<f64>,
    pub temperature_celsius: Option<f64>,
    pub power_watts: Option<f64>,
    pub power_limit_watts: Option<f64>,
}

pub(in crate::tui) struct RefreshSnapshot {
    health: proto::HealthResponse,
    models: proto::ListModelsResponse,
    local: proto::ListLocalModelsResponse,
    telemetry: proto::TelemetrySnapshot,
    history: proto::TelemetryHistoryResponse,
    configuration: proto::ConfigurationSnapshot,
}

impl App {
    pub(in crate::tui) async fn fetch_refresh(
        mut client: Client,
    ) -> Result<RefreshSnapshot, String> {
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
                Ok(RefreshSnapshot {
                    health: health.into_inner(),
                    models: models.into_inner(),
                    local: local.into_inner(),
                    telemetry: telemetry.into_inner(),
                    history: history.into_inner(),
                    configuration: configuration.into_inner(),
                })
            },
            (Err(error), _, _, _, _, _)
            | (_, Err(error), _, _, _, _)
            | (_, _, Err(error), _, _, _)
            | (_, _, _, Err(error), _, _)
            | (_, _, _, _, Err(error), _)
            | (_, _, _, _, _, Err(error)) => Err(error.to_string()),
        }
    }

    pub(in crate::tui) fn apply_refresh(&mut self, result: Result<RefreshSnapshot, String>) {
        self.initial_refresh = InitialRefresh::Complete;
        let snapshot = match result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.last_error = Some(error);
                return;
            },
        };
        if snapshot.health.protocol_version != PROTOCOL_VERSION {
            self.last_error = Some(format!(
                "server protocol {} is incompatible with client protocol {PROTOCOL_VERSION}",
                snapshot.health.protocol_version
            ));
            return;
        }
        self.server_version = snapshot.health.server_version;
        self.protocol_version = snapshot.health.protocol_version;
        self.models = snapshot.models.models;
        self.local_models = snapshot.local.models;
        self.local_selected = self.local_selected.min(self.local_model_count().saturating_sub(1));
        self.apply_history(&snapshot.history);
        self.apply_telemetry(snapshot.telemetry);
        self.configuration = Some(snapshot.configuration);
        self.last_error = None;
    }

    pub(in crate::tui) fn apply_chat_telemetry(
        &mut self,
        result: Result<proto::TelemetrySnapshot, String>,
    ) {
        match result {
            Ok(telemetry) => self.apply_telemetry(telemetry),
            Err(error) => self.last_error = Some(error),
        }
    }

    fn apply_telemetry(&mut self, telemetry: proto::TelemetrySnapshot) {
        self.update_chat_telemetry(&telemetry);
        self.telemetry = Some(telemetry);
    }

    fn apply_history(&mut self, history: &proto::TelemetryHistoryResponse) {
        self.telemetry_history = history
            .samples
            .iter()
            .map(|sample| TelemetryPoint {
                memory_total_bytes: sample.memory_total_bytes,
                memory_used_bytes: used_memory(
                    sample.memory_total_bytes,
                    sample.memory_available_bytes,
                ),
                memory_percent: percentage(
                    sample.memory_total_bytes,
                    sample.memory_available_bytes,
                ),
                gpu_percent: sample.gpu_utilization_percent,
                temperature_celsius: sample.device_temperature_celsius,
                power_watts: sample.device_power_watts,
                power_limit_watts: sample.device_power_limit_watts,
            })
            .collect::<VecDeque<_>>();
    }
}

fn used_memory(total: Option<u64>, available: Option<u64>) -> Option<u64> {
    total.zip(available).map(|(total, available)| total.saturating_sub(available))
}

fn percentage(total: Option<u64>, available: Option<u64>) -> Option<f64> {
    match (total, available) {
        (Some(total), Some(available)) if total > 0 => {
            let used = total.saturating_sub(available);
            let basis_points =
                u32::try_from(u128::from(used) * 10_000 / u128::from(total)).unwrap_or(10_000);
            Some(f64::from(basis_points) / 100.0)
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_bounded_memory_percentage() {
        assert_eq!(percentage(Some(100), Some(25)), Some(75.0));
        assert_eq!(percentage(Some(100), Some(120)), Some(0.0));
        assert_eq!(percentage(None, None), None);
    }

    #[test]
    fn replaces_local_charts_with_server_history() {
        let mut app = App::new(true);
        app.telemetry_history.push_back(TelemetryPoint {
            memory_percent: Some(99.0),
            ..Default::default()
        });
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
        let points = app.telemetry_history.iter().copied().collect::<Vec<_>>();
        assert_eq!(points[0].memory_percent, Some(75.0));
        assert_eq!(points[0].memory_used_bytes, Some(75));
    }

    #[test]
    fn failed_initial_refresh_removes_the_loading_overlay() {
        let mut app = App::new(true);
        app.apply_refresh(Err("runtime unavailable".to_owned()));
        assert_eq!(app.initial_refresh, InitialRefresh::Complete);
        assert_eq!(app.last_error.as_deref(), Some("runtime unavailable"));
    }
}

use super::{
    super::app::{App, Screen, TelemetryPoint},
    fixtures::{rendered, telemetry},
};

#[test]
fn renders_identity_dashboard_at_minimum_size() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    app.telemetry = Some(telemetry());
    app.telemetry_history.extend([
        TelemetryPoint {
            memory_total_bytes: Some(64 * 1024 * 1024 * 1024),
            memory_used_bytes: Some(26 * 1024 * 1024 * 1024),
            memory_percent: Some(40.0),
            gpu_percent: Some(20.0),
            temperature_celsius: Some(58.0),
            power_watts: Some(22.0),
            power_limit_watts: Some(48.0),
        },
        TelemetryPoint {
            memory_total_bytes: Some(64 * 1024 * 1024 * 1024),
            memory_used_bytes: Some(27 * 1024 * 1024 * 1024),
            memory_percent: Some(42.0),
            gpu_percent: Some(25.0),
            temperature_celsius: Some(61.0),
            power_watts: Some(26.0),
            power_limit_watts: Some(48.0),
        },
    ]);
    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("MiRMiR"));
    assert!(text.contains("PREFILL"));
    assert!(text.contains("DECODE"));
    assert!(text.contains("TTFT"));
    assert!(text.contains("MEMORY"));
    assert!(text.contains("GiB"));
    assert!(text.contains("GPU"));
    assert!(text.contains("TEMPERATURE"));
    assert!(text.contains("POWER"));
    assert!(text.contains("HEALTHY"));
    assert!(!text.contains("LOCAL OWNER"));
    assert!(!text.contains("ATTACHED"));
    assert!(!text.contains("gRPC"));
    assert!(text.contains("Dashboard"));
    assert!(text.contains("Models"));
    assert!(text.contains("Chat"));
    assert!(text.contains("Settings"));
    assert!(!text.contains("[F1]"));
    assert!(!text.contains("Ctrl"));
    assert!(text.contains("? help"));
    Ok(())
}

#[test]
fn renders_activity_and_cancellation_capability() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Dashboard;
    app.activities.push(crate::rpc::proto::ActivityEvent {
        operation_id: "op-7".to_owned(),
        kind: "generate".to_owned(),
        target: "Qwen--Test".to_owned(),
        state: "running".to_owned(),
        stage: "decode".to_owned(),
        detail: "streaming tokens".to_owned(),
        started_at_unix_ms: 1,
        updated_at_unix_ms: 2,
        cancellable: true,
        current: None,
        total: None,
    });
    let text = rendered(&mut app, 110, 28)?;
    assert!(text.contains("RECENT ACTIVITY"));
    assert!(text.contains("Qwen--Test"));
    assert!(text.contains("decode"));
    Ok(())
}

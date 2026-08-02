use super::{App, rendered};
use crate::rpc::proto::ActivityEvent;

#[test]
fn automatic_restoration_is_reported_without_blocking_dashboard()
-> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    app.activities.push(ActivityEvent {
        operation_id: "restore-test".to_owned(),
        kind: "restore".to_owned(),
        target: "Qwen--Saved".to_owned(),
        state: "running".to_owned(),
        stage: "loading".to_owned(),
        detail: "materializing weight shard 1/4".to_owned(),
        current: Some(25),
        total: Some(100),
        ..Default::default()
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("PREFILL"));
    assert!(text.contains("RESTORE"));
    assert!(text.contains("Qwen--Saved"));
    assert!(!text.contains("LOADING MODEL"));
    Ok(())
}

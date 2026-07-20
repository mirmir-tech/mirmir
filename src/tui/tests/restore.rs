use super::{App, Screen, rendered};

#[test]
fn renders_saved_model_restoration_progress() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    app.screen = Screen::Models;
    app.load_dialog = Some(super::super::app::LoadDialog {
        target: super::super::app::LoadTarget {
            selector: "Qwen--Saved".to_owned(),
            config_id: "Qwen--Saved".to_owned(),
            repo_id: "Qwen/Saved".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
        },
        task: "generation".to_owned(),
        capabilities: None,
        status: super::super::app::LoadStatus::Loading,
        fields: std::array::from_fn(|_| String::new()),
        selected: 0,
        has_mirmir_overrides: true,
        memory: None,
        force: false,
        progress: Some(crate::rpc::proto::ModelLifecycleEvent {
            operation_id: "op-test".to_owned(),
            selector: "Qwen--Saved".to_owned(),
            phase: "loading".to_owned(),
            current: 25,
            total: Some(100),
            unit: "byte".to_owned(),
            detail: "materializing weight shard 1/4".to_owned(),
            model: None,
        }),
        error: None,
        restore: Some(super::super::app::RestorePosition { current: 2, total: 3 }),
    });

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("RESTORING MODELS 2/3"));
    assert!(text.contains("saved active model configuration"));
    assert!(text.contains("weights 25%"));
    assert!(text.contains("materializing weight shard 1/4"));
    Ok(())
}

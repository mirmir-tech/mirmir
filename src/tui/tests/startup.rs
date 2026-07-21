use super::{App, fixtures::rendered_starting};

#[test]
fn renders_content_and_loading_overlay_before_initial_refresh()
-> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    let text = rendered_starting(&mut app, 100, 28)?;
    assert!(text.contains("MiRMiR"));
    assert!(text.contains("[F1] Dashboard"));
    assert!(text.contains("Connecting to runtime"));
    assert!(text.contains("Loading models, telemetry and settings"));
    Ok(())
}

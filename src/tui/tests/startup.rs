use super::{App, fixtures::rendered_starting};

#[test]
fn renders_dashboard_without_blocking_before_initial_refresh()
-> Result<(), std::convert::Infallible> {
    let mut app = App::new(false);
    let text = rendered_starting(&mut app, 100, 28)?;
    assert!(text.contains("MiRMiR"));
    assert!(text.contains("Dashboard"));
    assert!(!text.contains("[F1]"));
    assert!(text.contains("PREFILL"));
    assert!(text.contains("RECENT ACTIVITY"));
    assert!(!text.contains("Connecting to runtime"));
    Ok(())
}

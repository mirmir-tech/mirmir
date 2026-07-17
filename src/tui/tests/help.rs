use super::{App, Screen, rendered};

#[test]
fn renders_context_help_for_current_tab() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.help_open = true;

    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("MODELS HELP"));
    assert!(text.contains("download, load, or unload selected model"));
    assert!(text.contains("mouse wheel"));
    Ok(())
}

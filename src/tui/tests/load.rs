use super::{App, Screen, rendered};
use crate::{
    rpc::proto,
    tui::app::{ContextChoice, ContextControl, LoadDialog, LoadStatus, LoadTarget},
};

#[test]
fn renders_context_row_and_memory_for_the_chosen_context() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    let mut context = ContextControl::new(proto::ModelContext {
        block_tokens: 16,
        min_tokens: 16,
        max_tokens: 40_960,
        saved: Some(ContextChoice::Automatic.to_proto()),
        automatic_tokens: 194_128,
        resident_tokens: Some(8_192),
    });
    let mut dialog = LoadDialog {
        target: LoadTarget {
            selector: "Qwen--Test".to_owned(),
            config_id: "Qwen--Test".to_owned(),
            repo_id: "Qwen/Test".to_owned(),
            revision: "main".to_owned(),
            commit: "abc123".to_owned(),
        },
        task: "generation".to_owned(),
        capabilities: None,
        status: LoadStatus::Editing,
        context: Some(context.clone()),
        fields: ["2048", "0.7", "0.9", "40", "1.1"].map(str::to_owned),
        selected: 0,
        has_mirmir_overrides: false,
        memory: Some(proto::ModelMemoryEstimate {
            required_bytes: 1_814_623_027,
            budget_bytes: Some(12 * 1024 * 1024 * 1024),
            fit: "does_not_fit".to_owned(),
            max_safe_context_tokens: Some(40_960),
            configured_cache_tokens: 8_192,
            ..Default::default()
        }),
        force: false,
        progress: None,
        error: None,
    };
    app.load_dialog = Some(dialog.clone());

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("context"));
    assert!(text.contains("Automatic · 194,128 now · loaded with 8,192"));
    assert!(text.contains("does not fit · need 1.68 GiB / 12.00 GiB"));
    assert!(text.contains("safe 40,960"));
    assert!(text.contains("a auto context"));

    assert!(context.step(-1));
    dialog.context = Some(context);
    app.load_dialog = Some(dialog);
    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("32,768 tokens"));
    Ok(())
}

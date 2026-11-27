use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

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

/// A load dialog over search results, the search field still being edited.
fn searching_with_load_dialog() -> App {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "Qwen".to_owned();
    app.editing_search = true;
    app.load_dialog = Some(LoadDialog {
        target: LoadTarget {
            selector: "/cache/models--Qwen--Test/snapshots/abc".to_owned(),
            config_id: "Qwen--Test".to_owned(),
            repo_id: "Qwen/Test".to_owned(),
            revision: "main".to_owned(),
            commit: "abc".to_owned(),
        },
        task: "generation".to_owned(),
        capabilities: None,
        status: LoadStatus::Editing,
        context: None,
        fields: ["2048", "0.7", "0.9", "40", "1.1"].map(str::to_owned),
        selected: 0,
        has_mirmir_overrides: false,
        memory: None,
        force: false,
        progress: None,
        error: None,
    });
    app
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::from(code))
}

/// A load dialog opened from search results, the search field still being
/// edited, offers its own keys: the search's do not take Enter from it.
#[test]
fn a_load_dialog_from_search_results_offers_its_own_keys() -> Result<(), std::convert::Infallible> {
    let mut app = searching_with_load_dialog();
    let text = rendered(&mut app, 140, 36)?;
    assert!(text.contains("f force") && !text.contains("type search"), "{text}");
    Ok(())
}

/// The keys a load dialog leaves, opened from search results, are
/// navigation's as wherever it opens: none reaches the search field, nor
/// the screen, behind it.
#[tokio::test]
async fn a_load_dialog_from_search_results_leaves_its_keys_to_navigation() {
    let mut client = crate::rpc::offline();
    for quit in ['q', 'Q'] {
        let mut app = searching_with_load_dialog();
        assert!(app.handle(Some(&press(KeyCode::Char(quit))), &mut client).await);
        assert_eq!(app.search_query, "Qwen");
    }

    // Esc asks to quit, over the dialog, which keeps the search behind it;
    // the confirmation then takes Enter, which the dialog would load by.
    let mut app = searching_with_load_dialog();
    assert!(!app.handle(Some(&press(KeyCode::Esc)), &mut client).await);
    assert!(app.navigation.exit_dialog && app.editing_search && app.load_dialog.is_some());
    assert!(!app.handle(Some(&press(KeyCode::Char('n'))), &mut client).await);
    assert!(!app.navigation.exit_dialog && app.load_dialog.is_some());
    assert!(!app.handle(Some(&press(KeyCode::Esc)), &mut client).await);
    assert!(app.handle(Some(&press(KeyCode::Enter)), &mut client).await);

    // A function key switches screens, the dialog staying; there, the chat
    // input behind it takes no key.
    let mut app = searching_with_load_dialog();
    assert!(!app.handle(Some(&press(KeyCode::F(3))), &mut client).await);
    assert_eq!(app.screen, Screen::Chat);
    assert!(!app.editing_search && app.load_dialog.is_some());
    assert!(!app.handle(Some(&press(KeyCode::Char('q'))), &mut client).await);
    let paste = Event::Paste("pasted".to_owned());
    assert!(!app.handle(Some(&paste), &mut client).await);
    assert!(app.chat_input.is_empty() && app.load_dialog.is_some());

    // Held Tab / BackTab repeats stay inside the modal load dialog rather than
    // switching screens behind it.
    let mut app = searching_with_load_dialog();
    for code in [KeyCode::Tab, KeyCode::BackTab] {
        let repeat =
            Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Repeat));
        assert!(!app.handle(Some(&repeat), &mut client).await);
        assert_eq!(app.screen, Screen::Models);
        assert!(app.load_dialog.is_some());
    }

    // Its own keys stay its own, and closed, it leaves the search editing.
    let mut app = searching_with_load_dialog();
    assert!(!app.handle(Some(&press(KeyCode::Char('x'))), &mut client).await);
    assert!(app.load_dialog.is_none() && app.editing_search);
    assert_eq!(app.search_query, "Qwen");
}

/// No click behind a dialog reaches the search field or the list.
#[tokio::test]
async fn a_click_behind_a_load_dialog_is_nothing() {
    let mut client = crate::rpc::offline();
    let mut app = searching_with_load_dialog();
    drop(rendered(&mut app, 140, 36));
    let area = app.list_view.expect("the models list is drawn").area;
    // The search field's clear button, which would end the search.
    let click = Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.right() - 2,
        row: area.y - 2,
        modifiers: KeyModifiers::NONE,
    });
    assert!(!app.handle(Some(&click), &mut client).await);
    assert!(app.editing_search && app.load_dialog.is_some());
    assert_eq!(app.search_query, "Qwen");
}

use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::{
    media::AttachedImage,
    tui::app::{Message, Screen},
};

#[test]
fn conversation_can_scroll_from_bottom_to_top() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Chat;
    app.chat_messages.push(Message {
        role: "assistant".to_owned(),
        content: "line-0\nline-1\nline-2\nline-3\nline-4\nline-5\nline-6\nline-7\nline-8\n\
                  line-9\nline-10\nline-11\nline-12\nline-13\nline-14\nline-15\nline-16\n\
                  line-17\nline-18\nline-19\n"
            .to_owned(),
        thought: String::new(),
    });
    let bottom = rendered_conversation(&app)?;
    assert!(bottom.contains("line-19"));

    app.chat_scroll = usize::MAX;
    let top = rendered_conversation(&app)?;
    assert!(top.contains("line-0"));
    Ok(())
}

#[test]
fn input_shows_the_attached_image() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.chat_image = Some(AttachedImage {
        name: "sample.png".to_owned(),
        bytes: vec![0; 2048],
    });
    let mut terminal = Terminal::new(TestBackend::new(60, 7))?;
    terminal.draw(|frame| composer(frame, frame.area(), &app))?;
    let rendered: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(rendered.contains("sample.png"));
    Ok(())
}

fn rendered_conversation(app: &App) -> Result<String, std::convert::Infallible> {
    let mut terminal = Terminal::new(TestBackend::new(48, 10))?;
    terminal.draw(|frame| conversation(frame, frame.area(), app))?;
    Ok(terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect())
}

use super::*;

#[test]
fn applies_one_stream_event_per_render_poll() {
    let mut app = App::new(true);
    app.chat_messages.push(Message {
        role: "assistant".to_owned(),
        content: String::new(),
        thought: String::new(),
    });
    let (sender, receiver) = mpsc::channel(4);
    sender.try_send(Ok(token("A", true))).expect("first token should queue");
    sender.try_send(Ok(token("B", false))).expect("second token should queue");
    app.chat_rx = Some(receiver);

    app.poll_chat();
    assert_eq!(app.chat_messages[0].thought, "A");
    app.poll_chat();
    assert_eq!(app.chat_messages[0].content, "B");
}

fn token(text: &str, reasoning: bool) -> proto::GenerateEvent {
    proto::GenerateEvent {
        event: Some(proto::generate_event::Event::Token(proto::Token {
            id: 1,
            text: text.to_owned(),
            reasoning,
        })),
    }
}

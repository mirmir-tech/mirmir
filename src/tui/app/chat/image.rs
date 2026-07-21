use super::{ChatStatus, Message};
use crate::{
    media::{dropped_file_path, read_image},
    rpc::{Client, proto},
    tui::app::App,
};

impl App {
    pub(in crate::tui::app) fn handle_chat_paste(&mut self, value: &str) {
        if self.chat_status == ChatStatus::Generating {
            return;
        }
        let Some(path) = dropped_file_path(value) else {
            self.chat_input.push_str(value);
            return;
        };
        if let Some(model) = self.selected_chat_model().filter(|model| !model.image_input) {
            self.chat_error = Some(image_unavailable(model));
            return;
        }
        match read_image(&path) {
            Ok(image) => {
                self.chat_image = Some(image);
                self.chat_error = None;
            },
            Err(error) => self.chat_error = Some(error.to_string()),
        }
    }

    pub(super) fn start_chat(&mut self, client: &Client) {
        if self.chat_status == ChatStatus::Generating || self.chat_input.trim().is_empty() {
            return;
        }
        if self.chat_image.is_none() && dropped_file_path(&self.chat_input).is_some() {
            let path = std::mem::take(&mut self.chat_input);
            self.handle_chat_paste(&path);
            return;
        }
        let Some(model) = self.selected_chat_model() else {
            self.chat_error = Some("load a model before starting chat".to_owned());
            return;
        };
        if self.chat_image.is_some() && !model.image_input {
            self.chat_error = Some(image_unavailable(model));
            return;
        }
        let model = model.id.clone();
        let content = self.chat_input.trim().to_owned();
        self.chat_input.clear();
        self.chat_scroll = 0;
        self.chat_messages.push(Message {
            role: "user".to_owned(),
            content,
            thought: String::new(),
        });
        let messages = self.messages_for_request();
        let image = self.chat_image.take().map(|image| image.bytes);
        self.chat_messages.push(Message {
            role: "assistant".to_owned(),
            content: String::new(),
            thought: String::new(),
        });
        let mut request = proto::GenerateRequest {
            model,
            prompt: String::new(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            repetition_penalty: None,
            seed: None,
            messages,
            image,
        };
        self.apply_chat_parameters(&mut request);
        self.spawn_chat(client, request);
    }

    fn messages_for_request(&self) -> Vec<proto::ChatMessageInput> {
        let mut messages: Vec<_> = self
            .chat_messages
            .iter()
            .map(|message| proto::ChatMessageInput {
                role: message.role.clone(),
                content: message.content.clone(),
                reasoning_content: (!message.thought.is_empty()).then(|| message.thought.clone()),
            })
            .collect();
        if self.chat_image.is_some()
            && let Some(message) = messages.iter_mut().rev().find(|message| message.role == "user")
        {
            message.content = format!("{}\n{}", libmir::IMAGE_PLACEHOLDER, message.content);
        }
        messages
    }
}

fn image_unavailable(model: &proto::ModelInfo) -> String {
    if model.image_unavailable_reason.is_empty() {
        "selected model cannot accept images".to_owned()
    } else {
        format!("selected model cannot accept images: {}", model.image_unavailable_reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{media::AttachedImage, tui::app::Message};

    #[test]
    fn attaches_image_to_latest_user_message_only() {
        let mut app = App::new(true);
        app.chat_messages = vec![
            message("user", "first"),
            message("assistant", "answer"),
            message("user", "second"),
        ];
        app.chat_image = Some(AttachedImage {
            name: "pixel.png".to_owned(),
            bytes: b"image".to_vec(),
        });

        let messages = app.messages_for_request();

        assert_eq!(messages[0].content, "first");
        assert_eq!(messages[2].content, format!("{}\nsecond", libmir::IMAGE_PLACEHOLDER));
    }

    fn message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_owned(),
            content: content.to_owned(),
            thought: String::new(),
        }
    }
}

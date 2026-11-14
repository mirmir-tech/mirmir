use std::time::Duration;

use tokio::sync::mpsc;

use super::{super::App, context::ContextChoice};
use crate::rpc::{Client, proto};

/// How long the context must stay unchanged before the memory estimate is
/// refreshed.
const REFRESH_DELAY: Duration = Duration::from_millis(300);

impl App {
    pub(super) fn step_context(&mut self, direction: i8, client: &Client) {
        let changed = self
            .load_dialog
            .as_mut()
            .and_then(|dialog| dialog.context.as_mut())
            .is_some_and(|context| context.step(direction));
        if changed {
            self.refresh_context(client);
        }
    }

    pub(super) fn choose_automatic_context(&mut self, client: &Client) {
        let changed = self
            .load_dialog
            .as_mut()
            .and_then(|dialog| dialog.context.as_mut())
            .is_some_and(|context| context.choose(ContextChoice::Automatic));
        if changed {
            self.refresh_context(client);
        }
    }

    /// Re-inspects the model for the chosen context once the choice settles.
    ///
    /// Replacing the receiver cancels the previous refresh: its task finds the
    /// channel closed after the pause and skips the request, and a response
    /// already in flight is dropped with the old receiver.
    fn refresh_context(&mut self, client: &Client) {
        let Some(dialog) = self.load_dialog.as_mut() else {
            return;
        };
        let Some(context) = dialog.context.as_ref() else {
            return;
        };
        dialog.error = None;
        let request = proto::InspectModelRequest {
            selector: dialog.target.selector.clone(),
            context: Some(context.choice.to_proto()),
        };
        let (sender, receiver) = mpsc::channel(1);
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            tokio::time::sleep(REFRESH_DELAY).await;
            if sender.is_closed() {
                return;
            }
            let result = crate::tui::string_result(client.inspect_model(request).await)
                .map(tonic::Response::into_inner);
            drop(sender.send(result).await);
        }));
        self.context_rx = Some(receiver);
    }

    pub(super) fn poll_context_refresh(&mut self) {
        let Some(result) = self.context_rx.as_mut().map(mpsc::Receiver::try_recv) else {
            return;
        };
        match result {
            Ok(Ok(response)) => self.apply_context_refresh(response),
            Ok(Err(error)) => {
                if let Some(dialog) = self.load_dialog.as_mut() {
                    dialog.error = Some(error);
                }
            },
            Err(mpsc::error::TryRecvError::Empty) => return,
            Err(mpsc::error::TryRecvError::Disconnected) => {},
        }
        self.context_rx = None;
    }

    /// Takes the memory estimate and context limits; edited settings stay as
    /// they are.
    fn apply_context_refresh(&mut self, response: proto::InspectModelResponse) {
        let Some(dialog) = self.load_dialog.as_mut() else {
            return;
        };
        dialog.memory = response.memory;
        if let (Some(control), Some(limits)) = (dialog.context.as_mut(), response.context) {
            control.limits = limits;
        }
    }

    pub(in crate::tui) const fn context_refreshing(&self) -> bool {
        self.context_rx.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{ContextControl, context::limits, dialog::dialog},
        *,
    };

    #[test]
    fn refreshed_estimate_keeps_edited_settings_and_choice() {
        let mut app = App::new(true);
        let mut control = ContextControl::new(limits(ContextChoice::Automatic));
        control.choice = ContextChoice::Tokens(8_192);
        let mut edited = dialog("generation", Some(control));
        edited.fields[1] = "1.3".to_owned();
        app.load_dialog = Some(edited);
        let mut refreshed = limits(ContextChoice::Automatic);
        refreshed.automatic_tokens = 100_000;
        app.apply_context_refresh(proto::InspectModelResponse {
            settings: Some(proto::GenerationSettings { temperature: 0.1, ..Default::default() }),
            memory: Some(proto::ModelMemoryEstimate {
                configured_cache_tokens: 8_192,
                ..Default::default()
            }),
            context: Some(refreshed),
            ..Default::default()
        });
        let dialog = app.load_dialog.expect("dialog stays open");
        assert_eq!(dialog.fields[1], "1.3");
        assert_eq!(dialog.memory.map(|memory| memory.configured_cache_tokens), Some(8_192));
        let context = dialog.context.expect("context row");
        assert_eq!(context.choice, ContextChoice::Tokens(8_192));
        assert_eq!(context.limits.automatic_tokens, 100_000);
    }
}

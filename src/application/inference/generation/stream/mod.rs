mod disconnect;

use libmir::{GenerationRequest, ProgressEvent, ProgressStage};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{Application, GenerationEvent, Result};

impl Application {
    pub fn generation_stream(
        &self,
        selector: &str,
        request: GenerationRequest,
        image: Option<Vec<u8>>,
    ) -> ReceiverStream<Result<GenerationEvent>> {
        let application = self.clone();
        let mut session = self.start_generation(selector);
        let (sender, receiver) = mpsc::channel(64);
        drop(sender.try_send(Ok(GenerationEvent::Started {
            operation_id: session.operation_id().to_owned(),
        })));
        let disconnect = disconnect::Watch::new(sender.clone(), session.cancellation());
        drop(tokio::task::spawn_blocking(move || {
            let _disconnect = disconnect;
            let cancellation = session.cancellation();
            let output_events = sender.clone();
            let mut output_started = false;
            let mut progress = move |event: ProgressEvent| {
                if !output_started
                    && event.stage() == ProgressStage::DecodeTokens
                    && event.count().current() == 0
                {
                    output_started = true;
                    drop(output_events.blocking_send(Ok(GenerationEvent::OutputStarted)));
                }
            };
            let tokens = sender.clone();
            let mut token = move |token| {
                if tokens.blocking_send(Ok(GenerationEvent::Token(token))).is_err() {
                    cancellation.cancel();
                }
            };
            let result = application.generate(
                &mut session,
                &request,
                image.as_deref(),
                &mut progress,
                &mut token,
            );
            drop(match result {
                Ok(result) => {
                    sender.blocking_send(Ok(GenerationEvent::Completion(Box::new(result))))
                },
                Err(error) => sender.blocking_send(Err(error)),
            });
        }));
        ReceiverStream::new(receiver)
    }
}

use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::BenchmarkEvent;
use crate::{
    prompt::benchmark::SampleReport,
    rpc::{Client, proto},
};

pub(super) async fn run(
    client: &mut Client,
    request: proto::GenerateRequest,
    warmup: usize,
    samples: usize,
    sender: &mpsc::Sender<Result<BenchmarkEvent, String>>,
) -> Result<(), String> {
    for index in 0..warmup {
        send(sender, BenchmarkEvent::Phase(format!("warmup {}/{}", index + 1, warmup))).await?;
        drop(one(client, request.clone(), sender).await?);
    }
    for index in 0..samples {
        send(sender, BenchmarkEvent::Phase(format!("sample {}/{}", index + 1, samples))).await?;
        let completion = one(client, request.clone(), sender).await?;
        send(sender, BenchmarkEvent::Sample(SampleReport::from(completion))).await?;
    }
    send(sender, BenchmarkEvent::Finished).await
}

async fn one(
    client: &mut Client,
    request: proto::GenerateRequest,
    sender: &mpsc::Sender<Result<BenchmarkEvent, String>>,
) -> Result<proto::Completion, String> {
    let mut stream =
        client.generate(request).await.map_err(|error| error.to_string())?.into_inner();
    while let Some(event) = stream.next().await {
        match event.map_err(|error| error.to_string())?.event {
            Some(proto::generate_event::Event::Started(started)) => {
                send(sender, BenchmarkEvent::Started(started.operation_id)).await?;
            },
            Some(proto::generate_event::Event::Completion(completion)) => return Ok(completion),
            _ => {},
        }
    }
    Err("generation ended without benchmark completion".to_owned())
}

async fn send(
    sender: &mpsc::Sender<Result<BenchmarkEvent, String>>,
    event: BenchmarkEvent,
) -> Result<(), String> {
    sender.send(Ok(event)).await.map_err(|_| "benchmark UI closed".to_owned())
}

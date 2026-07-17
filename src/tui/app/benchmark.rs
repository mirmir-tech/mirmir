mod export;
mod runner;

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use self::runner::run;
use super::App;
use crate::{
    prompt::benchmark::{Aggregate, BenchmarkReport, SampleReport},
    rpc::{Client, proto},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkStatus {
    Idle,
    Running,
}

pub struct BenchmarkState {
    pub prompt: String,
    pub warmup: usize,
    pub samples: usize,
    pub completed: usize,
    pub phase: String,
    pub status: BenchmarkStatus,
    pub model_index: usize,
    pub results: Vec<SampleReport>,
    pub aggregate: Option<Aggregate>,
    pub runs: Vec<BenchmarkReport>,
    pub error: Option<String>,
    pub operation_id: Option<String>,
    export_dir: Option<PathBuf>,
    receiver: Option<mpsc::Receiver<Result<BenchmarkEvent, String>>>,
    running_model: String,
}

enum BenchmarkEvent {
    Phase(String),
    Started(String),
    Sample(SampleReport),
    Finished,
}

impl Default for BenchmarkState {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            warmup: 1,
            samples: 3,
            completed: 0,
            phase: "ready".to_owned(),
            status: BenchmarkStatus::Idle,
            model_index: 0,
            results: Vec::new(),
            aggregate: None,
            runs: Vec::new(),
            error: None,
            operation_id: None,
            export_dir: None,
            receiver: None,
            running_model: String::new(),
        }
    }
}

impl App {
    pub fn set_benchmark_export_dir(&mut self, path: PathBuf) {
        self.benchmark.export_dir = Some(path);
    }

    pub(super) fn handle_benchmark_key(&mut self, key: KeyEvent, client: &Client) {
        if self.benchmark.status == BenchmarkStatus::Running {
            if matches!(key.code, KeyCode::Char('x' | 'X')) {
                self.cancel_benchmark(client);
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('k' | 'K') => self.benchmark.prompt.clear(),
                KeyCode::Char('e' | 'E') => self.export_benchmark(),
                _ => {},
            }
            return;
        }
        match key.code {
            KeyCode::Enter => self.start_benchmark(client),
            KeyCode::Backspace => drop(self.benchmark.prompt.pop()),
            KeyCode::Left => {
                self.benchmark.model_index = self.benchmark.model_index.saturating_sub(1);
            },
            KeyCode::Right => {
                self.benchmark.model_index = self
                    .benchmark
                    .model_index
                    .saturating_add(1)
                    .min(self.models.len().saturating_sub(1));
            },
            KeyCode::Up => {
                self.benchmark.samples = self.benchmark.samples.saturating_add(1).min(20);
            },
            KeyCode::Down => {
                self.benchmark.samples = self.benchmark.samples.saturating_sub(1).max(1);
            },
            KeyCode::PageUp => {
                self.benchmark.warmup = self.benchmark.warmup.saturating_add(1).min(10);
            },
            KeyCode::PageDown => self.benchmark.warmup = self.benchmark.warmup.saturating_sub(1),
            KeyCode::Char(character) => self.benchmark.prompt.push(character),
            _ => {},
        }
    }

    pub fn poll_benchmark(&mut self) {
        loop {
            let Some(result) = self.benchmark.receiver.as_mut().map(mpsc::Receiver::try_recv)
            else {
                return;
            };
            match result {
                Ok(Ok(event)) => self.apply_benchmark_event(event),
                Ok(Err(error)) => {
                    self.fail_benchmark(error);
                    return;
                },
                Err(mpsc::error::TryRecvError::Empty) => return,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    if self.benchmark.status == BenchmarkStatus::Running {
                        self.fail_benchmark("benchmark stream ended unexpectedly".to_owned());
                    }
                    return;
                },
            }
        }
    }

    fn start_benchmark(&mut self, client: &Client) {
        let prompt = self.benchmark.prompt.trim().to_owned();
        let Some(model) = self.models.get(self.benchmark.model_index) else {
            self.benchmark.error = Some("load a model before benchmarking".to_owned());
            return;
        };
        if prompt.is_empty() {
            self.benchmark.error = Some("enter a benchmark prompt".to_owned());
            return;
        }
        let request = proto::GenerateRequest {
            model: model.id.clone(),
            prompt,
            max_tokens: Some(128),
            temperature: None,
            top_p: None,
            top_k: None,
            repetition_penalty: None,
            seed: Some(42),
            messages: Vec::new(),
        };
        let (sender, receiver) = mpsc::channel(64);
        let mut client = client.clone();
        let warmup = self.benchmark.warmup;
        let samples = self.benchmark.samples;
        drop(tokio::spawn(async move {
            if let Err(error) = run(&mut client, request, warmup, samples, &sender).await {
                drop(sender.send(Err(error)).await);
            }
        }));
        self.benchmark.running_model.clone_from(&model.id);
        self.benchmark.results.clear();
        self.benchmark.aggregate = None;
        self.benchmark.completed = 0;
        "starting".clone_into(&mut self.benchmark.phase);
        self.benchmark.status = BenchmarkStatus::Running;
        self.benchmark.error = None;
        self.benchmark.receiver = Some(receiver);
    }

    fn apply_benchmark_event(&mut self, event: BenchmarkEvent) {
        match event {
            BenchmarkEvent::Phase(phase) => self.benchmark.phase = phase,
            BenchmarkEvent::Started(id) => self.benchmark.operation_id = Some(id),
            BenchmarkEvent::Sample(sample) => {
                self.benchmark.results.push(sample);
                self.benchmark.completed = self.benchmark.results.len();
            },
            BenchmarkEvent::Finished => self.finish_benchmark(),
        }
    }

    fn finish_benchmark(&mut self) {
        let aggregate = Aggregate::from_samples(&self.benchmark.results);
        self.benchmark.runs.push(BenchmarkReport {
            model: self.benchmark.running_model.clone(),
            warmup: self.benchmark.warmup,
            server_reused: self.server_reused,
            samples: self.benchmark.results.clone(),
            aggregate: aggregate.clone(),
        });
        self.benchmark.aggregate = Some(aggregate);
        self.benchmark.status = BenchmarkStatus::Idle;
        "completed".clone_into(&mut self.benchmark.phase);
        self.benchmark.operation_id = None;
        self.benchmark.receiver = None;
    }

    fn fail_benchmark(&mut self, error: String) {
        self.benchmark.error = Some(error);
        self.benchmark.status = BenchmarkStatus::Idle;
        "failed".clone_into(&mut self.benchmark.phase);
        self.benchmark.operation_id = None;
        self.benchmark.receiver = None;
    }

    fn cancel_benchmark(&mut self, client: &Client) {
        let Some(operation_id) = self.benchmark.operation_id.clone() else {
            return;
        };
        let mut client = client.clone();
        drop(tokio::spawn(async move {
            drop(client.cancel_operation(proto::CancelOperationRequest { operation_id }).await);
        }));
        "cancelling".clone_into(&mut self.benchmark.phase);
    }
}

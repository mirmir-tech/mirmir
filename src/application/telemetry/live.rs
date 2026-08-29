use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use libmir::{ProgressEvent, ProgressStage};

use super::stage::Stage;

#[derive(Clone)]
pub struct Registry(Arc<RegistryInner>);

struct RegistryInner {
    next: AtomicU64,
    updates: AtomicU64,
    requests: Mutex<HashMap<u64, Arc<Mutex<Request>>>>,
}

pub struct Handle {
    id: u64,
    registry: Registry,
    request: Arc<Mutex<Request>>,
}

struct Request {
    started: Instant,
    stage_started: Instant,
    stage_sequence: u64,
    stage: Stage,
    prompt_current: u64,
    prompt_total: u64,
    prefill_rate: Option<f64>,
    completion_tokens: u64,
    ttft_ms: Option<f64>,
}

#[derive(Default)]
pub struct Snapshot {
    pub requests: u64,
    pub rate: Option<f64>,
    pub prefill_rate: Option<f64>,
    pub decode_rate: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub elapsed_ms: f64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub(super) stage: Option<Stage>,
}

impl Registry {
    pub fn new() -> Self {
        Self(Arc::new(RegistryInner {
            next: AtomicU64::new(1),
            updates: AtomicU64::new(1),
            requests: Mutex::new(HashMap::new()),
        }))
    }

    pub fn begin(&self) -> Handle {
        let id = self.0.next.fetch_add(1, Ordering::Relaxed);
        let stage_sequence = self.0.updates.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        let request = Arc::new(Mutex::new(Request {
            started: now,
            stage_started: now,
            stage_sequence,
            stage: Stage::Starting,
            prompt_current: 0,
            prompt_total: 0,
            prefill_rate: None,
            completion_tokens: 0,
            ttft_ms: None,
        }));
        self.0
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, request.clone());
        Handle { id, registry: self.clone(), request }
    }

    pub fn snapshot(&self) -> Snapshot {
        let requests = self
            .0
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshot = Snapshot::default();
        let mut selected_stage = None;
        for request in requests {
            let stage = add_request(&mut snapshot, &request);
            if selected_stage.is_none_or(|selected: (u64, Stage)| stage.0 > selected.0) {
                selected_stage = Some(stage);
            }
        }
        snapshot.stage = selected_stage.map(|(_, stage)| stage);
        snapshot
    }
}

impl Handle {
    pub fn resolving(&self) {
        let mut request = self.request.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_stage(&self.registry, &mut request, Stage::Resolving);
    }

    pub fn stage(&self, stage: ProgressStage) {
        let mut request = self.request.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        set_stage(&self.registry, &mut request, Stage::Runtime(stage));
    }

    pub fn progress(&self, event: &ProgressEvent) {
        let mut request = self.request.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let stage = event.stage();
        let count = event.count();
        if stage == ProgressStage::DecodeTokens
            && request.stage == Stage::Runtime(ProgressStage::PrefillTokens)
        {
            request.prefill_rate =
                Some(rate(request.prompt_current, request.stage_started.elapsed().as_secs_f64()));
        }
        set_stage(&self.registry, &mut request, Stage::Runtime(stage));
        match stage {
            ProgressStage::PrefillTokens => {
                request.prompt_current = count.current();
                request.prompt_total = count.total();
            },
            ProgressStage::DecodeTokens => {
                request.completion_tokens = count.current();
            },
            ProgressStage::LoadWeights
            | ProgressStage::InitializeRuntime
            | ProgressStage::Warmup => {},
        }
    }

    pub fn token_emitted(&self) {
        let mut request = self.request.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if request.ttft_ms.is_none() {
            request.ttft_ms = Some(ms(request.started.elapsed().as_secs_f64()));
        }
    }

    pub fn finish(&self) {
        self.registry
            .0
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.id);
    }
}

fn add_request(snapshot: &mut Snapshot, request: &Arc<Mutex<Request>>) -> (u64, Stage) {
    let request = request.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let elapsed = request.started.elapsed().as_secs_f64();
    let stage_elapsed = request.stage_started.elapsed().as_secs_f64();
    snapshot.requests = snapshot.requests.saturating_add(1);
    add_rate(&mut snapshot.rate, request.completion_tokens, elapsed);
    if let Some(value) = request.prefill_rate {
        add_value(&mut snapshot.prefill_rate, value);
    }
    match request.stage {
        Stage::Runtime(ProgressStage::PrefillTokens) => {
            add_rate(&mut snapshot.prefill_rate, request.prompt_current, stage_elapsed);
        },
        Stage::Runtime(ProgressStage::DecodeTokens) => add_rate(
            &mut snapshot.decode_rate,
            request.completion_tokens.saturating_sub(1),
            stage_elapsed,
        ),
        _ => {},
    }
    snapshot.ttft_ms =
        max_option(snapshot.ttft_ms, Some(request.ttft_ms.unwrap_or_else(|| ms(elapsed))));
    snapshot.elapsed_ms = snapshot.elapsed_ms.max(ms(elapsed));
    snapshot.prompt_tokens = snapshot.prompt_tokens.saturating_add(request.prompt_total);
    snapshot.completion_tokens =
        snapshot.completion_tokens.saturating_add(request.completion_tokens);
    (request.stage_sequence, request.stage)
}

fn set_stage(registry: &Registry, request: &mut Request, stage: Stage) {
    if request.stage != stage {
        request.stage = stage;
        request.stage_started = Instant::now();
        request.stage_sequence = registry.0.updates.fetch_add(1, Ordering::Relaxed);
    }
}

fn add_rate(rate: &mut Option<f64>, tokens: u64, seconds: f64) {
    add_value(rate, self::rate(tokens, seconds));
}

fn rate(tokens: u64, seconds: f64) -> f64 {
    if seconds > 0.0 {
        to_f64(tokens) / seconds
    } else {
        0.0
    }
}

fn add_value(rate: &mut Option<f64>, value: f64) {
    *rate = Some(rate.unwrap_or(0.0) + value);
}

fn max_option(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (left, right) => left.or(right),
    }
}

fn to_f64(value: u64) -> f64 {
    value.to_string().parse().unwrap_or(f64::INFINITY)
}

fn ms(seconds: f64) -> f64 {
    seconds * 1_000.0
}

#[cfg(test)]
mod tests;

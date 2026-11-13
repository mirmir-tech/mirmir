mod history;
mod live;
mod memory;
mod rates;
mod recorder;
mod source;
pub use recorder::Recorder;
pub use source::Source;
mod request;
mod runtime;
mod snapshot;
mod stage;

pub use history::{History, RETENTION_LIMIT, SAMPLING_INTERVAL_MS};
pub use memory::{AcceleratorMemory, AcceleratorMemoryPool, HostMemory};
pub use request::{GenerationTelemetry, Telemetry};
pub use runtime::{KvTelemetry, RuntimeTelemetry};
pub use snapshot::{HistorySample, Snapshot};

pub struct CompletionMetrics {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub tokens_per_second: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
}

#[derive(Default)]
pub(super) struct Rates {
    last_rate: Option<f64>,
    rate_sum: f64,
    rate_samples: u64,
    last_ttft: Option<f64>,
    ttft_sum: f64,
    ttft_samples: u64,
    last_prefill_rate: Option<f64>,
    prefill_rate_sum: f64,
    prefill_rate_samples: u64,
    last_decode_rate: Option<f64>,
    decode_rate_sum: f64,
    decode_rate_samples: u64,
}

pub(super) struct Snapshot {
    pub(super) last_rate: Option<f64>,
    pub(super) mean_rate: Option<f64>,
    pub(super) last_ttft: Option<f64>,
    pub(super) mean_ttft: Option<f64>,
    pub(super) last_prefill_rate: Option<f64>,
    pub(super) mean_prefill_rate: Option<f64>,
    pub(super) last_decode_rate: Option<f64>,
    pub(super) mean_decode_rate: Option<f64>,
}

impl Rates {
    pub(super) fn record_rate(&mut self, value: Option<f64>) {
        record(value, &mut self.last_rate, &mut self.rate_sum, &mut self.rate_samples);
    }

    pub(super) fn record_ttft(&mut self, value: Option<f64>) {
        record(value, &mut self.last_ttft, &mut self.ttft_sum, &mut self.ttft_samples);
    }

    pub(super) fn record_prefill_rate(&mut self, value: Option<f64>) {
        record(
            value,
            &mut self.last_prefill_rate,
            &mut self.prefill_rate_sum,
            &mut self.prefill_rate_samples,
        );
    }

    pub(super) fn record_decode_rate(&mut self, value: Option<f64>) {
        record(
            value,
            &mut self.last_decode_rate,
            &mut self.decode_rate_sum,
            &mut self.decode_rate_samples,
        );
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            last_rate: self.last_rate,
            mean_rate: mean(self.rate_sum, self.rate_samples),
            last_ttft: self.last_ttft,
            mean_ttft: mean(self.ttft_sum, self.ttft_samples),
            last_prefill_rate: self.last_prefill_rate,
            mean_prefill_rate: mean(self.prefill_rate_sum, self.prefill_rate_samples),
            last_decode_rate: self.last_decode_rate,
            mean_decode_rate: mean(self.decode_rate_sum, self.decode_rate_samples),
        }
    }
}

fn record(value: Option<f64>, last: &mut Option<f64>, sum: &mut f64, samples: &mut u64) {
    if let Some(value) = value.filter(|value| value.is_finite()) {
        *last = Some(value);
        *sum += value;
        *samples = samples.saturating_add(1);
    }
}

fn mean(sum: f64, samples: u64) -> Option<f64> {
    (samples > 0).then(|| sum / samples.to_string().parse::<f64>().unwrap_or(f64::INFINITY))
}

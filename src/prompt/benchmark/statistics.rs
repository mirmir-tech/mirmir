use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Distribution {
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub p50: f64,
    pub p95: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
    pub raw: Vec<f64>,
}

impl Distribution {
    pub fn new(values: impl Iterator<Item = f64>) -> Self {
        let raw = values.filter(|value| value.is_finite()).collect::<Vec<_>>();
        let mut sorted = raw.clone();
        sorted.sort_by(f64::total_cmp);
        let count = sorted.len();
        let average = mean(&sorted);
        let variance =
            mean(&sorted.iter().map(|value| (value - average).powi(2)).collect::<Vec<_>>());
        Self {
            count,
            mean: average,
            median: median(&sorted),
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            stddev: variance.sqrt(),
            min: sorted.first().copied().unwrap_or_default(),
            max: sorted.last().copied().unwrap_or_default(),
            raw,
        }
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let count = values.len().to_string().parse::<f64>().unwrap_or(f64::INFINITY);
    values.iter().sum::<f64>() / count
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn median(sorted: &[f64]) -> f64 {
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => 0.0,
        length if length.is_multiple_of(2) => f64::midpoint(sorted[middle - 1], sorted[middle]),
        _ => sorted[middle],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_percentiles_and_population_deviation() {
        let values = Distribution::new([1.0, 2.0, 3.0, 4.0, 5.0].into_iter());
        assert!((values.median - 3.0).abs() < f64::EPSILON);
        assert!((values.p95 - 5.0).abs() < f64::EPSILON);
        assert!((values.stddev - 2.0_f64.sqrt()).abs() < 1e-12);
        assert_eq!(values.raw, [1.0, 2.0, 3.0, 4.0, 5.0]);
        let even = Distribution::new([1.0, 2.0, 3.0, 4.0].into_iter());
        assert!((even.median - 2.5).abs() < f64::EPSILON);
        assert!((even.p50 - 2.0).abs() < f64::EPSILON);
    }
}

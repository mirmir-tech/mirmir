//! A variant whose header no backend admits is refused before it downloads:
//! it would not load after the transfer either.

use libmir::{AdmissionStatus, BackendAdmissionReport};

use crate::{
    config::HubReference,
    error::{Error, Result},
};

/// Refuses `reference` when every backend's report refuses it, naming each
/// refusal libmir gave and the repository's variants, `offered`, to pick
/// another from; a variant some backend runs, or may run, passes.
pub(super) fn admitted(
    reference: &HubReference,
    reports: &[BackendAdmissionReport],
    offered: &str,
) -> Result<()> {
    if reports.is_empty()
        || reports.iter().any(|report| report.status != AdmissionStatus::Unsupported)
    {
        return Ok(());
    }
    let reasons = reports
        .iter()
        .flat_map(|report| &report.checks)
        .filter(|check| check.status() == AdmissionStatus::Unsupported)
        .map(|check| check.detail.as_str())
        .collect::<Vec<_>>();
    Err(Error::Config(format!(
        "`{reference}` runs on no backend, so it is not downloaded: {}; `{}` offers {offered}",
        reasons.join("; "),
        reference.repository()
    )))
}

#[cfg(test)]
mod tests;

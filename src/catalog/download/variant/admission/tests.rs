use libmir::{
    AdmissionCheck, AdmissionStatus, BackendAdmissionReport, BackendTarget, CheckSubject,
    CheckpointEncoding, EncodingKind,
};

use super::admitted;
use crate::{config::HubReference, error::Result};

fn report(backend: BackendTarget, status: AdmissionStatus, detail: &str) -> BackendAdmissionReport {
    BackendAdmissionReport {
        backend,
        status,
        encoding: CheckpointEncoding::default(),
        architecture: None,
        checks: vec![AdmissionCheck {
            subject: CheckSubject::Encoding { kind: EncodingKind::Ggml, status },
            detail: detail.into(),
        }],
    }
}

fn reference() -> Result<HubReference> {
    "owner/gpt-oss-20b-GGUF:Q8_0".parse()
}

#[test]
fn a_variant_no_backend_admits_is_refused_with_every_reason() -> Result<()> {
    let reports = [
        report(BackendTarget::Metal, AdmissionStatus::Unsupported, "Metal needs MXFP4 experts"),
        report(BackendTarget::Cuda, AdmissionStatus::Unsupported, "CUDA needs MXFP4 experts"),
    ];
    let refusal =
        admitted(&reference()?, &reports, "Q8_0, Q4_K_M").expect_err("no backend admits it");
    let message = refusal.to_string();
    assert!(message.contains("owner/gpt-oss-20b-GGUF:Q8_0"), "{message}");
    assert!(message.contains("Metal needs MXFP4 experts"), "{message}");
    assert!(message.contains("CUDA needs MXFP4 experts"), "{message}");
    assert!(message.contains("offers Q8_0, Q4_K_M"), "{message}");
    Ok(())
}

#[test]
fn a_variant_one_backend_runs_or_may_run_passes() -> Result<()> {
    for status in [AdmissionStatus::Supported, AdmissionStatus::Partial, AdmissionStatus::Unknown] {
        let reports = [
            report(BackendTarget::Metal, AdmissionStatus::Unsupported, "Metal refuses"),
            report(BackendTarget::Cuda, status, "CUDA executes GGML blocks in place"),
        ];
        admitted(&reference()?, &reports, "Q8_0")?;
    }
    Ok(())
}

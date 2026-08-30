use tonic::Code;

use super::super::status::application_ref;

#[test]
fn reports_atomic_memory_admission_failures_as_resource_exhaustion() {
    let error: crate::application::Error = libmir::Error::MemoryAdmission {
        model: "test".into(),
        required_bytes: 2,
        available_bytes: 1,
    }
    .into();
    let status = application_ref(&error);

    assert_eq!(status.code(), Code::ResourceExhausted);
    assert!(status.message().contains("only 1 bytes remain"));
}

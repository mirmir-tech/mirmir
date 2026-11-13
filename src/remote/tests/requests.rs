//! Both `Runtime` listeners decode requests up to `rpc::MAX_REQUEST_BYTES`,
//! so an image the media policy accepts reaches Generate; a larger image is
//! refused with a message instead of a transport error.

use tonic::Code;

use super::*;
use crate::media::MAX_IMAGE_BYTES;

/// The status Generate ends with for an image of `size` bytes and no model.
async fn generate_with_image(client: &mut rpc::Client, size: usize) -> tonic::Status {
    let request = proto::GenerateRequest {
        model: "review-no-model".to_owned(),
        prompt: "Describe <|mirmir_image|>".to_owned(),
        image: Some(vec![0x89; size]),
        ..Default::default()
    };
    match client.generate(request).await {
        Err(status) => status,
        Ok(response) => {
            let mut stream = response.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(_)) => {},
                    Ok(None) => panic!("generation without a model succeeded"),
                    Err(status) => break status,
                }
            }
        },
    }
}

#[tokio::test]
async fn images_beyond_the_default_grpc_limit_reach_generate() -> crate::error::Result<()> {
    let (service, root) = service();
    let remote = service.remote().clone();
    let status = remote.enable(Some("127.0.0.1:0"), service.clone()).await?;
    let listening = status.listening.expect("listener running");
    let (_, key) = remote.create_key("images", Scope::Use)?;
    let address = listening.address.to_string();
    let mut client = rpc::connect_remote(&target(&address, &key, listening.fingerprint)).await?;

    let accepted = generate_with_image(&mut client, 6 * 1024 * 1024).await;
    assert_ne!(
        accepted.code(),
        Code::OutOfRange,
        "a 6 MiB image passes the transport: {accepted}"
    );
    assert!(!accepted.message().contains("message length too large"), "{accepted}");

    let oversized = generate_with_image(&mut client, MAX_IMAGE_BYTES + 1).await;
    assert_eq!(oversized.code(), Code::InvalidArgument, "{oversized}");
    assert!(oversized.message().contains("images may have at most"), "{oversized}");

    let beyond = generate_with_image(&mut client, rpc::MAX_REQUEST_BYTES + 1).await;
    assert_eq!(beyond.code(), Code::OutOfRange, "the transport bound still applies: {beyond}");

    remote.shutdown().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

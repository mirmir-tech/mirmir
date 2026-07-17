use std::path::Path;

use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use super::{
    PROTOCOL_VERSION,
    proto::{HealthRequest, runtime_client::RuntimeClient},
};
use crate::error::{Error, Result};

pub type Client = RuntimeClient<Channel>;

pub async fn connect(socket: &Path) -> Result<Client> {
    let path = socket.to_owned();
    let channel = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = path.clone();
            async move { UnixStream::connect(path).await.map(TokioIo::new) }
        }))
        .await?;
    let mut client = RuntimeClient::new(channel);
    let health = client.health(HealthRequest {}).await?.into_inner();
    validate_protocol(&health.protocol_version)?;
    Ok(client)
}

fn validate_protocol(version: &str) -> Result<()> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "server protocol {version} is incompatible with client protocol {PROTOCOL_VERSION}; restart mirmir serve"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_incompatible_server_protocol() {
        assert!(validate_protocol(PROTOCOL_VERSION).is_ok());
        let error = validate_protocol("old").expect_err("old protocol must be rejected");
        assert!(error.to_string().contains("restart mirmir serve"));
    }
}

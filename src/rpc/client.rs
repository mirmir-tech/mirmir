use std::{path::Path, sync::Arc};

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
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if !std::fs::metadata(socket)?.file_type().is_socket() {
            return Err(Error::NotSocket(socket.to_owned()));
        }
    }
    let path = socket.to_owned();
    let initial = Arc::new(tokio::sync::Mutex::new(Some(UnixStream::connect(socket).await?)));
    let channel = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = path.clone();
            let initial = initial.clone();
            async move {
                let first = initial.lock().await.take();
                let stream = match first {
                    Some(stream) => stream,
                    None => UnixStream::connect(path).await?,
                };
                Ok::<_, std::io::Error>(TokioIo::new(stream))
            }
        }))
        .await?;
    let mut client = RuntimeClient::new(channel);
    let health = client.health(HealthRequest {}).await?.into_inner();
    validate_protocol(&health.protocol_version)?;
    Ok(client)
}

pub fn unavailable(error: &Error) -> bool {
    match error {
        Error::NotSocket(_) => true,
        Error::Io(error) => matches!(
            error.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        ),
        _ => false,
    }
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

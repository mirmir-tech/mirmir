use std::{path::Path, sync::Arc};

use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::{Interceptor, interceptor::InterceptedService},
    transport::{Channel, Endpoint, Uri},
};
use tower::service_fn;

use super::{
    PROTOCOL_VERSION,
    proto::{HealthRequest, runtime_client::RuntimeClient},
};
use crate::{
    error::{Error, Result},
    remote::RemoteTarget,
};

pub type Client = RuntimeClient<InterceptedService<Channel, Credentials>>;

/// The remote API key sent with every call; local socket clients send none.
#[derive(Clone, Default)]
pub struct Credentials(Option<MetadataValue<Ascii>>);

impl Interceptor for Credentials {
    fn call(&mut self, mut request: Request<()>) -> std::result::Result<Request<()>, Status> {
        if let Some(value) = &self.0 {
            request.metadata_mut().insert("authorization", value.clone());
        }
        Ok(request)
    }
}

/// Connects to a remote server's TLS listener and checks its protocol.
pub async fn connect_remote(target: &RemoteTarget) -> Result<Client> {
    let header = MetadataValue::try_from(format!("Bearer {}", target.key))
        .map_err(|_| Error::Remote("the API key contains invalid characters".to_owned()))?;
    let channel = target.channel().await.map_err(|error| remote_failure(target, &error))?;
    let mut client = RuntimeClient::with_interceptor(channel, Credentials(Some(header)));
    let health = client
        .health(HealthRequest {})
        .await
        .map_err(|status| remote_failure(target, &Error::Status(status)))?
        .into_inner();
    validate_protocol(&health.protocol_version)?;
    Ok(client)
}

/// A remote failure with its whole cause chain, which names TLS problems such
/// as a fingerprint mismatch that the transport error alone hides.
fn remote_failure(target: &RemoteTarget, error: &Error) -> Error {
    let mut detail = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !detail.contains(&text) {
            detail.push_str(": ");
            detail.push_str(&text);
        }
        source = cause.source();
    }
    Error::Remote(format!("cannot use {}: {detail}", target.address))
}

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
    let mut client = RuntimeClient::with_interceptor(channel, Credentials::default());
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

use tonic::Status;

use super::{RuntimeService, proto};
use crate::{
    error::Error,
    remote::{self, CertificateSource, Scope},
};

impl RuntimeService {
    pub(super) async fn remote_status(&self) -> Result<proto::RemoteAccessStatus, Status> {
        self.remote.status().await.map(convert).map_err(failure)
    }

    pub(super) async fn set_remote(
        &self,
        request: proto::SetRemoteAccessRequest,
    ) -> Result<proto::RemoteAccessStatus, Status> {
        let status = if request.enabled {
            let bind = Some(request.bind.as_str()).filter(|bind| !bind.is_empty());
            self.remote.enable(bind, self.clone()).await
        } else {
            self.remote.disable().await
        };
        status.map(convert).map_err(failure)
    }

    pub(super) fn issue_remote_key(
        &self,
        request: &proto::CreateRemoteKeyRequest,
    ) -> Result<proto::CreateRemoteKeyResponse, Status> {
        let scope = scope_from(request.scope)?;
        let (record, secret) = self.remote.create_key(&request.name, scope).map_err(failure)?;
        Ok(proto::CreateRemoteKeyResponse {
            key: Some(key(remote::KeyStatus { record, last_used_unix_ms: None })),
            secret,
        })
    }

    pub(super) fn withdraw_remote_key(
        &self,
        request: &proto::RevokeRemoteKeyRequest,
    ) -> Result<proto::RevokeRemoteKeyResponse, Status> {
        let revoked = self.remote.revoke_key(&request.id).map_err(failure)?;
        Ok(proto::RevokeRemoteKeyResponse { revoked })
    }
}

fn convert(status: remote::Status) -> proto::RemoteAccessStatus {
    proto::RemoteAccessStatus {
        bind: status.bind.unwrap_or_default(),
        listener: status.listening.map(|listening| proto::RemoteListener {
            address: listening.address.to_string(),
            certificate_fingerprint: listening.fingerprint.to_string(),
            certificate_source: certificate_source(listening.source).into(),
        }),
        error: status.error.unwrap_or_default(),
        keys: status.keys.into_iter().map(key).collect(),
        clients: status.clients.into_iter().map(client).collect(),
    }
}

fn key(status: remote::KeyStatus) -> proto::RemoteKey {
    let record = status.record;
    proto::RemoteKey {
        display_prefix: remote::display_prefix(&record.id),
        id: record.id,
        name: record.name,
        scope: scope_to(record.scope).into(),
        created_at_unix_ms: record.created_at_unix_ms,
        last_used_at_unix_ms: status.last_used_unix_ms,
    }
}

fn client(client: remote::Client) -> proto::RemoteClient {
    proto::RemoteClient {
        key_id: client.key_id,
        key_name: client.key_name,
        scope: scope_to(client.scope).into(),
        peer_address: client.peer.map(|peer| peer.to_string()).unwrap_or_default(),
        first_seen_unix_ms: client.first_seen_unix_ms,
        last_seen_unix_ms: client.last_seen_unix_ms,
        requests: client.requests,
    }
}

const fn scope_to(scope: Scope) -> proto::RemoteKeyScope {
    match scope {
        Scope::Use => proto::RemoteKeyScope::Use,
        Scope::Control => proto::RemoteKeyScope::Control,
    }
}

fn scope_from(value: i32) -> Result<Scope, Status> {
    match proto::RemoteKeyScope::try_from(value) {
        Ok(proto::RemoteKeyScope::Use) => Ok(Scope::Use),
        Ok(proto::RemoteKeyScope::Control) => Ok(Scope::Control),
        Ok(proto::RemoteKeyScope::Unspecified) | Err(_) => {
            Err(Status::invalid_argument("a key scope of use or control is required"))
        },
    }
}

const fn certificate_source(source: CertificateSource) -> proto::RemoteCertificateSource {
    match source {
        CertificateSource::SelfSigned => proto::RemoteCertificateSource::SelfSigned,
        CertificateSource::Configured => proto::RemoteCertificateSource::Configured,
    }
}

fn failure(error: Error) -> Status {
    match error {
        Error::Config(message) => Status::invalid_argument(message),
        Error::Remote(message) => Status::failed_precondition(message),
        other => Status::internal(other.to_string()),
    }
}

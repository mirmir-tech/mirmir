use serde::Serialize;

use super::presented_fingerprint;
use crate::{
    cli::{RemoteCommand, RemoteKeyCommand, with_port},
    error::Result,
    instance::Instance,
    output,
    rpc::{Client, proto},
};

pub async fn run(instance: &Instance, command: RemoteCommand) -> Result<()> {
    if let RemoteCommand::Fingerprint { address } = &command {
        return output::line(presented_fingerprint(&with_port(address)).await?.to_string());
    }
    let mut connection = instance.connect().await?;
    let reused = connection.reused();
    let reads = matches!(
        command,
        RemoteCommand::Status | RemoteCommand::Key { command: RemoteKeyCommand::List }
    );
    let result = execute(&mut connection.client, command).await;
    if result.is_ok() && !reused {
        // A server started for one command never opens the remote listener.
        output::diagnostic(if reads {
            "no server is running; the remote listener starts with `mirmir serve`"
        } else {
            "no server was running; the change applies when `mirmir serve` starts"
        })?;
    }
    result.and(connection.shutdown().await)
}

async fn execute(client: &mut Client, command: RemoteCommand) -> Result<()> {
    match command {
        RemoteCommand::Status => {
            let status = client.get_remote_access(proto::GetRemoteAccessRequest {}).await?;
            output::json(&StatusView::from(status.into_inner()))
        },
        RemoteCommand::Enable { bind } => enable(client, bind.unwrap_or_default()).await,
        RemoteCommand::Disable => {
            let request = proto::SetRemoteAccessRequest { enabled: false, bind: String::new() };
            let status = client.set_remote_access(request).await?.into_inner();
            output::json(&StatusView::from(status))
        },
        RemoteCommand::Key { command } => key(client, command).await,
        RemoteCommand::Fingerprint { address } => {
            output::line(presented_fingerprint(&with_port(&address)).await?.to_string())
        },
    }
}

async fn enable(client: &mut Client, bind: String) -> Result<()> {
    let request = proto::SetRemoteAccessRequest { enabled: true, bind };
    let mut status = client.set_remote_access(request).await?.into_inner();
    let mut created = None;
    if status.keys.is_empty() {
        let request = proto::CreateRemoteKeyRequest {
            name: "first-control-key".to_owned(),
            scope: proto::RemoteKeyScope::Control.into(),
        };
        created = Some(client.create_remote_key(request).await?.into_inner());
        status = client.get_remote_access(proto::GetRemoteAccessRequest {}).await?.into_inner();
    }
    output::json(&EnabledView {
        status: StatusView::from(status),
        created_key: created.map(|created| CreatedView {
            key: created.key.map(KeyView::from),
            secret: created.secret,
            note: "shown once; the server keeps only its digest",
        }),
    })
}

async fn key(client: &mut Client, command: RemoteKeyCommand) -> Result<()> {
    match command {
        RemoteKeyCommand::Create { name, scope } => {
            let scope = match scope {
                super::Scope::Use => proto::RemoteKeyScope::Use,
                super::Scope::Control => proto::RemoteKeyScope::Control,
            };
            let request = proto::CreateRemoteKeyRequest { name, scope: scope.into() };
            let created = client.create_remote_key(request).await?.into_inner();
            output::json(&CreatedView {
                key: created.key.map(KeyView::from),
                secret: created.secret,
                note: "shown once; the server keeps only its digest",
            })
        },
        RemoteKeyCommand::List => {
            let status = client.get_remote_access(proto::GetRemoteAccessRequest {}).await?;
            let keys: Vec<KeyView> =
                status.into_inner().keys.into_iter().map(KeyView::from).collect();
            output::json(&keys)
        },
        RemoteKeyCommand::Revoke { id } => {
            let request = proto::RevokeRemoteKeyRequest { id };
            let revoked = client.revoke_remote_key(request).await?.into_inner().revoked;
            output::json(&serde_json::json!({ "revoked": revoked }))
        },
    }
}

#[derive(Serialize)]
struct EnabledView {
    #[serde(flatten)]
    status: StatusView,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_key: Option<CreatedView>,
}

#[derive(Serialize)]
struct CreatedView {
    key: Option<KeyView>,
    secret: String,
    note: &'static str,
}

#[derive(Serialize)]
struct StatusView {
    enabled: bool,
    bind: Option<String>,
    listening: Option<String>,
    certificate_fingerprint: Option<String>,
    certificate: Option<&'static str>,
    error: Option<String>,
    keys: Vec<KeyView>,
    clients: Vec<ClientView>,
}

impl From<proto::RemoteAccessStatus> for StatusView {
    fn from(status: proto::RemoteAccessStatus) -> Self {
        let source = status.listener.as_ref().map(proto::RemoteListener::certificate_source);
        Self {
            enabled: !status.bind.is_empty(),
            bind: Some(status.bind).filter(|bind| !bind.is_empty()),
            listening: status.listener.as_ref().map(|listener| listener.address.clone()),
            certificate_fingerprint: status
                .listener
                .map(|listener| listener.certificate_fingerprint),
            certificate: source.map(|source| match source {
                proto::RemoteCertificateSource::Configured => "configured",
                _ => "self_signed",
            }),
            error: Some(status.error).filter(|error| !error.is_empty()),
            keys: status.keys.into_iter().map(KeyView::from).collect(),
            clients: status.clients.into_iter().map(ClientView::from).collect(),
        }
    }
}

#[derive(Serialize)]
struct KeyView {
    id: String,
    name: String,
    scope: &'static str,
    prefix: String,
    created_at_unix_ms: u64,
    last_used_at_unix_ms: Option<u64>,
}

impl From<proto::RemoteKey> for KeyView {
    fn from(key: proto::RemoteKey) -> Self {
        Self {
            scope: scope_name(key.scope()),
            id: key.id,
            name: key.name,
            prefix: key.display_prefix,
            created_at_unix_ms: key.created_at_unix_ms,
            last_used_at_unix_ms: key.last_used_at_unix_ms,
        }
    }
}

#[derive(Serialize)]
struct ClientView {
    key: String,
    scope: &'static str,
    peer: String,
    last_seen_unix_ms: u64,
    requests: u64,
}

impl From<proto::RemoteClient> for ClientView {
    fn from(client: proto::RemoteClient) -> Self {
        Self {
            scope: scope_name(client.scope()),
            key: client.key_name,
            peer: client.peer_address,
            last_seen_unix_ms: client.last_seen_unix_ms,
            requests: client.requests,
        }
    }
}

const fn scope_name(scope: proto::RemoteKeyScope) -> &'static str {
    match scope {
        proto::RemoteKeyScope::Use => "use",
        proto::RemoteKeyScope::Control => "control",
        proto::RemoteKeyScope::Unspecified => "unspecified",
    }
}

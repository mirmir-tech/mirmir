//! The TLS identity of the remote listener: a configured certificate, or a
//! self-signed one generated once in the state directory. Clients pin the
//! self-signed certificate by its SHA-256 fingerprint.

mod fingerprint;

use std::{fs, path::Path};

pub use fingerprint::Fingerprint;

use super::lock::FileLock;
use crate::{
    config::{RemoteCertificate, ServerSettings, ensure_private_file, write_text},
    error::Result,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateSource {
    SelfSigned,
    Configured,
}

#[derive(Clone)]
pub struct Identity {
    pub certificate: String,
    pub private_key: String,
    pub fingerprint: Fingerprint,
    pub source: CertificateSource,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Identity")
            .field("fingerprint", &self.fingerprint)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

pub fn identity(state_dir: &Path, settings: &ServerSettings) -> Result<Identity> {
    match settings.remote_certificate()? {
        RemoteCertificate::Configured { certificate, key } => {
            read(certificate, key, CertificateSource::Configured, false)
        },
        RemoteCertificate::SelfSigned => self_signed(&state_dir.join("remote").join("tls")),
    }
}

/// Reads the generated pair, or generates it under a lock beside the
/// certificate so concurrent servers never write mismatched halves. The key
/// is written before the certificate, so a complete pair is never in flux,
/// and a half pair left by an interrupted generation is replaced.
fn self_signed(directory: &Path) -> Result<Identity> {
    let certificate = directory.join("certificate.pem");
    let key = directory.join("private-key.pem");
    if certificate.exists() && key.exists() {
        return read(&certificate, &key, CertificateSource::SelfSigned, true);
    }
    let _generating = FileLock::acquire(&certificate)?;
    if certificate.exists() && key.exists() {
        return read(&certificate, &key, CertificateSource::SelfSigned, true);
    }
    let generated = rcgen::generate_simple_self_signed(subject_names())?;
    write_text(&key, &generated.signing_key.serialize_pem(), true)?;
    write_text(&certificate, &generated.cert.pem(), false)?;
    tracing::info!(certificate = %certificate.display(), "generated a self-signed remote access certificate");
    read(&certificate, &key, CertificateSource::SelfSigned, true)
}

fn read(
    certificate: &Path,
    key: &Path,
    source: CertificateSource,
    owned: bool,
) -> Result<Identity> {
    if owned {
        ensure_private_file(key)?;
    }
    let certificate = fs::read_to_string(certificate)?;
    Ok(Identity {
        fingerprint: Fingerprint::of_pem(&certificate)?,
        private_key: fs::read_to_string(key)?,
        certificate,
        source,
    })
}

/// Names the certificate is valid for. Pinned clients ignore them; they help
/// clients that add the certificate to a trust store instead.
fn subject_names() -> Vec<String> {
    let mut names = vec!["localhost".to_owned(), "127.0.0.1".to_owned(), "::1".to_owned()];
    if let Some(host) = sysinfo::System::host_name().filter(|host| dns_label(host)) {
        let local = format!("{host}.local");
        if !host.contains('.') {
            names.push(local);
        }
        names.push(host);
    }
    names
}

fn dns_label(host: &str) -> bool {
    !host.is_empty()
        && host
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-.".contains(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_self_signed_certificate_once() -> Result<()> {
        let root = std::env::temp_dir().join(format!("mirmir-tls-{}", std::process::id()));
        drop(fs::remove_dir_all(&root));
        let settings = ServerSettings::default();
        let first = identity(&root, &settings)?;
        assert_eq!(first.source, CertificateSource::SelfSigned);
        let second = identity(&root, &settings)?;
        assert_eq!(first.fingerprint, second.fingerprint);
        assert!(!format!("{second:?}").contains("PRIVATE KEY"));
        let configured = ServerSettings {
            remote_tls_cert: Some(root.join("remote/tls/certificate.pem")),
            remote_tls_key: Some(root.join("remote/tls/private-key.pem")),
            ..ServerSettings::default()
        };
        let reused = identity(&root, &configured)?;
        assert_eq!(reused.source, CertificateSource::Configured);
        assert_eq!(reused.fingerprint, first.fingerprint);
        let half = ServerSettings { remote_tls_key: None, ..configured };
        let rejection = identity(&root, &half).expect_err("a half pair must not start TLS");
        assert!(
            rejection.to_string().contains("server.remote_tls_key is not set"),
            "{rejection}"
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn concurrent_servers_generate_one_matching_pair() -> Result<()> {
        let root = std::env::temp_dir().join(format!("mirmir-tls-race-{}", std::process::id()));
        drop(fs::remove_dir_all(&root));
        let settings = ServerSettings::default();
        let identities = std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for _ in 0..8 {
                workers.push(scope.spawn(|| identity(&root, &settings)));
            }
            workers
                .into_iter()
                .map(|worker| worker.join().expect("worker"))
                .collect::<Result<Vec<_>>>()
        })?;
        let first = &identities[0];
        for identity in &identities {
            assert_eq!(identity.fingerprint, first.fingerprint);
            assert_eq!(identity.private_key, first.private_key);
        }

        let tls = root.join("remote/tls");
        fs::remove_file(tls.join("private-key.pem"))?;
        let replaced = identity(&root, &settings)?;
        assert_ne!(replaced.fingerprint, first.fingerprint, "a half pair is replaced");
        assert_eq!(identity(&root, &settings)?.private_key, replaced.private_key);
        fs::remove_dir_all(root)?;
        Ok(())
    }
}

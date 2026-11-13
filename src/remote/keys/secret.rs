use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::error::{Error, Result};

/// Every presented key reads `mmk_<id>_<secret>`. The id is public and shown
/// in key lists; only the SHA-256 of the secret is stored.
const PREFIX: &str = "mmk_";
const ID_BYTES: usize = 8;
const SECRET_BYTES: usize = 32;

/// A newly created key: its public id, the full value shown once, and the
/// digest the server keeps.
pub struct Generated {
    pub id: String,
    pub presented: String,
    pub digest: String,
}

pub fn generate() -> Result<Generated> {
    let mut id = [0_u8; ID_BYTES];
    let mut secret = [0_u8; SECRET_BYTES];
    fill(&mut id)?;
    fill(&mut secret)?;
    let id = hex(&id);
    let secret = URL_SAFE_NO_PAD.encode(secret);
    Ok(Generated {
        presented: format!("{PREFIX}{id}_{secret}"),
        digest: digest(&secret),
        id,
    })
}

/// Splits a presented key into its id and secret.
#[must_use]
pub fn parse(presented: &str) -> Option<(&str, &str)> {
    let rest = presented.strip_prefix(PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    let valid_id = id.len() == ID_BYTES * 2 && id.bytes().all(|byte| byte.is_ascii_hexdigit());
    (valid_id && !secret.is_empty()).then_some((id, secret))
}

#[must_use]
pub fn display_prefix(id: &str) -> String {
    format!("{PREFIX}{id}")
}

#[must_use]
pub fn digest(secret: &str) -> String {
    hex(&Sha256::digest(secret.as_bytes()))
}

/// Compares a presented secret with a stored digest in constant time.
#[must_use]
pub fn matches(secret: &str, stored: &str) -> bool {
    digest(secret).as_bytes().ct_eq(stored.as_bytes()).into()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _infallible = write!(text, "{byte:02x}");
        text
    })
}

fn fill(bytes: &mut [u8]) -> Result<()> {
    getrandom::fill(bytes)
        .map_err(|error| Error::Remote(format!("cannot generate an API key: {error}")))
}

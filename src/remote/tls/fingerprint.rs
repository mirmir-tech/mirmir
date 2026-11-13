use std::{fmt, str::FromStr};

use rustls::pki_types::{CertificateDer, pem::PemObject};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// The SHA-256 of a certificate's DER encoding, the value a client pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    #[must_use]
    pub fn of_der(der: &[u8]) -> Self {
        Self(Sha256::digest(der).into())
    }

    /// The fingerprint of the first certificate in a PEM chain: the leaf.
    pub fn of_pem(pem: &str) -> Result<Self> {
        let leaf = CertificateDer::pem_slice_iter(pem.as_bytes())
            .next()
            .ok_or_else(|| Error::Remote("the certificate file holds no certificate".to_owned()))?
            .map_err(|error| Error::Remote(format!("cannot read the certificate: {error}")))?;
        Ok(Self::of_der(&leaf))
    }
}

/// Colon-separated uppercase hex, as browsers and `openssl x509 -fingerprint`
/// show it.
impl fmt::Display for Fingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str(":")?;
            }
            write!(formatter, "{byte:02X}")?;
        }
        Ok(())
    }
}

/// Accepts the displayed form, plain hex, any case and an optional `SHA256:`
/// or `sha256/` prefix.
impl FromStr for Fingerprint {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        let body = ["SHA256:", "sha256:", "SHA256/", "sha256/"]
            .iter()
            .find_map(|prefix| trimmed.strip_prefix(prefix))
            .unwrap_or(trimmed);
        let digits: String =
            body.chars().filter(|character| !matches!(character, ':' | ' ' | '-')).collect();
        let invalid =
            || Error::Remote(format!("`{input}` is not a SHA-256 certificate fingerprint"));
        if digits.len() != 64 {
            return Err(invalid());
        }
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let pair = digits.get(index * 2..index * 2 + 2).ok_or_else(invalid)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
        }
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_displayed_form_and_accepts_variants() -> Result<()> {
        let fingerprint = Fingerprint::of_der(b"certificate");
        let shown = fingerprint.to_string();
        assert_eq!(shown.len(), 32 * 3 - 1);
        assert_eq!(shown.parse::<Fingerprint>()?, fingerprint);
        assert_eq!(shown.replace(':', "").to_lowercase().parse::<Fingerprint>()?, fingerprint);
        assert_eq!(format!("SHA256:{shown}").parse::<Fingerprint>()?, fingerprint);
        assert!("AB:CD".parse::<Fingerprint>().is_err());
        assert!("Z".repeat(64).parse::<Fingerprint>().is_err());
        Ok(())
    }
}

use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use thiserror::Error;

pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
pub const DEFAULT_IMAGE_REQUEST_BYTES: usize = 28 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct AttachedImage {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct MediaError(String);

pub fn decode_data_url(url: &str) -> Result<Vec<u8>, MediaError> {
    let data = url
        .strip_prefix("data:")
        .ok_or_else(|| invalid("only data: image URLs are supported"))?;
    let (metadata, payload) =
        data.split_once(',').ok_or_else(|| invalid("image data URL is malformed"))?;
    let mut fields = metadata.split(';');
    let declared_mime = fields.next().unwrap_or_default();
    if !supported_mime(declared_mime) || !fields.any(|field| field.eq_ignore_ascii_case("base64")) {
        return Err(invalid("image must be base64 JPEG, PNG, WebP, or GIF data"));
    }
    if payload.len() > MAX_IMAGE_BYTES.saturating_mul(4).div_ceil(3) + 4 {
        return Err(invalid("image exceeds the 20 MiB limit"));
    }
    let Ok(decoded) = STANDARD.decode(payload) else {
        return Err(invalid("image contains invalid base64"));
    };
    validate_image(&decoded, Some(declared_mime))?;
    Ok(decoded)
}

pub fn read_image(path: &Path) -> Result<AttachedImage, MediaError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(invalid(format!("cannot inspect image {}: {error}", path.display())));
        },
    };
    if !metadata.is_file() {
        return Err(invalid("dropped path is not a file"));
    }
    if metadata.len() > u64::try_from(MAX_IMAGE_BYTES).unwrap_or(u64::MAX) {
        return Err(invalid("image exceeds the 20 MiB limit"));
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(invalid(format!("cannot read image {}: {error}", path.display())));
        },
    };
    validate_image(&bytes, None)?;
    let name = path
        .file_name()
        .map_or_else(|| "image".to_owned(), |name| name.to_string_lossy().into_owned());
    Ok(AttachedImage { name, bytes })
}

pub fn dropped_file_path(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')))
        .unwrap_or(value);
    let value = value.strip_prefix("file://").unwrap_or(value);
    let value = percent_decode(value)?;
    let path = PathBuf::from(shell_unescape(&value));
    path.is_file().then_some(path)
}

fn validate_image(bytes: &[u8], declared_mime: Option<&str>) -> Result<(), MediaError> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(invalid("image exceeds the 20 MiB limit"));
    }
    let detected = detected_mime(bytes).ok_or_else(|| invalid("file is not a supported image"))?;
    if declared_mime.is_some_and(|declared| declared != detected) {
        return Err(invalid(format!(
            "image content does not match declared MIME type {declared_mime:?}"
        )));
    }
    Ok(())
}

fn detected_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn supported_mime(mime: &str) -> bool {
    matches!(mime, "image/jpeg" | "image/png" | "image/webp" | "image/gif")
}

fn shell_unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let encoded = bytes.get(index + 1..index + 3)?;
            let encoded = std::str::from_utf8(encoded).ok()?;
            output.push(u8::from_str_radix(encoded, 16).ok()?);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn invalid(message: impl Into<String>) -> MediaError {
    MediaError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_png_data_url_and_rejects_mismatched_mime() {
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        assert!(decode_data_url(&format!("data:image/png;base64,{png}")).is_ok());
        assert!(decode_data_url(&format!("data:image/jpeg;base64,{png}")).is_err());
    }

    #[test]
    fn unescapes_a_dropped_file_path() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("mirmir dropped {}", std::process::id()));
        std::fs::write(&root, b"GIF89a")?;
        let escaped = root.display().to_string().replace(' ', "\\ ");
        assert_eq!(dropped_file_path(&escaped).as_deref(), Some(root.as_path()));
        std::fs::remove_file(root)?;
        Ok(())
    }
}

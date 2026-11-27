//! A stable configuration id for a model folder no repository names.

use std::{ffi::OsStr, path::Path};

use sha2::{Digest, Sha256};

/// The id a model folder's configuration is saved under: its name, kept to
/// the characters a model key allows, and a short digest of its canonical
/// path, so folders with the same name stay apart and the same folder always
/// gets the same id.
#[must_use]
pub fn folder_key(path: &Path) -> String {
    named_key(path.file_name(), path)
}

/// The id of `path` under the readable `name`, such as a file's stem: the
/// digest is still of the whole canonical path, so `Foo.gguf` and a sibling
/// folder `Foo` get different ids.
#[must_use]
pub fn named_key(name: Option<&OsStr>, path: &Path) -> String {
    let name = name
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || "-_.".contains(character) {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let name = name.trim_matches('.');
    let digest = Sha256::digest(path.as_os_str().as_encoded_bytes());
    let suffix = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    if name.is_empty() {
        format!("model-{suffix}")
    } else {
        format!("{name}-{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model_key;

    #[test]
    fn a_folder_gets_a_valid_stable_id_apart_from_its_namesakes() -> crate::error::Result<()> {
        let first = folder_key(Path::new("/Users/someone/Models/Qwen3 0.6B (MLX)"));
        let again = folder_key(Path::new("/Users/someone/Models/Qwen3 0.6B (MLX)"));
        let namesake = folder_key(Path::new("/Volumes/Other/Qwen3 0.6B (MLX)"));
        assert_eq!(first, again);
        assert_ne!(first, namesake);
        assert!(first.starts_with("Qwen3-0.6B--MLX-"));
        assert_eq!(model_key(&first)?, first);
        assert_eq!(model_key(&folder_key(Path::new("/..")))?, folder_key(Path::new("/..")));
        Ok(())
    }
}

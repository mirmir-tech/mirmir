mod fixture;
mod imports;

use std::{fs, path::PathBuf};

use fixture::Metadata;

use super::*;
use crate::config::Paths;

fn root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("mirmir-gguf-{label}-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("temporary directory");
    root
}

/// A GGUF v3 file of no tensors whose one metadata key names `architecture`.
fn header(path: &Path, architecture: &str) -> std::io::Result<()> {
    Metadata::default().text("general.architecture", architecture).write(path)
}

#[test]
fn a_gguf_file_is_its_own_text_model() -> Result<()> {
    let root = root("file");
    let file = root.join("Qwen3-8B-Q4_K_M.gguf");
    fs::write(&file, b"")?;
    assert_eq!(source(&file)?, Some(Source { text: file.clone(), vision: None }));
    let other = root.join("model.safetensors");
    fs::write(&other, b"")?;
    assert_eq!(source(&other)?, None);
    Ok(())
}

#[test]
fn a_directory_of_gguf_files_pairs_the_model_with_its_vision_tower() -> Result<()> {
    let root = root("directory");
    // The projector is told by its header, though it sorts first and Google
    // names it after the model.
    header(&root.join("a-gemma-4-E4B-it-mmproj.gguf"), libmir::gguf::PROJECTOR_ARCHITECTURE)?;
    header(&root.join("gemma-4-E4B-it-Q4_K_M.gguf"), "gemma4")?;
    assert_eq!(
        source(&root)?,
        Some(Source {
            text: root.join("gemma-4-E4B-it-Q4_K_M.gguf"),
            vision: Some(root.join("a-gemma-4-E4B-it-mmproj.gguf")),
        })
    );
    // A checkpoint directory, or one importing to itself, loads as it is.
    fs::write(root.join("config.json"), b"{}")?;
    assert_eq!(source(&root)?, None);
    Ok(())
}

/// An import made under a dotted directory's truncated name moves to its
/// whole name, and is dropped when that import exists; another source's
/// import under the old key stays.
#[cfg(unix)]
#[test]
fn an_import_under_the_old_key_is_adopted_or_removed() -> Result<()> {
    let root = root("legacy");
    let store =
        Store::new(Paths::from_roots(root.join("config"), root.join("state"), &root.join("run")));
    let model = root.join("models/Qwen2.5-7B-GGUF");
    fs::create_dir_all(&model)?;
    header(&model.join("qwen2.5-7b-q4_k_m.gguf"), "qwen2")?;
    let model = fs::canonicalize(&model)?;
    let text = model.join("qwen2.5-7b-q4_k_m.gguf");
    let source = Source { text: text.clone(), vision: None };
    let legacy = store.paths.gguf_imports_dir.join(folder_key(&model.with_extension("")));
    let directory = store.gguf_import_dir(&model);
    assert_ne!(legacy, directory);
    let old_import = |target: &Path| -> Result<()> {
        fs::create_dir_all(&legacy)?;
        fs::write(legacy.join("config.json"), b"{}")?;
        std::os::unix::fs::symlink(target, legacy.join(text.file_name().unwrap_or_default()))?;
        Ok(())
    };
    old_import(&text)?;
    adopt(&legacy, &directory, &source)?;
    assert!(!legacy.exists() && directory.join("config.json").exists());
    old_import(&text)?;
    adopt(&legacy, &directory, &source)?;
    assert!(!legacy.exists() && directory.exists());
    // `models/Qwen2.gguf` imports under the same old key.
    fs::remove_dir_all(&directory)?;
    let other = root.join("models/Qwen2.gguf");
    header(&other, "qwen2")?;
    fs::create_dir_all(&legacy)?;
    std::os::unix::fs::symlink(&other, legacy.join("Qwen2.gguf"))?;
    adopt(&legacy, &directory, &source)?;
    assert!(legacy.exists() && !directory.exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

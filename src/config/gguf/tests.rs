use std::{fs, path::PathBuf, time::Duration};

use super::*;

fn root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("mirmir-gguf-{label}-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("temporary directory");
    root
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
    fs::write(root.join("gemma-4-26B-A4B-it-Q4_K_M.gguf"), b"")?;
    fs::write(root.join("mmproj-F16.gguf"), b"")?;
    assert_eq!(
        source(&root)?,
        Some(Source {
            text: root.join("gemma-4-26B-A4B-it-Q4_K_M.gguf"),
            vision: Some(root.join("mmproj-F16.gguf")),
        })
    );
    // A checkpoint directory, or one importing to itself, loads as it is.
    fs::write(root.join("config.json"), b"{}")?;
    assert_eq!(source(&root)?, None);
    Ok(())
}

#[test]
fn an_import_older_than_its_source_is_refreshed() -> Result<()> {
    let root = root("current");
    let file = root.join("model.gguf");
    fs::write(&file, b"")?;
    let import = root.join("import");
    let source = Source { text: file.clone(), vision: None };
    assert!(!current(&import, &source)?);
    fs::create_dir_all(&import)?;
    fs::write(import.join("config.json"), b"{}")?;
    assert!(current(&import, &source)?);
    let later = SystemTime::now() + Duration::from_secs(60);
    fs::File::options().write(true).open(&file)?.set_modified(later)?;
    assert!(!current(&import, &source)?);
    Ok(())
}

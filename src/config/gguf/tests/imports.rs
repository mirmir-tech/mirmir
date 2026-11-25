//! Whole imports through the store: keys, completion and freshness.

use std::{fs, path::Path, time::Duration};

use libmir::gguf::{GgufModel, Import, ImportManifest};

use super::{Metadata, root};
use crate::{
    config::{
        Paths, Store,
        gguf::{LISTING, current, imports, listing},
    },
    error::Result,
};

fn store(root: &Path) -> Store {
    Store::new(Paths::from_roots(root.join("config"), root.join("state"), &root.join("run")))
}

fn later(path: &Path) -> std::io::Result<()> {
    let later = std::time::SystemTime::now() + Duration::from_secs(60);
    fs::File::options().write(true).open(path)?.set_modified(later)
}

/// An import missing a shard's link, as an interrupted import leaves it, is
/// imported again, as is one older than its second shard alone.
#[test]
fn every_shard_completes_and_freshens_an_import() -> Result<()> {
    let root = root("shards");
    let model = root.join("models/Split-GGUF");
    fs::create_dir_all(&model)?;
    let shards = [1, 2].map(|number| model.join(format!("split-{number:05}-of-00002.gguf")));
    Metadata::llama().shard(1, 2).write(&shards[0])?;
    Metadata::default().shard(2, 2).write(&shards[1])?;
    let model = fs::canonicalize(&model)?;
    let shards = shards.map(|shard| model.join(shard.file_name().unwrap_or_default()));
    let store = store(&root);
    let directory = store.gguf_import(&model)?.unwrap_or_default();
    assert!(shards.iter().all(|shard| imports(&directory, shard)));
    fs::remove_file(directory.join(shards[1].file_name().unwrap_or_default()))?;
    assert_eq!(store.gguf_import(&model)?, Some(directory.clone()));
    assert!(imports(&directory, &shards[1]));
    let imported = || fs::metadata(directory.join("config.json")).and_then(|meta| meta.modified());
    later(&shards[1])?;
    let stale = imported()?;
    store.gguf_import(&model)?;
    assert!(imported()? > stale);
    assert!(shards.iter().all(|shard| imports(&directory, shard)));
    fs::remove_dir_all(root)?;
    Ok(())
}

/// `Foo.gguf` and a sibling folder `Foo` import apart, in either order, each
/// linking its own model.
#[test]
fn a_file_and_its_namesake_folder_import_apart() -> Result<()> {
    let root = root("namesakes");
    let models = root.join("models");
    fs::create_dir_all(models.join("Foo"))?;
    Metadata::llama().write(&models.join("Foo.gguf"))?;
    Metadata::llama()
        .text("general.name", "folder")
        .write(&models.join("Foo/foo.gguf"))?;
    let models = fs::canonicalize(&models)?;
    let (file, folder) = (models.join("Foo.gguf"), models.join("Foo"));
    let texts = [file.clone(), folder.join("foo.gguf")];
    for (label, order) in [("file-first", [0, 1]), ("folder-first", [1, 0])] {
        let store = store(&root.join(label));
        let sources = [&file, &folder];
        let mut directories = [None, None];
        for index in order {
            directories[index] = store.gguf_import(sources[index])?;
        }
        let [of_file, of_folder] = directories.map(Option::unwrap_or_default);
        assert_ne!(of_file, of_folder);
        assert!(imports(&of_file, &texts[0]) && imports(&of_folder, &texts[1]));
        assert_eq!(store.gguf_import(&file)?, Some(of_file));
        assert_eq!(store.gguf_import(&folder)?, Some(of_folder));
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A lost generated file is written again on the next lookup.
#[test]
fn an_import_missing_a_generated_file_is_redone() -> Result<()> {
    let root = root("generated");
    let model = root.join("model.gguf");
    Metadata::llama().write(&model)?;
    let model = fs::canonicalize(&model)?;
    let store = store(&root);
    let directory = store.gguf_import(&model)?.unwrap_or_default();
    fs::remove_file(directory.join("tokenizer.json"))?;
    assert!(!current(&directory, &model));
    assert_eq!(store.gguf_import(&model)?, Some(directory.clone()));
    assert!(directory.join("tokenizer.json").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A folder whose projector is gone is a text model now: its multimodal
/// import is redone without the vision tower.
#[test]
fn a_folder_losing_its_projector_imports_text_only() -> Result<()> {
    let root = root("projector");
    let folder = root.join("models/Vision-GGUF");
    fs::create_dir_all(&folder)?;
    Metadata::llama().write(&folder.join("model.gguf"))?;
    fs::write(folder.join("mmproj.gguf"), b"projector")?;
    let folder = fs::canonicalize(&folder)?;
    let store = store(&root);
    let directory = store.gguf_import_dir(&folder);
    // As the multimodal import published it, without deriving the tower.
    let publish = || -> libmir::Result<()> {
        let mut import = Import::derive(&GgufModel::open(folder.join("model.gguf"))?)?;
        import.vision_shards.push(folder.join("mmproj.gguf"));
        Ok(import.publish(&directory)?)
    };
    publish()?;
    fs::write(directory.join(LISTING), serde_json::to_vec(&listing(&folder)?)?)?;
    assert!(current(&directory, &folder));
    fs::remove_file(folder.join("mmproj.gguf"))?;
    assert!(!current(&directory, &folder));
    assert_eq!(store.gguf_import(&folder)?, Some(directory.clone()));
    let manifest = ImportManifest::read(&directory)
        .ok_or_else(|| crate::error::Error::Config("no manifest".to_owned()))?;
    assert!(manifest.vision.is_empty() && !directory.join("mmproj.gguf").exists());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A current import is found again from names and metadata alone: with its
/// sources unreadable the lookup still succeeds, and once a source changes
/// it is imported again.
#[cfg(unix)]
#[test]
fn a_current_import_is_found_without_reading_headers() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = root("headers");
    let folder = root.join("models/Split-GGUF");
    fs::create_dir_all(&folder)?;
    let shards = [1, 2].map(|number| folder.join(format!("split-{number:05}-of-00002.gguf")));
    Metadata::llama().shard(1, 2).write(&shards[0])?;
    Metadata::default().shard(2, 2).write(&shards[1])?;
    let folder = fs::canonicalize(&folder)?;
    let store = store(&root);
    let directory = store.gguf_import(&folder)?.unwrap_or_default();
    let mode = |mode| {
        shards
            .iter()
            .try_for_each(|shard| fs::set_permissions(shard, fs::Permissions::from_mode(mode)))
    };
    mode(0o000)?;
    let unread = store.gguf_import(&folder);
    mode(0o644)?;
    assert_eq!(unread?, Some(directory.clone()));
    let imported = || fs::metadata(directory.join("import.json")).and_then(|meta| meta.modified());
    let before = imported()?;
    later(&shards[1])?;
    store.gguf_import(&folder)?;
    assert!(imported()? > before);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// An unreadable GGUF file beside the text model is skipped; a folder with
/// no readable text model reports why.
#[cfg(unix)]
#[test]
fn an_unreadable_gguf_file_does_not_block_the_folder() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = root("unreadable");
    let folder = root.join("models/Mixed-GGUF");
    fs::create_dir_all(&folder)?;
    Metadata::llama().write(&folder.join("model.gguf"))?;
    let broken = folder.join("broken.gguf");
    fs::write(&broken, b"GGUF")?;
    fs::set_permissions(&broken, fs::Permissions::from_mode(0o000))?;
    let folder = fs::canonicalize(&folder)?;
    let store = store(&root);
    let imported = store.gguf_import(&folder);
    let linked = imported
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .is_some_and(|directory| imports(directory, &folder.join("model.gguf")));
    fs::remove_file(folder.join("model.gguf"))?;
    let alone = super::super::source(&folder);
    fs::set_permissions(&broken, fs::Permissions::from_mode(0o644))?;
    imported?;
    assert!(linked);
    assert!(alone.is_err());
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A legacy import that cannot be removed stays behind without failing the
/// lookup.
#[cfg(unix)]
#[test]
fn a_stuck_legacy_import_does_not_block_loading() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = root("stuck-legacy");
    let model = root.join("models/Qwen2.5-GGUF");
    fs::create_dir_all(&model)?;
    Metadata::llama().write(&model.join("model.gguf"))?;
    let model = fs::canonicalize(&model)?;
    let store = store(&root);
    let directory = store.gguf_import(&model)?.unwrap_or_default();
    let legacy = store
        .paths
        .gguf_imports_dir
        .join(crate::config::folder_key(&model.with_extension("")));
    fs::create_dir_all(legacy.join("locked"))?;
    fs::write(legacy.join("locked/file"), b"")?;
    std::os::unix::fs::symlink(model.join("model.gguf"), legacy.join("model.gguf"))?;
    fs::set_permissions(legacy.join("locked"), fs::Permissions::from_mode(0o555))?;
    fs::remove_file(directory.join("import.json"))?;
    let found = store.gguf_import(&model);
    fs::set_permissions(legacy.join("locked"), fs::Permissions::from_mode(0o755))?;
    assert_eq!(found?, Some(directory));
    fs::remove_dir_all(root)?;
    Ok(())
}

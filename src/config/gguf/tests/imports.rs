//! Whole imports through the store: keys, completion and freshness.

use std::{fs, path::Path, time::Duration};

use super::{Metadata, root};
use crate::{
    config::{Paths, Store, gguf::imports},
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

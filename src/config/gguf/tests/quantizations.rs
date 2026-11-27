//! A directory holding several quantizations of one model imports the
//! first and finds that import again until the files it was chosen from
//! change.

use std::fs;

use super::{Metadata, root};
use crate::{
    config::{Paths, Store, gguf::current},
    error::Result,
};

/// A second lookup of an unchanged two-quantization directory reuses the
/// first import; a change to the chosen source imports it again.
#[test]
fn an_unchanged_directory_of_quantizations_reuses_its_import() -> Result<()> {
    let root = root("quantizations");
    let folder = root.join("models/Model-GGUF");
    fs::create_dir_all(&folder)?;
    for name in ["m-Q4.gguf", "m-Q8.gguf"] {
        Metadata::llama().write(&folder.join(name))?;
    }
    let folder = fs::canonicalize(&folder)?;
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let store = Store::new(paths);
    let directory = store.gguf_import(&folder)?.unwrap_or_default();
    assert!(current(&directory, &folder));
    // A republish replaces the directory whole, taking this with it.
    let marker = directory.join("marker");
    fs::write(&marker, b"")?;
    assert_eq!(store.gguf_import(&folder)?, Some(directory.clone()));
    assert!(marker.exists(), "the unchanged directory was imported again");

    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(folder.join("m-Q4.gguf"))?
        .set_modified(later)?;
    assert!(!current(&directory, &folder));
    assert_eq!(store.gguf_import(&folder)?, Some(directory.clone()));
    assert!(!marker.exists() && current(&directory, &folder));
    fs::remove_dir_all(root)?;
    Ok(())
}

/// A directory's model linked to another file of the same size and time
/// imports again, and the checkpoint then loads the new file.
#[cfg(unix)]
#[test]
fn a_link_turned_to_another_file_imports_it() -> Result<()> {
    let root = root("relinked");
    let (store_dir, folder) = (root.join("store"), root.join("models/Linked-GGUF"));
    fs::create_dir_all(&store_dir)?;
    fs::create_dir_all(&folder)?;
    let (first, second) = (store_dir.join("a.gguf"), store_dir.join("b.gguf"));
    Metadata::llama().write(&first)?;
    Metadata::llama().write(&second)?;
    let modified = fs::metadata(&first)?.modified()?;
    fs::File::options().write(true).open(&second)?.set_modified(modified)?;
    let link = folder.join("model.gguf");
    std::os::unix::fs::symlink(&first, &link)?;
    let folder = fs::canonicalize(&folder)?;
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let store = Store::new(paths);
    let loads = |store: &Store| -> Result<Vec<std::path::PathBuf>> {
        let checkpoint = store.checkpoint(&folder)?;
        Ok(libmir::gguf::files_in(&checkpoint)
            .map_err(libmir::Error::from)?
            .iter()
            .map(fs::canonicalize)
            .collect::<std::io::Result<_>>()?)
    };
    assert_eq!(loads(&store)?, [fs::canonicalize(&first)?]);
    fs::remove_file(&link)?;
    std::os::unix::fs::symlink(&second, &link)?;
    assert_eq!(loads(&store)?, [fs::canonicalize(&second)?]);
    fs::remove_dir_all(root)?;
    Ok(())
}

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

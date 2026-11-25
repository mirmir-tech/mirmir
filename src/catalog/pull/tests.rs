use super::*;
use crate::config::{Paths, Store};

/// A pull registered under its repository also holds the variant it chose:
/// that row reports downloading, a second pull of it is refused, and both
/// are forgotten when the pull ends.
#[test]
fn a_chosen_variant_transfers_under_its_own_id() -> Result<()> {
    let root = std::env::temp_dir().join(format!("mirmir-transfer-{}", std::process::id()));
    let paths = Paths::from_roots(root.join("config"), root.join("state"), &root.join("run"));
    let catalog = Catalog::new(Store::new(paths));
    let repository = catalog.reserve("owner/model-GGUF")?;
    let chosen = catalog.reserve("owner/model-GGUF:Q8_0")?;
    assert!(catalog.is_transferring("owner/model-GGUF:Q8_0")?);
    assert!(catalog.reserve("owner/model-GGUF:Q8_0").is_err());
    drop((chosen, repository));
    assert!(!catalog.is_transferring("owner/model-GGUF:Q8_0")?);
    assert!(!catalog.is_transferring("owner/model-GGUF")?);
    Ok(())
}

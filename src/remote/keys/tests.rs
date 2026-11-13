use super::*;

fn store(name: &str) -> (KeyStore, PathBuf) {
    let root = std::env::temp_dir().join(format!("mirmir-keys-{name}-{}", std::process::id()));
    drop(fs::remove_dir_all(&root));
    (KeyStore::new(root.join("remote").join("keys.json")), root)
}

#[test]
fn creates_verifies_and_revokes_a_key() -> Result<()> {
    let (keys, root) = store("lifecycle");
    let (record, presented) = keys.create("laptop", Scope::Use)?;
    assert!(presented.starts_with(&display_prefix(&record.id)));
    assert_eq!(keys.verify(&presented)?, Some(record.clone()));
    let stored = fs::read_to_string(root.join("remote").join("keys.json"))?;
    let (_, secret_part) = secret::parse(&presented).expect("generated key parses");
    assert!(!stored.contains(secret_part), "the secret itself must not be stored");
    assert!(keys.revoke(&record.id)?);
    assert_eq!(keys.verify(&presented)?, None);
    assert!(!keys.revoke(&record.id)?);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn rejects_wrong_malformed_and_foreign_keys() -> Result<()> {
    let (keys, root) = store("reject");
    let (record, presented) = keys.create("ci", Scope::Control)?;
    let tampered = format!("{}x", &presented[..presented.len() - 1]);
    assert_eq!(keys.verify(&tampered)?, None);
    assert_eq!(keys.verify("")?, None);
    assert_eq!(keys.verify("mmk_short_secret")?, None);
    assert_eq!(keys.verify(&format!("{}_", display_prefix(&record.id)))?, None);
    let (other, other_root) = store("reject-other");
    assert_eq!(other.verify(&presented)?, None);
    fs::remove_dir_all(root)?;
    drop(fs::remove_dir_all(other_root));
    Ok(())
}

#[test]
fn rereads_a_key_file_changed_by_another_writer() -> Result<()> {
    let (server, root) = store("reload");
    let writer = KeyStore::new(root.join("remote").join("keys.json"));
    assert!(server.list()?.is_empty());
    let (record, presented) = writer.create("new", Scope::Use)?;
    assert_eq!(server.verify(&presented)?, Some(record.clone()));
    writer.revoke(&record.id)?;
    assert_eq!(server.verify(&presented)?, None);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn validates_key_names() {
    let (keys, root) = store("names");
    assert!(keys.create("  ", Scope::Use).is_err());
    assert!(keys.create("tab\there", Scope::Use).is_err());
    assert!(keys.create(&"x".repeat(MAX_NAME_CHARS + 1), Scope::Use).is_err());
    drop(fs::remove_dir_all(root));
}

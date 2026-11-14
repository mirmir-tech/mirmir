use std::{collections::BTreeSet, fs};

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

const WRITERS: usize = 32;

fn ids(records: &[KeyRecord]) -> BTreeSet<String> {
    records.iter().map(|record| record.id.clone()).collect()
}

#[test]
fn concurrent_creates_keep_every_key() -> Result<()> {
    let (keys, root) = store("concurrent-create");
    let shared = &keys;
    let created = std::thread::scope(|scope| {
        // Spawn every writer before joining any, so they all race.
        let mut workers = Vec::with_capacity(WRITERS);
        for index in 0..WRITERS {
            workers.push(scope.spawn(move || shared.create(&format!("key-{index}"), Scope::Use)));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().expect("writer thread"))
            .collect::<Vec<_>>()
    });
    let created = created.into_iter().collect::<Result<Vec<_>>>()?;
    let records = created.iter().map(|(record, _)| record.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&keys.list()?), ids(&records));
    assert_eq!(keys.list()?.len(), WRITERS);
    let reread = KeyStore::new(root.join("remote").join("keys.json"));
    assert_eq!(reread.list()?.len(), WRITERS);
    for (record, presented) in &created {
        assert_eq!(reread.verify(presented)?.as_ref(), Some(record));
    }
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn concurrent_creates_through_separate_stores_keep_every_key() -> Result<()> {
    let (first, root) = store("concurrent-stores");
    let path = root.join("remote").join("keys.json");
    let stores: Vec<_> = (0..WRITERS).map(|_| KeyStore::new(path.clone())).collect();
    std::thread::scope(|scope| {
        let workers: Vec<_> = stores
            .iter()
            .enumerate()
            .map(|(index, keys)| {
                scope.spawn(move || keys.create(&format!("key-{index}"), Scope::Use))
            })
            .collect();
        workers
            .into_iter()
            .try_for_each(|worker| worker.join().expect("writer thread").map(drop))
    })?;
    assert_eq!(first.list()?.len(), WRITERS);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn concurrent_create_and_revoke_lose_neither_change() -> Result<()> {
    let (keys, root) = store("concurrent-revoke");
    let doomed = (0..WRITERS)
        .map(|index| keys.create(&format!("old-{index}"), Scope::Control).map(|(record, _)| record))
        .collect::<Result<Vec<_>>>()?;
    let shared = &keys;
    let created = std::thread::scope(|scope| {
        let revokers: Vec<_> = doomed
            .iter()
            .map(|record| scope.spawn(move || shared.revoke(&record.id)))
            .collect();
        let mut creators = Vec::with_capacity(WRITERS);
        for index in 0..WRITERS {
            creators.push(scope.spawn(move || shared.create(&format!("new-{index}"), Scope::Use)));
        }
        for revoker in revokers {
            assert!(revoker.join().expect("revoker thread")?, "every old key is revoked once");
        }
        creators
            .into_iter()
            .map(|creator| creator.join().expect("creator thread").map(|(record, _)| record))
            .collect::<Result<Vec<_>>>()
    })?;
    assert_eq!(ids(&keys.list()?), ids(&created));
    let reread = KeyStore::new(root.join("remote").join("keys.json"));
    assert_eq!(ids(&reread.list()?), ids(&created));
    fs::remove_dir_all(root)?;
    Ok(())
}

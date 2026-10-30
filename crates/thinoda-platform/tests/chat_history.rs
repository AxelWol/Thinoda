use thinoda_platform::{Paths, chat_history::*, fs};

#[test]
fn simultaneous_native_clients_cannot_overwrite_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    let original = Store::load(&paths).unwrap();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let run = || {
            let mut next = original.clone();
            next.documents.push(history());
            barrier.wait();
            original.commit(&paths, next).is_ok()
        };
        let first = scope.spawn(run);
        let second = scope.spawn(run);
        assert_ne!(first.join().unwrap(), second.join().unwrap());
    });
    assert_eq!(Store::load(&paths).unwrap().documents.len(), 1);
}

#[test]
fn aggregate_thread_and_serialized_store_limits_refuse_growth_without_eviction() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    let initial = Store::load(&paths).unwrap();
    let mut next = initial.clone();
    let mut doc = history();
    doc.threads[0].messages = (0..32)
        .map(|i| Entry {
            assistant: i % 2 == 1,
            text: "x".repeat(8192),
        })
        .collect();
    next.documents.push(doc.clone());
    assert!(next.validate().is_ok());
    next.documents[0].threads[0].messages[1].text.push('x');
    assert!(next.validate().is_err());
    next.documents.clear();
    for i in 0..16 {
        let mut doc = doc.clone();
        doc.binding.path = format!("/fixture/{i}");
        next.documents.push(doc);
    }
    assert!(initial.commit(&paths, next).is_err());
    assert_eq!(Store::load(&paths).unwrap(), initial);
}

fn history() -> History {
    History {
        binding: Binding {
            path: "/fixture/map.thinoda".into(),
            digest: "a".repeat(64),
        },
        threads: vec![Thread {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Research".into(),
            updated: 100,
            messages: vec![
                Entry {
                    assistant: false,
                    text: "Authored prompt".into(),
                },
                Entry {
                    assistant: true,
                    text: "Completed answer".into(),
                },
            ],
        }],
    }
}

#[test]
fn default_load_does_not_create_any_files_and_roundtrip_is_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    let initial = Store::load(&paths).unwrap();
    assert!(!paths.data_dir.exists());
    let mut next = initial.clone();
    next.documents.push(history());
    let saved = initial.commit(&paths, next).unwrap();
    assert_eq!(Store::load(&paths).unwrap(), saved);
    let raw = std::fs::read_to_string(paths.data_dir.join("ai-conversations.json")).unwrap();
    for forbidden in [
        "api_key", "system", "context", "draft", "preview", "provider", "base_url",
    ] {
        assert!(!raw.contains(forbidden));
    }
}

#[test]
fn stale_writers_are_refused_including_an_aba_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    let original = Store::load(&paths).unwrap();
    let mut next = original.clone();
    next.documents.push(history());
    let saved = original.commit(&paths, next.clone()).unwrap();
    assert!(original.commit(&paths, next).is_err());
    let cleared = saved.commit(&paths, Store::default()).unwrap();
    assert!(cleared.documents.is_empty());
    assert!(original.commit(&paths, Store::default()).is_err());
    assert_eq!(Store::load(&paths).unwrap(), cleared);
}

#[test]
fn malformed_unsupported_and_oversized_storage_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let path = paths.data_dir.join("ai-conversations.json");
    for bytes in [
        b"not json".to_vec(),
        br#"{"version":2,"revision":"","retention_days":30,"documents":[]}"#.to_vec(),
        br#"{"version":1,"revision":"","retention_days":30,"documents":[],"api_key":"forbidden"}"#
            .to_vec(),
        vec![b' '; MAX_BYTES + 1],
    ] {
        std::fs::write(&path, &bytes).unwrap();
        assert!(Store::load(&paths).is_err());
        assert!(Store::default().commit(&paths, Store::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn validation_bounds_names_roles_counts_and_bytes() {
    let mut store = Store::default();
    store.documents.push(history());
    assert!(store.validate().is_ok());
    for name in ["", " ", " padded", "two\nlines", &"é".repeat(41)] {
        let mut bad = store.clone();
        bad.documents[0].threads[0].name = name.into();
        assert!(bad.validate().is_err(), "{name:?}");
    }
    let mut bad = store.clone();
    bad.documents[0].threads[0].messages[0].assistant = true;
    assert!(bad.validate().is_err());
    let mut bad = store.clone();
    bad.documents[0].threads[0].messages[0].text = "a".repeat(8193);
    assert!(bad.validate().is_err());
    let mut bad = store.clone();
    bad.documents[0].threads[0].messages.pop();
    assert!(bad.validate().is_err());
    let mut bad = store.clone();
    let mut duplicate = bad.documents[0].threads[0].clone();
    duplicate.id = uuid::Uuid::new_v4().to_string();
    duplicate.name = "RESEARCH".into();
    bad.documents[0].threads.push(duplicate);
    assert!(bad.validate().is_err());
    for limit in [MAX_THREADS + 1, MAX_DOCUMENTS + 1] {
        let mut bad = store.clone();
        if limit == MAX_THREADS + 1 {
            for i in 1..limit {
                let mut thread = bad.documents[0].threads[0].clone();
                thread.name = format!("Thread {i}");
                thread.id = uuid::Uuid::new_v4().to_string();
                bad.documents[0].threads.push(thread);
            }
        } else {
            for i in 1..limit {
                let mut doc = history();
                doc.binding.path = format!("/fixture/{i}");
                bad.documents.push(doc);
            }
        }
        assert!(bad.validate().is_err());
    }
}

#[test]
fn retention_is_explicit_and_boundary_inclusive() {
    let mut store = Store::default();
    store.retention_days = 7;
    store.documents.push(history());
    let boundary = 100 + 7 * 86_400;
    store.expire(boundary);
    assert_eq!(store.documents[0].threads.len(), 1);
    store.expire(boundary + 1);
    assert!(store.documents[0].threads.is_empty());
    store.retention_days = 1;
    assert!(store.validate().is_err());
}

#[test]
fn canonical_path_plus_digest_isolates_duplicates_moves_and_changed_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("map.thinoda");
    fs::atomic_write(&path, b"map bytes", false).unwrap();
    let binding = Binding::read(&path).unwrap();
    let alias = dir.path().join(".").join("map.thinoda");
    assert_eq!(binding, Binding::read(&alias).unwrap());
    let copy = dir.path().join("duplicate.thinoda");
    std::fs::copy(&path, &copy).unwrap();
    assert_ne!(binding, Binding::read(&copy).unwrap());
    let moved = dir.path().join("moved.thinoda");
    std::fs::rename(&path, &moved).unwrap();
    assert_ne!(binding, Binding::read(&moved).unwrap());
    fs::atomic_write(&path, b"unrelated", false).unwrap();
    assert_ne!(binding, Binding::read(&path).unwrap());
}

#[test]
fn failed_storage_write_preserves_previous_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::from_root(dir.path());
    let initial = Store::load(&paths).unwrap();
    let mut next = initial.clone();
    next.documents.push(history());
    let saved = initial.commit(&paths, next).unwrap();
    let lock = paths.data_dir.join("ai-conversations.lock");
    std::fs::remove_file(&lock).unwrap();
    std::fs::create_dir(&lock).unwrap();
    let mut deletion = saved.clone();
    deletion.documents.clear();
    assert!(saved.commit(&paths, deletion).is_err());
    assert_eq!(Store::load(&paths).unwrap(), saved);
}

#[test]
fn virtual_storage_reads_are_bounded_and_do_not_need_a_native_filesystem() {
    let vfs = thinoda_platform::vfs::Vfs::memory();
    let path = std::path::Path::new("/virtual/map.thinoda");
    vfs.write(path, b"map bytes", 1).unwrap();
    assert_eq!(vfs.read_bounded(path, 9).unwrap(), b"map bytes");
    assert!(vfs.read_bounded(path, 8).is_err());
    assert!(
        vfs.read_bounded(std::path::Path::new("/virtual/missing"), 9)
            .is_err()
    );
    assert_eq!(
        thinoda_platform::vfs::normalize(std::path::Path::new("/virtual/./child/../map.thinoda")),
        "/virtual/map.thinoda"
    );
}

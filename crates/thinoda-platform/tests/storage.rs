use std::{
    path::Path,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};
#[cfg(not(target_arch = "wasm32"))]
use thinoda_platform::storage::revision_from_fingerprint;

// Compile the private worker primitives with their deterministic chunk/commit
// hooks too. Integration tests otherwise cannot exercise crate-private hooks;
// this also keeps storage verification independent of unrelated lib fixtures.
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
#[path = "../src/fs.rs"]
mod worker_primitives;
use thinoda_platform::{
    fs,
    storage::{
        Availability, CancelToken, ExpectedRevision, LocalStorageProvider, PollWatcher, Revision,
        StorageError, StorageFuture, StorageId, StorageProvider, VfsStorageProvider,
    },
    vfs::{KvStore, MemoryStore, Vfs},
};

// These adapters intentionally complete synchronously when polled on a worker.
fn run<T>(mut future: StorageFuture<'_, T>) -> Result<T, StorageError> {
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("synchronous storage primitive unexpectedly yielded"),
    }
}

fn token() -> CancelToken {
    CancelToken::default()
}

fn memory(backup: bool) -> (Arc<Vfs>, VfsStorageProvider) {
    let vfs = Arc::new(Vfs::memory());
    let provider = VfsStorageProvider::new(vfs.clone(), backup);
    (vfs, provider)
}

#[test]
fn contract_is_object_safe_and_providers_are_send_sync() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<CancelToken>();
    send_sync::<LocalStorageProvider>();
    send_sync::<VfsStorageProvider>();
    let (_, provider) = memory(false);
    let object: &dyn StorageProvider = &provider;
    assert!(object.capabilities().revisions);
    assert!(object.capabilities().watch);
    assert!(!object.capabilities().share_links);
    let _remote = StorageError::Remote("test failure".into());
    #[cfg(not(target_arch = "wasm32"))]
    {
        fn send<T: Send>(_: T) {}
        let location = StorageId("/".into());
        send(object.list(&location, token()));
    }
}

#[test]
fn cancellation_is_shared_and_pre_cancelled_operations_have_no_effect() {
    let (vfs, provider) = memory(true);
    let cancel = token();
    cancel.clone().cancel();
    assert!(cancel.is_cancelled());
    assert!(matches!(cancel.check(), Err(StorageError::Cancelled)));
    let id = StorageId("/Documents/map.thinoda".into());
    assert!(matches!(
        run(provider.write(&id, b"new", ExpectedRevision::Missing, cancel.clone())),
        Err(StorageError::Cancelled)
    ));
    assert!(!vfs.exists(Path::new(&id.0)));
    assert!(matches!(
        run(provider.read(&id, cancel.clone())),
        Err(StorageError::Cancelled)
    ));
    assert!(matches!(
        run(provider.list(&StorageId("/".into()), cancel)),
        Err(StorageError::Cancelled)
    ));
}

#[test]
fn vfs_conditional_writes_backups_and_same_size_revision_changes() {
    let (vfs, provider) = memory(true);
    let id = StorageId("/Documents/map.thinoda".into());
    let first = run(provider.write(&id, b"first", ExpectedRevision::Missing, token())).unwrap();
    let loaded = run(provider.read(&id, token())).unwrap();
    assert_eq!(first, loaded.revision);
    assert_eq!(loaded.bytes, b"first");
    vfs.write(Path::new(&id.0), b"other", 1).unwrap();
    let external = run(provider.read(&id, token())).unwrap().revision;
    assert_ne!(first, external);
    let stale = run(provider.write(&id, b"stale", ExpectedRevision::Match(first), token()));
    assert!(matches!(stale, Err(StorageError::Conflict { actual: Some(r) }) if r == external));
    assert_eq!(vfs.read(Path::new(&id.0)).unwrap(), b"other");
    assert!(!vfs.exists(&fs::backup_path(Path::new(&id.0))));
    assert!(matches!(
        run(provider.write(&id, b"create", ExpectedRevision::Missing, token())),
        Err(StorageError::Conflict { actual: Some(_) })
    ));
    let saved =
        run(provider.write(&id, b"saved", ExpectedRevision::Match(external), token())).unwrap();
    assert_eq!(run(provider.read(&id, token())).unwrap().revision, saved);
    assert_eq!(
        vfs.read(&fs::backup_path(Path::new(&id.0))).unwrap(),
        b"other"
    );
    run(provider.write(&id, b"overwrite", ExpectedRevision::Any, token())).unwrap();
    assert_eq!(vfs.read(Path::new(&id.0)).unwrap(), b"overwrite");
}

#[test]
fn matching_deleted_file_is_a_conflict_and_does_not_recreate_it() {
    let (vfs, provider) = memory(true);
    let id = StorageId("/map".into());
    let revision = run(provider.write(&id, b"old", ExpectedRevision::Missing, token())).unwrap();
    vfs.remove_file(Path::new(&id.0)).unwrap();
    assert!(matches!(
        run(provider.write(&id, b"new", ExpectedRevision::Match(revision), token())),
        Err(StorageError::Conflict { actual: None })
    ));
    assert!(!vfs.exists(Path::new(&id.0)));
    assert!(!vfs.exists(&fs::backup_path(Path::new(&id.0))));
}

#[test]
fn vfs_listing_includes_implicit_directories_and_file_revisions() {
    let (vfs, provider) = memory(false);
    vfs.write(Path::new("/Documents/folder/map"), b"data", 1)
        .unwrap();
    vfs.write(Path::new("/Documents/plain"), b"local", 1)
        .unwrap();
    let entries = run(provider.list(&StorageId("/Documents".into()), token())).unwrap();
    assert_eq!(entries.len(), 2);
    let dir = entries.iter().find(|entry| entry.is_dir).unwrap();
    assert_eq!(dir.name, "folder");
    assert_eq!(dir.revision, None);
    let file = entries.iter().find(|entry| !entry.is_dir).unwrap();
    assert_eq!(file.availability, Availability::Local);
    assert_eq!(file.size, 5);
    assert_eq!(
        file.revision.as_ref(),
        Some(&run(provider.read(&file.id, token())).unwrap().revision)
    );
}

struct CancelAfterCommit {
    memory: MemoryStore,
    cancel: CancelToken,
}

impl KvStore for CancelAfterCommit {
    fn get(&self, key: &str) -> Option<String> {
        self.memory.get(key)
    }
    fn set(&self, key: &str, value: &str) -> std::io::Result<()> {
        self.memory.set(key, value)?;
        self.cancel.cancel();
        Ok(())
    }
    fn remove(&self, key: &str) {
        self.memory.remove(key);
    }
    fn keys(&self) -> Vec<String> {
        self.memory.keys()
    }
}

#[test]
fn cancellation_after_commit_returns_success_not_cancelled() {
    let cancel = token();
    let vfs = Arc::new(Vfs::new(Box::new(CancelAfterCommit {
        memory: MemoryStore::default(),
        cancel: cancel.clone(),
    })));
    let provider = VfsStorageProvider::new(vfs.clone(), false);
    let revision = run(provider.write(
        &StorageId("/map".into()),
        b"saved",
        ExpectedRevision::Missing,
        cancel.clone(),
    ))
    .unwrap();
    assert!(cancel.is_cancelled());
    assert!(!revision.0.is_empty());
    assert_eq!(vfs.read(Path::new("/map")).unwrap(), b"saved");
}

fn poll(
    watcher: &mut PollWatcher,
    provider: &dyn StorageProvider,
    ms: u64,
) -> Vec<thinoda_platform::storage::StorageChange> {
    run(watcher.poll(provider, Duration::from_millis(ms), token())).unwrap()
}

#[test]
fn watcher_suppresses_own_save_but_not_later_external_edit() {
    let (vfs, provider) = memory(false);
    let id = StorageId("/Documents/map".into());
    run(provider.write(&id, b"initial", ExpectedRevision::Missing, token())).unwrap();
    let mut watcher = provider.watch(StorageId("/Documents".into()), Duration::from_millis(100));
    assert!(poll(&mut watcher, &provider, 0).is_empty());
    run(provider.write(&id, b"own save", ExpectedRevision::Any, token())).unwrap();
    assert!(poll(&mut watcher, &provider, 10).is_empty());
    assert!(poll(&mut watcher, &provider, 200).is_empty());
    vfs.write(Path::new(&id.0), b"external", 30).unwrap();
    assert!(poll(&mut watcher, &provider, 210).is_empty());
    let changes = poll(&mut watcher, &provider, 310);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].id, id);
    assert_eq!(changes[0].entry.as_ref().unwrap().size, 8);
}

#[test]
fn watcher_coalesces_sync_steps_reversion_and_deletion() {
    let (vfs, provider) = memory(false);
    let id = StorageId("/Documents/map".into());
    vfs.write(Path::new(&id.0), b"baseline", 1).unwrap();
    let mut watcher = PollWatcher::new(StorageId("/Documents".into()), Duration::from_millis(100));
    assert!(poll(&mut watcher, &provider, 0).is_empty());
    vfs.write(Path::new(&id.0), b"step one", 2).unwrap();
    assert!(poll(&mut watcher, &provider, 10).is_empty());
    vfs.write(Path::new(&id.0), b"final", 3).unwrap();
    assert!(poll(&mut watcher, &provider, 50).is_empty());
    assert!(poll(&mut watcher, &provider, 110).is_empty());
    let changes = poll(&mut watcher, &provider, 150);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entry.as_ref().unwrap().size, 5);
    vfs.write(Path::new(&id.0), b"transient", 4).unwrap();
    assert!(poll(&mut watcher, &provider, 160).is_empty());
    vfs.write(Path::new(&id.0), b"final", 3).unwrap();
    assert!(poll(&mut watcher, &provider, 170).is_empty());
    assert!(poll(&mut watcher, &provider, 300).is_empty());
    vfs.remove_file(Path::new(&id.0)).unwrap();
    assert!(poll(&mut watcher, &provider, 310).is_empty());
    let deleted = poll(&mut watcher, &provider, 410);
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].entry, None);
}

#[test]
fn watcher_manual_suppression_is_exact_and_cancel_does_not_advance_baseline() {
    let (vfs, provider) = memory(false);
    let id = StorageId("/map".into());
    vfs.write(Path::new("/map"), b"old", 1).unwrap();
    let mut watcher = PollWatcher::new(StorageId("/".into()), Duration::ZERO);
    poll(&mut watcher, &provider, 0);
    watcher.suppress_own_save(id.clone(), Revision("different".into()));
    vfs.write(Path::new("/map"), b"new", 2).unwrap();
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        run(watcher.poll(&provider, Duration::from_secs(1), cancel)),
        Err(StorageError::Cancelled)
    ));
    assert_eq!(poll(&mut watcher, &provider, 2000).len(), 1);
}

#[test]
fn each_watcher_suppresses_own_save_once_and_external_restore_is_not_suppressed() {
    let (vfs, provider) = memory(false);
    let id = StorageId("/map".into());
    vfs.write(Path::new("/map"), b"initial", 1).unwrap();
    let mut first = provider.watch(StorageId("/".into()), Duration::ZERO);
    let mut second = provider.watch(StorageId("/".into()), Duration::ZERO);
    poll(&mut first, &provider, 0);
    poll(&mut second, &provider, 0);
    run(provider.write(&id, b"saved", ExpectedRevision::Any, token())).unwrap();
    let saved_time = vfs.metadata(Path::new("/map")).unwrap().modified_ms;
    assert!(poll(&mut first, &provider, 1).is_empty());
    assert!(poll(&mut second, &provider, 1).is_empty());
    vfs.write(Path::new("/map"), b"other", saved_time + 1)
        .unwrap();
    assert_eq!(poll(&mut first, &provider, 2).len(), 1);
    assert_eq!(poll(&mut second, &provider, 2).len(), 1);
    vfs.write(Path::new("/map"), b"saved", saved_time).unwrap();
    assert_eq!(poll(&mut first, &provider, 3).len(), 1);
    assert_eq!(poll(&mut second, &provider, 3).len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_revisions_detect_same_size_edits_with_restored_mtime_and_replacement_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("map.thinoda");
    let provider = LocalStorageProvider::new(false);
    let id = StorageId::from_path(&path);
    assert_eq!(id.to_path().unwrap(), path);
    let first = run(provider.write(&id, b"first", ExpectedRevision::Missing, token())).unwrap();
    let old = fs::fingerprint(&path).unwrap();
    assert_eq!(first, revision_from_fingerprint(&old));
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, b"other").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let edited = fs::fingerprint(&path).unwrap();
    assert_eq!(old.len, edited.len);
    assert_eq!(old.modified, edited.modified);
    assert_ne!(old.content_hash, edited.content_hash);
    assert!(old.changed_since().unwrap());
    let read = run(provider.read(&id, token())).unwrap();
    assert_eq!(read.bytes, b"other");
    assert_eq!(read.revision, revision_from_fingerprint(&edited));
    fs::atomic_write(&path, b"other", false).unwrap();
    let replaced = fs::fingerprint(&path).unwrap();
    assert_ne!(edited.identity, replaced.identity);
    assert_ne!(
        revision_from_fingerprint(&edited),
        revision_from_fingerprint(&replaced)
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_stale_and_missing_writes_never_overwrite_or_create_backups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("map");
    let provider = LocalStorageProvider::new(true);
    let id = StorageId::from_path(&path);
    let first = run(provider.write(&id, b"first", ExpectedRevision::Missing, token())).unwrap();
    assert!(!fs::backup_path(&path).exists());
    std::fs::write(&path, b"other").unwrap();
    let external = run(provider.read(&id, token())).unwrap().revision;
    assert!(matches!(
        run(provider.write(&id, b"stale", ExpectedRevision::Match(first), token())),
        Err(StorageError::Conflict { actual: Some(r) }) if r == external
    ));
    assert!(matches!(
        run(provider.write(&id, b"new", ExpectedRevision::Missing, token())),
        Err(StorageError::Conflict { actual: Some(_) })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), b"other");
    assert!(!fs::backup_path(&path).exists());
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        run(provider.write(&id, b"cancelled", ExpectedRevision::Any, cancel)),
        Err(StorageError::Cancelled)
    ));
    assert!(!fs::backup_path(&path).exists());
    run(provider.write(&id, b"saved", ExpectedRevision::Match(external), token())).unwrap();
    assert_eq!(std::fs::read(fs::backup_path(&path)).unwrap(), b"other");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn independent_native_providers_serialize_conditional_creates() {
    let dir = tempfile::tempdir().unwrap();
    let id = StorageId::from_path(&dir.path().join("map"));
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let threads = (0..2)
        .map(|_| {
            let id = id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                run(LocalStorageProvider::new(true).write(
                    &id,
                    b"saved",
                    ExpectedRevision::Missing,
                    token(),
                ))
            })
        })
        .collect::<Vec<_>>();
    let results = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(StorageError::Conflict { actual: Some(_) })))
            .count(),
        1
    );
    assert!(!fs::backup_path(id.to_path().unwrap().as_path()).exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_watch_uses_saved_handle_revision_without_self_reload() {
    let dir = tempfile::tempdir().unwrap();
    let provider = LocalStorageProvider::new(false);
    let id = StorageId::from_path(&dir.path().join("map"));
    run(provider.write(&id, b"first", ExpectedRevision::Missing, token())).unwrap();
    let mut watcher = provider.watch(StorageId::from_path(dir.path()), Duration::ZERO);
    poll(&mut watcher, &provider, 0);
    let saved = run(provider.write(&id, b"saved", ExpectedRevision::Any, token())).unwrap();
    assert_eq!(saved, run(provider.read(&id, token())).unwrap().revision);
    assert!(poll(&mut watcher, &provider, 1).is_empty());
    std::fs::write(id.to_path().unwrap(), b"other").unwrap();
    assert_eq!(poll(&mut watcher, &provider, 2).len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn conservative_conflict_patterns_never_modify_siblings() {
    let dir = tempfile::tempdir().unwrap();
    let names = [
        "map (test conflicted copy 2026-11-01).thinoda",
        "map (conflict copy 2026-11-01).thinoda",
        "map (Konfliktkopie 2026-11-01).thinoda",
        "map_conflict-20261101-091000.thinoda",
        "map-DESKTOP-TEST.thinoda",
        "map 2.thinoda",
    ];
    let unrelated = [
        "mapping (conflicted copy).thinoda",
        "map (ordinary copy).thinoda",
        "map_conflict-not-a-date.thinoda",
        "map_conflict-20261101.txt",
        "map-project.thinoda",
        "map 1.thinoda",
    ];
    for name in names.iter().chain(&unrelated) {
        std::fs::write(dir.path().join(name), b"keep").unwrap();
    }
    let copies = fs::conflict_copies(dir.path().join("map.thinoda")).unwrap();
    assert_eq!(copies.len(), names.len());
    for name in names.iter().chain(&unrelated) {
        assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), b"keep");
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn configured_backups_are_durable_outside_source_and_keyed_by_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let provider = LocalStorageProvider::new(true).with_backup_directory(backups.clone());
    let first_path = dir.path().join("source-a/map.thinoda");
    let second_path = dir.path().join("source-b/map.thinoda");
    let first_id = StorageId::from_path(&first_path);
    let second_id = StorageId::from_path(&second_path);
    let first =
        run(provider.write(&first_id, b"first old", ExpectedRevision::Missing, token())).unwrap();
    let second = run(provider.write(
        &second_id,
        b"second old",
        ExpectedRevision::Missing,
        token(),
    ))
    .unwrap();
    assert!(
        !backups.exists(),
        "creation has no prior version to back up"
    );
    run(provider.write(
        &first_id,
        b"first new",
        ExpectedRevision::Match(first),
        token(),
    ))
    .unwrap();
    run(provider.write(
        &second_id,
        b"second new",
        ExpectedRevision::Match(second),
        token(),
    ))
    .unwrap();
    assert!(!fs::backup_path(&first_path).exists());
    assert!(!fs::backup_path(&second_path).exists());
    let files = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 2);
    let mut contents = files
        .iter()
        .map(|path| {
            assert_eq!(path.extension().unwrap(), "bak");
            assert_eq!(path.file_stem().unwrap().to_string_lossy().len(), 64);
            std::fs::read(path).unwrap()
        })
        .collect::<Vec<_>>();
    contents.sort();
    assert_eq!(
        contents,
        vec![b"first old".to_vec(), b"second old".to_vec()]
    );
    assert_eq!(std::fs::read(first_path).unwrap(), b"first new");
    assert_eq!(std::fs::read(second_path).unwrap(), b"second new");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn configured_backup_stale_cancelled_and_disabled_saves_create_no_backup_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source/map");
    let id = StorageId::from_path(&path);
    let backups = dir.path().join("backups");
    let provider = LocalStorageProvider::new(true).with_backup_directory(backups.clone());
    let first = run(provider.write(&id, b"first", ExpectedRevision::Missing, token())).unwrap();
    std::fs::write(&path, b"other").unwrap();
    assert!(matches!(
        run(provider.write(&id, b"stale", ExpectedRevision::Match(first), token())),
        Err(StorageError::Conflict { actual: Some(_) })
    ));
    let cancel = token();
    cancel.cancel();
    assert!(matches!(
        run(provider.write(&id, b"cancelled", ExpectedRevision::Any, cancel)),
        Err(StorageError::Cancelled)
    ));
    assert!(!backups.exists());
    let disabled = LocalStorageProvider::new(false).with_backup_directory(backups.clone());
    run(disabled.write(&id, b"saved", ExpectedRevision::Any, token())).unwrap();
    assert!(!backups.exists());
    assert!(!fs::backup_path(&path).exists());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn configured_backup_rejects_source_tree_and_failed_backup_preserves_original() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let path = source.join("map");
    std::fs::write(&path, b"original").unwrap();
    let id = StorageId::from_path(&path);
    for backup in [source.clone(), source.join("backup")] {
        let provider = LocalStorageProvider::new(true).with_backup_directory(backup);
        assert!(matches!(
            run(provider.write(&id, b"replacement", ExpectedRevision::Any, token())),
            Err(StorageError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
        ));
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    let invalid = dir.path().join("not-a-directory");
    std::fs::write(&invalid, b"keep").unwrap();
    let provider = LocalStorageProvider::new(true).with_backup_directory(invalid.clone());
    assert!(run(provider.write(&id, b"replacement", ExpectedRevision::Any, token())).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read(invalid).unwrap(), b"keep");
    assert_eq!(
        std::fs::read_dir(source).unwrap().count(),
        1,
        "no temporary files remain"
    );
}

#[cfg(windows)]
#[test]
fn windows_offline_attribute_never_hydrates_in_listing_fingerprints_or_ordinary_reads() {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
    }
    struct Reset(Vec<u16>);
    impl Drop for Reset {
        fn drop(&mut self) {
            // SAFETY: null-terminated path is live for the call.
            unsafe {
                SetFileAttributesW(self.0.as_ptr(), 0x80);
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("offline");
    std::fs::write(&path, b"data").unwrap();
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: valid null-terminated UTF-16 path; test changes only its own file.
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), 0x1000) }, 0);
    let _reset = Reset(wide);
    assert!(fs::online_only(&path).unwrap());
    let fingerprint = fs::fingerprint(&path).unwrap();
    assert!(fingerprint.online_only);
    assert_eq!(fingerprint.content_hash, None);
    assert_eq!(
        fs::read(&path).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let provider = LocalStorageProvider::new(false);
    let id = StorageId::from_path(&path);
    let entries = run(provider.list(&StorageId::from_path(dir.path()), token())).unwrap();
    let entry = entries.iter().find(|entry| entry.id == id).unwrap();
    assert_eq!(entry.availability, Availability::OnlineOnly);
    assert_eq!(entry.revision, None);
    assert_eq!(run(provider.read(&id, token())).unwrap().bytes, b"data");
    assert!(matches!(
        run(provider.write(&id, b"no", ExpectedRevision::Missing, token())),
        Err(StorageError::Conflict { actual: Some(_) })
    ));
}

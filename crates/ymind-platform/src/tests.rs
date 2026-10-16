use crate::{
    Paths,
    cloud::*,
    crash::*,
    fs::{atomic_write, backup_path, changed_since, conflict_copies, fingerprint},
    recovery::*,
    settings::*,
    single_instance::SingleInstance,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;
use ymind_core::Workbook;

// Keep test artifacts inside the permitted crate rather than the OS temp directory.
fn directory() -> TempDir {
    tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
}
fn paths(dir: &TempDir) -> Paths {
    Paths::from_root(dir.path())
}
fn meta(title: &str) -> SnapshotMeta {
    SnapshotMeta {
        title: title.into(),
        dirty: true,
        saved_at: 42,
        original_path: Some(PathBuf::from("map.xmind")),
    }
}
fn crash_session(manager: RecoveryManager) {
    assert!(manager.try_flush_for_crash());
    drop(manager);
}
fn mkdir(path: impl AsRef<Path>) {
    fs::create_dir_all(path).unwrap();
}
fn create(path: impl AsRef<Path>) {
    fs::write(path, b"test").unwrap();
}

#[test]
fn directories_are_created_on_demand() {
    let dir = directory();
    let paths = paths(&dir);
    assert!(!paths.config_dir.exists());
    assert_eq!(paths.config_dir, dir.path().join("config"));
    paths.ensure().unwrap();
    assert!(paths.config_dir.is_dir() && paths.data_dir.is_dir() && paths.cache_dir.is_dir());
}

#[test]
fn settings_defaults_and_empty_toml() {
    let defaults = Settings::default();
    let dir = directory();
    assert_eq!(Settings::load(&paths(&dir)).unwrap(), defaults);
    assert_eq!(toml::from_str::<Settings>("").unwrap(), defaults);
    assert_eq!(defaults.theme, ThemeMode::System);
    assert_eq!(defaults.language, Language::Auto);
    assert!(defaults.autosave && defaults.backup_on_save);
    assert!(!defaults.label_auto_sort);
    assert!((1..=2).contains(&defaults.recovery_interval_secs));
}

#[test]
fn template_recents_are_bounded_deduplicated_validated_and_independent_of_files() {
    let dir = directory();
    let paths = paths(&dir);
    paths.ensure().unwrap();
    let mut settings = Settings::default();
    let path = dir.path().join("original.ymind");
    create(&path);
    settings.add_recent(&path);
    let files = settings.recent_files.clone();
    for template in ymind_core::templates::TEMPLATES {
        settings.add_recent_template(template.id);
    }
    assert_eq!(settings.recent_templates.len(), 8);
    settings.add_recent_template("swot");
    settings.add_recent_template("swot");
    assert_eq!(settings.recent_templates[0], "swot");
    assert_eq!(
        settings
            .recent_templates
            .iter()
            .filter(|&id| id == "swot")
            .count(),
        1
    );
    settings.add_recent_template("unknown-template");
    assert_eq!(settings.recent_templates[0], "swot");
    assert_eq!(settings.recent_files, files);
    settings.save(&paths).unwrap();
    assert_eq!(Settings::load(&paths).unwrap(), settings);
    let legacy: Settings = toml::from_str("language = \"english\"").unwrap();
    assert!(legacy.recent_templates.is_empty());
}

#[test]
fn settings_roundtrip_every_field() {
    let dir = directory();
    let paths = paths(&dir);
    let mut settings = Settings {
        theme: ThemeMode::Dark,
        language: Language::German,
        autosave: false,
        autosave_interval_secs: 60,
        recovery_interval_secs: 2,
        backup_on_save: false,
        default_structure: "logic_chart".into(),
        default_theme: "ocean".into(),
        ui_scale: 1.5,
        show_status_bar: false,
        show_format_panel: false,
        show_topic_toolbar: false,
        show_topic_add_buttons: false,
        label_auto_sort: true,
        initial_canvas_zoom: 0.8,
        reuse_last_structure: true,
        reuse_last_theme: true,
        snap_to_grid: true,
        smooth_zoom: false,
        spell_check: true,
        check_for_updates: false,
        confirm_on_delete: false,
        window: Some(WindowState {
            width: 900.0,
            height: 650.0,
            x: Some(-100.0),
            y: Some(20.0),
            maximized: true,
        }),
        last_open_dir: Some(dir.path().to_path_buf()),
        last_export_format: Some("svg".into()),
        cloud: CloudSettings {
            preferred_folder: Some(dir.path().join("cloud")),
            show_cloud_badges: false,
        },
        ..Settings::default()
    };
    settings
        .shortcuts_overrides
        .insert("save".into(), "Ctrl+Shift+S".into());
    settings.add_recent(dir.path().join("ÃƒÂ¤ map.xmind"));
    settings.pin_recent(dir.path().join("ÃƒÂ¤ map.xmind"), true);
    settings.save(&paths).unwrap();
    assert_eq!(Settings::load(&paths).unwrap(), settings);
}

#[test]
fn broken_settings_are_preserved_with_warning() {
    let dir = directory();
    let paths = paths(&dir);
    mkdir(&paths.config_dir);
    let file = paths.config_dir.join("settings.toml");
    fs::write(&file, "theme = [broken").unwrap();
    let loaded = Settings::load_with_warning(&paths).unwrap();
    assert_eq!(loaded.settings, Settings::default());
    assert!(loaded.warning.is_some());
    assert!(!file.exists());
    let entries: Vec<_> = fs::read_dir(&paths.config_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("settings.toml.broken-")
    );
    assert_eq!(fs::read_to_string(&entries[0]).unwrap(), "theme = [broken");
    settings_invalid_utf8(&paths);
}

fn settings_invalid_utf8(paths: &Paths) {
    fs::write(paths.config_dir.join("settings.toml"), [255, 254]).unwrap();
    assert!(
        Settings::load_with_warning(paths)
            .unwrap()
            .warning
            .is_some()
    );
    assert_eq!(fs::read_dir(&paths.config_dir).unwrap().count(), 2);
}

#[test]
fn settings_unknown_fields_and_nested_defaults() {
    let settings: Settings = toml::from_str("future_option = true\n[cloud]\nfuture_cloud = 42\n[window]\nmaximized = true\nfuture_window = 'yes'").unwrap();
    assert!(settings.cloud.show_cloud_badges);
    assert!(settings.window.unwrap().maximized);
    assert_eq!(settings.language, Language::Auto);
    assert!(settings.show_topic_toolbar && settings.show_topic_add_buttons);
    assert_eq!(settings.initial_canvas_zoom, 1.0);
    assert!(!settings.reuse_last_structure && !settings.reuse_last_theme);
}

#[test]
fn initial_canvas_zoom_is_normalized_without_affecting_ui_scale() {
    let dir = directory();
    let paths = paths(&dir);
    for (value, expected) in [(0.0, 0.1), (8.0, 5.0), (f32::NAN, 1.0)] {
        let settings = Settings {
            initial_canvas_zoom: value,
            ui_scale: 1.5,
            ..Settings::default()
        };
        settings.save(&paths).unwrap();
        let loaded = Settings::load(&paths).unwrap();
        assert_eq!(loaded.initial_canvas_zoom, expected);
        assert_eq!(loaded.ui_scale, 1.5);
    }
}

#[test]
fn recent_order_cap_pinning_and_deduplication() {
    let mut settings = Settings::default();
    settings.add_recent("pinned.xmind");
    settings.pin_recent("pinned.xmind", true);
    for i in 0..30 {
        settings.add_recent(format!("map{i}.xmind"));
    }
    assert_eq!(settings.recent_files.len(), 20);
    assert_eq!(settings.recent_files[0].path, PathBuf::from("pinned.xmind"));
    assert_eq!(settings.recent_files[1].path, PathBuf::from("map29.xmind"));
    settings.add_recent("map20.xmind");
    assert_eq!(settings.recent_files[1].path, PathBuf::from("map20.xmind"));
    assert_eq!(
        settings
            .recent_files
            .iter()
            .filter(|r| r.path == Path::new("map20.xmind"))
            .count(),
        1
    );
    settings.add_recent("pinned.xmind");
    assert!(settings.recent_files[0].pinned);
    settings.remove_recent("map20.xmind");
    assert_eq!(settings.recent_files.len(), 19);
    settings.clear_recent();
    assert!(settings.recent_files.is_empty());
}

#[test]
fn recent_existing_and_loaded_list_normalization() {
    let dir = directory();
    let paths = paths(&dir);
    let existing = dir.path().join("map.xmind");
    create(&existing);
    let mut settings = Settings {
        recent_files: (0..25)
            .map(|i| RecentFile {
                path: PathBuf::from(format!("map{i}")),
                opened_at: i,
                pinned: i == 0,
            })
            .collect(),
        ..Settings::default()
    };
    settings.recent_files.push(RecentFile {
        path: existing.clone(),
        opened_at: 99,
        pinned: false,
    });
    settings.recent_files.push(RecentFile {
        path: existing.clone(),
        opened_at: 80,
        pinned: false,
    });
    settings.save(&paths).unwrap();
    let loaded = Settings::load(&paths).unwrap();
    assert_eq!(loaded.recent_files.len(), 20);
    assert!(loaded.recent_files[0].pinned);
    assert_eq!(loaded.existing_recent().len(), 1);
    assert_eq!(loaded.existing_recent()[0].path, existing);
}

#[test]
fn atomic_write_create_replace_backup_no_temporary_files() {
    let dir = directory();
    let file = dir.path().join("nested").join("map.xmind");
    atomic_write(&file, b"first", true).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"first");
    assert!(!backup_path(&file).exists());
    atomic_write(&file, b"second", true).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"second");
    assert_eq!(fs::read(backup_path(&file)).unwrap(), b"first");
    atomic_write(&file, b"third", true).unwrap();
    assert_eq!(fs::read(backup_path(&file)).unwrap(), b"second");
    atomic_write(&file, b"fourth", false).unwrap();
    assert_eq!(fs::read(backup_path(&file)).unwrap(), b"second");
    assert_eq!(fs::read_dir(file.parent().unwrap()).unwrap().count(), 2);
}

#[test]
fn atomic_write_error_cleans_temporary_file_and_preserves_target() {
    let dir = directory();
    let target = dir.path().join("target");
    mkdir(&target);
    create(target.join("child"));
    let error = atomic_write(&target, b"no", false).unwrap_err();
    assert!(error.to_string().contains("read-only or in use"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(target.join("child").exists());
}

#[test]
fn fingerprints_detect_replacement_and_deletion() {
    let dir = directory();
    let file = dir.path().join("map");
    atomic_write(&file, b"first", false).unwrap();
    let old = fingerprint(&file).unwrap();
    assert!(!changed_since(&old).unwrap());
    atomic_write(&file, b"changed length", false).unwrap();
    assert!(old.changed_since().unwrap());
    let current = fingerprint(&file).unwrap();
    fs::remove_file(&file).unwrap();
    assert!(changed_since(&current).unwrap());
}

#[test]
fn conflict_copy_patterns_and_false_positives() {
    let dir = directory();
    let names = [
        "map.xmind",
        "map (conflicted copy 2026).xmind",
        "map (Alice's conflicted copy 2026).xmind",
        "map-DESKTOP-XYZ.xmind",
        "map-LAPTOP-ABC.xmind",
        "map 2.xmind",
        "map 10.xmind",
        "map 1.xmind",
        "map 0.xmind",
        "map2.xmind",
        "map (normal copy).xmind",
        "map (conflicted copy).txt",
        "mapping 2.xmind",
        "map-feature.xmind",
    ];
    for name in names {
        create(dir.path().join(name));
    }
    mkdir(dir.path().join("map 3.xmind"));
    let conflicts = conflict_copies(dir.path().join("map.xmind")).unwrap();
    assert_eq!(conflicts.len(), 6);
    assert!(conflicts.iter().any(|p| p.ends_with("map 2.xmind")));
    assert!(!conflicts.iter().any(|p| p.ends_with("map 1.xmind")));
}

#[test]
fn recovery_roundtrip_orphan_discard() {
    let dir = directory();
    let paths = paths(&dir);
    let manager = RecoveryManager::start(&paths).unwrap();
    let workbook = Workbook::default();
    manager.snapshot("doc-1", meta("First"), &workbook).unwrap();
    let session = manager.session_dir().to_path_buf();
    crash_session(manager);
    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].docs.len(), 1);
    let doc = &orphans[0].docs[0];
    assert_eq!(doc.meta, meta("First"));
    assert_eq!(
        serde_json::to_value(doc.load().unwrap()).unwrap(),
        serde_json::to_value(workbook).unwrap()
    );
    assert!(
        RecoveryManager::find_orphans(&paths).is_empty(),
        "held orphan lease prevents double recovery"
    );
    doc.discard().unwrap();
    doc.discard().unwrap();
    assert!(!session.exists());
}

#[test]
fn own_live_session_is_not_orphan_and_finish_is_idempotent() {
    let dir = directory();
    let paths = paths(&dir);
    let manager = RecoveryManager::start(&paths).unwrap();
    manager
        .snapshot("live", meta("Live"), &Workbook::default())
        .unwrap();
    assert!(RecoveryManager::find_orphans(&paths).is_empty());
    manager.finish().unwrap();
    manager.finish().unwrap();
    assert!(!manager.session_dir().exists());
    assert!(manager.queue_snapshot("live", Vec::new()).is_err());
}

#[test]
fn recovery_queue_latest_flush_remove_and_multi_document() {
    let dir = directory();
    let paths = paths(&dir);
    let manager = RecoveryManager::start(&paths).unwrap();
    let workbook = Workbook::default();
    manager
        .queue_snapshot("doc1", Snapshot::to_bytes(meta("Old"), &workbook).unwrap())
        .unwrap();
    manager
        .queue_snapshot("doc1", Snapshot::to_bytes(meta("New"), &workbook).unwrap())
        .unwrap();
    manager
        .queue_snapshot(
            "doc2",
            Snapshot::to_bytes(meta("Other"), &workbook).unwrap(),
        )
        .unwrap();
    assert_eq!(
        fs::read_dir(manager.session_dir().join("docs"))
            .unwrap()
            .count(),
        0
    );
    manager.flush().unwrap();
    let snapshot: Snapshot = serde_json::from_slice(
        &fs::read(manager.session_dir().join("docs").join("doc1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.meta.title, "New");
    manager
        .queue_snapshot(
            "doc2",
            Snapshot::to_bytes(meta("Pending"), &workbook).unwrap(),
        )
        .unwrap();
    manager.remove("doc2").unwrap();
    manager.flush().unwrap();
    assert_eq!(
        fs::read_dir(manager.session_dir().join("docs"))
            .unwrap()
            .count(),
        1
    );
    crash_session(manager);
    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans[0].docs.len(), 1);
    assert_eq!(orphans[0].docs[0].doc_key, "doc1");
    orphans[0].discard().unwrap();
}

#[test]
fn corrupted_and_invalid_workbooks_are_recoverable_errors_not_panics() {
    let dir = directory();
    let paths = paths(&dir);
    let session = paths.data_dir.join("recovery").join("dead-process");
    mkdir(session.join("docs"));
    fs::write(session.join("docs").join("corrupt.json"), b"{ not json").unwrap();
    fs::write(
        session.join("docs").join("invalid.json"),
        br#"{"meta":{},"workbook":{"sheets":[]}}"#,
    )
    .unwrap();
    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].docs.len(), 2);
    for doc in &orphans[0].docs {
        assert!(doc.load().is_err());
    }
    orphans[0].discard().unwrap();
    assert!(!session.exists());
}

#[test]
fn empty_orphan_sessions_and_clean_drop_are_removed() {
    let dir = directory();
    let paths = paths(&dir);
    let empty = paths.data_dir.join("recovery").join("empty");
    mkdir(empty.join("docs"));
    assert!(RecoveryManager::find_orphans(&paths).is_empty());
    assert!(!empty.exists());
    let manager = RecoveryManager::start(&paths).unwrap();
    let session = manager.session_dir().to_path_buf();
    manager
        .snapshot("doc", meta("Clean"), &Workbook::default())
        .unwrap();
    drop(manager);
    assert!(!session.exists());
}

#[test]
fn discard_all_orphans_keeps_live_documents() {
    let dir = directory();
    let paths = paths(&dir);
    let live = RecoveryManager::start(&paths).unwrap();
    live.snapshot("live", meta("Live"), &Workbook::default())
        .unwrap();
    let dead = RecoveryManager::start(&paths).unwrap();
    dead.snapshot("dead", meta("Dead"), &Workbook::default())
        .unwrap();
    let dead_path = dead.session_dir().to_path_buf();
    crash_session(dead);
    RecoveryManager::discard_all_orphans(&paths).unwrap();
    assert!(!dead_path.exists());
    assert!(live.session_dir().join("docs").join("live.json").exists());
}

#[test]
fn recovery_rejects_traversal_keys() {
    let dir = directory();
    let manager = RecoveryManager::start(&paths(&dir)).unwrap();
    for key in ["", "../oops", "a/b", "a\\b", ".", "..", "a:b"] {
        assert!(manager.queue_snapshot(key, Vec::new()).is_err());
        assert!(manager.remove(key).is_err());
    }
}

#[test]
fn panic_hook_logs_backtrace_and_flushes_pending_snapshot() {
    type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync>;
    struct RestoreHook(Option<PanicHook>);
    impl Drop for RestoreHook {
        fn drop(&mut self) {
            if let Some(hook) = self.0.take() {
                std::panic::set_hook(hook);
            }
        }
    }
    let dir = directory();
    let paths = paths(&dir);
    let manager = Arc::new(RecoveryManager::start(&paths).unwrap());
    manager
        .queue_snapshot(
            "panic-doc",
            Snapshot::to_bytes(meta("Panic"), &Workbook::default()).unwrap(),
        )
        .unwrap();
    let old = std::panic::take_hook();
    let restore = RestoreHook(Some(old));
    let chained = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = Arc::clone(&chained);
    std::panic::set_hook(Box::new(move |_| {
        flag.store(true, std::sync::atomic::Ordering::Release);
    }));
    install_panic_hook(paths.clone(), Some(Arc::clone(&manager)));
    let result = std::thread::spawn(|| panic!("intentional platform recovery test")).join();
    drop(restore);
    assert!(result.is_err());
    assert!(chained.load(std::sync::atomic::Ordering::Acquire));
    let logs = list_crash_logs(&paths).unwrap();
    assert_eq!(logs.len(), 1);
    let content = fs::read_to_string(&logs[0].path).unwrap();
    assert!(content.contains("intentional platform recovery test"));
    assert!(content.contains("Location:"));
    assert!(content.contains(crate::system::arch()));
    assert!(content.contains("Recovery flushed: true"));
    assert!(
        last_crash_since(&paths, logs[0].timestamp)
            .unwrap()
            .is_some()
    );
    assert!(
        last_crash_since(&paths, logs[0].timestamp + 1)
            .unwrap()
            .is_none()
    );
    drop(manager);
    let orphans = RecoveryManager::find_orphans(&paths);
    assert_eq!(orphans[0].docs[0].doc_key, "panic-doc");
    orphans[0].docs[0].load().unwrap();
    orphans[0].discard().unwrap();
}

#[test]
fn crash_log_listing_ignores_unrelated_files() {
    let dir = directory();
    let paths = paths(&dir);
    assert!(list_crash_logs(&paths).unwrap().is_empty());
    let logs = paths.data_dir.join("crash-logs");
    mkdir(&logs);
    for name in [
        "crash-20-one.log",
        "crash-10.log",
        "unrelated.log",
        "crash-oops.log",
        "crash-30.txt",
    ] {
        create(logs.join(name));
    }
    let found = list_crash_logs(&paths).unwrap();
    assert_eq!(
        found.iter().map(|log| log.timestamp).collect::<Vec<_>>(),
        vec![20, 10]
    );
}

#[test]
fn cloud_windows_environment_and_drive_mounts() {
    let dir = directory();
    let mut env = CloudEnv::new(dir.path(), CloudOs::Windows);
    for (var, name) in [
        ("OneDrive", "onedrive"),
        ("OneDriveCommercial", "work"),
        ("OneDriveConsumer", "personal"),
    ] {
        let path = dir.path().join(name);
        mkdir(&path);
        env.vars.insert(var.into(), path.into_os_string());
    }
    for name in [
        "iCloudDrive",
        "Google Drive",
        "Box",
        "Nextcloud",
        "ownCloud",
    ] {
        mkdir(dir.path().join(name));
    }
    let drive = dir.path().join("drive-g");
    mkdir(drive.join("My Drive"));
    mkdir(drive.join("Meine Ablage"));
    env.drive_roots.push(drive);
    let folders = detect_cloud_folders_with(&env);
    assert_eq!(
        folders
            .iter()
            .filter(|f| f.provider == CloudProvider::OneDrive)
            .count(),
        3
    );
    assert_eq!(
        folders
            .iter()
            .filter(|f| f.provider == CloudProvider::GoogleDrive)
            .count(),
        3
    );
    assert!(folders.iter().any(|f| f.provider == CloudProvider::ICloud));
    assert!(folders.iter().all(|f| f.path.is_dir()));
}

#[test]
fn cloud_mac_storage_and_icloud() {
    let dir = directory();
    let env = CloudEnv::new(dir.path(), CloudOs::MacOs);
    let storage = dir.path().join("Library").join("CloudStorage");
    for name in [
        "OneDrive-Work",
        "OneDrive-Personal",
        "GoogleDrive-test@example.org",
        "Box-Box",
    ] {
        mkdir(storage.join(name));
    }
    mkdir(
        dir.path()
            .join("Library")
            .join("Mobile Documents")
            .join("com~apple~CloudDocs"),
    );
    let folders = detect_cloud_folders_with(&env);
    assert_eq!(folders.len(), 5);
    assert_eq!(
        folders
            .iter()
            .filter(|f| f.provider == CloudProvider::OneDrive)
            .count(),
        2
    );
    assert!(
        folders
            .iter()
            .any(|f| f.provider == CloudProvider::GoogleDrive)
    );
    assert!(folders.iter().any(|f| f.provider == CloudProvider::ICloud));
}

#[test]
fn cloud_linux_defaults_dropbox_info_and_nonexistent_paths() {
    let dir = directory();
    let mut env = CloudEnv::new(dir.path(), CloudOs::Linux);
    for name in [
        "OneDrive",
        "Dropbox",
        "Box",
        "Nextcloud",
        "ownCloud",
        "custom-business",
    ] {
        mkdir(dir.path().join(name));
    }
    mkdir(dir.path().join(".dropbox"));
    fs::write(
        dir.path().join(".dropbox").join("info.json"),
        serde_json::to_vec(&serde_json::json!({
            "personal": {"path": dir.path().join("Dropbox")},
            "business": {"path": dir.path().join("custom-business")},
            "missing": {"path": dir.path().join("not-installed")}
        }))
        .unwrap(),
    )
    .unwrap();
    env.vars.insert(
        "OneDrive".into(),
        dir.path().join("ignored-on-linux").into_os_string(),
    );
    let folders = detect_cloud_folders_with(&env);
    assert_eq!(folders.len(), 6);
    assert_eq!(
        folders
            .iter()
            .filter(|f| f.provider == CloudProvider::Dropbox)
            .count(),
        2
    );
}

#[test]
fn dropbox_windows_info_locations_and_corrupt_config() {
    let dir = directory();
    let mut env = CloudEnv::new(dir.path(), CloudOs::Windows);
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let root = dir.path().join(var);
        mkdir(root.join("Dropbox"));
        let synced = dir.path().join(format!("sync-{var}"));
        mkdir(&synced);
        fs::write(
            root.join("Dropbox").join("info.json"),
            serde_json::to_vec(&serde_json::json!({"personal": {"path": synced}})).unwrap(),
        )
        .unwrap();
        env.vars.insert(var.into(), root.into_os_string());
    }
    mkdir(dir.path().join(".dropbox"));
    fs::write(
        dir.path().join(".dropbox").join("info.json"),
        "invalid json",
    )
    .unwrap();
    assert_eq!(detect_cloud_folders_with(&env).len(), 2);
}

#[test]
fn cloud_prefix_boundaries_case_and_nested_provider() {
    let dir = directory();
    let root = dir.path().join("Dropbox");
    mkdir(root.join("nested"));
    let folders = vec![
        CloudFolder {
            provider: CloudProvider::Dropbox,
            name: "Dropbox".into(),
            path: root.clone(),
        },
        CloudFolder {
            provider: CloudProvider::Box,
            name: "Nested".into(),
            path: root.join("nested"),
        },
    ];
    assert_eq!(
        provider_for_path(&folders, &root).unwrap().provider,
        CloudProvider::Dropbox
    );
    assert_eq!(
        provider_for_path(&folders, &root.join("nested").join("new.xmind"))
            .unwrap()
            .provider,
        CloudProvider::Box
    );
    assert!(
        provider_for_path(
            &folders,
            &dir.path().join("Dropbox-other").join("map.xmind")
        )
        .is_none()
    );
    assert!(provider_for_path(&folders, &root.join("..").join("outside.xmind")).is_none());
    let upper = PathBuf::from(root.join("new.xmind").to_string_lossy().to_uppercase());
    assert!(provider_for_path_on_os(&folders, &upper, CloudOs::Windows).is_some());
    assert!(provider_for_path_on_os(&folders, &upper, CloudOs::MacOs).is_some());
    assert!(provider_for_path_on_os(&folders, &upper, CloudOs::Linux).is_none());
}

#[test]
fn single_instance_election_forward_poll_and_re_election() {
    let dir = directory();
    let paths = paths(&dir);
    let primary = SingleInstance::acquire(&paths).unwrap();
    assert!(primary.is_primary());
    let secondary = SingleInstance::acquire(&paths).unwrap();
    assert!(!secondary.is_primary());
    assert!(secondary.poll().is_err());
    let file = dir.path().join("map with spaces.xmind");
    secondary.forward_open(&file).unwrap();
    assert_eq!(primary.poll().unwrap(), vec![file]);
    assert!(primary.poll().unwrap().is_empty());
    drop(primary);
    assert!(SingleInstance::acquire(&paths).unwrap().is_primary());
}

#[test]
fn system_helpers_are_gui_independent_and_reject_unsafe_urls() {
    assert!(!crate::system::os_name().is_empty());
    assert!(!crate::system::arch().is_empty());
    assert_eq!(crate::system::app_version(), env!("CARGO_PKG_VERSION"));
    assert!(crate::system::open_url("file:///etc/passwd").is_err());
    assert!(crate::system::open_url("https://example.org\n").is_err());
}

#[test]
fn isolated_home_env_child() {
    if let Some(root) = std::env::var_os("YMIND_PLATFORM_TEST_HOME") {
        let paths = Paths::new().unwrap();
        assert_eq!(paths, Paths::from_root(root));
        assert!(crate::system::is_portable());
        paths.ensure().unwrap();
    }
}

#[test]
fn ymind_home_overrides_platform_paths_without_mutating_test_environment() {
    let dir = directory();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("tests::isolated_home_env_child")
        .env("YMIND_HOME", dir.path())
        .env("YMIND_PLATFORM_TEST_HOME", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join("config").is_dir());
    assert!(dir.path().join("data").is_dir());
    assert!(dir.path().join("cache").is_dir());
}

#[test]
fn failed_recovery_flush_retains_pending_bytes_for_retry() {
    let dir = directory();
    let paths = paths(&dir);
    let manager = RecoveryManager::start(&paths).unwrap();
    let docs = manager.session_dir().join("docs");
    fs::remove_dir(&docs).unwrap();
    fs::write(&docs, b"blocking directory creation").unwrap();
    manager
        .queue_snapshot(
            "retry",
            Snapshot::to_bytes(meta("Retry"), &Workbook::default()).unwrap(),
        )
        .unwrap();
    assert!(manager.flush().is_err());
    fs::remove_file(&docs).unwrap();
    manager.flush().unwrap();
    let snapshot: Snapshot =
        serde_json::from_slice(&fs::read(docs.join("retry.json")).unwrap()).unwrap();
    assert_eq!(snapshot.meta.title, "Retry");
}

#[test]
fn fingerprint_detects_same_length_modified_time_change() {
    let dir = directory();
    let file = dir.path().join("map");
    fs::write(&file, b"first").unwrap();
    let before = fingerprint(&file).unwrap();
    let handle = fs::OpenOptions::new().write(true).open(&file).unwrap();
    handle
        .set_modified(before.modified + std::time::Duration::from_secs(5))
        .unwrap();
    assert!(before.changed_since().unwrap());
}

#[test]
fn concurrent_queue_and_flush_preserve_latest_document() {
    let dir = directory();
    let paths = paths(&dir);
    let manager = Arc::new(RecoveryManager::start(&paths).unwrap());
    let writer = Arc::clone(&manager);
    let task = std::thread::spawn(move || {
        for i in 0..50 {
            writer
                .queue_snapshot(
                    "doc",
                    Snapshot::to_bytes(meta(&i.to_string()), &Workbook::default()).unwrap(),
                )
                .unwrap();
        }
    });
    for _ in 0..10 {
        manager.flush().unwrap();
    }
    task.join().unwrap();
    manager.flush().unwrap();
    let snapshot: Snapshot = serde_json::from_slice(
        &fs::read(manager.session_dir().join("docs").join("doc.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.meta.title, "49");
}

#[test]
fn duplicate_loaded_recents_merge_pinning_and_latest_timestamp() {
    let dir = directory();
    let paths = paths(&dir);
    let settings = Settings {
        recent_files: vec![
            RecentFile {
                path: "map.xmind".into(),
                opened_at: 10,
                pinned: true,
            },
            RecentFile {
                path: "map.xmind".into(),
                opened_at: 30,
                pinned: false,
            },
            RecentFile {
                path: "other.xmind".into(),
                opened_at: 20,
                pinned: true,
            },
        ],
        ..Settings::default()
    };
    settings.save(&paths).unwrap();
    let loaded = Settings::load(&paths).unwrap();
    assert_eq!(loaded.recent_files.len(), 2);
    assert_eq!(loaded.recent_files[0].path, PathBuf::from("map.xmind"));
    assert_eq!(loaded.recent_files[0].opened_at, 30);
    assert!(loaded.recent_files[0].pinned);
}

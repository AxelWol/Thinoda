use thinoda_core::quick_styles::{QuickStyle, preset};
use thinoda_platform::{Paths, Settings};

#[test]
fn quick_style_library_survives_profile_reload_without_normalization_loss() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = Paths::from_root(dir.path());
    let mut style = preset("Note").unwrap();
    style.name = "My notes".into();
    style.style.width = Some(345.0);
    style.style.font_family = Some("monospace".into());
    let settings = Settings {
        quick_styles: vec![style],
        ..Default::default()
    };
    settings.save(&paths).unwrap();
    assert_eq!(Settings::load(&paths).unwrap(), settings);
    assert!(Settings::default().quick_styles.is_empty());
}

#[test]
fn invalid_library_save_never_overwrites_the_previous_settings() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = Paths::from_root(dir.path());
    let mut settings = Settings {
        quick_styles: vec![preset("Note").unwrap()],
        ..Default::default()
    };
    settings.save(&paths).unwrap();
    let mut duplicate = preset("Warning").unwrap();
    duplicate.name = " note ".into();
    settings.quick_styles.push(duplicate);
    assert!(settings.save(&paths).is_err());
    assert_eq!(
        Settings::load(&paths).unwrap().quick_styles,
        [preset("Note").unwrap()]
    );
    assert!(
        Settings::validate_quick_styles(
            &(0..33)
                .map(|index| QuickStyle {
                    name: format!("Style {index}"),
                    style: Default::default()
                })
                .collect::<Vec<_>>()
        )
        .is_err()
    );
    std::fs::write(
        paths.config_dir.join("settings.toml"),
        "[[quick_styles]]\nname = \"\"\n[quick_styles.style]\n",
    )
    .unwrap();
    let loaded = Settings::load_with_warning(&paths).unwrap();
    assert!(loaded.warning.is_some());
    assert!(loaded.settings.quick_styles.is_empty());
    assert!(std::fs::read_dir(&paths.config_dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("settings.toml.broken-")
    }));
}

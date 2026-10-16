use ymind_core::{custom_theme::CustomTheme, theme::ThemeSettings};
use ymind_platform::{Paths, Settings};

#[test]
fn custom_themes_and_opt_in_recent_choice_survive_profile_reload() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = Paths::from_root(dir.path());
    let mut theme = CustomTheme::from_settings(&ThemeSettings::default());
    theme.roles.summary.font_size = Some(25.0);
    let settings = Settings {
        custom_themes: vec![theme.clone()],
        default_custom_theme: Some(theme),
        reuse_last_theme: true,
        ..Default::default()
    };
    settings.save(&paths).unwrap();
    assert_eq!(Settings::load(&paths).unwrap(), settings);
    assert!(Settings::default().custom_themes.is_empty());
}

#[test]
fn duplicate_or_invalid_custom_themes_cannot_overwrite_the_library() {
    let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let paths = Paths::from_root(dir.path());
    let theme = CustomTheme::from_settings(&ThemeSettings::default());
    let mut settings = Settings {
        custom_themes: vec![theme.clone()],
        ..Default::default()
    };
    settings.save(&paths).unwrap();
    let original = settings.clone();
    let mut duplicate = theme.clone();
    duplicate.name = format!(" {} ", theme.name.to_uppercase());
    settings.custom_themes.push(duplicate);
    assert!(settings.save(&paths).is_err());
    assert_eq!(Settings::load(&paths).unwrap(), original);
    settings.custom_themes = vec![theme; 33];
    assert!(settings.save(&paths).is_err());
    settings.custom_themes.clear();
    let mut invalid = CustomTheme::from_settings(&ThemeSettings::default());
    invalid.palette.clear();
    settings.default_custom_theme = Some(invalid);
    assert!(settings.save(&paths).is_err());
}

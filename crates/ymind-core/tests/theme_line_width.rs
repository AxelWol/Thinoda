use ymind_core::{
    Sheet, Vec2,
    theme::{Theme, ThemeSettings, resolve_style},
};

#[test]
fn global_width_applies_to_every_theme_and_level_without_replacing_explicit_styles() {
    for id in Theme::IDS {
        let mut sheet = Sheet::new("Width", "Root");
        sheet.theme.theme_id = id.into();
        let main = sheet.add_child(sheet.root, None, "Main");
        let sub = sheet.add_child(main, None, "Sub");
        let floating = sheet.add_floating("Floating", Vec2::ZERO);
        let summary = sheet.add_summary(&[main], "Summary").unwrap();
        let ids = [sheet.root, main, sub, floating, summary];
        let original = ids.map(|id| resolve_style(&sheet, id, false));
        sheet.theme.line_width = Some(3.5);
        for (index, id) in ids.into_iter().enumerate() {
            let mut expected = original[index].clone();
            expected.line_width = 3.5;
            assert_eq!(resolve_style(&sheet, id, false), expected);
            assert_eq!(resolve_style(&sheet, id, true).line_width, 3.5);
        }
        sheet.topic_mut(main).unwrap().style.line_width = Some(7.0);
        assert_eq!(resolve_style(&sheet, main, false).line_width, 7.0);
        assert_eq!(resolve_style(&sheet, sub, false).line_width, 3.5);
        sheet.theme.line_width = None;
        assert_eq!(resolve_style(&sheet, sub, false), original[2]);
    }
}

#[test]
fn invalid_widths_fail_validation_and_legacy_settings_keep_automatic_widths() {
    let old: ThemeSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.line_width, None);
    for width in [f32::NAN, f32::INFINITY, -1.0, 0.0, 0.49, 16.01] {
        let mut sheet = Sheet::new("Invalid", "Root");
        sheet.theme.line_width = Some(width);
        assert!(sheet.validate().unwrap_err().contains("branch line width"));
    }
    for width in [0.5, 1.5, 16.0] {
        let settings = ThemeSettings {
            line_width: Some(width),
            ..Default::default()
        };
        assert!(settings.validate().is_ok());
        let roundtrip: ThemeSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(roundtrip, settings);
    }
}

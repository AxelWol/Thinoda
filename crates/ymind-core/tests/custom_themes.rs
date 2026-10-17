use ymind_core::{
    Color, Sheet, TopicShape, TopicStyle, Workbook,
    custom_theme::CustomTheme,
    theme::{Level, Theme, ThemeSettings, resolve_style},
};

#[test]
fn role_defaults_follow_topic_roles_and_never_replace_explicit_overrides() {
    let mut sheet = Sheet::new("Map", "Root");
    let main = sheet.add_child(sheet.root, None, "Main");
    let sub = sheet.add_child(main, None, "Sub");
    let floating = sheet.add_floating("Floating", Default::default());
    let summary = sheet.add_summary(&[main], "Summary").unwrap();
    let mut custom = CustomTheme::from_settings(&sheet.theme);
    for (index, (role, _id)) in [
        (Level::Central, sheet.root),
        (Level::Main, main),
        (Level::Sub, sub),
        (Level::Floating, floating),
        (Level::Summary, summary),
    ]
    .into_iter()
    .enumerate()
    {
        let style = custom.roles.get_mut(role);
        style.font_size = Some(20.0 + index as f32);
        style.shape = Some(TopicShape::Ellipse);
        style.width = Some(200.0);
        style.text_color = Some(Color::rgb(40, 50, 60));
    }
    sheet.theme.custom_theme = Some(custom);
    for dark in [false, true] {
        for (index, id) in [sheet.root, main, sub, floating, summary]
            .into_iter()
            .enumerate()
        {
            let resolved = resolve_style(&sheet, id, dark);
            assert_eq!(resolved.font_size, 20.0 + index as f32);
            assert_eq!(resolved.shape, TopicShape::Ellipse);
            assert_eq!(resolved.width, Some(200.0));
            assert_eq!(resolved.text_color, Color::rgb(40, 50, 60));
        }
    }
    sheet.topic_mut(sub).unwrap().style.font_size = Some(40.0);
    assert_eq!(resolve_style(&sheet, sub, false).font_size, 40.0);
    sheet.topic_mut(sub).unwrap().style = TopicStyle::default();
    assert_eq!(resolve_style(&sheet, sub, false).font_size, 22.0);
    sheet.theme.custom_theme = None;
    assert_eq!(resolve_style(&sheet, sub, false).font_size, 14.0);
}

#[test]
fn custom_palette_backgrounds_and_connector_precedence_work_in_both_modes() {
    let mut sheet = Sheet::new("Map", "Root");
    let main = sheet.add_child(sheet.root, None, "Main");
    let sub = sheet.add_child(main, None, "Sub");
    let mut custom = CustomTheme::from_settings(&sheet.theme);
    custom.base_theme_id = "neon".into();
    custom.palette = vec![Color::rgb(10, 20, 30), Color::rgb(30, 20, 10)];
    custom.light_background = Color::rgb(250, 240, 230);
    custom.dark_background = Color::rgb(10, 15, 20);
    custom.roles.sub.line_color = Some(Color::WHITE);
    custom.roles.sub.line_width = Some(4.0);
    custom.font_family = Some("serif".into());
    custom.roles.sub.font_family = Some("monospace".into());
    sheet.theme.custom_theme = Some(custom.clone());
    let theme = Theme::for_settings(&sheet.theme);
    assert_eq!(theme.branch_color(&sheet.theme, 0), custom.palette[0]);
    assert_eq!(theme.branch_color(&sheet.theme, 1), custom.palette[1]);
    assert_eq!(
        theme.background(&sheet.theme, false),
        custom.light_background
    );
    assert_eq!(theme.background(&sheet.theme, true), custom.dark_background);
    assert_eq!(resolve_style(&sheet, sub, false).line_color, Color::WHITE);
    assert_eq!(resolve_style(&sheet, sub, false).line_width, 4.0);
    assert_eq!(
        resolve_style(&sheet, sub, false).font_family.as_deref(),
        Some("monospace")
    );
    sheet.theme.font_family = Some("sans-serif".into());
    assert_eq!(
        resolve_style(&sheet, sub, false).font_family.as_deref(),
        Some("sans-serif")
    );
    sheet.theme.line_width = Some(2.0);
    sheet.topic_mut(main).unwrap().style.line_color = Some(Color::BLACK);
    assert_eq!(resolve_style(&sheet, sub, false).line_color, Color::BLACK);
    assert_eq!(resolve_style(&sheet, sub, false).line_width, 2.0);
    sheet.topic_mut(sub).unwrap().style.line_width = Some(3.0);
    sheet.topic_mut(sub).unwrap().style.font_family = Some("fantasy".into());
    assert_eq!(resolve_style(&sheet, sub, false).line_width, 3.0);
    assert_eq!(
        resolve_style(&sheet, sub, false).font_family.as_deref(),
        Some("fantasy")
    );
    sheet.theme.background = Some(Color::WHITE);
    assert_eq!(theme.background(&sheet.theme, true), Color::WHITE);
}

#[test]
fn custom_theme_validation_and_legacy_default_are_explicit() {
    let legacy: ThemeSettings = serde_json::from_str("{}").unwrap();
    assert!(legacy.custom_theme.is_none());
    let theme = CustomTheme::from_settings(&legacy);
    theme.validate().unwrap();
    for field in ["name", "base", "palette", "font"] {
        let mut invalid = theme.clone();
        match field {
            "name" => invalid.name.clear(),
            "base" => invalid.base_theme_id = "missing".into(),
            "palette" => invalid.palette.clear(),
            "font" => invalid.roles.central.font_size = Some(f32::NAN),
            _ => unreachable!(),
        }
        assert!(invalid.validate().is_err());
    }
    let mut settings = legacy;
    settings.custom_theme = Some(theme);
    assert_eq!(
        serde_json::from_str::<ThemeSettings>(&serde_json::to_string(&settings).unwrap()).unwrap(),
        settings
    );
}

#[test]
fn masked_copies_remove_custom_theme_names_and_fonts_but_keep_visual_defaults() {
    let mut book = Workbook::default();
    let mut custom = CustomTheme::from_settings(&book.active().theme);
    custom.name = "SECRET theme".into();
    custom.font_family = Some("SECRET font".into());
    custom.roles.main.font_family = Some("SECRET role font".into());
    custom.roles.main.font_size = Some(25.0);
    book.active_mut().theme.custom_theme = Some(custom);
    let source = book.clone();
    let masked = book.masked_copy();
    masked.validate().unwrap();
    assert!(!serde_json::to_string(&masked).unwrap().contains("SECRET"));
    assert_eq!(
        masked
            .active()
            .theme
            .custom_theme
            .as_ref()
            .unwrap()
            .roles
            .main
            .font_size,
        Some(25.0)
    );
    assert_eq!(book, source);
    assert_eq!(masked.masked_copy(), masked);
}

use thinoda_core::{
    Color, Sheet, TopicStyle,
    quick_styles::{PRESETS, QuickStyle, preset},
    theme::{Theme, resolve_style},
};

#[test]
fn original_quick_styles_are_valid_and_replace_explicit_appearance() {
    assert!(preset("unknown").is_none());
    for name in PRESETS {
        let preset = preset(name).unwrap();
        preset.validate().unwrap();
        let mut sheet = Sheet::new("Test", "Content");
        let root = sheet.root;
        let child = sheet.add_child(root, None, "Child");
        sheet
            .update_style(&[root, child], |style| *style = preset.style.clone())
            .unwrap();
        for dark in [false, true] {
            let a = resolve_style(&sheet, root, dark);
            let b = resolve_style(&sheet, child, dark);
            assert_eq!(a.fill, b.fill);
            assert_eq!(a.text_color, b.text_color);
            assert_eq!(a.font_size, b.font_size);
            assert_eq!(a.bold, b.bold);
            assert_eq!(a.line_color, b.line_color);
        }
    }
}

#[test]
fn saved_appearance_keeps_all_effective_fields_across_roles_themes_and_modes() {
    for theme in Theme::IDS {
        for dark in [false, true] {
            let mut sheet = Sheet::new("Test", "Content");
            sheet.theme.theme_id = theme.into();
            sheet.theme.font_family = Some("monospace".into());
            let main = sheet.add_child(sheet.root, None, "Branch");
            let sub = sheet.add_child(main, None, "Leaf");
            for source in [sheet.root, main, sub] {
                let before = resolve_style(&sheet, source, dark);
                let saved = QuickStyle::from_appearance("Saved".into(), &before);
                saved.validate().unwrap();
                let mut target = Sheet::new("Target", "Different content");
                target.theme.theme_id = "ocean".into();
                target.topic_mut(target.root).unwrap().style = saved.style;
                let after = resolve_style(&target, target.root, !dark);
                let mut expected = before;
                if expected.fill.is_none() {
                    expected.fill = Some(Color::TRANSPARENT);
                }
                if expected.border_color.is_none() {
                    expected.border_color = Some(Color::TRANSPARENT);
                    expected.border_width = 0.0;
                }
                // Defaults use the generic proportional family after capture.
                if expected.font_family.is_none() {
                    expected.font_family = Some("sans-serif".into());
                }
                assert_eq!(after, expected, "{theme}, dark={dark}, source={source:?}");
            }
        }
    }
}

#[test]
fn quick_style_validation_rejects_bad_names_and_nonfinite_or_invalid_dimensions() {
    for name in ["", "  ", "bad\nname", &"x".repeat(81)] {
        assert!(
            QuickStyle {
                name: name.into(),
                style: TopicStyle::default()
            }
            .validate()
            .is_err()
        );
    }
    for value in [f32::NAN, f32::INFINITY, -1.0, 10001.0] {
        let style = TopicStyle {
            width: Some(value),
            ..Default::default()
        };
        assert!(
            QuickStyle {
                name: "Size".into(),
                style
            }
            .validate()
            .is_err()
        );
    }
}

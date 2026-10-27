use std::io::Cursor;

use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{CenterMode, Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Vec2, attachments::Attachment, images, resources::Resource};
use thinoda_platform::settings::{Language, Settings, ThemeMode};

fn png(color: [u8; 3]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 16, image::Rgb(color)))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn fixture(theme: ThemeMode) -> (ThinodaApp, thinoda_core::TopicId, thinoda_core::TopicId) {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            theme,
            show_format_panel: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.exec(&egui::Context::default(), Cmd::New);
    app.mode = CenterMode::Outliner;
    let doc = app.doc_mut().unwrap();
    let root = doc.doc.sheet().root;
    let mut image_topic = root;
    let mut plain_topic = root;
    doc.doc
        .edit_sheet("Outliner image previews", |sheet| {
            sheet.set_title(root, "Root")?;
            image_topic = sheet.add_child(root, None, "Embedded image");
            plain_topic = sheet.add_child(root, None, "No image");
            let legacy_path = sheet.add_child(root, None, "Legacy path");
            let legacy_url = sheet.add_child(root, None, "Legacy URL");
            let mismatched = sheet.add_child(root, None, "Mismatched media");
            let unsupported_topic = sheet.add_child(root, None, "Unsupported image bytes");
            let attachment = sheet.add_child(root, None, "Opaque attachment");

            let image_resource =
                Resource::new("preview.png", "image/png", png([20, 130, 240])).unwrap();
            images::set_image(
                sheet,
                image_topic,
                Some((image_resource, Vec2::new(64.0, 32.0))),
            )
            .unwrap();
            let image_reference = sheet
                .topic(image_topic)
                .and_then(|topic| topic.image.clone())
                .expect("test image was not attached");

            sheet.topic_mut(legacy_path).unwrap().image =
                Some(r"C:\private\local-image.png".into());
            sheet.topic_mut(legacy_url).unwrap().image =
                Some("https://example.invalid/local-image.png".into());

            // Correct PNG bytes with opaque attachment metadata are not a trusted image.
            let opaque =
                Resource::new("opaque.bin", "application/octet-stream", png([240, 40, 20]))
                    .unwrap();
            let opaque_reference = sheet.resources.insert(opaque).unwrap();
            sheet.topic_mut(mismatched).unwrap().image = Some(opaque_reference);
            let unsupported_resource =
                Resource::new("unsupported.png", "image/png", b"not an image".to_vec()).unwrap();
            let unsupported_reference = sheet.resources.insert(unsupported_resource).unwrap();
            sheet.topic_mut(unsupported_topic).unwrap().image = Some(unsupported_reference);
            sheet.topic_mut(attachment).unwrap().attachment = Some(Attachment {
                resource: image_reference,
                name: "preview.png".into(),
            });
            Ok(())
        })
        .unwrap();
    doc.view.select_only(image_topic);
    (app, image_topic, plain_topic)
}

#[test]
fn outliner_previews_only_valid_embedded_images_without_changing_navigation_or_selection() {
    for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
        let (app, image_topic, plain_topic) = fixture(theme);
        let mut h = Harness::builder()
            .with_size(egui::vec2(960.0, 720.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(4);

        let preview_label = format!("{}: Embedded image", t("Image preview"));
        let preview_rect = h.get_by_role_and_label(Role::Image, &preview_label).rect();
        assert!(h.ctx.content_rect().contains_rect(preview_rect));
        assert!(preview_rect.width() <= 48.0);
        assert!(preview_rect.height() <= 32.0);
        for title in [
            "Root",
            "No image",
            "Legacy path",
            "Legacy URL",
            "Mismatched media",
            "Unsupported image bytes",
            "Opaque attachment",
        ] {
            let label = format!("{}: {title}", t("Image preview"));
            assert!(
                h.query_by_role_and_label(Role::Image, &label).is_none(),
                "unexpected preview for {title}"
            );
        }

        h.get_by_role_and_label(Role::Button, "Embedded image")
            .click();
        h.run_steps(3);
        assert_eq!(
            h.state().doc().unwrap().view.primary(),
            Some(image_topic),
            "selecting the Outliner title updates the shared document selection"
        );
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().view.primary(), Some(plain_topic));
        assert!(
            h.get_by_role_and_label(Role::Button, "No image")
                .is_focused()
        );

        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
        h.run_steps(4);
        assert_eq!(h.state().mode, CenterMode::Map);
        assert_eq!(h.state().doc().unwrap().view.primary(), Some(plain_topic));
        h.state_mut().exec(&ctx, Cmd::ToggleOutliner);
        h.run_steps(4);
        assert_eq!(h.state().mode, CenterMode::Outliner);
        assert_eq!(h.state().doc().unwrap().view.primary(), Some(plain_topic));
        assert!(
            h.get_by_role_and_label(Role::Button, "No image")
                .is_focused()
        );
        h.get_by_role_and_label(Role::Button, "Embedded image")
            .click();
        h.key_press(egui::Key::F2);
        h.run_steps(3);
        h.get_by_role_and_label(Role::TextInput, t("Topic title"));
    }
}

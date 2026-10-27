//! Ownership-safe local image picker. No URL/path execution.
use crate::{app::ThinodaApp, files, i18n::t};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use thinoda_core::{TopicId, images, resources::Resource};

#[derive(Clone, Debug)]
pub(crate) struct ImageRequest {
    pub request: u64,
    document: String,
    sheet: usize,
    root: TopicId,
    topic: TopicId,
    revision: u64,
}

impl ImageRequest {
    fn matches(&self, doc: &crate::doc::OpenDoc) -> bool {
        self.document == doc.key
            && self.sheet == doc.doc.active_index()
            && self.root == doc.doc.sheet().root
            && self.revision == doc.doc.state_id()
            && doc.view.selection == [self.topic]
            && doc.view.selected_object.is_none()
            && !doc.has_inspector_changes()
    }
}

impl ThinodaApp {
    pub(crate) fn begin_image(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.doc_mut() else { return };
        if doc.view.selection.len() != 1 || doc.view.selected_object.is_some() {
            return;
        }
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        let Some(topic) = doc.view.primary() else {
            return;
        };
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let owner = ImageRequest {
            request: NEXT.fetch_add(1, Ordering::Relaxed),
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            topic,
            revision: doc.doc.state_id(),
        };
        let request = owner.request;
        self.image_request = Some(owner);
        if let Some(paths) = files::pick_files(
            files::Purpose::Image(request),
            Some(t("Local Image")),
            &[("PNG / JPEG", &["png", "jpg", "jpeg"])],
            None,
            false,
        ) {
            if let Some(path) = paths.first() {
                self.finish_image(request, path);
            }
            self.image_request = None;
        } else if !files::is_web() {
            self.image_request = None;
        }
    }

    pub(crate) fn finish_image(&mut self, request: u64, path: &Path) {
        let Some(owner) = self
            .image_request
            .as_ref()
            .filter(|r| r.request == request)
            .cloned()
        else {
            return;
        };
        if !self.doc().is_some_and(|d| owner.matches(d)) {
            self.toasts
                .error(t("Image request expired. Select a topic and try again."));
            return;
        }
        let result = (|| -> Result<(), String> {
            let bytes = read_image(path)?;
            let decoded = images::decode(&bytes)?;
            let size = images::default_size(decoded.width, decoded.height);
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("invalid image filename")?;
            let resource = Resource::new(name, decoded.media_type, bytes)?;
            let doc = self.doc_mut().ok_or("image document is closed")?;
            doc.doc
                .edit_sheet("Local Image", |sheet| {
                    images::set_image(sheet, owner.topic, Some((resource, size)))
                        .map_err(thinoda_core::edit::EditError::Invalid)
                })
                .map_err(|e| e.to_string())?;
            self.zen = false;
            self.settings.show_format_panel = true;
            self.sidebar_family = crate::app::SidebarFamily::Format;
            self.format_tab = crate::app::FormatTab::Style;
            Ok(())
        })();
        if let Err(error) = result {
            self.toasts
                .error(format!("{}: {error}", t("Could not insert image")));
        }
    }
}

fn read_image(path: &Path) -> Result<Vec<u8>, String> {
    let max = thinoda_core::resources::MAX_RESOURCE_BYTES;
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        bytes
    };
    #[cfg(target_arch = "wasm32")]
    let bytes = thinoda_platform::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("image exceeds 8 MiB".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_platform::settings::ThemeMode;

    fn encoded(color: u8) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            40,
            20,
            image::Rgb([color, 10, 20]),
        ))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        bytes.into_inner()
    }

    fn owner(app: &mut ThinodaApp, request: u64) -> ImageRequest {
        let doc = app.doc_mut().unwrap();
        let topic = doc.doc.sheet().root;
        doc.view.select_only(topic);
        ImageRequest {
            request,
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: topic,
            topic,
            revision: doc.doc.state_id(),
        }
    }

    #[test]
    fn image_picker_rejects_stale_request_selection_document_sheet_and_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(250)).unwrap();
        for stale in 0..6 {
            let mut app = ThinodaApp::headless();
            let request = owner(&mut app, 10);
            app.image_request = Some(request);
            match stale {
                0 => {
                    app.image_request.as_mut().unwrap().request = 11;
                }
                1 => {
                    app.doc_mut().unwrap().view.selection.clear();
                }
                2 => {
                    app.doc_mut().unwrap().key.push_str("-other-document");
                }
                3 => {
                    app.image_request.as_mut().unwrap().sheet += 1;
                }
                4 => {
                    app.image_request.as_mut().unwrap().root.0 += 1;
                }
                _ => {
                    app.doc_mut()
                        .unwrap()
                        .doc
                        .edit_sheet("Change", |s| s.set_title(s.root, "Changed"))
                        .unwrap();
                }
            }
            let before = app.doc().unwrap().doc.workbook().clone();
            app.finish_image(10, &path);
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[test]
    fn image_picker_insert_replace_invalid_source_and_undo_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        let mut app = ThinodaApp::headless();
        for color in [200, 250] {
            std::fs::write(&path, encoded(color)).unwrap();
            app.image_request = Some(owner(&mut app, color as u64));
            app.finish_image(color as u64, &path);
            let doc = app.doc().unwrap();
            assert_eq!(doc.doc.sheet().resources.entries.len(), 1);
            let topic = doc.doc.sheet().root_topic();
            assert_eq!(topic.image_size, Some(thinoda_core::Vec2::new(40.0, 20.0)));
        }
        let before = app.doc().unwrap().doc.workbook().clone();
        std::fs::write(&path, b"not an image").unwrap();
        app.image_request = Some(owner(&mut app, 500));
        app.finish_image(500, &path);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(std::fs::read(path).unwrap(), b"not an image");
        app.doc_mut().unwrap().doc.undo();
        assert_ne!(app.doc().unwrap().doc.workbook(), &before);
        app.doc_mut().unwrap().doc.redo();
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[test]
    fn image_inspector_context_placement_reset_remove_and_themes() {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut app = ThinodaApp::headless();
            app.settings.theme = theme;
            let request = owner(&mut app, 1);
            let doc = app.doc_mut().unwrap();
            let resource = Resource::new("evidence.png", "image/png", encoded(250)).unwrap();
            doc.doc
                .edit_sheet("Image", |s| {
                    images::set_image(
                        s,
                        request.topic,
                        Some((resource, thinoda_core::Vec2::new(200.0, 100.0))),
                    )
                    .map_err(thinoda_core::edit::EditError::Invalid)
                })
                .unwrap();
            let mut h = Harness::builder()
                .with_size(egui::vec2(420.0, 600.0))
                .with_context_setup(|ctx| {
                    ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
                })
                .build_ui_state(
                    |ui, app: &mut ThinodaApp| {
                        app.apply_appearance(ui.ctx());
                        app.image_properties(ui);
                    },
                    app,
                );
            h.run_steps(3);
            h.get_by_label("evidence.png");
            h.get_by_label(t("Image width")).click();
            h.run_steps(2);
            h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
            h.run_steps(1);
            h.get_by_label(t("Image width")).type_text("300");
            h.run_steps(2);
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(
                h.state().doc().unwrap().doc.sheet().root_topic().image_size,
                Some(thinoda_core::Vec2::new(300.0, 150.0))
            );
            h.get_by_label(t("Image below topic")).click();
            h.run_steps(3);
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .image_below
            );
            h.get_by_label(t("Reset Size")).click();
            h.run_steps(3);
            assert_eq!(
                h.state().doc().unwrap().doc.sheet().root_topic().image_size,
                Some(thinoda_core::Vec2::new(40.0, 20.0))
            );
            h.get_by_label(t("Remove image")).click();
            h.run_steps(3);
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .image
                    .is_none()
            );
            h.state_mut().doc_mut().unwrap().doc.undo();
            h.run_steps(3);
            assert!(
                h.state()
                    .doc()
                    .unwrap()
                    .doc
                    .sheet()
                    .root_topic()
                    .image
                    .is_some()
            );
        }
        for lang in crate::i18n::Lang::ALL {
            for key in [
                "Local Image",
                "Insert image",
                "Replace image",
                "Remove image",
                "Image width",
                "Image below topic",
                "Reset Size",
                "PNG/JPEG only. Resizing keeps the aspect ratio.",
                "Could not insert image",
                "Image request expired. Select a topic and try again.",
            ] {
                assert!(!crate::i18n::t_in(lang, key).is_empty());
                if lang != crate::i18n::Lang::En {
                    assert_ne!(crate::i18n::t_in(lang, key), key);
                }
            }
        }
    }

    #[test]
    fn narrow_image_inspector_does_not_clamp_or_edit_on_each_frame() {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::new(1, 4096))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let resource = Resource::new("portrait.png", "image/png", bytes.into_inner()).unwrap();
        let mut app = ThinodaApp::headless();
        let request = owner(&mut app, 1);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Image", |s| {
                images::set_image(
                    s,
                    request.topic,
                    Some((resource, images::default_size(1, 4096))),
                )
                .map_err(thinoda_core::edit::EditError::Invalid)
            })
            .unwrap();
        let before = app.doc().unwrap().doc.workbook().clone();
        let revision = app.doc().unwrap().doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(420.0, 600.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.image_properties(ui), app);
        h.run_steps(12);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
    }
}

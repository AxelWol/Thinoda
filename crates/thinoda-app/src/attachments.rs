//! User-mediated attachment insertion and saving. Never launches or parses a file.
use crate::{
    app::{Cmd, ThinodaApp},
    files,
    i18n::t,
    images::ImageRequest,
};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
use thinoda_core::attachments;

impl ThinodaApp {
    pub(crate) fn begin_attachment(&mut self, ctx: &egui::Context) {
        if !self.command_available(&Cmd::Attachment) {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.flush_inspector_edit(ctx) {
            self.toasts.error(error);
            return;
        }
        doc.view.commit_edit(&mut doc.doc);
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let request = NEXT.fetch_add(1, Ordering::Relaxed);
        self.attachment_request = ImageRequest::capture(doc, request);
        if let Some(paths) = files::pick_files(
            files::Purpose::Attachment(request),
            Some(t("Attachment")),
            &[],
            None,
            false,
        ) {
            self.complete_attachment_pick(request, &paths);
        } else if !files::is_web() {
            self.attachment_request = None;
        }
    }

    /// Also used for cancellation. A late older completion must not clear a newer owner.
    pub(crate) fn complete_attachment_pick(&mut self, request: u64, paths: &[std::path::PathBuf]) {
        if let Some(path) = paths.first() {
            self.finish_attachment(request, path);
        }
        if self
            .attachment_request
            .as_ref()
            .is_some_and(|r| r.request == request)
        {
            self.attachment_request = None;
        }
    }

    pub(crate) fn finish_attachment(&mut self, request: u64, path: &Path) {
        let Some(owner) = self
            .attachment_request
            .as_ref()
            .filter(|r| r.request == request)
            .cloned()
        else {
            return;
        };
        if !self.doc().is_some_and(|d| owner.matches(d)) {
            self.toasts.error(t(
                "Attachment request expired. Select a topic and try again.",
            ));
            return;
        }
        let result = (|| -> Result<(), String> {
            let bytes = crate::images::read_local_resource(path)?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("invalid attachment filename")?;
            let doc = self.doc_mut().ok_or("attachment document is closed")?;
            let id = doc
                .doc
                .edit_sheet("Attachment", |sheet| {
                    attachments::insert(sheet, owner.topic, name, bytes)
                })
                .map_err(|e| e.to_string())?;
            doc.view.select_only(id);
            self.zen = false;
            self.settings.show_format_panel = true;
            self.sidebar_family = crate::app::SidebarFamily::Format;
            self.format_tab = crate::app::FormatTab::Data;
            Ok(())
        })();
        if let Err(error) = result {
            self.toasts
                .error(format!("{}: {error}", t("Could not insert attachment")));
        }
    }

    pub(crate) fn save_attachment(&mut self) {
        if !self.command_available(&Cmd::SaveAttachment) {
            return;
        }
        let result = (|| -> Result<(), String> {
            let doc = self.doc().ok_or("attachment document is closed")?;
            let topic = doc
                .view
                .primary()
                .and_then(|id| doc.doc.sheet().topic(id))
                .ok_or("attachment topic is missing")?;
            let attachment = topic.attachment.as_ref().ok_or("attachment is missing")?;
            let resource = doc.doc.sheet().resources.get(&attachment.resource)?.clone();
            let name = attachments::safe_name(&attachment.name);
            // Separate explicit action; the chooser/prompt is the user's consent.
            let Some(path) = files::save_target(
                Some(t("Save/Download attachment")),
                &[],
                &name,
                None,
                files::Target::Download,
            ) else {
                return Ok(());
            };
            save_to(&path, resource.bytes())
        })();
        if let Err(error) = result {
            self.toasts
                .error(format!("{}: {error}", t("Could not save attachment")));
        }
    }

    pub(crate) fn attachment_properties(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = self.doc() else { return };
        let Some(topic) = doc.view.primary().and_then(|id| doc.doc.sheet().topic(id)) else {
            return;
        };
        let Some(attachment) = topic.attachment.clone() else {
            return;
        };
        let size = doc
            .doc
            .sheet()
            .resources
            .get(&attachment.resource)
            .ok()
            .map(|r| r.bytes().len());
        let enabled = self.command_available(&Cmd::SaveAttachment) && !self.palette.open;
        let mut save = false;
        thinoda_ui::widgets::section(ui, t("Attachment"), true, |ui| {
            ui.label(attachments::safe_name(&attachment.name));
            if let Some(size) = size {
                ui.label(format!("{size} {}", t("bytes")));
            }
            ui.label(t("Opaque file (not inspected)"));
            save = ui
                .add_enabled(enabled, egui::Button::new(t("Save/Download attachment")))
                .clicked();
            thinoda_ui::widgets::caption(
                ui,
                t(
                    "Files stay embedded. Save only files you trust; nothing is opened automatically.",
                ),
            );
        });
        if save && thinoda_ui::widgets::touch_activation_allowed(ui.ctx()) {
            self.exec(ui.ctx(), Cmd::SaveAttachment);
        }
    }
}

fn save_to(path: &Path, bytes: &[u8]) -> Result<(), String> {
    thinoda_platform::fs::atomic_write(path, bytes, false).map_err(|e| e.to_string())?;
    files::deliver_attachment(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_platform::settings::ThemeMode;

    fn owner(app: &mut ThinodaApp, request: u64) -> ImageRequest {
        let doc = app.doc_mut().unwrap();
        doc.view.select_only(doc.doc.sheet().root);
        ImageRequest::capture(doc, request).unwrap()
    }

    #[test]
    fn picker_is_owned_cancellable_and_stale_completion_cannot_insert() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.docm");
        std::fs::write(&path, b"\0opaque\xff").unwrap();
        for stale in 0..7 {
            let mut app = ThinodaApp::headless();
            let mut request = owner(&mut app, 10);
            match stale {
                0 => request.request = 11,
                1 => app.doc_mut().unwrap().view.selection.clear(),
                2 => app.doc_mut().unwrap().key.push_str("-other-document"),
                3 => {
                    app.doc_mut()
                        .unwrap()
                        .doc
                        .add_sheet(thinoda_core::Sheet::new("Other", "Root"))
                        .unwrap();
                }
                4 => {
                    app.doc_mut()
                        .unwrap()
                        .doc
                        .edit_sheet("Change", |s| s.set_title(s.root, "Changed"))
                        .unwrap();
                }
                5 => app.docs.clear(),
                _ => app
                    .doc_mut()
                    .unwrap()
                    .view
                    .select_only(thinoda_core::TopicId(u64::MAX)),
            }
            app.attachment_request = Some(request);
            let before = app.doc().map(|d| d.doc.workbook().clone());
            app.complete_attachment_pick(10, std::slice::from_ref(&path));
            assert_eq!(app.doc().map(|d| d.doc.workbook().clone()), before);
            if stale == 0 {
                assert_eq!(app.attachment_request.as_ref().unwrap().request, 11);
            } else {
                assert!(app.attachment_request.is_none());
            }
        }
        let mut app = ThinodaApp::headless();
        app.attachment_request = Some(owner(&mut app, 20));
        let before = app.doc().unwrap().doc.workbook().clone();
        app.complete_attachment_pick(20, &[]);
        assert!(app.attachment_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[test]
    fn insert_save_exact_bytes_undo_and_invalid_source_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("evidence.docm");
        let bytes = b"\0PK\x03\x04opaque macro data\xff";
        std::fs::write(&source, bytes).unwrap();
        let mut app = ThinodaApp::headless();
        let before = app.doc().unwrap().doc.workbook().clone();
        assert!(!app.command_available(&Cmd::SaveAttachment));
        app.attachment_request = Some(owner(&mut app, 1));
        app.complete_attachment_pick(1, std::slice::from_ref(&source));
        assert!(app.command_available(&Cmd::SaveAttachment));
        let doc = app.doc().unwrap();
        let topic = doc.doc.sheet().topic(doc.view.primary().unwrap()).unwrap();
        assert_eq!(topic.parent, Some(doc.doc.sheet().root));
        let attachment = topic.attachment.as_ref().unwrap();
        assert_eq!(attachment.name, "evidence.docm");
        let resource = doc.doc.sheet().resources.get(&attachment.resource).unwrap();
        assert_eq!(resource.media_type, "application/octet-stream");
        let destination = dir.path().join("saved.docm");
        save_to(&destination, resource.bytes()).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        let attached = app.doc().unwrap().doc.workbook().clone();
        app.doc_mut().unwrap().doc.undo();
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        app.doc_mut().unwrap().doc.redo();
        assert_eq!(app.doc().unwrap().doc.workbook(), &attached);
        for content in [
            vec![],
            vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1],
        ] {
            std::fs::write(&source, content).unwrap();
            app.attachment_request = Some(owner(&mut app, 2));
            app.complete_attachment_pick(2, std::slice::from_ref(&source));
            assert_eq!(app.doc().unwrap().doc.workbook(), &attached);
        }
        assert!(save_to(dir.path(), bytes).is_err());
    }

    #[test]
    fn attachment_inspector_themes_and_all_catalogs() {
        for theme in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::System] {
            let mut app = ThinodaApp::headless();
            app.settings.theme = theme;
            let doc = app.doc_mut().unwrap();
            let root = doc.doc.sheet().root;
            let id = doc
                .doc
                .edit_sheet("Attachment", |s| {
                    attachments::insert(s, root, "evidence.docm", vec![0, 255])
                })
                .unwrap();
            doc.view.select_only(id);
            let before = doc.doc.workbook().clone();
            let mut h = Harness::builder()
                .with_size(egui::vec2(420.0, 600.0))
                .build_ui_state(
                    |ui, app: &mut ThinodaApp| {
                        app.apply_appearance(ui.ctx());
                        app.attachment_properties(ui);
                    },
                    app,
                );
            h.run_steps(4);
            h.get_by_label("evidence.docm");
            h.get_by_label(t("Save/Download attachment"));
            h.get_by_label(t("Opaque file (not inspected)"));
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        }
        for lang in crate::i18n::Lang::ALL {
            for key in [
                "Attachment",
                "Save/Download attachment",
                "bytes",
                "Opaque file (not inspected)",
                "Could not insert attachment",
                "Could not save attachment",
                "Attachment request expired. Select a topic and try again.",
                "Files stay embedded. Save only files you trust; nothing is opened automatically.",
                "Select a topic, then Insert > Attachment to add a local file as a child topic (1 byte to 8 MiB). Style > Attachment offers a separate Save/Download action. Files are opaque: nothing is opened or executed. Save as .thinoda to retain attachments; other exports are not supported yet.",
            ] {
                assert!(!crate::i18n::t_in(lang, key).is_empty());
                if lang != crate::i18n::Lang::En && key != "bytes" {
                    assert_ne!(crate::i18n::t_in(lang, key), key);
                }
            }
        }
    }
}

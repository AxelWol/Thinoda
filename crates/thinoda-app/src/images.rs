//! Ownership-safe local image picker. No URL/path execution.
use crate::{
    app::{CenterMode, ThinodaApp},
    files,
    i18n::t,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use thinoda_canvas::{ObjectSelection, Xf};
use thinoda_core::{
    ApproxMeasure, CanvasObjectId, Card, CardKind, Sheet, TextMeasure, TopicId, Vec2, images,
    resources::Resource,
};

fn next_request_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["png", "jpg", "jpeg"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[derive(Clone, Debug)]
pub(crate) struct ImageRequest {
    pub request: u64,
    document: String,
    sheet: usize,
    root: TopicId,
    // The topic target is unused for board requests. Keeping it here lets
    // attachments continue to share the existing topic-request ownership.
    pub(crate) topic: TopicId,
    revision: u64,
    target: ImageTarget,
}

#[derive(Clone, Debug)]
enum ImageTarget {
    Topic,
    Board {
        topics: Vec<TopicId>,
        objects: Vec<CanvasObjectId>,
        selected_object: Option<ObjectSelection>,
    },
}

enum ImageSource<'a> {
    Path(&'a Path),
    Upload(files::ImageUpload),
}

impl ImageRequest {
    pub(crate) fn matches(&self, doc: &crate::doc::OpenDoc) -> bool {
        self.document == doc.key
            && self.sheet == doc.doc.active_index()
            && self.root == doc.doc.sheet().root
            && self.revision == doc.doc.state_id()
            && !doc.has_inspector_changes()
            && match &self.target {
                ImageTarget::Topic => {
                    doc.view.selection == [self.topic] && doc.view.selected_object.is_none()
                }
                ImageTarget::Board {
                    topics,
                    objects,
                    selected_object,
                } => {
                    doc.view.selection == *topics
                        && doc.view.board_selection == *objects
                        && doc.view.selected_object == *selected_object
                        && doc.view.editing_topic().is_none()
                        && doc.view.editing_board_object().is_none()
                        && doc.fim_edit.is_none()
                }
            }
    }

    pub(crate) fn capture(doc: &crate::doc::OpenDoc, request: u64) -> Option<Self> {
        Some(Self {
            request,
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            topic: doc.view.primary()?,
            revision: doc.doc.state_id(),
            target: ImageTarget::Topic,
        })
    }

    fn capture_board(doc: &crate::doc::OpenDoc, request: u64) -> Self {
        Self {
            request,
            document: doc.key.clone(),
            sheet: doc.doc.active_index(),
            root: doc.doc.sheet().root,
            topic: doc.doc.sheet().root,
            revision: doc.doc.state_id(),
            target: ImageTarget::Board {
                topics: doc.view.selection.clone(),
                objects: doc.view.board_selection.clone(),
                selected_object: doc.view.selected_object,
            },
        }
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
        let Some(owner) = ImageRequest::capture(doc, next_request_id()) else {
            return;
        };
        self.start_image_pick(owner, files::is_web(), pick_image);
    }

    /// Opens the local image picker to create a floating image card on the active sheet.
    ///
    /// Unlike topic image insertion, this does not require a selected topic. The
    /// card is added only after a valid pick and is centered in the current canvas.
    /// This action is available only while the map canvas is visible.
    pub fn begin_board_image(&mut self, ctx: &egui::Context) {
        self.begin_board_image_with_picker(ctx, files::is_web(), pick_image);
    }

    fn begin_board_image_with_picker(
        &mut self,
        ctx: &egui::Context,
        asynchronous: bool,
        picker: impl FnOnce(u64) -> Option<Vec<PathBuf>>,
    ) {
        if self.mode != CenterMode::Map {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc
            .flush_inspector_edit(ctx)
            .and_then(|_| doc.commit_canvas_drafts())
        {
            self.toasts.error(error);
            return;
        }
        let owner = ImageRequest::capture_board(doc, next_request_id());
        self.start_image_pick(owner, asynchronous, picker);
    }

    fn start_image_pick(
        &mut self,
        owner: ImageRequest,
        asynchronous: bool,
        picker: impl FnOnce(u64) -> Option<Vec<PathBuf>>,
    ) {
        let request = owner.request;
        self.image_request = Some(owner);
        match picker(request) {
            Some(paths) => self.complete_image_pick(request, &paths),
            None if !asynchronous => self.complete_image_pick(request, &[]),
            None => {}
        }
    }

    /// A canceled or late completion must not clear a newer request.
    fn complete_image_pick(&mut self, request: u64, paths: &[PathBuf]) {
        if let Some(path) = paths.first() {
            self.finish_image(request, path);
        }
        self.clear_image_request(request);
    }

    /// Completes a browser picker without persisting its transient image bytes.
    pub(crate) fn complete_image_upload(&mut self, request: u64, upload: files::ImageUpload) {
        if let Some(owner) = self
            .image_request
            .as_ref()
            .filter(|owner| owner.request == request)
            .cloned()
        {
            self.finish_owned_image(&owner, ImageSource::Upload(upload));
        }
        self.clear_image_request(request);
    }

    fn clear_image_request(&mut self, request: u64) {
        if self
            .image_request
            .as_ref()
            .is_some_and(|owner| owner.request == request)
        {
            self.image_request = None;
        }
    }

    pub(crate) fn capture_drop_owner(&mut self, ctx: &egui::Context) -> Option<ImageRequest> {
        let result: Result<Option<ImageRequest>, String> = (|| {
            let doc = self.doc_mut().ok_or_else(|| "no map is open".to_owned())?;
            if doc.view.selection.len() != 1 || doc.view.selected_object.is_some() {
                return Ok(None);
            }
            doc.flush_inspector_edit(ctx)?;
            doc.view.commit_edit(&mut doc.doc);
            Ok(ImageRequest::capture(doc, next_request_id()))
        })();
        match result {
            Ok(owner) => owner,
            Err(error) => {
                self.toasts.error(error);
                None
            }
        }
    }

    pub(crate) fn finish_dropped_image(&mut self, owner: Option<ImageRequest>, path: &Path) {
        self.finish_dropped_image_source(owner, ImageSource::Path(path));
    }

    pub(crate) fn finish_dropped_image_upload(
        &mut self,
        owner: Option<ImageRequest>,
        upload: files::ImageUpload,
    ) {
        self.finish_dropped_image_source(owner, ImageSource::Upload(upload));
    }

    fn finish_dropped_image_source(
        &mut self,
        owner: Option<ImageRequest>,
        source: ImageSource<'_>,
    ) {
        let Some(owner) = owner else {
            self.toasts.error(t("Select a topic first"));
            return;
        };
        // A delayed drop owns its own target; it must not replace a pending picker.
        self.finish_owned_image(&owner, source);
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
        self.finish_owned_image(&owner, ImageSource::Path(path));
    }

    fn finish_owned_image(&mut self, owner: &ImageRequest, source: ImageSource<'_>) {
        if (matches!(&owner.target, ImageTarget::Board { .. }) && self.mode != CenterMode::Map)
            || !self.doc().is_some_and(|d| owner.matches(d))
        {
            let message = match &owner.target {
                ImageTarget::Topic => t("Image request expired. Select a topic and try again."),
                ImageTarget::Board { .. } => t("The document changed. Try again."),
            };
            self.toasts.error(message);
            return;
        }
        let result = (|| -> Result<(), String> {
            let (name, bytes) = match source {
                ImageSource::Path(path) => {
                    let bytes = read_local_resource(path)?;
                    let name = path.file_name().and_then(|s| s.to_str()).map(str::to_owned);
                    (name, bytes)
                }
                ImageSource::Upload(upload) => {
                    if upload.bytes.len() > thinoda_core::resources::MAX_RESOURCE_BYTES {
                        return Err("local resource exceeds 8 MiB".into());
                    }
                    (Some(upload.name), upload.bytes)
                }
            };
            let decoded = images::decode(&bytes)?;
            let size = images::default_size(decoded.width, decoded.height);
            let name = name.ok_or("invalid image filename")?;
            let resource = Resource::new(&name, decoded.media_type, bytes)?;
            let doc = self.doc_mut().ok_or("image document is closed")?;
            match &owner.target {
                ImageTarget::Topic => {
                    doc.doc
                        .edit_sheet("Local Image", |sheet| {
                            images::set_image(sheet, owner.topic, Some((resource, size)))
                                .map_err(thinoda_core::edit::EditError::Invalid)
                        })
                        .map_err(|e| e.to_string())?;
                    self.zen = false;
                    self.settings.show_format_panel = true;
                    self.sidebar_family = crate::app::SidebarFamily::Format;
                    self.format_tab = crate::app::FormatTab::Data;
                }
                ImageTarget::Board { .. } => {
                    if !doc.view.zoom.is_finite() || doc.view.zoom <= 0.0 {
                        return Err("invalid canvas zoom".into());
                    }
                    let rect = doc.view.canvas_rect();
                    let center = Xf {
                        origin: rect.center() + doc.view.offset,
                        zoom: doc.view.zoom,
                    }
                    .sheet(rect.center());
                    // The filename distinguishes image cards in the canvas's
                    // accessible item list as well as on the board itself.
                    let title = format!("{}: {name}", t("Local Image"));
                    let id = doc
                        .doc
                        .edit_sheet("Local Image", |sheet| {
                            let id = sheet
                                .add_card(
                                    &title,
                                    center,
                                    Card::new(CardKind::Image, Vec2::new(240.0, 120.0)),
                                )
                                .map_err(thinoda_core::edit::EditError::Invalid)?;
                            let card_size = image_card_size(sheet, id, &title, size);
                            sheet
                                .set_topic_card(id, Some(Card::new(CardKind::Image, card_size)))
                                .map_err(thinoda_core::edit::EditError::Invalid)?;
                            images::set_image(sheet, id, Some((resource, size)))
                                .map_err(thinoda_core::edit::EditError::Invalid)?;
                            Ok(id)
                        })
                        .map_err(|e| e.to_string())?;
                    doc.view.select_only(id);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.toasts
                .error(format!("{}: {error}", t("Could not insert image")));
        }
    }
}

fn image_card_size(sheet: &Sheet, id: TopicId, title: &str, image: Vec2) -> Vec2 {
    let style = thinoda_core::theme::resolve_style(sheet, id, false);
    let stroke_padding = style.border_width.max(style.line_width) / 2.0 + 4.0;
    let px = stroke_padding.max(14.0);
    let py = stroke_padding.max(9.0);
    let width = (image.x + 2.0 * px).max(240.0);
    let wrap_width = (width - 2.0 * px).max(style.font_size);
    let text_height = ApproxMeasure
        .measure(title, style.font_size, style.bold, wrap_width)
        .y;
    // Leave room for font-metric differences from the approximate measurement.
    let height = (image.y + 6.0 + text_height * 1.5 + 2.0 * py + 16.0).max(120.0);
    Vec2::new(width, height)
}

fn pick_image(request: u64) -> Option<Vec<PathBuf>> {
    files::pick_files(
        files::Purpose::Image(request),
        Some(t("Local Image")),
        &[("PNG / JPEG", &["png", "jpg", "jpeg"])],
        None,
        false,
    )
}

pub(crate) fn read_local_resource(path: &Path) -> Result<Vec<u8>, String> {
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
        return Err("local resource exceeds 8 MiB".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_core::{Sheet, SheetRootMode, Workbook};
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

    fn large_jpeg() -> Vec<u8> {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(40, 20)
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        let encoded = encoded.into_inner();
        let target = 4_368_119;
        let mut remaining = target - encoded.len();
        let mut bytes = Vec::with_capacity(target);
        bytes.extend_from_slice(&encoded[..2]);
        // Legal JPEG APP15 segments keep the file decodable while exercising an
        // upload larger than common localStorage quotas.
        while remaining != 0 {
            assert!(remaining >= 4);
            let mut payload = (remaining - 4).min(65_533);
            let tail = remaining - payload - 4;
            if (1..4).contains(&tail) {
                payload -= 4 - tail;
            }
            bytes.extend_from_slice(&[0xff, 0xef]);
            bytes.extend_from_slice(&((payload + 2) as u16).to_be_bytes());
            bytes.resize(bytes.len() + payload, 0x5a);
            remaining -= payload + 4;
        }
        bytes.extend_from_slice(&encoded[2..]);
        assert_eq!(bytes.len(), target);
        bytes
    }

    fn owner(app: &mut ThinodaApp, request: u64) -> ImageRequest {
        let doc = app.doc_mut().unwrap();
        let topic = doc.doc.sheet().root;
        doc.view.select_only(topic);
        ImageRequest::capture(doc, request).unwrap()
    }

    fn rootless_app() -> ThinodaApp {
        let mut app = ThinodaApp::headless();
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        sheet.validate().unwrap();
        app.new_doc(Workbook::new(sheet));
        app
    }

    fn begin_async_board_pick(app: &mut ThinodaApp) -> u64 {
        let mut started = None;
        app.begin_board_image_with_picker(&egui::Context::default(), true, |request| {
            started = Some(request);
            None
        });
        started.expect("picker should be started")
    }

    #[test]
    fn browser_image_upload_inserts_large_jpeg_without_persistent_staging() {
        let bytes = large_jpeg();
        let mut app = rootless_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let request = begin_async_board_pick(&mut app);
        app.complete_image_upload(
            request,
            files::ImageUpload {
                name: "large.jpg".into(),
                bytes: bytes.clone(),
            },
        );
        assert!(app.image_request.is_none());
        let doc = app.doc().unwrap();
        let id = doc.view.primary().unwrap();
        let sheet = doc.doc.sheet();
        let topic = sheet.topic(id).unwrap();
        assert_eq!(topic.card.unwrap().kind, CardKind::Image);
        assert_eq!(sheet.floating, vec![id]);
        let resource = sheet
            .resources
            .get(topic.image.as_deref().unwrap())
            .unwrap();
        assert_eq!(resource.media_type, "image/jpeg");
        assert_eq!(resource.name, "large.jpg");
        assert_eq!(resource.bytes(), bytes);
        let inserted = doc.doc.workbook().clone();
        let doc = app.doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.workbook(), &inserted);
    }

    #[test]
    fn browser_image_upload_rejects_stale_invalid_and_oversized_results_atomically() {
        let mut app = rootless_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let state = app.doc().unwrap().doc.state_id();
        let older = begin_async_board_pick(&mut app);
        let newer = begin_async_board_pick(&mut app);
        app.complete_image_upload(
            older,
            files::ImageUpload {
                name: "late.jpg".into(),
                bytes: large_jpeg(),
            },
        );
        assert_eq!(app.image_request.as_ref().unwrap().request, newer);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        app.complete_image_upload(
            newer,
            files::ImageUpload {
                name: "invalid.jpg".into(),
                bytes: b"not an image".to_vec(),
            },
        );
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.doc().unwrap().doc.state_id(), state);

        let request = begin_async_board_pick(&mut app);
        app.complete_image_upload(
            request,
            files::ImageUpload {
                name: "too-large.jpg".into(),
                bytes: vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1],
            },
        );
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.doc().unwrap().doc.state_id(), state);
        assert!(!app.doc().unwrap().doc.can_undo());

        let request = begin_async_board_pick(&mut app);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Change", |sheet| {
                sheet.title = "Changed".into();
                Ok(())
            })
            .unwrap();
        let changed = app.doc().unwrap().doc.workbook().clone();
        app.complete_image_upload(
            request,
            files::ImageUpload {
                name: "stale.jpg".into(),
                bytes: large_jpeg(),
            },
        );
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &changed);
    }

    #[test]
    fn browser_image_upload_preserves_existing_topic_image_behavior() {
        let mut app = ThinodaApp::headless();
        let request = 41;
        app.image_request = Some(owner(&mut app, request));
        app.zen = true;
        app.settings.show_format_panel = false;
        app.complete_image_upload(
            request,
            files::ImageUpload {
                name: "topic.png".into(),
                bytes: encoded(42),
            },
        );
        assert!(app.image_request.is_none());
        assert!(!app.zen);
        assert!(app.settings.show_format_panel);
        let doc = app.doc().unwrap();
        let root = doc.doc.sheet().root;
        let topic = doc.doc.sheet().topic(root).unwrap();
        assert!(topic.image.is_some());
        assert!(doc.doc.sheet().floating.is_empty());
        assert_eq!(doc.view.selection, vec![root]);
        assert_eq!(doc.doc.undo_label(), Some("Local Image"));
    }

    #[test]
    fn browser_dropped_image_keeps_topic_ownership_and_does_not_replace_picker() {
        let mut app = ThinodaApp::headless();
        let ctx = egui::Context::default();
        let drop_owner = app.capture_drop_owner(&ctx).unwrap();
        let pending_picker = begin_async_board_pick(&mut app);
        app.finish_dropped_image_upload(
            Some(drop_owner),
            files::ImageUpload {
                name: "dropped.jpg".into(),
                bytes: large_jpeg(),
            },
        );
        assert_eq!(app.image_request.as_ref().unwrap().request, pending_picker);
        let doc = app.doc().unwrap();
        let root = doc.doc.sheet().root;
        assert!(doc.doc.sheet().topic(root).unwrap().image.is_some());
        assert!(doc.doc.sheet().floating.is_empty());
        let after_drop = doc.doc.workbook().clone();
        app.complete_image_upload(
            pending_picker,
            files::ImageUpload {
                name: "late.png".into(),
                bytes: encoded(90),
            },
        );
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &after_drop);

        let mut rootless = rootless_app();
        let before = rootless.doc().unwrap().doc.workbook().clone();
        rootless.finish_dropped_image_upload(
            None,
            files::ImageUpload {
                name: "unowned.png".into(),
                bytes: encoded(40),
            },
        );
        assert_eq!(rootless.doc().unwrap().doc.workbook(), &before);
        assert!(!rootless.doc().unwrap().doc.can_undo());
    }

    fn board_label_harness() -> (Harness<'static, ThinodaApp>, CanvasObjectId) {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut object = thinoda_core::CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(160.0, 120.0)),
        );
        if let thinoda_core::CanvasObjectKind::Shape { label, .. } = &mut object.kind {
            *label = "Original".into();
        }
        let id = sheet.add_canvas_object(object).unwrap();
        let mut app = ThinodaApp::headless();
        app.new_doc(Workbook::new(sheet));
        app.doc_mut().unwrap().view.select_board_object(id);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 800.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    let doc = app.doc_mut().unwrap();
                    let output = thinoda_canvas::MapCanvas::show(ui, &mut doc.doc, &mut doc.view);
                    assert!(output.error.is_none());
                },
                app,
            );
        harness.run_steps(3);
        (harness, id)
    }

    fn edit_board_label(harness: &mut Harness<'_, ThinodaApp>, id: CanvasObjectId, text: &str) {
        let doc = harness.state_mut().doc_mut().unwrap();
        assert!(doc.view.begin_board_label(&doc.doc, id));
        harness.run_steps(2);
        let editor_label = harness
            .state()
            .doc()
            .unwrap()
            .view
            .board_labels
            .label_editor
            .clone();
        harness.get_by_label(editor_label.as_str()).type_text(text);
        harness.run_steps(2);
        let doc = harness.state().doc().unwrap();
        assert!(doc.view.has_board_label_changes());
        assert_eq!(doc.view.board_label_draft().unwrap().2, text);
    }

    #[test]
    fn board_image_on_empty_rootless_sheet_is_centered_selected_and_one_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        let bytes = encoded(210);
        std::fs::write(&path, &bytes).unwrap();
        let mut app = rootless_app();
        let doc = app.doc_mut().unwrap();
        assert!(doc.view.primary().is_none());
        assert!(doc.doc.sheet().floating.is_empty());
        doc.view.offset = egui::vec2(80.0, -40.0);
        doc.view.zoom = 2.0;
        let before = doc.doc.workbook().clone();
        let was_zen = app.zen;
        let format_panel_was_visible = app.settings.show_format_panel;

        let request = begin_async_board_pick(&mut app);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        // A web picker can finish after the camera has moved. Place the card in
        // the viewport that is actually visible when the result is applied.
        let doc = app.doc_mut().unwrap();
        doc.view.offset = egui::vec2(150.0, -75.0);
        doc.view.zoom = 1.5;
        app.complete_image_pick(request, std::slice::from_ref(&path));

        let doc = app.doc().unwrap();
        let sheet = doc.doc.sheet();
        let id = doc.view.primary().unwrap();
        let topic = sheet.topic(id).unwrap();
        assert_eq!(sheet.root_mode, SheetRootMode::Hidden);
        assert_eq!(sheet.floating, vec![id]);
        assert_ne!(id, sheet.root);
        assert_eq!(topic.parent, None);
        assert_eq!(topic.position, Some(Vec2::new(-100.0, 50.0)));
        assert_eq!(topic.title, format!("{}: evidence.png", t("Local Image")));
        assert_eq!(topic.card.unwrap().kind, CardKind::Image);
        assert_eq!(topic.image_size, Some(Vec2::new(40.0, 20.0)));
        let reference = topic.image.as_deref().unwrap();
        let resource = sheet.resources.get(reference).unwrap();
        assert_eq!(resource.media_type, "image/png");
        assert_eq!(resource.bytes(), bytes);
        assert_eq!(doc.view.selection, vec![id]);
        assert!(doc.view.board_selection.is_empty());
        assert!(doc.view.selected_object.is_none());
        assert_eq!(doc.doc.undo_label(), Some("Local Image"));
        assert!(app.image_request.is_none());
        assert_eq!(app.zen, was_zen);
        assert_eq!(app.settings.show_format_panel, format_panel_was_visible);
        sheet.validate().unwrap();

        let layout = thinoda_core::layout_sheet(sheet, &thinoda_core::ApproxMeasure, false);
        let layout = &layout.topics[&id];
        let image = layout.image_rect.unwrap();
        assert!(layout.rect.contains(image.min));
        assert!(layout.rect.contains(image.max()));
        assert!(layout.rect.contains(layout.title_rect.min));
        assert!(layout.rect.contains(layout.title_rect.max()));

        let inserted = doc.doc.workbook().clone();
        let doc = app.doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
        assert!(!doc.doc.can_undo());
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.workbook(), &inserted);
    }

    #[test]
    fn board_image_is_visible_and_accessible_at_the_rendered_canvas_center() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(120)).unwrap();
        let app = rootless_app();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1000.0, 750.0))
            .build_ui_state(
                |ui, app: &mut ThinodaApp| {
                    let doc = app.doc_mut().unwrap();
                    let output = thinoda_canvas::MapCanvas::show(ui, &mut doc.doc, &mut doc.view);
                    assert!(output.error.is_none());
                },
                app,
            );
        h.run_steps(3);
        {
            let doc = h.state_mut().doc_mut().unwrap();
            doc.view.offset = egui::vec2(126.0, -84.0);
            doc.view.zoom = 1.4;
        }
        let request = begin_async_board_pick(h.state_mut());
        h.state_mut()
            .complete_image_pick(request, std::slice::from_ref(&path));
        h.run_steps(4);
        let label = format!(
            "{}: evidence.png, {}",
            t("Local Image"),
            h.state().doc().unwrap().view.board_labels.selected
        );
        h.get_by_label(label.as_str());
        let doc = h.state_mut().doc_mut().unwrap();
        let id = doc.view.primary().unwrap();
        let layout = doc.view.layout_for(&doc.doc);
        let rect = layout.rect(id).unwrap();
        let canvas = doc.view.canvas_rect();
        let xf = Xf {
            origin: canvas.center() + doc.view.offset,
            zoom: doc.view.zoom,
        };
        assert!((xf.pos(rect.center()) - canvas.center()).length() < 0.1);
        assert!(canvas.contains_rect(xf.rect(rect)));
    }

    #[test]
    fn board_picker_cancellation_failure_and_invalid_camera_leave_no_card() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        let mut app = rootless_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let state = app.doc().unwrap().doc.state_id();
        let ctx = egui::Context::default();

        app.begin_board_image_with_picker(&ctx, false, |_| None);
        assert!(app.image_request.is_none());
        app.begin_board_image_with_picker(&ctx, false, |_| Some(Vec::new()));
        assert!(app.image_request.is_none());
        let request = begin_async_board_pick(&mut app);
        app.complete_image_pick(request, &[]);
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.doc().unwrap().doc.state_id(), state);

        let mut too_wide = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4097, 1)
            .write_to(&mut too_wide, image::ImageFormat::Png)
            .unwrap();
        for bytes in [
            Vec::new(),
            b"not an image".to_vec(),
            vec![0; thinoda_core::resources::MAX_RESOURCE_BYTES + 1],
            too_wide.into_inner(),
        ] {
            std::fs::write(&path, &bytes).unwrap();
            app.begin_board_image_with_picker(&ctx, false, |_| Some(vec![path.clone()]));
            assert!(app.image_request.is_none());
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
            assert_eq!(app.doc().unwrap().doc.state_id(), state);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        std::fs::write(&path, encoded(100)).unwrap();
        for zoom in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            app.doc_mut().unwrap().view.zoom = zoom;
            app.begin_board_image_with_picker(&ctx, false, |_| Some(vec![path.clone()]));
            assert!(app.image_request.is_none());
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
            assert_eq!(app.doc().unwrap().doc.state_id(), state);
        }
        app.doc_mut().unwrap().view.zoom = 1.0;
        for offset in [
            egui::vec2(f32::NAN, 0.0),
            egui::vec2(f32::INFINITY, 0.0),
            egui::vec2(2_000_000.0, 0.0),
        ] {
            app.doc_mut().unwrap().view.offset = offset;
            app.begin_board_image_with_picker(&ctx, false, |_| Some(vec![path.clone()]));
            assert!(app.image_request.is_none());
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
            assert_eq!(app.doc().unwrap().doc.state_id(), state);
        }
        assert!(!app.doc().unwrap().doc.can_undo());

        app.mode = CenterMode::Outliner;
        let mut opened_in_outliner = false;
        app.begin_board_image_with_picker(&ctx, false, |_| {
            opened_in_outliner = true;
            None
        });
        assert!(!opened_in_outliner);
        assert!(app.image_request.is_none());
        app.mode = CenterMode::Map;
        app.docs.clear();
        let mut opened = false;
        app.begin_board_image_with_picker(&ctx, false, |_| {
            opened = true;
            None
        });
        assert!(!opened);
        assert!(app.image_request.is_none());
    }

    #[test]
    fn board_image_accepts_jpeg_and_preserves_its_original_bytes_and_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("portrait.JPG");
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(300, 600)
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let bytes = bytes.into_inner();
        std::fs::write(&path, &bytes).unwrap();
        let mut app = rootless_app();
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc().unwrap();
        let topic = doc.doc.sheet().topic(doc.view.primary().unwrap()).unwrap();
        assert_eq!(topic.image_size, Some(Vec2::new(50.0, 100.0)));
        let resource = doc
            .doc
            .sheet()
            .resources
            .get(topic.image.as_deref().unwrap())
            .unwrap();
        assert_eq!(resource.media_type, "image/jpeg");
        assert_eq!(resource.name, "portrait.JPG");
        assert_eq!(resource.bytes(), bytes);
    }

    #[test]
    fn board_image_card_fits_a_long_filename_and_large_custom_font() {
        let dir = tempfile::tempdir().unwrap();
        let filename = format!("{}.png", "W".repeat(96));
        let path = dir.path().join(&filename);
        std::fs::write(&path, encoded(120)).unwrap();
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut custom = thinoda_core::custom_theme::CustomTheme::from_settings(&sheet.theme);
        custom.roles.floating.font_size = Some(48.0);
        sheet.theme.custom_theme = Some(custom);
        let mut app = ThinodaApp::headless();
        app.new_doc(Workbook::new(sheet));
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc().unwrap();
        let id = doc.view.primary().unwrap();
        let sheet = doc.doc.sheet();
        assert_eq!(
            sheet.topic(id).unwrap().title,
            format!("{}: {filename}", t("Local Image"))
        );
        // Check real egui font metrics, not just the approximation used when
        // choosing the initial card size.
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let layout = thinoda_core::layout_sheet(
                sheet,
                &thinoda_canvas::EguiMeasure::new(ui.ctx()),
                false,
            );
            let topic = &layout.topics[&id];
            let image = topic.image_rect.unwrap();
            assert!(topic.rect.contains(image.min));
            assert!(topic.rect.contains(image.max()));
            assert!(topic.rect.contains(topic.title_rect.min));
            assert!(topic.rect.contains(topic.title_rect.max()));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn board_image_resource_limit_failure_rolls_back_the_new_card() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(120)).unwrap();
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        for index in 0..thinoda_core::resources::MAX_RESOURCES {
            sheet
                .resources
                .insert(
                    Resource::new("opaque.bin", "application/octet-stream", vec![index as u8])
                        .unwrap(),
                )
                .unwrap();
        }
        let mut app = ThinodaApp::headless();
        app.new_doc(Workbook::new(sheet));
        let before = app.doc().unwrap().doc.workbook().clone();
        let state = app.doc().unwrap().doc.state_id();
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), state);
        assert!(doc.doc.sheet().floating.is_empty());
        assert!(doc.view.selection.is_empty());
        assert!(!doc.doc.can_undo());
    }

    #[test]
    fn board_image_respects_sheet_and_workbook_byte_budgets_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(130)).unwrap();
        let mut full = Sheet::new("Full", "");
        full.root_mode = SheetRootMode::Hidden;
        for value in 0..4 {
            full.resources
                .insert(
                    Resource::new(
                        "large.bin",
                        "application/octet-stream",
                        vec![value; thinoda_core::resources::MAX_RESOURCE_BYTES],
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        assert_eq!(
            full.resources.byte_len(),
            thinoda_core::resources::MAX_SHEET_RESOURCE_BYTES
        );
        for workbook_full in [false, true] {
            let mut book = Workbook::new(full.clone());
            if workbook_full {
                book.sheets.push(full.clone());
                let mut empty = Sheet::new("Board", "");
                empty.root_mode = SheetRootMode::Hidden;
                book.sheets.push(empty);
            }
            let mut app = ThinodaApp::headless();
            app.new_doc(book);
            if workbook_full {
                app.doc_mut().unwrap().doc.set_active(2);
            }
            let before = app.doc().unwrap().doc.workbook().clone();
            let state = app.doc().unwrap().doc.state_id();
            app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
                Some(vec![path.clone()])
            });
            let doc = app.doc().unwrap();
            assert_eq!(doc.doc.workbook(), &before, "workbook_full={workbook_full}");
            assert_eq!(doc.doc.state_id(), state);
            assert!(doc.doc.sheet().floating.is_empty());
            assert!(doc.view.selection.is_empty());
            assert!(!doc.doc.can_undo());
        }
    }

    #[test]
    fn older_board_picker_completions_cannot_clear_or_apply_a_newer_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(180)).unwrap();
        let mut app = rootless_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let older = begin_async_board_pick(&mut app);
        let newer = begin_async_board_pick(&mut app);
        assert_ne!(older, newer);

        app.complete_image_pick(older, &[]);
        app.complete_image_pick(older, std::slice::from_ref(&path));
        assert_eq!(app.image_request.as_ref().unwrap().request, newer);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);

        app.complete_image_pick(newer, std::slice::from_ref(&path));
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.sheet().floating.len(), 1);
        let inserted = app.doc().unwrap().doc.workbook().clone();
        app.complete_image_pick(newer, std::slice::from_ref(&path));
        assert_eq!(app.doc().unwrap().doc.workbook(), &inserted);
    }

    #[test]
    fn board_picker_rejects_stale_documents_sheets_selections_and_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(170)).unwrap();
        for stale in 0..13 {
            let mut sheet = Sheet::new("Board", "");
            sheet.root_mode = SheetRootMode::Hidden;
            let topic = sheet.add_floating("Existing", Vec2::new(220.0, 40.0));
            let rect = thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(160.0, 140.0));
            let shape = sheet
                .add_canvas_object(thinoda_core::CanvasObject::shape(
                    thinoda_core::ShapeKind::Rectangle,
                    rect,
                ))
                .unwrap();
            let zone = sheet.create_board_frame("Frame", rect).unwrap();
            let mut book = Workbook::new(sheet);
            book.sheets.push(Sheet::new("Second", "Root"));
            let mut app = ThinodaApp::headless();
            app.new_doc(book);
            app.doc_mut().unwrap().view.selection.clear();
            if matches!(stale, 8 | 9) {
                app.doc_mut().unwrap().view.select_only(topic);
            } else if stale == 10 {
                app.doc_mut().unwrap().view.select_board_object(shape);
            }
            let request = begin_async_board_pick(&mut app);
            match stale {
                0 => app.doc_mut().unwrap().view.select_only(topic),
                1 => app.doc_mut().unwrap().view.select_board_object(shape),
                2 => app
                    .doc_mut()
                    .unwrap()
                    .view
                    .select_object(ObjectSelection::Zone(zone)),
                3 => app.doc_mut().unwrap().key.push_str("-other-document"),
                4 => app.doc_mut().unwrap().doc.set_active(1),
                5 => {
                    app.doc_mut()
                        .unwrap()
                        .doc
                        .edit_sheet("Concurrent edit", |sheet| sheet.set_title(topic, "Changed"))
                        .unwrap();
                }
                6 => app.new_doc(Workbook::new(Sheet::new("Other", "Root"))),
                7 => app.docs.clear(),
                8 => {
                    let doc = app.doc_mut().unwrap();
                    doc.view
                        .begin_edit_with(doc.doc.sheet(), topic, "Uncommitted title");
                    assert!(doc.view.has_title_changes());
                }
                9 => {
                    let doc = app.doc_mut().unwrap();
                    doc.inspector_edit = Some(crate::doc::InspectorEdit {
                        buffer_id: None,
                        sheet_index: doc.doc.active_index(),
                        sheet_root: doc.doc.sheet().root,
                        topic,
                        field: crate::doc::InspectorField::Notes,
                        original: None,
                        format_notes: false,
                        text: "Uncommitted note".into(),
                    });
                    assert!(doc.has_inspector_changes());
                }
                10 => {
                    let doc = app.doc_mut().unwrap();
                    assert!(doc.view.begin_board_label(&doc.doc, shape));
                }
                11 => app.image_request.as_mut().unwrap().root = TopicId(u64::MAX),
                _ => app.mode = CenterMode::Outliner,
            }
            let snapshot = |app: &ThinodaApp| {
                app.docs
                    .iter()
                    .map(|doc| {
                        (
                            doc.doc.workbook().clone(),
                            doc.doc.state_id(),
                            doc.view.selection.clone(),
                            doc.view.board_selection.clone(),
                            doc.view.selected_object,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let before = snapshot(&app);
            app.complete_image_pick(request, std::slice::from_ref(&path));
            assert_eq!(snapshot(&app), before, "stale case {stale}");
            assert!(app.image_request.is_none(), "stale case {stale}");
            if stale == 8 {
                assert!(app.doc().unwrap().view.has_title_changes());
            } else if stale == 9 {
                assert!(app.doc().unwrap().has_inspector_changes());
            } else if stale == 10 {
                assert_eq!(app.doc().unwrap().view.editing_board_object(), Some(shape));
            }
        }
    }

    #[test]
    fn board_image_accepts_an_object_selection_and_preserves_existing_roots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(100)).unwrap();
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let first = sheet.add_floating("First root", Vec2::new(-200.0, 0.0));
        let second = sheet.add_floating("Second root", Vec2::new(200.0, 0.0));
        let shape = sheet
            .add_canvas_object(thinoda_core::CanvasObject::shape(
                thinoda_core::ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(80.0, 60.0)),
            ))
            .unwrap();
        let first_before = sheet.topic(first).unwrap().clone();
        let second_before = sheet.topic(second).unwrap().clone();
        let shape_before = sheet.canvas.object(shape).unwrap().clone();
        let mut app = ThinodaApp::headless();
        app.new_doc(Workbook::new(sheet));
        app.doc_mut().unwrap().view.select_board_object(shape);
        assert!(app.doc().unwrap().view.primary().is_none());
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc().unwrap();
        let card = doc.view.primary().unwrap();
        assert_eq!(doc.doc.sheet().floating, vec![first, second, card]);
        assert_eq!(doc.doc.sheet().topic(first).unwrap(), &first_before);
        assert_eq!(doc.doc.sheet().topic(second).unwrap(), &second_before);
        assert_eq!(doc.doc.sheet().canvas.object(shape).unwrap(), &shape_before);
        assert!(doc.view.board_selection.is_empty());
        assert_eq!(
            doc.doc.sheet().topic(card).unwrap().card.unwrap().kind,
            CardKind::Image
        );
    }

    #[test]
    fn board_picker_commits_existing_draft_before_its_separate_undoable_insert() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(100)).unwrap();
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let before = doc.doc.workbook().clone();
        doc.view
            .begin_edit_with(doc.doc.sheet(), root, "A pending title");
        let request = begin_async_board_pick(&mut app);
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.sheet().root_topic().title, "A pending title");
        assert!(doc.doc.sheet().floating.is_empty());
        let before_card = doc.doc.workbook().clone();

        app.complete_image_pick(request, std::slice::from_ref(&path));
        let doc = app.doc_mut().unwrap();
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before_card);
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
    }

    #[test]
    fn board_picker_rejects_late_result_without_losing_a_new_board_label_draft() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(120)).unwrap();
        let (mut harness, id) = board_label_harness();
        let request = begin_async_board_pick(harness.state_mut());
        edit_board_label(&mut harness, id, "Pending board label");
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        let state = harness.state().doc().unwrap().doc.state_id();

        harness
            .state_mut()
            .complete_image_pick(request, std::slice::from_ref(&path));
        let doc = harness.state_mut().doc_mut().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.doc.state_id(), state);
        assert_eq!(
            doc.view.board_label_draft().unwrap().2,
            "Pending board label"
        );
        assert_eq!(doc.view.editing_board_object(), Some(id));
        assert_eq!(doc.view.board_selection, vec![id]);
        assert!(doc.view.try_commit_board_label(&mut doc.doc).unwrap());
        assert_eq!(
            doc.doc.sheet().canvas.object(id).unwrap().label(),
            "Pending board label"
        );
        assert!(harness.state().image_request.is_none());
    }

    #[test]
    fn browser_image_upload_preserves_a_dirty_board_label_draft() {
        let (mut harness, id) = board_label_harness();
        let request = begin_async_board_pick(harness.state_mut());
        edit_board_label(&mut harness, id, "Pending board label");
        let before = harness.state().doc().unwrap().doc.workbook().clone();
        harness.state_mut().complete_image_upload(
            request,
            files::ImageUpload {
                name: "late.png".into(),
                bytes: encoded(120),
            },
        );
        let app = harness.state();
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(
            doc.view.board_label_draft().unwrap().2,
            "Pending board label"
        );
        assert_eq!(doc.view.editing_board_object(), Some(id));
        assert!(app.image_request.is_none());
    }

    #[test]
    fn board_picker_commits_existing_board_label_before_the_image_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(150)).unwrap();
        let (mut harness, id) = board_label_harness();
        let before_label = harness.state().doc().unwrap().doc.workbook().clone();
        edit_board_label(&mut harness, id, "Saved board label");

        let cancelled = begin_async_board_pick(harness.state_mut());
        let doc = harness.state().doc().unwrap();
        assert_eq!(doc.view.editing_board_object(), None);
        assert_eq!(
            doc.doc.sheet().canvas.object(id).unwrap().label(),
            "Saved board label"
        );
        let after_label = doc.doc.workbook().clone();
        harness.state_mut().complete_image_pick(cancelled, &[]);
        assert_eq!(harness.state().doc().unwrap().doc.workbook(), &after_label);
        let request = begin_async_board_pick(harness.state_mut());
        harness
            .state_mut()
            .complete_image_pick(request, std::slice::from_ref(&path));
        let doc = harness.state_mut().doc_mut().unwrap();
        assert_eq!(doc.doc.sheet().floating.len(), 1);
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &after_label);
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before_label);
    }

    #[test]
    fn conflicting_board_label_draft_blocks_picker_and_is_retained() {
        let (mut harness, id) = board_label_harness();
        edit_board_label(&mut harness, id, "My pending board label");
        let doc = harness.state_mut().doc_mut().unwrap();
        doc.doc
            .update_canvas_object("Concurrent label", id, |object| {
                if let thinoda_core::CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                    *label = "Changed elsewhere".into();
                }
            })
            .unwrap();
        let before = doc.doc.workbook().clone();
        let mut opened = false;
        harness
            .state_mut()
            .begin_board_image_with_picker(&egui::Context::default(), false, |_| {
                opened = true;
                None
            });
        let app = harness.state();
        assert!(!opened);
        assert!(app.image_request.is_none());
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(
            doc.view.board_label_draft().unwrap().2,
            "My pending board label"
        );
        assert_eq!(doc.view.editing_board_object(), Some(id));
    }

    #[test]
    fn invalid_pending_title_blocks_board_picker_without_losing_the_draft() {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.view
            .begin_edit_with(doc.doc.sheet(), root, "My pending title");
        doc.doc
            .edit_sheet("Concurrent title", |sheet| {
                sheet.set_title(root, "A different title")
            })
            .unwrap();
        let before = doc.doc.workbook().clone();
        let mut opened = false;
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            opened = true;
            None
        });
        assert!(!opened);
        assert!(app.image_request.is_none());
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.view.editing_text(), Some("My pending title"));
    }

    #[test]
    fn board_image_deduplicates_an_attachment_and_undo_restores_opaque_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        let bytes = encoded(140);
        std::fs::write(&path, &bytes).unwrap();
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        let attachment = doc
            .doc
            .edit_sheet("Attachment", |sheet| {
                thinoda_core::attachments::insert(sheet, root, "evidence.bin", bytes.clone())
            })
            .unwrap();
        let reference = doc
            .doc
            .sheet()
            .topic(attachment)
            .unwrap()
            .attachment
            .as_ref()
            .unwrap()
            .resource
            .clone();
        assert_eq!(
            doc.doc
                .sheet()
                .resources
                .get(&reference)
                .unwrap()
                .media_type,
            "application/octet-stream"
        );
        let before = doc.doc.workbook().clone();

        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc_mut().unwrap();
        let card = doc.view.primary().unwrap();
        let sheet = doc.doc.sheet();
        assert_eq!(
            sheet.topic(card).unwrap().image.as_deref(),
            Some(reference.as_str())
        );
        assert_eq!(sheet.resources.entries.len(), 1);
        assert_eq!(
            sheet.resources.get(&reference).unwrap().media_type,
            "image/png"
        );
        assert_eq!(sheet.resources.get(&reference).unwrap().bytes(), bytes);
        assert_eq!(
            sheet
                .topic(attachment)
                .unwrap()
                .attachment
                .as_ref()
                .unwrap()
                .name,
            "evidence.bin"
        );
        let inserted = doc.doc.workbook().clone();
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.workbook(), &inserted);

        doc.doc
            .edit_sheet("Remove image", |sheet| {
                images::set_image(sheet, card, None).map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        let sheet = doc.doc.sheet();
        assert_eq!(sheet.resources.get(&reference).unwrap().bytes(), bytes);
        assert_eq!(
            sheet
                .topic(attachment)
                .unwrap()
                .attachment
                .as_ref()
                .unwrap()
                .resource,
            reference
        );
    }

    #[test]
    fn board_image_does_not_remove_a_shared_existing_topic_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(160)).unwrap();
        let mut app = ThinodaApp::headless();
        app.image_request = Some(owner(&mut app, 21));
        app.finish_image(21, &path);
        let doc = app.doc().unwrap();
        let root = doc.doc.sheet().root;
        let reference = doc.doc.sheet().root_topic().image.clone().unwrap();
        app.begin_board_image_with_picker(&egui::Context::default(), false, |_| {
            Some(vec![path.clone()])
        });
        let doc = app.doc_mut().unwrap();
        let card = doc.view.primary().unwrap();
        assert_eq!(
            doc.doc.sheet().topic(card).unwrap().image.as_ref(),
            Some(&reference)
        );
        assert_eq!(doc.doc.sheet().resources.entries.len(), 1);
        doc.doc
            .edit_sheet("Remove image", |sheet| {
                images::set_image(sheet, card, None).map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        assert_eq!(
            doc.doc.sheet().topic(root).unwrap().image.as_ref(),
            Some(&reference)
        );
        assert!(doc.doc.sheet().resources.get(&reference).is_ok());
    }

    #[test]
    fn failed_delayed_drop_does_not_replace_a_pending_board_picker() {
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("invalid.png");
        let valid = dir.path().join("valid.png");
        std::fs::write(&invalid, b"not an image").unwrap();
        std::fs::write(&valid, encoded(180)).unwrap();
        let mut app = ThinodaApp::headless();
        let drop_owner = app.capture_drop_owner(&egui::Context::default());
        let request = begin_async_board_pick(&mut app);
        let before = app.doc().unwrap().doc.workbook().clone();
        app.finish_dropped_image(drop_owner, &invalid);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.image_request.as_ref().unwrap().request, request);
        app.complete_image_pick(request, std::slice::from_ref(&valid));
        assert!(app.image_request.is_none());
        assert_eq!(app.doc().unwrap().doc.sheet().floating.len(), 1);
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
    fn dropped_images_insert_on_the_selected_topic_and_remain_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.PNG");
        let bytes = encoded(180);
        std::fs::write(&path, &bytes).unwrap();
        let mut app = ThinodaApp::headless();
        let topic = app.doc().unwrap().doc.sheet().root;

        assert!(is_supported_image_path(&path));
        let owner = app.capture_drop_owner(&egui::Context::default());
        app.finish_dropped_image(owner, &path);

        let doc = app.doc().unwrap();
        let image = doc.doc.sheet().topics[&topic].image.as_ref().unwrap();
        let id = image
            .strip_prefix(thinoda_core::resources::RESOURCE_PREFIX)
            .unwrap();
        assert_eq!(
            doc.doc.sheet().resources.entries[id].bytes(),
            bytes.as_slice()
        );
        assert!(app.doc_mut().unwrap().doc.undo());
        assert!(
            app.doc().unwrap().doc.sheet().topics[&topic]
                .image
                .is_none()
        );
    }

    #[test]
    fn dropped_image_without_single_topic_selection_is_not_inserted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.jpg");
        std::fs::write(&path, encoded(180)).unwrap();
        let mut app = ThinodaApp::headless();
        app.doc_mut().unwrap().view.selection.clear();
        let before = app.doc().unwrap().doc.workbook().clone();

        let owner = app.capture_drop_owner(&egui::Context::default());
        app.finish_dropped_image(owner, &path);

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
                "Resize image",
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

    #[test]
    fn delayed_drop_cannot_mutate_a_changed_selection_or_document_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evidence.png");
        std::fs::write(&path, encoded(180)).unwrap();
        let mut app = ThinodaApp::headless();
        let owner = app.capture_drop_owner(&egui::Context::default());
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Concurrent edit", |sheet| {
                sheet.set_title(sheet.root, "Changed")
            })
            .unwrap();
        let before = app.doc().unwrap().doc.workbook().clone();

        app.finish_dropped_image(owner, &path);

        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }
}

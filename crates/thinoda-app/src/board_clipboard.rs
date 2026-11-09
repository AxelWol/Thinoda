//! Portable clipboard markup shared by the desktop and browser transports.
//!
//! The readable text travels in the clipboard's `text/plain` representation.
//! A separate `text/html` representation carries this app's opaque board or
//! topic data. Older session-only topic tokens are recognized for a warned
//! plain-text fallback. The marker is never inferred from plain text.

use std::collections::HashSet;
use thinoda_canvas::{Action, ObjectSelection, apply_action};
#[cfg(test)]
use thinoda_core::clipboard::MAX_TOPIC_CLIPBOARD_DEPTH;
use thinoda_core::{
    CanvasObjectId, CanvasObjectKind, Endpoint, TopicId,
    clipboard::{self, MAX_BOARD_CLIPBOARD_BYTES},
};
use web_time::Instant;

use crate::{
    doc::OpenDoc,
    sheet_clipboard::{PreparedSheet, SheetWrite},
};

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
pub(crate) fn install_web_event_capture() {
    if let Err(error) = web::install_event_capture() {
        log::warn!("Could not install browser clipboard event capture: {error}");
    }
}

const BOARD_ATTRIBUTE: &str = "data-thinoda-board-v1";
const TOPIC_ATTRIBUTE: &str = "data-thinoda-topic-v1";
const TOPIC_PAYLOAD_ATTRIBUTE: &str = "data-thinoda-topic-v2";
// Hex encoding takes two bytes per board byte; HTML escaping of visible text
// takes at most five bytes per text byte. Leave room for the wrapper.
const MAX_HTML_BYTES: usize = 7 * MAX_BOARD_CLIPBOARD_BYTES + 1024;
const MAX_PENDING_CUT: std::time::Duration = std::time::Duration::from_secs(30);
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RichClipboard {
    /// Validated again by the core board clipboard decoder when pasted.
    Board(String),
    /// Validated again by the core topic clipboard decoder when pasted.
    Topic(String),
    /// An older session-only token whose rich data is no longer available.
    TopicToken(String),
}

/// Identity and selection at the time an asynchronous clipboard operation began.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentGuard {
    key: String,
    sheet: usize,
    state: u64,
    topics: Vec<TopicId>,
    objects: Vec<CanvasObjectId>,
    selected_object: Option<ObjectSelection>,
}

impl DocumentGuard {
    pub(crate) fn capture(doc: &OpenDoc) -> Self {
        Self {
            key: doc.key.clone(),
            sheet: doc.doc.active_index(),
            state: doc.doc.state_id(),
            topics: doc.view.selection.clone(),
            objects: doc.view.board_selection.clone(),
            selected_object: doc.view.selected_object,
        }
    }

    pub(crate) fn matches(&self, doc: &OpenDoc) -> bool {
        *self == Self::capture(doc)
            && doc.view.editing_topic().is_none()
            && doc.view.editing_board_object().is_none()
    }
}

pub(crate) struct PreparedCopy {
    pub(crate) text: String,
    pub(crate) html: String,
    pub(crate) rich: RichClipboard,
    pub(crate) cut: Option<DocumentGuard>,
    started: Instant,
}

pub(crate) struct Snapshot {
    text: String,
    html: Option<String>,
}

fn verify_cut_snapshot(prepared: &PreparedCopy, snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.text.len() > MAX_BOARD_CLIPBOARD_BYTES {
        return Err("clipboard text exceeds the size limit".into());
    }
    if !same_clipboard_text(&snapshot.text, &prepared.text) {
        return Err("clipboard text changed".into());
    }
    let html = snapshot
        .html
        .as_deref()
        .ok_or("clipboard HTML is missing")?;
    match read_html(html)? {
        Some(rich) if rich == prepared.rich => Ok(()),
        Some(_) => Err("clipboard rich content changed".into()),
        None => Err("clipboard HTML no longer contains Thinoda data".into()),
    }
}

fn same_clipboard_text(left: &str, right: &str) -> bool {
    left == right || left.replace("\r\n", "\n") == right.replace("\r\n", "\n")
}

pub(crate) enum Completion {
    Write {
        generation: u64,
        prepared: PreparedCopy,
        result: Result<(), String>,
    },
    Read {
        guard: DocumentGuard,
        fallback: Option<String>,
        result: Result<Snapshot, String>,
    },
    SheetWrite {
        generation: u64,
        result: Result<SheetWrite, String>,
    },
}

fn localize_error(error: thinoda_core::EditError) -> String {
    let message = error.to_string();
    crate::i18n::localize_model_error(&message).to_owned()
}

#[derive(Default)]
pub(crate) struct ClipboardBridge {
    generation: u64,
    #[cfg(target_os = "windows")]
    pending_sheet: Option<(u64, PreparedSheet)>,
    #[cfg(not(target_arch = "wasm32"))]
    transport: native::Transport,
    #[cfg(target_arch = "wasm32")]
    transport: web::Transport,
}

impl ClipboardBridge {
    fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        #[cfg(target_arch = "wasm32")]
        self.transport.set_generation(self.generation);
        #[cfg(target_os = "windows")]
        {
            self.pending_sheet = None;
        }
        self.generation
    }

    fn write(&mut self, ctx: &egui::Context, prepared: PreparedCopy) -> Option<Completion> {
        let generation = self.next_generation();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx;
            let result = self.transport.write(&prepared);
            Some(Completion::Write {
                generation,
                prepared,
                result,
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.transport.write(ctx, generation, prepared)
        }
    }

    /// Writes plain text, cancelling any pending cut from an earlier write.
    fn write_text(&mut self, ctx: &egui::Context, text: String) -> Result<(), String> {
        self.next_generation();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx;
            self.transport.write_text(&text)
        }
        #[cfg(target_arch = "wasm32")]
        {
            ctx.copy_text(text);
            Ok(())
        }
    }

    fn write_sheet(&mut self, ctx: &egui::Context, prepared: PreparedSheet) -> Option<Completion> {
        let generation = self.next_generation();
        #[cfg(target_os = "windows")]
        if !self.transport.uses_fake() {
            self.pending_sheet = Some((generation, prepared));
            let _ = ctx;
            return None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx;
            let result = self.transport.write_sheet(&prepared, None);
            Some(Completion::SheetWrite { generation, result })
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.transport.write_sheet(ctx, generation, prepared)
        }
    }

    fn read(
        &mut self,
        ctx: &egui::Context,
        guard: DocumentGuard,
        fallback: Option<String>,
    ) -> Option<Completion> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = ctx;
            let result = self.transport.read(fallback.as_deref());
            Some(Completion::Read {
                guard,
                fallback,
                result,
            })
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.transport.read(ctx, guard, fallback)
        }
    }

    fn poll(&mut self) -> Vec<Completion> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Vec::new()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.transport.poll()
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn headless() -> Self {
        Self {
            transport: native::Transport::headless(),
            ..Self::default()
        }
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    fn with_fake(fake: std::rc::Rc<std::cell::RefCell<native::FakeClipboard>>) -> Self {
        Self {
            transport: native::Transport::with_fake(fake),
            ..Self::default()
        }
    }
}

/// Prepare all representations before a cut can mutate the document.
pub(crate) fn prepare_copy(doc: &mut OpenDoc, cut: bool) -> Result<Option<PreparedCopy>, String> {
    doc.view
        .try_commit_edit(&mut doc.doc)
        .map_err(localize_error)?;
    doc.view
        .try_commit_board_label(&mut doc.doc)
        .map_err(localize_error)?;
    let requested = if cut { Action::Cut } else { Action::Copy };
    if !requested.available(&doc.doc, &doc.view) {
        return Ok(None);
    }
    doc.view.clipboard_out = None;
    if !apply_action(&mut doc.doc, &mut doc.view, Action::Copy).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let text = doc
        .view
        .clipboard_out
        .take()
        .ok_or("copy did not produce clipboard text")?;
    let rich = if let Some(board) = doc.view.board_clipboard() {
        RichClipboard::Board(clipboard::encode_board_payload(board)?)
    } else {
        let payload = doc
            .view
            .clipboard
            .as_ref()
            .ok_or("copy did not produce clipboard content")?;
        RichClipboard::Topic(clipboard::encode_topic_payload(payload)?)
    };
    let html = make_html(&text, &rich)?;
    Ok(Some(PreparedCopy {
        text,
        html,
        rich,
        cut: cut.then(|| DocumentGuard::capture(doc)),
        started: Instant::now(),
    }))
}

/// Commit a previously prepared cut only while its exact source is still current.
pub(crate) fn complete_cut(doc: &mut OpenDoc, guard: &DocumentGuard) -> Result<(), String> {
    doc.ensure_no_pending_ink()?;
    if !guard.matches(doc) {
        return Err("cut cancelled because the document or selection changed".into());
    }
    let result =
        apply_action(&mut doc.doc, &mut doc.view, Action::Cut).map_err(|error| error.to_string());
    // The rich clipboard was already written; do not emit a second plain write.
    doc.view.clipboard_out = None;
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err("cut is no longer available".into()),
        Err(error) => Err(error),
    }
}

/// Apply a clipboard snapshot, never reusing an internal payload for plain text.
pub(crate) fn paste(
    doc: &mut OpenDoc,
    text: String,
    rich: Option<RichClipboard>,
) -> Result<bool, String> {
    match rich {
        Some(RichClipboard::Board(encoded)) => {
            doc.view.invalidate_clipboard();
            let payload = clipboard::decode_board_payload(&encoded)?;
            if text.len() > MAX_BOARD_CLIPBOARD_BYTES || !same_clipboard_text(&text, &payload.text)
            {
                return Err("clipboard text does not match rich board data".into());
            }
            doc.view
                .paste_board_payload(&mut doc.doc, payload)
                .map_err(localize_error)
        }
        Some(RichClipboard::Topic(encoded)) => {
            doc.view.invalidate_clipboard();
            let payload = clipboard::decode_topic_payload(&encoded)?;
            if text.len() > MAX_BOARD_CLIPBOARD_BYTES || !same_clipboard_text(&text, &payload.text)
            {
                return Err("clipboard text does not match rich topic data".into());
            }
            doc.view.clipboard = Some(payload);
            let result = apply_action(&mut doc.doc, &mut doc.view, Action::Paste(None));
            if !matches!(&result, Ok(true)) {
                doc.view.invalidate_clipboard();
            }
            result.map_err(localize_error)
        }
        Some(RichClipboard::TopicToken(_)) | None => {
            apply_action(&mut doc.doc, &mut doc.view, Action::Paste(Some(text)))
                .map_err(localize_error)
        }
    }
}

/// Commits pending edits and encodes the selection as draw.io clipboard text.
pub(crate) fn prepare_drawio_copy(
    doc: &mut OpenDoc,
) -> Result<Option<thinoda_io::drawio_clipboard::DrawioCopy>, String> {
    doc.view
        .try_commit_edit(&mut doc.doc)
        .map_err(localize_error)?;
    doc.view
        .try_commit_board_label(&mut doc.doc)
        .map_err(localize_error)?;
    if doc.doc.sheet().fim.is_some() {
        return Ok(None);
    }
    let Some(payload) = doc
        .view
        .selection_board_payload(&doc.doc)
        .map_err(localize_error)?
    else {
        return Ok(None);
    };
    let selected: HashSet<_> = payload.source_object_ids().collect();
    let sheet = doc.doc.sheet();
    let connector_ids: HashSet<_> = sheet
        .canvas
        .objects
        .iter()
        .filter(|object| object.is_connector())
        .map(|object| object.id)
        .collect();
    let detached_external = sheet
        .canvas
        .objects
        .iter()
        .filter(|object| selected.contains(&object.id))
        .filter_map(|object| match &object.kind {
            CanvasObjectKind::Connector(connector) => Some([connector.from, connector.to]),
            _ => None,
        })
        .flatten()
        .filter(|end| {
            matches!(end, Endpoint::Object(id) if !selected.contains(id) && connector_ids.contains(id))
        })
        .count();
    let mut copy = thinoda_io::drawio_clipboard::encode_with_report(&payload)
        .map_err(|error| error.to_string())?;
    if detached_external > 0 {
        if let Some(thinoda_io::ExportLoss::ConnectorAttachmentsDetached(count)) =
            copy.losses.iter_mut().find(|loss| {
                matches!(
                    loss,
                    thinoda_io::ExportLoss::ConnectorAttachmentsDetached(_)
                )
            })
        {
            *count += detached_external;
        } else {
            copy.losses
                .push(thinoda_io::ExportLoss::ConnectorAttachmentsDetached(
                    detached_external,
                ));
        }
    }
    Ok(Some(copy))
}

pub(crate) struct DrawioPasteOutcome {
    simplified: bool,
    possible_omissions: bool,
    losses: Vec<thinoda_io::drawio_clipboard::DrawioPasteLoss>,
    detached_connectors: bool,
    unsupported_shapes: bool,
    folded_notes: bool,
}

/// Pastes draw.io clipboard text as one undoable edit centred in the view.
pub(crate) fn paste_drawio(doc: &mut OpenDoc, text: &str) -> Result<DrawioPasteOutcome, String> {
    if doc.doc.sheet().fim.is_some() {
        return Err("draw.io content cannot be pasted into a FIM sheet".into());
    }
    let pasted = thinoda_io::drawio_clipboard::decode(text).map_err(|error| match error {
        thinoda_io::IoError::DrawioClipboardEmptyFirstPage => crate::i18n::t(
            "The first draw.io page has no pasteable items; additional pages were not pasted.",
        )
        .to_owned(),
        thinoda_io::IoError::DrawioClipboardEmptyPage => {
            crate::i18n::t("The draw.io page has no pasteable items.").to_owned()
        }
        error => error.to_string(),
    })?;
    doc.view
        .paste_foreign_board(
            &mut doc.doc,
            &pasted.payload,
            pasted.bounds,
            "Paste from draw.io",
        )
        .map_err(localize_error)?;
    Ok(DrawioPasteOutcome {
        simplified: !pasted.warnings.is_empty(),
        possible_omissions: pasted.warnings.contains(&thinoda_io::ImportWarning::Other),
        losses: pasted.losses,
        detached_connectors: pasted
            .warnings
            .contains(&thinoda_io::ImportWarning::DetachedConnectors),
        unsupported_shapes: pasted
            .warnings
            .contains(&thinoda_io::ImportWarning::UnsupportedDrawioShapes),
        folded_notes: pasted
            .warnings
            .contains(&thinoda_io::ImportWarning::DrawioFoldedNotesApproximated),
    })
}

fn drawio_paste_loss_lines(
    losses: &[thinoda_io::drawio_clipboard::DrawioPasteLoss],
) -> Vec<String> {
    use thinoda_io::drawio_clipboard::DrawioPasteLoss;

    losses
        .iter()
        .map(|loss| {
            let (count, label) = match loss {
                DrawioPasteLoss::ContainerFrames(count) => (count, "Container frames omitted"),
                DrawioPasteLoss::AdditionalPages(count) => {
                    (count, "Additional draw.io pages omitted")
                }
            };
            format!("{count} — {}", crate::i18n::t(label))
        })
        .collect()
}

impl crate::app::ThinodaApp {
    pub(crate) fn copy_sheet(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = doc.view.try_commit_edit(&mut doc.doc) {
            self.toasts.error(format!(
                "{}: {}",
                crate::i18n::t("Could not copy sheet"),
                localize_error(error)
            ));
            return;
        }
        let prepared = match crate::sheet_clipboard::prepare(doc) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    crate::i18n::t("Could not copy sheet"),
                    crate::i18n::localize_model_error(&error)
                ));
                return;
            }
        };
        for doc in &mut self.docs {
            doc.view.invalidate_clipboard();
        }
        if let Some(completion) = self.clipboard_bridge.write_sheet(ctx, prepared) {
            self.finish_clipboard(ctx, completion, false);
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn run_native_sheet_copy(&mut self, frame: &eframe::Frame, ctx: &egui::Context) {
        use raw_window_handle::HasWindowHandle;

        let Some((generation, prepared)) = self.clipboard_bridge.pending_sheet.take() else {
            return;
        };
        let result = match frame.window_handle() {
            Ok(owner) => self
                .clipboard_bridge
                .transport
                .write_sheet(&prepared, Some(owner)),
            Err(error) => self
                .clipboard_bridge
                .transport
                .write_text(&prepared.text)
                .map(|()| SheetWrite::TextOnly(format!("application window unavailable: {error}"))),
        };
        self.finish_clipboard(ctx, Completion::SheetWrite { generation, result }, false);
    }

    /// Copies the selection so it can be pasted into draw.io (diagrams.net).
    pub(crate) fn copy_as_drawio(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let copy = match prepare_drawio_copy(doc) {
            Ok(Some(copy)) => copy,
            Ok(None) => {
                self.toasts.warning(crate::i18n::t("Not possible here"));
                return;
            }
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    crate::i18n::t("Could not copy"),
                    crate::i18n::localize_model_error(&error)
                ));
                return;
            }
        };
        // The OS clipboard no longer holds Thinoda data.
        for doc in &mut self.docs {
            doc.view.invalidate_clipboard();
        }
        match self.clipboard_bridge.write_text(ctx, copy.text) {
            Ok(()) => {
                let message = crate::i18n::t("Copied for draw.io; paste it into a diagram");
                if copy.losses.is_empty() {
                    self.toasts.info(message);
                } else {
                    let report = crate::export_preflight::LossReport {
                        losses: copy.losses,
                        ..Default::default()
                    };
                    let lines =
                        crate::export_preflight::loss_lines(&report, thinoda_io::Format::DrawIo);
                    self.toasts.sticky(
                        thinoda_ui::toast::ToastKind::Warning,
                        format!("{message}\n{}", lines.join("\n")),
                    );
                }
            }
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    crate::i18n::t("Could not copy"),
                    crate::i18n::localize_model_error(&error)
                ));
            }
        }
    }

    /// Unified entry point for menu, configured shortcut and canvas events.
    pub(crate) fn clipboard_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Copy | Action::Cut => {
                let cut = action == Action::Cut;
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                let prepared = match prepare_copy(doc, cut) {
                    Ok(Some(prepared)) => prepared,
                    Ok(None) => {
                        self.toasts.warning(crate::i18n::t("Not possible here"));
                        return;
                    }
                    Err(error) => {
                        let message = if cut {
                            crate::i18n::t("Cut cancelled; original content was kept")
                        } else {
                            crate::i18n::t("Could not copy")
                        };
                        self.toasts.error(format!(
                            "{message}: {}",
                            crate::i18n::localize_model_error(&error)
                        ));
                        return;
                    }
                };
                if let Some(completion) = self.clipboard_bridge.write(ctx, prepared) {
                    self.finish_clipboard(ctx, completion, false);
                }
            }
            Action::Paste(fallback) => {
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                if let Err(error) = doc.view.try_commit_edit(&mut doc.doc) {
                    self.toasts.error(localize_error(error));
                    return;
                }
                if let Err(error) = doc.view.try_commit_board_label(&mut doc.doc) {
                    self.toasts.error(localize_error(error));
                    return;
                }
                let guard = DocumentGuard::capture(doc);
                if let Some(completion) = self.clipboard_bridge.read(ctx, guard, fallback) {
                    self.finish_clipboard(ctx, completion, false);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn poll_clipboard(&mut self, ctx: &egui::Context) {
        for completion in self.clipboard_bridge.poll() {
            self.finish_clipboard(ctx, completion, true);
        }
    }

    fn finish_clipboard(&mut self, ctx: &egui::Context, completion: Completion, delayed: bool) {
        match completion {
            Completion::SheetWrite { generation, result } => {
                if generation != self.clipboard_bridge.generation {
                    return;
                }
                match result {
                    Ok(SheetWrite::TextAndSvg) => {
                        self.toasts
                            .info(crate::i18n::t("Copied sheet and SVG to clipboard"));
                    }
                    Ok(SheetWrite::TextOnly(reason)) => {
                        self.toasts.warning(format!(
                            "{}: {reason}",
                            crate::i18n::t("Copied sheet as text only; SVG was not copied")
                        ));
                    }
                    Err(error) => {
                        self.toasts.error(format!(
                            "{}: {}",
                            crate::i18n::t("Could not copy sheet"),
                            crate::i18n::localize_model_error(&error)
                        ));
                    }
                }
            }
            Completion::Write {
                generation,
                prepared,
                result,
            } => {
                if let Err(error) = result {
                    let message = if prepared.cut.is_some() {
                        crate::i18n::t("Cut cancelled; original content was kept")
                    } else {
                        crate::i18n::t("Could not copy")
                    };
                    self.toasts.error(format!(
                        "{message}: {}",
                        crate::i18n::localize_model_error(&error)
                    ));
                    return;
                }
                if generation != self.clipboard_bridge.generation {
                    if prepared.cut.is_some() {
                        self.toasts
                            .warning("Cut cancelled because a newer copy was requested");
                    }
                    return;
                }
                if let Some(guard) = prepared.cut {
                    if prepared.started.elapsed() > MAX_PENDING_CUT {
                        self.toasts.warning(format!(
                            "{}: clipboard verification took too long",
                            crate::i18n::t("Cut cancelled; original content was kept")
                        ));
                        return;
                    }
                    if delayed
                        && (self.any_modal_open()
                            || ctx.text_edit_focused()
                            || ctx.input(|input| input.pointer.any_down()))
                    {
                        self.toasts.warning("Cut cancelled because focus changed");
                        return;
                    }
                    let Some(doc) = self.docs.get_mut(self.active) else {
                        self.toasts
                            .warning("Cut cancelled because the document was closed");
                        return;
                    };
                    if let Err(error) = complete_cut(doc, &guard) {
                        self.toasts.error(format!("Cut not applied: {error}"));
                    }
                }
            }
            Completion::Read {
                guard,
                fallback,
                result,
            } => {
                if delayed
                    && (self.any_modal_open()
                        || ctx.text_edit_focused()
                        || ctx.input(|input| input.pointer.any_down()))
                {
                    self.toasts.warning("Paste cancelled because focus changed");
                    return;
                }
                let Some(doc) = self.docs.get(self.active) else {
                    self.toasts
                        .warning("Paste cancelled because the document or selection changed");
                    return;
                };
                if let Err(error) = doc.ensure_no_pending_ink() {
                    self.toasts.warning(format!(
                        "{error} Clipboard data remains available; paste again after resolving it."
                    ));
                    return;
                }
                if !guard.matches(doc) {
                    self.toasts
                        .warning("Paste cancelled because the document or selection changed");
                    return;
                }
                let snapshot = match result {
                    Ok(snapshot) => snapshot,
                    Err(error) => match fallback {
                        Some(text) => {
                            self.toasts.warning(format!(
                                "Rich clipboard unavailable; pasting text only: {error}"
                            ));
                            Snapshot { text, html: None }
                        }
                        None => {
                            self.toasts
                                .error(format!("Could not read clipboard: {error}"));
                            return;
                        }
                    },
                };
                let rich = match snapshot.html.as_deref().map(read_html).transpose() {
                    Ok(rich) => rich.flatten(),
                    Err(error) => {
                        for doc in &mut self.docs {
                            doc.view.invalidate_clipboard();
                        }
                        self.toasts.error(format!(
                            "{}: {}",
                            crate::i18n::t("Could not paste clipboard data"),
                            crate::i18n::localize_model_error(&error)
                        ));
                        return;
                    }
                };
                // Cached view payloads never prove OS ownership, including in
                // other tabs. Rich content must come from actual HTML.
                for doc in &mut self.docs {
                    doc.view.invalidate_clipboard();
                }
                let legacy_topic = matches!(&rich, Some(RichClipboard::TopicToken(_)));
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                if rich.is_none() && thinoda_io::drawio_clipboard::looks_like_drawio(&snapshot.text)
                {
                    match paste_drawio(doc, &snapshot.text) {
                        Ok(outcome)
                            if !outcome.losses.is_empty()
                                || outcome.detached_connectors
                                || outcome.unsupported_shapes
                                || outcome.folded_notes
                                || outcome.possible_omissions =>
                        {
                            let message = crate::i18n::t(
                                "Pasted from draw.io; some content may have been omitted or simplified",
                            );
                            let mut lines = drawio_paste_loss_lines(&outcome.losses);
                            if outcome.detached_connectors {
                                lines.push(
                                    crate::i18n::t("Connector attachments may have been detached")
                                        .to_owned(),
                                );
                            }
                            if outcome.unsupported_shapes {
                                lines.push(
                                    crate::i18n::t(
                                        "Unsupported shapes or styles in the draw.io source may appear as rectangles",
                                    )
                                    .to_owned(),
                                );
                            }
                            if outcome.folded_notes {
                                lines.push(
                                    crate::i18n::t(
                                        "draw.io folded notes may be flattened or simplified",
                                    )
                                    .to_owned(),
                                );
                            }
                            let message = if lines.is_empty() {
                                message.to_owned()
                            } else {
                                format!("{message}\n{}", lines.join("\n"))
                            };
                            self.toasts
                                .sticky(thinoda_ui::toast::ToastKind::Warning, message);
                        }
                        Ok(outcome) if outcome.simplified => {
                            self.toasts.warning(crate::i18n::t(
                                "Pasted from draw.io; some content may have been omitted or simplified",
                            ));
                        }
                        Ok(_) => {}
                        Err(error) => {
                            self.toasts.error(format!(
                                "{}: {}",
                                crate::i18n::t("Could not paste draw.io content"),
                                crate::i18n::localize_model_error(&error)
                            ));
                        }
                    }
                    return;
                }
                match paste(doc, snapshot.text, rich) {
                    Ok(true) if legacy_topic => {
                        self.toasts.warning(crate::i18n::t(
                            "Older Thinoda clipboard data is unavailable; pasted text only",
                        ));
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.toasts.error(format!(
                            "{}: {}",
                            crate::i18n::t("Could not paste clipboard data"),
                            crate::i18n::localize_model_error(&error)
                        ));
                    }
                }
            }
        }
    }
}

/// Creates safe HTML markup for a clipboard write after bounding both parts.
pub(crate) fn make_html(text: &str, rich: &RichClipboard) -> Result<String, String> {
    let (attribute, payload) = match rich {
        RichClipboard::Board(value) => (BOARD_ATTRIBUTE, value),
        RichClipboard::Topic(value) => (TOPIC_PAYLOAD_ATTRIBUTE, value),
        RichClipboard::TopicToken(value) => (TOPIC_ATTRIBUTE, value),
    };
    if text.len() > MAX_BOARD_CLIPBOARD_BYTES || payload.len() > MAX_BOARD_CLIPBOARD_BYTES {
        return Err("clipboard content exceeds the size limit".into());
    }

    let mut html = String::new();
    html.push_str("<pre ");
    html.push_str(attribute);
    html.push_str("=\"");
    for byte in payload.bytes() {
        html.push(char::from(HEX[usize::from(byte >> 4)]));
        html.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    html.push_str("\">");
    for ch in text.chars() {
        match ch {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            _ => html.push(ch),
        }
    }
    html.push_str("</pre>");
    debug_assert!(html.len() <= MAX_HTML_BYTES);
    Ok(html)
}

/// Reads only a real HTML clipboard representation. An unrelated HTML fragment
/// has no rich data; malformed Thinoda markers are errors, never cache hits.
pub(crate) fn read_html(html: &str) -> Result<Option<RichClipboard>, String> {
    if html.len() > MAX_HTML_BYTES {
        return Err("clipboard HTML exceeds the size limit".into());
    }
    let bytes = html.as_bytes();
    let mut cursor = 0;
    let mut found = None;
    while cursor + 4 <= bytes.len() {
        let Some(relative) = bytes[cursor..]
            .windows(4)
            .position(|window| window.eq_ignore_ascii_case(b"<pre"))
        else {
            break;
        };
        let start = cursor + relative + 4;
        if bytes
            .get(start)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'>')
        {
            cursor = start;
            continue;
        }
        let Some(end) = tag_end(&bytes[start..]) else {
            if marker_in_tag(&html[start..])?.is_some() {
                return Err("invalid Thinoda clipboard markup".into());
            }
            break;
        };
        let end = start + end;
        let tag = &html[start..end];
        if let Some(marker) = marker_in_tag(tag)?
            && found.replace(marker).is_some()
        {
            return Err("clipboard HTML contains multiple Thinoda markers".into());
        }
        cursor = end + 1;
    }
    Ok(found)
}

fn tag_end(bytes: &[u8]) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in bytes.iter().copied().enumerate() {
        match (quote, byte) {
            (Some(current), byte) if current == byte => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some(index),
            _ => {}
        }
    }
    None
}

fn marker_in_tag(tag: &str) -> Result<Option<RichClipboard>, String> {
    let bytes = tag.as_bytes();
    let mut cursor = 0;
    let mut result = None;
    while cursor < bytes.len() {
        while bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'/')
        {
            cursor += 1;
        }
        let start = cursor;
        while bytes
            .get(cursor)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(*byte, b'=' | b'/'))
        {
            cursor += 1;
        }
        if cursor == start {
            break;
        }
        let name = &tag[start..cursor];
        let is_board = name.eq_ignore_ascii_case(BOARD_ATTRIBUTE);
        let is_topic = name.eq_ignore_ascii_case(TOPIC_PAYLOAD_ATTRIBUTE);
        let is_topic_token = name.eq_ignore_ascii_case(TOPIC_ATTRIBUTE);
        if !is_board
            && !is_topic
            && !is_topic_token
            && ["data-thinoda-board-v", "data-thinoda-topic-v"]
                .iter()
                .any(|prefix| {
                    name.get(..prefix.len())
                        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                        && name.get(prefix.len()..).is_some_and(|version| {
                            !version.is_empty() && version.bytes().all(|byte| byte.is_ascii_digit())
                        })
                })
        {
            return Err("unsupported Thinoda clipboard marker".into());
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let value = if bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() {
                cursor += 1;
                let start = cursor;
                let Some(length) = bytes[start..].iter().position(|byte| *byte == quote) else {
                    return if is_board || is_topic || is_topic_token || result.is_some() {
                        Err("invalid clipboard HTML attribute".into())
                    } else {
                        Ok(None)
                    };
                };
                cursor += length + 1;
                Some(&tag[start..start + length])
            } else {
                let start = cursor;
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| !byte.is_ascii_whitespace())
                {
                    cursor += 1;
                }
                Some(&tag[start..cursor])
            }
        } else {
            None
        };
        if !is_board && !is_topic && !is_topic_token {
            continue;
        }
        let decoded = decode_hex(value.ok_or("invalid Thinoda clipboard marker")?)?;
        let marker = if is_board {
            RichClipboard::Board(decoded)
        } else if is_topic {
            RichClipboard::Topic(decoded)
        } else {
            RichClipboard::TopicToken(decoded)
        };
        if result.replace(marker).is_some() {
            return Err("clipboard HTML contains multiple Thinoda markers".into());
        }
    }
    Ok(result)
}

fn decode_hex(value: &str) -> Result<String, String> {
    if value.len() > 2 * MAX_BOARD_CLIPBOARD_BYTES || !value.len().is_multiple_of(2) {
        return Err("invalid or oversized Thinoda clipboard marker".into());
    }
    let mut decoded = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        };
        let high = digit(pair[0]).ok_or("invalid Thinoda clipboard marker")?;
        let low = digit(pair[1]).ok_or("invalid Thinoda clipboard marker")?;
        decoded.push((high << 4) | low);
    }
    String::from_utf8(decoded).map_err(|_| "invalid Thinoda clipboard marker".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use std::{cell::RefCell, rc::Rc};
    use thinoda_core::{
        CanvasObject, CanvasObjectKind, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
    };

    fn board_with_shape() -> OpenDoc {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(0.0, 0.0), Vec2::new(120.0, 80.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Same text".into();
        }
        let id = sheet.add_canvas_object(shape).unwrap();
        let mut doc = OpenDoc::new(Workbook::new(sheet));
        doc.view.board_selection = vec![id];
        doc
    }

    #[test]
    fn markup_round_trips_rich_data_without_exposing_markup_in_text() {
        let rich = RichClipboard::Board("THINODA-BOARD\n{\"label\":\"<a & 😃>\"}".into());
        let html = make_html("<not a tag> & café\nnext", &rich).unwrap();
        assert!(html.contains("&lt;not a tag&gt; &amp; café\nnext"));
        assert!(!html.contains("<not a tag>"));
        assert_eq!(read_html(&html).unwrap(), Some(rich));
    }

    #[test]
    fn formatted_card_html_transport_pastes_and_rejects_a_downgraded_payload() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let title = "A🦀é\n第二";
        let card = sheet
            .add_card(
                title,
                Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Text, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let topic = sheet.topic_mut(card).unwrap();
        topic
            .inline_text
            .update_range(title, 0..3, |style| style.bold = Some(true))
            .unwrap();
        topic
            .inline_text
            .paragraphs
            .push(thinoda_core::InlineParagraph {
                index: 1,
                list: thinoda_core::ListKind::Numbered,
            });
        let mut doc = OpenDoc::new(Workbook::new(sheet));
        doc.view.selection = vec![card];
        let before = doc.doc.workbook().clone();
        let prepared = prepare_copy(&mut doc, false).unwrap().unwrap();
        let rich = read_html(&prepared.html).unwrap().unwrap();
        let RichClipboard::Board(encoded) = &rich else {
            panic!("expected board payload");
        };
        assert!(encoded.starts_with("THINODA-BOARD-CLIPBOARD-2\n"));
        let old = encoded.replacen("THINODA-BOARD-CLIPBOARD-2", "THINODA-BOARD-CLIPBOARD-1", 1);
        assert!(
            paste(
                &mut doc,
                prepared.text.clone(),
                Some(RichClipboard::Board(old)),
            )
            .is_err()
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert!(paste(&mut doc, prepared.text, Some(rich)).unwrap());
        let pasted = doc.view.selection[0];
        assert_ne!(pasted, card);
        assert_eq!(
            doc.doc.sheet().topic(pasted).unwrap().inline_text,
            before.active().topic(card).unwrap().inline_text
        );
        let after = doc.doc.workbook().clone();
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
        assert!(doc.doc.redo());
        assert_eq!(doc.doc.workbook(), &after);
    }

    #[test]
    fn rounded_shape_clipboard_preserves_radius_and_rejects_downgrade() {
        use thinoda_core::{DrawioRectangleStencil, ShapeRounding};

        let mut doc = board_with_shape();
        let id = doc.view.board_selection[0];
        let rounding = ShapeRounding {
            radius: 25.0,
            stencil: DrawioRectangleStencil::Text,
        };
        doc.doc
            .update_canvas_object("Rounding", id, |object| object.rounding = Some(rounding))
            .unwrap();
        let before = doc.doc.workbook().clone();
        let prepared = prepare_copy(&mut doc, false).unwrap().unwrap();
        let rich = read_html(&prepared.html).unwrap().unwrap();
        let RichClipboard::Board(encoded) = &rich else {
            panic!("expected board payload");
        };
        assert!(encoded.starts_with("THINODA-BOARD-CLIPBOARD-12\n"));
        let old = encoded.replacen(
            "THINODA-BOARD-CLIPBOARD-12",
            "THINODA-BOARD-CLIPBOARD-11",
            1,
        );
        assert!(
            paste(
                &mut doc,
                prepared.text.clone(),
                Some(RichClipboard::Board(old))
            )
            .is_err()
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert!(paste(&mut doc, prepared.text, Some(rich)).unwrap());
        let pasted = doc.view.board_selection[0];
        assert_ne!(pasted, id);
        assert_eq!(
            doc.doc.sheet().canvas.object(pasted).unwrap().rounding,
            Some(rounding)
        );
        assert!(doc.doc.undo());
        assert_eq!(doc.doc.workbook(), &before);
    }

    #[test]
    fn cut_verification_requires_matching_text_and_rich_data() {
        let mut source = board_with_shape();
        let prepared = prepare_copy(&mut source, true).unwrap().unwrap();
        let snapshot = |text: String, html: Option<String>| Snapshot { text, html };

        assert!(
            verify_cut_snapshot(
                &prepared,
                &snapshot(
                    prepared.text.replace('\n', "\r\n"),
                    Some(format!("<html><body>{}</body></html>", prepared.html))
                )
            )
            .is_ok()
        );
        for (text, html) in [
            ("different".into(), Some(prepared.html.clone())),
            (prepared.text.clone(), None),
            (prepared.text.clone(), Some("<pre>sanitized</pre>".into())),
            (
                prepared.text.clone(),
                Some("<pre data-thinoda-board-v1='xz'></pre>".into()),
            ),
            (
                prepared.text.clone(),
                Some(make_html("", &RichClipboard::Board("other".into())).unwrap()),
            ),
        ] {
            assert!(verify_cut_snapshot(&prepared, &snapshot(text, html)).is_err());
        }
        assert_eq!(
            verify_cut_snapshot(
                &prepared,
                &snapshot(
                    "x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1),
                    Some(prepared.html.clone())
                )
            )
            .unwrap_err(),
            "clipboard text exceeds the size limit"
        );

        let token = RichClipboard::TopicToken("0123456789abcdef".into());
        let token_copy = PreparedCopy {
            text: "topic".into(),
            html: make_html("topic", &token).unwrap(),
            rich: token,
            cut: None,
            started: Instant::now(),
        };
        assert!(
            verify_cut_snapshot(
                &token_copy,
                &snapshot(token_copy.text.clone(), Some(token_copy.html.clone()))
            )
            .is_ok()
        );
        assert!(
            verify_cut_snapshot(
                &token_copy,
                &snapshot(token_copy.text.clone(), Some(prepared.html.clone()))
            )
            .is_err()
        );
    }

    #[test]
    fn formatted_card_copy_keeps_a_rejected_title_draft() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Styled",
                Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Sticky, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        sheet
            .topic_mut(card)
            .unwrap()
            .inline_text
            .update_range("Styled", 0..3, |style| style.bold = Some(true))
            .unwrap();
        let mut doc = OpenDoc::new(Workbook::new(sheet));
        doc.view.select_only(card);
        doc.view.begin_edit(doc.doc.sheet(), card, false);
        doc.view.set_editing_text("Uncommitted");
        let before = doc.doc.workbook().clone();
        let error = prepare_copy(&mut doc, false).err().unwrap();
        assert_eq!(
            error,
            crate::i18n::t(
                "Formatted card titles cannot be changed until rich-text editing is supported"
            )
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.view.editing_text(), Some("Uncommitted"));
    }

    #[test]
    fn accepts_wrappers_and_single_quoted_attributes() {
        let rich = RichClipboard::TopicToken("0123456789abcdef".into());
        let html = make_html("hello", &rich).unwrap().replace('"', "'");
        let wrapped = format!("<html><body><PRE class='other'>{html}</PRE></body></html>");
        assert_eq!(read_html(&wrapped).unwrap(), Some(rich));
    }

    #[test]
    fn arbitrary_plain_html_and_visible_marker_text_are_not_rich() {
        assert_eq!(
            read_html("<pre>data-thinoda-board-v1=\"deadbeef\"</pre>").unwrap(),
            None
        );
        assert_eq!(
            read_html("<pre title=\" data-thinoda-board-v1='61' >\">ordinary text</pre>").unwrap(),
            None
        );
        assert_eq!(
            read_html("<pre title=\" data-thinoda-topic-v2='61'").unwrap(),
            None
        );
        assert_eq!(read_html("<p>external text</p>").unwrap(), None);
    }

    #[test]
    fn rejects_malformed_and_duplicate_markers() {
        for html in [
            "<pre data-thinoda-board-v1='xz'></pre>",
            "<pre data-thinoda-board-v1='a'></pre>",
            "<pre data-thinoda-board-v1='ff'></pre>",
            "<pre data-thinoda-board-v1='61' data-thinoda-board-v1='62'></pre>",
            "<pre data-thinoda-board-v1='61' data-thinoda-topic-v1='62'></pre>",
            "<pre data-thinoda-topic-v1='61' data-thinoda-topic-v2='62'></pre>",
            "<pre data-thinoda-topic-v2='61'></pre><pre data-thinoda-board-v1='62'></pre>",
            "<pre data-thinoda-topic-v2='61",
            "<pre data-thinoda-board-v1='61'",
            "<pre data-thinoda-topic-v1=61",
            "<pre data-thinoda-topic-v3='61'></pre>",
            "<pre data-thinoda-board-v2='61'></pre>",
        ] {
            assert!(read_html(html).is_err(), "{html}");
        }
    }

    #[test]
    fn rejects_oversized_input_before_creating_html() {
        let huge = "x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1);
        assert!(make_html(&huge, &RichClipboard::TopicToken("x".into())).is_err());
        assert!(make_html("", &RichClipboard::Board(huge)).is_err());
    }

    #[test]
    fn rich_board_snapshot_pastes_into_another_document_and_undoes() {
        let mut source = board_with_shape();
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let rich = read_html(&prepared.html).unwrap();
        assert_eq!(rich, Some(prepared.rich));
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Target", "Root")));
        let before = target.doc.workbook().clone();
        assert!(paste(&mut target, prepared.text.replace('\n', "\r\n"), rich).unwrap());
        assert_eq!(target.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(target.doc.sheet().canvas.objects[0].label(), "Same text");
        assert!(target.doc.undo());
        assert_eq!(target.doc.workbook(), &before);
    }

    #[test]
    fn board_paste_rejects_changed_text_without_editing() {
        let mut source = board_with_shape();
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Target", "Root")));
        let before = target.doc.workbook().clone();

        assert!(paste(&mut target, "Different text".into(), Some(prepared.rich)).is_err());
        assert_eq!(target.doc.workbook(), &before);
    }

    #[test]
    fn cut_is_unchanged_on_write_failure_and_guarded_against_late_completion() {
        let mut source = board_with_shape();
        let before = source.doc.workbook().clone();
        let prepared = prepare_copy(&mut source, true).unwrap().unwrap();
        // A failed clipboard write never calls complete_cut.
        let failed_write: Result<(), &str> = Err("clipboard denied");
        if failed_write.is_ok() {
            complete_cut(&mut source, prepared.cut.as_ref().unwrap()).unwrap();
        }
        assert_eq!(source.doc.workbook(), &before);

        source.view.board_selection.clear();
        assert!(complete_cut(&mut source, prepared.cut.as_ref().unwrap()).is_err());
        assert_eq!(source.doc.workbook(), &before);
    }

    #[test]
    fn successful_prepared_cut_is_one_undoable_edit() {
        let mut source = board_with_shape();
        let before = source.doc.workbook().clone();
        let prepared = prepare_copy(&mut source, true).unwrap().unwrap();
        // A transport would confirm its write before this call.
        complete_cut(&mut source, prepared.cut.as_ref().unwrap()).unwrap();
        assert!(source.doc.sheet().canvas.objects.is_empty());
        assert!(source.view.clipboard_out.is_none());
        assert!(source.doc.undo());
        assert_eq!(source.doc.workbook(), &before);
    }

    #[test]
    fn identical_external_text_does_not_restore_stale_board_data() {
        let mut source = board_with_shape();
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        assert!(source.view.board_clipboard().is_some());
        assert!(paste(&mut source, prepared.text, None).unwrap());
        assert_eq!(source.doc.sheet().canvas.objects.len(), 1);
        assert!(source.view.board_clipboard().is_none());
        let pasted = source.doc.sheet().topic(source.view.selection[0]).unwrap();
        assert_eq!(pasted.title, "Same text");
    }

    #[test]
    fn topic_payload_survives_without_a_session_cache_and_supports_undo() {
        let mut sheet = Sheet::new("Map", "Root");
        let root = sheet.root;
        let child = sheet.add_child(root, None, "Same text");
        sheet.add_child(child, None, "Nested");
        sheet.topic_mut(child).unwrap().style.bold = Some(true);
        sheet.topic_mut(child).unwrap().notes = Some(thinoda_core::notes::NoteContent::markdown(
            "**Notes**".into(),
        ));
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(child);
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        assert!(matches!(&prepared.rich, RichClipboard::Topic(_)));
        assert_eq!(
            read_html(&prepared.html).unwrap(),
            Some(prepared.rich.clone())
        );

        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Other", "Root")));
        let before = target.doc.workbook().clone();
        assert!(
            paste(
                &mut target,
                prepared.text.replace('\n', "\r\n"),
                Some(prepared.rich.clone())
            )
            .unwrap()
        );
        let pasted = target.doc.sheet().topic(target.view.selection[0]).unwrap();
        assert_eq!(pasted.style.bold, Some(true));
        assert_eq!(
            pasted.notes,
            Some(thinoda_core::notes::NoteContent::markdown(
                "**Notes**".into()
            ))
        );
        assert_eq!(
            target.doc.sheet().topic(pasted.children[0]).unwrap().title,
            "Nested"
        );
        let after = target.doc.workbook().clone();
        assert!(target.doc.undo());
        assert_eq!(target.doc.workbook(), &before);
        assert!(target.doc.redo());
        assert_eq!(target.doc.workbook(), &after);

        let mut plain_target = OpenDoc::new(Workbook::new(Sheet::new("Plain", "Root")));
        assert!(paste(&mut plain_target, prepared.text, None).unwrap());
        assert_eq!(
            plain_target
                .doc
                .sheet()
                .topic(plain_target.view.selection[0])
                .unwrap()
                .style
                .bold,
            None
        );
    }

    #[test]
    fn portable_topic_clipboard_preserves_blank_and_whitespace_titles() {
        for title in ["", "  ", "\t"] {
            let mut sheet = Sheet::new("Source", "Root");
            let topic = sheet.add_child(sheet.root, None, title);
            let mut source = OpenDoc::new(Workbook::new(sheet));
            source.view.select_only(topic);
            let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
            let mut target = OpenDoc::new(Workbook::new(Sheet::new("Target", "Root")));

            assert!(paste(&mut target, prepared.text, Some(prepared.rich)).unwrap());
            assert_eq!(
                target
                    .doc
                    .sheet()
                    .topic(target.view.selection[0])
                    .unwrap()
                    .title,
                title
            );
        }
    }

    #[test]
    fn invalid_topic_style_cannot_delete_a_cut_selection() {
        let mut sheet = Sheet::new("Source", "Root");
        let topic = sheet.add_child(sheet.root, None, "Invalid font");
        sheet.topic_mut(topic).unwrap().style.font_size = Some(0.0);
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(topic);
        let before = source.doc.workbook().clone();

        assert!(prepare_copy(&mut source, true).is_err());
        assert_eq!(source.doc.workbook(), &before);
    }

    #[test]
    fn deep_topic_selection_is_rejected_before_copy_or_cut() {
        let mut sheet = Sheet::new("Source", "Root");
        let selected = sheet.add_child(sheet.root, None, "Selected");
        let mut parent = selected;
        for _ in 0..=MAX_TOPIC_CLIPBOARD_DEPTH {
            parent = sheet.add_child(parent, None, "Child");
        }
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(selected);
        let before = source.doc.workbook().clone();
        for cut in [false, true] {
            assert!(matches!(
                prepare_copy(&mut source, cut),
                Err(error) if error.contains("depth")
            ));
            assert_eq!(source.doc.workbook(), &before);
        }
    }

    #[test]
    fn deep_board_selection_is_rejected_before_layout_copy_or_cut() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Card",
                Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Sticky, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let mut parent = card;
        for _ in 0..=MAX_TOPIC_CLIPBOARD_DEPTH {
            parent = sheet.add_child(parent, None, "Child");
        }
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(card);
        let before = source.doc.workbook().clone();
        for cut in [false, true] {
            assert!(matches!(
                prepare_copy(&mut source, cut),
                Err(error) if error.contains("depth")
            ));
            assert_eq!(source.doc.workbook(), &before);
            assert!(!source.doc.can_undo());
        }
    }

    #[test]
    fn portable_topic_paste_checks_depth_against_destination_ancestry() {
        let mut source = Sheet::new("Source", "Root");
        let copied = source.add_child(source.root, None, "Parent");
        source.add_child(copied, None, "Child");
        let payload = clipboard::copy_topics(&source, &[copied]).unwrap().unwrap();
        let encoded = clipboard::encode_topic_payload(&payload).unwrap();

        for (parent_depth, should_succeed) in [(254, true), (255, false)] {
            let mut sheet = Sheet::new("Destination", "Root");
            let mut parent = sheet.root;
            for _ in 0..parent_depth {
                parent = sheet.add_child(parent, None, "Ancestor");
            }
            let mut destination = OpenDoc::new(Workbook::new(sheet));
            destination.view.select_only(parent);
            let before = destination.doc.workbook().clone();
            let result = paste(
                &mut destination,
                payload.text.clone(),
                Some(RichClipboard::Topic(encoded.clone())),
            );
            if should_succeed {
                assert!(result.unwrap());
            } else {
                assert!(result.unwrap_err().contains("depth"));
                assert_eq!(destination.doc.workbook(), &before);
                assert!(destination.view.clipboard.is_none());
            }
        }
    }

    #[test]
    fn plain_text_paste_checks_depth_and_preserves_undo_on_map_and_board() {
        fn nested_text(depth: usize) -> String {
            let mut text = "Root\n".to_string();
            for level in 1..=depth {
                text.extend(std::iter::repeat_n('\t', level));
                text.push_str("Child\n");
            }
            text
        }

        for rootless in [false, true] {
            let mut sheet = Sheet::new("Destination", "");
            if rootless {
                sheet.root_mode = SheetRootMode::Hidden;
            }
            let mut destination = OpenDoc::new(Workbook::new(sheet));
            let before = destination.doc.workbook().clone();
            let valid_depth = MAX_TOPIC_CLIPBOARD_DEPTH - usize::from(!rootless);
            let error = paste(&mut destination, nested_text(valid_depth + 1), None).unwrap_err();
            assert!(error.contains("depth"), "{error}");
            assert_eq!(destination.doc.workbook(), &before);
            assert!(!destination.doc.can_undo());

            assert!(paste(&mut destination, nested_text(valid_depth), None).unwrap());
            let after = destination.doc.workbook().clone();
            assert!(destination.doc.undo());
            assert_eq!(destination.doc.workbook(), &before);
            assert!(destination.doc.redo());
            assert_eq!(destination.doc.workbook(), &after);
        }
    }

    #[test]
    fn plain_text_paste_counts_summary_ownership_in_destination_depth() {
        for (descendants, valid) in [(253, true), (254, false)] {
            let mut sheet = Sheet::new("Destination", "Root");
            let covered = sheet.add_child(sheet.root, None, "Covered");
            let mut parent = sheet.add_summary(&[covered], "Summary").unwrap();
            for _ in 0..descendants {
                parent = sheet.add_child(parent, None, "Ancestor");
            }
            let mut destination = OpenDoc::new(Workbook::new(sheet));
            destination.view.select_only(parent);
            let before = destination.doc.workbook().clone();
            let result = paste(&mut destination, "One\n\tTwo".into(), None);
            if valid {
                assert!(result.unwrap());
                assert!(destination.doc.undo());
                assert_eq!(destination.doc.workbook(), &before);
            } else {
                assert!(result.unwrap_err().contains("depth"));
                assert_eq!(destination.doc.workbook(), &before);
                assert!(!destination.doc.can_undo());
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn rejected_plain_text_paste_shows_an_error_toast() {
        use crate::app::Cmd;

        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(Sheet::new("Map", "Root")))];
        let mut text = "Root\n".to_string();
        for level in 1..=MAX_TOPIC_CLIPBOARD_DEPTH + 1 {
            text.extend(std::iter::repeat_n('\t', level));
            text.push_str("Child\n");
        }
        clipboard.borrow_mut().text = text;
        clipboard.borrow_mut().html = None;
        let before = app.doc().unwrap().doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::MenuPaste);

        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(!app.doc().unwrap().doc.can_undo());
        assert!(
            app.toasts
                .texts()
                .any(|message| message.contains("paste") && message.contains("depth"))
        );
    }

    #[test]
    fn portable_card_payload_cannot_silently_lose_its_presentation() {
        let mut source = Sheet::new("Source", "Root");
        let card = source
            .add_card(
                "Card",
                Vec2::new(40.0, 60.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Sticky, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        let payload = clipboard::copy_topics(&source, &[card]).unwrap().unwrap();
        let encoded = clipboard::encode_topic_payload(&payload).unwrap();
        let mut destination = OpenDoc::new(Workbook::new(Sheet::new("Destination", "Root")));
        let before = destination.doc.workbook().clone();

        assert!(
            paste(
                &mut destination,
                payload.text.clone(),
                Some(RichClipboard::Topic(encoded.clone()))
            )
            .is_err()
        );
        assert_eq!(destination.doc.workbook(), &before);
        assert!(destination.view.clipboard.is_none());

        let mut board = Sheet::new("Board", "");
        board.root_mode = SheetRootMode::Hidden;
        let mut destination = OpenDoc::new(Workbook::new(board));
        assert!(
            paste(
                &mut destination,
                payload.text,
                Some(RichClipboard::Topic(encoded))
            )
            .unwrap()
        );
        let pasted = destination.view.selection[0];
        assert_eq!(
            destination.doc.sheet().topic(pasted).unwrap().card,
            source.topic(card).unwrap().card
        );
    }

    #[test]
    fn topic_paste_rejects_changed_text_or_invalid_payload_without_editing() {
        let mut sheet = Sheet::new("Map", "Root");
        let child = sheet.add_child(sheet.root, None, "Same text");
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(child);
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let RichClipboard::Topic(encoded) = prepared.rich else {
            panic!("expected topic payload");
        };
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Other", "Root")));
        let before = target.doc.workbook().clone();

        assert!(
            paste(
                &mut target,
                "Changed text".into(),
                Some(RichClipboard::Topic(encoded.clone()))
            )
            .is_err()
        );
        assert_eq!(target.doc.workbook(), &before);
        assert!(
            paste(
                &mut target,
                prepared.text,
                Some(RichClipboard::Topic(format!("invalid{encoded}")))
            )
            .is_err()
        );
        assert_eq!(target.doc.workbook(), &before);
    }

    #[test]
    fn rejected_topic_paste_does_not_leave_a_cached_payload() {
        let mut sheet = Sheet::new("Map", "Root");
        let child = sheet.add_child(sheet.root, None, "Source");
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(child);
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let xml = format!(
            "<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'><x:stammdatenschema><x:identifikation><x:id>S1</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>",
            thinoda_core::fim::Dialect::Xdf3.namespace()
        );
        let sheet = thinoda_core::fim::FimDocument::parse(xml.as_bytes())
            .unwrap()
            .into_sheet()
            .unwrap();
        let mut target = OpenDoc::new(Workbook::new(sheet));
        let before = target.doc.workbook().clone();

        assert!(paste(&mut target, prepared.text, Some(prepared.rich)).is_err());
        assert_eq!(target.doc.workbook(), &before);
        assert!(target.view.clipboard.is_none());
    }

    #[test]
    fn portable_topic_paste_rejects_incompatible_destination_calendars() {
        let mut sheet = Sheet::new("Source", "Root");
        let topic = sheet.add_child(sheet.root, None, "Scheduled");
        sheet
            .scheduling
            .calendar
            .working_weekdays
            .insert(thinoda_core::Weekday::Saturday);
        sheet
            .set_task(
                topic,
                Some(thinoda_core::Task {
                    start_date: Some("2024-03-02".parse().unwrap()),
                    ..Default::default()
                }),
            )
            .unwrap();
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(topic);
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Target", "Root")));
        let before = target.doc.workbook().clone();

        assert!(
            paste(
                &mut target,
                prepared.text.clone(),
                Some(prepared.rich.clone())
            )
            .unwrap_err()
            .contains("nonworking")
        );
        assert_eq!(target.doc.workbook(), &before);
        assert!(target.view.clipboard.is_none());

        target
            .doc
            .edit_sheet("Allow Saturday", |sheet| {
                sheet
                    .scheduling
                    .calendar
                    .working_weekdays
                    .insert(thinoda_core::Weekday::Saturday);
                Ok(())
            })
            .unwrap();
        assert!(paste(&mut target, prepared.text, Some(prepared.rich)).unwrap());
        assert_eq!(target.doc.sheet().scheduling.tasks.len(), 1);
        target.doc.workbook().validate().unwrap();
    }

    #[test]
    fn older_topic_token_falls_back_to_plain_text() {
        let token = RichClipboard::TopicToken("0123456789abcdef".into());
        let html = make_html("Legacy topic", &token).unwrap();
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Other", "Root")));
        assert!(
            paste(
                &mut target,
                "Legacy topic".into(),
                read_html(&html).unwrap()
            )
            .unwrap()
        );
        let pasted = target.doc.sheet().topic(target.view.selection[0]).unwrap();
        assert_eq!(pasted.title, "Legacy topic");
        assert_eq!(pasted.style.bold, None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn app_with_fake_clipboard() -> (crate::app::ThinodaApp, Rc<RefCell<native::FakeClipboard>>) {
        let fake = Rc::new(RefCell::new(native::FakeClipboard::default()));
        let mut app = crate::app::ThinodaApp::headless();
        app.docs = vec![board_with_shape()];
        app.active = 0;
        app.clipboard_bridge = ClipboardBridge::with_fake(fake.clone());
        (app, fake)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sheet_copy_writes_editable_and_svg_representations_together() {
        use crate::app::Cmd;

        let (mut app, clipboard) = app_with_fake_clipboard();
        let original = app.doc().unwrap().doc.sheet().clone();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::CopySheet);

        let text = {
            let clipboard = clipboard.borrow();
            assert_eq!(
                thinoda_core::clipboard::decode_sheet(&clipboard.text).unwrap(),
                original
            );
            assert!(clipboard.svg.as_deref().unwrap().contains("Same text"));
            assert!(clipboard.html.is_none());
            clipboard.text.clone()
        };
        assert!(
            app.toasts
                .texts()
                .any(|text| { text.contains(crate::i18n::t("Copied sheet and SVG to clipboard")) })
        );
        app.exec(&ctx, Cmd::PasteSheet);
        let input = egui::RawInput {
            events: vec![egui::Event::Paste(text)],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.handle_sheet_paste(ui.ctx()))
            .drop_without_applying_deltas();
        assert_eq!(app.doc().unwrap().doc.workbook().sheets.len(), 2);
        assert_eq!(app.doc().unwrap().doc.sheet(), &original);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sheet_copy_reports_partial_and_complete_clipboard_failures() {
        use crate::app::Cmd;

        let (mut app, clipboard) = app_with_fake_clipboard();
        clipboard.borrow_mut().fail_svg_write = true;
        app.exec(&egui::Context::default(), Cmd::CopySheet);
        assert!(thinoda_core::clipboard::decode_sheet(&clipboard.borrow().text).is_ok());
        assert!(clipboard.borrow().svg.is_none());
        assert!(app.toasts.texts().any(|text| {
            text.contains(crate::i18n::t(
                "Copied sheet as text only; SVG was not copied",
            ))
        }));

        let (mut app, clipboard) = app_with_fake_clipboard();
        {
            let mut clipboard = clipboard.borrow_mut();
            clipboard.text = "existing".into();
            clipboard.fail_write = true;
        }
        app.exec(&egui::Context::default(), Cmd::CopySheet);
        assert_eq!(clipboard.borrow().text, "existing");
        assert!(app.toasts.texts().any(|text| {
            text.contains(crate::i18n::t("Could not copy sheet"))
                && text.contains("clipboard write denied")
        }));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sheet_svg_generation_failure_retains_the_editable_clipboard_data() {
        let (mut app, clipboard) = app_with_fake_clipboard();
        let prepared = PreparedSheet {
            text: "editable sheet".into(),
            svg: Err("SVG generation failed".into()),
        };
        let ctx = egui::Context::default();
        let completion = app.clipboard_bridge.write_sheet(&ctx, prepared).unwrap();
        app.finish_clipboard(&ctx, completion, false);
        assert_eq!(clipboard.borrow().text, "editable sheet");
        assert!(clipboard.borrow().svg.is_none());
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains("SVG generation failed"))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn subsequent_board_copy_removes_the_sheet_svg_representation() {
        use crate::app::Cmd;

        let (mut app, clipboard) = app_with_fake_clipboard();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::CopySheet);
        assert!(clipboard.borrow().svg.is_some());
        app.exec(&ctx, Cmd::Canvas(Action::Copy));
        assert!(clipboard.borrow().svg.is_none());
        assert!(clipboard.borrow().html.is_some());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sheet_copy_supersedes_a_pending_cut_without_deleting_content() {
        use crate::app::Cmd;

        let (mut app, _) = app_with_fake_clipboard();
        let before = app.doc().unwrap().doc.workbook().clone();
        let pending_cut = prepare_copy(app.doc_mut().unwrap(), true).unwrap().unwrap();
        app.clipboard_bridge.generation = 1;
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::CopySheet);
        app.finish_clipboard(
            &ctx,
            Completion::Write {
                generation: 1,
                prepared: pending_cut,
                result: Ok(()),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cut_preparation_failure_retains_the_draft_and_reports_cancellation() {
        use crate::app::Cmd;

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Styled",
                Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Sticky, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        sheet
            .topic_mut(card)
            .unwrap()
            .inline_text
            .update_range("Styled", 0..3, |style| style.bold = Some(true))
            .unwrap();
        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(sheet))];
        let doc = app.doc_mut().unwrap();
        doc.view.select_only(card);
        doc.view.begin_edit(doc.doc.sheet(), card, false);
        doc.view.set_editing_text("Uncommitted");
        let before = doc.doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::Canvas(Action::Cut));

        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.view.editing_text(), Some("Uncommitted"));
        assert!(clipboard.borrow().html.is_none());
        assert!(
            app.toasts
                .texts()
                .any(|text| text
                    .contains(crate::i18n::t("Cut cancelled; original content was kept")))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn older_topic_token_warns_when_only_text_can_be_pasted() {
        use crate::app::Cmd;

        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(Sheet::new("Map", "Root")))];
        let token = RichClipboard::TopicToken("0123456789abcdef".into());
        {
            let mut clipboard = clipboard.borrow_mut();
            clipboard.text = "Legacy topic".into();
            clipboard.html = Some(make_html(&clipboard.text, &token).unwrap());
        }

        app.exec(&egui::Context::default(), Cmd::MenuPaste);

        assert!(
            app.doc()
                .unwrap()
                .doc
                .sheet()
                .topics
                .values()
                .any(|topic| topic.title == "Legacy topic")
        );
        assert!(app.toasts.texts().any(|text| text.contains(crate::i18n::t(
            "Older Thinoda clipboard data is unavailable; pasted text only"
        ))));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_topic_copy_survives_a_new_app_instance() {
        use crate::app::Cmd;

        let mut sheet = Sheet::new("Source", "Root");
        let topic = sheet.add_child(sheet.root, None, "Styled topic");
        sheet.topic_mut(topic).unwrap().style.bold = Some(true);
        sheet.add_child(topic, None, "Nested topic");
        let (mut source, clipboard) = app_with_fake_clipboard();
        source.docs = vec![OpenDoc::new(Workbook::new(sheet))];
        source.doc_mut().unwrap().view.select_only(topic);
        source.exec(&egui::Context::default(), Cmd::Canvas(Action::Copy));
        assert!(matches!(
            read_html(clipboard.borrow().html.as_deref().unwrap()).unwrap(),
            Some(RichClipboard::Topic(_))
        ));
        drop(source);

        let mut target = crate::app::ThinodaApp::headless();
        target.docs = vec![OpenDoc::new(Workbook::new(Sheet::new("Target", "Root")))];
        target.active = 0;
        target.clipboard_bridge = ClipboardBridge::with_fake(clipboard);
        let before = target.doc().unwrap().doc.workbook().clone();
        target.exec(&egui::Context::default(), Cmd::MenuPaste);
        let doc = target.doc().unwrap();
        let pasted = doc.doc.sheet().topic(doc.view.selection[0]).unwrap();
        assert_eq!(pasted.title, "Styled topic");
        assert_eq!(pasted.style.bold, Some(true));
        assert_eq!(
            doc.doc.sheet().topic(pasted.children[0]).unwrap().title,
            "Nested topic"
        );
        target.exec(&egui::Context::default(), Cmd::Canvas(Action::Undo));
        assert_eq!(target.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn menu_copy_and_paste_use_separate_rich_representation_across_documents() {
        use crate::app::Cmd;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::Canvas(Action::Copy));
        assert_eq!(clipboard.borrow().text.trim_end(), "Same text");
        assert!(clipboard.borrow().html.is_some());
        app.docs
            .push(OpenDoc::new(Workbook::new(Sheet::new("Other", "Root"))));
        app.active = 1;
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.sheet().canvas.objects.len(), 1);
        assert_eq!(
            app.doc().unwrap().doc.sheet().canvas.objects[0].label(),
            "Same text"
        );
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert!(app.doc().unwrap().doc.sheet().canvas.objects.is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn menu_cut_never_deletes_when_the_clipboard_write_fails() {
        use crate::app::Cmd;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().fail_write = true;
        app.exec(&egui::Context::default(), Cmd::Canvas(Action::Cut));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains("clipboard write denied"))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_cut_retains_original_when_clipboard_readback_is_unreliable() {
        use crate::app::Cmd;

        for scenario in [
            "text",
            "missing-html",
            "sanitized-html",
            "malformed-html",
            "changed-rich",
            "read-denied",
            "changed-during-read",
        ] {
            let (mut app, clipboard) = app_with_fake_clipboard();
            let before = app.doc().unwrap().doc.workbook().clone();
            {
                let mut fake = clipboard.borrow_mut();
                match scenario {
                    "text" => fake.written_text = Some("Different text".into()),
                    "missing-html" => fake.written_html = Some(None),
                    "sanitized-html" => fake.written_html = Some(Some("<p>Same text</p>".into())),
                    "malformed-html" => {
                        fake.written_html =
                            Some(Some("<pre data-thinoda-board-v1='xz'></pre>".into()));
                    }
                    "changed-rich" => {
                        fake.written_html = Some(Some(
                            make_html("", &RichClipboard::Board("other".into())).unwrap(),
                        ));
                    }
                    "read-denied" => fake.fail_read = true,
                    "changed-during-read" => {
                        fake.read_html_after = Some(Some("<p>Changed clipboard</p>".into()));
                    }
                    _ => unreachable!(),
                }
            }

            app.exec(&egui::Context::default(), Cmd::Canvas(Action::Cut));

            assert_eq!(app.doc().unwrap().doc.workbook(), &before, "{scenario}");
            assert!(
                app.toasts.texts().any(|text| {
                    text.contains(crate::i18n::t("Cut cancelled; original content was kept"))
                }),
                "{scenario}"
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_formatted_card_cut_verifies_then_pastes_and_undoes() {
        use crate::app::Cmd;

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Styled card",
                Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(thinoda_core::CardKind::Text, Vec2::new(160.0, 100.0)),
            )
            .unwrap();
        sheet
            .topic_mut(card)
            .unwrap()
            .inline_text
            .update_range("Styled card", 0..6, |style| style.bold = Some(true))
            .unwrap();
        let formatting = sheet.topic(card).unwrap().inline_text.clone();
        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(sheet))];
        app.doc_mut().unwrap().view.select_only(card);
        let before = app.doc().unwrap().doc.workbook().clone();
        let ctx = egui::Context::default();

        clipboard.borrow_mut().written_text = Some("Different text".into());
        app.exec(&ctx, Cmd::Canvas(Action::Cut));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);

        clipboard.borrow_mut().written_text = None;
        app.exec(&ctx, Cmd::Canvas(Action::Cut));
        assert!(!app.doc().unwrap().doc.sheet().contains(card));
        app.exec(&ctx, Cmd::MenuPaste);
        let doc = app.doc().unwrap();
        let pasted = doc.view.primary().unwrap();
        assert_ne!(pasted, card);
        assert_eq!(
            doc.doc.sheet().topic(pasted).unwrap().inline_text,
            formatting
        );

        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert!(!app.doc().unwrap().doc.sheet().contains(pasted));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_topic_cut_verifies_self_contained_payload_before_deletion() {
        use crate::app::Cmd;

        let mut sheet = Sheet::new("Map", "Root");
        let topic = sheet.add_child(sheet.root, None, "Topic");
        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(sheet))];
        app.doc_mut().unwrap().view.select_only(topic);
        let before = app.doc().unwrap().doc.workbook().clone();
        let ctx = egui::Context::default();

        clipboard.borrow_mut().written_text = Some("Different text".into());
        app.exec(&ctx, Cmd::Canvas(Action::Cut));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);

        clipboard.borrow_mut().written_text = None;
        app.exec(&ctx, Cmd::Canvas(Action::Cut));
        assert!(!app.doc().unwrap().doc.sheet().contains(topic));
        assert!(matches!(
            read_html(clipboard.borrow().html.as_deref().unwrap()).unwrap(),
            Some(RichClipboard::Topic(_))
        ));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn external_identical_text_invalidates_all_view_caches() {
        use crate::app::Cmd;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::Canvas(Action::Copy));
        assert!(app.doc().unwrap().view.board_clipboard().is_some());
        // A different application replaced the clipboard with the same plain
        // text but no Thinoda HTML representation.
        clipboard.borrow_mut().html = None;
        app.exec(&ctx, Cmd::MenuPaste);
        let doc = app.doc().unwrap();
        assert!(doc.view.board_clipboard().is_none());
        assert_eq!(doc.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(
            doc.doc.sheet().topic(doc.view.selection[0]).unwrap().title,
            "Same text"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn canvas_keyboard_copy_cut_and_paste_use_the_same_transport() {
        use egui_kittest::Harness;
        let (app, clipboard) = app_with_fake_clipboard();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 800.0))
            .build_ui_state(|ui, app: &mut crate::app::ThinodaApp| app.draw(ui), app);
        h.ctx
            .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        h.run_steps(3);
        h.ctx
            .memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
        h.event(egui::Event::Copy);
        h.run_steps(2);
        assert!(clipboard.borrow().html.is_some());
        h.event(egui::Event::Cut);
        h.run_steps(2);
        assert!(
            h.state()
                .doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .objects
                .is_empty()
        );
        h.event(egui::Event::Paste("Same text".into()));
        h.run_steps(2);
        assert_eq!(h.state().doc().unwrap().doc.sheet().canvas.objects.len(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn malformed_rich_data_reports_failure_without_pasting_or_reusing_cache() {
        use crate::app::Cmd;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::Canvas(Action::Copy));
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().html = Some("<pre data-thinoda-board-v1='xz'></pre>".into());
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.doc().unwrap().view.board_clipboard().is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn delayed_cut_cannot_delete_a_changed_selection_or_an_older_request() {
        let (mut app, _) = app_with_fake_clipboard();
        let before = app.doc().unwrap().doc.workbook().clone();
        let prepared = prepare_copy(app.doc_mut().unwrap(), true).unwrap().unwrap();
        app.clipboard_bridge.generation = 1;
        app.doc_mut().unwrap().view.board_selection.clear();
        app.finish_clipboard(
            &egui::Context::default(),
            Completion::Write {
                generation: 1,
                prepared,
                result: Ok(()),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);

        app.doc_mut().unwrap().view.board_selection =
            vec![app.doc().unwrap().doc.sheet().canvas.objects[0].id];
        let prepared = prepare_copy(app.doc_mut().unwrap(), true).unwrap().unwrap();
        app.clipboard_bridge.generation = 3;
        app.finish_clipboard(
            &egui::Context::default(),
            Completion::Write {
                generation: 2,
                prepared,
                result: Ok(()),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cut_cannot_delete_after_verification_expires() {
        for delayed in [false, true] {
            let (mut app, _) = app_with_fake_clipboard();
            let before = app.doc().unwrap().doc.workbook().clone();
            let mut prepared = prepare_copy(app.doc_mut().unwrap(), true).unwrap().unwrap();
            prepared.started = Instant::now() - MAX_PENDING_CUT - std::time::Duration::from_secs(1);
            app.clipboard_bridge.generation = 1;
            app.finish_clipboard(
                &egui::Context::default(),
                Completion::Write {
                    generation: 1,
                    prepared,
                    result: Ok(()),
                },
                delayed,
            );
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
            assert!(
                app.toasts
                    .texts()
                    .any(|text| text.contains("clipboard verification took too long"))
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn timed_out_cut_reports_that_the_original_was_kept() {
        let (mut app, _) = app_with_fake_clipboard();
        let before = app.doc().unwrap().doc.workbook().clone();
        let prepared = prepare_copy(app.doc_mut().unwrap(), true).unwrap().unwrap();
        app.clipboard_bridge.generation = 1;
        app.finish_clipboard(
            &egui::Context::default(),
            Completion::Write {
                generation: 1,
                prepared,
                result: Err("clipboard operation timed out".into()),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.toasts.texts().any(|text| {
            text.contains("Cut cancelled; original content was kept")
                && text.contains("clipboard operation timed out")
        }));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn delayed_paste_cannot_target_a_changed_document_or_active_editor() {
        let (mut app, _) = app_with_fake_clipboard();
        let prepared = prepare_copy(app.doc_mut().unwrap(), false)
            .unwrap()
            .unwrap();
        let guard = DocumentGuard::capture(app.doc().unwrap());
        let before = app.doc().unwrap().doc.workbook().clone();
        let object = app.doc().unwrap().view.board_selection[0];
        {
            let doc = app.doc_mut().unwrap();
            assert!(doc.view.begin_board_label(&doc.doc, object));
        }
        app.finish_clipboard(
            &egui::Context::default(),
            Completion::Read {
                guard,
                fallback: None,
                result: Ok(Snapshot {
                    text: prepared.text,
                    html: Some(prepared.html),
                }),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.doc().unwrap().view.editing_board_object().is_some());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn delayed_clipboard_application_waits_for_unresolved_ink() {
        use thinoda_canvas::ink_controller::InkControllerError;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let ctx = egui::Context::default();
        app.clipboard_action(&ctx, Action::Copy);
        let guard = DocumentGuard::capture(app.doc().unwrap());
        let before = app.doc().unwrap().doc.workbook().clone();
        let snapshot = Snapshot {
            text: clipboard.borrow().text.clone(),
            html: clipboard.borrow().html.clone(),
        };
        app.doc_mut()
            .unwrap()
            .ink_feedback
            .record(InkControllerError::GesturePending);
        app.finish_clipboard(
            &ctx,
            Completion::Read {
                guard,
                fallback: None,
                result: Ok(snapshot),
            },
            true,
        );
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.doc().unwrap().has_unresolved_ink());
        assert!(clipboard.borrow().html.is_some());
        app.doc_mut().unwrap().discard_pending_ink();
        app.clipboard_action(&ctx, Action::Paste(None));
        assert_eq!(app.doc().unwrap().doc.sheet().canvas.objects.len(), 2);
    }

    #[cfg(not(target_arch = "wasm32"))]
    const DRAWIO_MODEL: &str = concat!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
        r#"<mxCell id="a" value="Start" style="rounded=1;whiteSpace=wrap;html=1;" vertex="1" parent="1">"#,
        r#"<mxGeometry x="100" y="100" width="120" height="60" as="geometry"/></mxCell>"#,
        r#"<mxCell id="b" value="End" style="ellipse;whiteSpace=wrap;html=1;" vertex="1" parent="1">"#,
        r#"<mxGeometry x="300" y="100" width="100" height="60" as="geometry"/></mxCell>"#,
        r#"<mxCell id="e" style="endArrow=classic;html=1;" edge="1" parent="1" source="a" target="b">"#,
        r#"<mxGeometry relative="1" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel>"#
    );

    #[cfg(not(target_arch = "wasm32"))]
    const DRAWIO_FRAMED_MODEL: &str = concat!(
        r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
        r#"<mxCell id="frame" value="Frame" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1">"#,
        r#"<mxGeometry x="100" y="200" width="240" height="120" as="geometry"/></mxCell>"#,
        r#"<mxCell id="child" value="Child" style="shape=rectangle;" vertex="1" parent="frame">"#,
        r#"<mxGeometry x="20" y="30" width="50" height="40" as="geometry"/></mxCell>"#,
        r#"</root></mxGraphModel>"#
    );

    #[cfg(not(target_arch = "wasm32"))]
    fn empty_board_app() -> (crate::app::ThinodaApp, Rc<RefCell<native::FakeClipboard>>) {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let (mut app, clipboard) = app_with_fake_clipboard();
        app.docs = vec![OpenDoc::new(Workbook::new(sheet))];
        app.active = 0;
        (app, clipboard)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn board_item_count(app: &crate::app::ThinodaApp) -> usize {
        let sheet = app.doc().unwrap().doc.sheet();
        sheet.topics.len() + sheet.canvas.objects.len()
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_text_pastes_as_one_undoable_selection() {
        use crate::app::Cmd;
        for text in [
            DRAWIO_MODEL.to_owned(),
            thinoda_io::drawio_clipboard::encode_uri_component(DRAWIO_MODEL),
        ] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            let items = board_item_count(&app);
            {
                let mut fake = clipboard.borrow_mut();
                fake.text = text;
                fake.html = None;
            }
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            assert_eq!(board_item_count(&app), items + 3);
            let doc = app.doc().unwrap();
            assert!(doc.view.selection.len() + doc.view.board_selection.len() >= 2);
            assert!(doc.view.board_clipboard().is_none());

            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_omitted_frames_and_pages_until_dismissed() {
        use crate::app::Cmd;

        let file = format!(
            "<mxfile><diagram name=\"First\">{DRAWIO_FRAMED_MODEL}</diagram><diagram name=\"Second\">{DRAWIO_MODEL}</diagram></mxfile>"
        );
        for (text, extra_page) in [
            (DRAWIO_FRAMED_MODEL.to_owned(), false),
            (file.clone(), true),
            (
                thinoda_io::drawio_clipboard::encode_uri_component(&file),
                true,
            ),
        ] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            let items = board_item_count(&app);
            clipboard.borrow_mut().text = text;
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            assert_eq!(board_item_count(&app), items + 1);
            app.toasts.tick(60.0);
            let notice = app
                .toasts
                .texts()
                .find(|text| text.contains(crate::i18n::t("Container frames omitted")))
                .expect("omitted frame warning should stay visible");
            assert!(notice.starts_with(crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
            assert!(notice.contains(&format!(
                "1 — {}",
                crate::i18n::t("Container frames omitted")
            )));
            assert_eq!(
                notice.contains(crate::i18n::t("Additional draw.io pages omitted")),
                extra_page
            );
            if extra_page {
                assert!(notice.contains(&format!(
                    "1 — {}",
                    crate::i18n::t("Additional draw.io pages omitted")
                )));
            }

            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_additional_pages_without_frames() {
        use crate::app::Cmd;

        let file = format!(
            "<mxfile><diagram name=\"First\">{DRAWIO_MODEL}</diagram><diagram name=\"Second\">{DRAWIO_MODEL}</diagram></mxfile>"
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let items = board_item_count(&app);
        clipboard.borrow_mut().text = file;
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 3);
        app.toasts.tick(60.0);
        let notice = app
            .toasts
            .texts()
            .find(|text| text.contains(crate::i18n::t("Additional draw.io pages omitted")))
            .expect("omitted page warning should stay visible");
        assert!(notice.contains(&format!(
            "1 — {}",
            crate::i18n::t("Additional draw.io pages omitted")
        )));
        assert!(!notice.contains(crate::i18n::t("Container frames omitted")));

        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_detached_connectors_with_and_without_frames() {
        use crate::app::Cmd;

        let missing_terminal = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" value="Shape" vertex="1" parent="1"><mxGeometry x="100" width="100" height="60" as="geometry"/></mxCell>"#,
            r#"<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="sourcePoint"/></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let framed = DRAWIO_FRAMED_MODEL.replace(
            "</root>",
            r#"<mxCell id="edge" edge="1" parent="1" source="frame" target="child" style="endArrow=none;"><mxGeometry relative="1" as="geometry"/></mxCell></root>"#,
        );
        for (text, has_frame) in [(missing_terminal.to_owned(), false), (framed, true)] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            clipboard.borrow_mut().text = text;
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            assert_ne!(app.doc().unwrap().doc.workbook(), &before);
            app.toasts.tick(60.0);
            let notice = app
                .toasts
                .texts()
                .find(|text| {
                    text.contains(crate::i18n::t(
                        "Connector attachments may have been detached",
                    ))
                })
                .expect("detached connector warning should stay visible");
            assert_eq!(
                notice.contains(crate::i18n::t("Container frames omitted")),
                has_frame
            );
            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_ignores_detachments_on_omitted_pages() {
        use crate::app::Cmd;

        let later = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
            r#"<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="sourcePoint"/></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let file = format!(
            "<mxfile><diagram name=\"First\">{DRAWIO_MODEL}</diagram><diagram name=\"Later\">{later}</diagram></mxfile>"
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let items = board_item_count(&app);
        clipboard.borrow_mut().text = file;
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 3);
        app.toasts.tick(60.0);
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains(crate::i18n::t("Additional draw.io pages omitted")))
        );
        assert!(!app.toasts.texts().any(|text| {
            text.contains(crate::i18n::t(
                "Connector attachments may have been detached",
            ))
        }));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_unsupported_shapes_only_on_the_first_page() {
        use crate::app::Cmd;

        let chair = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="chair" value="Chair" style="shape=mxgraph.floorplan.office_chair;" vertex="1" parent="1"><mxGeometry width="40" height="43" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let later = format!(
            "<mxfile><diagram name=\"First\">{DRAWIO_MODEL}</diagram><diagram name=\"Later\">{chair}</diagram></mxfile>"
        );
        let first = format!(
            "<mxfile><diagram name=\"First\">{chair}</diagram><diagram name=\"Later\">{DRAWIO_MODEL}</diagram></mxfile>"
        );
        for (text, expected_items, omitted_pages, unsupported_shapes) in [
            (chair.to_owned(), 1, false, true),
            (first, 1, true, true),
            (later, 3, true, false),
        ] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            let items = board_item_count(&app);
            clipboard.borrow_mut().text = text;
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            assert_eq!(board_item_count(&app), items + expected_items);
            app.toasts.tick(60.0);
            assert_eq!(
                app.toasts.texts().any(|text| {
                    text.contains(crate::i18n::t(
                        "Unsupported shapes or styles in the draw.io source may appear as rectangles",
                    ))
                }),
                unsupported_shapes
            );
            assert_eq!(
                app.toasts
                    .texts()
                    .any(|text| text.contains(crate::i18n::t("Additional draw.io pages omitted"))),
                omitted_pages
            );
            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_warns_when_a_malformed_endpoint_omits_a_connector() {
        use crate::app::Cmd;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1"><mxGeometry x="100" width="100" height="60" as="geometry"/></mxCell>"#,
            r#"<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="invalid" y="8" as="sourcePoint"/></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let items = board_item_count(&app);
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 1);
        app.toasts.tick(60.0);
        assert!(app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
        assert!(!app.toasts.texts().any(|text| text.contains(crate::i18n::t(
            "Connector attachments may have been detached"
        ))));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_preserves_connector_after_omitting_unrepresentable_waypoint() {
        use crate::app::Cmd;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="edge" edge="1" parent="1" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/><mxPoint x="100" y="0" as="targetPoint"/><Array as="points"><mxPoint x="50" y="20"/><mxPoint x="1000001" y="5"/></Array></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let items = board_item_count(&app);
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 1);
        let object = &app.doc().unwrap().doc.sheet().canvas.objects[0];
        let thinoda_core::CanvasObjectKind::Connector(connector) = &object.kind else {
            panic!("expected a connector");
        };
        assert_eq!(connector.waypoints.len(), 1);
        app.toasts.tick(60.0);
        assert!(app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_unresolved_shape_appearance() {
        use crate::app::Cmd;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r##"<mxCell id="shape" vertex="1" parent="1" value="Theme" style="shape=rectangle;fillColor=light-dark(#ffffff,#111111);strokeColor=inherit;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        let items = board_item_count(&app);
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 1);
        assert!(app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_preserves_double_ellipse_and_undoes_paste() {
        use crate::app::Cmd;
        use thinoda_core::{CanvasObjectKind, ShapeKind};

        let text = concat!(
            r#"<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r##"<mxCell id="shape" vertex="1" parent="1" value="End" style="shape=doubleEllipse;fillColor=#ffffcc;strokeColor=#123456;fontColor=#000000;margin=8.5;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        let object = app
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .first()
            .unwrap();
        assert!(matches!(
            object.kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert_eq!(object.label(), "End");
        assert_eq!(object.double_ellipse_margin, Some(8.5));
        assert!(!app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));

        app.exec(&ctx, Cmd::CopyAsDrawio);
        let copied = clipboard.borrow().text.clone();
        let restored = thinoda_io::drawio_clipboard::decode(&copied).unwrap();
        let mut sheet = Sheet::new("Restored", "");
        sheet.root_mode = SheetRootMode::Hidden;
        clipboard::paste_board_selection(&mut sheet, &restored.payload, Vec2::ZERO).unwrap();
        assert_eq!(sheet.canvas.objects[0].double_ellipse_margin, Some(8.5));
        assert!(matches!(
            sheet.canvas.objects[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert!(app.toasts.texts().any(|text| text.contains(crate::i18n::t(
            "Double-ellipse appearance may differ in draw.io"
        ))));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_folded_note_paste_and_copy_disclose_appearance_changes() {
        use crate::app::Cmd;
        use thinoda_core::CardKind;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="note" vertex="1" parent="1" value="Note" style="shape=note2;boundedLbl=1;size=15;"><mxGeometry width="180" height="120" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        let doc = app.doc().unwrap();
        let note = doc.doc.sheet().topic(doc.view.selection[0]).unwrap();
        assert_eq!(note.title, "Note");
        assert_eq!(note.card.unwrap().kind, CardKind::Sticky);
        assert!(app.toasts.texts().any(|text| text.contains(crate::i18n::t(
            "draw.io folded notes may be flattened or simplified"
        ))));

        app.exec(&ctx, Cmd::CopyAsDrawio);
        let copied = clipboard.borrow().text.clone();
        let pasted = thinoda_io::drawio_clipboard::decode(&copied).unwrap();
        assert!(
            pasted
                .warnings
                .contains(&thinoda_io::ImportWarning::DrawioFoldedNotesApproximated)
        );
        assert!(
            app.toasts
                .texts()
                .any(|text| text
                    .contains(crate::i18n::t("Cards may show folded corners in draw.io")))
        );

        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_paste_reports_only_first_page_appearance_warnings() {
        use crate::app::Cmd;

        let plain = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1" value="Rectangle" style="shape=rectangle;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let changed = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="note" vertex="1" parent="1" value="Note" style="shape=note2;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
            r#"<mxCell id="shape" vertex="1" parent="1" value="Unknown" style="shape=unknown;"><mxGeometry x="120" width="100" height="60" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        for (first, later, changed_is_pasted) in [(plain, changed, false), (changed, plain, true)] {
            let file = format!(
                "<mxfile><diagram name=\"First\">{first}</diagram><diagram name=\"Later\">{later}</diagram></mxfile>"
            );
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            clipboard.borrow_mut().text = file;
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            let messages: Vec<_> = app.toasts.texts().collect();
            assert!(
                messages
                    .iter()
                    .any(|text| text.contains(crate::i18n::t("Additional draw.io pages omitted")))
            );
            for warning in [
                "draw.io folded notes may be flattened or simplified",
                "Unsupported shapes or styles in the draw.io source may appear as rectangles",
            ] {
                assert_eq!(
                    messages
                        .iter()
                        .any(|text| text.contains(crate::i18n::t(warning))),
                    changed_is_pasted,
                    "{warning}"
                );
            }
            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_process_approximation_and_copies_square_geometry() {
        use crate::app::Cmd;
        use thinoda_core::{CanvasObjectKind, ShapeKind};

        let text = concat!(
            r#"<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1" value="Task" style="shape=mxgraph.flowchart.process;"><mxGeometry width="300" height="200" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        assert!(matches!(
            app.doc().unwrap().doc.sheet().canvas.objects[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::Process,
                ..
            }
        ));
        assert!(app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));

        app.exec(&ctx, Cmd::CopyAsDrawio);
        let copied = clipboard.borrow().text.clone();
        assert!(copied.contains("shape%3Drectangle%3Brounded%3D0%3BthinodaShape%3Dprocess%3B"));
        let pasted = thinoda_io::drawio_clipboard::decode(&copied).unwrap();
        assert!(pasted.warnings.is_empty(), "{:?}", pasted.warnings);
        let mut restored = Sheet::new("Restored", "");
        restored.root_mode = SheetRootMode::Hidden;
        clipboard::paste_board_selection(&mut restored, &pasted.payload, Vec2::ZERO).unwrap();
        assert!(matches!(
            restored.canvas.objects[0].kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::Process,
                ..
            }
        ));

        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_bracket_is_editable_and_undoable() {
        use crate::app::Cmd;
        use thinoda_core::{Color, RectangleSides};

        let text = concat!(
            r#"<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r##"<mxCell id="shape" vertex="1" parent="1" style="shape=mxgraph.flowchart.annotation_1;fillColor=#ff0000;strokeColor=#336699;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"##,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        let object = &app.doc().unwrap().doc.sheet().canvas.objects[0];
        let id = object.id;
        let sides = RectangleSides {
            top: true,
            right: false,
            bottom: true,
            left: true,
        };
        assert_eq!(object.rectangle_sides, Some(sides));
        assert_eq!(object.style.fill, Some(Color::TRANSPARENT));
        assert_eq!(object.style.stroke, Some(Color::rgb(51, 102, 153)));
        assert!(app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
        let pasted = app.doc().unwrap().doc.workbook().clone();
        app.doc_mut()
            .unwrap()
            .doc
            .update_canvas_object("Outline sides", id, |object| {
                object.rectangle_sides.as_mut().unwrap().right = true;
            })
            .unwrap();
        assert!(
            app.doc()
                .unwrap()
                .doc
                .sheet()
                .canvas
                .object(id)
                .unwrap()
                .rectangle_sides
                .unwrap()
                .right
        );
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &pasted);
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_preserves_css_colors_and_undoes_paste() {
        use crate::app::Cmd;
        use thinoda_core::Color;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="shape" vertex="1" parent="1" value="Color" style="shape=rectangle;fillColor=rgba(10,20,30,0.5);fillOpacity=50;strokeColor=blue;fontColor=green;"><mxGeometry width="100" height="60" as="geometry"/></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        let sheet = app.doc().unwrap().doc.sheet();
        let object = sheet.canvas.objects.first().unwrap();
        assert_eq!(object.style.fill, Some(Color::rgba(10, 20, 30, 64)));
        assert_eq!(object.style.stroke, Some(Color::rgb(0, 0, 255)));
        assert_eq!(object.style.text_color, Some(Color::rgb(0, 128, 0)));
        assert!(!app.toasts.texts().any(|text| text
            == crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            )));
        app.exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_rejects_an_unpasteable_malformed_connector() {
        use crate::app::Cmd;

        let text = concat!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>"#,
            r#"<mxCell id="edge" edge="1" parent="1" style="endArrow=none;"><mxGeometry relative="1" as="geometry"><mxPoint x="invalid" y="8" as="sourcePoint"/><mxPoint x="100" y="8" as="targetPoint"/></mxGeometry></mxCell>"#,
            r#"</root></mxGraphModel>"#
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = text.into();
        app.exec(&egui::Context::default(), Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(
            app.toasts
                .texts()
                .any(|text| text
                    .contains(crate::i18n::t("The draw.io page has no pasteable items.")))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_malformed_shape_geometry_and_preserves_undo() {
        use crate::app::Cmd;

        let bad = r#"<mxCell id="bad" value="Bad" vertex="1" parent="1"><mxGeometry x="invalid" width="40" height="50" as="geometry"/></mxCell>"#;
        let safe = r#"<mxCell id="safe" value="Safe" vertex="1" parent="1"><mxGeometry x="100" width="40" height="50" as="geometry"/></mxCell>"#;
        for include_safe in [false, true] {
            let text = format!(
                r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{bad}{}</root></mxGraphModel>"#,
                if include_safe { safe } else { "" }
            );
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            let items = board_item_count(&app);
            clipboard.borrow_mut().text = text;
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            if include_safe {
                assert_eq!(board_item_count(&app), items + 1);
                app.toasts.tick(60.0);
                assert!(app.toasts.texts().any(|text| text
                    == crate::i18n::t(
                        "Pasted from draw.io; some content may have been omitted or simplified"
                    )));
                app.exec(&ctx, Cmd::Canvas(Action::Undo));
            } else {
                assert!(app.toasts.texts().any(|text| {
                    text.contains(crate::i18n::t("The draw.io page has no pasteable items."))
                }));
            }
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_explains_when_the_first_page_has_no_pasteable_items() {
        use crate::app::Cmd;

        let empty = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#;
        let file = format!(
            "<mxfile><diagram name=\"Empty\">{empty}</diagram><diagram name=\"Later\">{DRAWIO_MODEL}</diagram></mxfile>"
        );
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = file;
        app.exec(&egui::Context::default(), Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.toasts.texts().any(|text| text.contains(crate::i18n::t(
            "The first draw.io page has no pasteable items; additional pages were not pasted."
        ))));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_a_frame_only_page_as_unpasteable() {
        use crate::app::Cmd;

        let frame_only = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="frame" value="Frame" style="swimlane;" vertex="1" parent="1"><mxGeometry width="240" height="120" as="geometry"/></mxCell></root></mxGraphModel>"#;
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        clipboard.borrow_mut().text = frame_only.to_owned();
        app.exec(&egui::Context::default(), Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(
            app.toasts
                .texts()
                .any(|text| text
                    .contains(crate::i18n::t("The draw.io page has no pasteable items.")))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_reports_marker_and_port_approximation() {
        use crate::app::Cmd;
        for style in ["endArrow=cross", "endArrow=classic;exitX=0.25"] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            let items = board_item_count(&app);
            clipboard.borrow_mut().text = DRAWIO_MODEL.replace("endArrow=classic", style);
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            assert_eq!(board_item_count(&app), items + 3, "{style}");
            assert!(
                app.toasts.texts().any(|text| text.contains(crate::i18n::t(
                    "Pasted from draw.io; some content may have been omitted or simplified"
                ))),
                "{style}"
            );
            app.toasts.tick(60.0);
            assert!(!app.toasts.texts().any(|text| text.contains(crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            ))));
            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn drawio_clipboard_preserves_supported_connector_markers_without_warning() {
        use crate::app::Cmd;
        use thinoda_core::{CanvasObjectKind, ConnectorMarkerKind};

        for (style, start, end, start_size, end_size, start_unfilled, end_unfilled) in [
            (
                "startArrow=block;endArrow=open",
                ConnectorMarkerKind::Block,
                ConnectorMarkerKind::Open,
                None,
                None,
                false,
                false,
            ),
            (
                "startArrow=classicThin;endArrow=openThin",
                ConnectorMarkerKind::ClassicThin,
                ConnectorMarkerKind::OpenThin,
                None,
                None,
                false,
                false,
            ),
            (
                "startArrow=classic;startSize=12.5;endArrow=open;endSize=0",
                ConnectorMarkerKind::Classic,
                ConnectorMarkerKind::Open,
                Some(12.5),
                Some(0.0),
                false,
                false,
            ),
            (
                "startArrow=block;startFill=0;endArrow=classic;endFill=1",
                ConnectorMarkerKind::Block,
                ConnectorMarkerKind::Classic,
                None,
                None,
                true,
                false,
            ),
            (
                "startArrow=classic;startFill=1;endArrow=open;endFill=0",
                ConnectorMarkerKind::Classic,
                ConnectorMarkerKind::Open,
                None,
                None,
                false,
                true,
            ),
            (
                "startArrow=diamond;startSize=12.5;startFill=0;endArrow=diamondThin;endSize=8",
                ConnectorMarkerKind::Diamond,
                ConnectorMarkerKind::DiamondThin,
                Some(12.5),
                Some(8.0),
                true,
                false,
            ),
            (
                "startArrow=oval;startSize=12.5;startFill=0;endArrow=oval;endSize=8",
                ConnectorMarkerKind::Oval,
                ConnectorMarkerKind::Oval,
                Some(12.5),
                Some(8.0),
                true,
                false,
            ),
            (
                "startArrow=halfCircle;startSize=12.5;startFill=0;endArrow=oval;endSize=8",
                ConnectorMarkerKind::HalfCircle,
                ConnectorMarkerKind::Oval,
                Some(12.5),
                Some(8.0),
                true,
                false,
            ),
            (
                "startArrow=baseDash;startSize=12.5;startFill=0;endArrow=dash;endSize=0",
                ConnectorMarkerKind::BaseDash,
                ConnectorMarkerKind::Dash,
                Some(12.5),
                Some(0.0),
                true,
                false,
            ),
        ] {
            let (mut app, clipboard) = empty_board_app();
            let before = app.doc().unwrap().doc.workbook().clone();
            clipboard.borrow_mut().text = DRAWIO_MODEL.replace("endArrow=classic", style);
            let ctx = egui::Context::default();
            app.exec(&ctx, Cmd::MenuPaste);
            let sheet = app.doc().unwrap().doc.sheet();
            let connector = sheet
                .canvas
                .objects
                .iter()
                .find_map(|object| match &object.kind {
                    CanvasObjectKind::Connector(connector) => Some(connector),
                    _ => None,
                })
                .unwrap();
            assert_eq!(connector.start_marker.unwrap().kind, start);
            assert_eq!(connector.end_marker.unwrap().kind, end);
            assert_eq!(connector.start_marker_size, start_size);
            assert_eq!(connector.end_marker_size, end_size);
            assert_eq!(connector.start_marker_unfilled, start_unfilled);
            assert_eq!(connector.end_marker_unfilled, end_unfilled);
            assert!(!app.toasts.texts().any(|text| text.contains(crate::i18n::t(
                "Pasted from draw.io; some content may have been omitted or simplified"
            ))));
            app.exec(&ctx, Cmd::Canvas(Action::Undo));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn published_drawio_links_remain_plain_text_when_pasted_normally() {
        use crate::app::Cmd;
        let (mut app, clipboard) = app_with_fake_clipboard();
        let link = "https://viewer.diagrams.net/#R%3Cmxfile%3Eexample%3C%2Fmxfile%3E";
        assert!(!thinoda_io::drawio_clipboard::looks_like_drawio(link));
        clipboard.borrow_mut().text = link.into();
        let objects = app.doc().unwrap().doc.sheet().canvas.objects.len();

        app.exec(&egui::Context::default(), Cmd::MenuPaste);

        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.sheet().canvas.objects.len(), objects);
        assert_eq!(doc.view.selection.len(), 1);
        assert_eq!(
            doc.doc.sheet().topic(doc.view.selection[0]).unwrap().title,
            link
        );
        assert!(app.dialogs.drawio_link.is_none());
        assert!(app.dialogs.imports.is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_as_drawio_writes_plain_encoded_model_that_pastes_back() {
        use crate::app::Cmd;
        let (mut app, clipboard) = empty_board_app();
        {
            let mut fake = clipboard.borrow_mut();
            fake.text = DRAWIO_MODEL.to_owned();
            fake.html = None;
        }
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::MenuPaste);
        let items = board_item_count(&app);

        app.exec(&ctx, Cmd::CopyAsDrawio);
        let text = clipboard.borrow().text.clone();
        assert!(text.starts_with("%3CmxGraphModel"), "{text}");
        assert!(clipboard.borrow().html.is_none());
        assert_eq!(board_item_count(&app), items);

        app.exec(&ctx, Cmd::MenuPaste);
        assert_eq!(board_item_count(&app), items + 3);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_as_drawio_warns_and_preserves_partial_rectangle_sides() {
        use crate::app::Cmd;
        use thinoda_core::{Color, RectangleSides};

        let (mut app, clipboard) = empty_board_app();
        let doc = app.doc_mut().unwrap();
        let sides = RectangleSides {
            top: true,
            right: false,
            bottom: true,
            left: false,
        };
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(120.0, 80.0)),
        );
        shape.rectangle_sides = Some(sides);
        shape.style.fill = Some(Color::rgb(40, 100, 160));
        let id = doc.doc.add_canvas_object(shape).unwrap();
        doc.view.board_selection = vec![id];
        let before = doc.doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::CopyAsDrawio);
        let text = clipboard.borrow().text.clone();
        assert!(text.starts_with("%3CmxGraphModel"));
        let pasted = thinoda_io::drawio_clipboard::decode(&text).unwrap();
        let mut restored = Sheet::new("Restored", "");
        restored.root_mode = SheetRootMode::Hidden;
        clipboard::paste_board_selection(&mut restored, &pasted.payload, Vec2::ZERO).unwrap();
        assert_eq!(restored.canvas.objects[0].rectangle_sides, Some(sides));

        let expected: Vec<_> = crate::i18n::Lang::ALL
            .into_iter()
            .map(|language| {
                format!(
                    "1 — {}",
                    crate::i18n::t_in(
                        language,
                        "Rectangle colors or automatic outlines may differ in draw.io"
                    )
                )
            })
            .collect();
        let has_warning = |app: &crate::app::ThinodaApp| {
            app.toasts
                .texts()
                .any(|text| expected.iter().any(|line| text.contains(line)))
        };
        assert!(has_warning(&app));
        app.toasts.tick(10.0);
        assert!(has_warning(&app));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_as_drawio_warns_about_collapsed_process_outlines() {
        use crate::app::Cmd;

        let (mut app, clipboard) = empty_board_app();
        let doc = app.doc_mut().unwrap();
        let id = doc
            .doc
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Process,
                Rect::from_min_size(Vec2::ZERO, Vec2::new(0.0, 100.0)),
            ))
            .unwrap();
        doc.view.board_selection = vec![id];
        let before = doc.doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::CopyAsDrawio);
        assert!(clipboard.borrow().text.starts_with("%3CmxGraphModel"));
        let expected: Vec<_> = crate::i18n::Lang::ALL
            .into_iter()
            .map(|language| {
                format!(
                    "1 — {}",
                    crate::i18n::t_in(
                        language,
                        "Collapsed process outlines may be omitted in draw.io"
                    )
                )
            })
            .collect();
        let has_warning = |app: &crate::app::ThinodaApp| {
            app.toasts
                .texts()
                .any(|text| expected.iter().any(|line| text.contains(line)))
        };
        assert!(has_warning(&app));
        app.toasts.tick(10.0);
        assert!(has_warning(&app));
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_as_drawio_warns_when_waypoint_connector_attachments_detach() {
        use crate::app::Cmd;
        use thinoda_core::{CanvasConnector, Endpoint, SheetRootMode};

        for include_target in [false, true] {
            let (mut app, clipboard) = empty_board_app();
            let doc = app.doc_mut().unwrap();
            let mut target = CanvasConnector::new(
                Endpoint::Point(Vec2::ZERO),
                Endpoint::Point(Vec2::new(250.0, 100.0)),
            );
            target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
            let target = doc
                .doc
                .add_canvas_object(CanvasObject::connector(target))
                .unwrap();
            let mut dependent = CanvasConnector::new(
                Endpoint::Object(target),
                Endpoint::Point(Vec2::new(300.0, 200.0)),
            );
            dependent.label = "Dependent".into();
            let dependent = doc
                .doc
                .add_canvas_object(CanvasObject::connector(dependent))
                .unwrap();
            doc.view.board_selection = if include_target {
                vec![target, dependent]
            } else {
                vec![dependent]
            };
            let before = doc.doc.workbook().clone();

            app.exec(&egui::Context::default(), Cmd::CopyAsDrawio);
            let text = clipboard.borrow().text.clone();
            assert!(text.starts_with("%3CmxGraphModel"));
            let pasted = thinoda_io::drawio_clipboard::decode(&text).unwrap();
            let mut restored = thinoda_core::Sheet::new("Restored", "");
            restored.root_mode = SheetRootMode::Hidden;
            clipboard::paste_board_selection(&mut restored, &pasted.payload, Vec2::ZERO).unwrap();
            let dependent = restored
                .canvas
                .objects
                .iter()
                .find_map(|object| match &object.kind {
                    CanvasObjectKind::Connector(connector) if connector.label == "Dependent" => {
                        Some(connector)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(dependent.from, Endpoint::Point(Vec2::new(175.0, 0.0)));
            let expected: Vec<_> = crate::i18n::Lang::ALL
                .into_iter()
                .map(|language| {
                    format!(
                        "1 — {}",
                        crate::i18n::t_in(language, "Connector-to-connector attachments detached")
                    )
                })
                .collect();
            let has_warning = |app: &crate::app::ThinodaApp| {
                app.toasts
                    .texts()
                    .any(|text| expected.iter().any(|line| text.contains(line)))
            };
            assert!(has_warning(&app));
            app.toasts.tick(10.0);
            assert!(has_warning(&app));
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn copy_as_drawio_counts_selected_and_external_connector_attachments_together() {
        use thinoda_core::CanvasConnector;

        let (mut app, _) = empty_board_app();
        let doc = app.doc_mut().unwrap();
        let make_target = |start: f32| {
            CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(start, 0.0)),
                Endpoint::Point(Vec2::new(start + 100.0, 0.0)),
            ))
        };
        let selected = doc.doc.add_canvas_object(make_target(0.0)).unwrap();
        let external = doc.doc.add_canvas_object(make_target(200.0)).unwrap();
        let dependent = doc
            .doc
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Object(selected),
                Endpoint::Object(external),
            )))
            .unwrap();
        doc.view.board_selection = vec![selected, dependent];
        let copy = prepare_drawio_copy(doc).unwrap().unwrap();
        assert_eq!(
            copy.losses,
            vec![
                thinoda_io::ExportLoss::ConnectorAttachmentsDetached(2),
                thinoda_io::ExportLoss::LegacyConnectorMarkersApproximated(2),
            ]
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn malformed_drawio_text_reports_an_error_without_editing() {
        use crate::app::Cmd;
        let (mut app, clipboard) = empty_board_app();
        let before = app.doc().unwrap().doc.workbook().clone();
        {
            let mut fake = clipboard.borrow_mut();
            fake.text = "<mxGraphModel><root><mxCell".into();
            fake.html = None;
        }
        app.exec(&egui::Context::default(), Cmd::MenuPaste);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains(crate::i18n::t("Could not paste draw.io content")))
        );
    }
}

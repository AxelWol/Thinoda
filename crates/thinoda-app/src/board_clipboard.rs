//! Portable clipboard markup shared by the desktop and browser transports.
//!
//! The readable text travels in the clipboard's `text/plain` representation.
//! A separate `text/html` representation carries this app's opaque board data
//! or a session-only topic token. The marker is never inferred from plain text.

use thinoda_canvas::{Action, ObjectSelection, apply_action};
use thinoda_core::{
    CanvasObjectId, TopicId,
    clipboard::{self, ClipboardPayload, MAX_BOARD_CLIPBOARD_BYTES},
};
use web_time::Instant;

use crate::doc::OpenDoc;

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
// Hex encoding takes two bytes per board byte; HTML escaping of visible text
// takes at most five bytes per text byte. Leave room for the wrapper.
const MAX_HTML_BYTES: usize = 7 * MAX_BOARD_CLIPBOARD_BYTES + 1024;
const MAX_PENDING_CUT: std::time::Duration = std::time::Duration::from_secs(30);
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RichClipboard {
    /// Validated again by the core board clipboard decoder when pasted.
    Board(String),
    /// Unpredictable identifier for a legacy topic payload held by this app.
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
    pub(crate) topic: Option<ClipboardPayload>,
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
    if snapshot.text != prepared.text
        && snapshot.text.replace("\r\n", "\n") != prepared.text.replace("\r\n", "\n")
    {
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
}

fn localize_error(error: thinoda_core::EditError) -> String {
    let message = error.to_string();
    crate::i18n::localize_model_error(&message).to_owned()
}

#[derive(Default)]
pub(crate) struct ClipboardBridge {
    topic: Option<(String, ClipboardPayload)>,
    generation: u64,
    #[cfg(not(target_arch = "wasm32"))]
    transport: native::Transport,
    #[cfg(target_arch = "wasm32")]
    transport: web::Transport,
}

impl ClipboardBridge {
    fn write(&mut self, ctx: &egui::Context, prepared: PreparedCopy) -> Option<Completion> {
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
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

    fn accept_write(&mut self, generation: u64, prepared: &PreparedCopy) -> bool {
        if generation != self.generation {
            return false;
        }
        self.topic = match (&prepared.rich, &prepared.topic) {
            (RichClipboard::TopicToken(token), Some(payload)) => {
                Some((token.clone(), payload.clone()))
            }
            _ => None,
        };
        true
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
    let (rich, topic) = if let Some(board) = doc.view.board_clipboard() {
        (
            RichClipboard::Board(clipboard::encode_board_payload(board)?),
            None,
        )
    } else {
        let payload = doc
            .view
            .clipboard
            .clone()
            .ok_or("copy did not produce clipboard content")?;
        (
            RichClipboard::TopicToken(uuid::Uuid::new_v4().simple().to_string()),
            Some(payload),
        )
    };
    let html = make_html(&text, &rich)?;
    Ok(Some(PreparedCopy {
        text,
        html,
        rich,
        topic,
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
    topic: Option<(&str, &ClipboardPayload)>,
) -> Result<bool, String> {
    match rich {
        Some(RichClipboard::Board(encoded)) => doc
            .view
            .paste_encoded_board(&mut doc.doc, &encoded)
            .map_err(localize_error),
        Some(RichClipboard::TopicToken(token)) => {
            doc.view.invalidate_clipboard();
            if let Some((_, payload)) = topic.filter(|(owned, _)| *owned == token) {
                doc.view.clipboard = Some(payload.clone());
                apply_action(&mut doc.doc, &mut doc.view, Action::Paste(None))
                    .map_err(localize_error)
            } else {
                // This token came from another app session (or expired).
                apply_action(&mut doc.doc, &mut doc.view, Action::Paste(Some(text)))
                    .map_err(localize_error)
            }
        }
        None => apply_action(&mut doc.doc, &mut doc.view, Action::Paste(Some(text)))
            .map_err(localize_error),
    }
}

impl crate::app::ThinodaApp {
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
                        self.toasts.error(format!("Could not copy: {error}"));
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
            Completion::Write {
                generation,
                prepared,
                result,
            } => {
                if let Err(error) = result {
                    let message = if prepared.cut.is_some() {
                        crate::i18n::t("Cut cancelled; original content was kept")
                    } else {
                        "Could not copy"
                    };
                    self.toasts.error(format!("{message}: {error}"));
                    return;
                }
                if !self.clipboard_bridge.accept_write(generation, &prepared) {
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
                        self.toasts
                            .error(format!("Could not paste clipboard data: {error}"));
                        return;
                    }
                };
                // Cached view payloads never prove OS ownership, including in
                // other tabs. A topic token is accepted only from actual HTML.
                for doc in &mut self.docs {
                    doc.view.invalidate_clipboard();
                }
                let topic = self
                    .clipboard_bridge
                    .topic
                    .as_ref()
                    .map(|(token, payload)| (token.as_str(), payload));
                let Some(doc) = self.docs.get_mut(self.active) else {
                    return;
                };
                if let Err(error) = paste(doc, snapshot.text, rich, topic) {
                    self.toasts
                        .error(format!("Could not paste clipboard data: {error}"));
                }
            }
        }
    }
}

/// Creates safe HTML markup for a clipboard write after bounding both parts.
pub(crate) fn make_html(text: &str, rich: &RichClipboard) -> Result<String, String> {
    let (attribute, payload) = match rich {
        RichClipboard::Board(value) => (BOARD_ATTRIBUTE, value),
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
                    return Err("invalid clipboard HTML attribute".into());
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
        let is_board = name.eq_ignore_ascii_case(BOARD_ATTRIBUTE);
        let is_topic = name.eq_ignore_ascii_case(TOPIC_ATTRIBUTE);
        if !is_board && !is_topic {
            continue;
        }
        let decoded = decode_hex(value.ok_or("invalid Thinoda clipboard marker")?)?;
        let marker = if is_board {
            RichClipboard::Board(decoded)
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
                None
            )
            .is_err()
        );
        assert_eq!(doc.doc.workbook(), &before);
        assert!(paste(&mut doc, prepared.text, Some(rich), None).unwrap());
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
            topic: None,
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
        assert!(paste(&mut target, prepared.text, rich, None).unwrap());
        assert_eq!(target.doc.sheet().canvas.objects.len(), 1);
        assert_eq!(target.doc.sheet().canvas.objects[0].label(), "Same text");
        assert!(target.doc.undo());
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
        assert!(paste(&mut source, prepared.text, None, None).unwrap());
        assert_eq!(source.doc.sheet().canvas.objects.len(), 1);
        assert!(source.view.board_clipboard().is_none());
        let pasted = source.doc.sheet().topic(source.view.selection[0]).unwrap();
        assert_eq!(pasted.title, "Same text");
    }

    #[test]
    fn session_token_is_required_for_legacy_topic_fidelity() {
        let mut sheet = Sheet::new("Map", "Root");
        let root = sheet.root;
        let child = sheet.add_child(root, None, "Same text");
        sheet.topic_mut(child).unwrap().style.bold = Some(true);
        let mut source = OpenDoc::new(Workbook::new(sheet));
        source.view.select_only(child);
        let prepared = prepare_copy(&mut source, false).unwrap().unwrap();
        let RichClipboard::TopicToken(token) = prepared.rich.clone() else {
            panic!("expected topic token");
        };
        let payload = prepared.topic.as_ref().unwrap();
        let mut target = OpenDoc::new(Workbook::new(Sheet::new("Other", "Root")));
        assert!(
            paste(
                &mut target,
                prepared.text.clone(),
                Some(prepared.rich),
                Some((&token, payload)),
            )
            .unwrap()
        );
        assert_eq!(
            target
                .doc
                .sheet()
                .topic(target.view.selection[0])
                .unwrap()
                .style
                .bold,
            Some(true)
        );
        assert!(paste(&mut target, prepared.text, None, Some((&token, payload))).unwrap());
        assert_eq!(
            target
                .doc
                .sheet()
                .topic(target.view.selection[0])
                .unwrap()
                .style
                .bold,
            None
        );
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
    fn native_topic_cut_verifies_session_token_before_deletion() {
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
            Some(RichClipboard::TopicToken(_))
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
}

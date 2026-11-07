//! Clipboard interop with draw.io (diagrams.net).
//!
//! draw.io copies a selection as `encodeURIComponent(<mxGraphModel>…)` text and
//! accepts that form, plain `mxGraphModel` XML, or a whole `mxfile` when
//! pasting. These helpers convert between that text and Thinoda's validated
//! board clipboard payload, so pasting reuses the ordinary undoable board paste.

use crate::{ExportLoss, ExportOptions, Format, ImportWarning, IoError, drawio, export_losses};
use thinoda_core::{
    ApproxMeasure, Rect, Sheet, SheetRootMode, Vec2, Workbook,
    clipboard::{
        BoardClipboardPayload, MAX_BOARD_CLIPBOARD_BYTES, copy_board_selection,
        paste_board_selection,
    },
    layout_sheet,
    scene::canvas_bounds,
};

/// Largest clipboard text accepted or produced, matching the board clipboard.
pub const MAX_TEXT_BYTES: usize = MAX_BOARD_CLIPBOARD_BYTES;

/// Board content decoded from draw.io clipboard text.
#[derive(Debug)]
pub struct DrawioPaste {
    pub payload: BoardClipboardPayload,
    /// Visible sheet-space bounds of the content, used to place the paste.
    pub bounds: Option<Rect>,
    pub warnings: Vec<ImportWarning>,
}

/// draw.io clipboard text and the changes made while converting the selection.
#[derive(Debug)]
pub struct DrawioCopy {
    pub text: String,
    pub losses: Vec<ExportLoss>,
}

/// `true` when clipboard text looks like draw.io diagram data.
///
/// This is a cheap prefix check; [`decode`] performs full validation.
pub fn looks_like_drawio(text: &str) -> bool {
    let text = text.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    let starts = |prefix: &str| {
        text.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };
    ["<mxGraphModel", "<mxfile", "%3CmxGraphModel", "%3Cmxfile"]
        .iter()
        .any(|prefix| starts(prefix))
}

/// Converts draw.io clipboard text into a board payload (first page only).
pub fn decode(text: &str) -> Result<DrawioPaste, IoError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(IoError::Invalid(
            "draw.io clipboard data exceeds the size limit".into(),
        ));
    }
    if !looks_like_drawio(text) {
        return Err(IoError::Invalid(
            "clipboard text is not draw.io data".into(),
        ));
    }
    let trimmed = text.trim();
    let xml = if trimmed.starts_with('%') {
        drawio::percent_decode(trimmed)
            .ok_or_else(|| IoError::Invalid("draw.io clipboard data is not valid".into()))?
    } else {
        trimmed.to_owned()
    };
    let report = drawio::import(xml.as_bytes())?;
    let sheet = report
        .workbook
        .sheets
        .into_iter()
        .next()
        .ok_or_else(|| IoError::Invalid("draw.io clipboard data contains no pages".into()))?;
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let topics: Vec<_> = sheet
        .floating
        .iter()
        .copied()
        .filter(|id| !sheet.is_anchor(*id))
        .collect();
    let objects: Vec<_> = sheet
        .canvas
        .objects
        .iter()
        .map(|object| object.id)
        .collect();
    let payload = copy_board_selection(&sheet, &layout, &topics, &objects)
        .map_err(IoError::Invalid)?
        .ok_or_else(|| IoError::Invalid("draw.io clipboard data contains no shapes".into()))?;
    let bounds = layout
        .order
        .iter()
        .filter(|id| !sheet.is_anchor(**id))
        .filter_map(|id| layout.topics.get(id).map(|topic| topic.rect))
        .chain(canvas_bounds(&sheet, &layout))
        .reduce(|a, b| a.union(&b));
    Ok(DrawioPaste {
        payload,
        bounds,
        warnings: report.warnings,
    })
}

/// Encodes a board payload as draw.io clipboard text (`encodeURIComponent` of
/// an `mxGraphModel`), which draw.io pastes as editable cells. Refuses a lossy
/// conversion of the prepared fragment; use [`encode_with_report`] to handle
/// losses explicitly. The caller must report any losses in preparing it.
pub fn encode(payload: &BoardClipboardPayload) -> Result<String, IoError> {
    let copy = encode_with_report(payload)?;
    if !copy.losses.is_empty() {
        return Err(IoError::DrawioClipboardLoss(copy.losses));
    }
    Ok(copy.text)
}

/// Encodes an already-prepared fragment and reports conversion losses in it.
/// Losses incurred while preparing the fragment must be reported by the caller.
pub fn encode_with_report(payload: &BoardClipboardPayload) -> Result<DrawioCopy, IoError> {
    let mut sheet = Sheet::new("Page-1", "");
    sheet.root_mode = SheetRootMode::Hidden;
    paste_board_selection(&mut sheet, payload, Vec2::ZERO).map_err(IoError::Invalid)?;
    let book = Workbook::new(sheet);
    let losses = export_losses(&book, Format::DrawIo, &ExportOptions::default());
    let file = drawio::export(&book)?;
    let file = String::from_utf8(file)
        .map_err(|_| IoError::Invalid("draw.io export is not UTF-8".into()))?;
    let start = file
        .find("<mxGraphModel")
        .ok_or_else(|| IoError::Invalid("draw.io export has no graph model".into()))?;
    let end = file
        .rfind("</mxGraphModel>")
        .map(|end| end + "</mxGraphModel>".len())
        .ok_or_else(|| IoError::Invalid("draw.io export has no graph model".into()))?;
    let text = encode_uri_component(&file[start..end]);
    if text.len() > MAX_TEXT_BYTES {
        return Err(IoError::Invalid(
            "draw.io clipboard data exceeds the size limit".into(),
        ));
    }
    Ok(DrawioCopy { text, losses })
}

/// JavaScript `encodeURIComponent`, which draw.io uses for clipboard text.
/// Percent-encodes like JavaScript `encodeURIComponent`.
pub fn encode_uri_component(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len() * 3 / 2);
    for &byte in text.as_bytes() {
        if uri_component_safe(byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    out
}

fn uri_component_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte)
}

pub(crate) fn encoded_uri_component_len(text: &str) -> Option<usize> {
    text.bytes().try_fold(0usize, |length, byte| {
        length.checked_add(if uri_component_safe(byte) { 1 } else { 3 })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::board::CanvasObjectKind;

    const MODEL: &str = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="a" value="Start" style="rounded=1;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="100" y="40" width="120" height="60" as="geometry"/></mxCell><mxCell id="b" value="End" style="ellipse;whiteSpace=wrap;html=1;" vertex="1" parent="1"><mxGeometry x="320" y="40" width="80" height="80" as="geometry"/></mxCell><mxCell id="e" style="edgeStyle=orthogonalEdgeStyle;" edge="1" parent="1" source="a" target="b"><mxGeometry relative="1" as="geometry"/></mxCell></root></mxGraphModel>"#;

    fn kinds(payload: &BoardClipboardPayload) -> (usize, usize) {
        let mut sheet = Sheet::new("t", "");
        sheet.root_mode = SheetRootMode::Hidden;
        paste_board_selection(&mut sheet, payload, Vec2::ZERO).unwrap();
        let connectors = sheet
            .canvas
            .objects
            .iter()
            .filter(|o| matches!(o.kind, CanvasObjectKind::Connector(_)))
            .count();
        (sheet.canvas.objects.len() - connectors, connectors)
    }

    #[test]
    fn detects_plain_encoded_and_file_forms_only() {
        assert!(looks_like_drawio(MODEL));
        assert!(looks_like_drawio(&encode_uri_component(MODEL)));
        assert!(looks_like_drawio("  \n<mxfile host=\"x\"></mxfile>"));
        assert!(looks_like_drawio("%3cmxGraphModel%3E"));
        assert!(!looks_like_drawio("Start\nEnd"));
        assert!(!looks_like_drawio("<svg></svg>"));
        assert!(!looks_like_drawio("é"));
    }

    #[test]
    fn plain_and_uri_encoded_models_decode_to_shapes_and_a_connector() {
        for text in [MODEL.to_owned(), encode_uri_component(MODEL)] {
            let pasted = decode(&text).unwrap();
            assert_eq!(kinds(&pasted.payload), (2, 1));
            let bounds = pasted.bounds.unwrap();
            assert!((bounds.min.x - 100.0).abs() < 1.0, "{bounds:?}");
            assert!((bounds.max().x - 400.0).abs() < 1.0, "{bounds:?}");
        }
    }

    #[test]
    fn encoded_selection_round_trips_through_draw_io_text() {
        let pasted = decode(MODEL).unwrap();
        let text = encode(&pasted.payload).unwrap();
        assert!(text.starts_with("%3CmxGraphModel"), "{text}");
        assert!(!text.contains('<') && !text.contains(' '));
        let again = decode(&text).unwrap();
        assert_eq!(kinds(&again.payload), (2, 1));
        assert_eq!(again.bounds, pasted.bounds);
    }

    #[test]
    fn encoded_waypoint_connector_attachments_are_reported() {
        use thinoda_core::{CanvasConnector, CanvasObject, Endpoint};

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let mut target = CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(250.0, 100.0)),
        );
        target.waypoints = vec![Vec2::new(200.0, 0.0), Vec2::new(200.0, 100.0)].into();
        let target = sheet
            .add_canvas_object(CanvasObject::connector(target))
            .unwrap();
        let mut dependent = CanvasConnector::new(
            Endpoint::Object(target),
            Endpoint::Point(Vec2::new(300.0, 200.0)),
        );
        dependent.label = "Dependent".into();
        let dependent = sheet
            .add_canvas_object(CanvasObject::connector(dependent))
            .unwrap();
        let layout = layout_sheet(&sheet, &ApproxMeasure, false);
        let payload = copy_board_selection(&sheet, &layout, &[], &[target, dependent])
            .unwrap()
            .unwrap();
        let copied = encode_with_report(&payload).unwrap();
        assert_eq!(
            copied.losses,
            vec![ExportLoss::ConnectorAttachmentsDetached(1)]
        );
        assert!(matches!(
            encode(&payload),
            Err(IoError::DrawioClipboardLoss(losses))
                if losses == vec![ExportLoss::ConnectorAttachmentsDetached(1)]
        ));

        let pasted = decode(&copied.text).unwrap();
        let mut restored = Sheet::new("Restored", "");
        restored.root_mode = SheetRootMode::Hidden;
        paste_board_selection(&mut restored, &pasted.payload, Vec2::ZERO).unwrap();
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
    }

    #[test]
    fn rejects_empty_oversized_and_malformed_data() {
        let empty = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/></root></mxGraphModel>"#;
        assert!(decode(empty).is_err());
        assert!(decode("<mxGraphModel><root>").is_err());
        assert!(decode("%3CmxGraphModel%ZZ").is_err());
        let huge = format!("<mxGraphModel>{}", " ".repeat(MAX_TEXT_BYTES));
        assert!(decode(&huge).is_err());
        assert!(decode("plain words").is_err());
    }

    #[test]
    fn uri_component_encoding_matches_javascript() {
        assert_eq!(
            encode_uri_component("<a b=\"ü\">-_.!~*'()</a>"),
            "%3Ca%20b%3D%22%C3%BC%22%3E-_.!~*'()%3C%2Fa%3E"
        );
    }
}

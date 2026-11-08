//! Prepare matching editable and visual representations of the active sheet.

use thinoda_core::{Workbook, clipboard::encode_sheet};
use thinoda_io::{Format, PresentationAspectRatio};

use crate::doc::OpenDoc;

pub(crate) struct PreparedSheet {
    pub(crate) text: String,
    pub(crate) svg: Result<String, String>,
}

pub(crate) enum SheetWrite {
    TextAndSvg,
    TextOnly(String),
}

pub(crate) fn prepare(doc: &OpenDoc) -> Result<PreparedSheet, String> {
    let snapshot = doc.document_with_drafts()?;
    let source = snapshot.workbook();
    let text = encode_sheet(source.active())?;
    let workbook = Workbook {
        metadata: source.metadata.clone(),
        ..Workbook::new(source.active().clone())
    };
    let options = crate::visual_export::options_for_gui_export(
        Format::Svg,
        doc.view.dark,
        PresentationAspectRatio::default(),
        false,
        false,
    );
    let svg = thinoda_io::export_bytes_with_visual_context(
        &workbook,
        Format::Svg,
        &options,
        crate::visual_export::for_view(doc.view.accent, doc.view.dark),
    )
    .map_err(crate::doc::export_error)
    .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()));
    Ok(PreparedSheet { text, svg })
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::{Sheet, clipboard::decode_sheet, resources::Resource};

    #[test]
    fn both_representations_use_the_active_sheet_and_current_appearance() {
        let inactive = Sheet::new("Inactive", "Not copied");
        let active = crate::visual_export::test_helpers::sheet();
        let workbook = Workbook {
            sheets: vec![inactive, active.clone()],
            active_sheet: 1,
            ..Workbook::new(active.clone())
        };
        let mut doc = OpenDoc::new(workbook);
        doc.view.dark = true;
        doc.view.accent = egui::Color32::from_rgb(25, 115, 210);

        let copy = prepare(&doc).unwrap();
        assert_eq!(decode_sheet(&copy.text).unwrap(), active);
        let svg = copy.svg.unwrap();
        let expected = thinoda_io::export_bytes_with_visual_context(
            &Workbook::new(active),
            Format::Svg,
            &crate::visual_export::options_for_gui_export(
                Format::Svg,
                true,
                PresentationAspectRatio::default(),
                false,
                false,
            ),
            crate::visual_export::for_view(doc.view.accent, true),
        )
        .unwrap();
        assert_eq!(svg.as_bytes(), expected);
        assert!(svg.starts_with("<svg"));
        assert!(!svg.contains("Not copied"));
    }

    #[test]
    fn draft_title_is_shared_by_the_editable_and_visual_copies() {
        let mut doc = OpenDoc::new(Workbook::new(Sheet::new("Sheet", "Original")));
        let root = doc.doc.sheet().root;
        doc.view.begin_edit(doc.doc.sheet(), root, false);
        doc.view.set_editing_text("Uncommitted draft");

        let copy = prepare(&doc).unwrap();
        assert_eq!(
            decode_sheet(&copy.text).unwrap().topic(root).unwrap().title,
            "Uncommitted draft"
        );
        assert!(copy.svg.unwrap().contains("Uncommitted draft"));
        assert_eq!(doc.doc.sheet().topic(root).unwrap().title, "Original");
    }

    #[test]
    fn svg_and_sheet_text_retain_embedded_images() {
        let mut sheet = Sheet::new("Images", "Image");
        let reference = sheet
            .resources
            .insert(
                Resource::new(
                    "icon.png",
                    "image/png",
                    include_bytes!("../../../assets/icon-16.png").to_vec(),
                )
                .unwrap(),
            )
            .unwrap();
        sheet.topic_mut(sheet.root).unwrap().image = Some(reference);
        let doc = OpenDoc::new(Workbook::new(sheet.clone()));

        let copy = prepare(&doc).unwrap();
        assert_eq!(decode_sheet(&copy.text).unwrap(), sheet);
        assert!(copy.svg.unwrap().contains("data:image/png;base64,"));
    }
}

use std::{collections::BTreeMap, io::Cursor};
use thinoda_core::{Sheet, Workbook, notes::NoteExportError};
use thinoda_io::{ExportOptions, Format, IoError, export_bytes, export_file};
use zip::ZipArchive;

#[derive(Debug, Default)]
struct Run {
    text: String,
    properties: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct Cell {
    text: String,
    runs: Vec<Run>,
    style: Option<String>,
}

fn part(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive
        .by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn cells(bytes: &[u8]) -> BTreeMap<String, Cell> {
    use quick_xml::events::Event;
    let xml = part(bytes, "xl/worksheets/sheet1.xml");
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut cells = BTreeMap::new();
    let mut cell = Cell::default();
    let mut address = String::new();
    let mut run: Option<Run> = None;
    let mut in_text = false;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(event) | Event::Empty(event) => {
                let name = event.name().as_ref().to_owned();
                let attributes: BTreeMap<_, _> = event
                    .attributes()
                    .map(|attribute| {
                        let attribute = attribute.unwrap();
                        (
                            attribute.key.as_ref().to_owned(),
                            attribute
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .unwrap()
                                .into_owned(),
                        )
                    })
                    .collect();
                match name.as_str() {
                    "c" => {
                        assert_eq!(attributes["t"], "inlineStr");
                        address = attributes["r"].clone();
                        cell = Cell {
                            style: attributes.get("s").cloned(),
                            ..Default::default()
                        };
                    }
                    "r" => run = Some(Run::default()),
                    "t" => in_text = true,
                    "b" | "i" | "strike" | "rFont" => {
                        run.as_mut()
                            .unwrap()
                            .properties
                            .insert(name, attributes.get("val").cloned().unwrap_or_default());
                    }
                    "f" | "hyperlink" => panic!("note text must not become executable content"),
                    _ => {}
                }
            }
            Event::Text(text) if in_text => {
                let text = text.xml_content(quick_xml::XmlVersion::Implicit1_0);
                cell.text.push_str(&text);
                if let Some(run) = &mut run {
                    run.text.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_text => {
                let text =
                    quick_xml::escape::resolve_predefined_entity(reference.as_ref()).unwrap();
                cell.text.push_str(text);
                if let Some(run) = &mut run {
                    run.text.push_str(text);
                }
            }
            Event::End(event) => match event.name().as_ref() {
                "t" => in_text = false,
                "r" => cell.runs.push(run.take().unwrap()),
                "c" => {
                    cells.insert(address.clone(), std::mem::take(&mut cell));
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    cells
}

fn book(source: &str) -> Workbook {
    let mut sheet = Sheet::new("Notes", "Research café 日本語");
    sheet
        .set_markdown_notes(sheet.root, Some(source.into()))
        .unwrap();
    Workbook::new(sheet)
}

fn excel(book: &Workbook) -> Vec<u8> {
    export_bytes(book, Format::Excel, &Default::default()).unwrap()
}

#[test]
fn rich_cells_preserve_styles_whitespace_and_inert_text_without_mutation() {
    let book =
        book("Plain **bold *café*** ~~old~~ **`*literal*`**  \n日本語  &lt;tag&gt; &amp;\n\n=1+1");
    let before = book.clone();
    let bytes = excel(&book);
    assert_eq!(book, before);
    let cells = cells(&bytes);
    let note = &cells["B2"];
    assert_eq!(note.style.as_deref(), Some("1"));
    assert_eq!(
        note.text,
        "Plain bold café old *literal*\n日本語  <tag> &\n\n=1+1"
    );
    let cafe = note.runs.iter().find(|run| run.text == "café").unwrap();
    assert!(cafe.properties.contains_key("b"));
    assert!(cafe.properties.contains_key("i"));
    let code = note
        .runs
        .iter()
        .find(|run| run.text == "*literal*")
        .unwrap();
    assert!(code.properties.contains_key("b"));
    assert_eq!(code.properties["rFont"], "Courier New");
    assert!(
        note.runs
            .iter()
            .find(|run| run.text == "old")
            .unwrap()
            .properties
            .contains_key("strike")
    );
    let styles = part(&bytes, "xl/styles.xml");
    assert!(styles.contains("<alignment vertical=\"top\" wrapText=\"1\"/>"));
    assert!(part(&bytes, "xl/_rels/workbook.xml.rels").contains("Target=\"styles.xml\""));
    assert!(part(&bytes, "[Content_Types].xml").contains("PartName=\"/xl/styles.xml\""));
    assert!(
        part(&bytes, "xl/worksheets/sheet1.xml")
            .contains("<col min=\"2\" max=\"2\" width=\"60\" customWidth=\"1\"/>")
    );
    assert!(!part(&bytes, "xl/worksheets/sheet1.xml").contains("<f>"));
}

#[test]
fn list_counters_nesting_continuations_and_restarts_remain_visible() {
    let book = book(
        "3. **First**\n\n   Continued\n\n   - Nested\n   - Next\n\n4. Second\n\nParagraph\n\n7. Restart",
    );
    let cells = cells(&excel(&book));
    assert_eq!(
        cells["B2"].text,
        "3. First\n\n  Continued\n\n  • Nested\n\n  • Next\n\n4. Second\n\nParagraph\n\n7. Restart"
    );
}

#[test]
fn plain_notes_remain_literal_and_spreadsheet_escape_sequences_are_not_decoded() {
    let mut book = book("`_x0041_` `_x005F_` &amp;");
    let root = book.active().root;
    let child = book.active_mut().add_child(root, None, "Literal");
    book.active_mut().topic_mut(child).unwrap().notes = Some("**bold** _x000A_".into());
    let bytes = excel(&book);
    let cells = cells(&bytes);
    assert_eq!(cells["C2"].text, "_x005F_x0041_ _x005F_x005F_ &");
    assert_eq!(cells["C3"].text, "**bold** _x005F_x000A_");
    assert!(cells["C3"].runs.is_empty());
    assert!(cells["C3"].style.is_none());
    let plain = Workbook::new(Sheet::new("Plain", "Root"));
    let bytes = excel(&plain);
    let archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert!(
        !archive
            .file_names()
            .any(|name| name.unwrap() == "xl/styles.xml")
    );
}

#[test]
fn unsupported_and_oversized_notes_fail_before_replacing_destination() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.xlsx");
    for (source, reason) in [
        (
            "[link](https://example.invalid)".to_owned(),
            NoteExportError::Unsupported,
        ),
        (
            "![image](file:///missing)".to_owned(),
            NoteExportError::Unsupported,
        ),
        ("<b>HTML</b>".to_owned(), NoteExportError::Unsupported),
        ("# Heading".to_owned(), NoteExportError::Unsupported),
        ("Text\tTab".to_owned(), NoteExportError::Unsupported),
        ("- [x] Task".to_owned(), NoteExportError::Unsupported),
        ("a".repeat(16 * 1024 + 1), NoteExportError::Limit),
        (format!("{}z", "a  \n".repeat(254)), NoteExportError::Limit),
    ] {
        let book = book(&source);
        let before = book.clone();
        std::fs::write(&path, b"existing destination").unwrap();
        let error = export_file(&book, &path, Format::Excel, &Default::default()).unwrap_err();
        assert!(
            matches!(error, IoError::ExcelNote { reason: actual, ref topic }
            if actual == reason && topic == "Research café 日本語"),
            "{error}"
        );
        assert_eq!(book, before);
        assert_eq!(std::fs::read(&path).unwrap(), b"existing destination");
    }
}

#[test]
fn exact_source_and_line_break_limits_succeed_and_only_selected_sheet_is_exported() {
    assert_eq!(
        cells(&excel(&book(&"a".repeat(16 * 1024))))["B2"]
            .text
            .len(),
        16 * 1024
    );
    let source = format!("{}z", "a  \n".repeat(253));
    assert_eq!(
        cells(&excel(&book(&source)))["B2"]
            .text
            .matches('\n')
            .count(),
        253
    );
    let mut book = book("[unsupported](https://example.invalid)");
    book.sheets.push(Sheet::new("Other", "Selected"));
    let root = book.sheets[1].root;
    book.sheets[1]
        .set_markdown_notes(root, Some("**Chosen**".into()))
        .unwrap();
    let bytes = export_bytes(
        &book,
        Format::Excel,
        &ExportOptions {
            sheet: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cells(&bytes)["B2"].text, "Chosen");
}

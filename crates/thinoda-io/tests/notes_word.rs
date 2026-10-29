use std::{collections::BTreeMap, io::Cursor};
use thinoda_core::{Sheet, Workbook, notes::NoteExportError};
use thinoda_io::{ExportOptions, Format, IoError, export_bytes, export_file};
use zip::ZipArchive;

#[derive(Debug, Default)]
struct Element {
    name: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn child(&self, name: &str) -> &Element {
        self.children.iter().find(|node| node.name == name).unwrap()
    }

    fn attribute(&self, name: &str) -> &str {
        &self.attributes[name]
    }
}

fn xml(bytes: &[u8], name: &str) -> Element {
    use quick_xml::events::Event;
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut reader =
        quick_xml::Reader::from_reader(std::io::BufReader::new(archive.by_name(name).unwrap()));
    let mut buffer = Vec::new();
    let mut stack = vec![Element::default()];
    loop {
        let event = reader.read_event_into(&mut buffer).unwrap();
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(event) | Event::Empty(event) => {
                let element = Element {
                    name: event.name().as_ref().to_owned(),
                    attributes: event
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
                        .collect(),
                    ..Default::default()
                };
                // Empty elements do not have a later End event.
                if empty {
                    stack.last_mut().unwrap().children.push(element);
                } else {
                    stack.push(element);
                }
            }
            Event::Text(text) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&text.xml_content(quick_xml::XmlVersion::Implicit1_0)),
            Event::GeneralRef(reference) => {
                let name = reference.as_ref();
                let character = if let Some(number) = name.strip_prefix("#x") {
                    char::from_u32(u32::from_str_radix(number, 16).unwrap())
                        .unwrap()
                        .to_string()
                } else if let Some(number) = name.strip_prefix('#') {
                    char::from_u32(number.parse().unwrap()).unwrap().to_string()
                } else {
                    quick_xml::escape::resolve_predefined_entity(name)
                        .unwrap()
                        .to_owned()
                };
                stack.last_mut().unwrap().text.push_str(&character);
            }
            Event::End(_) => {
                let element = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(element);
            }
            Event::Eof => break,
            Event::Decl(_) => {}
            other => panic!("Unexpected XML event: {other:?}"),
        }
        buffer.clear();
    }
    assert_eq!(stack.len(), 1);
    let mut root = stack.pop().unwrap();
    assert_eq!(root.children.len(), 1);
    root.children.remove(0)
}

fn book(source: &str) -> Workbook {
    let mut sheet = Sheet::new("Notes", "Research");
    sheet
        .set_markdown_notes(sheet.root, Some(source.into()))
        .unwrap();
    Workbook::new(sheet)
}

fn word(book: &Workbook) -> Vec<u8> {
    export_bytes(book, Format::Word, &Default::default()).unwrap()
}

#[test]
fn word_retains_unicode_styled_runs_breaks_tabs_and_escapes_inert_text() {
    let book = book("Plain **bold *café*** ~~old~~ **`*literal*`**  \n日本語\t&lt;tag&gt; &amp;");
    let before = book.clone();
    let bytes = word(&book);
    assert_eq!(book, before);
    let document = xml(&bytes, "word/document.xml");
    let body = document.child("w:body");
    let paragraphs: Vec<_> = body
        .children
        .iter()
        .filter(|node| node.name == "w:p")
        .collect();
    assert_eq!(paragraphs.len(), 2);
    let runs = &paragraphs[1].children;
    let bold_italic = runs
        .iter()
        .find(|run| run.children.iter().any(|node| node.text == "café"))
        .unwrap();
    let properties = bold_italic.child("w:rPr");
    properties.child("w:b");
    properties.child("w:i");
    properties.child("w:bCs");
    properties.child("w:iCs");
    let code = runs
        .iter()
        .find(|run| run.children.iter().any(|node| node.text == "*literal*"))
        .unwrap();
    assert_eq!(code.child("w:rPr").children[0].name, "w:rFonts");
    code.child("w:rPr").child("w:b");
    let strike = runs
        .iter()
        .find(|run| run.children.iter().any(|node| node.text == "old"))
        .unwrap();
    strike.child("w:rPr").child("w:strike");
    assert!(
        runs.iter()
            .any(|run| run.children.iter().any(|node| node.name == "w:br"))
    );
    assert!(
        runs.iter()
            .any(|run| run.children.iter().any(|node| node.name == "w:tab"))
    );
    let visible: String = runs
        .iter()
        .flat_map(|run| &run.children)
        .filter(|node| node.name == "w:t")
        .map(|node| node.text.as_str())
        .collect();
    assert_eq!(visible, "Plain bold café old *literal*日本語<tag> &");
    assert!(
        runs.iter()
            .flat_map(|run| &run.children)
            .filter(|node| node.name == "w:t")
            .all(|node| node.attribute("xml:space") == "preserve")
    );
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert!(archive.by_name("word/numbering.xml").is_err());
}

#[test]
fn word_lists_have_real_numbering_indentation_continuations_and_per_note_counters() {
    let mut book =
        book("3. **First**\n\n   Continuation\n\n   - Nested\n\n4. Last\n\nOutside\n\n7. Restart");
    let sheet = book.active_mut();
    let child = sheet.add_child(sheet.root, None, "Child");
    sheet
        .set_markdown_notes(child, Some("1. Separate note".into()))
        .unwrap();
    let bytes = word(&book);
    let document = xml(&bytes, "word/document.xml");
    let body = document.child("w:body");
    let paragraphs: Vec<_> = body
        .children
        .iter()
        .filter(|node| node.name == "w:p")
        .collect();
    let list_id = |paragraph: &Element| {
        paragraph
            .child("w:pPr")
            .child("w:numPr")
            .child("w:numId")
            .attribute("w:val")
            .to_owned()
    };
    assert_eq!(list_id(paragraphs[1]), "1");
    assert_eq!(list_id(paragraphs[3]), "2");
    assert_eq!(list_id(paragraphs[4]), "1");
    assert_eq!(list_id(paragraphs[6]), "3");
    assert_eq!(list_id(paragraphs[8]), "4");
    let continuation = paragraphs[2].child("w:pPr");
    assert!(
        !continuation
            .children
            .iter()
            .any(|node| node.name == "w:numPr")
    );
    assert_eq!(continuation.child("w:ind").attribute("w:hanging"), "0");
    assert_eq!(
        paragraphs[3]
            .child("w:pPr")
            .child("w:ind")
            .attribute("w:left"),
        "720"
    );

    let numbering = xml(&bytes, "word/numbering.xml");
    assert_eq!(numbering.children.len(), 8);
    for (index, (start, format)) in [
        ("3", "decimal"),
        ("1", "bullet"),
        ("7", "decimal"),
        ("1", "decimal"),
    ]
    .iter()
    .enumerate()
    {
        let definition = &numbering.children[index];
        assert_eq!(definition.name, "w:abstractNum");
        let level = definition.child("w:lvl");
        assert_eq!(level.child("w:start").attribute("w:val"), *start);
        assert_eq!(level.child("w:numFmt").attribute("w:val"), *format);
        let instance = &numbering.children[index + 4];
        assert_eq!(instance.name, "w:num");
        assert_eq!(instance.attribute("w:numId"), (index + 1).to_string());
        assert_eq!(
            instance.child("w:abstractNumId").attribute("w:val"),
            definition.attribute("w:abstractNumId")
        );
    }
    assert_eq!(
        numbering.children[1]
            .child("w:lvl")
            .child("w:lvlText")
            .attribute("w:val"),
        "•"
    );
    let relationships = xml(&bytes, "word/_rels/document.xml.rels");
    assert!(
        relationships
            .children
            .iter()
            .any(|node| node.attribute("Target") == "numbering.xml"
                && node.attribute("Type").ends_with("/numbering"))
    );
    let types = xml(&bytes, "[Content_Types].xml");
    assert!(types.children.iter().any(|node| {
        node.attributes
            .get("PartName")
            .is_some_and(|name| name == "/word/numbering.xml")
    }));
}

#[test]
fn legacy_notes_are_literal_and_unsupported_formatted_notes_fail_without_writing() {
    let mut book = book("**literal** [link](https://example.invalid)");
    let root = book.active().root;
    book.active_mut().topic_mut(root).unwrap().notes =
        Some("**literal** [link](https://example.invalid)".into());
    let bytes = word(&book);
    let document = xml(&bytes, "word/document.xml");
    let paragraph = &document.child("w:body").children[1];
    assert_eq!(
        paragraph.child("w:r").child("w:t").text,
        "**literal** [link](https://example.invalid)"
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.docx");
    std::fs::write(&path, b"existing destination").unwrap();
    for (source, expected) in [
        (
            "Valid **prefix**\n\n[link](https://example.invalid)".to_owned(),
            NoteExportError::Unsupported,
        ),
        (
            "![alt](file:///private.png)".into(),
            NoteExportError::Unsupported,
        ),
        (
            "<script>inert</script>".into(),
            NoteExportError::Unsupported,
        ),
        ("# unsupported heading".into(), NoteExportError::Unsupported),
        (
            "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1),
            NoteExportError::Limit,
        ),
    ] {
        book.active_mut()
            .set_markdown_notes(root, Some(source))
            .unwrap();
        let before = book.clone();
        let error = export_file(&book, &path, Format::Word, &Default::default()).unwrap_err();
        assert!(
            matches!(error, IoError::WordNote { topic, reason } if topic == "Research" && reason == expected)
        );
        assert_eq!(book, before);
        assert_eq!(std::fs::read(&path).unwrap(), b"existing destination");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn word_validates_only_notes_in_the_selected_exported_tree() {
    let mut book = book("[not supported](https://example.invalid)");
    book.sheets.push(Sheet::new("Second", "Second root"));
    book.active_sheet = 1;
    let sheet = book.active_mut();
    sheet
        .set_markdown_notes(sheet.root, Some("**supported**".into()))
        .unwrap();
    let floating = sheet.add_floating("Not exported", thinoda_core::Vec2::ZERO);
    sheet
        .set_markdown_notes(floating, Some("<unsupported>".into()))
        .unwrap();
    assert!(!word(&book).is_empty());
    let options = ExportOptions {
        sheet: Some(0),
        ..Default::default()
    };
    assert!(matches!(
        export_bytes(&book, Format::Word, &options),
        Err(IoError::WordNote { .. })
    ));
}

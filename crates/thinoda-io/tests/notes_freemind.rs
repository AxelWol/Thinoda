use std::collections::BTreeMap;
use thinoda_core::{Sheet, Workbook, notes::NoteExportError};
use thinoda_io::{
    ExportOptions, Format, ImportWarning, IoError, export_bytes, export_file, import_bytes,
    import_bytes_with_report,
};

#[derive(Debug, Default)]
struct Element {
    name: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Element>,
    text: String,
}

impl Element {
    fn child(&self, name: &str) -> &Self {
        self.children
            .iter()
            .find(|node| node.name == name)
            .unwrap_or_else(|| panic!("Missing {name} in {self:#?}"))
    }

    fn all_text(&self) -> String {
        let mut text = self.text.clone();
        for child in &self.children {
            text.push_str(&child.all_text());
        }
        text
    }

    fn body(&self) -> &Self {
        self.child("richcontent").child("html").child("body")
    }
}

fn xml(bytes: &[u8]) -> Element {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(bytes);
    let mut stack = vec![Element::default()];
    loop {
        let event = reader.read_event().unwrap();
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
                let text = reference
                    .resolve_char_ref()
                    .unwrap()
                    .map(|character| character.to_string())
                    .unwrap_or_else(|| {
                        quick_xml::escape::resolve_predefined_entity(reference.as_ref())
                            .unwrap()
                            .to_owned()
                    });
                stack.last_mut().unwrap().text.push_str(&text);
            }
            Event::End(_) => {
                let element = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(element);
            }
            Event::Eof => break,
            Event::Decl(_) => {}
            other => panic!("Unexpected XML event: {other:?}"),
        }
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

fn freemind(book: &Workbook) -> Vec<u8> {
    export_bytes(book, Format::FreeMind, &Default::default()).unwrap()
}

#[test]
fn formatted_notes_emit_only_safe_structural_html_with_styled_runs_and_unicode() {
    let book = book("Plain **bold *café*** ~~old~~ **`*literal*`**  \n日本語 &lt;tag&gt; &amp;");
    let before = book.clone();
    let map = xml(&freemind(&book));
    let node = map.child("node");
    assert_eq!(node.child("richcontent").attributes["TYPE"], "NOTE");
    let paragraph = node.body().child("p");
    assert!(paragraph.children.iter().any(|node| {
        node.name == "b"
            && node
                .children
                .iter()
                .any(|child| child.name == "i" && child.text == "café")
    }));
    assert_eq!(paragraph.child("strike").all_text(), "old");
    assert!(paragraph.children.iter().any(|node| {
        node.name == "b"
            && node
                .children
                .iter()
                .any(|child| child.name == "code" && child.text == "*literal*")
    }));
    paragraph.child("br");
    assert!(paragraph.text.contains("日本語 <tag> &"));

    fn safe(node: &Element) {
        assert!(matches!(
            node.name.as_str(),
            "html" | "head" | "body" | "p" | "b" | "i" | "strike" | "code" | "br"
        ));
        assert!(node.attributes.is_empty());
        for child in &node.children {
            safe(child);
        }
    }
    safe(node.child("richcontent").child("html"));
    assert_eq!(book, before);
}

#[test]
fn structural_lists_keep_nesting_starts_continuations_and_independent_counters() {
    let mut book = book(
        "3. First\n\n   Continued\n\n   - Nested\n     - Deep\n   - Sibling\n\n4. Second\n\nParagraph\n\n7. Restart",
    );
    let root = book.active().root;
    let child = book.active_mut().add_child(root, None, "Child");
    book.active_mut()
        .set_markdown_notes(child, Some("1. Separate".into()))
        .unwrap();
    let map = xml(&freemind(&book));
    let root = map.child("node");
    let body = root.body();
    let ordered = &body.children[0];
    assert_eq!(ordered.name, "ol");
    assert_eq!(ordered.attributes["start"], "3");
    assert_eq!(ordered.children.len(), 2);
    let first = &ordered.children[0];
    assert_eq!(first.name, "li");
    assert_eq!(first.children[0].text, "First");
    assert_eq!(first.children[1].text, "Continued");
    let nested = first.child("ul");
    assert_eq!(nested.children.len(), 2);
    assert_eq!(nested.children[0].child("p").text, "Nested");
    assert_eq!(
        nested.children[0].child("ul").child("li").child("p").text,
        "Deep"
    );
    assert_eq!(nested.children[1].child("p").text, "Sibling");
    assert_eq!(ordered.children[1].child("p").text, "Second");
    assert_eq!(body.children[1].text, "Paragraph");
    assert_eq!(body.children[2].attributes["start"], "7");
    assert_eq!(
        root.child("node").body().child("ol").attributes["start"],
        "1"
    );
}

#[test]
fn lists_with_empty_parent_items_and_parent_continuations_are_well_formed() {
    let book = book(
        "-\n  - Child\n\n  Parent continuation\n\n- Last\n\n1. Ordered\n\n   8. Nested ordered",
    );
    let map = xml(&freemind(&book));
    let body = map.child("node").body();
    let bullet = &body.children[0];
    assert_eq!(bullet.children.len(), 2);
    let parent = &bullet.children[0];
    assert_eq!(parent.children[0].name, "p");
    assert!(parent.children[0].text.is_empty());
    assert_eq!(parent.child("ul").child("li").child("p").text, "Child");
    assert_eq!(parent.children[2].text, "Parent continuation");
    assert_eq!(bullet.children[1].child("p").text, "Last");
    let ordered = &body.children[1];
    assert_eq!(ordered.attributes["start"], "1");
    assert_eq!(ordered.child("li").child("ol").attributes["start"], "8");
}

#[test]
fn plain_notes_remain_literal_and_failed_formatted_exports_preserve_the_destination() {
    let source = "**literal** [link](https://example.invalid) & <tag>";
    let mut book = book(source);
    let root = book.active().root;
    book.active_mut().topic_mut(root).unwrap().notes = Some(source.into());
    let bytes = freemind(&book);
    assert_eq!(xml(&bytes).child("node").body().child("p").text, source);
    let imported = import_bytes(Format::FreeMind, &bytes).unwrap();
    assert_eq!(
        imported
            .active()
            .topic(imported.active().root)
            .unwrap()
            .notes
            .as_deref(),
        Some(source)
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.mm");
    std::fs::write(&path, b"existing destination").unwrap();
    for (source, reason) in [
        (
            "[link](https://example.invalid)".to_owned(),
            NoteExportError::Unsupported,
        ),
        (
            "![image](file:///private.png)".into(),
            NoteExportError::Unsupported,
        ),
        (
            "<script>inert</script>".into(),
            NoteExportError::Unsupported,
        ),
        ("# Heading".into(), NoteExportError::Unsupported),
        ("a\tb".into(), NoteExportError::Unsupported),
        ("a  **b**".into(), NoteExportError::Unsupported),
        (
            "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1),
            NoteExportError::Limit,
        ),
    ] {
        book.active_mut()
            .set_markdown_notes(root, Some(source))
            .unwrap();
        let before = book.clone();
        assert!(
            matches!(export_file(&book, &path, Format::FreeMind, &Default::default()),
            Err(IoError::FreeMindNote { topic, reason: actual }) if topic == "Research" && reason == actual)
        );
        assert_eq!(book, before);
        assert_eq!(std::fs::read(&path).unwrap(), b"existing destination");
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn formatted_import_requires_confirmation_and_preserves_visible_text_boundaries_and_entities() {
    let bytes = freemind(&book(
        "Before **café** &amp; after  \nbreak\n\n- First\n- Second",
    ));
    let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    let sheet = report.workbook.active();
    assert_eq!(
        sheet.topic(sheet.root).unwrap().notes.as_deref(),
        Some("Before café & after\nbreak\nFirst\nSecond")
    );
    assert!(matches!(import_bytes(Format::FreeMind, &bytes),
        Err(IoError::ImportNeedsConfirmation(warnings)) if warnings == vec![ImportWarning::Formatting]));
}

#[test]
fn exporter_checks_only_the_selected_sheet_tree_and_accepts_exact_byte_limit() {
    let mut book = book("[not supported](https://example.invalid)");
    book.sheets.push(Sheet::new("Second", "Second root"));
    book.active_sheet = 1;
    let sheet = book.active_mut();
    sheet
        .set_markdown_notes(
            sheet.root,
            Some("x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES)),
        )
        .unwrap();
    let floating = sheet.add_floating("Not exported", thinoda_core::Vec2::ZERO);
    sheet
        .set_markdown_notes(floating, Some("<script>not exported</script>".into()))
        .unwrap();
    let before = book.clone();
    assert_eq!(
        xml(&freemind(&book))
            .child("node")
            .body()
            .child("p")
            .text
            .len(),
        thinoda_core::notes::MAX_PROJECTION_BYTES
    );
    assert!(matches!(
        export_bytes(
            &book,
            Format::FreeMind,
            &ExportOptions {
                sheet: Some(0),
                ..Default::default()
            }
        ),
        Err(IoError::FreeMindNote { .. })
    ));
    assert_eq!(book, before);
}

#[test]
fn import_reports_styles_and_multiple_paragraphs_and_retains_cdata_and_entities() {
    for (html, text) in [
        ("<body style=\"color: red\"><p>Styled</p></body>", "Styled"),
        ("<body><p>First</p><p>Second</p></body>", "First\nSecond"),
        ("<body><p><![CDATA[A & B]]></p></body>", "A & B"),
        (
            "<body><p>&#x65e5;&#26412; &amp; &#233;</p><br/></body>",
            "日本 & é",
        ),
    ] {
        let bytes = format!(
            "<map><node TEXT=\"Root\"><richcontent TYPE=\"NOTE\"><html>{html}</html></richcontent></node></map>"
        );
        let report = import_bytes_with_report(Format::FreeMind, bytes.as_bytes()).unwrap();
        assert_eq!(report.warnings, vec![ImportWarning::Formatting], "{html}");
        let sheet = report.workbook.active();
        assert_eq!(
            sheet.topic(sheet.root).unwrap().notes.as_deref(),
            Some(text)
        );
    }
}

#[test]
fn list_depth_and_parser_event_bounds_fail_explicitly() {
    let nested = |levels: usize| {
        (0..levels)
            .map(|depth| format!("{}- Item\n", "  ".repeat(depth)))
            .collect::<String>()
    };
    let map = xml(&freemind(&book(&nested(9))));
    let mut list = map.child("node").body().child("ul");
    for _ in 1..9 {
        list = list.child("li").child("ul");
    }
    assert_eq!(list.child("li").child("p").text, "Item");
    for source in [nested(10), "*x* ".repeat(600)] {
        assert!(matches!(
            export_bytes(&book(&source), Format::FreeMind, &Default::default()),
            Err(IoError::FreeMindNote {
                reason: NoteExportError::Limit,
                ..
            })
        ));
    }
}

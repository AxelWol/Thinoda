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
    assert!(
        !imported
            .active()
            .topic(imported.active().root)
            .unwrap()
            .notes
            .as_ref()
            .unwrap()
            .is_markdown()
    );
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
fn supported_import_retains_markdown_without_loss_confirmation() {
    let bytes = freemind(&book(
        "Before **café** &amp; after  \nbreak\n\n- First\n- Second",
    ));
    let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
    assert!(report.warnings.is_empty());
    let sheet = report.workbook.active();
    assert_eq!(
        thinoda_core::notes::plain_text_projection(
            sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap()
        )
        .as_deref(),
        Some("Before café & after\nbreak\nFirst\nSecond")
    );
    assert!(
        sheet
            .topic(sheet.root)
            .unwrap()
            .notes
            .as_ref()
            .unwrap()
            .is_markdown()
    );
    import_bytes(Format::FreeMind, &bytes).unwrap();
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
    for (html, text, warnings) in [
        (
            "<body style=\"color: red\"><p>Styled</p></body>",
            "Styled",
            vec![ImportWarning::Formatting],
        ),
        (
            "<body><p>First</p><p>Second</p></body>",
            "First\nSecond",
            vec![],
        ),
        ("<body><p><![CDATA[A & B]]></p></body>", "A & B", vec![]),
        (
            "<body><p>&#x65e5;&#26412; &amp; &#233;</p><br/></body>",
            "日本 & é",
            vec![ImportWarning::Formatting],
        ),
    ] {
        let bytes = format!(
            "<map><node TEXT=\"Root\"><richcontent TYPE=\"NOTE\"><html>{html}</html></richcontent></node></map>"
        );
        let report = import_bytes_with_report(Format::FreeMind, bytes.as_bytes()).unwrap();
        assert_eq!(report.warnings, warnings, "{html}");
        let sheet = report.workbook.active();
        assert_eq!(
            {
                let notes = sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap();
                if notes.is_markdown() {
                    thinoda_core::notes::plain_text_projection(notes).unwrap()
                } else {
                    notes.to_string()
                }
            },
            text
        );
    }
}

fn normalized(source: &str) -> thinoda_core::notes::NoteDocument {
    let mut document = thinoda_core::notes::formatted_document(source).unwrap();
    for paragraph in &mut document.paragraphs {
        let mut runs: Vec<thinoda_core::notes::NoteRun> = Vec::new();
        for run in std::mem::take(&mut paragraph.runs) {
            if let Some(last) = runs.last_mut()
                && last.style == run.style
            {
                last.text.push_str(&run.text);
            } else if !run.text.is_empty() {
                runs.push(run);
            }
        }
        paragraph.runs = runs;
    }
    document
}

#[test]
fn supported_export_import_roundtrips_remain_editable_and_survive_native_storage() {
    for source in [
        "Plain **bold *café*** ~~old~~ **`*literal*`**  \n日本語 &lt;tag&gt; &amp;",
        "***both*** and *italic* **bold** ~~**old**~~ `a``b`",
        "First\n\nSecond  \nthird",
        "3. First\n\n   Continued\n\n   - Nested\n     - Deep\n   - Sibling\n\n4. Second\n\nParagraph\n\n7. Restart",
        "-\n  - Child\n\n  Parent continuation\n\n- Last\n\n1. Ordered\n\n   8. Nested ordered",
        "**\\*literal\\* \\[link\\]\\(url\\) &lt;b&gt; ![no] &#35; \\~\\~no\\~\\~**",
        "**&#32;leading** and **trailing&#32;**",
        "- First  \n  break\n\n  Continuation\n\n- Second",
        "~~` tick `~~ and `` `edge` ``",
        "- First\n\n  8. Nested\n\n+ Independent\n\n3. Ordered\n\n7) Restart",
        "9. First\n\n   Continued\n\n   99. Nested\n\n10. Second\n\n    Continuation\n\n    - Leaf",
        "0. Zero\n1. Next\n\n999999999) Last",
    ] {
        let bytes = freemind(&book(source));
        let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
        assert!(report.warnings.is_empty(), "{source}");
        let sheet = report.workbook.active();
        let notes = sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap();
        assert!(notes.is_markdown(), "{source}");
        assert_eq!(normalized(notes), normalized(source), "{source}\n{notes}");
        let native = export_bytes(&report.workbook, Format::Thinoda, &Default::default()).unwrap();
        assert_eq!(
            import_bytes(Format::Thinoda, &native).unwrap(),
            report.workbook
        );
        // A second format round trip must be equally stable.
        let again = import_bytes(Format::FreeMind, &freemind(&report.workbook)).unwrap();
        let sheet = again.active();
        assert_eq!(
            normalized(sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap()),
            normalized(source)
        );
    }
}

fn rich(html: &str) -> Vec<u8> {
    format!("<map><node TEXT=\"Root\"><richcontent TYPE=\"NOTE\">{html}</richcontent><node TEXT=\"Child\"/></node></map>").into_bytes()
}

#[test]
fn html_text_punctuation_is_literal_and_supported_aliases_are_explicit() {
    let bytes = rich(
        "<html><head/><body><p><strong>*literal* [x](url) &lt;script&gt; &amp; #title</strong></p><ol start=\"7\"><li><em>one</em></li><li><del>two</del><br/><s>last</s></li></ol></body></html>",
    );
    let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
    assert!(report.warnings.is_empty());
    let sheet = report.workbook.active();
    assert_eq!(sheet.topics.len(), 2);
    let notes = sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap();
    let document = normalized(notes);
    assert_eq!(document.lists, vec![Some(7)]);
    let run = &document.paragraphs[0].runs[0];
    assert_eq!(run.text, "*literal* [x](url) <script> & #title");
    assert!(run.style.bold);
    assert!(!run.style.italic);
}

#[test]
fn legacy_plain_notes_keep_literal_punctuation_and_source_whitespace() {
    let source = " \n**literal** _literal_ ~~literal~~ `code` [x](url)  \t日本語\n ";
    let mut workbook = book("unused");
    let sheet = workbook.active_mut();
    sheet.topic_mut(sheet.root).unwrap().notes = Some(source.into());
    let bytes = freemind(&workbook);
    let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(
        report.workbook.active().root_topic().notes,
        Some(source.into())
    );
}

#[test]
fn duplicate_note_envelopes_keep_both_readable_sources_and_warn() {
    let bytes = b"<map><node TEXT=\"Root\"><richcontent TYPE=\"NOTE\"><html><body><p><b>First</b></p></body></html></richcontent><richcontent TYPE=\"NOTE\"><html><body><p>Second</p></body></html></richcontent></node></map>";
    let report = import_bytes_with_report(Format::FreeMind, bytes).unwrap();
    assert_eq!(report.warnings, vec![ImportWarning::Formatting]);
    assert_eq!(
        report.workbook.active().root_topic().notes,
        Some("First\nSecond".into())
    );
}

#[test]
fn rich_import_bounds_accept_the_boundary_and_warn_on_the_next_unit() {
    use thinoda_core::notes::{MAX_PROJECTION_BYTES, MAX_PROJECTION_DEPTH, MAX_PROJECTION_EVENTS};
    let assert_warning = |html: String, lossy: bool| {
        let report = import_bytes_with_report(Format::FreeMind, &rich(&html)).unwrap();
        assert_eq!(!report.warnings.is_empty(), lossy);
        assert_eq!(
            report
                .workbook
                .active()
                .root_topic()
                .notes
                .as_ref()
                .unwrap()
                .is_markdown(),
            !lossy
        );
    };
    for extra in 0..=1 {
        let prefix = "<html><body><p><b>";
        let suffix = "</b></p></body></html>";
        let length =
            MAX_PROJECTION_BYTES - prefix.len() - suffix.len() - "</richcontent>".len() + extra;
        assert_warning(
            format!("{prefix}{}{suffix}", "x".repeat(length)),
            extra != 0,
        );
        // html/body/p account for the three non-style containers.
        let depth = MAX_PROJECTION_DEPTH - 3 + extra;
        assert_warning(
            format!(
                "<html><body><p>{}x{}</p></body></html>",
                "<b>".repeat(depth),
                "</b>".repeat(depth)
            ),
            extra != 0,
        );
        // Wrappers, b/text/end and the NOTE end account for ten XML events.
        assert_warning(
            format!(
                "<html><body><p><b>x</b>{}</p></body></html>",
                "<b/>".repeat(MAX_PROJECTION_EVENTS - 10 + extra)
            ),
            extra != 0,
        );
    }
}

#[test]
fn adjacent_style_combinations_survive_without_delimiter_collisions() {
    let styled = |mask, text: &str| {
        let tags = ["b", "i", "strike", "code"];
        let mut output = String::new();
        for (index, tag) in tags.iter().enumerate() {
            if mask & (1 << index) != 0 {
                output.push_str(&format!("<{tag}>"));
            }
        }
        output.push_str(text);
        for (index, tag) in tags.iter().enumerate().rev() {
            if mask & (1 << index) != 0 {
                output.push_str(&format!("</{tag}>"));
            }
        }
        output
    };
    for first in 0..16 {
        for second in 0..16 {
            if first == 0 && second == 0 {
                continue;
            }
            let html = format!(
                "<html><body><p>{}{}</p></body></html>",
                styled(first, "one"),
                styled(second, "two")
            );
            let report = import_bytes_with_report(Format::FreeMind, &rich(&html)).unwrap();
            assert!(report.warnings.is_empty(), "{first} -> {second}: {html}");
            let sheet = report.workbook.active();
            let notes = sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap();
            let document = normalized(notes);
            assert_eq!(
                document.paragraphs[0]
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<String>(),
                "onetwo"
            );
        }
    }
}

#[test]
fn unsupported_html_spacing_and_bounds_keep_readable_plain_fallback_and_consent() {
    let deep = format!("{}text{}", "<b>".repeat(33), "</b>".repeat(33));
    let large = format!(
        "<b>{}</b>",
        "x".repeat(thinoda_core::notes::MAX_PROJECTION_BYTES + 1)
    );
    let events = "<b>x</b>".repeat(700);
    let lists = format!(
        "{}{}",
        "<ul><li><p>Parent</p>".repeat(9),
        "</li></ul>".repeat(9)
    );
    let too_many_lists = format!("<ul><li><p>Outer</p>{lists}</li></ul>");
    for fragment in [
        "<p style=\"color:red\">Readable</p>",
        "<p><a href=\"https://example.invalid\">Readable</a><img src=\"file:///private\"/></p>",
        "<p><script>inert</script></p>",
        "<p><span>Readable</span></p>",
        "<p>Before<node TEXT=\"Not a map topic\">Readable</node>After</p>",
        "<p><b>a  b</b></p>",
        "<p><b>a\tb</b></p>",
        "<p><code>a<br/>b</code></p>",
        "<p><b>a\nb</b></p>",
        "<p/><p>Empty paragraph</p>",
        "<ol start=\"2147483648\"><li>Large counter</li></ol>",
        "<ol reversed=\"true\"><li>Reverse</li></ol>",
        "<ul><li value=\"5\">Value</li></ul>",
        "<ul><li>Mixed<ul><li>Nested</li></ul></li></ul>",
        "<p><b>x</b></p>&#160;<p>Non-breaking block space</p>",
        &format!("<p>{deep}</p>"),
        &format!("<p>{large}</p>"),
        &format!("<p>{events}</p>"),
        &too_many_lists,
    ] {
        let bytes = rich(&format!("<html><body>{fragment}</body></html>"));
        let report = import_bytes_with_report(Format::FreeMind, &bytes).unwrap();
        assert_eq!(
            report.warnings,
            vec![ImportWarning::Formatting],
            "{fragment}"
        );
        let sheet = report.workbook.active();
        assert_eq!(sheet.topics.len(), 2);
        let notes = sheet.topic(sheet.root).unwrap().notes.as_ref().unwrap();
        assert!(!notes.is_markdown(), "{fragment}");
        assert!(!notes.is_empty(), "{fragment}");
        assert!(matches!(
            import_bytes(Format::FreeMind, &bytes),
            Err(IoError::ImportNeedsConfirmation(_))
        ));
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
    let report = import_bytes_with_report(Format::FreeMind, &freemind(&book(&nested(9)))).unwrap();
    assert!(report.warnings.is_empty());
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

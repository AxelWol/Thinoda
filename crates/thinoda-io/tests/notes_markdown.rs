use pulldown_cmark::{Event, Options, Parser, Tag};
use std::io::{Cursor, Read};
use thinoda_core::{Sheet, Workbook};
use thinoda_io::{ExportOptions, Format, export_bytes};

fn markdown(book: &Workbook, options: &ExportOptions) -> String {
    String::from_utf8(export_bytes(book, Format::Markdown, options).unwrap()).unwrap()
}

fn textbundle(book: &Workbook, options: &ExportOptions) -> String {
    let bytes = export_bytes(book, Format::TextBundle, options).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    archive
        .by_name("mindmap.textbundle/text.md")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn formatted_multiline_notes_remain_markdown_at_heading_and_nested_list_depths() {
    let mut sheet = Sheet::new("Ideas", "Root");
    let child = sheet.add_child(sheet.root, None, "Child");
    let grandchild = sheet.add_child(child, None, "Grandchild");
    let deep = sheet.add_child(grandchild, None, "Deep");
    let source = "**café** and *日本語*  \nHard break\n\n3. First\n4. Second\n\n~~old~~ `code`";
    for id in [sheet.root, child, grandchild, deep] {
        sheet.set_markdown_notes(id, Some(source.into())).unwrap();
    }
    let book = Workbook::new(sheet);
    let before = book.clone();
    let text = markdown(&book, &Default::default());
    assert_eq!(book, before);
    assert_eq!(text, textbundle(&book, &Default::default()));
    assert!(text.contains("## Child\n\n**café**"));
    assert!(
        text.contains(
            "  - Grandchild\n\n    **café** and *日本語*  \n    Hard break\n\n    3. First"
        )
    );
    assert!(text.contains("    - Deep\n\n      **café**"));
    let events: Vec<_> = Parser::new_ext(&text, Options::ENABLE_STRIKETHROUGH).collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::Strong)))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::Emphasis)))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::Strikethrough)))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::HardBreak))
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::List(Some(3)))))
            .count(),
        4
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Start(Tag::CodeBlock(_))))
    );
}

#[test]
fn legacy_plain_notes_are_literal_even_with_html_and_embedded_fences() {
    let mut sheet = Sheet::new("Ideas", "Root");
    let child = sheet.add_child(sheet.root, None, "Child");
    let nested = sheet.add_child(child, None, "Nested");
    let literal =
        "**not bold**\n<script>literal</script>\n````\n# not a heading\n\n    spaces & <tags>\n";
    for id in [sheet.root, child, nested] {
        sheet.topic_mut(id).unwrap().notes = Some(literal.into());
    }
    let book = Workbook::new(sheet);
    let before = book.clone();
    let text = markdown(&book, &Default::default());
    assert_eq!(book, before);
    assert_eq!(text, textbundle(&book, &Default::default()));
    assert!(text.contains("`````\n**not bold**"));
    let events: Vec<_> = Parser::new(&text).collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Start(Tag::CodeBlock(_))))
            .count(),
        3
    );
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::Start(Tag::Strong) | Event::Html(_) | Event::InlineHtml(_)
    )));
    let code: String = events
        .iter()
        .filter_map(|event| {
            if let Event::Text(text) = event {
                Some(text.as_ref())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(code.matches(literal).count(), 3);
}

#[test]
fn formatted_source_is_not_limited_to_the_word_subset_or_truncated() {
    let source = format!(
        "# Note heading\n\n[link](https://example.invalid)\n\n```rust\nlet a = 1;\n```\n\n{}",
        "café ".repeat(4000)
    );
    let mut sheet = Sheet::new("Ideas", "Root");
    sheet
        .set_markdown_notes(sheet.root, Some(source.clone()))
        .unwrap();
    let book = Workbook::new(sheet);
    let text = markdown(&book, &Default::default());
    assert!(text.contains(&source));
    assert_eq!(text, textbundle(&book, &Default::default()));
}

#[test]
fn selected_sheet_scope_and_topic_only_hierarchy_are_unchanged() {
    let mut first = Sheet::new("First", "Not exported");
    first.topic_mut(first.root).unwrap().notes = Some("hidden".into());
    let mut selected = Sheet::new("Chosen", "Root");
    let child = selected.add_child(selected.root, None, "Child");
    selected.add_child(child, None, "Grandchild");
    let mut book = Workbook::new(first);
    book.sheets.push(selected);
    let options = ExportOptions {
        sheet: Some(1),
        ..Default::default()
    };
    let text = markdown(&book, &options);
    assert_eq!(text, "# Root\n\n## Child\n  - Grandchild\n");
    assert_eq!(text, textbundle(&book, &options));
    book.active_sheet = 1;
    assert_eq!(text, markdown(&book, &Default::default()));
}

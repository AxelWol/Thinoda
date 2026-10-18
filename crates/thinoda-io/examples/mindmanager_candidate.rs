//! Generate an original, non-sensitive map for independent MindManager verification.
use thinoda_core::{Color, Sheet, TextAlign, TopicShape, Workbook};
use thinoda_io::{ExportOptions, Format, export_file};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("provide a new output .mmap path")?;
    let path = std::path::Path::new(&path);
    if path.exists() {
        return Err("candidate destination already exists".into());
    }
    let mut sheet = Sheet::new("Interchange verification", "MindManager compatibility");
    let root = sheet.root;
    let unicode = sheet.add_child(root, None, "Unicode: Grüße · café · 日本語 · 🧠");
    sheet.add_child(unicode, None, "Ordered child 1");
    sheet.add_child(unicode, None, "Ordered child 2");
    let notes = sheet.add_child(root, None, "Notes, links and labels");
    let topic = sheet.topic_mut(notes).unwrap();
    topic.notes =
        Some("First line <&> \"quotes\"\nSecond line: café 日本語\n\nFinal paragraph.".into());
    topic.hyperlink = Some("https://example.org/?a=1&b=2".into());
    topic.labels = vec!["Original test fixture".into(), "Label <&>".into()];
    topic.markers = vec!["priority-2".into(), "task-half".into()];
    topic.style.font_family = Some("Arial".into());
    topic.style.font_size = Some(16.0);
    topic.style.bold = Some(true);
    topic.style.italic = Some(true);
    topic.style.underline = Some(true);
    topic.style.align = Some(TextAlign::Left);
    for shape in TopicShape::ALL {
        let id = sheet.add_child(root, None, format!("Shape: {}", shape.label()));
        sheet.topic_mut(id).unwrap().style.shape = Some(shape);
    }
    let collapsed = sheet.add_child(root, None, "Collapsed branch");
    sheet.add_child(collapsed, None, "Hidden until expanded");
    sheet.topic_mut(collapsed).unwrap().collapsed = true;
    for (title, fill) in [
        ("Opaque red / white text", Color::rgb(255, 0, 0)),
        ("Opaque blue / white text", Color::rgb(0, 0, 255)),
    ] {
        let id = sheet.add_child(root, None, title);
        let style = &mut sheet.topic_mut(id).unwrap().style;
        style.fill = Some(fill);
        style.text_color = Some(Color::WHITE);
        style.font_family = Some("Arial".into());
        style.font_size = Some(16.0);
    }
    let mut book = Workbook::new(sheet);
    book.metadata.creator = "Thinoda original interoperability fixture".into();
    book.metadata.description = Some("No user content. Verify hierarchy, Unicode, notes, styles, markers and links in MindManager.".into());
    export_file(&book, path, Format::MindManager, &ExportOptions::default())?;
    Ok(())
}

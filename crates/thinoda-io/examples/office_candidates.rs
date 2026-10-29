//! Original Office fixtures for SDK validation and installed-Office acceptance.
use thinoda_core::{Sheet, Workbook};
use thinoda_io::{
    ExportOptions, Format, PresentationAspectRatio, export_file, generate_presentation,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("provide a new output directory")?;
    let output = std::path::Path::new(&output);
    if output.exists() {
        return Err("candidate directory already exists".into());
    }
    std::fs::create_dir_all(output)?;
    let mut sheet = Sheet::new("Office validation", "Thinoda Office compatibility");
    let branch = sheet.add_child(sheet.root, None, "Roadmap & <\"delivery\">");
    sheet.add_child(branch, None, "First milestone: café · 日本語");
    let notes = sheet.add_child(sheet.root, None, "Rich notes");
    sheet.set_markdown_notes(
        notes,
        Some("**Bold *italic*** ~~strike~~ `code`  \nHard break\tTab\n\n3. First\n\n   Continued\n\n   - Nested\n\n4. Second\n\nParagraph\n\n7. Restart".into()),
    )?;
    let plain = sheet.add_child(sheet.root, None, "Plain notes");
    let topic = sheet.topic_mut(plain).unwrap();
    topic.notes = Some("Literal **Markdown** & <tags>\nSecond line".into());
    topic.labels = vec!["Label <&>".into()];
    topic.markers = vec!["priority-2".into()];
    topic.hyperlink = Some("https://example.invalid/?a=1&b=2".into());
    let mut book = Workbook::new(sheet);
    for (name, format) in [
        ("rich-notes.docx", Format::Word),
        ("map.xlsx", Format::Excel),
    ] {
        export_file(&book, &output.join(name), format, &Default::default())?;
    }
    let plain = Workbook::new(Sheet::new("Plain", "Legacy plain notes"));
    export_file(
        &plain,
        &output.join("plain.docx"),
        Format::Word,
        &Default::default(),
    )?;
    export_file(
        &plain,
        &output.join("plain.xlsx"),
        Format::Excel,
        &Default::default(),
    )?;
    for (appearance, dark) in [("light", false), ("dark", true)] {
        let options = ExportOptions {
            dark,
            ..Default::default()
        };
        book.active_mut().presentation = generate_presentation(book.active(), &options)?;
        for (ratio_name, ratio) in [
            ("16x9", PresentationAspectRatio::Widescreen16x9),
            ("4x3", PresentationAspectRatio::Standard4x3),
        ] {
            let options = ExportOptions {
                presentation_aspect_ratio: ratio,
                ..options.clone()
            };
            export_file(
                &book,
                &output.join(format!("slides-{appearance}-{ratio_name}.pptx")),
                Format::PowerPoint,
                &options,
            )?;
        }
    }
    println!(
        "Generated Office validation fixtures in {}",
        output.display()
    );
    Ok(())
}

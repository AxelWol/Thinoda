//! Original Office fixtures for SDK validation and installed-Office acceptance.
use thinoda_core::{Color, Sheet, TopicShape, Vec2, Workbook};
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
    let rich_source = "**Bold *italic*** ~~strike~~ **`code`**  \nHard break\tTab\n\n3. First\n\n   Continued\n\n   - Nested\n\n4. Second\n\nParagraph `_x0041_` &amp; &#x1F600;\n\n7. Restart";
    sheet.set_markdown_notes(notes, Some(rich_source.into()))?;
    let plain = sheet.add_child(sheet.root, None, "Plain notes");
    let topic = sheet.topic_mut(plain).unwrap();
    topic.notes = Some("Literal **Markdown** & <tags>\nSecond line".into());
    topic.labels = vec!["Label <&>".into()];
    topic.markers = vec!["priority-2".into()];
    topic.hyperlink = Some("https://example.invalid/?a=1&b=2".into());
    let mut book = Workbook::new(sheet);
    export_file(
        &book,
        &output.join("rich-notes.docx"),
        Format::Word,
        &Default::default(),
    )?;
    let mut excel = book.clone();
    // Excel rejects tabs, while the independent Word fixture exercises them.
    excel
        .active_mut()
        .set_markdown_notes(notes, Some(rich_source.replace('\t', "  ")))?;
    export_file(
        &excel,
        &output.join("map.xlsx"),
        Format::Excel,
        &Default::default(),
    )?;
    let plain = Workbook::new(Sheet::new("Plain", "Legacy plain notes"));
    export_file(
        &plain,
        &output.join("plain.docx"),
        Format::Word,
        &Default::default(),
    )?;
    // Native presentation primitives: all topic shapes, annotation ownership,
    // both connector paths and real PNG/JPEG pictures (never slide pictures).
    let sheet = book.active_mut();
    let mut shape_topics = Vec::new();
    for (index, shape) in TopicShape::ALL.into_iter().enumerate() {
        let id = sheet.add_child(branch, None, format!("Editable {shape:?}"));
        let topic = sheet.topic_mut(id).unwrap();
        topic.style.shape = Some(shape);
        topic.style.italic = Some(true);
        topic.style.underline = Some(true);
        topic.style.strikethrough = Some(index % 2 == 0);
        topic.markers = vec!["priority-2".into()];
        topic.labels = vec!["Native label".into()];
        topic.todo = Some(index % 2 == 0);
        topic.marker_colors.insert(0, Color::rgb(10, 120, 60));
        shape_topics.push(id);
    }
    sheet.add_boundary(&shape_topics[..2])?;
    sheet.add_summary(&shape_topics[..2], "Editable summary")?;
    sheet.add_relationship(shape_topics[0], shape_topics[1])?;
    for (id, format, media) in [
        (shape_topics[2], image::ImageFormat::Png, "image/png"),
        (shape_topics[3], image::ImageFormat::Jpeg, "image/jpeg"),
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            32,
            16,
            image::Rgb([30, 120, 210]),
        ))
        .write_to(&mut bytes, format)?;
        let resource = thinoda_core::resources::Resource::new(
            "Original fixture photo",
            media,
            bytes.into_inner(),
        )?;
        thinoda_core::images::set_image(sheet, id, Some((resource, Vec2::new(80.0, 40.0))))?;
    }
    let floating = sheet.add_floating("Zone root", Vec2::new(700.0, 500.0));
    sheet.add_child(floating, None, "Zone descendant");
    sheet.create_zone(
        "Native zone",
        vec![floating],
        thinoda_core::Rect::from_min_size(Vec2::new(600.0, 400.0), Vec2::new(500.0, 300.0)),
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

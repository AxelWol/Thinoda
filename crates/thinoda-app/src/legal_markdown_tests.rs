use super::*;
use egui_kittest::{Harness, kittest::Queryable};

const FIXTURE: &str = "# Heading\n\nA **strong** and *italic* and ~~deleted~~ `code` [site](https://example.com).\n\n- First **item**\n  - Nested\n- [x] Done\n\n> Quoted text\n\n| Crate | License |\n|:--|--:|\n| original | MIT |\n\n```text\nline one\nline two\n```\n\n---\n";

#[cfg(feature = "gpu-tests")]
#[test]
fn renders_markdown_readability_previews() {
    for dark in [false, true] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(720.0, 800.0))
            .build_ui_state(
                |ui, markdown: &mut Markdown| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    markdown.show(ui, FIXTURE);
                },
                Markdown::default(),
            );
        h.run_steps(3);
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target")
            .join(format!("legal-markdown-{dark}.png"));
        h.render().unwrap().save(output).unwrap();
    }
}

#[test]
fn renders_markdown_structure_links_and_typography_in_both_themes() {
    for dark in [false, true] {
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (Markdown, Option<String>)| {
                ui.ctx().set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                state.1 = state.0.show(ui, FIXTURE);
            },
            (Markdown::default(), None),
        );
        h.run_steps(3);
        for value in [
            "Heading",
            "strong",
            "italic",
            "deleted",
            "code",
            "First ",
            "item",
            "Nested",
            "Quoted text",
            "Crate",
            "License",
            "original",
            "MIT",
            "line one\nline two\n",
        ] {
            assert!(h.query_by_label(value).is_some(), "missing {value:?}");
        }
        assert!(h.query_by_label("**strong**").is_none());
        assert!(
            h.get_by_label("item").rect().left() > h.get_by_label("First ").rect().right() + 1.0
        );
        let texts = h
            .output()
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.iter().any(
            |text| text.galley.text() == "italic" && text.galley.job.sections[0].format.italics
        ));
        assert!(texts.iter().any(|text| text.galley.text() == "deleted"
            && text.galley.job.sections[0].format.strikethrough.width > 0.0));
        assert!(texts.iter().any(|text| text.galley.text() == "code"
            && text.galley.job.sections[0].format.font_id.family == egui::FontFamily::Monospace));
        let code = texts
            .iter()
            .find(|text| text.galley.text().starts_with("line one\n"))
            .unwrap();
        let section = &code.galley.job.sections[0].format;
        let natural = h.ctx.fonts_mut(|fonts| fonts.row_height(&section.font_id));
        assert_eq!(section.line_height, Some(natural * 1.5));
        let rows = &code.galley.rows;
        assert!(rows.len() >= 2);
        assert!(((rows[1].pos.y - rows[0].pos.y) - natural * 1.5).abs() <= 1.0);
        h.get_by_label("site").click();
        h.run_steps(1);
        assert_eq!(h.state().1.as_deref(), Some("https://example.com"));
    }
}

#[test]
fn every_text_format_uses_one_and_a_half_natural_line_height() {
    let mut h = Harness::new_ui(|ui| {
        for style in [
            InlineStyle::default(),
            InlineStyle {
                strong: true,
                ..Default::default()
            },
            InlineStyle {
                italics: true,
                ..Default::default()
            },
            InlineStyle {
                code: true,
                ..Default::default()
            },
            InlineStyle {
                heading: Some(1),
                ..Default::default()
            },
            InlineStyle {
                heading: Some(6),
                ..Default::default()
            },
            InlineStyle {
                link: Some("https://example.com".into()),
                ..Default::default()
            },
        ] {
            let format = format(ui, &style);
            let natural = ui.fonts_mut(|fonts| fonts.row_height(&format.font_id));
            assert_eq!(format.line_height, Some(natural * 1.5));
        }
    });
    h.run_steps(2);
}

#[test]
fn comments_do_not_render_or_open_network_resources() {
    let mut h = Harness::new_ui_state(
        |ui, markdown: &mut Markdown| {
            markdown.show(ui, "<!-- metadata -->\n\n![Offline alternative](https://example.com/image.png)\n\n<script>bad()</script>");
        },
        Markdown::default(),
    );
    h.run_steps(2);
    assert!(h.query_by_label("metadata").is_none());
    assert!(h.query_by_label("Offline alternative").is_some());
    assert!(h.output().platform_output.commands.is_empty());
}

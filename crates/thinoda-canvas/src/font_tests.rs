use crate::{EguiMeasure, MapCanvas, MapView};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{
    Card, CardKind, Color, Document, Sheet, SheetRootMode, TextAlign, TextMeasure, Vec2, Workbook,
};

#[test]
fn named_family_is_used_for_title_measurement_paint_and_inline_editing() {
    let mut doc = Document::new(Workbook::new(Sheet::new("Fonts", "iiiiiiiiiiiiiiiiiiii")));
    doc.edit_sheet("Family", |sheet| {
        sheet.theme.font_family = Some("Late font".into());
        Ok(())
    })
    .unwrap();
    let root = doc.sheet().root;
    let mut h = Harness::new_ui_state(
        |ui, state: &mut (Document, MapView)| {
            MapCanvas::show(ui, &mut state.0, &mut state.1);
        },
        (doc, MapView::default()),
    );
    h.run_steps(3);
    let initial = {
        let (doc, view) = h.state_mut();
        view.layout_for(doc).topics[&root].title_rect.size.x
    };
    let mut defs = egui::FontDefinitions::default();
    defs.font_data
        .insert("late font data".into(), defs.font_data["Hack"].clone());
    let family = egui::FontFamily::Name("Late font".into());
    defs.families
        .insert(family.clone(), vec!["late font data".into()]);
    h.ctx.set_fonts(defs);
    h.run_steps(3);
    let layout = {
        let (doc, view) = h.state_mut();
        view.layout_for(doc)
    };
    let topic = &layout.topics[&root];
    assert!(topic.title_rect.size.x > initial * 1.5);
    let expected = EguiMeasure::new(&h.ctx).measure_with_family(
        "iiiiiiiiiiiiiiiiiiii",
        topic.style.font_size,
        topic.style.bold,
        400.0,
        Some("Late font"),
    );
    assert_eq!(topic.title_rect.size, expected);
    fn uses_family(shape: &egui::Shape, family: &egui::FontFamily) -> bool {
        match shape {
            egui::Shape::Text(text) => text
                .galley
                .job
                .sections
                .iter()
                .any(|section| &section.format.font_id.family == family),
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| uses_family(shape, family)),
            _ => false,
        }
    }
    assert!(
        h.output()
            .shapes
            .iter()
            .any(|shape| uses_family(&shape.shape, &family))
    );
    let (doc, view) = h.state_mut();
    view.begin_edit(doc.sheet(), root, false);
    h.run_steps(3);
    assert!(
        h.output()
            .shapes
            .iter()
            .any(|shape| uses_family(&shape.shape, &family))
    );
    assert_eq!(
        h.state().0.sheet().theme.font_family.as_deref(),
        Some("Late font")
    );
    h.state_mut().1.cancel_edit();
    let mut defs = h.ctx.fonts(|fonts| fonts.definitions().clone());
    defs.font_data.insert(
        "late font data".into(),
        defs.font_data["Ubuntu-Light"].clone(),
    );
    h.ctx.set_fonts(defs);
    h.run_steps(3);
    let (doc, view) = h.state_mut();
    assert_eq!(
        view.layout_for(doc).topics[&root].title_rect.size.x,
        initial
    );
}

fn styled_card_editor(
    kind: CardKind,
    dark: bool,
    align: TextAlign,
    zoom: f32,
    title: &str,
) -> (Harness<'static, (Document, MapView)>, thinoda_core::TopicId) {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let id = sheet
        .add_card(title, Vec2::ZERO, Card::new(kind, Vec2::new(180.0, 120.0)))
        .unwrap();
    let style = &mut sheet.topic_mut(id).unwrap().style;
    style.bold = Some(true);
    style.italic = Some(true);
    style.underline = Some(true);
    style.strikethrough = Some(true);
    style.font_family = Some("monospace".into());
    style.font_size = Some(24.0);
    style.text_color = Some(Color::rgb(120, 45, 70));
    style.align = Some(align);

    let mut doc = Document::new(Workbook::new(sheet));
    doc.mark_saved();
    let mut view = MapView::default();
    view.dark = dark;
    view.show_add_controls = false;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(move |ctx| {
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(
            |ui, state: &mut (Document, MapView)| {
                MapCanvas::show(ui, &mut state.0, &mut state.1);
            },
            (doc, view),
        );
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(3);
    h.state_mut().1.set_zoom(zoom);
    {
        let (doc, view) = h.state_mut();
        view.select_only(id);
        view.begin_edit(doc.sheet(), id, true);
    }
    h.run_steps(4);
    (h, id)
}

#[test]
fn styled_card_editor_keeps_formatting_selection_clipping_and_undo() {
    let original = "A long first line that wraps\nLast line  ";
    for kind in [CardKind::Sticky, CardKind::Text] {
        for dark in [false, true] {
            for (align, zoom) in [
                (TextAlign::Left, 0.5),
                (TextAlign::Center, 1.0),
                (TextAlign::Right, 2.0),
            ] {
                let (mut h, id) = styled_card_editor(kind, dark, align, zoom, original);
                let shapes: Vec<_> = h
                    .output()
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == original => {
                            Some((shape.clip_rect, text))
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(shapes.len(), 2, "{kind:?} dark={dark} {align:?}");
                let (clip, underlay) = shapes[0];
                let (_, foreground) = shapes[1];
                let format = &underlay.galley.job.sections[0].format;
                assert_eq!(format.font_id.family, egui::FontFamily::Monospace);
                assert_eq!(format.font_id.size, 24.0 * zoom);
                assert_eq!(format.color, egui::Color32::from_rgb(120, 45, 70));
                assert!(format.italics);
                assert_ne!(format.underline, egui::Stroke::NONE);
                assert_ne!(format.strikethrough, egui::Stroke::NONE);
                assert_eq!(
                    underlay.galley.job.halign,
                    match align {
                        TextAlign::Left => egui::Align::LEFT,
                        TextAlign::Center => egui::Align::Center,
                        TextAlign::Right => egui::Align::RIGHT,
                    }
                );
                assert!(underlay.galley.job.keep_trailing_whitespace);
                assert!(underlay.galley.rows.len() > 1);
                let glyphs: String = underlay
                    .galley
                    .rows
                    .iter()
                    .flat_map(|row| row.row.glyphs.iter().map(|glyph| glyph.chr))
                    .collect();
                assert!(glyphs.ends_with("  "), "{kind:?} {align:?}: {glyphs:?}");
                assert!(
                    (underlay.pos.x - foreground.pos.x - 0.45 * zoom).abs() < 0.01,
                    "the bold copy should use the canvas's offset"
                );
                assert!(
                    foreground.galley.rows[0].row.visuals.mesh.indices.len()
                        > underlay.galley.rows[0].row.visuals.mesh.indices.len(),
                    "only the foreground should contain selection backgrounds"
                );
                let editor = h.get_by_label("Topic title").rect();
                assert!(clip.width() <= editor.width() + 0.5, "{clip:?} {editor:?}");
                assert!(
                    clip.height() <= editor.height() + 0.5,
                    "{clip:?} {editor:?}"
                );
                assert_eq!(h.state().0.sheet().topic(id).unwrap().title, original);
                assert!(!h.state().0.is_dirty());

                h.get_by_label("Topic title").type_text("Changed  ");
                h.run_steps(2);
                assert_eq!(h.state().1.editing_text(), Some("Changed  "));
                assert_eq!(h.state().0.sheet().topic(id).unwrap().title, original);
                h.key_press(egui::Key::Enter);
                h.run_steps(2);
                assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Changed  ");
                assert!(h.state_mut().0.undo());
                assert_eq!(h.state().0.sheet().topic(id).unwrap().title, original);
                assert!(h.state_mut().0.redo());
                assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Changed  ");
            }
        }
    }
}

#[test]
fn styled_card_editor_retains_ime_and_conflicting_drafts() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut h, id) = styled_card_editor(kind, false, TextAlign::Left, 1.0, "Original");
        h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "nihon".into(),
            active_range_chars: None,
        }));
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().1.editing_topic(), Some(id));
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Original");
        h.input_mut().events.extend([
            egui::Event::Ime(egui::ImeEvent::Commit("日本".into())),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        h.run_steps(2);
        assert_eq!(h.state().1.editing_topic(), Some(id));
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "日本");
        assert!(h.state_mut().0.undo());
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Original");

        {
            let (doc, view) = h.state_mut();
            view.begin_edit(doc.sheet(), id, false);
            view.set_editing_text("Pending  ");
            doc.edit_sheet("External edit", |sheet| sheet.set_title(id, "External"))
                .unwrap();
        }
        h.run_steps(4);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(
            h.state().1.title_draft(),
            Some((id, "Original", "Pending  "))
        );
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "External");
        assert!(h.state_mut().1.take_title_error().is_some());
    }
}

#[test]
fn styled_card_editor_accepts_an_empty_draft() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut h, id) = styled_card_editor(kind, false, TextAlign::Center, 1.0, "Delete me");
        h.key_press(egui::Key::Backspace);
        h.run_steps(3);
        assert_eq!(h.state().1.editing_topic(), Some(id));
        assert_eq!(h.state().1.editing_text(), Some(""));
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Delete me");
    }
}

#[test]
fn styled_card_editor_respects_each_independent_text_style() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        for flags in 0..16 {
            let (mut h, id) = styled_card_editor(kind, true, TextAlign::Right, 1.0, "Text");
            h.state_mut()
                .0
                .edit_sheet("Style combinations", |sheet| {
                    sheet.update_style(&[id], |style| {
                        style.bold = Some(flags & 1 != 0);
                        style.italic = Some(flags & 2 != 0);
                        style.underline = Some(flags & 4 != 0);
                        style.strikethrough = Some(flags & 8 != 0);
                    })
                })
                .unwrap();
            h.run_steps(3);
            let shapes: Vec<_> = h
                .output()
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Text" => Some(text),
                    _ => None,
                })
                .collect();
            assert_eq!(shapes.len(), if flags & 1 != 0 { 2 } else { 1 });
            let format = &shapes[0].galley.job.sections[0].format;
            assert_eq!(format.italics, flags & 2 != 0);
            assert_eq!(format.underline != egui::Stroke::NONE, flags & 4 != 0);
            assert_eq!(format.strikethrough != egui::Stroke::NONE, flags & 8 != 0);
            assert_eq!(format.font_id.family, egui::FontFamily::Monospace);
            assert_eq!(format.font_id.size, 24.0);
            assert_eq!(format.color, egui::Color32::from_rgb(120, 45, 70));
            assert_eq!(shapes[0].galley.job.halign, egui::Align::RIGHT);
            assert!(shapes[0].galley.job.keep_trailing_whitespace);
            assert_eq!(h.state().1.editing_topic(), Some(id));
        }
    }
}

#[test]
fn styled_card_editor_preserves_unicode_range_selection_and_plain_text_clipboard() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut h, id) = styled_card_editor(kind, false, TextAlign::Left, 1.0, "Café ab  ");
        h.key_press(egui::Key::ArrowRight);
        h.key_press(egui::Key::ArrowLeft);
        h.key_press(egui::Key::ArrowLeft);
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
        h.event(egui::Event::Copy);
        h.run_steps(1);
        assert!(
            h.output()
                .platform_output
                .commands
                .contains(&egui::OutputCommand::CopyText("b".into()))
        );
        h.event(egui::Event::Paste("Ω".into()));
        h.run_steps(2);
        assert_eq!(h.state().1.editing_text(), Some("Café aΩ  "));
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Café ab  ");
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Café aΩ  ");
        assert!(h.state_mut().0.undo());
        assert_eq!(h.state().0.sheet().topic(id).unwrap().title, "Café ab  ");
    }
}

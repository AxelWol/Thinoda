use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn deleted_and_regenerated_slides_do_not_reuse_retired_draft_ids() {
    let mut editor = editor();
    editor.duplicate_slide().unwrap();
    let retired = editor.sheet.presentation.slides.remove(editor.selected);
    editor.selected = 0;
    editor.duplicate_slide().unwrap();
    assert!(editor.sheet.presentation.slides[editor.selected].id > retired.id);
    let old: std::collections::HashSet<_> = editor
        .sheet
        .presentation
        .slides
        .iter()
        .map(|slide| slide.id)
        .collect();
    let generated = thinoda_io::generate_presentation(&editor.sheet, &Default::default()).unwrap();
    editor.replace_generated(generated).unwrap();
    assert!(
        editor
            .sheet
            .presentation
            .slides
            .iter()
            .all(|slide| !old.contains(&slide.id))
    );
}

fn editor() -> PitchEditor {
    let mut app = super::super::tests::setup(true);
    app.begin_presentation_editor(&Context::default());
    app.pitch.editor.take().unwrap()
}

#[test]
fn duplicate_preserves_composition_but_allocates_a_unique_identity() {
    let mut editor = editor();
    editor.sheet.presentation.slides[0].max_depth = Some(2);
    let mut original = editor.sheet.presentation.slides[0].clone();
    editor.duplicate_slide().unwrap();
    let copy = editor.sheet.presentation.slides[1].clone();
    assert_ne!(copy.id, original.id);
    assert_eq!(editor.selected, 1);
    original.id = copy.id;
    assert_eq!(copy, original);
    editor.move_slide(0);
    assert_eq!(editor.sheet.presentation.slides[0], copy);
    assert_eq!(editor.selected, 0);
    let ids: std::collections::HashSet<_> = editor
        .sheet
        .presentation
        .slides
        .iter()
        .map(|slide| slide.id)
        .collect();
    assert_eq!(ids.len(), editor.sheet.presentation.slides.len());
}

#[test]
fn automatic_to_manual_frame_preserves_composition_and_hidden_topics_can_be_restored() {
    for dark in [false, true] {
        let editor = editor();
        let source = match editor.sheet.presentation.slides[0].source {
            PresentationSource::Branch(topic) => topic,
            _ => panic!("expected branch"),
        };
        let frame = thinoda_core::resolve_presentation_slide(
            &editor.sheet,
            &editor.sheet.presentation.slides[0],
            &editor.layout,
        )
        .unwrap()
        .frame;
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 1000.0))
            .build_ui_state(
                move |ui, editor: &mut PitchEditor| {
                    ui.ctx().set_visuals(if dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    });
                    editor.workspace(ui, true);
                },
                editor,
            );
        h.run_steps(3);
        h.get_by_label("Auto-fit content").click();
        h.run_steps(2);
        let slide = &h.state().sheet.presentation.slides[0];
        assert!(!slide.auto_frame);
        assert_eq!(slide.frame, frame);
        h.get_by_label("Topic visibility").click();
        h.run_steps(2);
        let title = h.state().sheet.topic(source).unwrap().title.clone();
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, title.as_str())
            .click();
        h.run_steps(2);
        assert_eq!(
            h.state().sheet.presentation.slides[0].hidden_topics,
            vec![source]
        );
        h.get_by_label("Restore topic").click();
        h.run_steps(2);
        assert!(
            h.state().sheet.presentation.slides[0]
                .hidden_topics
                .is_empty()
        );
        assert!(h.state().generation >= 3);
    }
}

#[test]
fn unsafe_touch_does_not_change_visibility_or_framing() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 1000.0))
        .build_ui_state(
            |ui, editor: &mut PitchEditor| editor.workspace(ui, false),
            editor(),
        );
    h.run_steps(3);
    let before = h.state().sheet.presentation.clone();
    for label in [
        "Auto-fit content",
        "Map context",
        "Duplicate Slide",
        "Move Down",
        "Remove Slide",
    ] {
        h.get_by_label(label).click();
        h.run_steps(2);
        assert_eq!(h.state().sheet.presentation, before, "{label}");
    }
}

#[test]
fn wide_workspace_does_not_clip_the_first_slide_card() {
    for dark in [false, true] {
        let mut editor = editor();
        let ctx = Context::default();
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        for frame in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(frame as f64 / 60.0),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ui| editor.workspace(ui, true),
            );
            output.textures_delta.clear();
            if frame == 2 {
                let card = output
                    .shapes
                    .iter()
                    .find_map(|clipped| match &clipped.shape {
                        egui::epaint::Shape::Rect(rect)
                            if rect.stroke.width == 2.0
                                && rect.rect.width() > 140.0
                                && rect.rect.height() > 90.0 =>
                        {
                            Some((clipped.clip_rect, rect.rect))
                        }
                        _ => None,
                    })
                    .expect("selected slide card is painted");
                assert!(
                    card.0.contains_rect(card.1),
                    "first slide card {:?} is clipped by {:?}",
                    card.1,
                    card.0
                );
            }
        }
    }
}

#[test]
fn filmstrip_viewport_tracks_available_dialog_height() {
    let viewport_height = |height| {
        let ctx = Context::default();
        let mut editor = editor();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.set_max_height(height);
                editor.workspace(ui, true);
            },
        );
        let viewport = output
            .shapes
            .iter()
            .find_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Rect(rect)
                    if rect.stroke.width == 2.0
                        && rect.rect.width() > 140.0
                        && rect.rect.height() > 90.0 =>
                {
                    Some(clipped.clip_rect)
                }
                _ => None,
            })
            .expect("selected slide card is painted");
        output.textures_delta.clear();
        viewport.height()
    };

    let compact = viewport_height(480.0);
    let tall = viewport_height(900.0);
    assert!(compact > 300.0, "compact dialog viewport was {compact}");
    assert!(
        tall > compact + 350.0,
        "filmstrip did not grow with dialog height: {compact} → {tall}"
    );
    assert!(tall <= 900.0);
}

#[test]
fn near_limit_filmstrip_resolves_only_viewport_thumbnails_not_every_slide_on_redraw() {
    use super::super::pitch_preview::THUMBNAIL_RESOLVES;
    // Both map size and deck size are deliberately nontrivial. Cloning the full map
    // for every one of these slides each frame would dominate interactive redraw.
    let mut editor = editor();
    for branch in 0..48 {
        let topic = editor
            .sheet
            .insert_child(editor.sheet.root, &format!("Branch {branch}"))
            .unwrap();
        for detail in 0..8 {
            editor
                .sheet
                .insert_child(topic, &format!("Detail {branch}/{detail}"))
                .unwrap();
        }
    }
    editor.layout = Arc::new(thinoda_core::layout_sheet(
        &editor.sheet,
        &thinoda_core::ApproxMeasure,
        false,
    ));
    let mut original = editor.sheet.presentation.slides[0].clone();
    original.source = PresentationSource::Branch(editor.sheet.root);
    editor.sheet.presentation.slides = (0..512)
        .map(|index| {
            let mut slide = original.clone();
            slide.id = thinoda_core::SlideId(index + 1);
            slide.title = format!("Slide {index}");
            slide
        })
        .collect();
    assert!(editor.layout.order.len() > 400);
    for horizontal in [false, true] {
        for dark in [false, true] {
            let ctx = Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let mut max_resolves = 0;
            let start = std::time::Instant::now();
            for frame in 0..20 {
                THUMBNAIL_RESOLVES.with(|count| count.set(0));
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        time: Some(frame as f64 / 60.0),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(360.0, 560.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        editor.filmstrip(ui, true, horizontal);
                    },
                );
                output.textures_delta.clear();
                assert!(!output.shapes.is_empty());
                let calls = THUMBNAIL_RESOLVES.with(|count| count.get());
                assert!(calls > 0);
                // At most a handful of visible thumbnail rows/columns, including an
                // edge row; this bound is independent of the 512-slide deck length.
                assert!(
                    calls <= 8,
                    "{horizontal}/{dark}: {calls} resolver calls in one redraw"
                );
                max_resolves = max_resolves.max(calls);
            }
            eprintln!(
                "512 slides / {} topics / horizontal={horizontal} dark={dark}: 20 redraws {:?}, max thumbnail resolves/frame={max_resolves}",
                editor.layout.order.len(),
                start.elapsed()
            );
        }
    }
}

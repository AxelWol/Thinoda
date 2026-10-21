use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::ThinodaApp;
use thinoda_io::{Format, export_bytes, import_bytes};

#[test]
fn discovered_fonts_preserve_configured_faces_and_canvas_cache_metrics() {
    use std::sync::{Arc, Mutex};

    use egui::{FallbackRequest, FontDefinitions, FontFamily, FontId, FontInsert};
    use thinoda_canvas::EguiMeasure;
    use thinoda_core::layout_sheet;

    let defaults = FontDefinitions::default();
    let ubuntu = defaults.font_data["Ubuntu-Light"].as_ref().clone();
    let mut configured = defaults.clone();
    configured
        .families
        .insert(FontFamily::Proportional, vec!["Hack".into()]);
    let named = FontFamily::Name("Configured Hack".into());
    configured
        .families
        .insert(named.clone(), vec!["Hack".into()]);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let provider = Arc::new(move |request: &FallbackRequest<'_>| {
        recorded.lock().unwrap().push(request.cluster.to_owned());
        Some(FontInsert::new("discovered-test", ubuntu.clone(), vec![]))
    });
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Mixed title", |sheet| {
            sheet.set_title(root, "Latin and \u{018f}")
        })
        .unwrap();
    let before = d.doc.workbook().clone();
    let state = d.doc.state_id();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_context_setup(move |ctx| {
            ctx.set_fonts(configured.clone());
            ctx.add_font_provider(provider.clone());
        })
        .build_ui_state(
            |ui, app: &mut ThinodaApp| {
                ui.ctx()
                    .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                app.draw(ui);
            },
            app,
        );
    assert!(requests.lock().unwrap().iter().any(|s| s == "\u{018f}"));
    assert!(!requests.lock().unwrap().iter().any(|s| s == "L"));
    assert!(h.ctx.fonts_mut(|fonts| {
        fonts.has_glyph(&FontId::new(16.0, FontFamily::Proportional), '\u{018f}')
    }));
    let cached = {
        let d = h.state_mut().doc_mut().unwrap();
        d.view.layout_for(&d.doc)
    };
    let ctx = h.ctx.clone();
    let d = h.state().doc().unwrap();
    let fresh = layout_sheet(d.doc.sheet(), &EguiMeasure::new(&ctx), d.view.dark);
    assert_eq!(
        cached.topics[&root].title_rect,
        fresh.topics[&root].title_rect
    );
    let count = requests.lock().unwrap().len();
    h.run_steps(4);
    let d = h.state_mut().doc_mut().unwrap();
    assert!(Arc::ptr_eq(&cached, &d.view.layout_for(&d.doc)));
    assert_eq!(d.doc.workbook(), &before);
    assert_eq!(d.doc.state_id(), state);
    assert_eq!(requests.lock().unwrap().len(), count);
    // Named families installed before discovery and after a definitions reset
    // must retain their primary face and still discover missing glyphs.
    let defs = ctx.fonts(|fonts| fonts.definitions().clone());
    assert_eq!(defs.families[&named], ["Hack"]);
    ctx.set_fonts(defs.clone());
    let output = ctx.run_ui(Default::default(), |ui| {
        assert!(
            ui.fonts_mut(|fonts| fonts.has_glyph(&FontId::new(16.0, named.clone()), '\u{018f}'))
        );
        assert_eq!(
            ui.fonts(|fonts| fonts.definitions().families[&named].clone()),
            ["Hack"]
        );
    });
    output.drop_without_applying_deltas();
    let mut expanded = defs;
    expanded
        .families
        .insert(FontFamily::Name("Later Hack".into()), vec!["Hack".into()]);
    ctx.set_fonts(expanded);
    let output = ctx.run_ui(Default::default(), |ui| {
        assert!(ui.fonts_mut(|fonts| fonts.has_glyph(
            &FontId::new(16.0, FontFamily::Name("Later Hack".into())),
            '\u{018f}'
        )));
    });
    output.drop_without_applying_deltas();
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "Native acceptance: requires installed Japanese, Chinese and Korean fonts"]
fn windows_system_fallback_renders_mixed_titles_and_uncommitted_edits_across_documents() {
    use std::sync::Arc;

    use egui::{FontFamily, FontId};
    use egui_system_font_provider::SystemFontProvider;
    use thinoda_app::app::CenterMode;
    use thinoda_canvas::EguiMeasure;
    use thinoda_core::layout_sheet;

    const MIXED: &str = "Latin \u{65e5}\u{672c}\u{8a9e} \u{4e2d}\u{6587} \u{d55c}\u{ae00}";
    let ctx = egui::Context::default();
    thinoda_app::install_context(&ctx);
    let output = ctx.run_ui(Default::default(), |ui| {
        assert!(!ui.fonts_mut(|fonts| fonts.has_glyph(&FontId::default(), '\u{65e5}')));
    });
    output.drop_without_applying_deltas();
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Mixed title", |sheet| {
            sheet.theme.font_family = Some("Segoe UI".into());
            sheet.set_title(root, MIXED)
        })
        .unwrap();
    d.doc.mark_saved();
    let before = d.doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_context_setup(|ctx| {
            thinoda_app::install_context(ctx);
            ctx.add_font_provider(Arc::new(SystemFontProvider::new()));
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    let named = FontFamily::Name("Segoe UI".into());
    for family in [FontFamily::Proportional, FontFamily::Monospace, named] {
        assert!(
            h.ctx
                .fonts_mut(|fonts| fonts.has_glyphs(&FontId::new(16.0, family.clone()), MIXED)),
            "{family:?}"
        );
    }
    assert!(h.ctx.fonts(|fonts| {
        fonts
            .discovered_fonts()
            .iter()
            .any(|font| font.name.starts_with("system:"))
    }));
    let ctx = h.ctx.clone();
    let d = h.state_mut().doc_mut().unwrap();
    let cached = d.view.layout_for(&d.doc);
    let fresh = layout_sheet(d.doc.sheet(), &EguiMeasure::new(&ctx), d.view.dark);
    assert_eq!(
        cached.topics[&root].title_rect,
        fresh.topics[&root].title_rect
    );
    assert_eq!(d.doc.workbook(), &before);
    assert!(!d.doc.is_dirty());
    // A new script in a title draft is not a document revision yet.
    let draft = format!("{MIXED} \u{30ab}\u{30ca}");
    d.view.begin_edit_with(d.doc.sheet(), root, &draft);
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(d.view.editing_text(), Some(draft.as_str()));
    assert_eq!(d.doc.workbook(), &before);
    assert!(!d.doc.is_dirty());
    assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(&FontId::default(), &draft)));
    d.view.commit_edit(&mut d.doc);
    assert!(d.doc.undo());
    assert_eq!(d.doc.workbook(), &before);
    let other = before.clone();
    h.state_mut().new_doc(other);
    h.run_steps(3);
    h.state_mut().active = 0;
    h.state_mut().mode = CenterMode::Outliner;
    h.run_steps(3);
    assert!(h.query_all_by_label(MIXED).count() >= 1);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn global_and_topic_font_controls_change_measured_layout_with_undo_and_roundtrips() {
    let mut app = ThinodaApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    d.doc
        .edit_sheet("Title", |sheet| {
            sheet.set_title(root, "iiiiiiiiiiiiiiiiiiii")
        })
        .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let initial = {
        let d = h.state_mut().doc_mut().unwrap();
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x
    };
    h.get_by_role_and_label(Role::Tab, "Map").click();
    h.run_steps(2);
    h.get_by_label("Global Font").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Global Font").click();
    h.run_steps(2);
    h.get_by_label("Monospace").click();
    h.run_steps(3);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(
        d.doc.sheet().theme.font_family.as_deref(),
        Some("monospace")
    );
    let mono = d.view.layout_for(&d.doc).topics[&root].title_rect.size.x;
    assert!(mono > initial * 1.5, "{initial} -> {mono}");
    assert!(d.doc.undo());
    assert_eq!(d.doc.workbook(), &before);
    assert!(d.doc.redo());
    h.get_by_role_and_label(Role::Tab, "Style").click();
    h.run_steps(2);
    h.get_by_label("Font Family").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Font Family").click();
    h.run_steps(2);
    h.get_by_label("Monospace").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(root)
            .unwrap()
            .style
            .font_family
            .as_deref(),
        Some("monospace")
    );
    let book = h.state().doc().unwrap().doc.workbook().clone();
    for format in [Format::Thinoda, Format::XMind] {
        let bytes = export_bytes(&book, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), book);
    }
    h.get_by_label("Font Family").click();
    h.run_steps(2);
    h.get_by_label("Auto").click();
    h.run_steps(2);
    let d = h.state_mut().doc_mut().unwrap();
    assert_eq!(d.doc.sheet().topic(root).unwrap().style.font_family, None);
    assert_eq!(
        d.view.layout_for(&d.doc).topics[&root].title_rect.size.x,
        mono
    );
}

#[test]
fn unavailable_fonts_warn_without_changing_document_or_requested_name() {
    let mut app = ThinodaApp::headless();
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Missing font", |sheet| {
            sheet.theme.font_family = Some("Thinoda Absent Font Family".into());
            Ok(())
        })
        .unwrap();
    let before = app.doc().unwrap().doc.workbook().clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.run_steps(3);
    let message = "Font unavailable: Thinoda Absent Font Family. Using the default font; the requested name is preserved.";
    assert!(h.query_by_label(message).is_some());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().doc_mut().unwrap().doc.mark_saved();
    h.run_steps(3);
    assert!(!h.state().doc().unwrap().doc.is_dirty());
    assert_eq!(h.query_all_by_label(message).count(), 1);
}

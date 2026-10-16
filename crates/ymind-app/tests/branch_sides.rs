use egui_kittest::{Harness, kittest::Queryable};
use ymind_app::app::YMindApp;
use ymind_core::{BranchSide, Structure};
use ymind_io::{Format, export_bytes, import_bytes};

#[test]
fn style_branch_side_tracks_selection_resets_and_roundtrips() {
    let mut app = YMindApp::headless();
    let d = app.doc_mut().unwrap();
    let root = d.doc.sheet().root;
    let (a, b) = d
        .doc
        .edit_sheet("Fixture", |s| {
            s.set_structure(None, Some(Structure::MapClockwise))?;
            let a = s.insert_child(root, "A")?;
            let b = s.insert_child(root, "B")?;
            s.set_branch_side(a, Some(BranchSide::Left))?;
            Ok((a, b))
        })
        .unwrap();
    d.view.select_only(a);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(|ui, app: &mut YMindApp| app.draw(ui), app);
    h.run_steps(4);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    h.get_by_label("Branch side").click();
    h.run_steps(2);
    h.get_by_label("Right").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(a)
            .unwrap()
            .branch_side,
        Some(BranchSide::Right)
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    h.state_mut().doc_mut().unwrap().view.select_only(b);
    h.run_steps(2);
    h.get_by_label("Branch side").click();
    h.run_steps(2);
    h.get_by_label("Left").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(b)
            .unwrap()
            .branch_side,
        Some(BranchSide::Left)
    );
    let manual = h.state().doc().unwrap().doc.workbook().clone();
    for format in [Format::YMind, Format::XMind] {
        let bytes = export_bytes(&manual, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &bytes).unwrap(), manual);
    }
    h.get_by_label("Branch side").click();
    h.run_steps(2);
    h.get_by_label("Auto").click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(b)
            .unwrap()
            .branch_side,
        None
    );
    h.state_mut().doc_mut().unwrap().view.select_only(root);
    h.run_steps(2);
    assert!(h.query_by_label("Branch side").is_none());
}

use super::*;
use egui_kittest::Harness;
use thinoda_core::{SheetRootMode, Workbook};

struct State {
    doc: Document,
    view: MapView,
}

fn harness() -> Harness<'static, State> {
    let mut sheet = Sheet::new("Board", "");
    sheet.root_mode = SheetRootMode::Hidden;
    sheet.add_floating("First", Vec2::ZERO);
    let view = MapView {
        show_minimap: true,
        ..Default::default()
    };
    let mut h = Harness::builder()
        .with_size(vec2(640.0, 480.0))
        .build_ui_state(
            |ui, state: &mut State| {
                let out = MapCanvas::show(ui, &mut state.doc, &mut state.view);
                assert!(out.error.is_none(), "{:?}", out.error);
            },
            State {
                doc: Document::new(Workbook::new(sheet)),
                view,
            },
        );
    h.run_steps(3);
    h
}

#[test]
fn cached_scene_is_reused_for_camera_and_rebuilt_for_content_sheet_and_layout_changes() {
    let mut h = harness();
    let first = h.state().view.minimap_cache.as_ref().unwrap();
    let first_layout = first.layout.clone();
    let first_count = first.scene.visible_count();
    assert_eq!(first_count, 1);
    h.state_mut().view.offset += vec2(30.0, 20.0);
    h.state_mut().view.set_zoom(1.5);
    h.run_steps(2);
    assert!(Arc::ptr_eq(
        &first_layout,
        &h.state().view.minimap_cache.as_ref().unwrap().layout
    ));

    h.state_mut()
        .doc
        .edit_sheet("Add another card", |sheet| {
            sheet.add_floating("Second", Vec2::new(200.0, 0.0));
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    let edited = h.state().view.minimap_cache.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&first_layout, &edited.layout));
    assert_eq!(edited.scene.visible_count(), 2);

    let mut second = Sheet::new("Other board", "");
    second.root_mode = SheetRootMode::Hidden;
    second.add_floating("Different sheet", Vec2::ZERO);
    h.state_mut().doc.add_sheet(second).unwrap();
    h.run_steps(2);
    let other_layout = h
        .state()
        .view
        .minimap_cache
        .as_ref()
        .unwrap()
        .layout
        .clone();
    assert_eq!(
        h.state()
            .view
            .minimap_cache
            .as_ref()
            .unwrap()
            .scene
            .visible_count(),
        1
    );
    let state_id = h.state().doc.state_id();
    h.state_mut().doc.set_active(0);
    h.run_steps(2);
    let switched = h.state().view.minimap_cache.as_ref().unwrap();
    assert_eq!(h.state().doc.state_id(), state_id);
    assert!(!Arc::ptr_eq(&other_layout, &switched.layout));
    assert_eq!(switched.scene.visible_count(), 2);

    let layout = switched.layout.clone();
    let dark = !h.state().view.dark;
    h.state_mut().view.dark = dark;
    h.run_steps(2);
    assert!(!Arc::ptr_eq(
        &layout,
        &h.state().view.minimap_cache.as_ref().unwrap().layout
    ));
}

#[test]
fn hidden_minimap_rebuilds_an_invalidated_cache_when_shown_again() {
    let mut h = harness();
    let old = h
        .state()
        .view
        .minimap_cache
        .as_ref()
        .unwrap()
        .layout
        .clone();
    h.state_mut().view.set_minimap_visible(false);
    h.state_mut()
        .doc
        .edit_sheet("Add card while hidden", |sheet| {
            sheet.add_floating("Added", Vec2::new(50.0, 50.0));
            Ok(())
        })
        .unwrap();
    h.run_steps(2);
    assert!(Arc::ptr_eq(
        &old,
        &h.state().view.minimap_cache.as_ref().unwrap().layout
    ));
    h.state_mut().view.set_minimap_visible(true);
    h.run_steps(2);
    let current = h.state().view.minimap_cache.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&old, &current.layout));
    assert_eq!(current.scene.visible_count(), 2);
    h.state_mut().view.reset();
    assert!(h.state().view.minimap_cache.is_none());
    assert!(h.state().view.show_minimap);
}

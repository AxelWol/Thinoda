use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::app::{Cmd, Services, ThinodaApp};
use thinoda_platform::settings::{Language, Settings};

#[test]
fn shared_search_caption_controls_preserve_state_and_close_clears_filter() {
    let mut app = ThinodaApp::new(
        Settings {
            language: Language::English,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    app.new_doc(thinoda_core::Workbook::default());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1000.0, 760.0))
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::FindReplace);
    h.run_steps(5);
    h.state_mut().search.query = "Keep query".into();
    h.state_mut().search.replace = "Keep replacement".into();
    h.state_mut().search.dim = true;
    h.run_steps(4);
    let initial = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::unique("thinoda_search")))
        .unwrap();
    let maximize_rect = h.get_by_role_and_label(Role::Button, "Maximize").rect();
    assert!(
        !maximize_rect.intersects(h.get_by_role_and_label(Role::TextInput, "Search").rect()),
        "search field overlaps caption: {:?}",
        h.get_by_role_and_label(Role::TextInput, "Search").rect()
    );
    assert_eq!(
        h.ctx
            .layer_id_at(maximize_rect.center())
            .map(|layer| layer.id),
        Some(egui::Id::unique("thinoda_search")),
        "caption obscured: {maximize_rect:?}"
    );
    h.get_by_role_and_label(Role::Button, "Maximize").click();
    h.run_steps(5);
    assert!(
        thinoda_ui::DialogState::load(&h.ctx, egui::Id::unique("thinoda_search")).maximized,
        "maximize click was not delivered"
    );
    let maximized = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::unique("thinoda_search")))
        .unwrap();
    assert!(
        maximized.width() > initial.width(),
        "{maximized:?} vs {initial:?}"
    );
    assert!(
        maximized.height() > initial.height(),
        "{maximized:?} vs {initial:?}"
    );
    h.get_by_role_and_label(Role::Button, "Restore").click();
    h.run_steps(5);
    let restored = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::unique("thinoda_search")))
        .unwrap();
    assert!((restored.size() - initial.size()).length() < 3.0);
    let start = restored.max - egui::vec2(1.0, 1.0);
    h.event(egui::Event::PointerMoved(start));
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
    let end = start - egui::vec2(80.0, 80.0);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    let resized = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::unique("thinoda_search")))
        .unwrap();
    assert!(
        resized.width() < restored.width() - 40.0,
        "{resized:?} vs {restored:?}"
    );
    assert!(
        resized.height() < restored.height() - 40.0,
        "{resized:?} vs {restored:?}"
    );
    h.get_by_role_and_label(Role::Button, "Minimize").click();
    h.run_steps(4);
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Search")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Expand").click();
    h.run_steps(4);
    assert_eq!(h.state().search.query, "Keep query");
    assert_eq!(h.state().search.replace, "Keep replacement");
    assert!(h.state().search.show_replace && h.state().search.dim);
    let root = h.state().doc().unwrap().doc.sheet().root;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Nonmodal edit", |sheet| sheet.set_title(root, "Keep query"))
        .unwrap();
    h.run_steps(3);
    assert!(h.state().search.open);
    h.get_by_role_and_label(Role::Button, "Close Window")
        .click();
    h.run_steps(4);
    assert!(!h.state().search.open);
    assert!(h.state().doc().unwrap().view.filter.is_none());
    assert_eq!(
        h.state().doc().unwrap().doc.sheet().root_topic().title,
        "Keep query"
    );
}

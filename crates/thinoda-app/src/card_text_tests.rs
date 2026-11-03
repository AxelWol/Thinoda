use super::*;
use egui::{PointerButton, Role, accesskit::Toggled};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use thinoda_core::{
    CanvasObject, Card, Rect as BoardRect, Vec2,
    board::{BoardMember, ShapeKind},
};

fn app_with_card(kind: CardKind) -> (ThinodaApp, TopicId) {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = false;
    app.new_doc(templates::instantiate("blank-canvas"));
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .edit_sheet("Create card", |sheet| {
            sheet
                .add_card(
                    "Card title",
                    Vec2::ZERO,
                    Card::new(kind, Vec2::new(180.0, 120.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    doc.view.select_only(id);
    (app, id)
}

fn harness(app: ThinodaApp, size: egui::Vec2) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    harness.run_steps(6);
    harness
}

fn open_toolbar(harness: &mut Harness<'_, ThinodaApp>) {
    harness
        .get_by_role_and_label(Role::Button, card_text_label())
        .click();
    harness.run_steps(5);
    assert!(
        harness
            .query_by_role_and_label(Role::Button, "Bold")
            .is_some()
    );
}

fn scroll_and_click(harness: &mut Harness<'_, ThinodaApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).scroll_to_me();
    harness.run_steps(4);
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(3);
}

fn open_toolbar_from_context(h: &mut Harness<'_, ThinodaApp>) {
    let pos = {
        let doc = h.state().doc().unwrap();
        doc.view.selected_topic_rect(&doc.doc).unwrap().center()
    };
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(4);
    scroll_and_click(h, Role::Button, card_text_label());
}

fn style(harness: &Harness<'_, ThinodaApp>, id: TopicId) -> TopicStyle {
    harness
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topic(id)
        .unwrap()
        .style
        .clone()
}

#[test]
fn selected_sticky_and_text_controls_edit_whole_card_and_round_trip() {
    for kind in [CardKind::Sticky, CardKind::Text] {
        let (mut app, id) = app_with_card(kind);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Existing style", |sheet| {
                sheet.update_style(&[id], |style| {
                    style.fill = Some(Color::rgb(10, 20, 30));
                    style.strikethrough = Some(true);
                })
            })
            .unwrap();
        let mut h = harness(app, vec2(1200.0, 700.0));
        let before_open = h.state().doc().unwrap().doc.state_id();
        open_toolbar(&mut h);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before_open);
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Bold")
                .accesskit_node()
                .toggled(),
            Some(Toggled::False)
        );
        h.get_by_role_and_label(Role::Button, "Bold").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).bold, Some(true));
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).bold, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.redo());
        h.run_steps(3);
        assert_eq!(style(&h, id).bold, Some(true));
        assert_eq!(
            h.get_by_role_and_label(Role::Button, "Bold")
                .accesskit_node()
                .toggled(),
            Some(Toggled::True)
        );

        h.get_by_role_and_label(Role::Button, "Italic").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Underline").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Align Left").click();
        h.run_steps(2);
        assert_eq!(style(&h, id).italic, Some(true));
        assert_eq!(style(&h, id).underline, Some(true));
        assert_eq!(style(&h, id).align, Some(TextAlign::Left));
        assert_eq!(style(&h, id).fill, Some(Color::rgb(10, 20, 30)));
        assert_eq!(style(&h, id).strikethrough, Some(true));
        h.get_by_role_and_label(Role::Button, "Align Center")
            .click();
        h.run_steps(2);
        assert_eq!(style(&h, id).align, Some(TextAlign::Center));
        h.get_by_role_and_label(Role::Button, "Align Right").click();
        h.run_steps(2);
        assert_eq!(style(&h, id).align, Some(TextAlign::Right));
        h.get_by_role_and_label(Role::Button, "Align Left").click();
        h.run_steps(2);

        scroll_and_click(&mut h, Role::ComboBox, "Font Family");
        h.get_by_label("Monospace").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
        let size = h.get_by_role_and_label(Role::SpinButton, "Font Size");
        size.scroll_to_me();
        h.run_steps(3);
        h.get_by_role_and_label(Role::SpinButton, "Font Size")
            .click();
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.get_by_role_and_label(Role::SpinButton, "Font Size")
            .type_text("24");
        h.key_press(Key::Enter);
        h.run_steps(4);
        assert_eq!(style(&h, id).font_size, Some(24.0));
        assert_eq!(h.state().doc().unwrap().view.selection, [id]);
        assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);

        scroll_and_click(&mut h, Role::ColorWell, "Text Color #E8455C");
        assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
        let book = h.state().doc().unwrap().doc.workbook();
        let bytes =
            thinoda_io::export_bytes(book, thinoda_io::Format::Thinoda, &Default::default())
                .unwrap();
        let reopened = thinoda_io::import_bytes(thinoda_io::Format::Thinoda, &bytes).unwrap();
        assert_eq!(
            reopened.active().topic(id).unwrap().style,
            book.active().topic(id).unwrap().style
        );
        let svg =
            thinoda_io::export_bytes(book, thinoda_io::Format::Svg, &Default::default()).unwrap();
        let svg = String::from_utf8(svg).unwrap();
        let line = svg
            .lines()
            .find(|line| line.contains(&format!("id=\"topic-{id}-line-0\"")))
            .unwrap();
        assert!(line.contains("font-family=\"monospace\""), "{line}");
        assert!(line.contains("font-size=\"24.00\""), "{line}");
        assert!(line.contains("font-weight=\"bold\""), "{line}");
        assert!(line.to_ascii_uppercase().contains("#E8455C"), "{line}");

        scroll_and_click(&mut h, Role::Button, "Text Color · Auto");
        assert_eq!(style(&h, id).text_color, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
        h.run_steps(3);
        open_toolbar(&mut h);
        scroll_and_click(&mut h, Role::ComboBox, "Font Family");
        h.get_by_label("Auto").click();
        h.run_steps(3);
        assert_eq!(style(&h, id).font_family, None);
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
    }
}

#[test]
fn keyboard_controls_use_theme_state_and_close_without_changing_selection() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Theme", |sheet| {
            let mut custom = thinoda_core::custom_theme::CustomTheme::from_settings(&sheet.theme);
            custom.roles.floating.bold = Some(true);
            sheet.theme.custom_theme = Some(custom);
            Ok(())
        })
        .unwrap();
    let mut h = harness(app, vec2(900.0, 600.0));
    h.get_by_role_and_label(Role::Button, card_text_label())
        .focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(5);
    let bold = h.get_by_role_and_label(Role::Button, "Bold");
    assert!(bold.is_focused());
    assert_eq!(bold.accesskit_node().toggled(), Some(Toggled::True));
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(false));
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, "Italic").is_focused());
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).italic, Some(true));
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
}

#[test]
fn keyboard_can_choose_and_reset_color_and_choose_a_font() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(640.0, 360.0));
    open_toolbar(&mut h);
    h.get_by_role_and_label(Role::ColorWell, "Text Color #E8455C")
        .focus();
    h.run_steps(5);
    h.key_press(Key::Space);
    h.run_steps(3);
    assert_eq!(style(&h, id).text_color, Some(Color::rgb(0xE8, 0x45, 0x5C)));
    h.get_by_role_and_label(Role::Button, "Text Color · Auto")
        .focus();
    h.run_steps(5);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(style(&h, id).text_color, None);

    h.get_by_role_and_label(Role::ComboBox, "Font Family")
        .focus();
    h.run_steps(4);
    h.key_press(Key::Space);
    h.run_steps(4);
    h.get_by_label("Monospace").focus();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
}

#[test]
fn formatting_stays_docked_and_keyboard_scrolls_in_compact_light_and_dark_windows() {
    for dark in [false, true] {
        for size in [vec2(320.0, 240.0), vec2(640.0, 360.0), vec2(960.0, 540.0)] {
            let (mut app, id) = app_with_card(CardKind::Text);
            app.settings.theme = if dark {
                SettingsTheme::Dark
            } else {
                SettingsTheme::Light
            };
            let mut h = harness(app, size);
            if h.query_by_role_and_label(Role::Button, card_text_label())
                .is_some()
            {
                open_toolbar(&mut h);
            } else {
                open_toolbar_from_context(&mut h);
            }
            let canvas = h.state().doc().unwrap().view.canvas_rect();
            let bold = h.get_by_role_and_label(Role::Button, "Bold").rect();
            assert!(bold.bottom() <= canvas.top() + 0.5, "{bold:?} {canvas:?}");
            assert!(h.ctx.content_rect().contains_rect(bold));
            let board = h
                .ctx
                .memory(|memory| memory.area_rect(egui::Id::unique("thinoda_board_toolbar")));
            assert!(!board.unwrap().intersects(bold));

            h.get_by_role_and_label(Role::ComboBox, "Font Family")
                .focus();
            h.run_steps(6);
            let family = h
                .get_by_role_and_label(Role::ComboBox, "Font Family")
                .rect();
            assert!(h.ctx.content_rect().contains_rect(family), "{family:?}");
            h.key_press(Key::Space);
            h.run_steps(4);
            h.get_by_label("Monospace").focus();
            h.key_press(Key::Enter);
            h.run_steps(3);
            assert_eq!(style(&h, id).font_family.as_deref(), Some("monospace"));

            let color = h.get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF");
            color.focus();
            h.run_steps(6);
            let rect = h
                .get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF")
                .rect();
            assert!(h.ctx.content_rect().contains_rect(rect), "{rect:?}");
            assert!(rect.bottom() <= h.state().doc().unwrap().view.canvas_rect().top());
            h.key_press(Key::Space);
            h.run_steps(3);
            assert_eq!(style(&h, id).text_color, Some(Color::WHITE));
        }
    }
}

#[test]
fn compact_formatting_keeps_insert_actions_without_covering_the_card() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(320.0, 240.0));
    open_toolbar_from_context(&mut h);
    assert!(h.query_by_label("Whole-card text").is_some());
    assert!(
        !h.ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .iter()
                .any(|layer| layer.id == egui::Id::unique("thinoda_editing_toolbar"))
        }),
        "the floating insert toolbar must not obscure the card while formatting"
    );
    let action = h.get_by_role_and_label(Role::Button, "More insert actions");
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    assert!(action.rect().bottom() <= canvas.top());
    action.click();
    h.run_steps(4);
    h.get_by_role_and_label(Role::Button, "Subtopic").click();
    h.run_steps(4);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .children
            .len(),
        1
    );
}

#[test]
fn formatting_overflow_keeps_marker_and_find_actions() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(320.0, 240.0));
    open_toolbar_from_context(&mut h);
    h.get_by_role_and_label(Role::Button, "More insert actions")
        .click();
    h.run_steps(4);
    scroll_and_click(&mut h, Role::Button, "Marker");
    scroll_and_click(&mut h, Role::Button, "Priority 1");
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .markers
            .iter()
            .any(|marker| marker == "priority-1")
    );
    h.run_steps(4);
    open_toolbar_from_context(&mut h);
    h.get_by_role_and_label(Role::Button, "More insert actions")
        .click();
    h.run_steps(4);
    scroll_and_click(&mut h, Role::Button, "Find and Replace…");
    assert!(h.state().search.open);
}

#[test]
fn stale_selection_document_and_locks_never_receive_an_edit() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let second = h
        .state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Second card", |sheet| {
            sheet
                .add_card(
                    "Second",
                    Vec2::new(300.0, 0.0),
                    Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    h.state_mut().doc_mut().unwrap().view.select_only(second);
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    assert_eq!(style(&h, id).bold, None);
    assert_eq!(style(&h, second).bold, None);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());

    h.state_mut().doc_mut().unwrap().view.select_only(id);
    h.run_steps(4);
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Lock card", |sheet| {
            sheet
                .set_board_members_locked(&[BoardMember::Card(id)], true)
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
    assert_eq!(style(&h, id).bold, None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert!(
        h.get_by_role_and_label(Role::Button, card_text_label())
            .accesskit_node()
            .is_disabled()
    );

    h.state_mut()
        .new_doc(templates::instantiate("blank-canvas"));
    let state = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Italic(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
}

#[test]
fn locked_group_or_member_never_enables_single_card_formatting() {
    for group_lock in [None, Some(false), Some(true)] {
        let (mut app, id) = app_with_card(CardKind::Sticky);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Group", |sheet| {
                let shape = sheet
                    .add_canvas_object(CanvasObject::shape(
                        ShapeKind::Rectangle,
                        BoardRect::from_center_size(Vec2::new(300.0, 0.0), Vec2::new(80.0, 80.0)),
                    ))
                    .map_err(thinoda_core::EditError::Invalid)?;
                let group = sheet
                    .group_board_members(&[BoardMember::Card(id), BoardMember::Object(shape)])
                    .map_err(thinoda_core::EditError::Invalid)?;
                if group_lock == Some(true) {
                    sheet
                        .set_canvas_group_locked(group, true)
                        .map_err(thinoda_core::EditError::Invalid)?;
                } else if group_lock == Some(false) {
                    sheet
                        .set_board_members_locked(&[BoardMember::Object(shape)], true)
                        .map_err(thinoda_core::EditError::Invalid)?;
                }
                Ok(())
            })
            .unwrap();
        let (target, enabled) = app.card_text_target().unwrap();
        assert!(!enabled);
        let before = app.doc().unwrap().doc.workbook().clone();
        let state = app.doc().unwrap().doc.state_id();
        assert!(
            app.apply_card_text_change(&target, CardTextChange::Underline(true))
                .is_none()
        );
        assert_eq!(app.doc().unwrap().doc.state_id(), state);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let h = harness(app, vec2(1000.0, 700.0));
        assert!(
            h.get_by_role_and_label(Role::Button, card_text_label())
                .accesskit_node()
                .is_disabled()
        );
    }
}

#[test]
fn active_title_draft_and_ime_are_not_taken_over_by_the_toolbar() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let original = h
        .state()
        .doc()
        .unwrap()
        .doc
        .sheet()
        .topic(id)
        .unwrap()
        .title
        .clone();
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), id, false);
        doc.view.set_editing_text("Pending ");
    }
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    assert_eq!(
        h.state().doc().unwrap().view.editing_text(),
        Some("Pending ")
    );
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "nihon".into(),
        active_range_chars: None,
    }));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), Some(id));
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .title,
        original
    );
}

#[test]
fn canvas_context_menu_opens_the_toolbar_even_when_topic_toolbar_is_disabled() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    app.settings.show_topic_toolbar = false;
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar_from_context(&mut h);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
}

#[test]
fn numeric_editor_escape_does_not_send_a_canvas_shortcut() {
    let (app, id) = app_with_card(CardKind::Text);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    scroll_and_click(&mut h, Role::SpinButton, "Font Size");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::SpinButton, "Font Size")
        .type_text("26");
    h.run_steps(2);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    assert_eq!(h.state().doc().unwrap().view.editing_topic(), None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_some());
}

#[test]
fn sheet_replacement_and_undo_invalidate_the_old_toolbar_owner() {
    let (mut app, id) = app_with_card(CardKind::Sticky);
    {
        let doc = app.doc_mut().unwrap();
        doc.doc.duplicate_sheet(0).unwrap();
        doc.doc.set_active(0);
        doc.view.select_only(id);
    }
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let root = target.root;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .remove_sheet(0)
        .unwrap();
    assert_eq!(h.state().doc().unwrap().doc.active_index(), 0);
    assert_eq!(h.state().doc().unwrap().doc.sheet().root, root);
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(style(&h, id).bold, None);
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    h.get_by_role_and_label(Role::Button, "Bold").click();
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, None);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    let unchanged = h.state().doc().unwrap().doc.state_id();
    let align = {
        let doc = h.state().doc().unwrap();
        thinoda_core::theme::resolve_style(doc.doc.sheet(), id, doc.view.dark).align
    };
    let label = match align {
        TextAlign::Left => "Align Left",
        TextAlign::Center => "Align Center",
        TextAlign::Right => "Align Right",
    };
    h.get_by_role_and_label(Role::Button, label).click();
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), unchanged);
    assert!(h.state().doc().unwrap().doc.can_redo());
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    h.run_steps(3);
    assert_eq!(style(&h, id).bold, Some(true));
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    assert_eq!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .toggled(),
        Some(Toggled::True)
    );
}

#[test]
fn quick_style_and_formatting_preserve_each_others_fields() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let quick = TopicStyle {
        fill: Some(Color::rgb(20, 40, 60)),
        bold: Some(true),
        italic: Some(true),
        underline: Some(true),
        font_size: Some(23.0),
        align: Some(TextAlign::Right),
        text_color: Some(Color::rgb(0xE8, 0x45, 0x5C)),
        ..Default::default()
    };
    h.state_mut().apply_quick_style(quick.clone());
    h.run_steps(4);
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    open_toolbar(&mut h);
    for label in ["Bold", "Italic", "Underline", "Align Right"] {
        assert_eq!(
            h.get_by_role_and_label(Role::Button, label)
                .accesskit_node()
                .toggled(),
            Some(Toggled::True),
            "{label}"
        );
    }
    let unchanged = h.state().doc().unwrap().doc.state_id();
    h.get_by_role_and_label(Role::Button, "Align Right").click();
    h.run_steps(3);
    scroll_and_click(&mut h, Role::ColorWell, "Text Color #E8455C");
    assert_eq!(h.state().doc().unwrap().doc.state_id(), unchanged);
    assert_eq!(style(&h, id), quick);
    scroll_and_click(&mut h, Role::Button, "Bold");
    let mut expected = quick.clone();
    expected.bold = Some(false);
    assert_eq!(style(&h, id), expected);
    assert_eq!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topic(id)
            .unwrap()
            .card
            .unwrap()
            .color,
        quick.fill
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(style(&h, id), quick);
}

#[test]
fn canceled_and_multitouch_releases_do_not_format_cards() {
    for (role, label) in [
        (Role::Button, "Bold"),
        (Role::ColorWell, "Text Color #E8455C"),
    ] {
        for (contacts, cancel) in [(1, true), (2, false), (1, false)] {
            let (app, id) = app_with_card(CardKind::Sticky);
            let mut h = harness(app, vec2(1000.0, 700.0));
            open_toolbar(&mut h);
            h.get_by_role_and_label(role, label).scroll_to_me();
            h.run_steps(4);
            let pos = h.get_by_role_and_label(role, label).rect().center();
            let before = h.state().doc().unwrap().doc.state_id();
            h.event(egui::Event::PointerMoved(pos));
            h.event(egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            for phase in [
                egui::TouchPhase::Start,
                if cancel {
                    egui::TouchPhase::Cancel
                } else {
                    egui::TouchPhase::End
                },
            ] {
                for id in 1..=contacts {
                    h.event(egui::Event::Touch {
                        device_id: egui::TouchDeviceId(1),
                        id: egui::TouchId(id),
                        phase,
                        pos: pos + vec2((id - 1) as f32 * 40.0, 0.0),
                        force: None,
                    });
                }

                if phase == egui::TouchPhase::End && contacts == 1 && !cancel {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    });
                }
                h.run_steps(2);
            }
            if contacts != 1 || cancel {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                });
            }
            h.run_steps(4);
            let activated = contacts == 1 && !cancel;
            assert_eq!(
                h.state().doc().unwrap().doc.state_id() != before,
                activated,
                "{label} contacts={contacts} cancel={cancel}"
            );
            if role == Role::Button {
                assert_eq!(style(&h, id).bold, activated.then_some(true));
            } else {
                assert_eq!(
                    style(&h, id).text_color,
                    activated.then_some(Color::rgb(0xE8, 0x45, 0x5C))
                );
            }
        }
    }
}

#[test]
fn selection_or_lock_change_between_press_and_release_rejects_the_click() {
    for lock in [false, true] {
        let (mut app, first) = app_with_card(CardKind::Sticky);
        let second = app
            .doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Second card", |sheet| {
                sheet
                    .add_card(
                        "Other",
                        Vec2::new(300.0, 0.0),
                        Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                    )
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        let mut h = harness(app, vec2(1000.0, 700.0));
        open_toolbar(&mut h);
        let pos = h
            .get_by_role_and_label(Role::Button, "Bold")
            .rect()
            .center();
        h.hover_at(pos);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
        if lock {
            h.state_mut()
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Lock card", |sheet| {
                    sheet
                        .set_board_members_locked(&[BoardMember::Card(first)], true)
                        .map_err(thinoda_core::EditError::Invalid)
                })
                .unwrap();
        } else {
            h.state_mut().doc_mut().unwrap().view.select_only(second);
        }
        let before_release = h.state().doc().unwrap().doc.state_id();
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(4);
        assert_eq!(h.state().doc().unwrap().doc.state_id(), before_release);
        assert_eq!(style(&h, first).bold, None);
        assert_eq!(style(&h, second).bold, None);
        assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
    }
}

#[test]
fn pending_ink_invalidates_the_formatting_target() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .view
        .set_board_tool(BoardTool::Pen);
    h.run_steps(2);
    let canvas = h.state().doc().unwrap().view.canvas_rect();
    let pos = canvas.left_top() + vec2(canvas.width() * 0.3, canvas.height() * 0.7);
    h.hover_at(pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(2);
    assert!(h.state().doc().unwrap().view.has_pending_ink_gesture());
    assert_eq!(h.state().doc().unwrap().view.selection, [id]);
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert_eq!(style(&h, id).bold, None);
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .is_disabled()
    );
    let end = pos + vec2(40.0, 20.0);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(2);
    assert_eq!(h.state().doc().unwrap().view.canvas_rect(), canvas);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert!(matches!(
        h.state().doc().unwrap().doc.sheet().canvas.objects[0].kind,
        thinoda_core::CanvasObjectKind::Ink(_)
    ));
    assert_eq!(style(&h, id).bold, None);
}

#[test]
fn active_card_resize_cannot_receive_formatting_or_lose_its_gesture() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let handle = h.get_by_label("Resize board item").rect().center();
    let end = handle + vec2(45.0, 30.0);
    h.hover_at(handle);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: handle,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    let doc = h.state().doc().unwrap();
    assert_ne!(doc.doc.workbook(), &before);
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, None);
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn escape_cancels_card_resize_before_formatting_handles_it() {
    let (app, _) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let before = h.state().doc().unwrap().doc.workbook().clone();
    let handle = h.get_by_label("Resize board item").rect().center();
    let end = handle + vec2(45.0, 30.0);
    h.hover_at(handle);
    h.run_steps(1);
    h.event(egui::Event::PointerButton {
        pos: handle,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
    h.event(egui::Event::PointerMoved(end));
    h.run_steps(3);
    assert!(h.state().doc().unwrap().view.has_active_canvas_gesture());
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: end,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
}

#[test]
fn pending_inspector_draft_is_preserved_and_blocks_formatting() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    {
        let doc = h.state_mut().doc_mut().unwrap();
        doc.inspector_edit = Some(crate::doc::InspectorEdit {
            buffer_id: None,
            sheet_index: doc.doc.active_index(),
            sheet_root: doc.doc.sheet().root,
            topic: id,
            field: crate::doc::InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Pending note draft".into(),
        });
    }
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(3);
    let doc = h.state().doc().unwrap();
    assert_eq!(doc.doc.state_id(), before);
    assert_eq!(doc.doc.sheet().topic(id).unwrap().style.bold, None);
    assert_eq!(
        doc.inspector_edit.as_ref().unwrap().text,
        "Pending note draft"
    );
    assert!(
        h.get_by_role_and_label(Role::Button, "Bold")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn multiple_selection_and_image_cards_do_not_offer_text_formatting() {
    let (app, _) = app_with_card(CardKind::Image);
    assert!(app.card_text_target().is_none());
    let h = harness(app, vec2(1000.0, 700.0));
    assert!(
        h.query_by_role_and_label(Role::Button, card_text_label())
            .is_none()
    );

    let (mut app, first) = app_with_card(CardKind::Sticky);
    let doc = app.doc_mut().unwrap();
    let second = doc
        .doc
        .edit_sheet("Second card", |sheet| {
            sheet
                .add_card(
                    "Other",
                    Vec2::new(300.0, 0.0),
                    Card::new(CardKind::Text, Vec2::new(160.0, 90.0)),
                )
                .map_err(thinoda_core::EditError::Invalid)
        })
        .unwrap();
    doc.view.selection = vec![first, second];
    assert!(app.card_text_target().is_none());
    let h = harness(app, vec2(1000.0, 700.0));
    assert!(
        h.query_by_role_and_label(Role::Button, card_text_label())
            .is_none()
    );
}

#[test]
fn deleted_selected_card_invalidates_the_toolbar() {
    let (app, id) = app_with_card(CardKind::Sticky);
    let mut h = harness(app, vec2(1000.0, 700.0));
    open_toolbar(&mut h);
    let target = h.state().card_text_target().unwrap().0;
    h.state_mut()
        .doc_mut()
        .unwrap()
        .doc
        .edit_sheet("Delete card", |sheet| sheet.remove_topics(&[id]))
        .unwrap();
    let before = h.state().doc().unwrap().doc.state_id();
    assert!(
        h.state_mut()
            .apply_card_text_change(&target, CardTextChange::Bold(true))
            .is_none()
    );
    h.run_steps(3);
    assert_eq!(h.state().doc().unwrap().doc.state_id(), before);
    assert!(h.state().doc().unwrap().doc.sheet().topic(id).is_none());
    assert!(h.query_by_role_and_label(Role::Button, "Bold").is_none());
}

#[test]
fn logical_viewports_keep_independent_toolbar_owners() {
    let (mut app, root_card) = app_with_card(CardKind::Sticky);
    let root_key = app.doc().unwrap().key.clone();
    app.new_doc(templates::instantiate("blank-canvas"));
    let child_card = {
        let doc = app.doc_mut().unwrap();
        let id = doc
            .doc
            .edit_sheet("Create card", |sheet| {
                sheet
                    .add_card(
                        "Other window",
                        Vec2::ZERO,
                        Card::new(CardKind::Text, Vec2::new(180.0, 120.0)),
                    )
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        doc.view.select_only(id);
        id
    };
    let child_key = app.doc().unwrap().key.clone();
    let child_viewport = egui::ViewportId::from_hash_of("card-text-detached");
    app.document_windows
        .insert(child_key.clone(), child_viewport);
    app.window_active_documents
        .insert(child_viewport, child_key.clone());
    app.window_active_documents
        .insert(egui::ViewportId::ROOT, root_key.clone());
    let ctx = Context::default();
    app.activate_document_window(egui::ViewportId::ROOT);
    let root_target = app.card_text_target().unwrap().0;
    ThinodaApp::open_card_text_toolbar(&ctx, egui::ViewportId::ROOT, root_target.clone());
    app.activate_document_window(child_viewport);
    let child_target = app.card_text_target().unwrap().0;
    ThinodaApp::open_card_text_toolbar(&ctx, child_viewport, child_target.clone());
    for _ in 0..4 {
        for viewport in [egui::ViewportId::ROOT, child_viewport] {
            app.current_viewport = viewport;
            app.activate_document_window(viewport);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1000.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    app.card_text_toolbar(ui);
                },
            );
            output.textures_delta.clear();
        }
    }
    assert_eq!(
        ctx.data(|data| data.get_temp::<CardTextTarget>(card_text_slot(egui::ViewportId::ROOT))),
        Some(root_target.clone())
    );
    assert_eq!(
        ctx.data(|data| data.get_temp::<CardTextTarget>(card_text_slot(child_viewport))),
        Some(child_target.clone())
    );
    app.activate_document_window(child_viewport);
    assert!(
        app.apply_card_text_change(&root_target, CardTextChange::Bold(true))
            .is_none()
    );
    assert!(
        app.apply_card_text_change(&child_target, CardTextChange::Bold(true))
            .is_some()
    );
    assert_eq!(
        app.doc()
            .unwrap()
            .doc
            .sheet()
            .topic(child_card)
            .unwrap()
            .style
            .bold,
        Some(true)
    );
    let root_doc = app.docs.iter().find(|doc| doc.key == root_key).unwrap();
    assert_eq!(
        root_doc.doc.sheet().topic(root_card).unwrap().style.bold,
        None
    );
}

#[cfg(feature = "gpu-tests")]
#[test]
fn card_text_visual_previews() {
    let Some(directory) = std::env::var_os("THINODA_CARD_TEXT_VISUAL_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    for dark in [false, true] {
        for (size, width) in [
            (vec2(1200.0, 700.0), "wide"),
            (vec2(320.0, 240.0), "compact"),
        ] {
            let (mut app, _) = app_with_card(CardKind::Sticky);
            app.settings.theme = if dark {
                SettingsTheme::Dark
            } else {
                SettingsTheme::Light
            };
            let mut h = Harness::builder()
                .wgpu()
                .with_size(size)
                .with_step_dt(1.0 / 60.0)
                .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
            h.run_steps(6);
            if h.query_by_role_and_label(Role::Button, card_text_label())
                .is_some()
            {
                open_toolbar(&mut h);
            } else {
                open_toolbar_from_context(&mut h);
            }
            let theme = if dark { "dark" } else { "light" };
            h.render()
                .expect("formatting toolbar render")
                .save(directory.join(format!("card-text-{theme}-{width}.png")))
                .unwrap();
            h.get_by_role_and_label(Role::ColorWell, "Text Color #FFFFFF")
                .scroll_to_me();
            h.run_steps(5);
            h.render()
                .expect("formatting palette render")
                .save(directory.join(format!("card-color-{theme}-{width}.png")))
                .unwrap();
        }
    }
}

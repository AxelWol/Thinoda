//! One document-bound draft for every label-editing entry point.

use std::collections::BTreeSet;

use egui::{Context, Id, Key, Modifiers, Rect};
use thinoda_core::{TopicId, edit::normalize_labels};

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

pub(crate) struct LabelEditor {
    key: String,
    sheet: usize,
    state: u64,
    topic: TopicId,
    labels: Vec<String>,
    input: String,
    ime_composing: bool,
    focus: bool,
    auto_sort: bool,
    anchor: Rect,
}

impl LabelEditor {
    fn normalize(&mut self) {
        self.labels = normalize_labels(std::mem::take(&mut self.labels));
        if self.auto_sort {
            self.labels.sort_unstable();
        }
    }

    fn add_input(&mut self) {
        self.labels
            .extend(self.input.split(',').map(str::to_string));
        self.input.clear();
        self.normalize();
        self.focus = true;
    }
}

impl ThinodaApp {
    pub(crate) fn protect_labels_ime(&mut self, ctx: &Context) {
        let Some(editor) = self.label_editor.as_mut() else {
            return;
        };
        let input_id = Id::unique("topic-label-input");
        if editor.ime_composing || ctx.memory(|memory| memory.has_focus(input_id)) {
            thinoda_canvas::protect_ime_candidate_keys(ctx, input_id, &mut editor.ime_composing);
        } else {
            editor.ime_composing = false;
        }
    }

    pub(crate) fn begin_labels(&mut self, ctx: &Context) {
        if !self.command_available(&Cmd::Labels) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        let auto_sort = self.settings.label_auto_sort;
        let d = self.doc_mut().unwrap();
        d.view.commit_edit(&mut d.doc);
        let topic = d.view.primary().unwrap();
        let labels = d.doc.sheet().topic(topic).unwrap().labels.clone();
        let anchor = d
            .view
            .selected_topic_rect(&d.doc)
            .filter(|rect| ctx.content_rect().contains_rect(*rect))
            .unwrap_or_else(|| {
                Rect::from_center_size(ctx.content_rect().center(), egui::Vec2::ZERO)
            });
        self.label_editor = Some(LabelEditor {
            key: d.key.clone(),
            sheet: d.doc.active_index(),
            state: d.doc.state_id(),
            topic,
            labels,
            input: String::new(),
            ime_composing: false,
            focus: true,
            auto_sort,
            anchor,
        });
        egui::Popup::open_id(ctx, Id::unique("topic-label-editor"));
    }

    pub(crate) fn labels_popup(&mut self, ctx: &Context) {
        let Some(mut editor) = self.label_editor.take() else {
            return;
        };
        let valid = self.doc().is_some_and(|d| {
            d.key == editor.key
                && d.doc.active_index() == editor.sheet
                && d.doc.state_id() == editor.state
                && d.view.selection == [editor.topic]
                && d.doc.sheet().contains(editor.topic)
        });
        let id = Id::unique("topic-label-editor");
        if !valid {
            egui::Popup::close_id(ctx, id);
            self.toasts.warning(t(
                "Label editing canceled because the document, selection or labels changed",
            ));
            ctx.input_mut(|i| {
                i.consume_key(Modifiers::NONE, Key::Escape);
            });
            return;
        }
        let suggestions: BTreeSet<String> = self
            .doc()
            .unwrap()
            .doc
            .sheet()
            .topics
            .values()
            .flat_map(|topic| topic.labels.iter().cloned())
            .collect();
        let width = 340.0_f32.min((ctx.content_rect().width() - 32.0).max(140.0));
        editor.normalize();
        let mut apply = false;
        let mut cancel = false;
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        // Consume Escape before Popup closes so it cannot reach the map/outliner.
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let popup = egui::Popup::new(id, ctx.clone(), editor.anchor, egui::LayerId::background())
            .open_memory(None)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(width)
            .gap(10.0)
            .show(|ui| {
                ui.set_width(width);
                ui.heading(t("Labels"));
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() * 0.45).min(320.0))
                    .show(ui, |ui| label_fields(ui, &mut editor, &suggestions));
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    cancel = ui.button(t("Cancel")).clicked() && touch_allowed;
                    apply = ui.button(t("Apply")).clicked() && touch_allowed;
                });
            });
        if let Some(popup) = popup
            && editor.anchor.width() > 0.0
        {
            pointer(ctx, &popup.response, editor.anchor);
        }
        if !touch_allowed && !escape {
            egui::Popup::open_id(ctx, id);
        }
        if apply {
            editor.add_input();
            let result = self
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Labels", |s| s.set_labels(editor.topic, editor.labels));
            if let Err(e) = result {
                self.toasts.error(e.to_string());
            } else {
                self.settings.label_auto_sort = editor.auto_sort;
                self.save_settings();
            }
            egui::Popup::close_id(ctx, id);
        } else if !egui::Popup::is_id_open(ctx, id) || cancel || escape {
            egui::Popup::close_id(ctx, id);
        } else {
            self.label_editor = Some(editor);
        }
        if apply || cancel || escape {
            if self.mode == crate::app::CenterMode::Map {
                ctx.memory_mut(|m| m.request_focus(thinoda_canvas::MapCanvas::id()));
            } else if let Some(focused) = ctx.memory(|m| m.focused()) {
                ctx.memory_mut(|m| m.surrender_focus(focused));
            }
        }
    }
}

fn label_fields(ui: &mut egui::Ui, editor: &mut LabelEditor, suggestions: &BTreeSet<String>) {
    let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
    let auto_sort = editor.auto_sort;
    ui.checkbox(&mut editor.auto_sort, t("Auto sort"))
        .on_hover_text(t(
            "Sort labels by Unicode code-point order (case-sensitive).",
        ));
    if !touch_allowed {
        editor.auto_sort = auto_sort;
    }
    if !editor.auto_sort {
        ui.label(t(
            "Label order is kept. Matching is case-sensitive; empty and duplicate labels are removed.",
        ));
    }
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (index, label) in editor.labels.iter().enumerate() {
            if ui
                .add(egui::Button::new(format!("{label} ×")).wrap())
                .accessible_name(format!("{}: {label}", t("Remove label")))
                .clicked()
                && touch_allowed
            {
                remove = Some(index);
            }
        }
    });
    if let Some(index) = remove {
        editor.labels.remove(index);
    }
    let input_id = Id::unique("topic-label-input");
    let enter = ui.memory(|m| m.has_focus(input_id))
        && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    let label = ui.label(t("New labels"));
    let input = ui
        .add(
            egui::TextEdit::singleline(&mut editor.input)
                .id(input_id)
                .lock_focus(editor.ime_composing)
                .hint_text(t("comma separated"))
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id);
    if std::mem::take(&mut editor.focus) {
        input.request_focus();
    }
    if !input.has_focus() {
        editor.ime_composing = false;
    }
    if (ui
        .add_enabled(
            !editor.input.trim().is_empty(),
            egui::Button::new(t("Add labels")),
        )
        .clicked()
        || enter)
        && touch_allowed
    {
        editor.add_input();
    }
    let query = editor.input.trim().to_lowercase();
    let available: Vec<_> = suggestions
        .iter()
        .filter(|label| !editor.labels.contains(label))
        .filter(|label| query.is_empty() || label.to_lowercase().contains(&query))
        .collect();
    if !available.is_empty() {
        ui.label(t("Labels on this sheet"));
        ui.horizontal_wrapped(|ui| {
            for label in available {
                if ui
                    .add(egui::Button::new(label).wrap())
                    .accessible_name(format!("{}: {label}", t("Use label")))
                    .clicked()
                    && touch_allowed
                {
                    editor.labels.push(label.clone());
                }
            }
        });
    }
}

fn pointer(ctx: &Context, response: &egui::Response, anchor: Rect) {
    let rect = response.rect;
    let x = anchor
        .center()
        .x
        .clamp(rect.left() + 12.0, rect.right() - 12.0);
    let edge = if rect.top() >= anchor.bottom() {
        Some((rect.top(), -8.0))
    } else if rect.bottom() <= anchor.top() {
        Some((rect.bottom(), 8.0))
    } else {
        None
    };
    if let Some((y, offset)) = edge {
        ctx.layer_painter(response.layer_id)
            .add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(x - 8.0, y),
                    egui::pos2(x, y + offset),
                    egui::pos2(x + 8.0, y),
                ],
                ctx.global_style().visuals.window_fill,
                egui::Stroke::NONE,
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CenterMode;
    use egui::{PointerButton, accesskit::Role};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_canvas::Action;

    fn harness() -> Harness<'static, ThinodaApp> {
        let mut app = ThinodaApp::headless();
        app.settings.show_topic_toolbar = false;
        let d = app.doc_mut().unwrap();
        let root = d.doc.sheet().root;
        d.view.select_only(root);
        d.doc
            .edit_sheet("Seed labels", |s| {
                s.set_labels(root, vec!["Existing".into(), "a,b".into()])?;
                let other = s.insert_child(root, "Suggested labels")?;
                s.set_labels(other, vec!["Shared".into(), "a,b".into()])
            })
            .unwrap();
        d.doc.mark_saved();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1100.0, 800.0))
            .with_context_setup(|ctx| {
                ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
            })
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        h
    }

    fn open(h: &mut Harness<'_, ThinodaApp>) {
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Labels);
        h.run_steps(4);
        assert!(h.state().label_editor.is_some());
        h.get_by_label("New labels");
    }

    #[test]
    fn label_ime_candidate_keys_do_not_add_or_discard_drafts() {
        use egui::{Event, ImeEvent};
        for mode in [CenterMode::Map, CenterMode::Outliner] {
            for key in [Key::Enter, Key::Tab, Key::Escape] {
                for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                    let mut h = harness();
                    h.ctx
                        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
                    h.state_mut().mode = mode;
                    let before = h.state().doc().unwrap().doc.workbook().clone();
                    open(&mut h);
                    let labels = h.state().label_editor.as_ref().unwrap().labels.clone();
                    h.event(Event::Ime(ImeEvent::Preedit {
                        text: "nihon".into(),
                        active_range_chars: Some(0..5),
                    }));
                    h.run_steps(3);
                    assert!(h.state().label_editor.as_ref().unwrap().ime_composing);
                    assert!(h.get_by_label("New labels").is_focused());
                    h.key_press_modifiers(modifiers, key);
                    h.run_steps(3);
                    let editor = h.state().label_editor.as_ref().unwrap_or_else(|| {
                        panic!("{mode:?} {modifiers:?}+{key:?} closed the draft")
                    });
                    assert_eq!(
                        editor.labels, labels,
                        "{modifiers:?}+{key:?} added a candidate"
                    );
                    assert!(h.get_by_label("New labels").is_focused());
                    h.input_mut().events.extend([
                        Event::Ime(ImeEvent::Commit("日本".into())),
                        Event::Key {
                            key: Key::Enter,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        },
                    ]);
                    h.run_steps(3);
                    let editor = h.state().label_editor.as_ref().unwrap();
                    assert_eq!(editor.input, "日本");
                    assert_eq!(editor.labels, labels);
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                    h.key_press(Key::Enter);
                    h.run_steps(3);
                    assert!(h.state().label_editor.as_ref().unwrap().input.is_empty());
                    assert_eq!(
                        h.state()
                            .label_editor
                            .as_ref()
                            .unwrap()
                            .labels
                            .last()
                            .unwrap(),
                        "日本"
                    );
                    h.get_by_role_and_label(Role::Button, "Apply").click();
                    h.run_steps(3);
                    let ctx = h.ctx.clone();
                    h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
                    assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
                }
            }
        }
    }

    #[test]
    fn label_ime_dismissal_restores_tab_navigation_and_escape_cancel() {
        use egui::{Event, ImeEvent};
        for mode in [CenterMode::Map, CenterMode::Outliner] {
            for cancellation in [
                ImeEvent::Preedit {
                    text: String::new(),
                    active_range_chars: None,
                },
                ImeEvent::Commit(String::new()),
            ] {
                let mut h = harness();
                h.state_mut().mode = mode;
                let before = h.state().doc().unwrap().doc.workbook().clone();
                open(&mut h);
                h.get_by_label("New labels").type_text("Keep");
                h.run_steps(2);
                h.event(Event::Ime(ImeEvent::Preedit {
                    text: "candidate".into(),
                    active_range_chars: None,
                }));
                h.run_steps(3);
                h.input_mut().events.extend([
                    Event::Ime(cancellation),
                    Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ]);
                h.run_steps(3);
                let editor = h.state().label_editor.as_ref().unwrap();
                assert!(!editor.ime_composing);
                assert_eq!(editor.input, "Keep");
                h.key_press(Key::Tab);
                h.run_steps(3);
                assert!(
                    h.get_by_role_and_label(Role::Button, "Add labels")
                        .is_focused()
                );
                h.key_press(Key::Escape);
                h.run_steps(3);
                assert!(h.state().label_editor.is_none());
                assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            }
        }
    }

    #[test]
    fn labels_draft_suggestions_removal_normalization_one_undo_and_roundtrip() {
        let mut h = harness();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let state = h.state().doc().unwrap().doc.state_id();
        open(&mut h);
        h.get_by_label("Remove label: Existing").click();
        h.run_steps(2);
        h.get_by_label("Use label: Shared").click();
        h.run_steps(2);
        h.get_by_label("New labels").click();
        h.run_steps(1);
        h.get_by_label("New labels")
            .type_text(" Alpha, Alpha, alpha, , ");
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert_eq!(
            h.state().label_editor.as_ref().unwrap().labels,
            ["a,b", "Shared", "Alpha", "alpha"]
        );
        assert!(h.state().label_editor.as_ref().unwrap().input.is_empty());
        assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.get_by_role_and_label(Role::Button, "Apply").click();
        h.run_steps(3);
        assert!(h.state().label_editor.is_none());
        let applied = h.state().doc().unwrap().doc.workbook().clone();
        assert_ne!(applied, before);
        for format in [thinoda_io::Format::Thinoda, thinoda_io::Format::XMind] {
            let bytes = thinoda_io::export_bytes(&applied, format, &Default::default()).unwrap();
            assert_eq!(thinoda_io::import_bytes(format, &bytes).unwrap(), applied);
        }
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Undo));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.state_mut().exec(&ctx, Cmd::Canvas(Action::Redo));
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &applied);
    }

    #[test]
    fn labels_escape_cancel_outside_and_empty_apply_preserve_document_and_outline() {
        let mut h = harness();
        h.state_mut().mode = CenterMode::Outliner;
        let before = h.state().doc().unwrap().doc.workbook().clone();
        let state = h.state().doc().unwrap().doc.state_id();
        for how in 0..4 {
            open(&mut h);
            if how != 3 {
                h.get_by_label("New labels").type_text("Discard me");
                h.run_steps(2);
            }
            match how {
                0 => h.key_press(Key::Escape),
                1 => h.get_by_role_and_label(Role::Button, "Cancel").click(),
                2 => {
                    let pos = egui::pos2(20.0, 400.0);
                    h.hover_at(pos);
                    h.run_steps(1);
                    for pressed in [true, false] {
                        h.event(egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        });
                        h.run_steps(1);
                    }
                }
                _ => h.get_by_role_and_label(Role::Button, "Apply").click(),
            }
            h.run_steps(3);
            assert!(h.state().label_editor.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), state);
        }
        open(&mut h);
        h.key_press(Key::Enter);
        h.run_steps(3);
        assert!(h.state().label_editor.is_some());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }

    #[test]
    fn labels_stale_document_sheet_selection_and_content_are_never_written() {
        for change in 0..5 {
            let mut h = harness();
            open(&mut h);
            h.get_by_label("New labels").type_text("Must not apply");
            h.run_steps(2);
            let ctx = h.ctx.clone();
            match change {
                0 => h.state_mut().exec(&ctx, Cmd::New),
                1 => h.state_mut().doc_mut().unwrap().view.selection.clear(),
                2 => {
                    let d = h.state_mut().doc_mut().unwrap();
                    let root = d.doc.sheet().root;
                    d.doc
                        .edit_sheet("Other edit", |s| s.set_title(root, "Edited elsewhere"))
                        .unwrap();
                }
                3 => {
                    let d = h.state_mut().doc_mut().unwrap();
                    d.doc.add_sheet(thinoda_core::Sheet::new("Other", "Other"));
                    d.doc.set_active(1);
                    let root = d.doc.sheet().root;
                    d.view.select_only(root);
                }
                _ => {
                    let d = h.state_mut().doc_mut().unwrap();
                    let root = d.doc.sheet().root;
                    d.doc
                        .edit_sheet("External labels", |s| {
                            s.set_labels(root, vec!["Newer".into()])
                        })
                        .unwrap();
                }
            }
            let before = h.state().doc().unwrap().doc.workbook().clone();
            h.run_steps(3);
            assert!(h.state().label_editor.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
            h.get_by_label(
                "Label editing canceled because the document, selection or labels changed",
            );
        }
    }

    #[test]
    fn labels_insert_menu_sidebar_and_availability_use_one_editor() {
        let mut h = harness();
        h.get_by_label(crate::i18n::t("Main Menu")).click();
        h.run_steps(2);
        h.get_by_label("Insert").click();
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Labels").click();
        h.run_steps(4);
        assert!(h.state().label_editor.is_some());
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Edit labels")
            .scroll_to_me();
        h.run_steps(6);
        h.get_by_role_and_label(Role::Button, "Edit labels").click();
        h.run_steps(4);
        assert!(h.state().label_editor.is_some());
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(
            ThinodaApp::palette_commands()
                .iter()
                .any(|(cmd, _)| matches!(cmd, Cmd::Labels))
        );
        h.state_mut().doc_mut().unwrap().view.selection.clear();
        assert!(!h.state().command_available(&Cmd::Labels));
        let d = h.state_mut().doc_mut().unwrap();
        d.view.selection = d.doc.sheet().topics.keys().copied().take(2).collect();
        assert!(!h.state().command_available(&Cmd::Labels));
        h.state_mut().active = usize::MAX;
        assert!(!h.state().command_available(&Cmd::Labels));
    }

    #[test]
    fn labels_auto_sort_is_opt_in_draft_only_and_persisted_on_apply() {
        let mut h = harness();
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = thinoda_platform::Paths::from_root(dir.path());
        h.state_mut().services.paths = Some(paths.clone());
        assert!(!h.state().settings.label_auto_sort);
        let before = h.state().doc().unwrap().doc.workbook().clone();
        open(&mut h);
        h.get_by_label("Auto sort").click();
        h.run_steps(3);
        h.get_by_role_and_label(Role::Button, "Cancel").click();
        h.run_steps(3);
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        assert!(!h.state().settings.label_auto_sort);
        open(&mut h);
        h.get_by_label("Auto sort").click();
        h.run_steps(3);
        h.get_by_label("New labels").click();
        h.run_steps(1);
        h.get_by_label("New labels").type_text(" ä, Z, z, A, Z ");
        h.run_steps(2);
        h.get_by_role_and_label(Role::Button, "Apply").click();
        h.run_steps(3);
        let d = h.state().doc().unwrap();
        assert_eq!(
            d.doc
                .sheet()
                .topic(d.view.primary().unwrap())
                .unwrap()
                .labels,
            ["A", "Existing", "Z", "a,b", "z", "ä"]
        );
        assert!(h.state().settings.label_auto_sort);
        assert!(
            thinoda_platform::settings::Settings::load(&paths)
                .unwrap()
                .label_auto_sort
        );
        open(&mut h);
        assert!(h.state().label_editor.as_ref().unwrap().auto_sort);
    }

    #[test]
    fn labels_popup_stays_inside_small_high_dpi_viewport_in_both_appearances() {
        for dark in [false, true] {
            let mut h = harness();
            h.state_mut().settings.show_format_panel = false;
            h.state_mut().zen = true;
            h.state_mut().settings.theme = if dark {
                thinoda_platform::settings::ThemeMode::Dark
            } else {
                thinoda_platform::settings::ThemeMode::Light
            };
            h.ctx.set_pixels_per_point(2.0);
            h.set_size(egui::vec2(360.0, 420.0));
            h.run_steps(5);
            open(&mut h);
            assert_eq!(h.ctx.theme() == egui::Theme::Dark, dark);
            assert!(
                h.ctx.content_rect().contains_rect(
                    h.ctx
                        .read_response(Id::unique("topic-label-editor"))
                        .unwrap()
                        .rect
                )
            );
            for name in ["New labels", "Cancel", "Apply"] {
                let rect = h.get_by_label(name).rect();
                assert!(h.ctx.content_rect().contains_rect(rect), "{name}: {rect:?}");
            }
        }
    }

    #[test]
    fn labels_long_chips_and_many_suggestions_keep_footer_in_view() {
        let mut h = harness();
        h.state_mut().zen = true;
        let d = h.state_mut().doc_mut().unwrap();
        let root = d.doc.sheet().root;
        d.doc
            .edit_sheet("Long label", |s| {
                s.set_labels(root, vec!["x".repeat(500)])?;
                let child = s.insert_child(root, "Label inventory")?;
                s.set_labels(child, (0..100).map(|i| format!("Suggestion {i}")).collect())
            })
            .unwrap();
        h.set_size(egui::vec2(360.0, 420.0));
        h.run_steps(4);
        open(&mut h);
        assert!(
            h.ctx.content_rect().contains_rect(
                h.ctx
                    .read_response(Id::unique("topic-label-editor"))
                    .unwrap()
                    .rect
            )
        );
        for name in ["Cancel", "Apply"] {
            assert!(
                h.ctx
                    .content_rect()
                    .contains_rect(h.get_by_label(name).rect())
            );
        }
        h.get_by_label("New labels").scroll_to_me();
        h.run_steps(6);
        h.get_by_label("New labels").click();
        h.run_steps(1);
        h.get_by_label("New labels")
            .type_text("Visible after scrolling");
        h.run_steps(2);
        h.get_by_label("Apply").click();
        h.run_steps(3);
        let d = h.state().doc().unwrap();
        assert_eq!(
            d.doc.sheet().topic(root).unwrap().labels,
            ["x".repeat(500), "Visible after scrolling".to_string()]
        );
    }
}

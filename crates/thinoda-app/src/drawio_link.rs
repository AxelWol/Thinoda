//! Explicit, offline import of self-contained diagrams.net published links.

use egui::{Context, Id, TextEdit, ViewportId};
use thinoda_ui::{Dialog, widgets};

use crate::{ThinodaApp, doc::OpenDoc, i18n, i18n::t};

pub(crate) const TITLE: &str = "Import draw.io link…";
pub(crate) const LINK_LABEL: &str = "draw.io link or #R fragment";
pub(crate) const HELP: &str = "Paste a self-contained link. The link contains the diagram and is decoded locally; no website is contacted.";
pub(crate) const IMPORT_ERROR: &str = "Could not import draw.io link";
pub(crate) const TOO_LARGE: &str = "This draw.io link exceeds the supported size limit.";
pub(crate) const SOURCE_NOTICE: &str =
    "This import may omit unsupported content. Your original link is unchanged.";

pub(crate) struct LinkDraft {
    owner: ViewportId,
    text: String,
    error: Option<String>,
    focus_requested: bool,
    ime_composing: bool,
}

fn input_id() -> Id {
    Id::unique("thinoda_drawio_link_input")
}

fn clear_input_state(ctx: &Context) {
    egui::text_edit::TextEditState::default().store(ctx, input_id());
    ctx.memory_mut(|memory| memory.surrender_focus(input_id()));
}

fn selected_bytes(ctx: &Context, text: &str) -> usize {
    let Some(range) = egui::TextEdit::load_state(ctx, input_id())
        .and_then(|state| state.cursor.char_range())
        .map(|range| range.as_sorted_char_range())
    else {
        return 0;
    };
    let offset = |index| {
        text.char_indices()
            .nth(index)
            .map_or(text.len(), |(byte, _)| byte)
    };
    offset(range.end.0).saturating_sub(offset(range.start.0))
}

fn input_insertion(event: &egui::Event, text_focused: bool) -> Option<(usize, bool)> {
    match event {
        egui::Event::Text(text) | egui::Event::Paste(text) => Some((text.len(), false)),
        egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => Some((text.len(), true)),
        egui::Event::Ime(egui::ImeEvent::Commit(text)) => Some((text.len(), false)),
        egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            modifiers,
            ..
        } if text_focused && modifiers.matches_logically(egui::Modifiers::NONE) => Some((1, false)),
        egui::Event::AccessKitActionRequest(request)
            if request.action == egui::accesskit::Action::ReplaceSelectedText
                && request.target_node == input_id().accesskit_id()
                && request.target_tree == egui::accesskit::TreeId::ROOT =>
        {
            match &request.data {
                Some(egui::accesskit::ActionData::Value(text)) => Some((text.len(), false)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn reject_oversized_input(
    events: &mut Vec<egui::Event>,
    current_bytes: usize,
    selected_bytes: usize,
    limit: usize,
    text_focused: bool,
) -> bool {
    let mut estimated = current_bytes.saturating_sub(selected_bytes);
    let mut composition_bytes = 0usize;
    let mut rejected = false;
    events.retain(|event| {
        let Some((length, preedit)) = input_insertion(event, text_focused) else {
            return true;
        };
        let base = estimated.saturating_sub(composition_bytes);
        let allowed = base.checked_add(length).is_some_and(|size| size <= limit);
        if allowed {
            estimated = base + length;
            composition_bytes = if preedit { length } else { 0 };
        }
        rejected |= !allowed;
        allowed
    });
    rejected
}

impl ThinodaApp {
    pub(crate) fn begin_drawio_link(&mut self, ctx: &Context) {
        clear_input_state(ctx);
        self.dialogs.drawio_link = Some(LinkDraft {
            owner: self.current_viewport,
            text: String::new(),
            error: None,
            focus_requested: false,
            ime_composing: false,
        });
    }

    pub(crate) fn protect_drawio_link_ime(&mut self, ctx: &Context) {
        if let Some(draft) = &mut self.dialogs.drawio_link
            && draft.owner == self.current_viewport
        {
            thinoda_canvas::protect_ime_candidate_keys(ctx, input_id(), &mut draft.ime_composing);
        }
    }

    pub(crate) fn return_drawio_import_viewport(&mut self, viewport: ViewportId) {
        if viewport == ViewportId::ROOT {
            return;
        }
        if let Some(draft) = &mut self.dialogs.drawio_link
            && draft.owner == viewport
        {
            draft.owner = ViewportId::ROOT;
            draft.focus_requested = false;
            draft.ime_composing = false;
        }
        for (_, _, _, owner) in &mut self.dialogs.imports {
            if *owner == Some(viewport) {
                *owner = Some(ViewportId::ROOT);
            }
        }
    }

    fn import_drawio_link_text(&mut self, text: &str) -> Result<(), thinoda_io::IoError> {
        let report = thinoda_io::import_drawio_link(text)?;
        let mut document = OpenDoc::new(report.workbook);
        document.doc.mark_dirty();
        if report.warnings.is_empty() {
            self.install_opened_doc(None, document);
        } else {
            self.dialogs.imports.push_back((
                None,
                document,
                report.warnings,
                Some(self.current_viewport),
            ));
        }
        Ok(())
    }

    pub(crate) fn drawio_link_ui(&mut self, ctx: &Context) -> bool {
        let Some(mut draft) = self.dialogs.drawio_link.take() else {
            return false;
        };
        if draft.owner != self.current_viewport {
            self.dialogs.drawio_link = Some(draft);
            return true;
        }

        let text_focused = ctx.memory(|memory| memory.has_focus(input_id()));
        let insertion_bytes = ctx.input(|input| {
            input.events.iter().fold(0usize, |total, event| {
                total
                    .saturating_add(input_insertion(event, text_focused).map_or(0, |event| event.0))
            })
        });
        let selection = if draft.text.len().saturating_add(insertion_bytes)
            > thinoda_io::MAX_DRAWIO_LINK_BYTES
        {
            selected_bytes(ctx, &draft.text)
        } else {
            0
        };
        let rejected_input = ctx.input_mut(|input| {
            reject_oversized_input(
                &mut input.events,
                draft.text.len(),
                selection,
                thinoda_io::MAX_DRAWIO_LINK_BYTES,
                text_focused,
            )
        });
        if rejected_input || draft.text.len() > thinoda_io::MAX_DRAWIO_LINK_BYTES {
            draft.error = Some(t(TOO_LARGE).into());
        }

        let touch = widgets::touch_activation_allowed(ctx);
        let mut import = false;
        let mut cancel = false;
        let response = Dialog::new(Id::unique("thinoda_drawio_link"), t(TITLE))
            .labels(&i18n::dialog_labels())
            .modal(true)
            .close_on_backdrop(true)
            .default_width(620.0)
            .min_size(egui::vec2(320.0, 180.0))
            .fit_content_height(true)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(t(HELP));
                let label = ui.label(t(LINK_LABEL));
                let edit = egui::ScrollArea::vertical()
                    .id_salt(input_id().with("scroll"))
                    .max_height(150.0)
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut draft.text)
                                .id(input_id())
                                .lock_focus(draft.ime_composing)
                                .desired_rows(4)
                                .desired_width(ui.available_width()),
                        )
                        .labelled_by(label.id)
                    })
                    .inner;
                if !draft.focus_requested {
                    edit.request_focus();
                    draft.focus_requested = true;
                }
                if !edit.has_focus() && !edit.lost_focus() {
                    draft.ime_composing = false;
                }
                if edit.changed()
                    && !rejected_input
                    && draft.text.len() <= thinoda_io::MAX_DRAWIO_LINK_BYTES
                {
                    draft.error = None;
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.add_space(8.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), ui.spacing().interact_size.y.max(28.0)),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        import = widgets::primary_button(ui, t("Import converted copy")).clicked()
                            && touch;
                        cancel = widgets::secondary_button(ui, t("Cancel")).clicked() && touch;
                    },
                );
            });

        if (response.close_requested && touch) || cancel {
            clear_input_state(ctx);
        } else if import {
            match self.import_drawio_link_text(&draft.text) {
                Ok(()) => clear_input_state(ctx),
                Err(error) => {
                    let reason: String = error.to_string().chars().take(240).collect();
                    draft.error = Some(format!("{}: {reason}", t(IMPORT_ERROR)));
                    self.dialogs.drawio_link = Some(draft);
                }
            }
        } else {
            self.dialogs.drawio_link = Some(draft);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Cmd;
    use egui::{Event, ImeEvent, Key, Modifiers, accesskit::Role};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_io::ImportWarning;

    fn link(style: &str) -> String {
        let xml = format!(
            r#"<mxfile><diagram name="Linked page"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="shape" value="Link shape" style="{style}" vertex="1" parent="1"><mxGeometry x="20" y="30" width="100" height="60" as="geometry"/></mxCell></root></mxGraphModel></diagram></mxfile>"#
        );
        format!(
            "https://viewer.diagrams.net/?lightbox=1#R{}",
            thinoda_io::drawio_clipboard::encode_uri_component(&xml)
        )
    }

    fn app_with_changes() -> ThinodaApp {
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        let root = doc.doc.sheet().root;
        doc.doc
            .edit_sheet("Existing work", |sheet| {
                sheet.set_title(root, "Keep this document")
            })
            .unwrap();
        app
    }

    fn harness() -> Harness<'static, ThinodaApp> {
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .allow_missing_glyphs()
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app_with_changes());
        h.run_steps(3);
        h
    }

    fn open(h: &mut Harness<'_, ThinodaApp>) {
        let ctx = h.ctx.clone();
        h.state_mut().exec(&ctx, Cmd::ImportDrawioLink);
        h.run_steps(3);
        assert!(h.state().any_modal_open());
        assert!(h.get_by_label(t(LINK_LABEL)).is_focused());
    }

    #[test]
    fn link_import_creates_an_unsaved_copy_without_changing_existing_work() {
        let mut app = app_with_changes();
        let before = app.doc().unwrap().doc.workbook().clone();
        let previous_recents = app.settings.recent_files.clone();
        let count = app.docs.len();
        app.import_drawio_link_text(&link("rounded=1;")).unwrap();
        assert!(app.dialogs.imports.is_empty());
        assert_eq!(app.docs.len(), count + 1);
        assert_eq!(app.docs[0].doc.workbook(), &before);
        let imported = app.doc().unwrap();
        assert!(imported.is_dirty());
        assert!(imported.path.is_none());
        assert!(imported.import_source.is_none());
        assert_eq!(imported.doc.workbook().sheets[0].title, "Linked page");
        assert!(
            imported
                .doc
                .sheet()
                .canvas
                .objects
                .iter()
                .any(|object| object.label() == "Link shape")
        );
        assert_eq!(app.settings.recent_files, previous_recents);
    }

    #[test]
    fn link_import_replaces_a_pristine_tab_with_a_dirty_copy() {
        let mut app = ThinodaApp::headless();
        assert_eq!(app.docs.len(), 1);
        assert!(!app.doc().unwrap().is_dirty());
        app.import_drawio_link_text(&link("rounded=1;")).unwrap();
        assert_eq!(app.docs.len(), 1);
        let imported = app.doc().unwrap();
        assert!(imported.is_dirty());
        assert!(imported.path.is_none());
        assert!(imported.import_source.is_none());
        assert_eq!(imported.doc.workbook().sheets[0].title, "Linked page");
    }

    #[test]
    fn link_import_loss_requires_cancelable_confirmation() {
        let mut app = app_with_changes();
        let before = app.doc().unwrap().doc.workbook().clone();
        let count = app.docs.len();
        let lossy = link("dashed=1;");
        app.import_drawio_link_text(&lossy).unwrap();
        assert_eq!(app.docs.len(), count);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        let (source, _, warnings, owner) = app.dialogs.imports.front().unwrap();
        assert!(source.is_none());
        assert!(warnings.contains(&ImportWarning::Formatting));
        assert_eq!(*owner, Some(ViewportId::ROOT));
        app.resolve_import(false);
        assert_eq!(app.docs.len(), count);
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);

        app.import_drawio_link_text(&lossy).unwrap();
        app.resolve_import(true);
        assert_eq!(app.docs.len(), count + 1);
        assert_eq!(app.docs[0].doc.workbook(), &before);
        assert!(app.doc().unwrap().is_dirty());
        assert!(app.doc().unwrap().path.is_none());
    }

    #[test]
    fn rejected_link_does_not_change_documents_or_queue_warnings() {
        let mut app = app_with_changes();
        let before = app.doc().unwrap().doc.workbook().clone();
        let count = app.docs.len();
        for invalid in ["#Uhttps://example.invalid/map", "#R%ZZ", "not a link"] {
            assert!(app.import_drawio_link_text(invalid).is_err());
            assert_eq!(app.docs.len(), count);
            assert_eq!(app.doc().unwrap().doc.workbook(), &before);
            assert!(app.dialogs.imports.is_empty());
        }
    }

    #[test]
    fn link_import_keeps_the_originating_viewport_and_falls_back_when_it_closes() {
        let mut app = app_with_changes();
        let viewport = ViewportId::from_hash_of("link-import-window");
        let key = app.doc().unwrap().key.clone();
        app.document_windows.insert(key, viewport);
        app.current_viewport = viewport;
        let ctx = Context::default();
        app.begin_drawio_link(&ctx);
        assert_eq!(app.dialogs.drawio_link.as_ref().unwrap().owner, viewport);
        app.return_drawio_import_viewport(viewport);
        assert_eq!(
            app.dialogs.drawio_link.as_ref().unwrap().owner,
            ViewportId::ROOT
        );
        app.dialogs.drawio_link = None;

        app.import_drawio_link_text(&link("dashed=1;")).unwrap();
        assert_eq!(app.dialogs.imports.front().unwrap().3, Some(viewport));
        app.current_viewport = ViewportId::ROOT;
        app.resolve_import(true);
        let key = app.doc().unwrap().key.clone();
        assert_eq!(app.document_windows.get(&key), Some(&viewport));

        app.current_viewport = viewport;
        app.import_drawio_link_text(&link("dashed=1;")).unwrap();
        app.return_drawio_import_viewport(viewport);
        assert_eq!(
            app.dialogs.imports.front().unwrap().3,
            Some(ViewportId::ROOT)
        );
    }

    #[test]
    fn dialog_supports_retry_cancel_and_consent_without_remembering_the_link() {
        let mut h = harness();
        let before = h.state().doc().unwrap().doc.workbook().clone();
        open(&mut h);
        h.get_by_label(t(LINK_LABEL)).type_text("#R%ZZ");
        h.get_by_role_and_label(Role::Button, t("Import converted copy"))
            .click();
        h.run_steps(3);
        assert!(
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .unwrap()
                .error
                .is_some()
        );
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
        h.get_by_label(t(LINK_LABEL)).focus();
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run_steps(2);
        let published_link = link("dashed=1;");
        h.get_by_label(t(LINK_LABEL)).type_text(&published_link);
        h.run_steps(2);
        assert_eq!(
            h.state().dialogs.drawio_link.as_ref().unwrap().text,
            published_link
        );
        h.get_by_role_and_label(Role::Button, t("Import converted copy"))
            .click();
        h.run_steps(3);
        assert!(
            h.state().dialogs.drawio_link.is_none(),
            "import error: {:?}",
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .and_then(|draft| draft.error.as_ref())
        );
        assert_eq!(h.state().dialogs.imports.len(), 1);
        assert!(
            h.query_all_by_role(Role::Label).any(|node| node
                .value()
                .is_some_and(|value| value.contains(t(SOURCE_NOTICE)))),
            "conversion warning must identify the unchanged link"
        );
        assert!(
            h.query_all_by_role(Role::Label)
                .any(|node| node.value().is_some_and(|value| value
                    .contains(t("Converted drawing labels (up to 8)"))
                    && value.contains("Link shape")
                    && value.contains(t("Conversion warnings"))
                    && value.contains(t("Formatting and styles")))),
            "drawing labels and conversion warnings must be shown separately"
        );
        h.get_by_role_and_label(Role::Button, t("Cancel")).click();
        h.run_steps(3);
        assert!(h.state().dialogs.imports.is_empty());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);

        open(&mut h);
        assert!(
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .unwrap()
                .text
                .is_empty()
        );
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        assert!(
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .unwrap()
                .text
                .is_empty()
        );
        h.get_by_label(t(LINK_LABEL)).type_text("#Rcancelled");
        h.run_steps(2);
        h.key_press(Key::Escape);
        h.run_steps(3);
        assert!(h.state().dialogs.drawio_link.is_none());
        open(&mut h);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        h.run_steps(2);
        assert!(
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .unwrap()
                .text
                .is_empty()
        );
    }

    #[test]
    fn conversion_confirmation_limits_drawing_labels_and_separates_warnings() {
        let mut h = harness();
        let cells: String = (0..10)
            .map(|index| {
                format!(
                    r#"<mxCell id="shape{index}" value="Diagram item {index}" style="dashed=1;" vertex="1" parent="1"><mxGeometry x="{}" y="0" width="100" height="60" as="geometry"/></mxCell>"#,
                    index * 110
                )
            })
            .collect();
        let xml = format!(
            r#"<mxfile><diagram name="Page"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{cells}</root></mxGraphModel></diagram></mxfile>"#
        );
        let link = format!(
            "#R{}",
            thinoda_io::drawio_clipboard::encode_uri_component(&xml)
        );
        h.state_mut().import_drawio_link_text(&link).unwrap();
        h.run_steps(3);

        assert!(h.query_all_by_role(Role::Label).any(|node| {
            node.value().is_some_and(|value| {
                value.contains(t("Converted drawing labels (up to 8)"))
                    && value.matches("- Diagram item ").count() == 8
                    && !value.contains(t("Converted outline preview"))
                    && value.contains(&format!(
                        "{}:\n- {}",
                        t("Conversion warnings"),
                        t("Formatting and styles")
                    ))
            })
        }));
    }

    #[test]
    fn composing_text_does_not_cancel_the_link_dialog() {
        let mut h = harness();
        open(&mut h);
        h.event(Event::Ime(ImeEvent::Preedit {
            text: "nihon".into(),
            active_range_chars: Some(0..5),
        }));
        h.run_steps(2);
        assert!(
            h.state()
                .dialogs
                .drawio_link
                .as_ref()
                .unwrap()
                .ime_composing
        );
        h.key_press(Key::Escape);
        h.run_steps(2);
        assert!(h.state().dialogs.drawio_link.is_some());
        h.event(Event::Ime(ImeEvent::Commit("日本".into())));
        h.run_steps(2);
        assert_eq!(h.state().dialogs.drawio_link.as_ref().unwrap().text, "日本");
    }

    #[test]
    fn link_dialog_keeps_actions_near_the_editor_for_long_input() {
        let mut h = harness();
        open(&mut h);
        h.get_by_label(t(LINK_LABEL))
            .type_text(&format!("#R{}", "a".repeat(4096)));
        h.run_steps(3);
        let editor = h.get_by_label(t(LINK_LABEL)).rect();
        let action = h
            .get_by_role_and_label(Role::Button, t("Import converted copy"))
            .rect();
        assert!(
            (-2.0..80.0).contains(&(action.top() - editor.bottom())),
            "link actions should remain close to the visible editor: editor {editor:?}, action {action:?}"
        );
        assert!(action.bottom() < 1000.0);
    }

    #[test]
    fn link_selection_counts_utf8_bytes() {
        let mut h = harness();
        open(&mut h);
        h.get_by_label(t(LINK_LABEL)).type_text("a猫b");
        h.run_steps(2);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.run_steps(2);
        let draft = h.state().dialogs.drawio_link.as_ref().unwrap();
        assert_eq!(selected_bytes(&h.ctx, &draft.text), 5);
    }

    #[test]
    fn input_limit_counts_bytes_and_allows_replacing_a_selection() {
        let limit = thinoda_io::MAX_DRAWIO_LINK_BYTES;
        let mut events = vec![Event::Paste("abc".into())];
        assert!(!reject_oversized_input(
            &mut events,
            limit - 3,
            0,
            limit,
            true
        ));
        assert_eq!(events.len(), 1);

        let mut events = vec![Event::Paste("abc".into())];
        assert!(reject_oversized_input(
            &mut events,
            limit - 2,
            0,
            limit,
            true
        ));
        assert!(events.is_empty());

        let mut events = vec![
            Event::Ime(ImeEvent::Commit("猫".into())),
            Event::Text("x".into()),
        ];
        assert!(!reject_oversized_input(&mut events, limit, 4, limit, true));
        assert_eq!(events.len(), 2);

        let mut events = vec![
            Event::Ime(ImeEvent::Commit("猫".into())),
            Event::Text("xy".into()),
        ];
        assert!(reject_oversized_input(&mut events, limit, 4, limit, true));
        assert_eq!(events.len(), 1);

        let preedit = Event::Ime(ImeEvent::Preedit {
            text: "猫".into(),
            active_range_chars: None,
        });
        let mut events = vec![preedit.clone(), Event::Ime(ImeEvent::Commit("猫".into()))];
        assert!(!reject_oversized_input(
            &mut events,
            limit - 3,
            0,
            limit,
            true
        ));
        assert_eq!(events.len(), 2);

        let mut events = vec![preedit];
        assert!(reject_oversized_input(
            &mut events,
            limit - 2,
            0,
            limit,
            true
        ));
        assert!(events.is_empty());

        let enter = Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let mut events = vec![enter.clone()];
        assert!(reject_oversized_input(&mut events, limit, 0, limit, true));
        assert!(events.is_empty());
        let mut events = vec![enter];
        assert!(!reject_oversized_input(&mut events, limit, 0, limit, false));
        assert_eq!(events.len(), 1);

        let replacement = Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::ReplaceSelectedText,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: input_id().accesskit_id(),
            data: Some(egui::accesskit::ActionData::Value("猫".into())),
        });
        let mut events = vec![replacement];
        assert!(reject_oversized_input(
            &mut events,
            limit - 2,
            0,
            limit,
            false
        ));
        assert!(events.is_empty());
    }

    #[test]
    fn link_dialog_strings_are_present_in_every_locale_catalog() {
        for key in [
            TITLE,
            LINK_LABEL,
            HELP,
            IMPORT_ERROR,
            TOO_LARGE,
            SOURCE_NOTICE,
        ] {
            assert!(i18n::has_translations(key), "missing translation: {key}");
            for language in i18n::Lang::ALL {
                assert!(!i18n::t_in(language, key).is_empty(), "{language:?}: {key}");
            }
        }
    }
}

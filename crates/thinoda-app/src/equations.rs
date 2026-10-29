//! Explicit, cancel-safe notes equation editor and offline vector preview.

use egui::{Align2, FontId, Id, Response, Ui};
use thinoda_core::equations::{self as math, Primitive};

use crate::i18n::t;

pub(crate) const TITLE: &str = "Equation preview in notes";
pub(crate) const SCOPE: &str = "Notes preview only; not a map equation. Native save preserves the source; other exports are blocked.";
pub(crate) const HELP: &str = "512 bytes maximum. Use groups, \\frac{a}{b}, ^{...} and _{...}. Limited syntax, not general LaTeX or chemistry.";
pub(crate) const PLAIN_ERROR: &str =
    "Format this nonempty plain note as Markdown explicitly before adding an equation.";

#[derive(Clone, Debug)]
struct Draft {
    owner: Id,
    frame: u64,
    source: String,
}

impl Draft {
    fn belongs_to(&self, owner: Id, frame: u64) -> bool {
        self.owner == owner && frame <= self.frame.saturating_add(1)
    }
}

fn layout(source: &str, ui: &Ui) -> Result<math::Layout, &'static str> {
    math::layout(source, 22.0, |character, size| {
        ui.fonts_mut(|fonts| {
            let font = FontId::proportional(size);
            (fonts.glyph_width(&font, character), fonts.row_height(&font))
        })
    })
}

fn paint(ui: &mut Ui, source: &str, layout: &math::Layout) {
    egui::ScrollArea::horizontal()
        .max_height(240.0)
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(layout.width + 12.0, layout.height + 12.0),
                egui::Sense::hover(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::accesskit::Role::Label, ui.is_enabled(), source)
            });
            let origin = rect.min + egui::vec2(6.0, 6.0);
            let color = ui.visuals().text_color();
            for primitive in &layout.primitives {
                match primitive {
                    Primitive::Glyph {
                        character,
                        x,
                        y,
                        size,
                    } => {
                        ui.painter().text(
                            origin + egui::vec2(*x, *y),
                            Align2::LEFT_TOP,
                            character,
                            FontId::proportional(*size),
                            color,
                        );
                    }
                    Primitive::Rule { x, y, width } => {
                        let start = origin + egui::vec2(*x, *y);
                        ui.painter().line_segment(
                            [start, start + egui::vec2(*width, 0.0)],
                            egui::Stroke::new(1.0, color),
                        );
                    }
                }
            }
        });
}

/// Returns a complete replacement note only on an explicit successful action.
/// The caller commits it through the same conflict-checked notes edit as typing.
pub(crate) fn tools(
    ui: &mut Ui,
    response: &Response,
    source: &str,
    markdown: bool,
    revision: u64,
) -> Option<String> {
    // One active draft across documents/topics; source/format/revision changes or
    // leaving the inspector invalidates it, including undo followed by redo.
    let id = Id::unique("notes-equation-draft");
    let owner = response.id.with((source, markdown, revision));
    let frame = ui.ctx().cumulative_frame_nr();
    let request_id = id.with("open-request");
    let source_owner = response.id.with((source, markdown));
    let open_requested = ui.data_mut(|data| {
        let request = data.get_temp::<(Id, u64)>(request_id);
        data.remove::<(Id, u64)>(request_id);
        match request {
            Some((expected, requested_frame)) if expected == source_owner => {
                if frame == requested_frame {
                    data.insert_temp(request_id, (expected, requested_frame));
                }
                frame == requested_frame.saturating_add(1)
            }
            _ => false,
        }
    });
    let mut draft = ui.data_mut(|data| {
        let draft = data
            .get_temp::<Draft>(id)
            .filter(|draft| draft.belongs_to(owner, frame));
        data.remove::<Draft>(id);
        draft
    });
    let allowed = ui.is_enabled() && thinoda_ui::widgets::touch_activation_allowed(ui.ctx());
    ui.separator();
    ui.strong(t(TITLE));
    ui.small(t(SCOPE));
    let block = if markdown {
        math::split_note(source)
    } else {
        Ok(None)
    };
    if let Err(error) = &block {
        ui.colored_label(ui.visuals().error_fg_color, t(error));
    }
    if draft.is_none() {
        if let Ok(Some((_, source))) = block {
            match layout(source, ui) {
                Ok(layout) => paint(ui, source, &layout),
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, t(error));
                }
            }
        }
        let can_edit = block.is_ok() && (markdown || source.is_empty());
        if ui
            .add_enabled(can_edit, egui::Button::new(t("Edit notes equation")))
            .clicked()
            && allowed
        {
            // Clicking here can blur/commit the ordinary Notes buffer later in
            // this frame. Start the scratch draft on the next frame, with that
            // commit's revision, rather than immediately invalidating it.
            ui.data_mut(|data| data.insert_temp(request_id, (source_owner, frame)));
            ui.ctx().request_repaint();
        }
        if open_requested && can_edit && allowed {
            // A new edit session must not reuse a canceled TextEdit undo buffer.
            egui::text_edit::TextEditState::default().store(ui.ctx(), id.with(owner));
            ui.memory_mut(|memory| memory.request_focus(id.with(owner)));
            draft = Some(Draft {
                owner,
                frame,
                source: block
                    .ok()
                    .flatten()
                    .map_or("\\frac{a}{b}+x^{2}", |(_, math)| math)
                    .to_owned(),
            });
        }
        if !markdown && !source.is_empty() {
            ui.small(t(PLAIN_ERROR));
        }
    }
    let mut replacement = None;
    if let Some(mut pending) = draft {
        // Consume before TextEdit/canvas handling: cancellation must not also
        // clear the selected topic or escape the surrounding view.
        let mut close = allowed
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        ui.small(t(HELP));
        let label = ui.label(t("Equation source"));
        let response = ui.add(
            egui::TextEdit::multiline(&mut pending.source)
                .id(id.with(owner))
                .desired_rows(3)
                .desired_width(ui.available_width())
                .code_editor(),
        );
        response.labelled_by(label.id);
        let preview = layout(&pending.source, ui);
        match &preview {
            Ok(layout) => paint(ui, &pending.source, layout),
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, t(error));
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    preview.is_ok(),
                    egui::Button::new(t("Apply notes equation")),
                )
                .clicked()
                && allowed
            {
                match math::replace_note(source, Some(&pending.source)) {
                    Ok(note) => {
                        replacement = Some(note);
                        close = true;
                    }
                    Err(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, t(error));
                    }
                }
            }
            if block.as_ref().is_ok_and(Option::is_some)
                && ui.button(t("Remove notes equation")).clicked()
                && allowed
            {
                match math::replace_note(source, None) {
                    Ok(note) => {
                        replacement = Some(note);
                        close = true;
                    }
                    Err(error) => {
                        ui.colored_label(ui.visuals().error_fg_color, t(error));
                    }
                }
            }
            if ui.button(t("Cancel equation edit")).clicked() && allowed {
                close = true;
            }
        });
        if !close {
            pending.frame = frame;
            ui.data_mut(|data| data.insert_temp(id, pending));
        }
    }
    replacement
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equation_messages_exist_in_every_translation_catalog() {
        for language in crate::i18n::Lang::ALL {
            for key in [
                TITLE,
                SCOPE,
                HELP,
                PLAIN_ERROR,
                "Edit notes equation",
                "Equation source",
                "Apply notes equation",
                "Remove notes equation",
                "Cancel equation edit",
                math::SYNTAX_ERROR,
                math::LIMIT_ERROR,
                math::BLOCK_ERROR,
                math::EXPORT_ERROR,
            ] {
                if language != crate::i18n::Lang::En {
                    assert_ne!(crate::i18n::t_in(language, key), key, "{language:?}: {key}");
                }
            }
        }
    }

    #[test]
    fn equation_draft_requires_exact_owner_revision_and_frame_continuity() {
        let owner = Id::unique(("document-a", "topic-a", "source", false, 1));
        let draft = Draft {
            owner,
            frame: 10,
            source: "x".into(),
        };
        assert!(draft.belongs_to(owner, 11));
        assert!(!draft.belongs_to(owner, 12));
        for owner in [
            Id::unique(("document-b", "topic-a", "source", false, 1)),
            Id::unique(("document-a", "topic-b", "source", false, 1)),
            Id::unique(("document-a", "topic-a", "changed", false, 1)),
            Id::unique(("document-a", "topic-a", "source", true, 1)),
            Id::unique(("document-a", "topic-a", "source", false, 2)),
        ] {
            assert!(!draft.belongs_to(owner, 11));
        }
    }
}

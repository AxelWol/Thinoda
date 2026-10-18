//! Offline legal documents embedded verbatim in the application.

use crate::{app::ThinodaApp, i18n::t};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum LegalDocument {
    #[default]
    License,
    Notices,
    LegalNotice,
}

impl LegalDocument {
    const ALL: [Self; 3] = [Self::License, Self::Notices, Self::LegalNotice];

    fn title(self) -> &'static str {
        match self {
            Self::License => "License",
            Self::Notices => "Third-party notices",
            Self::LegalNotice => "Legal notice",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::License => include_str!("../../../LICENSE.md"),
            Self::Notices => include_str!("../../../THIRD_PARTY_NOTICES.md"),
            Self::LegalNotice => include_str!("../../../IMPRESSUM.md"),
        }
    }
}

#[derive(Default)]
pub struct LegalState {
    pub open: bool,
    document: LegalDocument,
}

impl ThinodaApp {
    pub(crate) fn legal_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.legal.open {
            return;
        }
        let state = &mut self.dialogs.legal;
        let response = egui::Modal::new(egui::Id::unique("legal_documents")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.0).clamp(240.0, 720.0));
            ui.heading(t("Legal documents"));
            ui.horizontal_wrapped(|ui| {
                for document in LegalDocument::ALL {
                    ui.selectable_value(&mut state.document, document, t(document.title()));
                }
            });
            ui.weak(t("Original legal texts are shown unchanged."));
            let height = (ctx.content_rect().height() - 220.0).max(80.0);
            egui::ScrollArea::vertical()
                .id_salt(("legal_document", state.document.title()))
                .auto_shrink([false, false])
                .min_scrolled_height(height)
                .max_height(height)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(state.document.text())
                            .wrap()
                            .selectable(true),
                    )
                    .accessible_name(t(state.document.title()));
                });
            ui.separator();
            if ui.button(t("Close")).clicked() {
                state.open = false;
            }
        });
        if response.should_close() {
            state.open = false;
            // Do not let the same Escape dismiss the About window on the next frame.
            ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_complete_legal_documents_without_paraphrasing() {
        assert!(
            LegalDocument::License
                .text()
                .contains("EUROPEAN UNION PUBLIC LICENCE v. 1.2")
        );
        assert!(LegalDocument::License.text().contains("Article"));
        assert!(LegalDocument::Notices.text().contains("## Rust crates"));
        assert!(
            LegalDocument::LegalNotice
                .text()
                .contains("axelwolters@hotmail.com")
        );
        for document in LegalDocument::ALL {
            assert!(document.text().len() > 1_000);
        }
    }
}

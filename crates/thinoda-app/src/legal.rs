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
    markdown: crate::legal_markdown::Markdown,
}

impl ThinodaApp {
    pub(crate) fn legal_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.legal.open {
            return;
        }
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        let state = &mut self.dialogs.legal;
        let labels = crate::i18n::dialog_labels();
        let response =
            thinoda_ui::Dialog::new(egui::Id::unique("legal_documents"), t("Legal documents"))
                .icon(thinoda_ui::icons::INFO)
                .labels(&labels)
                .modal(true)
                .close_on_backdrop(true)
                .default_width(720.0)
                .default_height(640.0)
                .min_size(egui::vec2(320.0, 240.0))
                .scroll(false)
                .show(ctx, |ui| {
                    let previous = state.document;
                    ui.horizontal_wrapped(|ui| {
                        for document in LegalDocument::ALL {
                            ui.selectable_value(&mut state.document, document, t(document.title()));
                        }
                    });
                    if !touch_allowed {
                        state.document = previous;
                    }
                    ui.weak(t("Original legal texts are shown unchanged."));
                    let height = (ctx.content_rect().height() - 220.0).max(80.0);
                    let document = egui::ScrollArea::both()
                        .id_salt(("legal_document", state.document.title()))
                        .auto_shrink([false, false])
                        .min_scrolled_height(height)
                        .max_height(height)
                        .show(ui, |ui| state.markdown.show(ui, state.document.text()));
                    if let Some(destination) = document.inner
                        && touch_allowed
                    {
                        match destination.as_str() {
                            "LICENSE.md" => state.document = LegalDocument::License,
                            "THIRD_PARTY_NOTICES.md" => state.document = LegalDocument::Notices,
                            "IMPRESSUM.md" => state.document = LegalDocument::LegalNotice,
                            url if url.starts_with("https://")
                                || url.starts_with("http://")
                                || url.starts_with("mailto:") =>
                            {
                                ctx.open_url(egui::OpenUrl::new_tab(url));
                            }
                            _ => {
                                log::warn!(
                                    "Unsupported bundled legal-document link: {destination}"
                                );
                                self.toasts.warning(t("Not possible here"));
                            }
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(t("Close")).clicked() && touch_allowed {
                            state.open = false;
                        }
                        if ui.button(t("Copy")).clicked() && touch_allowed {
                            ctx.copy_text(state.document.text().to_owned());
                        }
                    });
                });
        if response.close_requested && touch_allowed {
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

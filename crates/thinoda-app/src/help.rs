//! Offline product guidance. No network request is made until a link is opened.

use crate::{app::ThinodaApp, i18n::t};

#[derive(Default)]
pub struct GuideState {
    pub open: bool,
    pub section: Option<usize>,
    pub query: String,
}

struct Page {
    title: &'static str,
    paragraphs: &'static [&'static str],
}

const PAGES: &[Page] = &[
    Page {
        title: "Quick Start",
        paragraphs: &[
            "Create a blank map with Ctrl+N, or open Home, select a template and choose Create. Cancel returns without creating a document.",
            "Select a topic. Tab adds a child; Enter adds a sibling. Space or F2 edits its title. Escape cancels an edit or clears selection.",
            "Use the Style sidebar for topic appearance and the Map sidebar for layout and theme. Select a topic to see its current properties.",
            "Save with Ctrl+S to a .thinoda file. Use .xmind for interchange. Save As creates a separate destination; export formats may omit unsupported features.",
            "Use Ctrl+Z to undo map edits. On macOS, use Command instead of Ctrl for command shortcuts. Help > Keyboard Shortcuts lists the available bindings.",
        ],
    },
    Page {
        title: "Editing and Navigation",
        paragraphs: &[
            "Drag a topic onto a parent or sibling insertion slot to move its branch. The preview shows the proposed placement; Escape cancels the drag.",
            "Relationships connect topics. Boundaries group siblings; summaries attach a separate summary topic to a sibling range. Select their line or outline to edit or delete the object.",
            "Labels organize topics without changing their titles. Notes and hyperlinks are available in the sidebar. Ctrl+F searches; Ctrl+H opens replacement, with optional all-sheet scope.",
            "Sheet tabs support rename, duplicate, copy, paste, delete and separate save. New Sheet from Topic copies a branch; external relationships stay in the original sheet.",
        ],
    },
    Page {
        title: "Styles and Themes",
        paragraphs: &[
            "Topic styles override map and theme defaults. Reset Style removes explicit topic overrides. Copy/Paste Style transfers appearance, not text or hierarchy.",
            "Quick Styles offers original presets and named local styles captured from resolved appearance. Applying a style to several selected topics is one undo step.",
            "Map > Custom Themes opens a staged editor. Apply Theme embeds the theme in the sheet; Save to Library stores a separate local copy without changing the map.",
            "Custom themes provide palette, light/dark backgrounds, fonts and role defaults. Explicit map and topic overrides take precedence. Removing a library theme does not change existing maps.",
            "Select a built-in theme to remove custom defaults. Optional recent-theme reuse affects new blank maps, not named templates. Missing local fonts keep their names and show a warning.",
        ],
    },
    Page {
        title: "Files and Recovery",
        paragraphs: &[
            "Use .thinoda for full-fidelity native storage. Crash-recovery snapshots are stored locally; available recovery copies are offered at startup. Recovery is not a substitute for saving or backups.",
            "When an open file changes externally, Thinoda offers conflict handling rather than silently replacing your edits. Inspect both versions before deciding which to keep.",
            "Cloud-folder support uses locally synchronized folders supplied by your cloud client. OAuth sharing and live collaboration are not implemented.",
            "Merge Map Files appends sheets only after every input loads successfully. Create Masked Copy creates an unsaved preview with identifying text removed; inspect every sheet before sharing because visual structure can still reveal information.",
        ],
    },
    Page {
        title: "Import and Export",
        paragraphs: &[
            "The Open and Export menus list supported formats. Native and unchanged Thinoda-exported XMind files preserve Thinoda-specific data. Other applications may not understand those extensions.",
            "Public XMind export materializes supported topic styles. Editable custom-theme registries, native curve interchange and exact XMind visual parity are not verified.",
            "Map Shot (F7) captures the visible GPU-rendered canvas, with a crop preview and PNG or clipboard output. Export renders the complete sheet, including offscreen topics.",
            "Print (Ctrl+P) previews the current sheet on one page, including offscreen topics. Set paper, orientation, margins, resolution and colors, then continue to the browser print dialog or save a print-ready PDF. Keep Thinoda open until printing finishes; printer settings can change the result.",
            "External image references are not embedded attachments. Check imported content and exported files before relying on them in another application.",
        ],
    },
    Page {
        title: "Platform and Privacy",
        paragraphs: &[
            "Settings controls system/light/dark appearance, language, scaling and editor preferences. Installed fonts are local to each machine; companion devices may need the same fonts.",
            "AI commands require a configured provider and may send selected topic content to that provider. Review provider settings and do not submit confidential content without permission.",
            "Send Feedback opens the public GitHub issue chooser in your browser. It does not upload maps, settings or logs. Review anything you include before submitting an issue.",
            "Project News opens the repository commit history, not an automatic updater. Signing, native platform installation checks and additional cloud services remain under development.",
        ],
    },
];

impl ThinodaApp {
    pub(crate) fn open_guide(&mut self, quick_start: bool) {
        if let Some(doc) = self.doc_mut() {
            doc.view.commit_edit(&mut doc.doc);
        }
        self.dialogs.guide = GuideState {
            open: true,
            section: quick_start.then_some(0),
            query: String::new(),
        };
    }

    pub(crate) fn guide_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.guide.open {
            return;
        }
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.dialogs.guide.open = false;
            return;
        }
        let state = &mut self.dialogs.guide;
        let response = egui::Modal::new(egui::Id::unique("user_guide")).show(ctx, |ui| {
            ui.set_width(620.0_f32.min(ctx.content_rect().width() - 48.0).max(240.0));
            ui.horizontal(|ui| {
                crate::brand::logo(ui, 36.0);
                ui.vertical(|ui| {
                    ui.heading(t("User Guide"));
                    ui.weak(format!("Thinoda · {}", t(crate::brand::TAGLINE)));
                });
            });
            let previous = state.section;
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("guide_section")
                    .selected_text(
                        state
                            .section
                            .and_then(|index| PAGES.get(index))
                            .map_or(t("All"), |page| t(page.title)),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.section, None, t("All"));
                        for (index, page) in PAGES.iter().enumerate() {
                            ui.selectable_value(&mut state.section, Some(index), t(page.title));
                        }
                    })
                    .response
                    .accessible_name(t("Guide section"));
                if state.section.is_some() && ui.button(t("All sections")).clicked() {
                    state.section = None;
                }
            });
            let search_changed = ui
                .add(egui::TextEdit::singleline(&mut state.query).hint_text(t("Search the guide")))
                .accessible_name(t("Search the guide"))
                .changed();
            let query = state.query.trim().to_lowercase();
            let height = (ctx.content_rect().height() - 260.0).max(120.0);
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt(("guide_body", state.section))
                .auto_shrink([false, false])
                .min_scrolled_height(height)
                .max_height(height);
            if previous != state.section || search_changed {
                scroll = scroll.vertical_scroll_offset(0.0);
            }
            scroll.show(ui, |ui| {
                let mut found = false;
                for (index, page) in PAGES.iter().enumerate() {
                    if state.section.is_some_and(|section| section != index)
                        || (!t(page.title).to_lowercase().contains(&query)
                            && !page
                                .paragraphs
                                .iter()
                                .any(|text| t(text).to_lowercase().contains(&query)))
                    {
                        continue;
                    }
                    found = true;
                    ui.heading(t(page.title));
                    for paragraph in page.paragraphs {
                        ui.label(t(paragraph));
                        ui.add_space(6.0);
                    }
                    ui.add_space(12.0);
                }
                if !found {
                    ui.label(t("No matching guide sections"));
                }
            });
            if ui.button(t("Close")).clicked() {
                state.open = false;
            }
        });
        if response.should_close() {
            state.open = false;
        }
    }
}

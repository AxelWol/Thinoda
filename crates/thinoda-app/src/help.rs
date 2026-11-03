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

pub(crate) const BOARD_INK_GUIDE: &str = "Pen and Highlighter draw freehand. Adjust Ink color, Ink width and Opacity, or Eraser radius for erasing; settings are captured when each gesture begins.";
pub(crate) const BOARD_TOOLS_GUIDE: &str = "Object eraser removes whole ink strokes; Pixel eraser removes parts of strokes. Lasso selects topics and objects. Image card opens a local PNG/JPEG picker.";
pub(crate) const BOARD_DUPLICATION_GUIDE: &str = "On boards, Alt-drag or Alt+arrow duplicates movable items (Shift makes arrow steps larger). Ctrl+Alt+Left/Right rotates by 1° (Shift: 15°); use Command+Option on macOS. Locked items and groups cannot be duplicated by these gestures.";
pub(crate) const BOARD_TOOLBAR_FALLBACK_GUIDE: &str =
    "If the selected-topic toolbar cannot fit, use the main menu or the topic's context menu.";

const PAGES: &[Page] = &[
    Page {
        title: "Quick Start",
        paragraphs: &[
            "Create a blank map with Ctrl+N, or open Home, select a template and choose Create. Cancel returns without creating a document.",
            "The Boards category starts canvas maps without a visible central topic: a blank canvas, several independent mind maps, sticky-note walls, retrospectives, Kanban, the Business Model Canvas, flowcharts and SWOT. Board text uses the interface language.",
            "Affinity Diagram and Eisenhower Matrix provide visual frames. Moving a note into a frame does not automatically group it or schedule a task.",
            "Meeting Board has visual areas for agenda, notes, decisions and actions. Its frames do not group notes automatically, and it has no timer.",
            "Use the board toolbar to add sticky notes, text cards, shapes, connectors and frames. Use the select tool to move items and the hand tool to pan. The Board menu includes grouping, locking, grid and minimap options.",
            BOARD_TOOLBAR_FALLBACK_GUIDE,
            BOARD_INK_GUIDE,
            BOARD_TOOLS_GUIDE,
            BOARD_DUPLICATION_GUIDE,
            "Board > Hide central topic moves any central-topic content to a floating topic. Board > Show central topic shows the central topic again; floating topics stay separate. FIM sheets always show a central topic.",
            "Select a topic. Tab adds a child; Enter adds a sibling. Space or F2 edits its title. Escape cancels an edit or clears selection.",
            "Use the Style sidebar for topic appearance and the Map sidebar for layout and theme. Select a topic to see its current properties.",
            "Data is between Style and Map. It groups images, attachments, hyperlinks, labels, notes, notes equations and Task Planning in Map and Outliner.",
            "Save with Ctrl+S to a .thinoda file. Use .xmind for interchange. Save As creates a separate destination; export formats may omit unsupported features.",
            "Use Ctrl+Z to undo map edits. On macOS, use Command instead of Ctrl for command shortcuts. Help > Keyboard Shortcuts lists the available bindings.",
            "On Windows and macOS, Ctrl+F1 collapses or expands the ribbon. Alt+1 through Alt+8 selects a category. Escape closes a temporarily opened ribbon.",
        ],
    },
    Page {
        title: "Editing and Navigation",
        paragraphs: &[
            "Drag a topic onto a parent or sibling insertion slot to move its branch. The preview shows the proposed placement; Escape cancels the drag.",
            "Relationships connect topics. Boundaries group siblings; summaries attach a separate summary topic to a sibling range. Select their line or outline to edit or delete the object.",
            "Labels organize topics without changing their titles. Notes and hyperlinks are available in the sidebar. Ctrl+F searches; Ctrl+H opens replacement, with optional all-sheet scope.",
            "Formatting is stored as Markdown text; other views and exports may show markup.",
            "Preview does not open links or load images.",
            "Copies Markdown text without formatting or link/image destinations. The saved note is unchanged.",
            "Note preview limit reached. The full source is preserved and editable.",
            "Enlarge notes editor",
            "Enlarge equation editor",
            "Apply to keep edits. Cancel discards them. Unapplied dialog edits are not saved or recovered.",
            "Changing the selection, tab or document revision closes enlarged editors without applying their edits. Apply before editing elsewhere.",
            crate::equations::TITLE,
            crate::equations::SCOPE,
            crate::equations::HELP,
            crate::equations::PLAIN_ERROR,
            thinoda_core::equations::BLOCK_ERROR,
            thinoda_core::equations::EXPORT_ERROR,
            "Sheet tabs support rename, duplicate, copy, paste, delete and separate save. New Sheet from Topic copies a branch; external relationships stay in the original sheet.",
            "Data > Local Image provides width, placement and reset controls. Resize on the map with the lower-right handle; Escape cancels and arrow keys resize a focused handle.",
            "Insert > Attachment embeds a local file as a child topic (1 byte to 8 MiB). Data > Attachment saves or downloads it separately. Nothing is opened or executed. Save as .thinoda to retain attachments; other exports are not supported.",
            crate::tree_table::HELP,
            "Dates use YYYY-MM-DD. Duration counts inclusive working days.",
            "Displayed dates are computed; editing Start date changes its explicit constraint.",
            crate::tree_table::OUTPUT,
        ],
    },
    Page {
        title: "Styles and Themes",
        paragraphs: &[
            "Topic styles override map and theme defaults. Reset Style removes explicit topic overrides. Copy/Paste Style transfers appearance, not text or hierarchy.",
            "Quick Styles offers original presets and named local styles captured from resolved appearance. Applying a style to several selected topics is one undo step.",
            "Quick Styles and Shape/Text colors work on cards and shapes. For shapes, presets change fill, border and label colors without changing geometry. Locked or unsupported items are skipped; the custom color picker remains available. Each change is one undo step.",
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
            if cfg!(feature = "integrated-cloud-services") {
                "Cloud settings connect explicitly configured providers. Open and save are revision-checked; uploads and sharing require user actions. Live provider and sync-client compatibility remain unverified; real-time collaboration is not implemented."
            } else {
                "Integrated cloud accounts are disabled by default while publisher registrations are pending. Local sync folders, backups and browser document storage remain available."
            },
            "Merge Map Files appends sheets only after every input loads successfully. Create Masked Copy creates an unsaved preview with identifying text removed; inspect every sheet before sharing because visual structure can still reveal information.",
        ],
    },
    Page {
        title: "FIM data",
        paragraphs: &[
            "Open an XDF2 or XDF3 document to edit FIM data.",
            "Save drafts as .thinoda. XDF export requires valid schemas and semantics.",
            "Only official schema archives are downloaded from XRepository. Your document stays on this device. Validation then works offline.",
            "Validation does not certify the document.",
            "Apply before changing selection; Cancel discards this draft.",
        ],
    },
    Page {
        title: "Import and Export",
        paragraphs: &[
            "The Open and Export menus list supported formats. Native and unchanged Thinoda-exported XMind files preserve Thinoda-specific data. Other applications may not understand those extensions.",
            "Save boards as .thinoda to preserve cards, shapes, connectors, frames and board settings. Other formats may change or omit board content; review the export warnings.",
            "Frame clipping is visual, not secure redaction. Content outside a frame can remain in saved files or exports, and some exports may not apply clipping.",
            "Public XMind export materializes supported topic styles. Editable custom-theme registries, native curve interchange and exact XMind visual parity are not verified.",
            "Map Shot (F7) captures the visible GPU-rendered canvas, with a crop preview and PNG or clipboard output. Export renders the complete sheet, including offscreen topics.",
            "Print (Ctrl+P) previews the current sheet on one page, including offscreen topics. Set paper, orientation, margins, source resolution and colors, then open the native OS printer dialog or save a PDF. Choose a printer and its driver settings there. The whole preview page fits proportionally inside the selected printable area; OS paper changes scale it without reflow. Submitted means accepted by the OS, not physically printed. Native dialogs use the OS language and appearance.",
            "External image references are not embedded attachments. Check imported content and exported files before relying on them in another application.",
        ],
    },
    Page {
        title: "Platform and Privacy",
        paragraphs: &[
            "Settings controls system/light/dark appearance, language, scaling and editor preferences. Installed fonts are local to each machine; companion devices may need the same fonts.",
            "AI commands require a configured provider and may send selected topic content to that provider. Review provider settings and do not submit confidential content without permission.",
            "AI > AI settings opens provider settings. Ollama uses its own chat and embedding models. CLI providers use their own login and separate API embeddings; browsers cannot run CLIs. Unconfigured AI actions and Copilot generation are disabled.",
            "Send Feedback opens the public GitHub issue chooser in your browser. It does not upload maps, settings or logs. Review anything you include before submitting an issue.",
            "Project News opens the repository commit history, not an automatic updater. Signing, native platform installation checks and additional cloud services remain under development.",
        ],
    },
];

#[cfg(test)]
pub(crate) fn translation_keys() -> impl Iterator<Item = &'static str> {
    PAGES
        .iter()
        .flat_map(|page| std::iter::once(page.title).chain(page.paragraphs.iter().copied()))
}

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
        let touch_allowed = thinoda_ui::widgets::touch_activation_allowed(ctx);
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.dialogs.guide.open = false;
            return;
        }
        use crate::app::Cmd;
        use thinoda_canvas::Action;
        let active_shortcuts: [Vec<_>; PAGES.len()] = [
            vec![
                (Cmd::New, "New"),
                (Cmd::Canvas(Action::InsertChild), "Subtopic"),
                (Cmd::Canvas(Action::InsertSibling), "Topic"),
                (Cmd::Canvas(Action::StartEdit), "Edit"),
                (Cmd::Save, "Save"),
                (Cmd::Canvas(Action::Undo), "Undo"),
            ],
            vec![
                (Cmd::Find, "Find"),
                (Cmd::FindReplace, "Find and Replace…"),
                (Cmd::PreviousSheet, "Previous Sheet"),
                (Cmd::NextSheet, "Next Sheet"),
            ],
            vec![
                (Cmd::CopyStyle, "Copy Style"),
                (Cmd::PasteStyle, "Paste Style"),
            ],
            vec![
                (Cmd::Open, "Open…"),
                (Cmd::SaveAs, "Save As…"),
                (Cmd::SaveAll, "Save All"),
            ],
            vec![
                (
                    Cmd::Export(thinoda_io::Format::FimXdf2),
                    "FIM XDatenfelder 2",
                ),
                (
                    Cmd::Export(thinoda_io::Format::FimXdf3),
                    "FIM XDatenfelder 3",
                ),
            ],
            vec![(Cmd::MapShot, "Map Shot"), (Cmd::Print, "Print")],
            vec![
                (Cmd::Settings, "Settings…"),
                (Cmd::Shortcuts, "Keyboard Shortcuts"),
            ],
        ]
        .map(|commands| {
            commands
                .into_iter()
                .map(|(command, label)| {
                    (
                        t(label),
                        self.active_shortcut_hint(ctx, &command)
                            .unwrap_or_else(|| t("Unassigned").into()),
                    )
                })
                .collect()
        });
        let state = &mut self.dialogs.guide;
        let labels = crate::i18n::dialog_labels();
        let response = thinoda_ui::Dialog::new(egui::Id::unique("user_guide"), t("User Guide"))
            .icon(thinoda_ui::icons::USER_GUIDE)
            .labels(&labels)
            .modal(true)
            .close_on_backdrop(true)
            .default_width(620.0)
            .default_height(620.0)
            .min_size(egui::vec2(320.0, 240.0))
            .scroll(false)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                crate::brand::logo(ui, 36.0);
                ui.vertical(|ui| {
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
            if !touch_allowed {
                state.section = previous;
            }
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
                    ui.strong(t("Active shortcuts"));
                    for (label, hint) in &active_shortcuts[index] {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(*label);
                            ui.label(hint);
                        });
                    }
                    ui.weak(t("Shortcut examples below use Thinoda Standard; the active profile is shown above."));
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
            if ui.button(t("Close")).clicked() && touch_allowed {
                state.open = false;
            }
        });
        if response.close_requested && touch_allowed {
            state.open = false;
        }
    }
}

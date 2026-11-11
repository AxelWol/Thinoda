//! Host-independent menu hierarchy. Native shells and compact menus share commands.

use crate::{
    app::{Cmd, EXPORT_MENU, FocusField, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::{Action, BoardAction};
use thinoda_core::{Vec2, templates};

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Command { label: String, command: Cmd },
    Submenu { label: String, entries: Vec<Self> },
    Separator,
    Caption(String),
    ClearRecent,
}

fn item(label: &'static str, command: Cmd) -> MenuEntry {
    MenuEntry::Command {
        label: t(label).into(),
        command,
    }
}

fn submenu(label: &'static str, entries: Vec<MenuEntry>) -> MenuEntry {
    MenuEntry::Submenu {
        label: t(label).into(),
        entries,
    }
}

fn items(commands: impl IntoIterator<Item = (&'static str, Cmd)>) -> Vec<MenuEntry> {
    commands
        .into_iter()
        .map(|(label, cmd)| item(label, cmd))
        .collect()
}

impl ThinodaApp {
    pub(crate) fn zone_menu_commands() -> Vec<(Cmd, &'static str)> {
        vec![
            (Cmd::CreateZone, "Create Zone"),
            (Cmd::AddToZone, "Add to Zone…"),
            (Cmd::RemoveFromZone, "Remove from Zone"),
            (Cmd::EditZone, "Edit Zone"),
            (Cmd::RemoveZone, "Remove Zone"),
            (Cmd::Zones, "Manage Zones"),
        ]
    }

    pub(crate) fn board_menu_commands() -> Vec<(Cmd, &'static str)> {
        Self::board_commands()
            .into_iter()
            .chain(
                Self::arrangement_commands().into_iter().filter_map(
                    |(action, label)| match action {
                        Action::AlignFloating(arrangement) => {
                            Some((Cmd::BoardAction(BoardAction::Arrange(arrangement)), label))
                        }
                        _ => None,
                    },
                ),
            )
            .collect()
    }

    pub(crate) fn menu_entries_available(&self, entries: &[MenuEntry]) -> bool {
        // Preserve the disabled Export group without hiding discoverable editing
        // groups whose individual actions depend on the current selection.
        !entries.iter().all(|entry| {
            matches!(
                entry,
                MenuEntry::Command {
                    command: Cmd::Export(_),
                    ..
                }
            )
        }) || entries.iter().any(|entry| {
            matches!(entry,
                MenuEntry::Command { command, .. } if self.command_available(command))
        })
    }

    pub fn file_menu_entries(&self) -> Vec<MenuEntry> {
        let templates = templates::TemplateCategory::ALL
            .iter()
            .map(|category| {
                submenu(
                    category.label(),
                    templates::TEMPLATES
                        .iter()
                        .filter(|template| template.category == *category)
                        .map(|template| item(template.name, Cmd::NewFromTemplate(template.id)))
                        .collect(),
                )
            })
            .collect();
        let mut recent: Vec<_> = self
            .settings
            .existing_recent()
            .into_iter()
            .take(15)
            .map(|recent| MenuEntry::Command {
                label: recent.path.file_name().map_or_else(
                    || recent.path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                command: Cmd::OpenPath(recent.path.clone()),
            })
            .collect();
        if recent.is_empty() {
            recent.push(MenuEntry::Caption(t("No recent files").into()));
        }
        recent.extend([MenuEntry::Separator, MenuEntry::ClearRecent]);
        let mut file = vec![
            item("New", Cmd::New),
            submenu("New from Template…", templates),
            item("Open…", Cmd::Open),
            submenu("Open Recent", recent),
        ];
        if !self.browser_migration_conflicts.is_empty() {
            let recovered = self
                .browser_migration_conflicts
                .iter()
                .map(|conflict| {
                    let path = std::path::PathBuf::from(&conflict.preserved);
                    let version = path
                        .parent()
                        .and_then(std::path::Path::file_name)
                        .map(|name| name.to_string_lossy().chars().take(12).collect::<String>())
                        .unwrap_or_default();
                    MenuEntry::Command {
                        label: format!("{} [{version}]", conflict.original),
                        command: Cmd::OpenPath(path),
                    }
                })
                .collect();
            file.push(submenu("Recovered browser documents", recovered));
        }
        file.push(MenuEntry::Separator);
        file.extend(items([
            ("Save", Cmd::Save),
            ("Save As…", Cmd::SaveAs),
            ("Save All", Cmd::SaveAll),
            ("Import", Cmd::Import),
            ("Import draw.io link…", Cmd::ImportDrawioLink),
        ]));
        #[cfg(not(target_arch = "wasm32"))]
        file.push(item("Move to New Window", Cmd::MoveToNewWindow));
        file.push(submenu(
            "Export",
            EXPORT_MENU
                .iter()
                .map(|format| item(format.label(), Cmd::Export(*format)))
                .collect(),
        ));
        file.push(item("Print", Cmd::Print));
        #[cfg(not(target_arch = "wasm32"))]
        if self.doc().is_some_and(|doc| doc.path.is_some()) {
            file.push(item("Show in Folder", Cmd::ShowInFolder));
        }
        #[cfg(not(target_arch = "wasm32"))]
        file.extend([
            MenuEntry::Separator,
            item("Close Window", Cmd::CloseWindow),
            item("Quit", Cmd::Quit),
        ]);
        file
    }

    pub fn main_menu_entries(&self) -> Vec<MenuEntry> {
        let mut edit = items([
            ("Undo", Cmd::Canvas(Action::Undo)),
            ("Redo", Cmd::Canvas(Action::Redo)),
            ("Cut", Cmd::Canvas(Action::Cut)),
            ("Copy", Cmd::Canvas(Action::Copy)),
            ("Copy as draw.io", Cmd::CopyAsDrawio),
            ("Paste", Cmd::MenuPaste),
            ("Duplicate", Cmd::Canvas(Action::Duplicate)),
            ("Delete", Cmd::Canvas(Action::Delete)),
            ("Select All", Cmd::Canvas(Action::SelectAll)),
            ("Find…", Cmd::Find),
            ("Find and Replace…", Cmd::FindReplace),
            ("Command Palette…", Cmd::Palette),
        ]);
        edit.push(submenu(
            "Select",
            Self::selection_commands()
                .into_iter()
                .map(|(action, label)| item(label, Cmd::Canvas(action)))
                .collect(),
        ));
        let mut arrange: Vec<_> = Self::arrangement_commands()
            .into_iter()
            .map(|(action, label)| item(label, Cmd::Canvas(action)))
            .collect();
        arrange.push(MenuEntry::Caption(
            t("Alignment needs floating topics only; distribution spaces centers, not edges.")
                .into(),
        ));
        edit.push(submenu("Arrange", arrange));
        edit.push(MenuEntry::Separator);
        edit.extend(items([
            ("Indent", Cmd::Canvas(Action::Indent)),
            ("Outdent", Cmd::Canvas(Action::Outdent)),
            ("Copy Style", Cmd::CopyStyle),
            ("Paste Style", Cmd::PasteStyle),
            ("Reset Style", Cmd::ResetStyle),
            ("Save Quick Style…", Cmd::SaveQuickStyle),
        ]));
        edit.push(MenuEntry::Caption(
            t("Copies explicit overrides; inherited theme styles stay automatic.").into(),
        ));
        edit.push(MenuEntry::Separator);
        edit.push(item("New Sheet from Topic", Cmd::NewSheetFromTopic));
        edit.push(item("Settings…", Cmd::Settings));
        let mut help = items([
            ("Quick Start", Cmd::QuickStart),
            ("User Guide", Cmd::UserGuide),
            ("Send Feedback…", Cmd::Feedback),
            ("Project News…", Cmd::ProjectNews),
        ]);
        help.push(MenuEntry::Separator);
        help.extend(items([
            ("Keyboard Shortcuts", Cmd::Shortcuts),
            ("About Thinoda", Cmd::About),
        ]));
        vec![
            submenu("File", self.file_menu_entries()),
            submenu("Edit", edit),
            submenu("Sheet", items([
                ("New Sheet", Cmd::NewSheet),
                ("Rename Sheet", Cmd::RenameSheet),
                ("Copy Sheet", Cmd::CopySheet),
                ("Paste Sheet", Cmd::PasteSheet),
                ("Duplicate Sheet", Cmd::DuplicateSheet),
                ("Save Sheet As…", Cmd::SaveSheetAs),
                ("Delete Sheet", Cmd::DeleteSheet),
            ])),
            submenu("Insert", items([
                ("Topic", Cmd::Canvas(Action::InsertSibling)),
                ("Subtopic", Cmd::Canvas(Action::InsertChild)),
                ("Topic Before", Cmd::Canvas(Action::InsertSiblingBefore)),
                ("Parent Topic", Cmd::Canvas(Action::InsertParent)),
                ("Floating Topic", Cmd::Canvas(Action::InsertFloating(Vec2::new(0.0, -260.0)))),
                ("Relationship", Cmd::Canvas(Action::StartRelationship)),
                ("Summary", Cmd::Canvas(Action::AddSummary)),
                ("Boundary", Cmd::Canvas(Action::AddBoundary)),
                ("To-Do", Cmd::Canvas(Action::ToggleTodo)),
                ("Task Planning", Cmd::TaskPlanning),
                ("Notes", Cmd::EditField(FocusField::Notes)),
                ("Hyperlink", Cmd::EditField(FocusField::Hyperlink)),
                ("Labels", Cmd::Labels),
                ("Local Image", Cmd::LocalImage),
                ("Attachment", Cmd::Attachment),
                ("Save/Download attachment", Cmd::SaveAttachment),
                ("Add FIM child", Cmd::AddFimChild),
                ("Insert FIM node before", Cmd::InsertFimBefore),
                ("Insert FIM node after", Cmd::InsertFimAfter),
            ])),
            submenu("AI", std::iter::once(item("AI Chat", Cmd::AiChat))
                .chain(std::iter::once(MenuEntry::Separator))
                .chain(crate::ai::AiCommand::ALL.into_iter().map(|action| {
                    item(action.label(), Cmd::Ai(action))
                }))
                .chain([MenuEntry::Separator, item("AI settings", Cmd::AiSettings)])
                .collect()),
            submenu("Tools", vec![
                item("Map Shot", Cmd::MapShot),
                item("Create Masked Copy…", Cmd::MaskedCopy),
                item("Merge Map Files…", Cmd::MergeFiles),
                MenuEntry::Caption(t(
                    "Appends complete sheets. Destination metadata is kept; external image references are not embedded.",
                ).into()),
                MenuEntry::Separator,
                item("Work Calendar", Cmd::WorkCalendar),
                item("Edit FIM node", Cmd::EditFimNode),
            ]),
            submenu("Board", Self::board_menu_commands()
                .into_iter()
                .map(|(command, label)| item(label, command))
                .collect()),
            submenu("Zone", Self::zone_menu_commands()
                .into_iter()
                .map(|(command, label)| item(label, command))
                .collect()),
            submenu("View", items([
                ("Gantt Chart", Cmd::Gantt),
                ("Pitch", Cmd::Pitch),
                ("Presenter View", Cmd::PitchPresenter),
                ("Edit Presentation", Cmd::EditPresentation),
                ("Previous Sheet", Cmd::PreviousSheet),
                ("Next Sheet", Cmd::NextSheet),
                ("Outliner", Cmd::ToggleOutliner),
                ("Zen Mode", Cmd::ToggleZen),
                ("Format Panel", Cmd::ToggleFormatPanel),
                ("Symbols", Cmd::ToggleSymbolsPanel),
                ("Status Bar", Cmd::ToggleStatusBar),
                ("Zoom In", Cmd::Canvas(Action::ZoomIn)),
                ("Zoom Out", Cmd::Canvas(Action::ZoomOut)),
                ("Actual Size", Cmd::Canvas(Action::ZoomReset)),
                ("Fit Map", Cmd::Canvas(Action::Fit)),
                ("Expand All", Cmd::Canvas(Action::ExpandAll)),
                ("Collapse All", Cmd::Canvas(Action::CollapseAll)),
            ])),
            submenu("Help", help),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{doc::OpenDoc, files::MigrationReport};
    use egui::{Role, vec2};
    use egui_kittest::{Harness, kittest::Queryable};
    use thinoda_core::Workbook;

    #[test]
    fn main_board_menu_keeps_its_tool_commands() {
        assert!(
            ThinodaApp::board_menu_commands()
                .iter()
                .any(|(command, _)| matches!(command, Cmd::BoardTool(_))),
            "the first-level Board menu keeps its existing tool commands"
        );
    }

    #[test]
    fn browser_recovery_menu_retains_all_copies_after_the_notice_expires() {
        let mut app = ThinodaApp::headless();
        let conflicts: Vec<_> = (0..24)
            .map(|index| crate::files::MigrationConflict {
                original: format!("/Documents/folder-{index}/document.thinoda"),
                preserved: format!("/Documents/.thinoda-legacy/path/{index:064x}/document.thinoda"),
            })
            .collect();
        app.apply_browser_migration(MigrationReport {
            backend: "indexeddb".into(),
            verified: conflicts.len(),
            conflicts: conflicts.clone(),
        });
        app.toasts.tick(60.0);
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains("Recovered browser documents"))
        );

        let file = app.file_menu_entries();
        let entries = file
            .iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu { label, entries } if label == "Recovered browser documents" => {
                    Some(entries)
                }
                _ => None,
            })
            .expect("recovery submenu");
        assert_eq!(entries.len(), conflicts.len());
        for (entry, conflict) in entries.iter().zip(&conflicts) {
            assert!(matches!(
                entry,
                MenuEntry::Command { label, command: Cmd::OpenPath(path) }
                    if label.starts_with(&conflict.original)
                        && path == std::path::Path::new(&conflict.preserved)
                        && app.command_available(&Cmd::OpenPath(path.clone()))
            ));
        }
    }

    #[test]
    fn browser_recovery_opens_as_an_unsaved_copy_even_after_import_confirmation() {
        let mut app = ThinodaApp::headless();
        let path =
            std::path::PathBuf::from("/Documents/.thinoda-legacy/path/content/recovered.thinoda");
        app.apply_browser_migration(MigrationReport {
            backend: "opfs".into(),
            verified: 1,
            conflicts: vec![crate::files::MigrationConflict {
                original: "/Documents/recovered.thinoda".into(),
                preserved: path.to_string_lossy().into_owned(),
            }],
        });
        let mut doc = OpenDoc::new(Workbook::default());
        doc.path = Some(path.clone());
        app.add_opened(
            &path,
            Ok((doc, vec![thinoda_io::ImportWarning::Formatting])),
        );
        app.resolve_import(true);

        let doc = app.doc().expect("opened recovery copy");
        assert!(doc.is_dirty());
        assert!(doc.path.is_none());
        assert_eq!(doc.import_source.as_deref(), Some(path.as_path()));
        assert!(doc.ensure_export_destination(&path).is_err());
        assert!(
            !app.settings
                .recent_files
                .iter()
                .any(|recent| recent.path == path)
        );
    }

    #[test]
    fn browser_recovery_menu_scrolls_to_and_opens_the_last_copy() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recovered.thinoda");
        OpenDoc::new(Workbook::default())
            .save_to(&path, false)
            .unwrap();
        let conflicts = (0..24)
            .map(|index| crate::files::MigrationConflict {
                original: format!("/Documents/folder-{index}/document.thinoda"),
                preserved: path.to_string_lossy().into_owned(),
            })
            .collect();
        let mut app = ThinodaApp::headless();
        app.apply_browser_migration(MigrationReport {
            backend: "indexeddb".into(),
            verified: 24,
            conflicts,
        });
        let version: String = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .chars()
            .take(12)
            .collect();
        let last = format!("/Documents/folder-23/document.thinoda [{version}]");
        let mut harness = Harness::builder()
            .with_size(vec2(900.0, 500.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        harness.run_steps(5);
        harness.get_by_label("Main Menu").click();
        harness.run_steps(3);
        harness.get_by_role_and_label(Role::Button, "File").click();
        harness.run_steps(3);
        harness
            .get_by_role_and_label(Role::Button, "Recovered browser documents")
            .click();
        harness.run_steps(3);
        harness
            .get_by_role_and_label(Role::Button, &last)
            .scroll_to_me();
        harness.run_steps(3);
        harness.get_by_role_and_label(Role::Button, &last).click();
        harness.run_steps(5);
        let doc = harness.state().doc().expect("opened recovery copy");
        assert!(doc.is_dirty());
        assert!(doc.path.is_none());
        assert_eq!(doc.import_source.as_deref(), Some(path.as_path()));
    }

    #[test]
    fn browser_recovery_messages_are_localized() {
        for key in [
            "Recovered browser documents",
            "Preserved browser documents are available in File > Recovered browser documents.",
        ] {
            assert!(crate::i18n::has_translations(key), "{key}");
        }
    }
}

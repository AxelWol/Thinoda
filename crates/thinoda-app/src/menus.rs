//! Host-independent menu hierarchy. Native shells and compact menus share commands.

use crate::{
    app::{Cmd, EXPORT_MENU, FocusField, ThinodaApp},
    i18n::t,
};
use thinoda_canvas::Action;
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
            MenuEntry::Separator,
        ];
        file.extend(items([
            ("Save", Cmd::Save),
            ("Save As…", Cmd::SaveAs),
            ("Save All", Cmd::SaveAll),
            ("Import", Cmd::Import),
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
        file.extend([MenuEntry::Separator, item("Quit", Cmd::Quit)]);
        file
    }

    pub fn main_menu_entries(&self) -> Vec<MenuEntry> {
        let mut edit = items([
            ("Undo", Cmd::Canvas(Action::Undo)),
            ("Redo", Cmd::Canvas(Action::Redo)),
            ("Cut", Cmd::Canvas(Action::Cut)),
            ("Copy", Cmd::Canvas(Action::Copy)),
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
                ("Notes", Cmd::EditField(FocusField::Notes)),
                ("Hyperlink", Cmd::EditField(FocusField::Hyperlink)),
                ("Labels", Cmd::Labels),
            ])),
            submenu("AI", crate::ai::AiCommand::ALL.into_iter().map(|action| {
                item(action.label(), Cmd::Ai(action))
            }).collect()),
            submenu("Tools", vec![
                item("Map Shot", Cmd::MapShot),
                item("Create Masked Copy…", Cmd::MaskedCopy),
                item("Merge Map Files…", Cmd::MergeFiles),
                MenuEntry::Caption(t(
                    "Appends complete sheets. Destination metadata is kept; external image references are not embedded.",
                ).into()),
            ]),
            submenu("View", items([
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

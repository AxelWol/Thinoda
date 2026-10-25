//! Stable shortcut identities are separate from palette row IDs and translations.
//! Existing override strings remain inactive; numeric/legacy keys are not migrated
//! without a versioned, explicit mapping.

use crate::app::{Cmd, FocusField, ThinodaApp};
use thinoda_canvas::{Action, FloatingArrangement};
use thinoda_core::{Vec2, navigation::Direction};
use thinoda_io::Format;
use thinoda_platform::settings::ThemeMode;

impl Cmd {
    /// Version-independent identity for a built-in shortcut target. Arbitrary
    /// document paths, templates, styles and clipboard payloads are not targets.
    pub fn shortcut_id(&self) -> Option<&'static str> {
        Some(match self {
            Self::New => "file.new",
            Self::Home => "file.home",
            Self::Open => "file.open",
            Self::Save => "file.save",
            Self::SaveAs => "file.save-as",
            Self::SaveAll => "file.save-all",
            Self::CloseTab(_) => "file.close-tab",
            Self::CloseWindow => "window.close",
            Self::MoveToNewWindow => "window.move-to-new-window",
            Self::NextDoc => "view.next-document",
            Self::Import => "file.import",
            Self::MergeFiles => "file.merge",
            Self::MaskedCopy => "file.masked-copy",
            Self::MapShot => "tools.map-shot",
            Self::Print => "file.print",
            Self::Labels => "insert.labels",
            Self::TaskPlanning => "insert.task",
            Self::CreateZone => "insert.zone",
            Self::EditZone => "edit.zone",
            Self::Zones => "tools.zones",
            Self::Pitch => "view.pitch",
            Self::PitchPresenter => "view.pitch-presenter",
            Self::EditPresentation => "edit.presentation",
            Self::WorkCalendar => "tools.work-calendar",
            Self::Gantt => "view.gantt",
            Self::Settings => "edit.settings",
            Self::About => "help.about",
            Self::Shortcuts => "help.shortcuts",
            Self::UserGuide => "help.user-guide",
            Self::QuickStart => "help.quick-start",
            Self::Feedback => "help.feedback",
            Self::ProjectNews => "help.project-news",
            Self::Quit => "file.quit",
            Self::ToggleOutliner => "view.outliner",
            Self::ToggleZen => "view.zen",
            Self::ToggleFormatPanel => "view.format-panel",
            Self::ToggleSymbolsPanel => "view.symbols-panel",
            Self::ToggleStatusBar => "view.status-bar",
            Self::Find => "edit.find",
            Self::FindReplace => "edit.find-replace",
            Self::TopicStyle => "style.topic",
            Self::Palette => "edit.command-palette",
            Self::NewSheet => "sheet.new",
            Self::DuplicateSheet => "sheet.duplicate",
            Self::DeleteSheet => "sheet.delete",
            Self::RenameSheet => "sheet.rename",
            Self::PreviousSheet => "sheet.previous",
            Self::NextSheet => "sheet.next",
            Self::CopyStyle => "style.copy",
            Self::PasteStyle => "style.paste",
            Self::ResetStyle => "style.reset",
            Self::SaveQuickStyle => "style.save-quick-style",
            Self::EditCustomTheme => "style.edit-theme",
            Self::CopySheet => "sheet.copy",
            Self::PasteSheet => "sheet.paste",
            Self::SaveSheetAs => "sheet.save-as",
            Self::NewSheetFromTopic => "sheet.from-topic",
            Self::OpenLink => "topic.open-link",
            Self::ShowInFolder => "file.show-in-folder",
            Self::MenuPaste => "edit.paste",
            Self::SetTheme(ThemeMode::System) => "appearance.system",
            Self::SetTheme(ThemeMode::Light) => "appearance.light",
            Self::SetTheme(ThemeMode::Dark) => "appearance.dark",
            Self::EditField(FocusField::Hyperlink) => "insert.hyperlink",
            Self::EditField(FocusField::Notes) => "insert.notes",
            Self::Ai(action) => match action {
                crate::ai::AiCommand::Brainstorm => "ai.brainstorm",
                crate::ai::AiCommand::Breakdown => "ai.breakdown",
                crate::ai::AiCommand::Explain => "ai.explain",
                crate::ai::AiCommand::Summarize => "ai.summarize",
                crate::ai::AiCommand::GenerateMap => "ai.generate-map",
            },
            Self::Export(format) => match format {
                Format::Thinoda => "export.thinoda",
                Format::XMind => "export.xmind",
                Format::FreeMind => "export.freemind",
                Format::Opml => "export.opml",
                Format::Markdown => "export.markdown",
                Format::PlainText => "export.text",
                Format::MindManager => "export.mindmanager",
                Format::MindManagerXml => return None,
                Format::TextBundle => "export.textbundle",
                Format::Svg => "export.svg",
                Format::Png => "export.png",
                Format::Jpeg => "export.jpeg",
                Format::Pdf => "export.pdf",
                Format::Word => "export.word",
                Format::Excel => "export.excel",
                Format::Csv => "export.csv",
            },
            Self::Canvas(action) => match action {
                Action::InsertChild => "topic.insert-child",
                Action::InsertSibling => "topic.insert-sibling",
                Action::InsertSiblingBefore => "topic.insert-sibling-before",
                Action::InsertParent => "topic.insert-parent",
                Action::InsertFloating(position) if *position == Vec2::new(0.0, -260.0) => {
                    "topic.insert-floating"
                }
                Action::Delete => "topic.delete",
                Action::DeleteKeepChildren => "topic.delete-only",
                Action::Duplicate => "topic.duplicate",
                Action::StartEdit => "topic.edit",
                Action::ToggleCollapse => "topic.toggle-collapse",
                Action::ExpandAll => "topic.expand-all",
                Action::CollapseAll => "topic.collapse-all",
                Action::ToggleCollapseAll => "topic.toggle-collapse-all",
                Action::Navigate(Direction::Up) => "navigation.up",
                Action::Navigate(Direction::Down) => "navigation.down",
                Action::Navigate(Direction::Left) => "navigation.left",
                Action::Navigate(Direction::Right) => "navigation.right",
                Action::MoveUp => "topic.move-up",
                Action::MoveDown => "topic.move-down",
                Action::Indent => "topic.indent",
                Action::Outdent => "topic.outdent",
                Action::SelectAll => "selection.all",
                Action::SelectRoot => "selection.root",
                Action::SelectNone => "selection.none",
                Action::SelectSiblings => "selection.siblings",
                Action::SelectDescendants => "selection.descendants",
                Action::InvertSelection => "selection.invert",
                Action::SortAscending => "topic.sort-ascending",
                Action::SortDescending => "topic.sort-descending",
                Action::AlignFloating(arrangement) => match arrangement {
                    FloatingArrangement::Left => "arrange.left",
                    FloatingArrangement::HorizontalCenter => "arrange.horizontal-center",
                    FloatingArrangement::Right => "arrange.right",
                    FloatingArrangement::Top => "arrange.top",
                    FloatingArrangement::VerticalCenter => "arrange.vertical-center",
                    FloatingArrangement::Bottom => "arrange.bottom",
                    FloatingArrangement::DistributeHorizontal => "arrange.distribute-horizontal",
                    FloatingArrangement::DistributeVertical => "arrange.distribute-vertical",
                },
                Action::StartRelationship => "insert.relationship",
                Action::AddBoundary => "insert.boundary",
                Action::AddSummary => "insert.summary",
                Action::ToggleTodo => "insert.todo",
                Action::ToggleMarker(marker) if marker == "priority-1" => "insert.priority-1",
                Action::ToggleMarker(marker) if marker == "priority-2" => "insert.priority-2",
                Action::ToggleMarker(marker) if marker == "priority-3" => "insert.priority-3",
                Action::ZoomIn => "view.zoom-in",
                Action::ZoomOut => "view.zoom-out",
                Action::ZoomReset => "view.zoom-reset",
                Action::Fit => "view.fit",
                Action::Undo => "edit.undo",
                Action::Redo => "edit.redo",
                Action::Copy => "edit.copy",
                Action::Cut => "edit.cut",
                Action::Paste(None) => "edit.paste",
                Action::InsertFloating(_)
                | Action::ToggleMarker(_)
                | Action::ToggleMarkerColor(..)
                | Action::SetMarkerColor(..)
                | Action::RemoveMarker(..)
                | Action::Paste(Some(_)) => return None,
            },
            Self::NewFromTemplate(_)
            | Self::OpenPath(_)
            | Self::ApplyQuickStyle(_)
            | Self::ApplyCustomTheme(_) => return None,
        })
    }
}

impl ThinodaApp {
    /// Resolve built-in targets independently of display order and translation.
    /// The registry is not a palette or an editable keymap.
    pub fn shortcut_commands(&self) -> Vec<(&'static str, Cmd)> {
        let mut commands: Vec<_> = Self::palette_commands()
            .into_iter()
            .map(|(cmd, _)| cmd)
            .collect();
        commands.extend(
            Self::global_shortcut_bindings(self.active)
                .into_iter()
                .map(|(_, cmd)| cmd),
        );
        for mode in [
            crate::app::CenterMode::Map,
            crate::app::CenterMode::Outliner,
        ] {
            commands.extend(
                Self::contextual_shortcut_bindings_for_mode(mode)
                    .into_iter()
                    .map(|(_, action)| Cmd::Canvas(action)),
            );
        }
        commands.extend([
            Cmd::Home,
            Cmd::TopicStyle,
            Cmd::OpenLink,
            Cmd::ShowInFolder,
            Cmd::Canvas(Action::Copy),
            Cmd::Canvas(Action::Cut),
            Cmd::MenuPaste,
        ]);
        commands.extend(Format::EXPORTABLE.iter().copied().map(Cmd::Export));
        let mut registry = Vec::new();
        for mut command in commands {
            if matches!(command, Cmd::CloseTab(_)) {
                command = Cmd::CloseTab(self.active);
            }
            if let Some(id) = command.shortcut_id()
                && !registry.iter().any(|(existing, _)| *existing == id)
            {
                registry.push((id, command));
            }
        }
        registry
    }

    pub fn shortcut_command(&self, id: &str) -> Option<Cmd> {
        self.shortcut_commands()
            .into_iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, command)| command)
    }
}

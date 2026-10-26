use crate::{
    ai::AiCommand,
    app::{Cmd, FocusField},
};
use thinoda_canvas::{Action, FloatingArrangement};
use thinoda_ui::{
    icons as I,
    ribbon::{CommandIcon, IconTone as T},
};

pub(crate) fn command_glyph(command: &Cmd) -> CommandIcon {
    let (symbol, badge, tone) = match command {
        Cmd::New => (I::NEW_FILE, None, T::File),
        Cmd::NewFromTemplate(_) => (I::TEMPLATE, None, T::File),
        Cmd::Open | Cmd::OpenPath(_) => (I::OPEN, None, T::File),
        Cmd::ShowInFolder => (I::SHOW_IN_FOLDER, None, T::File),
        Cmd::Save => (I::SAVE, None, T::File),
        Cmd::SaveAs => (I::SAVE_AS, None, T::File),
        Cmd::SaveAll => (I::SAVE, Some(I::STACK), T::File),
        Cmd::SaveSheetAs => (I::SAVE_AS, Some(I::SHEET), T::File),
        Cmd::Print => (I::PRINT, None, T::File),
        Cmd::TaskPlanning => (I::TASK_PLANNING, None, T::Create),
        Cmd::CreateZone => (I::ZONES, Some(I::ADD), T::Create),
        Cmd::EditZone => (I::ZONES, Some(I::EDIT), T::Edit),
        Cmd::Zones => (I::ZONES, None, T::View),
        Cmd::Pitch => (I::PITCH, None, T::View),
        Cmd::PitchPresenter => (I::PRESENTER, None, T::View),
        Cmd::EditPresentation => (I::EDIT_PRESENTATION, None, T::Edit),
        Cmd::WorkCalendar => (I::CALENDAR, None, T::View),
        Cmd::Gantt => (I::GANTT, None, T::View),
        Cmd::Export(_) => (I::EXPORT, None, T::File),
        Cmd::Import => (I::IMPORT, None, T::File),
        Cmd::MenuPaste => (I::PASTE, None, T::Edit),
        Cmd::PasteSheet => (I::PASTE, Some(I::SHEET), T::Edit),
        Cmd::CopySheet => (I::COPY, Some(I::SHEET), T::Edit),
        Cmd::DuplicateSheet => (I::DUPLICATE, Some(I::SHEET), T::Create),
        Cmd::MaskedCopy => (I::MASK, None, T::File),
        Cmd::CopyStyle => (I::COPY_STYLE, None, T::Format),
        Cmd::PasteStyle => (I::PASTE, Some(I::TOPIC_STYLE), T::Format),
        Cmd::ResetStyle => (I::RESET_STYLE, None, T::Format),
        Cmd::SaveQuickStyle => (I::QUICK_STYLE, Some(I::ADD), T::Format),
        Cmd::NewSheet => (I::SHEET, Some(I::ADD), T::Create),
        Cmd::NextSheet => (I::SHEET, Some(I::ARROW_RIGHT), T::View),
        Cmd::PreviousSheet => (I::SHEET, Some(I::ARROW_LEFT), T::View),
        Cmd::RenameSheet => (I::SHEET, Some(I::EDIT), T::Edit),
        Cmd::NewSheetFromTopic => (I::SHEET, Some(I::BRANCH), T::Create),
        Cmd::DeleteSheet => (I::SHEET, Some(I::REMOVE), T::Danger),
        Cmd::AiChat => (I::AI_CHAT, Some(I::AI), T::Ai),
        Cmd::Ai(action) => match action {
            AiCommand::Brainstorm => (I::BRAINSTORM, None, T::Ai),
            AiCommand::Breakdown => (I::TODO, Some(I::AI), T::Ai),
            AiCommand::Explain => (I::EXPLAIN, None, T::Ai),
            AiCommand::Summarize => (I::SUMMARIZE, None, T::Ai),
            AiCommand::GenerateMap => (I::MAP, Some(I::AI), T::Ai),
            AiCommand::Rewrite => (I::EDIT, Some(I::AI), T::Ai),
            AiCommand::Translate => (I::TRANSLATE, None, T::Ai),
            AiCommand::Grow => (I::SUBTOPIC, Some(I::AI), T::Ai),
            AiCommand::Reorganize => (I::SORT, Some(I::AI), T::Ai),
            AiCommand::Refine => (I::AI, None, T::Ai),
        },
        Cmd::Find => (I::SEARCH, None, T::Edit),
        Cmd::FindReplace => (I::FIND_REPLACE, None, T::Edit),
        Cmd::Palette => (I::KEYBOARD, Some(I::SEARCH), T::Edit),
        Cmd::Settings => (I::SETTINGS, None, T::View),
        Cmd::ToggleOutliner => (I::OUTLINER, None, T::View),
        Cmd::ToggleStatusBar => (I::STATUS_BAR, None, T::View),
        Cmd::ToggleZen => (I::ZEN, None, T::View),
        Cmd::ToggleFormatPanel => (I::FORMAT, None, T::Format),
        Cmd::ToggleSymbolsPanel => (I::MARKER, None, T::Format),
        Cmd::TopicStyle => (I::TOPIC_STYLE, None, T::Format),
        Cmd::SetTheme(_) => (I::THEME, None, T::Format),
        Cmd::EditCustomTheme => (I::THEME, Some(I::EDIT), T::Format),
        Cmd::ApplyCustomTheme(_) => (I::THEME, Some(I::CHECK), T::Format),
        Cmd::ApplyQuickStyle(_) => (I::QUICK_STYLE, None, T::Format),
        Cmd::MapShot => (I::CAMERA, None, T::File),
        Cmd::MergeFiles => (I::MERGE, None, T::File),
        Cmd::OpenLink => (I::OPEN_LINK, None, T::Edit),
        Cmd::Home => (I::HOME, None, T::View),
        Cmd::CloseTab(_) => (I::CLOSE, None, T::Danger),
        Cmd::CloseWindow => (I::WINDOW, Some(I::CLOSE), T::Danger),
        Cmd::MoveToNewWindow => (I::WINDOW, None, T::View),
        Cmd::NextDoc => (I::NEW_FILE, Some(I::ARROW_RIGHT), T::View),
        Cmd::Quit => (I::QUIT, None, T::Danger),
        Cmd::EditField(FocusField::Notes) => (I::NOTE, None, T::Edit),
        Cmd::EditField(FocusField::Hyperlink) => (I::LINK, None, T::Edit),
        Cmd::Labels => (I::LABEL, None, T::Edit),
        Cmd::Canvas(action) => match action {
            Action::Undo => (I::UNDO, None, T::Edit),
            Action::Redo => (I::REDO, None, T::Edit),
            Action::Copy => (I::COPY, None, T::Edit),
            Action::Duplicate => (I::DUPLICATE, None, T::Create),
            Action::Cut => (I::CUT, None, T::Edit),
            Action::Paste(_) => (I::PASTE, None, T::Edit),
            Action::InsertSibling => (I::TOPIC, None, T::Create),
            Action::InsertSiblingBefore => (I::TOPIC_BEFORE, None, T::Create),
            Action::InsertParent => (I::PARENT_TOPIC, None, T::Create),
            Action::InsertFloating(_) => (I::FLOATING_TOPIC, None, T::Create),
            Action::InsertChild => (I::SUBTOPIC, None, T::Create),
            Action::StartEdit => (I::EDIT, None, T::Edit),
            Action::MoveUp => (I::MOVE_UP, None, T::Edit),
            Action::MoveDown => (I::MOVE_DOWN, None, T::Edit),
            Action::Indent => (I::INDENT, None, T::Edit),
            Action::Outdent => (I::OUTDENT, None, T::Edit),
            Action::Delete => (I::TRASH, None, T::Danger),
            Action::DeleteKeepChildren => (I::TRASH, Some(I::BRANCH), T::Danger),
            Action::StartRelationship => (I::RELATIONSHIP, None, T::Create),
            Action::AddBoundary => (I::BOUNDARY, None, T::Create),
            Action::AddSummary => (I::SUMMARY, None, T::Create),
            Action::ToggleTodo => (I::TODO, None, T::Create),
            Action::ToggleCollapse => (I::COLLAPSE, None, T::View),
            Action::ExpandAll => (I::EXPAND_ALL, None, T::View),
            Action::CollapseAll => (I::COLLAPSE_ALL, None, T::View),
            Action::ToggleCollapseAll => (I::EXPAND, Some(I::INVERT), T::View),
            Action::SelectAll => (I::SELECT_ALL, None, T::Edit),
            Action::SelectRoot => (I::ROOT, None, T::Edit),
            Action::SelectNone => (I::SELECT_NONE, None, T::Edit),
            Action::SelectSiblings => (I::TOPIC, Some(I::SELECT_ALL), T::Edit),
            Action::SelectDescendants => (I::SUBTOPIC, Some(I::SELECT_ALL), T::Edit),
            Action::InvertSelection => (I::INVERT, None, T::Edit),
            Action::SortAscending => (I::SORT, Some(I::ARROW_UP), T::Edit),
            Action::SortDescending => (I::SORT, Some(I::ARROW_DOWN), T::Edit),
            Action::AlignFloating(arrangement) => (
                match arrangement {
                    FloatingArrangement::Left => I::ALIGN_LEFT,
                    FloatingArrangement::HorizontalCenter => I::ALIGN_CENTER,
                    FloatingArrangement::Right => I::ALIGN_RIGHT,
                    FloatingArrangement::Top => I::ALIGN_TOP,
                    FloatingArrangement::VerticalCenter => I::ALIGN_MIDDLE,
                    FloatingArrangement::Bottom => I::ALIGN_BOTTOM,
                    FloatingArrangement::DistributeHorizontal => I::DISTRIBUTE_HORIZONTAL,
                    FloatingArrangement::DistributeVertical => I::DISTRIBUTE_VERTICAL,
                },
                None,
                T::Edit,
            ),
            Action::ToggleMarker(_) => (I::MARKER, Some(I::ADD), T::Format),
            Action::ToggleMarkerColor(..) | Action::SetMarkerColor(..) => {
                (I::MARKER, Some(I::FORMAT), T::Format)
            }
            Action::RemoveMarker(..) => (I::MARKER, Some(I::REMOVE), T::Danger),
            Action::Navigate(direction) => (
                match direction {
                    thinoda_core::navigation::Direction::Left => I::ARROW_LEFT,
                    thinoda_core::navigation::Direction::Right => I::ARROW_RIGHT,
                    thinoda_core::navigation::Direction::Up => I::ARROW_UP,
                    thinoda_core::navigation::Direction::Down => I::ARROW_DOWN,
                },
                None,
                T::View,
            ),
            Action::ZoomIn => (I::ZOOM_IN, None, T::View),
            Action::ZoomOut => (I::ZOOM_OUT, None, T::View),
            Action::ZoomReset => (I::ZOOM_RESET, None, T::View),
            Action::Fit => (I::FIT, None, T::View),
        },
        Cmd::QuickStart => (I::QUICK_START, None, T::Help),
        Cmd::UserGuide => (I::USER_GUIDE, None, T::Help),
        Cmd::About => (I::INFO, None, T::Help),
        Cmd::Shortcuts => (I::KEYBOARD, None, T::Help),
        Cmd::Feedback => (I::FEEDBACK, None, T::Help),
        Cmd::ProjectNews => (I::NEWS, None, T::Help),
    };
    CommandIcon {
        symbol,
        badge,
        tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ThinodaApp, menus::MenuEntry};

    #[test]
    fn sibling_menu_actions_have_distinct_semantic_icons() {
        fn check(entries: &[MenuEntry]) {
            let mut commands: Vec<(&String, CommandIcon)> = Vec::new();
            for entry in entries {
                match entry {
                    MenuEntry::Command { label, command } => {
                        // Format/template/path entries intentionally share a command family.
                        if !matches!(
                            command,
                            Cmd::Export(_) | Cmd::NewFromTemplate(_) | Cmd::OpenPath(_)
                        ) {
                            let icon = command_glyph(command);
                            assert_ne!(icon.tone, T::Neutral, "{label}");
                            for (previous, previous_icon) in &commands {
                                assert_ne!(
                                    (icon.symbol, icon.badge),
                                    (previous_icon.symbol, previous_icon.badge),
                                    "{previous} and {label} share an icon"
                                );
                            }
                            commands.push((label, icon));
                        }
                    }
                    MenuEntry::Submenu { entries, .. } => check(entries),
                    _ => {}
                }
            }
        }
        check(&ThinodaApp::headless().main_menu_entries());
    }

    #[test]
    fn surfaces_use_the_shared_icon_for_the_same_action() {
        use thinoda_canvas::Action;
        let pairs = [
            (Cmd::Canvas(Action::Fit), I::FIT),
            (Cmd::ToggleOutliner, I::OUTLINER),
            (Cmd::Canvas(Action::InsertSibling), I::TOPIC),
            (Cmd::Canvas(Action::InsertChild), I::SUBTOPIC),
            (Cmd::Canvas(Action::StartRelationship), I::RELATIONSHIP),
            (Cmd::Canvas(Action::AddBoundary), I::BOUNDARY),
            (Cmd::Canvas(Action::AddSummary), I::SUMMARY),
            (Cmd::Save, I::SAVE),
            (Cmd::Canvas(Action::Undo), I::UNDO),
            (Cmd::Canvas(Action::Redo), I::REDO),
            (Cmd::Print, I::PRINT),
            (Cmd::Find, I::SEARCH),
            (Cmd::ToggleZen, I::ZEN),
        ];
        for (command, symbol) in pairs {
            assert_eq!(command_glyph(&command).symbol, symbol, "{command:?}");
        }
        assert_ne!(I::FIT, I::MAP);
        assert_ne!(I::FIT, I::OUTLINER);
    }
}

use crate::{
    ai::AiCommand,
    app::{Cmd, FocusField},
};
use thinoda_canvas::{Action, FloatingArrangement};
use thinoda_ui::ribbon::{Badge as B, CommandIcon, Glyph as G, IconTone as T};

pub(crate) fn command_glyph(command: &Cmd) -> CommandIcon {
    let (glyph, badge, tone) = match command {
        Cmd::New => (G::File, Some(B::Plus), T::File),
        Cmd::NewFromTemplate(_) => (G::Template, None, T::File),
        Cmd::Open | Cmd::OpenPath(_) => (G::Folder, None, T::File),
        Cmd::ShowInFolder => (G::Folder, Some(B::Eye), T::File),
        Cmd::Save => (G::Save, None, T::File),
        Cmd::SaveAs => (G::Save, Some(B::Pencil), T::File),
        Cmd::SaveAll => (G::Save, Some(B::Stack), T::File),
        Cmd::SaveSheetAs => (G::Save, Some(B::Branch), T::File),
        Cmd::Print => (G::Print, None, T::File),
        Cmd::TaskPlanning => (G::Todo, Some(B::Pencil), T::Create),
        Cmd::CreateZone => (G::Layout, Some(B::Plus), T::Create),
        Cmd::EditZone => (G::Layout, Some(B::Pencil), T::Edit),
        Cmd::Zones => (G::Layout, Some(B::Stack), T::View),
        Cmd::Pitch => (G::Window, Some(B::Right), T::View),
        Cmd::PitchPresenter => (G::Window, Some(B::Eye), T::View),
        Cmd::EditPresentation => (G::Sheet, Some(B::Pencil), T::Edit),
        Cmd::WorkCalendar => (G::Settings, Some(B::Check), T::View),
        Cmd::Gantt => (G::Outline, Some(B::Right), T::View),
        Cmd::Export(_) => (G::Export, None, T::File),
        Cmd::Import => (G::File, Some(B::Down), T::File),
        Cmd::MenuPaste => (G::Clipboard, None, T::Edit),
        Cmd::PasteSheet => (G::Clipboard, Some(B::Branch), T::Edit),
        Cmd::CopySheet => (G::Copy, Some(B::Branch), T::Edit),
        Cmd::DuplicateSheet => (G::Copy, Some(B::Plus), T::Create),
        Cmd::MaskedCopy => (G::Mask, None, T::File),
        Cmd::CopyStyle => (G::Format, Some(B::Stack), T::Format),
        Cmd::PasteStyle => (G::Clipboard, Some(B::Pencil), T::Format),
        Cmd::ResetStyle => (G::Format, Some(B::Reset), T::Format),
        Cmd::SaveQuickStyle => (G::Format, Some(B::Save), T::Format),
        Cmd::NewSheet => (G::Sheet, Some(B::Plus), T::Create),
        Cmd::NextSheet => (G::Sheet, Some(B::Right), T::View),
        Cmd::PreviousSheet => (G::Sheet, Some(B::Left), T::View),
        Cmd::RenameSheet => (G::Sheet, Some(B::Pencil), T::Edit),
        Cmd::NewSheetFromTopic => (G::Sheet, Some(B::Branch), T::Create),
        Cmd::DeleteSheet => (G::Sheet, Some(B::Minus), T::Danger),
        Cmd::AiChat => (G::Window, Some(B::Spark), T::Ai),
        Cmd::Ai(action) => match action {
            AiCommand::Brainstorm => (G::Child, Some(B::Spark), T::Ai),
            AiCommand::Breakdown => (G::Todo, Some(B::Spark), T::Ai),
            AiCommand::Explain => (G::Notes, Some(B::Spark), T::Ai),
            AiCommand::Summarize => (G::Summary, Some(B::Spark), T::Ai),
            AiCommand::GenerateMap => (G::Topic, Some(B::Spark), T::Ai),
            AiCommand::Rewrite => (G::Topic, Some(B::Pencil), T::Ai),
            AiCommand::Translate => (G::Sheet, Some(B::Spark), T::Ai),
            AiCommand::Grow => (G::Child, Some(B::Plus), T::Ai),
            AiCommand::Reorganize => (G::Outline, Some(B::Spark), T::Ai),
            AiCommand::Refine => (G::Summary, Some(B::Pencil), T::Ai),
        },
        Cmd::Find => (G::Search, None, T::Edit),
        Cmd::FindReplace => (G::Search, Some(B::Reset), T::Edit),
        Cmd::Palette => (G::Keyboard, Some(B::Spark), T::Edit),
        Cmd::Settings => (G::Settings, None, T::View),
        Cmd::ToggleOutliner => (G::Outline, None, T::View),
        Cmd::ToggleStatusBar => (G::StatusBar, None, T::View),
        Cmd::ToggleZen => (G::Fit, Some(B::Eye), T::View),
        Cmd::ToggleFormatPanel => (G::Layout, Some(B::Pencil), T::Format),
        Cmd::ToggleSymbolsPanel => (G::Symbols, None, T::Format),
        Cmd::TopicStyle => (G::Format, None, T::Format),
        Cmd::SetTheme(_) => (G::Format, Some(B::Eye), T::Format),
        Cmd::EditCustomTheme => (G::Format, Some(B::Pencil), T::Format),
        Cmd::ApplyCustomTheme(_) => (G::Format, Some(B::Check), T::Format),
        Cmd::ApplyQuickStyle(_) => (G::Format, Some(B::Down), T::Format),
        Cmd::MapShot => (G::Camera, None, T::File),
        Cmd::MergeFiles => (G::Merge, None, T::File),
        Cmd::OpenLink => (G::Link, Some(B::Right), T::Edit),
        Cmd::Home => (G::Home, None, T::View),
        Cmd::CloseTab(_) => (G::Close, None, T::Danger),
        Cmd::CloseWindow => (G::Window, Some(B::Minus), T::Danger),
        Cmd::MoveToNewWindow => (G::Window, None, T::View),
        Cmd::NextDoc => (G::File, Some(B::Right), T::View),
        Cmd::Quit => (G::Power, None, T::Danger),
        Cmd::EditField(FocusField::Notes) => (G::Notes, None, T::Edit),
        Cmd::EditField(FocusField::Hyperlink) => (G::Link, None, T::Edit),
        Cmd::Labels => (G::Tag, None, T::Edit),
        Cmd::Canvas(action) => match action {
            Action::Undo => (G::Undo, None, T::Edit),
            Action::Redo => (G::Redo, None, T::Edit),
            Action::Copy => (G::Copy, None, T::Edit),
            Action::Duplicate => (G::Copy, Some(B::Plus), T::Create),
            Action::Cut => (G::Cut, None, T::Edit),
            Action::Paste(_) => (G::Clipboard, None, T::Edit),
            Action::InsertSibling => (G::Topic, Some(B::Plus), T::Create),
            Action::InsertSiblingBefore => (G::Topic, Some(B::Before), T::Create),
            Action::InsertParent => (G::Topic, Some(B::Parent), T::Create),
            Action::InsertFloating(_) => (G::Topic, Some(B::Floating), T::Create),
            Action::InsertChild => (G::Child, None, T::Create),
            Action::StartEdit => (G::Edit, None, T::Edit),
            Action::MoveUp => (G::Up, None, T::Edit),
            Action::MoveDown => (G::Down, None, T::Edit),
            Action::Indent => (G::Indent, None, T::Edit),
            Action::Outdent => (G::Outdent, None, T::Edit),
            Action::Delete => (G::Delete, None, T::Danger),
            Action::DeleteKeepChildren => (G::Delete, Some(B::Branch), T::Danger),
            Action::StartRelationship => (G::Relationship, None, T::Create),
            Action::AddBoundary => (G::Boundary, None, T::Create),
            Action::AddSummary => (G::Summary, None, T::Create),
            Action::ToggleTodo => (G::Todo, None, T::Create),
            Action::ToggleCollapse => (G::Topic, Some(B::Collapse), T::View),
            Action::ExpandAll => (G::Layout, Some(B::Expand), T::View),
            Action::CollapseAll => (G::Layout, Some(B::Collapse), T::View),
            Action::ToggleCollapseAll => (G::Layout, Some(B::Invert), T::View),
            Action::SelectAll => (G::Layout, Some(B::SelectAll), T::Edit),
            Action::SelectRoot => (G::Topic, Some(B::Root), T::Edit),
            Action::SelectNone => (G::Layout, Some(B::SelectNone), T::Edit),
            Action::SelectSiblings => (G::Topic, Some(B::SelectAll), T::Edit),
            Action::SelectDescendants => (G::Child, Some(B::SelectAll), T::Edit),
            Action::InvertSelection => (G::Layout, Some(B::Invert), T::Edit),
            Action::SortAscending => (G::Outline, Some(B::Up), T::Edit),
            Action::SortDescending => (G::Outline, Some(B::Down), T::Edit),
            Action::AlignFloating(arrangement) => (
                G::Layout,
                Some(match arrangement {
                    FloatingArrangement::Left => B::AlignLeft,
                    FloatingArrangement::HorizontalCenter => B::AlignCenter,
                    FloatingArrangement::Right => B::AlignRight,
                    FloatingArrangement::Top => B::AlignTop,
                    FloatingArrangement::VerticalCenter => B::AlignMiddle,
                    FloatingArrangement::Bottom => B::AlignBottom,
                    FloatingArrangement::DistributeHorizontal => B::DistributeHorizontal,
                    FloatingArrangement::DistributeVertical => B::DistributeVertical,
                }),
                T::Edit,
            ),
            Action::ToggleMarker(_) => (G::Symbols, Some(B::Plus), T::Format),
            Action::ToggleMarkerColor(..) | Action::SetMarkerColor(..) => {
                (G::Symbols, Some(B::Pencil), T::Format)
            }
            Action::RemoveMarker(..) => (G::Symbols, Some(B::Minus), T::Danger),
            Action::Navigate(direction) => (
                G::Topic,
                Some(match direction {
                    thinoda_core::navigation::Direction::Left => B::Left,
                    thinoda_core::navigation::Direction::Right => B::Right,
                    thinoda_core::navigation::Direction::Up => B::Up,
                    thinoda_core::navigation::Direction::Down => B::Down,
                }),
                T::View,
            ),
            Action::ZoomIn => (G::Zoom, None, T::View),
            Action::ZoomOut => (G::ZoomOut, None, T::View),
            Action::ZoomReset => (G::ZoomReset, None, T::View),
            Action::Fit => (G::Fit, None, T::View),
        },
        Cmd::QuickStart => (G::Play, None, T::Help),
        Cmd::UserGuide => (G::Book, None, T::Help),
        Cmd::About => (G::Info, None, T::Help),
        Cmd::Shortcuts => (G::Keyboard, None, T::Help),
        Cmd::Feedback => (G::Message, None, T::Help),
        Cmd::ProjectNews => (G::News, None, T::Help),
    };
    CommandIcon { glyph, badge, tone }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ThinodaApp, menus::MenuEntry};

    #[test]
    fn sibling_menu_actions_have_distinct_semantic_icons() {
        fn check(entries: &[MenuEntry]) {
            let mut commands = Vec::new();
            for entry in entries {
                match entry {
                    MenuEntry::Command { label, command } => {
                        // Format/template/path entries intentionally share a command family.
                        if !matches!(
                            command,
                            Cmd::Export(_) | Cmd::NewFromTemplate(_) | Cmd::OpenPath(_)
                        ) {
                            let icon = command_glyph(command);
                            let mut h = egui_kittest::Harness::new_ui(move |ui| {
                                icon.paint(
                                    ui,
                                    egui::Rect::from_min_size(
                                        egui::pos2(8., 8.),
                                        egui::vec2(32., 32.),
                                    ),
                                    egui::Color32::WHITE,
                                );
                            });
                            h.run_steps(2);
                            let painted = format!("{:?}", h.output().shapes);
                            assert_ne!(icon.tone, T::Neutral, "{label}");
                            for (previous, previous_icon, previous_painted) in &commands {
                                assert_ne!(
                                    &icon, previous_icon,
                                    "{previous} and {label} share an icon"
                                );
                                assert_ne!(
                                    &painted, previous_painted,
                                    "{previous} and {label} paint the same shape"
                                );
                            }
                            commands.push((label, icon, painted));
                        }
                    }
                    MenuEntry::Submenu { entries, .. } => check(entries),
                    _ => {}
                }
            }
        }
        check(&ThinodaApp::headless().main_menu_entries());
    }
}

use crate::app::Cmd;
use thinoda_canvas::Action;
use thinoda_ui::ribbon::Glyph;

pub(crate) fn command_glyph(command: &Cmd) -> Glyph {
    match command {
        Cmd::New | Cmd::NewFromTemplate(_) => Glyph::File,
        Cmd::Open | Cmd::OpenPath(_) | Cmd::ShowInFolder => Glyph::Folder,
        Cmd::Save | Cmd::SaveAs | Cmd::SaveAll | Cmd::SaveSheetAs => Glyph::Save,
        Cmd::Print => Glyph::Print,
        Cmd::Export(_) | Cmd::Import => Glyph::Export,
        Cmd::MenuPaste | Cmd::PasteSheet => Glyph::Clipboard,
        Cmd::CopyStyle | Cmd::CopySheet | Cmd::DuplicateSheet | Cmd::MaskedCopy => Glyph::Copy,
        Cmd::PasteStyle | Cmd::ResetStyle | Cmd::SaveQuickStyle => Glyph::Format,
        Cmd::NewSheet
        | Cmd::NextSheet
        | Cmd::PreviousSheet
        | Cmd::RenameSheet
        | Cmd::NewSheetFromTopic => Glyph::Sheet,
        Cmd::DeleteSheet => Glyph::Delete,
        Cmd::Ai(_) => Glyph::Spark,
        Cmd::Find | Cmd::FindReplace | Cmd::Palette => Glyph::Search,
        Cmd::Settings => Glyph::Settings,
        Cmd::ToggleOutliner | Cmd::ToggleStatusBar => Glyph::Layout,
        Cmd::ToggleZen => Glyph::Zoom,
        Cmd::ToggleFormatPanel
        | Cmd::ToggleSymbolsPanel
        | Cmd::TopicStyle
        | Cmd::SetTheme(_)
        | Cmd::EditCustomTheme
        | Cmd::ApplyCustomTheme(_)
        | Cmd::ApplyQuickStyle(_) => Glyph::Format,
        Cmd::MapShot => Glyph::Camera,
        Cmd::MergeFiles => Glyph::Topic,
        Cmd::OpenLink => Glyph::Link,
        Cmd::Home => Glyph::Home,
        Cmd::CloseTab(_) => Glyph::Close,
        Cmd::NextDoc => Glyph::File,
        Cmd::Quit => Glyph::Power,
        Cmd::EditField(_) | Cmd::Labels => Glyph::Edit,
        Cmd::Canvas(action) => match action {
            Action::Undo => Glyph::Undo,
            Action::Redo => Glyph::Redo,
            Action::Copy | Action::Duplicate => Glyph::Copy,
            Action::Cut => Glyph::Cut,
            Action::InsertSibling | Action::InsertSiblingBefore | Action::InsertParent => {
                Glyph::Topic
            }
            Action::InsertChild => Glyph::Child,
            Action::StartEdit => Glyph::Edit,
            Action::MoveUp => Glyph::Up,
            Action::MoveDown => Glyph::Down,
            Action::Indent => Glyph::Indent,
            Action::Outdent => Glyph::Outdent,
            Action::Delete | Action::DeleteKeepChildren => Glyph::Delete,
            Action::StartRelationship => Glyph::Link,
            Action::ZoomIn | Action::ZoomOut | Action::ZoomReset | Action::Fit => Glyph::Zoom,
            _ => Glyph::Layout,
        },
        Cmd::QuickStart
        | Cmd::UserGuide
        | Cmd::About
        | Cmd::Shortcuts
        | Cmd::Feedback
        | Cmd::ProjectNews => Glyph::Help,
    }
}

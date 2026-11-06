//! Target-specific canvas context menus.
//!
//! Each clicked target (blank canvas, topic, board item, annotation or zone) gets a
//! fixed, task-oriented order: create, clipboard, frequent edits, grouped submenus,
//! the command palette as an escape hatch, and destructive actions last. Optional
//! usage counts (see `menu_usage`) are reporting only and never reorder entries.

use crate::{
    ai::AiCommand,
    app::{Cmd, FocusField, ThinodaApp},
};
use thinoda_canvas::{Action, BoardAction, BoardTool, FloatingArrangement, ObjectSelection};
use thinoda_core::{Band, GridKind, ShapeKind, SheetRootMode, Vec2, ZOrder, board::BoardMember};

/// Bounded menu target used for layout and for optional usage-count keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MenuContext {
    Blank,
    Topic,
    Board,
    Annotation,
    Zone,
}

impl MenuContext {
    pub const ALL: [Self; 5] = [
        Self::Blank,
        Self::Topic,
        Self::Board,
        Self::Annotation,
        Self::Zone,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Topic => "topic",
            Self::Board => "board",
            Self::Annotation => "annotation",
            Self::Zone => "zone",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CtxEntry {
    Command {
        label: &'static str,
        command: Cmd,
        /// `Some` renders a check or radio state for toggles.
        checked: Option<bool>,
    },
    Submenu {
        label: &'static str,
        icon: Option<&'static str>,
        entries: Vec<CtxEntry>,
    },
    Separator,
    Caption(&'static str),
    /// The rich-text toolbar for sticky and text cards (not a `Cmd`).
    CardText,
}

fn cmd(label: &'static str, command: Cmd) -> CtxEntry {
    CtxEntry::Command {
        label,
        command,
        checked: None,
    }
}

fn check(label: &'static str, command: Cmd, checked: bool) -> CtxEntry {
    CtxEntry::Command {
        label,
        command,
        checked: Some(checked),
    }
}

fn canvas(label: &'static str, action: Action) -> CtxEntry {
    cmd(label, Cmd::Canvas(action))
}

fn submenu(label: &'static str, entries: Vec<CtxEntry>) -> CtxEntry {
    CtxEntry::Submenu {
        label,
        icon: None,
        entries,
    }
}

/// Joins non-empty groups with single separators.
fn grouped(groups: Vec<Vec<CtxEntry>>) -> Vec<CtxEntry> {
    let mut out = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !out.is_empty() {
            out.push(CtxEntry::Separator);
        }
        out.extend(group);
    }
    out
}

/// Facts about the current selection that decide which groups are relevant.
#[derive(Clone, Copy, Debug, Default)]
struct Target {
    topics: bool,
    single: bool,
    cards: bool,
    objects: bool,
    floating: bool,
    branch: bool,
    link: bool,
    summary: bool,
    locked: bool,
    unlocked: bool,
}

impl ThinodaApp {
    pub(crate) fn context_menu_target(&self) -> MenuContext {
        let Some(doc) = self.doc() else {
            return MenuContext::Blank;
        };
        let view = &doc.view;
        let sheet = doc.doc.sheet();
        match view.selected_object {
            Some(ObjectSelection::Zone(_)) => return MenuContext::Zone,
            Some(_) => return MenuContext::Annotation,
            None => {}
        }
        if !view.board_selection.is_empty()
            || view
                .selection
                .iter()
                .any(|id| sheet.board_member_for_topic(*id).is_some())
        {
            MenuContext::Board
        } else if view.selection.is_empty() {
            MenuContext::Blank
        } else {
            MenuContext::Topic
        }
    }

    fn context_target(&self) -> Target {
        let Some(doc) = self.doc() else {
            return Target::default();
        };
        let view = &doc.view;
        let sheet = doc.doc.sheet();
        let primary = view.primary();
        let mut members: Vec<BoardMember> = view
            .board_selection
            .iter()
            .copied()
            .map(BoardMember::Object)
            .collect();
        members.extend(
            view.selection
                .iter()
                .filter_map(|id| sheet.board_member_for_topic(*id)),
        );
        Target {
            topics: !view.selection.is_empty(),
            single: view.selection.len() == 1 && view.board_selection.is_empty(),
            cards: view
                .selection
                .iter()
                .any(|id| sheet.board_member_for_topic(*id).is_some()),
            objects: !view.board_selection.is_empty(),
            floating: view
                .selection
                .iter()
                .any(|id| sheet.is_floating(*id) && sheet.board_member_for_topic(*id).is_none()),
            branch: view
                .selection
                .iter()
                .any(|id| !sheet.children(*id).is_empty()),
            link: primary
                .and_then(|id| sheet.topic(id))
                .is_some_and(|topic| topic.hyperlink.is_some()),
            summary: view.selection.len() == 1
                && sheet.summaries.iter().any(|s| Some(s.topic) == primary),
            locked: members
                .iter()
                .any(|member| sheet.is_board_member_locked(*member)),
            unlocked: members
                .iter()
                .any(|member| !sheet.is_board_member_locked(*member)),
        }
    }

    /// The complete, ordered context menu for the current canvas target.
    pub(crate) fn canvas_context_menu(&self) -> (MenuContext, Vec<CtxEntry>) {
        let context = self.context_menu_target();
        let target = self.context_target();
        let entries = match context {
            MenuContext::Zone => grouped(vec![
                vec![
                    cmd("Edit Zone", Cmd::EditZone),
                    cmd("Zoom to selection", Cmd::BoardZoomSelection),
                    cmd("Manage Zones", Cmd::Zones),
                ],
                vec![cmd("Remove Zone", Cmd::RemoveZone)],
            ]),
            MenuContext::Annotation => self.annotation_menu(),
            MenuContext::Blank => self.blank_menu(),
            MenuContext::Topic => self.topic_menu(target),
            MenuContext::Board => self.board_menu(target),
        };
        (context, entries)
    }

    fn annotation_menu(&self) -> Vec<CtxEntry> {
        let selected = self.doc().and_then(|doc| doc.view.selected_object);
        let mut delete = Vec::new();
        let label = match selected {
            Some(ObjectSelection::Boundary(_)) => "Delete Boundary",
            Some(ObjectSelection::Summary(_)) => {
                delete.push(CtxEntry::Caption(
                    "Deletes the summary topic and its children, not the covered branches.",
                ));
                "Delete Summary"
            }
            _ => "Delete Relationship",
        };
        delete.push(canvas(label, Action::Delete));
        grouped(vec![
            vec![self.view_menu()],
            vec![cmd("Command Palette…", Cmd::Palette)],
            delete,
        ])
    }

    fn blank_menu(&self) -> Vec<CtxEntry> {
        let point = self
            .doc()
            .and_then(|doc| doc.view.context_point)
            .unwrap_or(Vec2::new(0.0, -260.0));
        let tool = |label, tool| cmd(label, Cmd::BoardTool(tool));
        grouped(vec![
            vec![
                canvas("Floating Topic", Action::InsertFloating(point)),
                cmd("Paste", Cmd::MenuPaste),
            ],
            vec![
                submenu(
                    "Add",
                    grouped(vec![
                        vec![
                            tool("Sticky note", BoardTool::Sticky),
                            tool("Text card", BoardTool::Text),
                            cmd("Image card", Cmd::BoardImage),
                        ],
                        vec![
                            tool("Shape tool", BoardTool::Shape(ShapeKind::Rectangle)),
                            tool("Connector tool", BoardTool::Connector),
                            tool("Frame tool", BoardTool::Frame),
                        ],
                    ]),
                ),
                submenu(
                    "Draw",
                    grouped(vec![
                        vec![
                            tool("Pen", BoardTool::Pen),
                            tool("Highlighter", BoardTool::Highlighter),
                        ],
                        vec![tool("Lasso", BoardTool::Lasso)],
                        vec![
                            tool("Object eraser", BoardTool::ObjectEraser),
                            tool("Pixel eraser", BoardTool::PixelEraser),
                        ],
                    ]),
                ),
                self.short_select_menu(),
                self.view_menu(),
                cmd("Manage Zones", Cmd::Zones),
            ],
            vec![CtxEntry::Submenu {
                label: "AI",
                icon: Some(thinoda_ui::icons::AI),
                entries: vec![cmd(
                    AiCommand::GenerateMap.label(),
                    Cmd::Ai(AiCommand::GenerateMap),
                )],
            }],
            vec![cmd("Command Palette…", Cmd::Palette)],
        ])
    }

    fn fim_group(&self) -> Vec<CtxEntry> {
        if !self.command_available(&Cmd::EditFimNode) {
            return Vec::new();
        }
        vec![
            cmd("Edit FIM node", Cmd::EditFimNode),
            cmd("Add FIM child", Cmd::AddFimChild),
            cmd("Insert FIM node before", Cmd::InsertFimBefore),
            cmd("Insert FIM node after", Cmd::InsertFimAfter),
        ]
    }

    fn topic_menu(&self, target: Target) -> Vec<CtxEntry> {
        let mut submenus = Vec::new();
        if target.link {
            submenus.push(cmd("Open Link", Cmd::OpenLink));
        }
        submenus.extend([
            self.add_menu(),
            self.structure_menu(),
            self.format_menu(target),
        ]);
        if target.floating {
            submenus.push(self.arrange_menu(false));
        }
        submenus.extend([
            self.select_menu(),
            self.zones_menu(),
            self.ai_menu(),
            self.view_menu(),
        ]);
        let mut collapse = Vec::new();
        if target.branch {
            collapse.push(canvas("Collapse / Expand", Action::ToggleCollapse));
        }
        grouped(vec![
            self.fim_group(),
            vec![
                canvas("Topic", Action::InsertSibling),
                canvas("Subtopic", Action::InsertChild),
                canvas("Parent Topic", Action::InsertParent),
            ],
            clipboard(),
            collapse,
            submenus,
            vec![cmd("Command Palette…", Cmd::Palette)],
            delete_group(target),
        ])
    }

    fn board_menu(&self, target: Target) -> Vec<CtxEntry> {
        let mut create = Vec::new();
        if target.cards && !target.objects {
            create.push(canvas("Subtopic", Action::InsertChild));
        }
        if target.link {
            create.push(cmd("Open Link", Cmd::OpenLink));
        }
        let mut organize = vec![
            cmd(
                "Group board selection",
                Cmd::BoardAction(BoardAction::Group),
            ),
            cmd(
                "Ungroup board selection",
                Cmd::BoardAction(BoardAction::Ungroup),
            ),
        ];
        let lock = cmd(
            "Lock board selection",
            Cmd::BoardAction(BoardAction::Lock(true)),
        );
        let unlock = cmd(
            "Unlock board selection",
            Cmd::BoardAction(BoardAction::Lock(false)),
        );
        match (target.locked, target.unlocked) {
            (true, true) => organize.push(submenu("Protection", vec![lock, unlock])),
            (true, false) => organize.push(unlock),
            _ => organize.push(lock),
        }
        let mut submenus = Vec::new();
        if target.cards && !target.objects {
            submenus.push(self.add_menu());
        }
        submenus.push(self.arrange_menu(true));
        if target.objects {
            submenus.push(layers_menu());
        }
        submenus.extend([self.zones_menu(), self.short_select_menu()]);
        if target.cards && !target.objects {
            submenus.push(self.ai_menu());
        }
        submenus.push(self.view_menu());
        let mut clipboard = clipboard();
        clipboard.insert(2, cmd("Copy as draw.io", Cmd::CopyAsDrawio));
        grouped(vec![
            create,
            clipboard,
            vec![self.format_menu(target)],
            organize,
            submenus,
            vec![cmd("Command Palette…", Cmd::Palette)],
            vec![canvas("Delete", Action::Delete)],
        ])
    }

    fn add_menu(&self) -> CtxEntry {
        submenu(
            "Add",
            grouped(vec![
                vec![
                    canvas("To-Do", Action::ToggleTodo),
                    cmd("Task Planning", Cmd::TaskPlanning),
                ],
                vec![
                    cmd("Edit notes", Cmd::EditField(FocusField::Notes)),
                    cmd("Edit hyperlink", Cmd::EditField(FocusField::Hyperlink)),
                    cmd("Labels", Cmd::Labels),
                ],
                vec![
                    cmd("Local Image", Cmd::LocalImage),
                    cmd("Attachment", Cmd::Attachment),
                    cmd("Save/Download attachment", Cmd::SaveAttachment),
                ],
                vec![
                    canvas("Relationship", Action::StartRelationship),
                    canvas("Boundary", Action::AddBoundary),
                    canvas("Summary", Action::AddSummary),
                ],
            ]),
        )
    }

    fn structure_menu(&self) -> CtxEntry {
        submenu(
            "Structure",
            grouped(vec![
                vec![
                    canvas("Indent", Action::Indent),
                    canvas("Outdent", Action::Outdent),
                ],
                vec![
                    canvas("Move Up", Action::MoveUp),
                    canvas("Move Down", Action::MoveDown),
                ],
                vec![
                    canvas("Sort Children Ascending", Action::SortAscending),
                    canvas("Sort Children Descending", Action::SortDescending),
                ],
                vec![cmd("New Sheet from Topic", Cmd::NewSheetFromTopic)],
            ]),
        )
    }

    fn format_menu(&self, target: Target) -> CtxEntry {
        let mut first = Vec::new();
        if target.single {
            first.push(cmd("Topic style", Cmd::TopicStyle));
        }
        if self.card_text_target_available() {
            first.push(CtxEntry::CardText);
        }
        first.push(cmd("Format Panel", Cmd::ToggleFormatPanel));
        let mut last = vec![cmd("Reset Style", Cmd::ResetStyle)];
        if target.topics && !target.objects {
            last.push(cmd("Save Quick Style…", Cmd::SaveQuickStyle));
        }
        submenu(
            "Format",
            grouped(vec![
                first,
                vec![
                    cmd("Copy Style", Cmd::CopyStyle),
                    cmd("Paste Style", Cmd::PasteStyle),
                ],
                last,
            ]),
        )
    }

    /// Floating topics use canvas alignment; board selections also rotate.
    fn arrange_menu(&self, board: bool) -> CtxEntry {
        let (mut align, mut distribute) = (Vec::new(), Vec::new());
        for (action, label) in Self::arrangement_commands() {
            let Action::AlignFloating(arrangement) = action else {
                continue;
            };
            let command = if board {
                Cmd::BoardAction(BoardAction::Arrange(arrangement))
            } else {
                Cmd::Canvas(Action::AlignFloating(arrangement))
            };
            if matches!(
                arrangement,
                FloatingArrangement::DistributeHorizontal | FloatingArrangement::DistributeVertical
            ) {
                distribute.push(cmd(label, command));
            } else {
                align.push(cmd(label, command));
            }
        }
        let mut rotate = Vec::new();
        if board {
            rotate = vec![
                cmd("Rotate left", Cmd::BoardAction(BoardAction::Rotate(-15.0))),
                cmd("Rotate right", Cmd::BoardAction(BoardAction::Rotate(15.0))),
            ];
        }
        submenu("Arrange", grouped(vec![align, distribute, rotate]))
    }

    fn select_menu(&self) -> CtxEntry {
        submenu(
            "Select",
            Self::selection_commands()
                .into_iter()
                .map(|(action, label)| canvas(label, action))
                .collect(),
        )
    }

    fn short_select_menu(&self) -> CtxEntry {
        submenu(
            "Select",
            vec![
                canvas("Select All", Action::SelectAll),
                canvas("Select None", Action::SelectNone),
                canvas("Central Topic", Action::SelectRoot),
            ],
        )
    }

    fn zones_menu(&self) -> CtxEntry {
        submenu(
            "Zones",
            grouped(vec![
                vec![
                    cmd("Create Zone", Cmd::CreateZone),
                    cmd("Add to Zone…", Cmd::AddToZone),
                    cmd("Remove from Zone", Cmd::RemoveFromZone),
                ],
                vec![cmd("Manage Zones", Cmd::Zones)],
            ]),
        )
    }

    fn ai_menu(&self) -> CtxEntry {
        let ai = |action: AiCommand| cmd(action.label(), Cmd::Ai(action));
        CtxEntry::Submenu {
            label: "AI",
            icon: Some(thinoda_ui::icons::AI),
            entries: grouped(vec![
                vec![
                    ai(AiCommand::Brainstorm),
                    ai(AiCommand::Grow),
                    ai(AiCommand::Breakdown),
                ],
                vec![
                    ai(AiCommand::Rewrite),
                    ai(AiCommand::Translate),
                    ai(AiCommand::Reorganize),
                    ai(AiCommand::Refine),
                ],
                vec![ai(AiCommand::Explain), ai(AiCommand::Summarize)],
            ]),
        }
    }

    fn view_menu(&self) -> CtxEntry {
        let (root, grid, snap, minimap) = self.doc().map_or(
            (SheetRootMode::Central, GridKind::None, false, false),
            |doc| {
                let sheet = doc.doc.sheet();
                (
                    sheet.root_mode,
                    sheet.board.grid,
                    sheet.board.snap,
                    doc.view.show_minimap,
                )
            },
        );
        let root = if root == SheetRootMode::Hidden {
            cmd("Show central topic", Cmd::BoardRoot(SheetRootMode::Central))
        } else {
            cmd("Hide central topic", Cmd::BoardRoot(SheetRootMode::Hidden))
        };
        submenu(
            "View",
            grouped(vec![
                vec![
                    cmd("Zoom to selection", Cmd::BoardZoomSelection),
                    canvas("Fit Map", Action::Fit),
                ],
                vec![
                    canvas("Expand All", Action::ExpandAll),
                    canvas("Collapse All", Action::CollapseAll),
                ],
                vec![
                    submenu(
                        "Canvas settings",
                        vec![
                            root,
                            check("Snap to grid", Cmd::BoardSnap, snap),
                            check("Board minimap", Cmd::BoardMinimap, minimap),
                        ],
                    ),
                    submenu(
                        "Grid",
                        [
                            ("No grid", GridKind::None),
                            ("Dot grid", GridKind::Dots),
                            ("Line grid", GridKind::Lines),
                        ]
                        .into_iter()
                        .map(|(label, kind)| check(label, Cmd::BoardGrid(kind), grid == kind))
                        .collect(),
                    ),
                ],
                vec![
                    cmd("Select tool", Cmd::BoardTool(BoardTool::Select)),
                    cmd("Hand tool", Cmd::BoardTool(BoardTool::Hand)),
                ],
            ]),
        )
    }

    fn card_text_target_available(&self) -> bool {
        self.has_card_text_target()
    }
}

fn clipboard() -> Vec<CtxEntry> {
    vec![
        canvas("Cut", Action::Cut),
        canvas("Copy", Action::Copy),
        cmd("Paste", Cmd::MenuPaste),
        canvas("Duplicate", Action::Duplicate),
    ]
}

fn layers_menu() -> CtxEntry {
    submenu(
        "Layers",
        grouped(vec![
            vec![
                cmd(
                    "Bring to front",
                    Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Front)),
                ),
                cmd(
                    "Bring forward",
                    Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Forward)),
                ),
                cmd(
                    "Send backward",
                    Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Backward)),
                ),
                cmd(
                    "Send to back",
                    Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Back)),
                ),
            ],
            vec![
                cmd(
                    "Above map",
                    Cmd::BoardAction(BoardAction::SetBand(Band::Above)),
                ),
                cmd(
                    "Below map",
                    Cmd::BoardAction(BoardAction::SetBand(Band::Below)),
                ),
            ],
        ]),
    )
}

fn delete_group(target: Target) -> Vec<CtxEntry> {
    let mut group = Vec::new();
    if target.summary {
        group.push(CtxEntry::Caption(
            "Deletes the summary topic and its children, not the covered branches.",
        ));
        group.push(canvas("Delete Summary", Action::Delete));
    } else {
        group.push(canvas("Delete", Action::Delete));
    }
    group.push(canvas("Delete Topic Only", Action::DeleteKeepChildren));
    group
}

/// Stable, non-localized identity for usage counts. Positional payloads are
/// collapsed so the key space stays bounded.
pub fn usage_command_id(command: &Cmd) -> Option<&'static str> {
    match command {
        Cmd::Canvas(Action::InsertFloating(_)) => Some("topic.insert-floating"),
        other => other.shortcut_id(),
    }
}

/// Visits every command entry, including nested ones.
#[cfg(test)]
fn for_each_command(entries: &[CtxEntry], f: &mut impl FnMut(&Cmd)) {
    for entry in entries {
        match entry {
            CtxEntry::Command { command, .. } => f(command),
            CtxEntry::Submenu { entries, .. } => for_each_command(entries, f),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn depth(entries: &[CtxEntry]) -> usize {
        entries
            .iter()
            .map(|entry| match entry {
                CtxEntry::Submenu { entries, .. } => 1 + depth(entries),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    fn rows(entries: &[CtxEntry]) -> usize {
        entries
            .iter()
            .filter(|entry| !matches!(entry, CtxEntry::Separator | CtxEntry::Caption(_)))
            .count()
    }

    fn check_limits(entries: &[CtxEntry], top: bool) {
        assert!(rows(entries) <= if top { 20 } else { 12 }, "{entries:#?}");
        assert!(!matches!(entries.first(), Some(CtxEntry::Separator)));
        assert!(!matches!(entries.last(), Some(CtxEntry::Separator)));
        assert!(
            !entries
                .windows(2)
                .any(|pair| matches!(pair, [CtxEntry::Separator, CtxEntry::Separator]))
        );
        for entry in entries {
            if let CtxEntry::Submenu { entries, .. } = entry {
                assert!(!entries.is_empty());
                check_limits(entries, false);
            }
        }
    }

    #[test]
    fn every_target_menu_is_shallow_bounded_and_unique() {
        let app = ThinodaApp::headless();
        let target = Target {
            topics: true,
            single: true,
            cards: true,
            objects: false,
            floating: true,
            branch: true,
            link: true,
            summary: true,
            locked: true,
            unlocked: true,
        };
        let objects = Target {
            objects: true,
            ..target
        };
        for entries in [
            app.blank_menu(),
            app.topic_menu(target),
            app.board_menu(target),
            app.board_menu(objects),
            app.annotation_menu(),
        ] {
            assert!(depth(&entries) <= 2, "{entries:#?}");
            check_limits(&entries, true);
            let mut seen = Vec::new();
            for_each_command(&entries, &mut |command| {
                assert!(!seen.contains(command), "duplicate {command:?}");
                seen.push(command.clone());
                assert!(usage_command_id(command).is_some(), "{command:?}");
            });
        }
    }

    #[test]
    fn retired_context_groups_stay_reachable_from_menus() {
        let app = ThinodaApp::headless();
        let target = Target {
            topics: true,
            single: true,
            floating: true,
            branch: true,
            ..Target::default()
        };
        let mut context = Vec::new();
        for entries in [
            app.blank_menu(),
            app.topic_menu(target),
            app.board_menu(Target {
                objects: true,
                ..target
            }),
        ] {
            for_each_command(&entries, &mut |command| context.push(command.clone()));
        }
        let mut main = Vec::new();
        fn collect(entries: &[crate::menus::MenuEntry], out: &mut Vec<Cmd>) {
            for entry in entries {
                match entry {
                    crate::menus::MenuEntry::Command { command, .. } => out.push(command.clone()),
                    crate::menus::MenuEntry::Submenu { entries, .. } => collect(entries, out),
                    _ => {}
                }
            }
        }
        collect(&app.main_menu_entries(), &mut main);
        for command in ThinodaApp::board_commands()
            .into_iter()
            .map(|(command, _)| command)
            .chain(
                ThinodaApp::arrangement_commands()
                    .into_iter()
                    .map(|(action, _)| Cmd::Canvas(action)),
            )
            .chain(
                ThinodaApp::selection_commands()
                    .into_iter()
                    .map(|(action, _)| Cmd::Canvas(action)),
            )
            .chain(AiCommand::ALL.into_iter().map(Cmd::Ai))
        {
            assert!(
                context.contains(&command) || main.contains(&command),
                "{command:?} is unreachable"
            );
        }
        for command in [
            Cmd::Canvas(Action::ToggleCollapse),
            Cmd::Canvas(Action::DeleteKeepChildren),
            Cmd::TaskPlanning,
            Cmd::Palette,
            Cmd::BoardTool(BoardTool::Pen),
            Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Front)),
        ] {
            assert!(context.contains(&command), "{command:?}");
        }
    }

    #[test]
    fn blank_canvas_offers_creation_without_topic_only_rows() {
        let app = ThinodaApp::headless();
        let mut commands = Vec::new();
        for_each_command(&app.blank_menu(), &mut |command| {
            commands.push(command.clone())
        });
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Cmd::Canvas(Action::InsertFloating(_))))
        );
        for absent in [
            Cmd::Canvas(Action::InsertSibling),
            Cmd::Canvas(Action::InsertChild),
            Cmd::Canvas(Action::Cut),
            Cmd::Canvas(Action::Delete),
        ] {
            assert!(!commands.contains(&absent), "{absent:?}");
        }
    }

    #[test]
    fn context_labels_are_translated() {
        let app = ThinodaApp::headless();
        let target = Target {
            topics: true,
            single: true,
            floating: true,
            locked: true,
            unlocked: true,
            objects: true,
            ..Target::default()
        };
        fn labels(entries: &[CtxEntry], out: &mut Vec<&'static str>) {
            for entry in entries {
                match entry {
                    CtxEntry::Command { label, .. } | CtxEntry::Caption(label) => out.push(label),
                    CtxEntry::Submenu { label, entries, .. } => {
                        out.push(label);
                        labels(entries, out);
                    }
                    _ => {}
                }
            }
        }
        let mut all = Vec::new();
        for entries in [
            app.blank_menu(),
            app.topic_menu(target),
            app.board_menu(target),
            app.annotation_menu(),
        ] {
            labels(&entries, &mut all);
        }
        all.sort_unstable();
        all.dedup();
        let missing: Vec<_> = all
            .into_iter()
            .filter(|label| !crate::i18n::has_translations(label))
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }
}

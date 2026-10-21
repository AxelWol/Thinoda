//! Command palette (Ctrl/Cmd+Shift+P) with fuzzy matching.

use crate::tokens::{Palette, Tokens};
use egui::{Context, Id, Key, Modal, RichText};

/// A command the palette can execute. `id` is returned when chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub id: String,
    pub title: String,
    pub shortcut: Option<String>,
    pub category: Option<String>,
}

impl Command {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            shortcut: None,
            category: None,
        }
    }

    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }

    pub fn category(mut self, c: impl Into<String>) -> Self {
        self.category = Some(c.into());
        self
    }
}

/// Subsequence fuzzy score: `None` when `query` is not a subsequence of `text`.
/// Higher is better; rewards contiguous matches and word starts.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut ti = 0;
    let mut prev: Option<usize> = None;
    for qc in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let mut found = None;
        while ti < t.len() {
            if t[ti] == qc {
                found = Some(ti);
                ti += 1;
                break;
            }
            ti += 1;
        }
        let i = found?;
        score += 1;
        if prev.is_some_and(|p| p + 1 == i) {
            score += 5;
        }
        if i == 0 || !t[i - 1].is_alphanumeric() {
            score += 8;
        }
        prev = Some(i);
    }
    Some(score * 10 - t.len() as i32)
}

#[derive(Debug, Default)]
pub struct CommandPalette {
    pub open: bool,
    query: String,
    selected: usize,
}

impl CommandPalette {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.query.clear();
        self.selected = 0;
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, q: impl Into<String>) {
        self.query = q.into();
        self.selected = 0;
    }

    /// Commands matching the current query, best first.
    pub fn filter<'a>(&self, commands: &'a [Command]) -> Vec<&'a Command> {
        let mut v: Vec<(i32, &Command)> = commands
            .iter()
            .filter_map(|c| {
                let hay = match &c.category {
                    Some(cat) => format!("{cat}: {}", c.title),
                    None => c.title.clone(),
                };
                fuzzy_score(&self.query, &c.title)
                    .or_else(|| fuzzy_score(&self.query, &hay).map(|s| s - 50))
                    .map(|s| (s, c))
            })
            .collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.0));
        v.into_iter().map(|(_, c)| c).collect()
    }

    /// Shows the palette; returns the chosen command id.
    pub fn show(
        &mut self,
        ctx: &Context,
        commands: &[Command],
        placeholder: &str,
    ) -> Option<String> {
        if !self.open {
            return None;
        }
        let p = Palette::of(ctx);
        let matches: Vec<Command> = self
            .filter(commands)
            .into_iter()
            .take(12)
            .cloned()
            .collect();
        let (up, down, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
            )
        });
        if down {
            self.selected = (self.selected + 1).min(matches.len().saturating_sub(1));
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        let mut chosen = None;
        let frame = egui::Frame::popup(&ctx.global_style());
        let resp = Modal::new(Id::unique("thinoda_command_palette"))
            .frame(frame)
            .show(ctx, |ui| {
                let available = (ctx.content_rect().size()
                    - frame.total_margin().sum()
                    - egui::Vec2::splat(16.0))
                .max(egui::Vec2::ZERO);
                ui.set_width(available.x.min(520.0));
                ui.set_max_height(available.y);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(placeholder)
                        .desired_width(f32::INFINITY)
                        .font(egui::FontId::proportional(Tokens::FONT_HEADING)),
                );
                edit.request_focus();
                if edit.changed() {
                    self.selected = 0;
                }
                ui.add_space(6.0);
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("palette_results")
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height());
                if edit.changed() {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    for (i, c) in matches.iter().enumerate() {
                        let sel = i == self.selected;
                        let r = ui
                            .add(
                                egui::Button::selectable(
                                    sel,
                                    RichText::new(&c.title).color(p.text),
                                )
                                .right_text(
                                    RichText::new(c.shortcut.clone().unwrap_or_default())
                                        .color(p.text_muted),
                                )
                                .min_size(egui::vec2(ui.available_width(), 26.0)),
                            )
                            .on_hover_text(c.category.clone().unwrap_or_default());
                        if sel && (up || down || edit.changed()) {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() || (sel && enter) {
                            chosen = Some(c.id.clone());
                        }
                    }
                    if matches.is_empty() {
                        crate::widgets::caption(ui, "—");
                    }
                });
            });
        if resp.should_close() || chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_basics() {
        assert!(fuzzy_score("ins", "Insert Topic").is_some());
        assert!(fuzzy_score("xyz", "Insert Topic").is_none());
        assert!(
            fuzzy_score("it", "Insert Topic").unwrap()
                > fuzzy_score("it", "Edit Notes").unwrap_or(i32::MIN)
        );
        assert_eq!(fuzzy_score("", "abc"), Some(0));
    }

    #[test]
    fn filter_orders() {
        let cmds = vec![
            Command::new("export.png", "Export PNG").category("File"),
            Command::new("zoom.in", "Zoom In").shortcut("Ctrl +"),
            Command::new("insert.topic", "Insert Topic").category("Insert"),
        ];
        let mut p = CommandPalette::default();
        p.set_query("zoom");
        assert_eq!(p.filter(&cmds)[0].id, "zoom.in");
        p.set_query("file");
        assert_eq!(p.filter(&cmds)[0].id, "export.png");
        p.set_query("");
        assert_eq!(p.filter(&cmds).len(), 3);
    }

    #[test]
    fn keyboard_selection_scrolls_without_pinning_manual_browsing() {
        use egui::accesskit::Role;
        use egui_kittest::{Harness, kittest::Queryable};

        let commands: Vec<_> = (0..12)
            .map(|index| Command::new(format!("command-{index}"), format!("Command {index:02}")))
            .collect();
        let mut palette = CommandPalette::default();
        palette.toggle();
        let mut h = Harness::builder()
            .with_size(egui::vec2(320.0, 180.0))
            .build_ui_state(
                |ui, state: &mut (CommandPalette, Vec<String>)| {
                    if let Some(chosen) = state.0.show(ui.ctx(), &commands, "Search") {
                        state.1.push(chosen);
                    }
                },
                (palette, Vec::new()),
            );
        h.run_steps(4);
        for index in 1..12 {
            h.key_press(Key::ArrowDown);
            h.run_steps(8);
            let rect = h
                .get_by_role_and_label(Role::Button, &format!("Command {index:02} "))
                .rect();
            assert!(
                h.ctx.content_rect().contains_rect(rect),
                "{index}: {rect:?}"
            );
            assert!(rect.top() >= h.get_by_role(Role::TextInput).rect().bottom());
        }
        h.get_by_role_and_label(Role::Button, "Command 00 ")
            .scroll_to_me();
        h.run_steps(8);
        assert!(
            h.ctx
                .content_rect()
                .contains_rect(h.get_by_role_and_label(Role::Button, "Command 00 ").rect())
        );
        h.key_press(Key::ArrowUp);
        h.run_steps(8);
        assert!(
            h.ctx
                .content_rect()
                .contains_rect(h.get_by_role_and_label(Role::Button, "Command 10 ").rect())
        );
        h.key_press(Key::Enter);
        h.run_steps(4);
        assert_eq!(h.state().1, ["command-10"]);
        assert!(!h.state().0.open);

        h.state_mut().0.toggle();
        h.run_steps(4);
        h.get_by_role(Role::TextInput).type_text("Command 05");
        h.run_steps(4);
        h.get_by_role_and_label(Role::Button, "Command 05 ");
        assert!(
            h.query_by_role_and_label(Role::Button, "Command 00 ")
                .is_none()
        );
        h.key_press(Key::Enter);
        h.run_steps(4);
        assert_eq!(h.state().1, ["command-10", "command-5"]);
    }
}

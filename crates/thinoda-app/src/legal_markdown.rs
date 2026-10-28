//! Offline CommonMark/GFM presentation. Source documents are never rewritten.

use egui::{Color32, TextFormat, TextStyle, Ui, text::LayoutJob};
use pulldown_cmark::{Event, Options, Parser, Tag};

pub(crate) const OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_FOOTNOTES);

#[derive(Clone, Debug)]
enum Node {
    Element(Tag<'static>, Vec<Node>),
    Leaf(Event<'static>),
}

fn parse(text: &str) -> Vec<Node> {
    fn children(events: &mut impl Iterator<Item = Event<'static>>) -> Vec<Node> {
        let mut nodes = Vec::new();
        while let Some(event) = events.next() {
            match event {
                Event::Start(tag) => nodes.push(Node::Element(tag, children(events))),
                Event::End(_) => break,
                event => nodes.push(Node::Leaf(event)),
            }
        }
        nodes
    }
    children(&mut Parser::new_ext(text, OPTIONS).map(Event::into_static))
}

#[derive(Clone, Default)]
pub(crate) struct Markdown {
    source: String,
    nodes: Vec<Node>,
}

impl Markdown {
    pub(crate) fn show(&mut self, ui: &mut Ui, source: &str) -> Option<String> {
        if self.source != source {
            self.nodes = parse(source);
            self.source = source.to_owned();
        }
        let mut clicked = None;
        ui.scope(|ui| {
            ui.style_mut().url_in_tooltip = true;
            ui.style_mut().interaction.multi_widget_text_select = true;
            blocks(ui, &self.nodes, &mut clicked);
        });
        clicked
    }
}

#[derive(Clone, Default)]
struct InlineStyle {
    strong: bool,
    italics: bool,
    strike: bool,
    code: bool,
    heading: Option<u8>,
    link: Option<String>,
}

fn format(ui: &Ui, style: &InlineStyle) -> TextFormat {
    let mut font = if style.code {
        TextStyle::Monospace.resolve(ui.style())
    } else {
        TextStyle::Body.resolve(ui.style())
    };
    if let Some(level) = style.heading {
        font.size *= match level {
            1 => 1.8,
            2 => 1.5,
            3 => 1.25,
            _ => 1.1,
        };
    }
    let row_height = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let color = if style.link.is_some() {
        ui.visuals().hyperlink_color
    } else if style.strong || style.heading.is_some() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let mut result = TextFormat {
        font_id: font,
        line_height: Some(row_height * 1.5),
        color,
        italics: style.italics,
        background: if style.code {
            ui.visuals().code_bg_color
        } else {
            Color32::TRANSPARENT
        },
        ..Default::default()
    };
    if style.strike {
        result.strikethrough = egui::Stroke::new(1.0, color);
    }
    if style.link.is_some() {
        result.underline = egui::Stroke::new(1.0, color);
    }
    result
}

fn text(ui: &mut Ui, value: &str, style: &InlineStyle, clicked: &mut Option<String>) {
    let mut job = LayoutJob::default();
    let format = format(ui, style);
    let leading_space = ui.fonts_mut(|fonts| {
        value
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .map(|character| fonts.glyph_width(&format.font_id, character))
            .sum::<f32>()
    });
    if value
        .chars()
        .all(|character| matches!(character, ' ' | '\t'))
    {
        ui.add_space(leading_space);
        return;
    }
    let trailing_space = ui.fonts_mut(|fonts| {
        value
            .chars()
            .rev()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .map(|character| fonts.glyph_width(&format.font_id, character))
            .sum::<f32>()
    });
    ui.add_space(leading_space);
    job.append(value, 0.0, format);
    job.wrap.max_width = ui.available_width();
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let label = egui::Label::new(galley.clone()).selectable(true);
    let response = if style.link.is_some() {
        ui.add(label.sense(egui::Sense::click()))
    } else {
        ui.add(label)
    };
    if style.strong || style.heading.is_some() {
        ui.painter().galley(
            response.rect.min + egui::vec2(0.4, 0.0),
            galley,
            ui.visuals().strong_text_color(),
        );
    }
    if let Some(destination) = &style.link {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Link, ui.is_enabled(), value)
        });
        let response = response.on_hover_text(destination);
        if response.clicked() {
            *clicked = Some(destination.clone());
        }
    }
    // egui's text bounds omit edge whitespace; preserve it between styled runs.
    ui.add_space(trailing_space);
}

fn inlines(ui: &mut Ui, nodes: &[Node], style: &InlineStyle, clicked: &mut Option<String>) {
    for (index, node) in nodes.iter().enumerate() {
        match node {
            Node::Element(tag, children) => {
                let mut style = style.clone();
                match tag {
                    Tag::Strong => style.strong = true,
                    Tag::Emphasis => style.italics = true,
                    Tag::Strikethrough => style.strike = true,
                    Tag::Link { dest_url, .. } => style.link = Some(dest_url.to_string()),
                    // Offline documents do not fetch remote images or execute HTML.
                    Tag::Image { .. } => {}
                    _ => {}
                }
                inlines(ui, children, &style, clicked);
            }
            Node::Leaf(Event::Text(value)) => text(ui, value, style, clicked),
            Node::Leaf(Event::Code(value)) => {
                let mut code = style.clone();
                code.code = true;
                text(ui, value, &code, clicked);
            }
            Node::Leaf(Event::SoftBreak) => text(ui, " ", style, clicked),
            Node::Leaf(Event::HardBreak) => {
                ui.end_row();
            }
            Node::Leaf(Event::TaskListMarker(checked)) => {
                let mut checked = *checked;
                ui.add_enabled(false, egui::Checkbox::without_text(&mut checked))
                    .accessible_name(plain_text(&nodes[index + 1..]));
            }
            Node::Leaf(Event::FootnoteReference(name)) => {
                text(ui, &format!("[{name}]"), style, clicked);
            }
            Node::Leaf(Event::InlineHtml(value) | Event::Html(value))
                if !value.trim_start().starts_with("<!--") =>
            {
                text(ui, value, style, clicked);
            }
            _ => {}
        }
    }
}

fn plain_text(nodes: &[Node]) -> String {
    let mut result = String::new();
    for node in nodes {
        match node {
            Node::Element(_, children) => result.push_str(&plain_text(children)),
            Node::Leaf(Event::Text(value) | Event::Code(value)) => result.push_str(value),
            Node::Leaf(Event::SoftBreak | Event::HardBreak) => result.push(' '),
            _ => {}
        }
    }
    result
}

fn paragraph(ui: &mut Ui, nodes: &[Node], style: InlineStyle, clicked: &mut Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        inlines(ui, nodes, &style, clicked);
    });
}

fn inline(node: &Node) -> bool {
    matches!(
        node,
        Node::Element(
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. },
            _
        ) | Node::Leaf(
            Event::Text(_)
                | Event::Code(_)
                | Event::SoftBreak
                | Event::HardBreak
                | Event::TaskListMarker(_)
                | Event::FootnoteReference(_)
                | Event::InlineHtml(_)
        )
    )
}

fn blocks(ui: &mut Ui, nodes: &[Node], clicked: &mut Option<String>) {
    let mut index = 0;
    while index < nodes.len() {
        if inline(&nodes[index]) {
            let count = nodes[index..]
                .iter()
                .take_while(|node| inline(node))
                .count();
            paragraph(
                ui,
                &nodes[index..index + count],
                InlineStyle::default(),
                clicked,
            );
            index += count;
            continue;
        }
        let node = &nodes[index];
        ui.push_id(index, |ui| match node {
            Node::Element(Tag::Heading { level, .. }, children) => {
                ui.add_space(8.0);
                paragraph(
                    ui,
                    children,
                    InlineStyle {
                        heading: Some(*level as u8),
                        strong: true,
                        ..Default::default()
                    },
                    clicked,
                );
                ui.add_space(4.0);
            }
            Node::Element(Tag::Paragraph, children) => {
                paragraph(ui, children, InlineStyle::default(), clicked);
                ui.add_space(6.0);
            }
            Node::Element(Tag::CodeBlock(_), children) => {
                egui::Frame::new()
                    .fill(ui.visuals().code_bg_color)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        paragraph(
                            ui,
                            children,
                            InlineStyle {
                                code: true,
                                ..Default::default()
                            },
                            clicked,
                        );
                    });
            }
            Node::Element(Tag::List(start), items) => {
                for (ordinal, item) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let prefix = start.map_or_else(
                            || "\u{2022}".into(),
                            |n| format!("{}.", n + ordinal as u64),
                        );
                        text(ui, &prefix, &InlineStyle::default(), clicked);
                        ui.vertical(|ui| {
                            if let Node::Element(Tag::Item, children) = item {
                                blocks(ui, children, clicked);
                            }
                        });
                    });
                }
            }
            Node::Element(Tag::BlockQuote(_), children) => {
                egui::Frame::new()
                    .inner_margin(8.0)
                    .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                    .show(ui, |ui| blocks(ui, children, clicked));
            }
            Node::Element(Tag::Table(alignments), rows) => {
                let width = (ui.available_width() / alignments.len().max(1) as f32).max(100.0);
                egui::Grid::new("markdown_table")
                    .striped(true)
                    .num_columns(alignments.len())
                    .min_col_width(width)
                    .max_col_width(width)
                    .show(ui, |ui| {
                        for row in rows {
                            if let Node::Element(tag, cells) = row {
                                for (column, cell) in cells.iter().enumerate() {
                                    if let Node::Element(Tag::TableCell, children) = cell {
                                        let align = match alignments.get(column) {
                                            Some(pulldown_cmark::Alignment::Center) => {
                                                egui::Align::Center
                                            }
                                            Some(pulldown_cmark::Alignment::Right) => {
                                                egui::Align::RIGHT
                                            }
                                            _ => egui::Align::LEFT,
                                        };
                                        ui.with_layout(egui::Layout::top_down(align), |ui| {
                                            ui.set_width(width);
                                            paragraph(
                                                ui,
                                                children,
                                                InlineStyle {
                                                    strong: matches!(tag, Tag::TableHead),
                                                    ..Default::default()
                                                },
                                                clicked,
                                            );
                                        });
                                    }
                                }
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(8.0);
            }
            Node::Element(Tag::FootnoteDefinition(name), children) => {
                text(ui, &format!("[{name}]"), &InlineStyle::default(), clicked);
                blocks(ui, children, clicked);
            }
            Node::Element(_, children) => blocks(ui, children, clicked),
            Node::Leaf(Event::Rule) => {
                ui.separator();
            }
            Node::Leaf(_) => paragraph(
                ui,
                std::slice::from_ref(node),
                InlineStyle::default(),
                clicked,
            ),
        });
        index += 1;
    }
}

#[cfg(test)]
#[path = "legal_markdown_tests.rs"]
mod tests;

use std::{collections::HashMap, sync::Arc};

use resvg::usvg::{self, fontdb::Database};
use thinoda_core::{
    Rect, Sheet, TopicId, Vec2,
    layout::{ApproxMeasure, TextMeasure},
    theme::resolve_style,
};

use crate::IoError;

use super::esc;

const DEFAULT_FAMILY: &str = "Segoe UI, Helvetica, Arial, sans-serif";

#[derive(Clone)]
pub(super) struct Line {
    pub text: String,
    pub bounds: Rect,
}

pub(super) struct Block {
    pub lines: Vec<Line>,
    pub line_height: f32,
    size: Vec2,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    text: String,
    size: u32,
    bold: bool,
    width: u32,
    family: Option<String>,
}

impl Key {
    fn new(text: &str, size: f32, bold: bool, width: f32, family: Option<&str>) -> Self {
        Self {
            text: text.into(),
            size: size.to_bits(),
            bold,
            width: width.to_bits(),
            family: family.map(str::to_owned),
        }
    }
}

/// Prepare fallible font shaping before entering the infallible layout backend.
/// Drawing reuses these exact lines, never rewraps at the measured ink width.
pub(super) struct ExportText {
    blocks: HashMap<Key, Block>,
    topics: HashMap<TopicId, Key>,
}

impl ExportText {
    pub fn new(sheet: &Sheet, dark: bool, fontdb: Arc<Database>) -> Result<Self, IoError> {
        let mut result = Self {
            blocks: HashMap::new(),
            topics: HashMap::new(),
        };
        let options = usvg::Options {
            fontdb,
            ..Default::default()
        };
        for (&id, topic) in &sheet.topics {
            let style = resolve_style(sheet, id, dark);
            let title = if topic.title.trim().is_empty() {
                " "
            } else {
                &topic.title
            };
            let width = style
                .width
                .unwrap_or(if id == sheet.root { 420.0 } else { 300.0 });
            let key = Key::new(
                title,
                style.font_size,
                style.bold,
                width,
                style.font_family.as_deref(),
            );
            result.topics.insert(id, key.clone());
            if let std::collections::hash_map::Entry::Vacant(entry) = result.blocks.entry(key) {
                let block = wrap(
                    title,
                    style.font_size,
                    style.bold,
                    width,
                    style.font_family.as_deref().unwrap_or(DEFAULT_FAMILY),
                    &options,
                )?;
                entry.insert(block);
            }
        }
        Ok(result)
    }

    pub fn topic(&self, id: TopicId) -> &Block {
        &self.blocks[&self.topics[&id]]
    }
}

impl TextMeasure for ExportText {
    fn measure(&self, text: &str, size: f32, bold: bool, width: f32) -> Vec2 {
        // Labels retain the layout engine's existing metrics.
        ApproxMeasure.measure(text, size, bold, width)
    }

    fn measure_with_family(
        &self,
        text: &str,
        size: f32,
        bold: bool,
        width: f32,
        family: Option<&str>,
    ) -> Vec2 {
        self.blocks[&Key::new(text, size, bold, width, family)].size
    }
}

fn wrap(
    text: &str,
    size: f32,
    bold: bool,
    width: f32,
    family: &str,
    options: &usvg::Options<'_>,
) -> Result<Block, IoError> {
    let mut cache = HashMap::<String, Rect>::new();
    let mut measure = |text: &str| -> Result<Rect, IoError> {
        if let Some(bounds) = cache.get(text) {
            return Ok(*bounds);
        }
        let source = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><text id=\"line\" font-family=\"{}\" font-size=\"{size}\" font-weight=\"{}\">{}</text></svg>",
            esc(family),
            if bold { "bold" } else { "normal" },
            esc(text),
        );
        let tree = usvg::Tree::from_str(&source, options)
            .map_err(|e| IoError::Invalid(format!("export text layout: {e}")))?;
        let bounds = if text.is_empty() {
            Rect::default()
        } else {
            let node = tree
                .node_by_id("line")
                .ok_or_else(|| IoError::Invalid(format!("no export font could render {text:?}")))?;
            let bbox = node.bounding_box();
            Rect::from_min_size(
                Vec2::new(bbox.x(), bbox.y()),
                Vec2::new(bbox.width(), bbox.height()),
            )
        };
        cache.insert(text.into(), bounds);
        Ok(bounds)
    };
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if measure(&candidate)?.size.x <= width {
                current = candidate;
                continue;
            }
            if !current.is_empty() {
                lines.push(Line {
                    bounds: measure(&current)?,
                    text: std::mem::take(&mut current),
                });
            }
            for ch in word.chars() {
                let candidate = format!("{current}{ch}");
                if !current.is_empty() && measure(&candidate)?.size.x > width {
                    lines.push(Line {
                        bounds: measure(&current)?,
                        text: std::mem::take(&mut current),
                    });
                }
                current.push(ch);
            }
        }
        lines.push(Line {
            bounds: measure(&current)?,
            text: current,
        });
    }
    let line_height = lines
        .iter()
        .map(|line| line.bounds.size.y + size * 0.25)
        .fold(size * ApproxMeasure::LINE_HEIGHT, f32::max);
    let measured_width = lines
        .iter()
        .map(|line| line.bounds.size.x)
        .fold(size * 0.5, f32::max);
    Ok(Block {
        size: Vec2::new(measured_width, lines.len() as f32 * line_height),
        lines,
        line_height,
    })
}

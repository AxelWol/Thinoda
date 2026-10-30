//! Bounded notes-equation syntax and vector layout, not a general TeX engine.
//! Sources remain in the existing versioned Markdown note envelope.

pub const MAX_SOURCE_BYTES: usize = 512;
pub const MAX_NODES: usize = 128;
pub const MAX_DEPTH: usize = 8;
pub const MAX_WIDTH: f32 = 4096.0;
pub const MAX_HEIGHT: f32 = 2048.0;
pub const FENCE: &str = "```thinoda-math-v1\n";
pub const EXPORT_ERROR: &str = "This format cannot preserve or render notes equations. Save as Thinoda or use a supported vector or source-preserving format.";
pub const SYNTAX_ERROR: &str =
    "Invalid equation. Use letters, digits, operators, groups, \\frac{a}{b}, ^{...} and _{...}.";
pub const LIMIT_ERROR: &str = "Equation limit reached (512 bytes, 128 nodes, 8 levels).";
pub const BLOCK_ERROR: &str =
    "Keep one equation block at the end of the note, or edit the note source directly.";

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Glyph(char),
    Row(Vec<Expr>),
    Fraction(Box<Expr>, Box<Expr>),
    Scripts {
        base: Box<Expr>,
        upper: Option<Box<Expr>>,
        lower: Option<Box<Expr>>,
    },
}

pub fn parse(source: &str) -> Result<Expr, &'static str> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(LIMIT_ERROR);
    }
    struct Parser<'a> {
        remaining: &'a str,
        nodes: usize,
    }
    impl Parser<'_> {
        fn node(&mut self) -> Result<(), &'static str> {
            self.nodes += 1;
            if self.nodes > MAX_NODES {
                Err(LIMIT_ERROR)
            } else {
                Ok(())
            }
        }

        fn group(&mut self, depth: usize) -> Result<Expr, &'static str> {
            self.remaining = self.remaining.strip_prefix('{').ok_or(SYNTAX_ERROR)?;
            let result = self.row(depth + 1)?;
            self.remaining = self.remaining.strip_prefix('}').ok_or(SYNTAX_ERROR)?;
            Ok(result)
        }

        fn row(&mut self, depth: usize) -> Result<Expr, &'static str> {
            if depth > MAX_DEPTH {
                return Err(LIMIT_ERROR);
            }
            self.node()?;
            let mut row = Vec::new();
            while let Some(c) = self.remaining.chars().next() {
                if c == '}' {
                    break;
                }
                if c == ' ' {
                    self.remaining = &self.remaining[1..];
                    continue;
                }
                self.node()?;
                let mut atom = if self.remaining.starts_with("\\frac") {
                    self.remaining = &self.remaining[5..];
                    Expr::Fraction(Box::new(self.group(depth)?), Box::new(self.group(depth)?))
                } else if c == '{' {
                    self.group(depth)?
                } else if c.is_ascii_alphanumeric() || "+-=()[],./:|".contains(c) {
                    self.remaining = &self.remaining[1..];
                    Expr::Glyph(c)
                } else {
                    return Err(SYNTAX_ERROR);
                };
                let mut upper = None;
                let mut lower = None;
                while let Some(script) = self.remaining.chars().next() {
                    let target = match script {
                        '^' => &mut upper,
                        '_' => &mut lower,
                        _ => break,
                    };
                    if target.is_some() {
                        return Err(SYNTAX_ERROR);
                    }
                    self.node()?;
                    self.remaining = &self.remaining[1..];
                    *target = Some(Box::new(self.group(depth)?));
                }
                if upper.is_some() || lower.is_some() {
                    atom = Expr::Scripts {
                        base: Box::new(atom),
                        upper,
                        lower,
                    };
                }
                row.push(atom);
            }
            if row.is_empty() {
                return Err(SYNTAX_ERROR);
            }
            Ok(Expr::Row(row))
        }
    }
    let mut parser = Parser {
        remaining: source,
        nodes: 0,
    };
    let expression = parser.row(0)?;
    if !parser.remaining.is_empty() {
        return Err(SYNTAX_ERROR);
    }
    Ok(expression)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    Glyph {
        character: char,
        x: f32,
        y: f32,
        size: f32,
    },
    Rule {
        x: f32,
        y: f32,
        width: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub primitives: Vec<Primitive>,
}

impl Layout {
    fn append(&mut self, mut other: Self, x: f32, y: f32) {
        for primitive in &mut other.primitives {
            match primitive {
                Primitive::Glyph { x: px, y: py, .. } | Primitive::Rule { x: px, y: py, .. } => {
                    *px += x;
                    *py += y;
                }
            }
        }
        self.primitives.extend(other.primitives);
    }
}

/// Parse before laying out; callers cannot supply an unbounded recursive tree.
/// `measure` returns glyph width/height in the same units as `size`.
pub fn layout(
    source: &str,
    size: f32,
    measure: impl Fn(char, f32) -> (f32, f32),
) -> Result<Layout, &'static str> {
    fn render(expr: &Expr, size: f32, measure: &impl Fn(char, f32) -> (f32, f32)) -> Layout {
        match expr {
            Expr::Glyph(character) => {
                let (width, height) = measure(*character, size);
                Layout {
                    width,
                    height,
                    baseline: height * 0.75,
                    primitives: vec![Primitive::Glyph {
                        character: *character,
                        x: 0.0,
                        y: 0.0,
                        size,
                    }],
                }
            }
            Expr::Row(items) => {
                let items: Vec<_> = items
                    .iter()
                    .map(|item| render(item, size, measure))
                    .collect();
                let baseline = items.iter().map(|item| item.baseline).fold(0.0, f32::max);
                let descent = items
                    .iter()
                    .map(|item| item.height - item.baseline)
                    .fold(0.0, f32::max);
                let mut result = Layout {
                    baseline,
                    height: baseline + descent,
                    ..Default::default()
                };
                for item in items {
                    let x = result.width;
                    let y = baseline - item.baseline;
                    result.width += item.width;
                    result.append(item, x, y);
                }
                result
            }
            Expr::Fraction(top, bottom) => {
                let top = render(top, size * 0.85, measure);
                let bottom = render(bottom, size * 0.85, measure);
                let width = top.width.max(bottom.width) + size * 0.4;
                let gap = size * 0.16;
                let rule_y = top.height + gap;
                let bottom_y = rule_y + gap;
                let mut result = Layout {
                    width,
                    height: bottom_y + bottom.height,
                    baseline: rule_y + size * 0.25,
                    ..Default::default()
                };
                let top_x = (width - top.width) / 2.0;
                let bottom_x = (width - bottom.width) / 2.0;
                result.append(top, top_x, 0.0);
                result.append(bottom, bottom_x, bottom_y);
                result.primitives.push(Primitive::Rule {
                    x: 0.0,
                    y: rule_y,
                    width,
                });
                result
            }
            Expr::Scripts { base, upper, lower } => {
                let base = render(base, size, measure);
                let upper = upper
                    .as_ref()
                    .map(|item| render(item, size * 0.65, measure));
                let lower = lower
                    .as_ref()
                    .map(|item| render(item, size * 0.65, measure));
                let top = upper.as_ref().map_or(0.0, |item| item.height * 0.75);
                let lower_y = top + base.height * 0.8;
                let mut result = Layout {
                    width: base.width
                        + upper
                            .as_ref()
                            .map_or(0.0, |item| item.width)
                            .max(lower.as_ref().map_or(0.0, |item| item.width)),
                    height: (top + base.height)
                        .max(lower_y + lower.as_ref().map_or(0.0, |item| item.height)),
                    baseline: top + base.baseline,
                    ..Default::default()
                };
                let x = base.width;
                result.append(base, 0.0, top);
                if let Some(upper) = upper {
                    result.append(upper, x, 0.0);
                }
                if let Some(lower) = lower {
                    result.append(lower, x, lower_y);
                }
                result
            }
        }
    }
    if !size.is_finite() || !(8.0..=32.0).contains(&size) {
        return Err(LIMIT_ERROR);
    }
    // Validate measurements before recursion: non-finite/negative metrics must
    // never reach a painter, even when supplied by another frontend.
    let checked = |c, size| {
        let (w, h) = measure(c, size);
        if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 {
            (w, h)
        } else {
            (f32::INFINITY, f32::INFINITY)
        }
    };
    let result = render(&parse(source)?, size, &checked);
    if !result.width.is_finite()
        || !result.height.is_finite()
        || result.width > MAX_WIDTH
        || result.height > MAX_HEIGHT
    {
        return Err(LIMIT_ERROR);
    }
    Ok(result)
}

/// Conservative export detection also catches malformed/unfinished source blocks.
pub fn has_equation(note: &crate::notes::NoteContent) -> bool {
    note.is_markdown() && note.contains("thinoda-math-v1")
}

pub fn sheet_has_equations(sheet: &crate::Sheet) -> bool {
    sheet
        .topics
        .values()
        .any(|topic| topic.notes.as_ref().is_some_and(has_equation))
}

fn strip_line_end(source: &str) -> Option<&str> {
    source
        .strip_suffix("\r\n")
        .or_else(|| source.strip_suffix('\n'))
        .or_else(|| source.strip_suffix('\r'))
}

/// Authored trailing block only. Preserve surrounding note bytes, including CRLF.
pub fn split_note(source: &str) -> Result<Option<(&str, &str)>, &'static str> {
    if !source.contains("thinoda-math-v1") {
        return Ok(None);
    }
    let header = "```thinoda-math-v1";
    let start = source.find(header).ok_or(BLOCK_ERROR)?;
    if source.matches("thinoda-math-v1").count() != 1 {
        return Err(BLOCK_ERROR);
    }
    let body = &source[start + header.len()..];
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .or_else(|| body.strip_prefix('\r'))
        .ok_or(BLOCK_ERROR)?;
    let math = strip_line_end(body)
        .and_then(|body| body.strip_suffix("```"))
        .and_then(strip_line_end)
        .ok_or(BLOCK_ERROR)?;
    let prefix = if start == 0 {
        ""
    } else {
        strip_line_end(&source[..start])
            .and_then(strip_line_end)
            .ok_or(BLOCK_ERROR)?
    };
    Ok(Some((prefix, math)))
}

pub fn replace_note(source: &str, equation: Option<&str>) -> Result<String, &'static str> {
    let prefix = split_note(source)?.map_or(source, |(prefix, _)| prefix);
    let Some(equation) = equation else {
        return Ok(prefix.to_owned());
    };
    parse(equation)?;
    let separator = if prefix.is_empty() {
        ""
    } else if prefix.ends_with("\r\n") {
        "\r\n\r\n"
    } else if prefix.ends_with('\r') {
        "\r\r"
    } else {
        "\n\n"
    };
    Ok(format!("{prefix}{separator}{FENCE}{equation}\n```\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equation_parser_rejects_unsupported_and_bounds_work() {
        for source in [
            "",
            "{}",
            "\\sqrt{x}",
            "\\input{file}",
            "$x$",
            "é",
            "x^2",
            "x^{2}^{3}",
            "{x",
            "x}",
            "\\frac{}{b}",
            "a\nb",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
        assert!(parse("\\frac{a_{1}}{b^{2}}+x_{i}^{3}").is_ok());
        assert!(parse(&format!("{}x", " ".repeat(MAX_SOURCE_BYTES - 1))).is_ok());
        assert!(parse(&"a".repeat(MAX_NODES - 1)).is_ok());
        assert!(
            parse(&format!(
                "{}x{}",
                "{".repeat(MAX_DEPTH),
                "}".repeat(MAX_DEPTH)
            ))
            .is_ok()
        );
        assert!(parse(&"a".repeat(MAX_SOURCE_BYTES + 1)).is_err());
        assert!(parse(&"a".repeat(MAX_NODES)).is_err());
        assert!(
            parse(&format!(
                "{}x{}",
                "{".repeat(MAX_DEPTH + 1),
                "}".repeat(MAX_DEPTH + 1)
            ))
            .is_err()
        );
    }

    #[test]
    fn equation_layout_has_real_fraction_rule_and_script_offsets() {
        let layout = layout("\\frac{a}{b}+x_{i}^{2}", 20.0, |_, size| (size * 0.6, size)).unwrap();
        assert!(
            layout
                .primitives
                .iter()
                .any(|item| matches!(item, Primitive::Rule { width, .. } if *width > 0.0))
        );
        let y = |c| {
            layout
                .primitives
                .iter()
                .find_map(|item| match item {
                    Primitive::Glyph { character, y, .. } if *character == c => Some(*y),
                    _ => None,
                })
                .unwrap()
        };
        assert!(y('a') < y('b'));
        assert!(y('2') < y('x'));
        assert!(y('i') > y('x'));
        assert!(layout.width > 0.0 && layout.height > 20.0);
        for metrics in [
            (f32::NAN, 1.0),
            (-1.0, 1.0),
            (5000.0, 1.0),
            (1.0, f32::INFINITY),
        ] {
            assert!(super::layout("x", 20.0, |_, _| metrics).is_err());
        }
    }

    #[test]
    fn equation_note_replacement_is_lossless_and_failure_preserves_source() {
        let original = "**research**\n\nliteral source\n";
        let saved = replace_note(original, Some("\\frac{a}{b}")).unwrap();
        assert_eq!(split_note(&saved), Ok(Some((original, "\\frac{a}{b}"))));
        assert_eq!(replace_note(&saved, None).unwrap(), original);
        let edited = replace_note(&saved, Some("x^{2}")).unwrap();
        assert_eq!(split_note(&edited).unwrap().unwrap().0, original);
        assert!(replace_note(&saved, Some("\\bad")).is_err());
        assert_eq!(split_note(&saved).unwrap().unwrap().1, "\\frac{a}{b}");
        assert!(split_note(&format!("{saved}other text")).is_err());
        assert!(!has_equation(&crate::notes::NoteContent::Plain(
            saved.clone()
        )));
        assert!(has_equation(&crate::notes::NoteContent::markdown(saved)));
    }

    #[test]
    fn equation_fences_accept_pasted_line_endings_without_rewriting_note_prefix() {
        for ending in ["\n", "\r\n", "\r"] {
            let prefix = format!("**Research**{ending}Keep these bytes{ending}");
            let source = format!(
                "{prefix}{ending}{ending}```thinoda-math-v1{ending}x^{{2}}{ending}```{ending}"
            );
            assert_eq!(split_note(&source), Ok(Some((prefix.as_str(), "x^{2}"))));
            assert_eq!(replace_note(&source, None).unwrap(), prefix);
            let replaced = replace_note(&source, Some("\\frac{a}{b}")).unwrap();
            assert_eq!(
                split_note(&replaced),
                Ok(Some((prefix.as_str(), "\\frac{a}{b}")))
            );
            assert!(replaced.starts_with(&prefix));
        }
    }
}

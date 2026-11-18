//! Object data of a canvas object: link, tags, custom properties and the
//! placeholder flag (draw.io "Edit Data", architecture §metadata).
//!
//! The tooltip is the object's accessibility description (`alt_text`). Links are
//! stored as written: an openable URL, a draw.io page link (`data:page/id,…`) or
//! another draw.io link that is preserved but not followed.

use crate::board::{CanvasLayer, CanvasObject, CanvasObjectKind};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashSet, ops::Range};

/// Upper bound for one object's data, in bytes of link, tags, names and values.
pub const MAX_OBJECT_DATA_BYTES: usize = 32 * 1024;
/// Upper bound for the custom properties of one object.
pub const MAX_OBJECT_PROPERTIES: usize = 256;
/// Upper bound for the tags of one object.
pub const MAX_OBJECT_TAGS: usize = 128;
/// Upper bound for one link, in bytes.
pub const MAX_LINK_BYTES: usize = 8 * 1024;
/// Prefix of a draw.io link to another page.
pub const PAGE_LINK_PREFIX: &str = "data:page/id,";

/// Attribute names a property may not use, because draw.io or the exporter
/// give them a meaning of their own.
pub const RESERVED_PROPERTY_NAMES: [&str; 7] = [
    "id",
    "label",
    "tooltip",
    "link",
    "tags",
    "placeholders",
    "placeholder",
];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Space-free tags in user order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Named values in user order; names are XML attribute names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<(String, String)>,
    /// Whether `%name%` in the label is replaced by property values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub placeholders: bool,
}

impl ObjectData {
    pub fn is_empty(&self) -> bool {
        self.link.is_none()
            && self.tags.is_empty()
            && self.properties.is_empty()
            && !self.placeholders
    }

    pub fn byte_len(&self) -> usize {
        self.link.as_ref().map_or(0, String::len)
            + self.tags.iter().map(String::len).sum::<usize>()
            + self
                .properties
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>()
    }

    /// The value of property `name`.
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The target diagram id of a draw.io page link.
    pub fn page_link(&self) -> Option<&str> {
        page_link_target(self.link.as_deref()?)
    }

    /// The tags as draw.io writes them: separated by single spaces.
    pub fn tags_text(&self) -> String {
        self.tags.join(" ")
    }

    /// Tags from space-separated text, without duplicates or empty entries.
    pub fn parse_tags(text: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        text.split_whitespace()
            .filter(|tag| seen.insert(*tag))
            .map(str::to_owned)
            .collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(link) = &self.link
            && (link.trim().is_empty()
                || link.len() > MAX_LINK_BYTES
                || link.chars().any(char::is_control))
        {
            return Err("invalid object link".into());
        }
        if self.tags.len() > MAX_OBJECT_TAGS {
            return Err("an object has too many tags".into());
        }
        let mut tags = HashSet::new();
        for tag in &self.tags {
            if tag.is_empty()
                || tag.chars().any(|c| c.is_whitespace() || c.is_control())
                || !tags.insert(tag.as_str())
            {
                return Err(format!("invalid object tag {tag:?}"));
            }
        }
        if self.properties.len() > MAX_OBJECT_PROPERTIES {
            return Err("an object has too many properties".into());
        }
        let mut names = HashSet::new();
        for (name, value) in &self.properties {
            if !valid_property_name(name)
                || value
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
                || !names.insert(name.as_str())
            {
                return Err(format!("invalid object property {name:?}"));
            }
        }
        if self.byte_len() > MAX_OBJECT_DATA_BYTES {
            return Err("object data is too large".into());
        }
        Ok(())
    }
}

/// A property name that draw.io can store as an attribute without a special meaning.
pub fn valid_property_name(name: &str) -> bool {
    crate::drawio_passthrough::valid_attribute_name(name)
        && !RESERVED_PROPERTY_NAMES.contains(&name)
}

/// The diagram id a `data:page/id,…` link points to.
pub fn page_link_target(link: &str) -> Option<&str> {
    link.strip_prefix(PAGE_LINK_PREFIX)
        .filter(|id| !id.is_empty())
}

/// Whether draw.io replaces `%name%` in the object's text. Files saved before
/// object data kept the flag as a passthrough attribute.
pub fn replaces_placeholders(object: &CanvasObject) -> bool {
    object.data.as_ref().is_some_and(|data| data.placeholders)
        || object.drawio.as_ref().is_some_and(|drawio| {
            drawio
                .attributes
                .iter()
                .any(|(name, value)| name == "placeholders" && value == "1")
        })
}

/// The value of attribute `name` on one object, as draw.io's `hasAttribute`.
fn attribute(object: &CanvasObject, name: &str) -> Option<String> {
    let data = object.data.as_deref();
    let value = match name {
        "link" => data.and_then(|data| data.link.clone()),
        "tags" => data
            .filter(|data| !data.tags.is_empty())
            .map(ObjectData::tags_text),
        "placeholders" => data
            .filter(|data| data.placeholders)
            .map(|_| "1".to_owned()),
        _ => data.and_then(|data| data.property(name)).map(str::to_owned),
    };
    value.or_else(|| {
        object.drawio.as_ref().and_then(|drawio| {
            drawio
                .attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
    })
}

/// Length of the name of a draw.io placeholder that starts after a `%`
/// (`%(date\{[^{}\n\r\u2028\u2029]*\}|[^%\{\}"'=;]+)%`).
fn placeholder_name_len(rest: &str) -> Option<usize> {
    if let Some(after) = rest.strip_prefix("date{")
        && let Some(close) = after.find(['{', '}', '\n', '\r', '\u{2028}', '\u{2029}'])
        && after[close..].starts_with("}%")
    {
        return Some("date{".len() + close + 1);
    }
    let len = rest
        .find(['%', '{', '}', '"', '\'', '=', ';'])
        .unwrap_or(rest.len());
    (len > 0 && rest[len..].starts_with('%')).then_some(len)
}

/// Replaces draw.io placeholders in `text` (Graph.replacePlaceholders): `%%name%`
/// writes `%name%`, `%label%` and `%tooltip%` stay, and unknown names stay.
pub fn replace_placeholders<'a>(
    text: &'a str,
    lookup: impl FnMut(&str) -> Option<String>,
) -> Cow<'a, str> {
    replace_placeholders_reporting(text, lookup, |_, _| {})
}

fn replace_placeholders_reporting<'a>(
    text: &'a str,
    mut lookup: impl FnMut(&str) -> Option<String>,
    mut changed: impl FnMut(Range<usize>, &str),
) -> Cow<'a, str> {
    let mut out = String::new();
    let mut last = 0;
    let mut pos = 0;
    while let Some(offset) = text[pos..].find('%') {
        let start = pos + offset;
        let Some(len) = placeholder_name_len(&text[start + 1..]) else {
            pos = start + 1;
            continue;
        };
        let end = start + len + 2;
        let name = &text[start + 1..end - 1];
        pos = end;
        if name == "label" || name == "tooltip" {
            continue;
        }
        let value = if start > last && text[..start].ends_with('%') {
            Some(text[start + 1..end].to_owned())
        } else {
            lookup(name)
        };
        if let Some(value) = &value
            && value != &text[start..end]
        {
            changed(start..end, value);
        }
        out.push_str(&text[last..start]);
        out.push_str(value.as_deref().unwrap_or(&text[start..end]));
        last = end;
    }
    if last == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[last..]);
    if out == text {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(out)
    }
}

/// Whether `text` uses a draw.io global or unit variable that Thinoda leaves
/// literal (dates, page numbers, file name, edge length, unit sizes).
pub fn uses_unresolved_drawio_variables(text: &str) -> bool {
    let mut found = false;
    let _ = replace_placeholders(text, |name| {
        let base = name.split('+').next().unwrap_or(name);
        found |= name.starts_with("date{")
            || matches!(
                base,
                "date" | "time" | "timestamp" | "pagenumber" | "pagecount" | "filename" | "length"
            )
            || ["width_", "height_"].iter().any(|prefix| {
                name.strip_prefix(prefix)
                    .is_some_and(|unit| matches!(unit, "mm" | "cm" | "in" | "m"))
            });
        None
    });
    found
}

/// Text of `object` with its placeholders replaced from its own data, then
/// its scene containers, then `id`, `width`, `height` and the page name.
pub fn resolve_placeholders<'a>(
    canvas: &CanvasLayer,
    object: &CanvasObject,
    page: Option<&str>,
    text: &'a str,
) -> Cow<'a, str> {
    resolve_placeholders_reporting(canvas, object, page, text, |_, _| {})
}

fn resolve_placeholders_reporting<'a>(
    canvas: &CanvasLayer,
    object: &CanvasObject,
    page: Option<&str>,
    text: &'a str,
    changed: impl FnMut(Range<usize>, &str),
) -> Cow<'a, str> {
    if !text.contains('%') || !replaces_placeholders(object) {
        return Cow::Borrowed(text);
    }
    replace_placeholders_reporting(
        text,
        |name| {
            match name {
                "id" => {
                    return Some(
                        object
                            .drawio
                            .as_ref()
                            .and_then(|drawio| drawio.source_id.clone())
                            .unwrap_or_else(|| object.id.to_string()),
                    );
                }
                "width" | "height" if !object.is_connector() => {
                    let size = object.rect.size;
                    return Some(if name == "width" { size.x } else { size.y }.to_string());
                }
                _ => {}
            }
            if !name.contains('{') {
                let mut current = Some(object);
                for _ in 0..=canvas.objects.len() {
                    let Some(cell) = current else { break };
                    if let Some(value) = attribute(cell, name) {
                        return Some(value);
                    }
                    current = canvas
                        .scene
                        .parents
                        .get(&cell.id)
                        .and_then(|parent| canvas.object(*parent));
                }
            }
            (name == "page").then(|| page.map(str::to_owned)).flatten()
        },
        changed,
    )
}

/// `object` with placeholders replaced in its labels and description; borrowed
/// when nothing changes.
pub fn with_resolved_placeholders<'a>(
    canvas: &CanvasLayer,
    object: &'a CanvasObject,
    page: Option<&str>,
) -> Cow<'a, CanvasObject> {
    if !replaces_placeholders(object) {
        return Cow::Borrowed(object);
    }
    let mut resolved: Option<CanvasObject> = None;
    let mut set = |edit: &dyn Fn(&mut CanvasObject, String), value: Cow<'_, str>| {
        if let Cow::Owned(value) = value {
            edit(resolved.get_or_insert_with(|| object.clone()), value);
        }
    };
    set(
        &|object, value| object.alt_text = value,
        resolve_placeholders(canvas, object, page, &object.alt_text),
    );
    let label = match &object.kind {
        CanvasObjectKind::Shape { label, .. } => Some(label),
        CanvasObjectKind::Connector(connector) => Some(&connector.label),
        CanvasObjectKind::Ink(_) => None,
    };
    if let Some(label) = label {
        let mut changes = Vec::new();
        let value = resolve_placeholders_reporting(canvas, object, page, label, |range, value| {
            changes.push((range, value.to_owned()))
        });
        set(
            &|object, value| {
                if let Some(rich) = &object.label_rich_text {
                    let mut text = label.clone();
                    let mut rich = (**rich).clone();
                    let result: Result<(), String> =
                        changes.iter().rev().try_for_each(|(range, replacement)| {
                            let start = text[..range.start].chars().count();
                            let end = text[..range.end].chars().count();
                            (text, rich) =
                                rich.replace_label_range(&text, start..end, replacement)?;
                            Ok(())
                        });
                    match result {
                        Ok(()) => {
                            debug_assert_eq!(text, value);
                            object.label_rich_text =
                                (!rich.is_empty()).then(|| std::sync::Arc::new(rich));
                        }
                        Err(error) => {
                            log::warn!("Cannot render formatted placeholder: {error}");
                            object.label_rich_text = None;
                        }
                    }
                }
                match &mut object.kind {
                    CanvasObjectKind::Shape { label, .. } => *label = value,
                    CanvasObjectKind::Connector(connector) => connector.label = value,
                    CanvasObjectKind::Ink(_) => unreachable!("ink has no label"),
                }
            },
            value,
        );
    }
    if let CanvasObjectKind::Connector(connector) = &object.kind {
        for (index, extra) in connector.extra_labels.iter().enumerate() {
            set(
                &|object, value| {
                    if let CanvasObjectKind::Connector(connector) = &mut object.kind {
                        connector.extra_labels[index].text = value;
                    }
                },
                resolve_placeholders(canvas, object, page, &extra.text),
            );
        }
    }
    resolved.map_or(Cow::Borrowed(object), Cow::Owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> ObjectData {
        ObjectData {
            link: Some("https://example.com".into()),
            tags: vec!["web".into(), "prod".into()],
            properties: vec![
                ("owner".into(), "Ada".into()),
                ("subnet".into(), "10.0".into()),
            ],
            placeholders: true,
        }
    }

    #[test]
    fn validates_links_tags_properties_and_budget() {
        data().validate().unwrap();
        assert!(ObjectData::default().is_empty());
        for link in ["", " ", "a\u{0}b"] {
            let mut d = data();
            d.link = Some(link.into());
            assert!(d.validate().is_err(), "{link:?}");
        }
        for tag in ["", "a b", "web"] {
            let mut d = data();
            d.tags.push(tag.into());
            assert!(d.validate().is_err(), "{tag:?}");
        }
        for name in ["link", "tags", "label", "1x", "owner", "xmlns"] {
            let mut d = data();
            d.properties.push((name.into(), String::new()));
            assert!(d.validate().is_err(), "{name}");
        }
        let mut large = data();
        large
            .properties
            .push(("big".into(), "x".repeat(MAX_OBJECT_DATA_BYTES)));
        assert!(large.validate().is_err());
    }

    #[test]
    fn replaces_placeholders_like_drawio() {
        let lookup = |name: &str| (name == "a").then(|| "1".to_owned());
        assert_eq!(
            replace_placeholders("x%a%y %b% %%a% %%", lookup),
            "x1y %b% %a% %%"
        );
        assert_eq!(
            replace_placeholders("%label% %tooltip%", lookup),
            "%label% %tooltip%"
        );
        assert_eq!(
            replace_placeholders("50% off %a=b%", lookup),
            "50% off %a=b%"
        );
        assert!(matches!(
            replace_placeholders("%b%", lookup),
            Cow::Borrowed(_)
        ));
        let dates = |name: &str| Some(format!("[{name}]"));
        assert_eq!(replace_placeholders("%date{yyyy}%", dates), "[date{yyyy}]");
    }

    #[test]
    fn detects_unresolved_drawio_variables() {
        for text in [
            "%date%",
            "%date{yyyy}%",
            "%pagenumber+1%",
            "%width_mm%",
            "%length%",
        ] {
            assert!(uses_unresolved_drawio_variables(text), "{text}");
        }
        for text in ["%width% %subnet% %page%", "%%date%", "%width_px%"] {
            assert!(!uses_unresolved_drawio_variables(text), "{text}");
        }
    }

    #[test]
    fn resolves_from_object_then_scene_containers() {
        use crate::board::{CanvasObjectId, ShapeKind};
        use crate::geometry::Rect;
        let mut canvas = CanvasLayer::default();
        let rect = Rect::from_min_size(Default::default(), crate::geometry::Vec2::new(40.0, 20.0));
        for (id, label, properties) in [
            (1, "", vec![("subnet".to_owned(), "10.0".to_owned())]),
            (
                2,
                "%subnet%.%n% %id% %width% %page% %missing%",
                vec![("n".to_owned(), "7".to_owned())],
            ),
        ] {
            let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect);
            object.id = CanvasObjectId(id);
            if let CanvasObjectKind::Shape { label: text, .. } = &mut object.kind {
                *text = label.to_owned();
            }
            object.data = Some(std::sync::Arc::new(ObjectData {
                properties,
                placeholders: id == 2,
                ..Default::default()
            }));
            canvas.objects.push(object);
        }
        canvas.scene.containers.insert(CanvasObjectId(1));
        canvas
            .scene
            .parents
            .insert(CanvasObjectId(2), CanvasObjectId(1));
        let child = &canvas.objects[1];
        let resolved = with_resolved_placeholders(&canvas, child, Some("Net"));
        assert_eq!(resolved.label(), "10.0.7 2 40 Net %missing%");
        let mut plain = child.clone();
        plain.data = None;
        assert!(matches!(
            with_resolved_placeholders(&canvas, &plain, None),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn resolved_placeholders_shift_each_rich_run_without_mutating_the_source() {
        use crate::board::ShapeKind;
        use crate::{InlineText, Rect, Vec2};
        use std::sync::Arc;

        let mut object = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 20.0)),
        );
        object.set_label("%name% + %role%").unwrap();
        let mut rich = InlineText::default();
        rich.update_range(object.label(), 0..6, |style| style.bold = Some(true))
            .unwrap();
        rich.update_range(object.label(), 9..15, |style| style.underline = Some(true))
            .unwrap();
        object.label_rich_text = Some(Arc::new(rich.clone()));
        object.data = Some(Arc::new(ObjectData {
            properties: vec![("name".into(), "🙂".into()), ("role".into(), "lead".into())],
            placeholders: true,
            ..Default::default()
        }));
        let resolved = with_resolved_placeholders(&CanvasLayer::default(), &object, None);
        assert_eq!(resolved.label(), "🙂 + lead");
        let resolved_rich = resolved.label_rich_text.as_ref().unwrap();
        assert_eq!(
            (resolved_rich.runs[0].start, resolved_rich.runs[0].end),
            (0, 1)
        );
        assert_eq!(
            (resolved_rich.runs[1].start, resolved_rich.runs[1].end),
            (4, 8)
        );
        assert_eq!(object.label(), "%name% + %role%");
        assert_eq!(object.label_rich_text.as_deref(), Some(&rich));
    }

    #[test]
    fn connector_placeholders_preserve_primary_label_formatting() {
        use crate::board::CanvasConnector;
        use crate::{Endpoint, InlineText, Vec2};
        use std::sync::Arc;

        let mut object = CanvasObject::connector(CanvasConnector::new(
            Endpoint::Point(Vec2::ZERO),
            Endpoint::Point(Vec2::new(100.0, 0.0)),
        ));
        object.set_label("%name% to %role%").unwrap();
        let mut rich = InlineText::default();
        rich.update_range(object.label(), 0..6, |style| style.bold = Some(true))
            .unwrap();
        rich.update_range(object.label(), 10..16, |style| style.italic = Some(true))
            .unwrap();
        object.label_rich_text = Some(Arc::new(rich.clone()));
        object.data = Some(Arc::new(ObjectData {
            properties: vec![("name".into(), "🙂".into()), ("role".into(), "lead".into())],
            placeholders: true,
            ..Default::default()
        }));
        let resolved = with_resolved_placeholders(&CanvasLayer::default(), &object, None);
        assert_eq!(resolved.label(), "🙂 to lead");
        let rich = resolved.label_rich_text.as_ref().unwrap();
        assert_eq!((rich.runs[0].start, rich.runs[0].end), (0, 1));
        assert_eq!((rich.runs[1].start, rich.runs[1].end), (5, 9));
        assert_eq!(object.label(), "%name% to %role%");
    }

    #[test]
    fn parses_tags_and_page_links() {
        assert_eq!(ObjectData::parse_tags(" a  b a\tc "), ["a", "b", "c"]);
        assert_eq!(data().tags_text(), "web prod");
        assert_eq!(data().property("subnet"), Some("10.0"));
        let mut page = data();
        page.link = Some("data:page/id,abc-1".into());
        assert_eq!(page.page_link(), Some("abc-1"));
        assert_eq!(data().page_link(), None);
        assert_eq!(page_link_target("data:page/id,"), None);
    }
}

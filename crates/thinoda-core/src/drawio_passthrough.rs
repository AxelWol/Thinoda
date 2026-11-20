//! draw.io round-trip passthrough (architecture §3.1.1).
//!
//! Imported draw.io data without a Thinoda field is kept here, so that saving back to
//! draw.io preserves it. Each value has a single owner: a style key is either
//! represented by a first-class field or stored as passthrough, never both. The
//! importer decides ownership; the exporter only appends passthrough keys it did not
//! write itself.
//!
//! Blobs are immutable and shared through `Arc`, so undo snapshots do not copy them.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Upper bound for one object's passthrough, in bytes of keys and values.
pub const MAX_OBJECT_PASSTHROUGH_BYTES: usize = 16 * 1024;
/// Upper bound for all passthrough of a sheet, objects and page together.
pub const MAX_SHEET_PASSTHROUGH_BYTES: usize = 2 * 1024 * 1024;
/// Upper bound for one draw.io cell or diagram id.
pub const MAX_SOURCE_ID_BYTES: usize = 256;
/// Upper bound for remembered source cell ids of a page.
pub const MAX_PAGE_CELL_IDS: usize = 100_000;
/// Upper bound for unknown attributes on the source `<mxfile>` element.
pub const MAX_DOCUMENT_PASSTHROUGH_BYTES: usize = 16 * 1024;
/// Attributes managed by the draw.io adapter rather than document passthrough.
pub const RESERVED_DOCUMENT_ATTRIBUTES: &[&str] = &[
    "agent",
    "compressed",
    "etag",
    "host",
    "modified",
    "pages",
    "type",
    "version",
];
/// Model attributes written by the draw.io adapter, not page passthrough.
pub const RESERVED_MODEL_ATTRIBUTES: &[&str] = &[
    "dx",
    "dy",
    "grid",
    "gridSize",
    "thinodaGrid",
    "thinodaSnap",
    "thinodaSankey",
    "background",
    "backgroundImage",
    "guides",
    "tooltips",
    "connect",
    "arrows",
    "fold",
    "page",
    "pageScale",
    "pageWidth",
    "pageHeight",
    "math",
    "shadow",
    "adaptiveColors",
];

/// Unknown attributes of the source `<mxfile>` element.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawioDocumentPassthrough {
    /// Attributes not represented by first-class workbook fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

impl DrawioDocumentPassthrough {
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }

    pub fn byte_len(&self) -> usize {
        pairs_len(&self.attributes)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut names = HashSet::new();
        for (name, value) in &self.attributes {
            if !valid_attribute_name(name)
                || RESERVED_DOCUMENT_ATTRIBUTES.contains(&name.as_str())
                || !valid_text(value)
                || !names.insert(name.as_str())
            {
                return Err(format!("invalid draw.io document attribute {name:?}"));
            }
        }
        if self.byte_len() > MAX_DOCUMENT_PASSTHROUGH_BYTES {
            return Err("draw.io document passthrough is too large".into());
        }
        Ok(())
    }
}

/// Unknown draw.io data of one cell that became a canvas object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawioCellPassthrough {
    /// Cell id in the source document; cleared on copy so ids stay unique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Style keys without a Thinoda field, in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style: Vec<(String, String)>,
    /// `UserObject` attributes without a Thinoda field, in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
}

/// `UserObject` attributes the exporter writes from first-class fields.
pub const RESERVED_ATTRIBUTES: [&str; 3] = ["id", "label", "tooltip"];

impl DrawioCellPassthrough {
    pub fn is_empty(&self) -> bool {
        self.source_id.is_none() && self.style.is_empty() && self.attributes.is_empty()
    }

    /// Bytes counted against the budgets.
    pub fn byte_len(&self) -> usize {
        self.source_id.as_ref().map_or(0, String::len)
            + pairs_len(&self.style)
            + pairs_len(&self.attributes)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(id) = &self.source_id {
            valid_id(id, "draw.io cell id")?;
        }
        let mut keys = HashSet::new();
        for (key, value) in &self.style {
            if !valid_style_key(key) || !valid_style_value(value) || !keys.insert(key.as_str()) {
                return Err(format!("invalid draw.io passthrough style key {key:?}"));
            }
        }
        let mut names = HashSet::new();
        for (name, value) in &self.attributes {
            if !valid_attribute_name(name)
                || RESERVED_ATTRIBUTES.contains(&name.as_str())
                || !valid_text(value)
                || !names.insert(name.as_str())
            {
                return Err(format!("invalid draw.io passthrough attribute {name:?}"));
            }
        }
        if self.byte_len() > MAX_OBJECT_PASSTHROUGH_BYTES {
            return Err("draw.io passthrough of an object is too large".into());
        }
        Ok(())
    }

    /// The passthrough a copy receives: unknown styling survives, the id does not.
    pub fn for_copy(this: &Option<std::sync::Arc<Self>>) -> Option<std::sync::Arc<Self>> {
        let this = this.as_ref()?;
        if this.source_id.is_none() {
            return Some(std::sync::Arc::clone(this));
        }
        let copy = Self {
            source_id: None,
            ..(**this).clone()
        };
        (!copy.is_empty()).then(|| std::sync::Arc::new(copy))
    }
}

/// Unknown draw.io data of a page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawioPagePassthrough {
    /// `<diagram id>` of the source page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagram_id: Option<String>,
    /// `mxGraphModel` attributes without a Thinoda field, in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_attributes: Vec<(String, String)>,
    /// Retained source IDs for imported objects, scene containers and named layers. Generated
    /// object, layer, zone, group, topic, map-decoration and auxiliary IDs avoid
    /// these after deletion. Sankey IDs are remapped in their semantic extension.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cell_ids: Vec<String>,
}

impl DrawioPagePassthrough {
    pub fn is_empty(&self) -> bool {
        self.diagram_id.is_none() && self.model_attributes.is_empty() && self.cell_ids.is_empty()
    }

    pub fn byte_len(&self) -> usize {
        self.diagram_id.as_ref().map_or(0, String::len)
            + pairs_len(&self.model_attributes)
            + self.cell_ids.iter().map(String::len).sum::<usize>()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(id) = &self.diagram_id {
            valid_id(id, "draw.io diagram id")?;
        }
        let mut names = HashSet::new();
        for (name, value) in &self.model_attributes {
            if !valid_attribute_name(name)
                || RESERVED_MODEL_ATTRIBUTES.contains(&name.as_str())
                || !valid_text(value)
                || !names.insert(name.as_str())
            {
                return Err(format!("invalid draw.io page attribute {name:?}"));
            }
        }
        if self.cell_ids.len() > MAX_PAGE_CELL_IDS {
            return Err("draw.io page remembers too many cell ids".into());
        }
        for id in &self.cell_ids {
            valid_id(id, "draw.io cell id")?;
        }
        if self.byte_len() > MAX_SHEET_PASSTHROUGH_BYTES {
            return Err("draw.io passthrough of the sheet is too large".into());
        }
        Ok(())
    }
}

fn pairs_len(pairs: &[(String, String)]) -> usize {
    pairs.iter().map(|(k, v)| k.len() + v.len()).sum()
}

fn valid_text(value: &str) -> bool {
    !value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\t' | '\r'))
}

pub(crate) fn valid_id(id: &str, what: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > MAX_SOURCE_ID_BYTES || id.chars().any(char::is_control) {
        return Err(format!("invalid {what}"));
    }
    Ok(())
}

/// A key that survives draw.io's `key=value;` style syntax unchanged.
pub fn valid_style_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .chars()
            .all(|c| !c.is_control() && !c.is_whitespace() && !matches!(c, ';' | '=' | '"'))
}

/// A value that survives draw.io's style syntax unchanged once trimmed.
pub fn valid_style_value(value: &str) -> bool {
    value.trim() == value && !value.contains(';') && value.chars().all(|c| !c.is_control())
}

/// An XML attribute name without a namespace prefix.
pub fn valid_attribute_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && name.len() <= 128
        && !name.to_ascii_lowercase().starts_with("xml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn cell() -> DrawioCellPassthrough {
        DrawioCellPassthrough {
            source_id: Some("abc-1".into()),
            style: vec![("shadow".into(), "1".into()), ("glass".into(), "1".into())],
            attributes: vec![("owner".into(), "Ada".into())],
        }
    }

    #[test]
    fn document_attributes_are_bounded_and_validated() {
        let document = DrawioDocumentPassthrough {
            attributes: vec![("custom".into(), "value".into())],
        };
        document.validate().unwrap();
        for attributes in [
            vec![("xmlns:evil".into(), "x".into())],
            vec![("host".into(), "spoofed".into())],
            vec![("custom".into(), "bad\u{1}".into())],
            vec![("custom".into(), "1".into()), ("custom".into(), "2".into())],
        ] {
            assert!(DrawioDocumentPassthrough { attributes }.validate().is_err());
        }
        let too_large = DrawioDocumentPassthrough {
            attributes: vec![("custom".into(), "x".repeat(MAX_DOCUMENT_PASSTHROUGH_BYTES))],
        };
        assert!(too_large.validate().is_err());
    }

    #[test]
    fn validates_keys_values_and_budget() {
        cell().validate().unwrap();
        for bad in [
            ("", "1"),
            ("a;b", "1"),
            ("a=b", "1"),
            ("a b", "1"),
            ("a", "x;y"),
            ("a", " x"),
        ] {
            let mut p = cell();
            p.style.push((bad.0.into(), bad.1.into()));
            assert!(p.validate().is_err(), "{bad:?}");
        }
        let mut duplicate = cell();
        duplicate.style.push(("shadow".into(), "0".into()));
        assert!(duplicate.validate().is_err());
        for bad in ["label", "id", "tooltip", "1x", "a:b", "xmlns"] {
            let mut p = cell();
            p.attributes.push((bad.into(), String::new()));
            assert!(p.validate().is_err(), "{bad}");
        }
        let mut large = cell();
        large
            .style
            .push(("big".into(), "x".repeat(MAX_OBJECT_PASSTHROUGH_BYTES)));
        assert!(large.validate().is_err());
        let mut no_id = cell();
        no_id.source_id = Some(String::new());
        assert!(no_id.validate().is_err());
    }

    #[test]
    fn copies_keep_unknown_data_but_not_the_source_id() {
        let original = Some(Arc::new(cell()));
        let copy = DrawioCellPassthrough::for_copy(&original).unwrap();
        assert_eq!(copy.source_id, None);
        assert_eq!(copy.style, cell().style);
        assert_eq!(copy.attributes, cell().attributes);
        let id_only = Some(Arc::new(DrawioCellPassthrough {
            source_id: Some("x".into()),
            ..Default::default()
        }));
        assert_eq!(DrawioCellPassthrough::for_copy(&id_only), None);
        let shared = Some(Arc::new(DrawioCellPassthrough {
            source_id: None,
            ..cell()
        }));
        assert!(Arc::ptr_eq(
            shared.as_ref().unwrap(),
            DrawioCellPassthrough::for_copy(&shared).as_ref().unwrap()
        ));
    }

    #[test]
    fn page_validation() {
        let page = DrawioPagePassthrough {
            diagram_id: Some("page-1".into()),
            model_attributes: vec![("future".into(), "1".into())],
            cell_ids: vec!["0".into(), "1".into()],
        };
        page.validate().unwrap();
        let mut bad = page.clone();
        bad.model_attributes.push(("future".into(), "0".into()));
        assert!(bad.validate().is_err());
        bad = page.clone();
        bad.model_attributes.push(("grid".into(), "0".into()));
        assert!(bad.validate().is_err());
        bad = page;
        bad.model_attributes
            .push(("large".into(), "x".repeat(MAX_SHEET_PASSTHROUGH_BYTES)));
        assert!(bad.validate().is_err());
    }
}

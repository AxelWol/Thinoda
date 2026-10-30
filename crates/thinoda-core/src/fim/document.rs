//! Ordered XML is authoritative. The map is a bound, rebuildable projection.
use super::Cardinality;
use crate::{Sheet, Structure, TopicId};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;

pub const MAX_XML_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORDS: usize = 1_000_000;
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dialect {
    Xdf2,
    Xdf3,
}
impl Dialect {
    pub fn namespace(self) -> &'static str {
        match self {
            Self::Xdf2 => "urn:xoev-de:fim:standard:xdatenfelder_2",
            Self::Xdf3 => "urn:xoev-de:fim:standard:xdatenfelder_3.0.0",
        }
    }
    pub fn from_namespace(namespace: &str) -> Option<Self> {
        [Self::Xdf2, Self::Xdf3]
            .into_iter()
            .find(|d| d.namespace() == namespace)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    DocumentProfile,
    Schema,
    Group,
    Field,
    CodeList,
    Translation,
    BuildingBlocks,
    FieldCatalogue,
    SchemaLibrary,
}
impl MessageKind {
    pub fn from_root(dialect: Dialect, root: &str) -> Option<Self> {
        Some(match root {
            "xdatenfelder.dokumentsteckbrief.0101" => Self::DocumentProfile,
            "xdatenfelder.stammdatenschema.0102" => Self::Schema,
            "xdatenfelder.datenfeldgruppe.0103" => Self::Group,
            "xdatenfelder.datenfeld.0104" => Self::Field,
            "xdatenfelder.codeliste.0105" => Self::CodeList,
            "xdatenfelder.uebersetzung.0106" if dialect == Dialect::Xdf3 => Self::Translation,
            "verzeichnis.datenfeldbaukasten.0201" => Self::BuildingBlocks,
            "verzeichnis.datenfeldkatalog.0202" => Self::FieldCatalogue,
            "verzeichnis.stammdatenschemabibliothek.0203" => Self::SchemaLibrary,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RecordId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FimKind {
    Schema,
    Group,
    Field,
    Rule,
    CodeList,
    Choice,
    DocumentProfile,
    Translation,
    Collection,
}
impl FimKind {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Schema => "S",
            Self::Group => "G",
            Self::Field => "F",
            Self::Rule => "R",
            Self::CodeList => "C",
            Self::Choice => "X",
            Self::DocumentProfile => "D",
            Self::Translation => "T",
            Self::Collection => "Library",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XmlAttribute {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Content {
    Element(RecordId),
    Raw(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Element {
    id: RecordId,
    parent: Option<RecordId>,
    name: String,
    namespace: String,
    attributes: Vec<XmlAttribute>,
    children: Vec<Content>,
    open: String,
    close: String,
}
impl Element {
    fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct XmlTree {
    records: BTreeMap<RecordId, Element>,
    document: Vec<Content>,
    next_id: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PersistedFimDocument")]
pub struct FimDocument {
    pub dialect: Dialect,
    pub message_kind: MessageKind,
    original: Arc<[u8]>,
    baseline: Arc<XmlTree>,
    tree: Arc<XmlTree>,
    /// Authoritative native presentation binding, not a transient parse cache.
    bindings: BTreeMap<TopicId, RecordId>,
}
#[derive(Deserialize)]
struct PersistedFimDocument {
    dialect: Dialect,
    message_kind: MessageKind,
    original: Arc<[u8]>,
    baseline: Arc<XmlTree>,
    tree: Arc<XmlTree>,
    bindings: BTreeMap<TopicId, RecordId>,
}
impl TryFrom<PersistedFimDocument> for FimDocument {
    type Error = FimModelError;
    fn try_from(value: PersistedFimDocument) -> Result<Self, Self::Error> {
        value.baseline.validate_native()?;
        value.tree.validate_native()?;
        if value.original.len() > MAX_XML_BYTES || value.baseline.bytes() != value.original.as_ref()
        {
            return Err(FimModelError::Integrity(
                "native FIM baseline does not match retained original bytes".into(),
            ));
        }
        let original = Self::parse(&value.original)?;
        let current = Self::parse(&value.tree.bytes())?;
        if original.dialect != value.dialect
            || current.dialect != value.dialect
            || original.message_kind != value.message_kind
            || current.message_kind != value.message_kind
        {
            return Err(FimModelError::Integrity(
                "native FIM profile does not match its XML envelope".into(),
            ));
        }
        let mut records = BTreeSet::new();
        for record in value.bindings.values() {
            if !value.tree.records.contains_key(record) || !records.insert(*record) {
                return Err(FimModelError::Integrity(
                    "invalid/duplicate native FIM binding".into(),
                ));
            }
        }
        Ok(Self {
            dialect: value.dialect,
            message_kind: value.message_kind,
            original: value.original,
            baseline: value.baseline,
            tree: value.tree,
            bindings: value.bindings,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordMetadata {
    pub record: RecordId,
    pub kind: FimKind,
    pub name: String,
    pub identity: Option<String>,
    pub version: Option<String>,
    pub usages: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicMetadata {
    pub record: RecordId,
    pub kind: FimKind,
    pub identity: Option<String>,
    pub version: Option<String>,
    pub cardinality: Option<String>,
    pub usages: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlField {
    pub record: RecordId,
    pub parent: Option<RecordId>,
    pub path: String,
    pub name: String,
    pub namespace: String,
    pub value: String,
    pub attributes: Vec<XmlAttribute>,
    pub has_elements: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
    pub record: Option<RecordId>,
    pub topic: Option<TopicId>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Containment,
    RuleTarget,
    RuleParameter,
    Relation,
    CodeListBinding,
    FreeTextMention,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyResolution {
    Internal(Vec<RecordId>),
    External,
    Ambiguous(Vec<RecordId>),
    Unresolved,
    ReviewHint(Vec<RecordId>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimDependency {
    pub source: RecordId,
    pub kind: DependencyKind,
    pub identity: String,
    pub resolution: DependencyResolution,
}
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FimModelError {
    #[error("FIM XML: {0}")]
    Xml(String),
    #[error("FIM requires UTF-8 XML; unsupported encoding: {0}")]
    Encoding(String),
    #[error("unsupported FIM message {{{namespace}}}{root}")]
    UnsupportedMessage { namespace: String, root: String },
    #[error("FIM processing limit exceeded: {0}")]
    Limit(&'static str),
    #[error("FIM decision required: {0}")]
    DecisionRequired(String),
    #[error("FIM operation rejected: {0}")]
    Integrity(String),
}
/// XML-first commands. `InsertElement.before` is an explicit schema-order anchor;
/// `None` appends. Existing siblings and mixed content are never sorted/dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FimChange {
    /// Explicit one-copy repair, never an implicit merge of conflicting identities.
    SetOccurrenceText {
        record: RecordId,
        value: String,
    },
    /// Adopt a caller-supplied identity for this occurrence, retaining nested reuse.
    /// The editor never allocates an official registry identifier.
    MakeIndependent {
        topic: TopicId,
        identity: String,
        version: Option<String>,
    },
    SetText {
        record: RecordId,
        value: String,
    },
    SetAttribute {
        record: RecordId,
        name: String,
        value: Option<String>,
    },
    InsertElement {
        parent: RecordId,
        before: Option<RecordId>,
        name: String,
        namespace: String,
        text: Option<String>,
    },
    RemoveElement {
        record: RecordId,
    },
    RenameDefinition {
        record: RecordId,
        name: String,
    },
    SetCardinality {
        topic: TopicId,
        value: String,
    },
    Add {
        parent: TopicId,
        kind: FimKind,
        name: String,
    },
    Reuse {
        parent: TopicId,
        record: RecordId,
    },
    Remove {
        topic: TopicId,
    },
    Move {
        topic: TopicId,
        parent: TopicId,
        index: usize,
    },
    Duplicate {
        topic: TopicId,
    },
}

fn xml_error(e: impl std::fmt::Display) -> FimModelError {
    FimModelError::Xml(e.to_string())
}
fn escape(s: &str) -> String {
    quick_xml::escape::escape(s).into_owned()
}
fn xml_char(c: char) -> bool {
    matches!(c,'\u{9}'|'\u{a}'|'\u{d}'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')
}
fn decode(s: &str) -> String {
    quick_xml::escape::unescape(s)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| s.to_owned())
}
impl XmlTree {
    /// Validate native/recovery arenas before any recursive traversal. Serde input
    /// cannot forge child cycles, dangling records, mismatched source metadata or
    /// an identifier allocator that would overwrite an existing XML record.
    fn validate_native(&self) -> Result<(), FimModelError> {
        if self.records.len() > MAX_RECORDS
            || self.next_id <= self.records.keys().map(|r| r.0).max().unwrap_or(0)
        {
            return Err(FimModelError::Integrity(
                "invalid native FIM arena/allocator".into(),
            ));
        }
        let mut visited = BTreeSet::new();
        let mut stack: Vec<_> = self.document.iter().map(|c| (c, None, 0)).collect();
        let mut bytes = 0usize;
        let mut preorder = Vec::new();
        while let Some((content, parent, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                return Err(FimModelError::Limit("native XML depth"));
            }
            match content {
                Content::Raw(raw) => bytes = bytes.saturating_add(raw.len()),
                Content::Element(id) => {
                    if !visited.insert(*id) {
                        return Err(FimModelError::Integrity(
                            "native XML cycle or multiply owned record".into(),
                        ));
                    }
                    let e = self.records.get(id).ok_or_else(|| {
                        FimModelError::Integrity("dangling native XML record".into())
                    })?;
                    if e.id != *id || e.parent != parent {
                        return Err(FimModelError::Integrity(
                            "native XML ownership mismatch".into(),
                        ));
                    }
                    bytes = bytes
                        .saturating_add(e.open.len())
                        .saturating_add(e.close.len());
                    preorder.push(*id);
                    stack.extend(e.children.iter().map(|c| (c, Some(*id), depth + 1)));
                }
            }
            if bytes > MAX_XML_BYTES {
                return Err(FimModelError::Limit("native XML bytes"));
            }
        }
        if visited.len() != self.records.len() {
            return Err(FimModelError::Integrity(
                "unreachable native XML records".into(),
            ));
        }
        let parsed = Self::parse(&self.bytes())?;
        let mut expected_stack: Vec<_> = parsed
            .document
            .iter()
            .filter_map(|c| {
                if let Content::Element(id) = c {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        let mut expected = Vec::new();
        while let Some(id) = expected_stack.pop() {
            expected.push(id);
            expected_stack.extend(parsed.children(id));
        }
        if expected.len() != preorder.len() {
            return Err(FimModelError::Integrity(
                "native XML record/source mismatch".into(),
            ));
        }
        for (a, b) in preorder.iter().zip(expected) {
            let a = &self.records[a];
            let b = &parsed.records[&b];
            if a.name != b.name || a.namespace != b.namespace || a.attributes != b.attributes {
                return Err(FimModelError::Integrity(
                    "native XML metadata/source mismatch".into(),
                ));
            }
        }
        Ok(())
    }
    fn parse(bytes: &[u8]) -> Result<Self, FimModelError> {
        if bytes.len() > MAX_XML_BYTES {
            return Err(FimModelError::Limit("XML bytes"));
        }
        let source =
            std::str::from_utf8(bytes).map_err(|e| FimModelError::Encoding(e.to_string()))?;
        if source.chars().any(|c| !xml_char(c)) {
            return Err(xml_error("prohibited XML character"));
        }
        let mut reader = NsReader::from_reader(bytes);
        reader.config_mut().check_comments = true;
        let mut tree = Self {
            records: BTreeMap::new(),
            document: Vec::new(),
            next_id: 1,
        };
        let mut stack: Vec<RecordId> = Vec::new();
        let mut root_seen = false;
        // quick-xml's positions exclude an initial UTF-8 BOM.
        let bom = if source.starts_with('\u{feff}') { 3 } else { 0 };
        if bom != 0 {
            tree.document.push(Content::Raw("\u{feff}".into()));
        }
        let mut previous = bom;
        loop {
            let (resolved, event) = reader.read_resolved_event().map_err(xml_error)?;
            let namespace = match resolved {
                ResolveResult::Bound(ns) => ns.as_ref().to_owned(),
                ResolveResult::Unbound => String::new(),
                ResolveResult::Unknown(prefix) => {
                    return Err(xml_error(format!("unbound namespace prefix {prefix}")));
                }
            };
            let end = reader.buffer_position() as usize + bom;
            let raw = source
                .get(previous..end)
                .ok_or_else(|| xml_error("invalid XML span"))?
                .to_owned();
            let first = previous == bom;
            previous = end;
            let content = match event {
                Event::Start(ref start) | Event::Empty(ref start) => {
                    if stack.len() >= MAX_DEPTH || tree.records.len() >= MAX_RECORDS {
                        return Err(FimModelError::Limit("XML depth or records"));
                    }
                    if start.name().as_ref().len() > 1024 {
                        return Err(FimModelError::Limit("XML name"));
                    }
                    let mut attributes = Vec::new();
                    for attr in start.attributes() {
                        let attr = attr.map_err(xml_error)?;
                        if attributes.len() >= 256 {
                            return Err(FimModelError::Limit("XML attributes"));
                        }
                        if let ResolveResult::Unknown(prefix) =
                            reader.resolver().resolve_attribute(attr.key).0
                        {
                            return Err(xml_error(format!("unbound attribute prefix {prefix}")));
                        }
                        attributes.push(XmlAttribute {
                            name: attr.key.as_ref().to_owned(),
                            value: attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(xml_error)?
                                .into_owned(),
                        });
                    }
                    if stack.is_empty() {
                        if root_seen {
                            return Err(xml_error("multiple document roots"));
                        }
                        root_seen = true;
                    }
                    let id = RecordId(tree.next_id);
                    tree.next_id += 1;
                    tree.records.insert(
                        id,
                        Element {
                            id,
                            parent: stack.last().copied(),
                            name: start.name().as_ref().to_owned(),
                            namespace,
                            attributes,
                            children: Vec::new(),
                            open: raw,
                            close: String::new(),
                        },
                    );
                    let content = Content::Element(id);
                    if let Some(parent) = stack.last() {
                        tree.records.get_mut(parent).unwrap().children.push(content);
                    } else {
                        tree.document.push(content);
                    }
                    if matches!(event, Event::Start(_)) {
                        stack.push(id);
                    }
                    continue;
                }
                Event::End(_) => {
                    let id = stack.pop().ok_or_else(|| xml_error("unexpected end"))?;
                    tree.records.get_mut(&id).unwrap().close = raw;
                    continue;
                }
                Event::DocType(_) => return Err(xml_error("DTD declarations are prohibited")),
                Event::Decl(decl) => {
                    if !first {
                        return Err(xml_error("misplaced XML declaration"));
                    }
                    if decl.version().map_err(xml_error)?.as_ref() != "1.0" {
                        return Err(xml_error("only XML 1.0 is supported"));
                    }
                    if let Some(enc) = decl.encoding() {
                        let enc = enc.map_err(xml_error)?;
                        if !enc.eq_ignore_ascii_case("utf-8") {
                            return Err(FimModelError::Encoding(enc.into_owned()));
                        }
                    }
                    Content::Raw(raw)
                }
                Event::PI(pi) if pi.target().eq_ignore_ascii_case("xml") => {
                    return Err(xml_error("reserved XML processing instruction"));
                }
                Event::Text(ref text) => {
                    // Resolve entities now: undeclared named entities are not repairable XML.
                    quick_xml::escape::unescape(text.as_ref()).map_err(xml_error)?;
                    if stack.is_empty()
                        && !text
                            .as_ref()
                            .bytes()
                            .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                    {
                        return Err(xml_error("text outside root"));
                    }
                    Content::Raw(raw)
                }
                Event::GeneralRef(reference) => {
                    let decoded = quick_xml::escape::unescape(&format!("&{};", reference.as_ref()))
                        .map_err(xml_error)?
                        .into_owned();
                    if decoded.chars().any(|c| !xml_char(c)) {
                        return Err(xml_error("prohibited XML character reference"));
                    }
                    if stack.is_empty() {
                        return Err(xml_error("reference outside root"));
                    }
                    Content::Raw(raw)
                }
                Event::CData(_) if stack.is_empty() => return Err(xml_error("CDATA outside root")),
                Event::Eof => {
                    if !stack.is_empty() || !root_seen {
                        return Err(xml_error("missing or unclosed root"));
                    }
                    if !raw.is_empty() {
                        tree.document.push(Content::Raw(raw));
                    }
                    return Ok(tree);
                }
                _ => Content::Raw(raw),
            };
            if let Some(parent) = stack.last() {
                tree.records.get_mut(parent).unwrap().children.push(content);
            } else {
                tree.document.push(content);
            }
        }
    }
    fn children(&self, id: RecordId) -> Vec<RecordId> {
        self.records[&id]
            .children
            .iter()
            .filter_map(|c| {
                if let Content::Element(id) = c {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }
    fn child(&self, id: RecordId, local: &str) -> Option<RecordId> {
        self.children(id).into_iter().find(|c| {
            self.records[c].local() == local
                && self.records[c].namespace == self.records[&id].namespace
        })
    }
    fn text(&self, id: RecordId) -> String {
        let mut out = String::new();
        for c in &self.records[&id].children {
            match c {
                Content::Element(c) => out.push_str(&self.text(*c)),
                Content::Raw(raw) if raw.starts_with("<![CDATA[") => out.push_str(
                    raw.strip_prefix("<![CDATA[")
                        .unwrap()
                        .strip_suffix("]]>")
                        .unwrap_or(""),
                ),
                Content::Raw(raw) if !raw.starts_with('<') => out.push_str(&decode(raw)),
                _ => {}
            }
        }
        out
    }
    fn child_text(&self, id: RecordId, local: &str) -> Option<String> {
        self.child(id, local).map(|c| self.text(c))
    }
    fn serialize_content(&self, nodes: &[Content], out: &mut String) {
        for n in nodes {
            match n {
                Content::Raw(raw) => out.push_str(raw),
                Content::Element(id) => {
                    let e = &self.records[id];
                    out.push_str(&e.open);
                    self.serialize_content(&e.children, out);
                    out.push_str(&e.close);
                }
            }
        }
    }
    fn bytes(&self) -> Vec<u8> {
        let mut s = String::new();
        self.serialize_content(&self.document, &mut s);
        s.into_bytes()
    }
    fn root(&self) -> RecordId {
        self.document
            .iter()
            .find_map(|n| {
                if let Content::Element(id) = n {
                    Some(*id)
                } else {
                    None
                }
            })
            .unwrap()
    }
    fn ensure_open(&mut self, id: RecordId) {
        let e = self.records.get_mut(&id).unwrap();
        if e.close.is_empty() {
            e.open = e
                .open
                .trim_end()
                .strip_suffix("/>")
                .unwrap_or(&e.open)
                .to_owned()
                + ">";
            e.close = format!("</{}>", e.name);
        }
    }
    fn set_text(&mut self, id: RecordId, value: &str) -> Result<(), FimModelError> {
        let e = self
            .records
            .get(&id)
            .ok_or_else(|| xml_error("record not found"))?;
        if e.children.iter().any(|c| matches!(c, Content::Element(_))) {
            return Err(FimModelError::DecisionRequired("mixed/element content must be edited via its individual XML fields; replacing it would discard data".into()));
        }
        if self.text(id) == value {
            return Ok(());
        }
        self.ensure_open(id);
        // Keep comments and PIs in place. Replace text/CDATA/reference segments only.
        let children = &mut self.records.get_mut(&id).unwrap().children;
        let first = children.iter().position(|c| matches!(c, Content::Raw(s) if !s.starts_with('<') || s.starts_with("<![CDATA["))).unwrap_or(children.len());
        children.retain(
            |c| !matches!(c, Content::Raw(s) if !s.starts_with('<') || s.starts_with("<![CDATA[")),
        );
        children.insert(first.min(children.len()), Content::Raw(escape(value)));
        Ok(())
    }
    fn insert(
        &mut self,
        parent: RecordId,
        before: Option<RecordId>,
        name: &str,
        namespace: &str,
        text: Option<&str>,
    ) -> Result<RecordId, FimModelError> {
        if !valid_name(name) {
            return Err(xml_error("invalid XML qualified name"));
        }
        if self.records.len() >= MAX_RECORDS {
            return Err(FimModelError::Limit("XML records"));
        }
        let p = self
            .records
            .get(&parent)
            .ok_or_else(|| xml_error("parent record missing"))?;
        let index = match before {
            Some(b) => p
                .children
                .iter()
                .position(|c| *c == Content::Element(b))
                .ok_or_else(|| xml_error("order anchor is not a child"))?,
            None => p.children.len(),
        };
        let id = RecordId(self.next_id);
        self.next_id += 1;
        let mut attrs = Vec::new();
        let prefix = name.split_once(':').map(|(p, _)| p);
        // Explicit namespace declaration makes inserted records independent of inherited prefixes.
        attrs.push(XmlAttribute {
            name: prefix
                .map(|p| format!("xmlns:{p}"))
                .unwrap_or_else(|| "xmlns".into()),
            value: namespace.into(),
        });
        let open = format!("<{name} {}=\"{}\">", attrs[0].name, escape(namespace));
        self.records.insert(
            id,
            Element {
                id,
                parent: Some(parent),
                name: name.into(),
                namespace: namespace.into(),
                attributes: attrs,
                children: text
                    .map(|s| vec![Content::Raw(escape(s))])
                    .unwrap_or_default(),
                open,
                close: format!("</{name}>"),
            },
        );
        self.ensure_open(parent);
        self.records
            .get_mut(&parent)
            .unwrap()
            .children
            .insert(index, Content::Element(id));
        Ok(id)
    }
    fn remove(&mut self, id: RecordId) -> Result<(), FimModelError> {
        let e = self
            .records
            .get(&id)
            .ok_or_else(|| xml_error("record missing"))?;
        let parent = e
            .parent
            .ok_or_else(|| FimModelError::Integrity("cannot remove exchange root".into()))?;
        for child in self.children(id) {
            self.remove(child)?;
        }
        self.records
            .get_mut(&parent)
            .unwrap()
            .children
            .retain(|c| *c != Content::Element(id));
        self.records.remove(&id);
        Ok(())
    }
    fn clone_into(&mut self, source: RecordId, parent: RecordId) -> RecordId {
        let mut e = self.records[&source].clone();
        let id = RecordId(self.next_id);
        self.next_id += 1;
        e.id = id;
        e.parent = Some(parent);
        e.children = e
            .children
            .into_iter()
            .map(|c| match c {
                Content::Element(c) => Content::Element(self.clone_into(c, id)),
                other => other,
            })
            .collect();
        self.records.insert(id, e);
        id
    }
    fn fingerprint(&self, id: RecordId) -> String {
        let e = &self.records[&id];
        let mut s = format!(
            "{{{}}}{}{:?}",
            e.namespace,
            e.local(),
            e.attributes
                .iter()
                .filter(|a| !a.name.starts_with("xmlns"))
                .collect::<Vec<_>>()
        );
        for c in &e.children {
            match c {
                Content::Element(c) => s.push_str(&self.fingerprint(*c)),
                Content::Raw(t) if !t.trim().is_empty() && !t.starts_with("<!--") => {
                    s.push_str(&decode(t))
                }
                _ => {}
            }
        }
        s
    }
}
fn valid_name(name: &str) -> bool {
    name.len() <= 1024
        && !name.is_empty()
        && name.split(':').count() <= 2
        && name.split(':').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
                && chars.all(|c| c == '_' || c == '-' || c == '.' || c.is_alphanumeric())
        })
}

impl FimDocument {
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }
    pub fn message_kind(&self) -> MessageKind {
        self.message_kind
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, FimModelError> {
        let tree = Arc::new(XmlTree::parse(bytes)?);
        let root = &tree.records[&tree.root()];
        let dialect = Dialect::from_namespace(&root.namespace).ok_or_else(|| {
            FimModelError::UnsupportedMessage {
                namespace: root.namespace.clone(),
                root: root.local().into(),
            }
        })?;
        let message_kind = MessageKind::from_root(dialect, root.local()).ok_or_else(|| {
            FimModelError::UnsupportedMessage {
                namespace: root.namespace.clone(),
                root: root.local().into(),
            }
        })?;
        Ok(Self {
            dialect,
            message_kind,
            original: Arc::from(bytes),
            baseline: tree.clone(),
            tree,
            bindings: BTreeMap::new(),
        })
    }
    pub fn original_bytes(&self) -> &[u8] {
        &self.original
    }
    pub fn is_original(&self) -> bool {
        self.tree == self.baseline
    }
    /// Raw serialization only: callers MUST apply exchange-XSD and semantic export gates.
    pub fn serialize(&self) -> Vec<u8> {
        if self.is_original() {
            self.original.to_vec()
        } else {
            self.tree.bytes()
        }
    }
    pub fn content_identity(&self) -> String {
        format!("{:x}", Sha256::digest(self.serialize()))
    }
    pub fn record_for_topic(&self, topic: TopicId) -> Option<RecordId> {
        self.bindings.get(&topic).copied()
    }
    pub fn topic_for_record(&self, record: RecordId) -> Option<TopicId> {
        self.bindings
            .iter()
            .find_map(|(t, r)| (*r == record).then_some(*t))
    }
    fn kind(&self, id: RecordId) -> Option<FimKind> {
        let e = &self.tree.records[&id];
        if e.namespace == "http://docs.oasis-open.org/codelist/ns/genericode/1.0/"
            && e.local() == "CodeList"
        {
            return Some(FimKind::CodeList);
        }
        if e.namespace != self.dialect.namespace() {
            return None;
        }
        if self.message_kind == MessageKind::Translation
            && matches!(
                e.local(),
                "stammdatenschema" | "datenfeldgruppe" | "datenfeld" | "regel"
            )
        {
            return Some(FimKind::Translation);
        }
        Some(match e.local() {
            "stammdatenschema" => FimKind::Schema,
            "datenfeldgruppe"
                if self
                    .tree
                    .child(id, "schemaelementart")
                    .is_some_and(|c| self.tree.text(c).trim() == "X") =>
            {
                FimKind::Choice
            }
            "datenfeldgruppe" => FimKind::Group,
            "datenfeld" => FimKind::Field,
            "regel" => FimKind::Rule,
            "codeliste" | "codelisteReferenz" => FimKind::CodeList,
            "dokumentsteckbrief" => FimKind::DocumentProfile,
            _ => return None,
        })
    }
    fn key(&self, id: RecordId) -> Option<(FimKind, String, Option<String>)> {
        let kind = match self.kind(id)? {
            FimKind::Choice => FimKind::Group,
            kind => kind,
        };
        let identification = self.tree.child(id, "identifikation");
        let identity = identification.and_then(|i| self.tree.child_text(i, "id"));
        let version = identification.and_then(|i| self.tree.child_text(i, "version"));
        if kind == FimKind::CodeList && identity.is_none() {
            if self.tree.records[&id].local() == "codelisteReferenz" {
                return None;
            }
            // Genericode has a different natural key; retain all three components.
            let identification = self
                .tree
                .children(id)
                .into_iter()
                .find(|r| self.tree.records[r].local() == "Identification")?;
            let child_text = |name: &str| {
                self.tree
                    .children(identification)
                    .into_iter()
                    .find(|r| self.tree.records[r].local() == name)
                    .map(|r| self.tree.text(r))
            };
            let canonical =
                child_text("CanonicalUri").or_else(|| child_text("CanonicalIdentification"))?;
            let version = child_text("Version");
            let uri = child_text("CanonicalVersionUri");
            return Some((kind, canonical, Some(format!("{version:?}|{uri:?}"))));
        }
        identity
            .filter(|i| !i.trim().is_empty())
            .map(|i| (kind, i, version))
    }
    fn equivalent(&self, id: RecordId) -> Result<Vec<RecordId>, FimModelError> {
        let Some(key) = self.key(id) else {
            return Ok(vec![id]);
        };
        let ids: Vec<_> = self
            .tree
            .records
            .keys()
            .copied()
            .filter(|r| self.key(*r).as_ref() == Some(&key))
            .collect();
        let fingerprint = self.tree.fingerprint(id);
        if ids.iter().any(|r| self.tree.fingerprint(*r) != fingerprint) {
            return Err(FimModelError::DecisionRequired(format!(
                "conflicting {} identity {}; reconcile the preserved copies before shared editing",
                key.0.badge(),
                key.1
            )));
        }
        Ok(ids)
    }
    pub fn records(&self) -> Vec<RecordMetadata> {
        self.tree
            .records
            .keys()
            .filter_map(|id| {
                let kind = self.kind(*id)?;
                let key = self.key(*id);
                Some(RecordMetadata {
                    record: *id,
                    kind,
                    name: self.label(*id),
                    identity: key.as_ref().map(|k| k.1.clone()),
                    version: key.as_ref().and_then(|k| k.2.clone()),
                    usages: key
                        .as_ref()
                        .map(|k| {
                            self.tree
                                .records
                                .keys()
                                .filter(|r| self.key(**r).as_ref() == Some(k))
                                .count()
                        })
                        .unwrap_or(1),
                })
            })
            .collect()
    }
    pub fn topic_metadata(&self, topic: TopicId) -> Option<TopicMetadata> {
        let record = self.record_for_topic(topic)?;
        let metadata = self.records().into_iter().find(|r| r.record == record);
        let kind = metadata
            .as_ref()
            .map(|m| m.kind)
            .unwrap_or(match self.message_kind {
                MessageKind::Translation => FimKind::Translation,
                MessageKind::CodeList => FimKind::CodeList,
                _ => FimKind::Collection,
            });
        Some(TopicMetadata {
            record,
            kind,
            identity: metadata.as_ref().and_then(|m| m.identity.clone()),
            version: metadata.as_ref().and_then(|m| m.version.clone()),
            cardinality: self
                .usage(record)
                .and_then(|u| self.tree.child_text(u, "anzahl")),
            usages: metadata.map(|m| m.usages).unwrap_or(1),
        })
    }
    pub fn fields(&self, record: RecordId) -> Vec<XmlField> {
        fn walk(doc: &FimDocument, id: RecordId, path: String, out: &mut Vec<XmlField>) {
            let Some(e) = doc.tree.records.get(&id) else {
                return;
            };
            let path = format!("{path}/{}[{}]", e.name, id.0);
            let children = doc.tree.children(id);
            out.push(XmlField {
                record: id,
                parent: e.parent,
                path: path.clone(),
                name: e.name.clone(),
                namespace: e.namespace.clone(),
                value: doc.tree.text(id),
                attributes: e.attributes.clone(),
                has_elements: !children.is_empty(),
            });
            for c in children {
                walk(doc, c, path.clone(), out);
            }
        }
        let mut fields = Vec::new();
        walk(self, record, String::new(), &mut fields);
        fields
    }
    pub fn root_record(&self) -> RecordId {
        self.tree.root()
    }
    /// One dependency service for inspector impact previews, deletion and export
    /// diagnostics. External relations/list bindings are deliberately not fetched.
    /// Free-text/script mentions are review hints, never executable dependencies.
    pub fn dependencies(&self) -> Vec<FimDependency> {
        let mut identities: BTreeMap<String, Vec<RecordId>> = BTreeMap::new();
        for id in self.tree.records.keys() {
            if let Some((_, identity, _)) = self.key(*id) {
                identities.entry(identity).or_default().push(*id);
            }
        }
        let resolve = |identity: &str, version: Option<&str>, external: bool| {
            let targets: Vec<_> = identities
                .get(identity)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|r| {
                    version.is_none_or(|v| self.key(*r).is_some_and(|k| k.2.as_deref() == Some(v)))
                })
                .collect();
            if targets.is_empty() {
                return if external {
                    DependencyResolution::External
                } else {
                    DependencyResolution::Unresolved
                };
            }
            let keys: BTreeSet<_> = targets.iter().filter_map(|r| self.key(*r)).collect();
            if keys.len() > 1
                || targets
                    .iter()
                    .any(|r| self.tree.fingerprint(*r) != self.tree.fingerprint(targets[0]))
            {
                DependencyResolution::Ambiguous(targets)
            } else {
                DependencyResolution::Internal(targets)
            }
        };
        let mut out = Vec::new();
        for (id, e) in &self.tree.records {
            if e.namespace != self.dialect.namespace() {
                continue;
            }
            if matches!(e.local(), "param" | "ziel") {
                if let Some(attr) = e.attributes.iter().find(|a| a.name == "element") {
                    out.push(FimDependency {
                        source: *id,
                        kind: if e.local() == "param" {
                            DependencyKind::RuleParameter
                        } else {
                            DependencyKind::RuleTarget
                        },
                        identity: attr.value.clone(),
                        resolution: resolve(&attr.value, None, false),
                    });
                }
            } else if e.local() == "relation" {
                if let Some(identity) = self
                    .tree
                    .child(*id, "objekt")
                    .and_then(|r| self.tree.child_text(r, "id"))
                {
                    out.push(FimDependency {
                        source: *id,
                        kind: DependencyKind::Relation,
                        identity: identity.clone(),
                        resolution: resolve(
                            &identity,
                            self.tree
                                .child(*id, "objekt")
                                .and_then(|r| self.tree.child_text(r, "version"))
                                .as_deref(),
                            true,
                        ),
                    });
                }
            } else if e.local() == "codelisteReferenz" {
                let identity = self
                    .tree
                    .child_text(*id, "canonicalIdentification")
                    .or_else(|| self.tree.child_text(*id, "canonicalVersionUri"))
                    .unwrap_or_default();
                out.push(FimDependency {
                    source: *id,
                    kind: DependencyKind::CodeListBinding,
                    identity: identity.clone(),
                    resolution: resolve(&identity, None, true),
                });
            } else if matches!(e.local(), "freitextRegel" | "script" | "skript") {
                let text = self.tree.text(*id);
                let mut mentions = BTreeSet::new();
                for token in text.split(|c: char| !c.is_ascii_alphanumeric()) {
                    if token.len() > 1
                        && matches!(token.as_bytes()[0], b'F' | b'G' | b'R' | b'S')
                        && token[1..].bytes().all(|b| b.is_ascii_digit())
                    {
                        mentions.insert(token.to_owned());
                    }
                }
                for identity in mentions {
                    out.push(FimDependency {
                        source: *id,
                        kind: DependencyKind::FreeTextMention,
                        resolution: DependencyResolution::ReviewHint(
                            identities.get(&identity).cloned().unwrap_or_default(),
                        ),
                        identity,
                    });
                }
            }
        }
        for (record, parent) in self.projected() {
            if let Some(parent) = parent {
                out.push(FimDependency {
                    source: parent,
                    kind: DependencyKind::Containment,
                    identity: self.key(record).map(|k| k.1).unwrap_or_default(),
                    resolution: DependencyResolution::Internal(vec![record]),
                });
            }
        }
        out
    }
    pub fn dependency_closure(&self, roots: &[RecordId]) -> Vec<FimDependency> {
        let dependencies = self.dependencies();
        let mut included: BTreeSet<_> = roots
            .iter()
            .flat_map(|r| self.fields(*r).into_iter().map(|f| f.record))
            .collect();
        loop {
            let count = included.len();
            for dependency in &dependencies {
                if included.contains(&dependency.source)
                    && let DependencyResolution::Internal(targets) = &dependency.resolution
                {
                    included.extend(targets);
                }
            }
            if included.len() == count {
                break;
            }
        }
        dependencies
            .into_iter()
            .filter(|d| included.contains(&d.source))
            .collect()
    }
    fn label(&self, id: RecordId) -> String {
        self.tree
            .child_text(id, "name")
            .or_else(|| self.tree.child_text(id, "bezeichnung"))
            .or_else(|| self.key(id).map(|k| k.1))
            .unwrap_or_else(|| self.tree.records[&id].local().into())
    }
    fn usage(&self, id: RecordId) -> Option<RecordId> {
        let parent = self.tree.records[&id].parent?;
        let e = &self.tree.records[&parent];
        if e.local() == "enthaelt" {
            e.parent
                .filter(|p| self.tree.records[p].local() == "struktur")
        } else {
            None
        }
    }
    fn containment_kind(&self, id: RecordId) -> bool {
        matches!(
            self.kind(id),
            Some(
                FimKind::Schema
                    | FimKind::Group
                    | FimKind::Choice
                    | FimKind::Field
                    | FimKind::DocumentProfile
            )
        )
    }
    fn projected(&self) -> Vec<(RecordId, Option<RecordId>)> {
        fn visit(
            doc: &FimDocument,
            id: RecordId,
            parent: Option<RecordId>,
            out: &mut Vec<(RecordId, Option<RecordId>)>,
        ) {
            let project = doc.containment_kind(id);
            let parent = if project {
                out.push((id, parent));
                Some(id)
            } else {
                parent
            };
            // Rules/list/value metadata remain associated objects, never fake containment.
            if matches!(doc.kind(id), Some(FimKind::Rule | FimKind::CodeList)) {
                return;
            }
            for c in doc.tree.children(id) {
                let e = &doc.tree.records[&c];
                // document-profile references in a schema are not contained definitions.
                if e.local() == "dokumentsteckbrief"
                    && doc.tree.child(c, "identifikation").is_none()
                {
                    continue;
                }
                visit(doc, c, parent, out);
            }
        }
        let mut out = Vec::new();
        visit(self, self.tree.root(), None, &mut out);
        if out.is_empty()
            && self.message_kind == MessageKind::CodeList
            && let Some(record) = self
                .tree
                .records
                .keys()
                .copied()
                .find(|r| self.kind(*r) == Some(FimKind::CodeList))
        {
            return vec![(record, None)];
        }
        if out.len() != 1 && out.iter().filter(|(_, p)| p.is_none()).count() != 1 || out.is_empty()
        {
            let root = self.tree.root();
            for (_, p) in &mut out {
                if p.is_none() {
                    *p = Some(root);
                }
            }
            out.insert(0, (root, None));
        }
        out
    }
    pub fn diagnostics(&self) -> Vec<FimDiagnostic> {
        let mut out = Vec::new();
        let mut keys: BTreeMap<(FimKind, String, Option<String>), Vec<RecordId>> = BTreeMap::new();
        for id in self.tree.records.keys().copied() {
            let e = &self.tree.records[&id];
            let diagnostic = if e.namespace == self.dialect.namespace() && e.local() == "anzahl" {
                Cardinality::parse(&self.tree.text(id))
                    .err()
                    .map(|e| ("cardinality", e.to_string(), DiagnosticSeverity::Error))
            } else if let Some(kind) = self.kind(id) {
                if let Some(key) = self.key(id) {
                    keys.entry(key).or_default().push(id);
                    None
                } else if matches!(
                    kind,
                    FimKind::Schema
                        | FimKind::Group
                        | FimKind::Choice
                        | FimKind::Field
                        | FimKind::Rule
                ) {
                    Some(("missing-identity","Definition has no registry identity; complete the local draft before exchange export".into(),DiagnosticSeverity::Error))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((code, message, severity)) = diagnostic {
                out.push(FimDiagnostic {
                    severity,
                    code,
                    message,
                    record: Some(id),
                    topic: self.topic_for_record(id),
                });
            }
        }
        for (key, ids) in keys {
            if let Some(first) = ids.first() {
                let fingerprint = self.tree.fingerprint(*first);
                if ids
                    .iter()
                    .any(|id| self.tree.fingerprint(*id) != fingerprint)
                {
                    for id in &ids {
                        out.push(FimDiagnostic {
                            severity: DiagnosticSeverity::Error,
                            code: "conflicting-identity",
                            message: format!(
                                "{} {} {:?} has conflicting preserved definitions",
                                key.0.badge(),
                                key.1,
                                key.2
                            ),
                            record: Some(*id),
                            topic: self.topic_for_record(*id),
                        });
                    }
                }
                let expected = match key.0 {
                    FimKind::Schema => Some('S'),
                    FimKind::Group | FimKind::Choice => Some('G'),
                    FimKind::Field => Some('F'),
                    FimKind::Rule => Some('R'),
                    _ => None,
                };
                if expected.is_some_and(|prefix| !key.1.starts_with(prefix)) {
                    out.push(FimDiagnostic {
                        severity: DiagnosticSeverity::Error,
                        code: "kind-identity",
                        message: format!(
                            "{} identity {} does not match its object kind",
                            key.0.badge(),
                            key.1
                        ),
                        record: Some(*first),
                        topic: self.topic_for_record(*first),
                    });
                }
            }
        }
        for dependency in self.dependencies() {
            let (severity, code, message) = match dependency.resolution {
                DependencyResolution::Unresolved => (
                    DiagnosticSeverity::Error,
                    "unresolved-reference",
                    format!(
                        "Structured {:?} reference {} does not resolve in this exchange document",
                        dependency.kind, dependency.identity
                    ),
                ),
                DependencyResolution::Ambiguous(_) => (
                    DiagnosticSeverity::Error,
                    "ambiguous-reference",
                    format!(
                        "Reference {} resolves to multiple versions or conflicting definitions",
                        dependency.identity
                    ),
                ),
                DependencyResolution::ReviewHint(targets) if targets.is_empty() => (
                    DiagnosticSeverity::Warning,
                    "rule-reference-review",
                    format!(
                        "Rule text mentions {}; review manually (free text is not a proven dependency)",
                        dependency.identity
                    ),
                ),
                _ => continue,
            };
            out.push(FimDiagnostic {
                severity,
                code,
                message,
                record: Some(dependency.source),
                topic: self.topic_for_record(dependency.source),
            });
        }
        out
    }
    pub fn into_sheet(mut self) -> Result<Sheet, FimModelError> {
        let mut sheet = Sheet::new("FIM", "FIM");
        sheet.structure = Structure::TreeRight;
        self.project_into(&mut sheet)?;
        sheet.title = sheet.root_topic().title.clone();
        sheet.fim = Some(self);
        Ok(sheet)
    }
    fn project_into(&mut self, sheet: &mut Sheet) -> Result<(), FimModelError> {
        let previous_children: BTreeMap<_, _> = sheet
            .topics
            .iter()
            .map(|(id, topic)| (*id, topic.children.clone()))
            .collect();
        let projection = self.projected();
        if projection.len() > 100_000 {
            return Err(FimModelError::Limit("projected occurrences"));
        }
        let old_bindings = self.bindings.clone();
        let mut bindings = BTreeMap::new();
        let mut records_to_topics = BTreeMap::new();
        for (index, (record, _)) in projection.iter().enumerate() {
            let topic = old_bindings
                .iter()
                .find_map(|(t, r)| (*r == *record).then_some(*t))
                .unwrap_or_else(|| {
                    if index == 0 {
                        sheet.root
                    } else {
                        sheet.alloc_topic_id()
                    }
                });
            sheet
                .topics
                .entry(topic)
                .or_insert_with(|| crate::Topic::new(topic, self.label(*record)));
            bindings.insert(topic, *record);
            records_to_topics.insert(*record, topic);
        }
        for (record, parent) in &projection {
            let id = records_to_topics[record];
            let title = self.label(*record);
            let t = sheet.topics.get_mut(&id).unwrap();
            t.title = title;
            t.parent = parent.map(|p| records_to_topics[&p]);
            t.children.clear();
        }
        for (record, parent) in &projection {
            if let Some(parent) = parent {
                let id = records_to_topics[record];
                sheet
                    .topics
                    .get_mut(&records_to_topics[parent])
                    .unwrap()
                    .children
                    .push(id);
            }
        }
        let retained: BTreeSet<_> = bindings.keys().copied().collect();
        sheet.topics.retain(|id, _| retained.contains(id));
        sheet.root = records_to_topics[&projection[0].0];
        sheet.floating.clear();
        sheet
            .relationships
            .retain(|r| retained.contains(&r.from) && retained.contains(&r.to));
        // Core deletion helpers maintain other annotations during generic map operations.
        // Domain removals must not leave visual references to removed topics.
        sheet.boundaries.retain_mut(|b| {
            if !retained.contains(&b.parent) {
                return false;
            }
            if let Some(range) = &mut b.range {
                let old = previous_children
                    .get(&b.parent)
                    .cloned()
                    .unwrap_or_default();
                let selected: Vec<_> = old
                    .get(range.start..=range.end)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|id| {
                        sheet.topics[&b.parent]
                            .children
                            .iter()
                            .position(|c| c == id)
                    })
                    .collect();
                let Some(start) = selected.iter().min() else {
                    return false;
                };
                range.start = *start;
                range.end = *selected.iter().max().unwrap();
            }
            true
        });
        sheet.summaries.retain(|s| retained.contains(&s.topic));
        sheet.scheduling.tasks.retain(|id, _| retained.contains(id));
        sheet
            .scheduling
            .dependencies
            .retain(|d| retained.contains(&d.predecessor) && retained.contains(&d.successor));
        for zone in &mut sheet.zones {
            zone.roots.retain(|id| retained.contains(id));
        }
        sheet.zones.retain(|zone| !zone.roots.is_empty());
        sheet
            .presentation
            .slides
            .retain(|slide| match slide.source {
                crate::PresentationSource::Branch(root) => retained.contains(&root),
                crate::PresentationSource::Zone(zone) => sheet.zones.iter().any(|z| z.id == zone),
            });
        self.bindings = bindings;
        Ok(())
    }
    fn rename(&mut self, record: RecordId, name: &str) -> Result<(), FimModelError> {
        let ids = self.equivalent(record)?;
        for id in ids {
            let name_record = self.tree.child(id, "name");
            let namespace = self.dialect.namespace();
            let tree = Arc::make_mut(&mut self.tree);
            if let Some(r) = name_record {
                tree.set_text(r, name)?;
            } else {
                let before = tree
                    .children(id)
                    .into_iter()
                    .find(|r| tree.records[r].local() != "identifikation");
                tree.insert(id, before, "name", namespace, Some(name))?;
            }
        }
        Ok(())
    }
    fn movable_parent(&self, parent: TopicId) -> Result<RecordId, FimModelError> {
        let id = self
            .record_for_topic(parent)
            .ok_or_else(|| xml_error("unbound parent topic"))?;
        if !matches!(
            self.kind(id),
            Some(FimKind::Schema | FimKind::Group | FimKind::Choice)
        ) {
            return Err(FimModelError::Integrity(
                "only schemas, groups and choices contain fields/groups".into(),
            ));
        }
        self.equivalent(id)?;
        Ok(id)
    }
    /// Resolve a property/edge to corresponding XML records in all equivalent
    /// definitions. Incoming cardinality belongs to the enclosing definition,
    /// never to the contained field/group's shared identity.
    fn shared_records(&self, record: RecordId) -> Result<Vec<RecordId>, FimModelError> {
        if !self.tree.records.contains_key(&record) {
            return Err(xml_error("record missing"));
        }
        let mut current = record;
        let mut path = Vec::new();
        loop {
            if self.key(current).is_some() {
                let owners = self.equivalent(current)?;
                return owners
                    .into_iter()
                    .map(|mut owner| {
                        for index in path.iter().rev() {
                            owner = *self.tree.children(owner).get(*index).ok_or_else(|| {
                                xml_error("equivalent definition property mismatch")
                            })?;
                        }
                        Ok(owner)
                    })
                    .collect();
            }
            let Some(parent) = self.tree.records[&current].parent else {
                return Ok(vec![record]);
            };
            path.push(
                self.tree
                    .children(parent)
                    .iter()
                    .position(|r| *r == current)
                    .unwrap(),
            );
            current = parent;
        }
    }
    fn add(
        &mut self,
        parent: TopicId,
        kind: FimKind,
        name: &str,
        reuse: Option<RecordId>,
    ) -> Result<(), FimModelError> {
        let parent_record = self
            .record_for_topic(parent)
            .ok_or_else(|| xml_error("parent missing"))?;
        let associated = matches!(kind, FimKind::Rule | FimKind::CodeList);
        if !associated {
            self.movable_parent(parent)?;
        }
        let ns = self.dialect.namespace();
        let local = match kind {
            FimKind::Field => "datenfeld",
            FimKind::Group | FimKind::Choice => "datenfeldgruppe",
            FimKind::Rule => "regel",
            FimKind::CodeList => "codeliste",
            _ => {
                return Err(FimModelError::DecisionRequired(
                    "this object needs an explicit exchange envelope workflow".into(),
                ));
            }
        };
        if let Some(source) = reuse {
            self.equivalent(source)?;
            if source == parent_record
                || self
                    .fields(source)
                    .iter()
                    .any(|f| f.record == parent_record)
            {
                return Err(FimModelError::Integrity(
                    "recursive containment reuse is prohibited".into(),
                ));
            }
        }
        let parents = self.equivalent(parent_record)?;
        let tree = Arc::make_mut(&mut self.tree);
        let container = if associated {
            parent_record
        } else {
            let edge = tree.insert(parent_record, None, "struktur", ns, None)?;
            tree.insert(edge, None, "anzahl", ns, Some("1:1"))?;
            tree.insert(edge, None, "enthaelt", ns, None)?
        };
        let added = if let Some(source) = reuse {
            let cloned = tree.clone_into(source, container);
            tree.ensure_open(container);
            tree.records
                .get_mut(&container)
                .unwrap()
                .children
                .push(Content::Element(cloned));
            cloned
        } else {
            let object = tree.insert(container, None, local, ns, None)?;
            tree.insert(object, None, "name", ns, Some(name))?;
            if kind == FimKind::Choice {
                let kind = tree.insert(object, None, "schemaelementart", ns, None)?;
                tree.insert(kind, None, "code", "", Some("X"))?;
            }
            object
        };
        let complete = if associated {
            added
        } else {
            tree.records[&container].parent.unwrap()
        };
        for parent in parents.into_iter().filter(|p| *p != parent_record) {
            let cloned = tree.clone_into(complete, parent);
            tree.ensure_open(parent);
            tree.records
                .get_mut(&parent)
                .unwrap()
                .children
                .push(Content::Element(cloned));
        }
        Ok(())
    }
    fn apply(&mut self, change: FimChange) -> Result<(), FimModelError> {
        let previous = self.diagnostics();
        match change {
            FimChange::SetOccurrenceText { record, value } => {
                Arc::make_mut(&mut self.tree).set_text(record, &value)?;
            }
            FimChange::MakeIndependent {
                topic,
                identity,
                version,
            } => {
                let record = self
                    .record_for_topic(topic)
                    .ok_or_else(|| xml_error("topic missing"))?;
                let kind = self
                    .kind(record)
                    .ok_or_else(|| xml_error("not a definition"))?;
                let expected = match kind {
                    FimKind::Field => 'F',
                    FimKind::Group | FimKind::Choice => 'G',
                    FimKind::Schema => 'S',
                    FimKind::Rule => 'R',
                    _ => {
                        return Err(FimModelError::DecisionRequired(
                            "this identity uses a separate natural-key workflow".into(),
                        ));
                    }
                };
                if !identity.starts_with(expected)
                    || identity[1..].is_empty()
                    || !identity[1..].bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(FimModelError::Integrity("supply a kind-correct registry identity; the editor does not allocate official identifiers".into()));
                }
                if self.tree.records.keys().any(|r| {
                    *r != record
                        && self.key(*r).is_some_and(|k| {
                            k.1 == identity && (version.is_none() || k.2 == version)
                        })
                }) {
                    return Err(FimModelError::DecisionRequired("variant identity/version is already present; use Reuse or a different identity".into()));
                }
                let tree = Arc::make_mut(&mut self.tree);
                let ns = self.dialect.namespace();
                let identification = if let Some(id) = tree.child(record, "identifikation") {
                    id
                } else {
                    let before = tree.children(record).first().copied();
                    tree.insert(record, before, "identifikation", ns, None)?
                };
                if let Some(id) = tree.child(identification, "id") {
                    tree.set_text(id, &identity)?;
                } else {
                    let before = tree.children(identification).first().copied();
                    tree.insert(identification, before, "id", ns, Some(&identity))?;
                }
                if let Some(version) = version {
                    if let Some(id) = tree.child(identification, "version") {
                        tree.set_text(id, &version)?;
                    } else {
                        tree.insert(identification, None, "version", ns, Some(&version))?;
                    }
                }
            }
            FimChange::SetText { record, value } => {
                for record in self.shared_records(record)? {
                    Arc::make_mut(&mut self.tree).set_text(record, &value)?;
                }
            }
            FimChange::SetAttribute {
                record,
                name,
                value,
            } => {
                if !valid_name(&name) {
                    return Err(xml_error("invalid attribute name"));
                }
                if name == "xmlns" || name.starts_with("xmlns:") {
                    return Err(FimModelError::DecisionRequired(
                        "namespace rebinding requires a complete staged XML replacement".into(),
                    ));
                }
                let records = self.shared_records(record)?;
                let tree = Arc::make_mut(&mut self.tree);
                for record in records {
                    let e = tree
                        .records
                        .get_mut(&record)
                        .ok_or_else(|| xml_error("record missing"))?;
                    if let Some(i) = e.attributes.iter().position(|a| a.name == name) {
                        if let Some(value) = &value {
                            e.attributes[i].value = value.clone();
                        } else {
                            e.attributes.remove(i);
                        }
                    } else if let Some(value) = &value {
                        e.attributes.push(XmlAttribute {
                            name: name.clone(),
                            value: value.clone(),
                        });
                    }
                    let empty = e.close.is_empty();
                    e.open = format!(
                        "<{}{}{}>",
                        e.name,
                        e.attributes
                            .iter()
                            .map(|a| format!(" {}=\"{}\"", a.name, escape(&a.value)))
                            .collect::<String>(),
                        if empty { "/" } else { "" }
                    );
                }
            }
            FimChange::InsertElement {
                parent,
                before,
                name,
                namespace,
                text,
            } => {
                let parents = self.shared_records(parent)?;
                let position = before
                    .map(|b| {
                        self.tree
                            .children(parent)
                            .iter()
                            .position(|r| *r == b)
                            .ok_or_else(|| xml_error("order anchor is not a child"))
                    })
                    .transpose()?;
                for parent in parents {
                    let before = position.and_then(|i| self.tree.children(parent).get(i).copied());
                    Arc::make_mut(&mut self.tree).insert(
                        parent,
                        before,
                        &name,
                        &namespace,
                        text.as_deref(),
                    )?;
                }
            }
            FimChange::RemoveElement { record } => {
                let records = self.shared_records(record)?;
                for record in &records {
                    self.check_removal(*record)?;
                }
                for record in records {
                    Arc::make_mut(&mut self.tree).remove(record)?;
                }
            }
            FimChange::RenameDefinition { record, name } => self.rename(record, &name)?,
            FimChange::SetCardinality { topic, value } => {
                Cardinality::parse(&value).map_err(|e| FimModelError::Integrity(e.to_string()))?;
                let record = self
                    .record_for_topic(topic)
                    .ok_or_else(|| xml_error("topic missing"))?;
                let usage = self.usage(record).ok_or_else(|| {
                    FimModelError::Integrity(
                        "root/associated object has no containment cardinality".into(),
                    )
                })?;
                let target = self.tree.child(usage, "anzahl").ok_or_else(|| {
                    xml_error("usage has no anzahl; insert it explicitly in schema order")
                })?;
                for target in self.shared_records(target)? {
                    Arc::make_mut(&mut self.tree).set_text(target, &value)?;
                }
            }
            FimChange::Add { parent, kind, name } => self.add(parent, kind, &name, None)?,
            FimChange::Reuse { parent, record } => {
                let kind = self
                    .kind(record)
                    .ok_or_else(|| xml_error("not a definition"))?;
                self.add(parent, kind, "", Some(record))?;
            }
            FimChange::Remove { topic } => {
                let record = self
                    .record_for_topic(topic)
                    .ok_or_else(|| xml_error("topic missing"))?;
                let remove = self.usage(record).unwrap_or(record);
                let records = self.shared_records(remove)?;
                for record in &records {
                    self.check_removal(*record)?;
                }
                for record in records {
                    Arc::make_mut(&mut self.tree).remove(record)?;
                }
            }
            FimChange::Move {
                topic,
                parent,
                index,
            } => {
                let target = self.movable_parent(parent)?;
                let record = self
                    .record_for_topic(topic)
                    .ok_or_else(|| xml_error("topic missing"))?;
                let edge = self.usage(record).ok_or_else(|| {
                    FimModelError::Integrity("root cannot be moved into containment".into())
                })?;
                if self.fields(record).iter().any(|f| f.record == target) {
                    return Err(FimModelError::Integrity("containment cycle".into()));
                }
                let old_parent = self.tree.records[&edge].parent.unwrap();
                let sources = self.shared_records(edge)?;
                let targets = self.equivalent(target)?;
                if sources.iter().any(|source| {
                    targets
                        .iter()
                        .any(|target| self.fields(*source).iter().any(|f| f.record == *target))
                }) {
                    return Err(FimModelError::Integrity("shared containment cycle".into()));
                }
                let same_parent = old_parent == target
                    || self.key(old_parent).is_some() && self.key(old_parent) == self.key(target);
                let tree = Arc::make_mut(&mut self.tree);
                for source in &sources {
                    let old_parent = tree.records[source].parent.unwrap();
                    tree.records
                        .get_mut(&old_parent)
                        .unwrap()
                        .children
                        .retain(|c| *c != Content::Element(*source));
                }
                let moves: Vec<_> = if same_parent {
                    sources
                        .iter()
                        .map(|edge| (*edge, tree.records[edge].parent.unwrap()))
                        .collect()
                } else {
                    targets
                        .iter()
                        .map(|parent| {
                            let moved = if *parent == target {
                                edge
                            } else {
                                tree.clone_into(edge, *parent)
                            };
                            (moved, *parent)
                        })
                        .collect()
                };
                for (edge, target) in moves {
                    let edges: Vec<_> = tree
                        .children(target)
                        .into_iter()
                        .filter(|c| tree.records[c].local() == "struktur")
                        .collect();
                    let anchor = edges.get(index).copied();
                    let p = tree.records.get_mut(&target).unwrap();
                    let index = anchor
                        .and_then(|a| p.children.iter().position(|c| *c == Content::Element(a)))
                        .unwrap_or(p.children.len());
                    p.children.insert(index, Content::Element(edge));
                    tree.records.get_mut(&edge).unwrap().parent = Some(target);
                }
                if !same_parent {
                    for source in sources {
                        if source != edge {
                            tree.remove(source)?;
                        }
                    }
                }
            }
            FimChange::Duplicate { topic } => {
                let record = self
                    .record_for_topic(topic)
                    .ok_or_else(|| xml_error("topic missing"))?;
                let edge = self.usage(record).ok_or_else(|| {
                    FimModelError::DecisionRequired(
                        "duplicate standalone objects using a complete independent sheet".into(),
                    )
                })?;
                self.equivalent(record)?;
                let parent = self.tree.records[&edge].parent.unwrap();
                let parent_topic = self
                    .topic_for_record(parent)
                    .ok_or_else(|| xml_error("parent binding missing"))?;
                self.movable_parent(parent_topic)?;
                let edges = self.shared_records(edge)?;
                let tree = Arc::make_mut(&mut self.tree);
                for edge in edges {
                    let parent = tree.records[&edge].parent.unwrap();
                    let cloned = tree.clone_into(edge, parent);
                    let p = tree.records.get_mut(&parent).unwrap();
                    let index = p
                        .children
                        .iter()
                        .position(|c| *c == Content::Element(edge))
                        .unwrap()
                        + 1;
                    p.children.insert(index, Content::Element(cloned));
                }
            }
        }
        // Every XML-first edit is staged and reparsed before commit. This checks namespace
        // bindings, escaping, envelope and processing budgets, without a network resolver.
        let bytes = self.tree.bytes();
        let parsed = Self::parse(&bytes)?;
        self.tree.validate_native()?;
        if parsed.dialect != self.dialect || parsed.message_kind != self.message_kind {
            return Err(FimModelError::Integrity(
                "change altered exchange envelope".into(),
            ));
        }
        for diagnostic in self.diagnostics() {
            if matches!(
                diagnostic.code,
                "cardinality"
                    | "kind-identity"
                    | "conflicting-identity"
                    | "unresolved-reference"
                    | "ambiguous-reference"
            ) && !previous.iter().any(|old| {
                old.code == diagnostic.code
                    && old.record == diagnostic.record
                    && old.message == diagnostic.message
            }) {
                return Err(FimModelError::Integrity(diagnostic.message));
            }
        }
        Ok(())
    }
    fn check_removal(&self, record: RecordId) -> Result<(), FimModelError> {
        let removed: BTreeSet<_> = self.fields(record).iter().map(|f| f.record).collect();
        for dependency in self.dependencies() {
            if removed.contains(&dependency.source)
                || dependency.kind == DependencyKind::Containment
            {
                continue;
            }
            if let DependencyResolution::Internal(targets)
            | DependencyResolution::Ambiguous(targets) = dependency.resolution
                && !targets.is_empty()
                && targets.iter().all(|target| removed.contains(target))
            {
                return Err(FimModelError::DecisionRequired(format!(
                    "removal breaks {:?} reference to {}; reconcile its dependencies explicitly",
                    dependency.kind, dependency.identity
                )));
            }
        }
        let removed_keys: Vec<_> = removed.iter().filter_map(|r| self.key(*r)).collect();
        for (id, e) in &self.tree.records {
            if removed.contains(id) {
                continue;
            }
            if matches!(
                e.local(),
                "id" | "datenfeld"
                    | "datenfeldgruppe"
                    | "ziel"
                    | "target"
                    | "parameter"
                    | "referenz"
            ) {
                let text = self.tree.text(*id);
                for key in &removed_keys {
                    let remaining = self
                        .tree
                        .records
                        .keys()
                        .any(|r| !removed.contains(r) && self.key(*r).as_ref() == Some(key));
                    if !remaining && text.trim() == key.1 && self.kind(*id).is_none() {
                        // Identity children of surviving unrelated records are checked by kind
                        // identity diagnostics; real structured references must not dangle.
                        if e.parent
                            .is_some_and(|p| self.tree.records[&p].local() != "identifikation")
                        {
                            return Err(FimModelError::DecisionRequired(format!(
                                "removal leaves structured reference to {}; repair/reconcile its dependencies explicitly",
                                key.1
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Sheet {
    /// Apply inside `Document::edit_sheet` for one complete undoable transaction.
    /// This method itself is atomic too, so rejected field drafts never leak.
    pub fn apply_fim_change(&mut self, change: FimChange) -> Result<(), FimModelError> {
        let mut candidate = self.clone();
        let mut fim = candidate
            .fim
            .take()
            .ok_or_else(|| FimModelError::Integrity("sheet is not a FIM document".into()))?;
        fim.apply(change)?;
        fim.project_into(&mut candidate)?;
        candidate.fim = Some(fim);
        candidate.validate().map_err(FimModelError::Integrity)?;
        *self = candidate;
        Ok(())
    }
}

/// Shared transaction boundary. Supported map mutations are translated through
/// XML-first commands; ambiguous/generic structural changes return useful errors.
pub(crate) fn reconcile(before: &Sheet, after: &mut Sheet) -> Result<(), FimModelError> {
    let Some(old) = before.fim.as_ref() else {
        if after.fim.is_some() {
            validate_projection(after)?;
        }
        return Ok(());
    };
    let Some(current) = after.fim.as_ref() else {
        return Err(FimModelError::DecisionRequired(
            "FIM source cannot be detached implicitly; use an explicit non-FIM projection copy"
                .into(),
        ));
    };
    // Validate the generic tree before walking user-supplied topology recursively.
    let mut stack = vec![(after.root, 0usize)];
    let mut visited = BTreeSet::new();
    while let Some((id, depth)) = stack.pop() {
        if depth > MAX_DEPTH || visited.len() > 100_000 {
            return Err(FimModelError::Limit("map projection depth/occurrences"));
        }
        if !visited.insert(id) {
            return Err(FimModelError::Integrity(
                "map projection cycle or multiply owned topic".into(),
            ));
        }
        let topic = after
            .topic(id)
            .ok_or_else(|| FimModelError::Integrity("map projection topic missing".into()))?;
        for child in &topic.children {
            if after.parent(*child) != Some(id) {
                return Err(FimModelError::Integrity(
                    "map projection parent/child mismatch".into(),
                ));
            }
            stack.push((*child, depth + 1));
        }
    }
    if visited.len() != after.topics.len() {
        return Err(FimModelError::DecisionRequired("FIM maps cannot contain unbound floating/summary topics; use a separate ordinary sheet".into()));
    }
    if current.tree != old.tree {
        return validate_projection(after);
    }
    if after.root != before.root {
        return Err(FimModelError::DecisionRequired(
            "changing a FIM root requires a message-envelope workflow".into(),
        ));
    }
    let mut fim = current.clone();
    // Rename first: definition-aware and propagated only across equivalent copies.
    let mut renames = Vec::new();
    for (topic, record) in &old.bindings {
        if let (Some(a), Some(b)) = (after.topic(*topic), before.topic(*topic))
            && a.title != b.title
        {
            renames.push((*record, a.title.clone()));
        }
    }
    for (i, (record, name)) in renames.iter().enumerate() {
        if renames[..i].iter().any(|(other, other_name)| {
            old.key(*record).is_some() && old.key(*record) == old.key(*other) && name != other_name
        }) {
            return Err(FimModelError::DecisionRequired("one transaction proposes conflicting names for a shared definition; choose one shared name or create an independent variant".into()));
        }
        fim.rename(*record, name)?;
    }
    // Remove topmost deleted usages, not every descendant independently.
    let removed: BTreeSet<_> = old
        .bindings
        .keys()
        .copied()
        .filter(|id| !after.contains(*id))
        .collect();
    // Reparent/reorder full usage records, preserving cardinality and metadata.
    for id in old.bindings.keys() {
        if !after.contains(*id) || removed.contains(id) {
            continue;
        }
        for id in &removed {
            if before.parent(*id).is_none_or(|p| !removed.contains(&p)) {
                fim.apply(FimChange::Remove { topic: *id })?;
            }
        }
        if (after.parent(*id) != before.parent(*id)
            || after.index_in_parent(*id) != before.index_in_parent(*id))
            && let Some(parent) = after.parent(*id)
        {
            fim.apply(FimChange::Move {
                topic: *id,
                parent,
                index: after.index_in_parent(*id).unwrap_or(usize::MAX),
            })?;
        }
    }
    // Ordinary map insertion cannot infer F/G/C/R identity or dependency closure.
    // Reuse is recognized only for an exact, intact copied occurrence tree.
    let added: BTreeSet<_> = after
        .topics
        .keys()
        .filter(|id| !before.contains(**id))
        .copied()
        .collect();
    fn same_projection(a: &Sheet, ai: TopicId, b: &Sheet, bi: TopicId) -> bool {
        let Some(at) = a.topic(ai) else {
            return false;
        };
        let Some(bt) = b.topic(bi) else {
            return false;
        };
        at.title == bt.title
            && at.children.len() == bt.children.len()
            && at
                .children
                .iter()
                .zip(&bt.children)
                .all(|(ac, bc)| same_projection(a, *ac, b, *bc))
    }
    let roots: Vec<_> = added
        .iter()
        .filter(|id| after.parent(**id).is_none_or(|p| !added.contains(&p)))
        .copied()
        .collect();
    for root in roots {
        let candidates: Vec<_> = old
            .bindings
            .keys()
            .copied()
            .filter(|source| same_projection(after, root, before, *source))
            .collect();
        let Some(source) = candidates.first().copied() else {
            return Err(FimModelError::DecisionRequired("choose Field, Group, Choice, Rule or Codelist (Add), or an existing definition (Reuse); generic paste cannot safely infer identity/reference closure".into()));
        };
        let source_record = old.bindings[&source];
        let key = old.key(source_record);
        if candidates.iter().any(|c| {
            old.key(old.bindings[c]) != key
                || old.tree.fingerprint(old.bindings[c]) != old.tree.fingerprint(source_record)
        }) {
            return Err(FimModelError::DecisionRequired("multiple definitions match the copied map shape; choose the source definition explicitly with Reuse".into()));
        }
        let subtree = after.subtree_ids(root);
        if subtree.iter().any(|id| !added.contains(id)) {
            return Err(FimModelError::DecisionRequired("generic paste mixed existing and new occurrences; use an explicit FIM move/reuse transaction".into()));
        }
        let prior: BTreeSet<_> = fim.tree.records.keys().copied().collect();
        fim.apply(FimChange::Duplicate { topic: source })?;
        let new_projection: Vec<_> = fim
            .projected()
            .into_iter()
            .filter(|(r, _)| !prior.contains(r))
            .collect();
        if new_projection.len() != subtree.len() {
            return Err(FimModelError::Integrity(
                "copied occurrence closure does not match its XML source".into(),
            ));
        }
        for (topic, (record, _)) in subtree.into_iter().zip(new_projection) {
            fim.bindings.insert(topic, record);
        }
        let parent = after.parent(root).ok_or_else(|| {
            FimModelError::DecisionRequired("FIM duplicates must have a containment parent".into())
        })?;
        fim.apply(FimChange::Move {
            topic: root,
            parent,
            index: after.index_in_parent(root).unwrap_or(usize::MAX),
        })?;
    }
    fim.project_into(after)?;
    after.fim = Some(fim);
    validate_projection(after)
}
pub fn validate_projection(sheet: &Sheet) -> Result<(), FimModelError> {
    let Some(fim) = sheet.fim.as_ref() else {
        return Ok(());
    };
    let projection = fim.projected();
    if fim.bindings.len() != projection.len() || fim.bindings.len() != sheet.topics.len() {
        return Err(FimModelError::Integrity(
            "topic/source binding is incomplete; explicit projection repair required".into(),
        ));
    }
    let root = &fim.tree.records[&fim.tree.root()];
    if root.namespace != fim.dialect.namespace()
        || MessageKind::from_root(fim.dialect, root.local()) != Some(fim.message_kind)
    {
        return Err(FimModelError::Integrity(
            "FIM profile does not match canonical envelope".into(),
        ));
    }
    let reverse: BTreeMap<_, _> = fim
        .bindings
        .iter()
        .map(|(topic, record)| (*record, *topic))
        .collect();
    if reverse.len() != fim.bindings.len() {
        return Err(FimModelError::Integrity(
            "multiple topics bound to one source record".into(),
        ));
    }
    let mut expected_children: BTreeMap<RecordId, Vec<TopicId>> = BTreeMap::new();
    for (record, parent) in &projection {
        if let Some(parent) = parent
            && let Some(topic) = reverse.get(record)
        {
            expected_children.entry(*parent).or_default().push(*topic);
        }
    }
    let mut seen = BTreeSet::new();
    for (record, parent) in projection {
        let topic = fim.topic_for_record(record).ok_or_else(|| {
            FimModelError::Integrity("source occurrence has no persistent topic binding".into())
        })?;
        if !seen.insert(topic) {
            return Err(FimModelError::Integrity("duplicate source binding".into()));
        }
        let t = sheet
            .topic(topic)
            .ok_or_else(|| FimModelError::Integrity("bound topic missing".into()))?;
        if t.title != fim.label(record) || t.parent != parent.and_then(|r| fim.topic_for_record(r))
        {
            return Err(FimModelError::Integrity(
                "map/source projection mismatch; use FIM domain commands".into(),
            ));
        }
        if t.children != expected_children.remove(&record).unwrap_or_default() {
            return Err(FimModelError::Integrity(
                "map/source child order mismatch".into(),
            ));
        }
    }
    Ok(())
}

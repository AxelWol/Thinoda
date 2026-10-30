//! Offline exchange-XSD validation infrastructure for FIM authoring.
//!
//! This is not an importer or a complete FIM semantic validator. Official
//! schemas are installed separately, after consent, and checked by hash.
//! Instance-supplied schema locations never participate in schema loading.

use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

use quick_xml::{NsReader, events::Event, name::ResolveResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use xsd_schema::{
    SchemaSet, parse_schema_only, process_loaded_schemas,
    validation::{
        SchemaValidator, SchemaValidity, ValidationError, ValidationFlags, ValidationSink,
        ValidationWarning, drive_quick_xml,
    },
    xml::ParseLimits,
};

pub const MAX_XML_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_XML_DEPTH: usize = 256;
pub const MAX_XML_NODES: usize = 1_000_000;
pub const MAX_DIAGNOSTICS: usize = 200;
pub const VALIDATION_POLICY_VERSION: u32 = 1;
const MAX_SCHEMA_ENTRY_BYTES: u64 = 512 * 1024;
// Pinned ZIPs are 19,679 / 16,552 / 32,140 bytes. Re-pinning an artifact
// requires reviewing this cap as well as its hash and complete schema closure.
const MAX_SCHEMA_ARCHIVE_BYTES: usize = 1024 * 1024;

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
            .find(|dialect| namespace == dialect.namespace())
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
    fn from_root(dialect: Dialect, root: &str) -> Option<Self> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageProfile {
    pub dialect: Dialect,
    pub kind: MessageKind,
}

#[derive(Debug, Error)]
pub enum FimError {
    #[error("FIM XML error: {0}")]
    Xml(String),
    #[error("FIM XML encoding is not supported by this validator: {0}")]
    Encoding(String),
    #[error("FIM processing limit exceeded: {0}")]
    Limit(&'static str),
    #[error("DTD declarations are prohibited in FIM documents")]
    Dtd,
    #[error("unsupported FIM message {{{namespace}}}{root}")]
    UnsupportedMessage { namespace: String, root: String },
    #[error("the installed FIM schemas are for a different dialect")]
    WrongDialect,
    #[error("official schema download {artifact} has hash {actual}, expected {expected}")]
    SchemaHash {
        artifact: &'static str,
        expected: &'static str,
        actual: String,
    },
    #[error("XDF3 code-list validation also requires the official examples schema supplement")]
    MissingSupplement,
    #[error("FIM schema archive error: {0}")]
    Archive(String),
    #[error("FIM schema compilation failed: {0}")]
    Schema(String),
}

/// Bounded message inspection, not proof of XML or XDF conformance.
pub fn inspect(bytes: &[u8]) -> Result<MessageProfile, FimError> {
    if bytes.len() > MAX_XML_BYTES {
        return Err(FimError::Limit("XML bytes"));
    }
    std::str::from_utf8(bytes).map_err(|error| FimError::Encoding(error.to_string()))?;
    let mut reader = NsReader::from_reader(bytes);
    let mut depth = 0usize;
    let mut nodes = 0usize;
    let mut profile = None;
    let mut seen_event = false;
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|error| FimError::Xml(error.to_string()))?;
        let at_start = !seen_event;
        seen_event = true;
        match event {
            Event::DocType(_) => return Err(FimError::Dtd),
            Event::Decl(declaration) => {
                if !at_start {
                    return Err(FimError::Xml(
                        "XML declaration must precede all document content".into(),
                    ));
                }
                if let Some(encoding) = declaration.encoding() {
                    let encoding = encoding.map_err(|error| FimError::Xml(error.to_string()))?;
                    if !encoding.eq_ignore_ascii_case("utf-8") {
                        return Err(FimError::Encoding(encoding.into_owned()));
                    }
                }
            }
            Event::PI(instruction) if instruction.target().eq_ignore_ascii_case("xml") => {
                return Err(FimError::Xml(
                    "the XML processing-instruction target is reserved".into(),
                ));
            }
            Event::Start(ref element) | Event::Empty(ref element) => {
                nodes += 1;
                if nodes > MAX_XML_NODES || depth >= MAX_XML_DEPTH {
                    return Err(FimError::Limit("XML nodes or depth"));
                }
                if element.name().as_ref().len() > 1024 {
                    return Err(FimError::Limit("XML name bytes"));
                }
                if element.attributes().count() > 256 {
                    return Err(FimError::Limit("XML attributes per element"));
                }
                if depth == 0 {
                    if profile.is_some() {
                        return Err(FimError::Xml("multiple document roots".into()));
                    }
                    let ns = match namespace {
                        ResolveResult::Bound(ns) => ns.as_ref().to_owned(),
                        ResolveResult::Unbound => String::new(),
                        ResolveResult::Unknown(prefix) => {
                            return Err(FimError::Xml(format!(
                                "unbound namespace prefix {}",
                                prefix
                            )));
                        }
                    };
                    let local = element.local_name();
                    profile = Dialect::from_namespace(&ns).and_then(|dialect| {
                        MessageKind::from_root(dialect, local.as_ref())
                            .map(|kind| MessageProfile { dialect, kind })
                    });
                    if profile.is_none() {
                        return Err(FimError::UnsupportedMessage {
                            namespace: ns,
                            root: local.as_ref().to_owned(),
                        });
                    }
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| FimError::Xml("unexpected closing element".into()))?;
            }
            Event::Text(text) if depth == 0 => {
                if !text
                    .as_ref()
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                {
                    return Err(FimError::Xml("text outside the document root".into()));
                }
            }
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => {
                return Err(FimError::Xml(
                    "CDATA or character reference outside the document root".into(),
                ));
            }
            Event::Eof => {
                if depth != 0 {
                    return Err(FimError::Xml("unclosed document root".into()));
                }
                return profile.ok_or_else(|| FimError::Xml("missing document root".into()));
            }
            _ => {}
        }
    }
}

/// Pinned official sources; these descriptors do not perform network access.
#[derive(Debug, Clone, Copy)]
pub struct SchemaDownload {
    pub artifact: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

const XDF2_SCHEMAS: SchemaDownload = SchemaDownload {
    artifact: "XDatenfelder_2.0_XML-Schema.zip",
    url: "https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_2.0:xmlschema:datei:XDatenfelder_2.0_XML-Schema.zip",
    sha256: "6687900227b8b151bb1a4a6ce9b61c8e3f21c3dada130296efa74a985936645f",
};
const XDF3_SCHEMAS: SchemaDownload = SchemaDownload {
    artifact: "XDatenfelder_3.0.0_XML-Schema.zip",
    url: "https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_3.0.0:xmlschema:datei:XDatenfelder_3.0.0_XML-Schema.zip",
    sha256: "3111627bfe4ee923a22e9ca1248b0478dfbfe11afa3cd4730db6a6e63b148a38",
};
const XDF3_SUPPLEMENT: SchemaDownload = SchemaDownload {
    artifact: "XDatenfelder_3.0.0_Beispiele.zip",
    url: "https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_3.0.0:dokument:XDatenfelder_3.0.0_Beispiele:datei:XDatenfelder_3.0.0_Beispiele.zip",
    sha256: "985c9bd0bacad9c4f4a9706cb0e8986a41d4eaa3bfe87523e4bcbbffa1d08fd0",
};

pub fn official_downloads(dialect: Dialect) -> &'static [SchemaDownload] {
    match dialect {
        Dialect::Xdf2 => &[XDF2_SCHEMAS],
        Dialect::Xdf3 => &[XDF3_SCHEMAS, XDF3_SUPPLEMENT],
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verified_zip(
    bytes: &[u8],
    download: SchemaDownload,
) -> Result<zip::ZipArchive<Cursor<&[u8]>>, FimError> {
    if bytes.len() > MAX_SCHEMA_ARCHIVE_BYTES {
        return Err(FimError::Limit("schema archive bytes"));
    }
    let actual = digest(bytes);
    if actual != download.sha256 {
        return Err(FimError::SchemaHash {
            artifact: download.artifact,
            expected: download.sha256,
            actual,
        });
    }
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| FimError::Archive(error.to_string()))
}

fn schema_entry(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, FimError> {
    let entry = zip
        .by_name(name)
        .map_err(|error| FimError::Archive(format!("{name}: {error}")))?;
    if entry.size() > MAX_SCHEMA_ENTRY_BYTES {
        return Err(FimError::Limit("schema entry bytes"));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_SCHEMA_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| FimError::Archive(error.to_string()))?;
    if bytes.len() as u64 > MAX_SCHEMA_ENTRY_BYTES {
        return Err(FimError::Limit("schema entry bytes"));
    }
    Ok(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
}

/// XSD-layer evidence for exact input bytes, not a FIM semantic approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExchangeReport {
    pub profile: MessageProfile,
    pub content_sha256: String,
    pub schema_sha256: String,
    pub policy_version: u32,
    pub diagnostics: Vec<Diagnostic>,
    pub omitted_diagnostics: usize,
    conformant: bool,
}

impl ExchangeReport {
    pub fn is_exchange_conformant(&self) -> bool {
        self.conformant
    }
}

#[derive(Default)]
struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
    omitted: usize,
    has_errors: bool,
}

impl DiagnosticSink {
    fn push(&mut self, diagnostic: Diagnostic) {
        self.has_errors |= diagnostic.severity == Severity::Error;
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        } else {
            self.omitted += 1;
        }
    }
}

impl ValidationSink for &mut DiagnosticSink {
    fn on_error(&mut self, error: ValidationError) {
        let path = error.instance_path().map(ToString::to_string);
        self.push(Diagnostic {
            severity: Severity::Error,
            code: error.constraint.to_owned(),
            message: error.message,
            path,
        });
    }

    fn on_warning(&mut self, warning: ValidationWarning) {
        let path = warning.instance_path().map(ToString::to_string);
        self.push(Diagnostic {
            severity: Severity::Warning,
            code: warning.code.to_owned(),
            message: warning.message,
            path,
        });
    }
}

/// Compile once off the UI thread, then reuse for this exact dialect/bundle.
pub struct ExchangeValidator {
    dialect: Dialect,
    schemas: SchemaSet,
    schema_sha256: String,
}

impl ExchangeValidator {
    pub fn from_official_archives(
        dialect: Dialect,
        exchange: &[u8],
        supplement: Option<&[u8]>,
    ) -> Result<Self, FimError> {
        let download = match dialect {
            Dialect::Xdf2 => XDF2_SCHEMAS,
            Dialect::Xdf3 => XDF3_SCHEMAS,
        };
        let mut archive = verified_zip(exchange, download)?;
        let mut files = BTreeMap::new();
        for name in [
            "xdatenfelder.xsd",
            "xdatenfelder-baukasten.xsd",
            "xdatenfelder-datentypen.xsd",
            "xdatenfelder_verzeichnis.xsd",
            "latinchars.xsd",
            "xoev-code.xsd",
        ] {
            files.insert(
                name.to_owned(),
                schema_entry(&mut archive, &format!("xsd/{name}"))?,
            );
        }
        let genericode = match dialect {
            Dialect::Xdf2 => schema_entry(&mut archive, "xsd/genericode.xsd")?,
            Dialect::Xdf3 => {
                let bytes = supplement.ok_or(FimError::MissingSupplement)?;
                let mut archive = verified_zip(bytes, XDF3_SUPPLEMENT)?;
                schema_entry(&mut archive, "Beispiele/xsd/genericode.xsd")?
            }
        };
        files.insert("genericode.xsd".into(), genericode);
        Self::compile(dialect, files)
    }

    fn compile(dialect: Dialect, mut files: BTreeMap<String, Vec<u8>>) -> Result<Self, FimError> {
        files.insert("xml.xsd".into(), xsd_schema::embedded::XML_XSD.to_vec());
        let mut schemas = SchemaSet::new();
        let mut hash = Sha256::new();
        for (name, bytes) in &files {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
            parse_schema_only(bytes, name, &mut schemas)
                .map_err(|error| FimError::Schema(error.to_string()))?;
        }
        process_loaded_schemas(&mut schemas)
            .map_err(|error| FimError::Schema(error.to_string()))?;
        if !schemas.directive_failures().is_empty() {
            return Err(FimError::Schema(format!(
                "incomplete local schema closure: {:?}",
                schemas.directive_failures()
            )));
        }
        Ok(Self {
            dialect,
            schemas,
            schema_sha256: format!("{:x}", hash.finalize()),
        })
    }

    pub fn validate(&self, bytes: &[u8]) -> Result<ExchangeReport, FimError> {
        let profile = inspect(bytes)?;
        if profile.dialect != self.dialect {
            return Err(FimError::WrongDialect);
        }
        let validator = SchemaValidator::new(
            &self.schemas,
            ValidationFlags::default() & !ValidationFlags::BUILD_PSVI_TYPED_VALUES,
        );
        let mut sink = DiagnosticSink::default();
        let mut runtime = validator.start_run(&mut sink);
        runtime.set_parse_limits(
            ParseLimits::none()
                .with_max_depth(MAX_XML_DEPTH)
                .with_max_nodes(MAX_XML_NODES)
                .with_max_name_bytes(1024),
        );
        let result = drive_quick_xml(bytes, &mut runtime, &self.schemas);
        drop(runtime);
        let conformant = match result {
            Ok(outcome) => outcome.root_validity == Some(SchemaValidity::Valid) && !sink.has_errors,
            Err(error) => {
                sink.push(Diagnostic {
                    severity: Severity::Error,
                    code: "xml-validation".into(),
                    message: error.to_string(),
                    path: None,
                });
                false
            }
        };
        Ok(ExchangeReport {
            profile,
            content_sha256: digest(bytes),
            schema_sha256: self.schema_sha256.clone(),
            policy_version: VALIDATION_POLICY_VERSION,
            diagnostics: sink.diagnostics,
            omitted_diagnostics: sink.omitted,
            conformant,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator() -> ExchangeValidator {
        let source = format!(
            "<xs:schema xmlns:xs='http://www.w3.org/2001/XMLSchema' targetNamespace='{}'>\
             <xs:element name='xdatenfelder.stammdatenschema.0102' type='xs:integer'/>\
             </xs:schema>",
            Dialect::Xdf3.namespace()
        );
        ExchangeValidator::compile(
            Dialect::Xdf3,
            BTreeMap::from([("test.xsd".into(), source.into_bytes())]),
        )
        .unwrap()
    }

    fn message(value: &str) -> String {
        format!(
            "<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'>{value}\
             </x:xdatenfelder.stammdatenschema.0102>",
            Dialect::Xdf3.namespace()
        )
    }

    #[test]
    fn strict_version_and_message_allowlist() {
        assert_eq!(
            inspect(message("1").as_bytes()).unwrap().kind,
            MessageKind::Schema
        );
        let source = format!(
            "<xdatenfelder.datenfeld.0104 xmlns='{}'/>",
            Dialect::Xdf2.namespace()
        );
        assert_eq!(inspect(source.as_bytes()).unwrap().kind, MessageKind::Field);
        for value in [
            message("1").replace("xdatenfelder_3.0.0", "xdatenfelder_3.1.0"),
            message("1").replace("stammdatenschema.0102", "anything.9999"),
            message("1")
                .replace(Dialect::Xdf3.namespace(), Dialect::Xdf2.namespace())
                .replace("stammdatenschema.0102", "uebersetzung.0106"),
        ] {
            assert!(matches!(
                inspect(value.as_bytes()),
                Err(FimError::UnsupportedMessage { .. })
            ));
        }
    }

    #[test]
    fn blocks_dtd_and_false_roots_without_fetching_anything() {
        for source in [
            format!(
                "<!DOCTYPE x SYSTEM 'https://not-requested.invalid/test'>{}",
                message("1")
            ),
            format!("{}<other/>", message("1")),
            format!("text{}", message("1")),
            "<!-- <xdatenfelder.stammdatenschema.0102/> -->".into(),
            "<x:xdatenfelder.stammdatenschema.0102/>".into(),
        ] {
            assert!(inspect(source.as_bytes()).is_err());
        }
        assert!(inspect(format!("<!-- <!DOCTYPE ignored> -->{}", message("1")).as_bytes()).is_ok());
    }

    #[test]
    fn rejects_truncated_and_unsupported_encoding() {
        assert!(inspect(message("1").trim_end_matches('>').as_bytes()).is_err());
        assert!(matches!(
            inspect(
                format!(
                    "<?xml version='1.0' encoding='ISO-8859-1'?>{}",
                    message("1")
                )
                .as_bytes()
            ),
            Err(FimError::Encoding(_))
        ));
        assert!(matches!(
            inspect(&[0xff, 0xfe, 0]),
            Err(FimError::Encoding(_))
        ));
    }

    #[test]
    fn rejects_non_misc_content_and_misplaced_declarations_outside_root() {
        let validator = validator();
        for source in [
            format!("<![CDATA[]]>{}", message("1")),
            format!("{}<![CDATA[ ]]> ", message("1")),
            format!("&#32;{}", message("1")),
            format!("{}&#32;", message("1")),
            format!(" <?xml version='1.0'?>{}", message("1")),
            format!("<!--comment--><?xml version='1.0'?>{}", message("1")),
            format!("<?xml version='1.0'?><?xml version='1.0'?>{}", message("1")),
            format!("{}<?xml version='1.0'?>", message("1")),
            format!("<?XmL version='1.0'?>{}", message("1")),
        ] {
            assert!(validator.validate(source.as_bytes()).is_err(), "{source}");
        }
        let valid = format!(
            "\u{feff}<?xml version='1.0' encoding='UTF-8'?>\r\n<!--comment--><?example value?>{}\n",
            message("<![CDATA[1]]>")
        );
        assert!(
            validator
                .validate(valid.as_bytes())
                .unwrap()
                .is_exchange_conformant()
        );
    }

    #[test]
    fn enforces_processing_budgets() {
        assert!(matches!(
            inspect(&vec![b' '; MAX_XML_BYTES + 1]),
            Err(FimError::Limit(_))
        ));
        let content = format!(
            "{}1{}",
            "<a>".repeat(MAX_XML_DEPTH),
            "</a>".repeat(MAX_XML_DEPTH)
        );
        assert!(matches!(
            inspect(message(&content).as_bytes()),
            Err(FimError::Limit(_))
        ));
    }

    #[test]
    fn returns_exact_input_evidence_and_actionable_errors() {
        let validator = validator();
        let valid = validator.validate(message("1").as_bytes()).unwrap();
        assert!(valid.is_exchange_conformant());
        let invalid = validator
            .validate(message("not-an-integer").as_bytes())
            .unwrap();
        assert!(!invalid.is_exchange_conformant());
        assert!(!invalid.diagnostics.is_empty());
        assert!(invalid.diagnostics.iter().any(|d| d.path.is_some()));
        assert_ne!(valid.content_sha256, invalid.content_sha256);
        assert_eq!(valid.schema_sha256, invalid.schema_sha256);
    }

    #[test]
    fn instance_schema_location_hints_do_not_change_validation() {
        let source = message("1").replacen(
            ">",
            " xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance' \
             xsi:schemaLocation='urn:not-installed https://not-requested.invalid/schema.xsd'>",
            1,
        );
        let report = validator().validate(source.as_bytes()).unwrap();
        assert!(report.is_exchange_conformant(), "{report:?}");
    }

    #[test]
    fn rejects_mismatched_and_untrusted_schema_bundles() {
        let other = message("1").replace(Dialect::Xdf3.namespace(), Dialect::Xdf2.namespace());
        assert!(matches!(
            validator().validate(other.as_bytes()),
            Err(FimError::WrongDialect)
        ));
        assert!(matches!(
            ExchangeValidator::from_official_archives(Dialect::Xdf3, b"not-official", None),
            Err(FimError::SchemaHash { .. })
        ));
    }

    #[test]
    fn caps_diagnostics_without_hiding_failure() {
        let mut sink = DiagnosticSink::default();
        for _ in 0..MAX_DIAGNOSTICS + 10 {
            sink.push(Diagnostic {
                severity: Severity::Error,
                code: "test".into(),
                message: "test failure".into(),
                path: None,
            });
        }
        assert_eq!(sink.diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(sink.omitted, 10);
        assert!(sink.has_errors);
    }
}

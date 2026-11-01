//! Typed, lossless node authoring. Full official exchange validation belongs to IO.
use super::*;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FimPrecision {
    pub text: Option<String>,
    pub min_length: Option<String>,
    pub max_length: Option<String>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FimCodeListReference {
    pub identity: Option<String>,
    /// XDF2 FIM identification version (not the Genericode version).
    pub version: Option<String>,
    pub canonical_identification: Option<String>,
    pub canonical_version_uri: Option<String>,
    pub genericode_version: Option<String>,
    pub code_key: Option<String>,
    pub name_key: Option<String>,
    pub help_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimNodeDraft {
    pub kind: FimKind,
    pub identity: Option<String>,
    pub version: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub definition: Option<String>,
    pub input_label: Option<String>,
    pub output_label: Option<String>,
    pub input_help: Option<String>,
    pub output_help: Option<String>,
    pub schema_element_type: Option<String>,
    pub field_type: Option<String>,
    pub datatype: Option<String>,
    pub cardinality: Option<String>,
    pub usage_justification: Option<String>,
    pub rule_text: Option<String>,
    pub rule_script: Option<String>,
    pub rule_type: Option<String>,
    pub precision: FimPrecision,
    pub code_list: Option<FimCodeListReference>,
    pub rules: Vec<RecordId>,
    pub group_choice: bool,
    /// Dialect-specific status code, not an inferred approval.
    pub status: Option<String>,
    pub status_list_version: Option<String>,
    pub last_changed: Option<String>,
    pub prefill: Option<String>,
    pub content: Option<String>,
    /// First legal-reference text. Further references and link attributes are preserved.
    pub legal_reference: Option<String>,
    pub max_size: Option<String>,
    pub media_types: Vec<String>,
}

impl FimNodeDraft {
    /// IDs, the editorial name and the input label are supplied by the caller.
    pub fn new(dialect: Dialect, kind: FimKind) -> Self {
        let xdf2 = dialect == Dialect::Xdf2;
        let schema_element = matches!(kind, FimKind::Field | FimKind::Group | FimKind::Choice);
        let general = kind != FimKind::CodeList && (xdf2 || kind != FimKind::Rule);
        Self {
            kind,
            identity: None,
            version: None,
            name: String::new(),
            description: (xdf2 && general).then(String::new),
            definition: (xdf2 && general).then(String::new),
            input_label: (schema_element || (xdf2 && general)).then(String::new),
            output_label: None,
            input_help: (xdf2 && schema_element).then(String::new),
            output_help: (xdf2 && schema_element).then(String::new),
            schema_element_type: schema_element.then(|| "RNG".into()),
            field_type: (kind == FimKind::Field).then(|| "input".into()),
            datatype: (kind == FimKind::Field).then(|| "text".into()),
            cardinality: schema_element.then(|| "1:1".into()),
            usage_justification: (xdf2 && schema_element).then(String::new),
            rule_text: None,
            rule_script: (xdf2 && kind == FimKind::Rule).then(String::new),
            rule_type: (!xdf2 && kind == FimKind::Rule).then(|| "V".into()),
            precision: FimPrecision {
                text: (xdf2 && kind == FimKind::Field).then(String::new),
                ..Default::default()
            },
            code_list: None,
            rules: Vec::new(),
            group_choice: kind == FimKind::Choice,
            status: general.then(|| if xdf2 { "inVorbereitung" } else { "2" }.into()),
            status_list_version: general.then(|| if xdf2 { "1.0" } else { "2022-07-12" }.into()),
            last_changed: None,
            prefill: (!xdf2 && kind == FimKind::Field).then(|| "keine".into()),
            content: (xdf2 && kind == FimKind::Field).then(String::new),
            legal_reference: (xdf2 && general).then(String::new),
            max_size: None,
            media_types: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FimNodeTarget {
    Edit {
        record: RecordId,
        independent: bool,
    },
    Insert {
        parent: RecordId,
        /// Containment position, not an XML metadata position.
        index: usize,
        /// Required for a new XDF3 group. An explicitly chosen existing F/G.
        initial_child: Option<RecordId>,
        reuse: Option<RecordId>,
    },
}

impl FimDocument {
    /// Immutable authoritative snapshot identity, independent of projection bindings.
    pub fn same_source_revision(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tree, &other.tree)
    }

    pub fn node_children(&self, record: RecordId) -> Vec<RecordId> {
        if !self.tree.records.contains_key(&record) {
            return Vec::new();
        }
        self.tree
            .children(record)
            .into_iter()
            .filter(|r| {
                self.tree.records[r].namespace == self.dialect.namespace()
                    && self.tree.records[r].local() == "struktur"
            })
            .filter_map(|r| self.tree.child(r, "enthaelt"))
            .flat_map(|r| self.tree.children(r))
            .filter(|r| {
                matches!(
                    self.kind(*r),
                    Some(FimKind::Field | FimKind::Group | FimKind::Choice)
                )
            })
            .collect()
    }

    pub fn node_parent(&self, record: RecordId) -> Option<RecordId> {
        let mut parent = self.tree.records.get(&record)?.parent;
        while let Some(id) = parent {
            if matches!(
                self.kind(id),
                Some(FimKind::Schema | FimKind::Group | FimKind::Choice | FimKind::Field)
            ) {
                return Some(id);
            }
            parent = self.tree.records.get(&id)?.parent;
        }
        None
    }

    /// Definition metadata only. Never recursively materializes contained definitions.
    pub fn node_fields(&self, record: RecordId) -> Vec<XmlField> {
        self.scoped_fields(record, false)
    }

    /// Incoming structure metadata, including attributes and legal-reference links.
    /// Multiplicity changes affect all copies of the enclosing shared definition.
    pub fn node_edge_fields(&self, record: RecordId) -> Vec<XmlField> {
        self.usage(record)
            .map(|edge| self.scoped_fields(edge, true))
            .unwrap_or_default()
    }

    fn scoped_fields(&self, record: RecordId, edge: bool) -> Vec<XmlField> {
        fn walk(
            doc: &FimDocument,
            root: RecordId,
            id: RecordId,
            path: &str,
            edge: bool,
            out: &mut Vec<XmlField>,
        ) {
            let Some(e) = doc.tree.records.get(&id) else {
                return;
            };
            if id != root
                && (doc.kind(id).is_some_and(|kind| {
                    kind != FimKind::DocumentProfile
                        || doc.tree.child(id, "identifikation").is_some()
                }) || (!edge
                    && e.namespace == doc.dialect.namespace()
                    && matches!(e.local(), "struktur" | "enthaelt")))
            {
                return;
            }
            let path = format!("{path}/{}[{}]", e.name, id.0);
            let children = doc.tree.children(id);
            let value = e
                .children
                .iter()
                .filter_map(|c| match c {
                    Content::Raw(raw) if raw.starts_with("<![CDATA[") => Some(
                        raw.strip_prefix("<![CDATA[")
                            .unwrap()
                            .strip_suffix("]]>")
                            .unwrap_or("")
                            .to_owned(),
                    ),
                    Content::Raw(raw) if !raw.starts_with('<') => Some(decode(raw)),
                    _ => None,
                })
                .collect();
            out.push(XmlField {
                record: id,
                parent: e.parent,
                path: path.clone(),
                name: e.name.clone(),
                namespace: e.namespace.clone(),
                value,
                attributes: e.attributes.clone(),
                has_elements: !children.is_empty(),
            });
            for child in children {
                walk(doc, root, child, &path, edge, out);
            }
        }
        let mut out = Vec::new();
        walk(self, record, record, "", edge, &mut out);
        out
    }

    pub fn node_draft(&self, record: RecordId) -> Result<FimNodeDraft, FimModelError> {
        let kind = self
            .kind(record)
            .ok_or_else(|| xml_error("record is not an authorable FIM node"))?;
        let text = |name| self.tree.child_text(record, name);
        let code_value = |record| {
            self.tree
                .children(record)
                .into_iter()
                .find(|r| {
                    self.tree.records[r].local() == "code"
                        && self.tree.records[r].namespace.is_empty()
                })
                .map(|r| self.tree.text(r))
                .unwrap_or_else(|| self.tree.text(record))
        };
        let code = |name| self.tree.child(record, name).map(code_value);
        let identification = self.tree.child(record, "identifikation");
        let usage = self.usage(record);
        let precision = self.tree.child(record, "praezisierung");
        let attr = |name| {
            precision.and_then(|r| {
                self.tree.records[&r]
                    .attributes
                    .iter()
                    .find(|a| a.name == name)
                    .map(|a| a.value.clone())
            })
        };
        let xdf2 = self.dialect == Dialect::Xdf2;
        let status = self
            .tree
            .child(record, if xdf2 { "status" } else { "freigabestatus" });
        let reference = if self.tree.records[&record].local() == "codelisteReferenz" {
            Some(record)
        } else {
            self.tree.child(record, "codelisteReferenz")
        };
        Ok(FimNodeDraft {
            kind,
            identity: identification.and_then(|r| self.tree.child_text(r, "id")),
            version: identification.and_then(|r| self.tree.child_text(r, "version")),
            name: text("name").unwrap_or_default(),
            description: text("beschreibung"),
            definition: text("definition"),
            input_label: text("bezeichnungEingabe"),
            output_label: text("bezeichnungAusgabe"),
            input_help: text("hilfetextEingabe"),
            output_help: text("hilfetextAusgabe"),
            schema_element_type: code("schemaelementart"),
            field_type: code("feldart"),
            datatype: code("datentyp"),
            cardinality: usage.and_then(|r| self.tree.child_text(r, "anzahl")),
            usage_justification: usage.and_then(|r| self.tree.child_text(r, "bezug")),
            rule_text: text("freitextRegel"),
            rule_script: text(if xdf2 { "script" } else { "skript" }),
            rule_type: code("typ"),
            precision: FimPrecision {
                text: precision.map(|r| self.tree.text(r)),
                min_length: attr("minLength"),
                max_length: attr("maxLength"),
                min_value: attr("minValue"),
                max_value: attr("maxValue"),
                pattern: attr("pattern"),
            },
            code_list: reference.map(|r| {
                self.read_code_list(
                    r,
                    if r == record {
                        self.tree.records[&record].parent.unwrap_or(record)
                    } else {
                        record
                    },
                )
            }),
            rules: self
                .tree
                .children(record)
                .into_iter()
                .filter(|r| self.kind(*r) == Some(FimKind::Rule))
                .collect(),
            group_choice: kind == FimKind::Choice,
            status: status.map(code_value),
            status_list_version: status.and_then(|r| {
                self.tree.records[&r]
                    .attributes
                    .iter()
                    .find(|a| a.name == "listVersionID")
                    .map(|a| a.value.clone())
            }),
            last_changed: text("letzteAenderung"),
            prefill: code("vorbefuellung"),
            content: text("inhalt"),
            legal_reference: text("bezug"),
            max_size: text("maxSize"),
            media_types: self
                .tree
                .children(record)
                .into_iter()
                .filter(|r| {
                    self.tree.records[r].namespace == self.dialect.namespace()
                        && self.tree.records[r].local() == "mediaType"
                })
                .map(|r| self.tree.text(r))
                .collect(),
        })
    }

    fn read_code_list(&self, reference: RecordId, field: RecordId) -> FimCodeListReference {
        let identification = self.tree.child(reference, "identifikation");
        let genericode = self
            .tree
            .child(reference, "genericodeIdentification")
            .unwrap_or(reference);
        FimCodeListReference {
            identity: identification.and_then(|r| self.tree.child_text(r, "id")),
            version: identification.and_then(|r| self.tree.child_text(r, "version")),
            canonical_identification: self.tree.child_text(genericode, "canonicalIdentification"),
            canonical_version_uri: self.tree.child_text(genericode, "canonicalVersionUri"),
            genericode_version: self.tree.child_text(genericode, "version"),
            code_key: self.tree.child_text(field, "codeKey"),
            name_key: self.tree.child_text(field, "nameKey"),
            help_key: self.tree.child_text(field, "helpKey"),
        }
    }

    /// Corresponding occurrences in copies of the owning definition, not all
    /// uses of the contained/associated object's own identity.
    pub(super) fn occurrence_records(
        &self,
        record: RecordId,
    ) -> Result<Vec<RecordId>, FimModelError> {
        let element = self
            .tree
            .records
            .get(&record)
            .ok_or_else(|| xml_error("record missing"))?;
        let Some(parent) = element.parent else {
            return Ok(vec![record]);
        };
        let position = self
            .tree
            .children(parent)
            .iter()
            .position(|r| *r == record)
            .ok_or_else(|| xml_error("node parent mismatch"))?;
        self.shared_records(parent)?
            .into_iter()
            .map(|parent| {
                self.tree
                    .children(parent)
                    .get(position)
                    .copied()
                    .ok_or_else(|| xml_error("shared parent child mismatch"))
            })
            .collect()
    }

    pub(super) fn author_node(
        &mut self,
        target: FimNodeTarget,
        draft: &FimNodeDraft,
        timestamp: &str,
    ) -> Result<(), FimModelError> {
        // XDF2 C exposes its identification on both the General and reference pages.
        // XDF3 C has only a Genericode natural key.
        let mut normalized = draft.clone();
        if draft.kind == FimKind::CodeList {
            if self.dialect == Dialect::Xdf3
                && (draft.identity.is_some() || draft.version.is_some())
            {
                return Err(FimModelError::Integrity(
                    "XDF3 C references have no FIM identity/version".into(),
                ));
            }
            if self.dialect == Dialect::Xdf2
                && let Some(reference) = &mut normalized.code_list
            {
                let old = match &target {
                    FimNodeTarget::Edit { record, .. } => Some(self.node_draft(*record)?),
                    _ => None,
                };
                if old.as_ref().is_some_and(|o| o.identity != draft.identity)
                    || (old.is_none() && draft.identity.is_some())
                {
                    reference.identity = draft.identity.clone()
                }
                if old.as_ref().is_some_and(|o| o.version != draft.version)
                    || (old.is_none() && draft.version.is_some())
                {
                    reference.version = draft.version.clone()
                }
                normalized.identity = reference.identity.clone();
                normalized.version = reference.version.clone();
            }
        }
        let draft = &normalized;
        match target {
            FimNodeTarget::Edit {
                record,
                independent,
            } => {
                let old = self.node_draft(record)?;
                if old == *draft && !independent {
                    return Ok(());
                }
                if draft.kind != old.kind {
                    return Err(FimModelError::Integrity("node kind conversion requires an explicit migration; group choice uses group_choice".into()));
                }
                let targets = if independent {
                    // Detach the occurrence, but keep copies of its shared parent coherent.
                    self.occurrence_records(record)?
                } else {
                    self.equivalent(record)?
                };
                self.validate_draft(draft, Some(&old), independent, &targets)?;
                let unchanged_identity = if draft.kind == FimKind::CodeList {
                    if self.dialect == Dialect::Xdf2 {
                        draft.identity == old.identity && draft.version == old.version
                    } else {
                        draft.code_list.as_ref().map(|r| {
                            (
                                &r.canonical_identification,
                                &r.genericode_version,
                                &r.canonical_version_uri,
                            )
                        }) == old.code_list.as_ref().map(|r| {
                            (
                                &r.canonical_identification,
                                &r.genericode_version,
                                &r.canonical_version_uri,
                            )
                        })
                    }
                } else {
                    draft.identity == old.identity && draft.version == old.version
                };
                if independent && unchanged_identity {
                    return Err(FimModelError::DecisionRequired(
                        "detach requires a different assigned identity/version or Genericode natural key".into(),
                    ));
                }
                let edges = if draft.cardinality != old.cardinality
                    || draft.usage_justification != old.usage_justification
                {
                    let edge = self.usage(record).ok_or_else(|| {
                        FimModelError::Integrity(
                            "standalone/associated nodes have no incoming multiplicity".into(),
                        )
                    })?;
                    self.shared_records(edge)?
                } else {
                    Vec::new()
                };
                self.validate_associations(draft, &old, record)?;
                let tree = Arc::make_mut(&mut self.tree);
                for target in targets {
                    update_node(tree, self.dialect, target, draft, Some(&old), timestamp)?;
                }
                for edge in edges {
                    update_property(
                        tree,
                        self.dialect,
                        edge,
                        "anzahl",
                        draft.cardinality.as_deref(),
                        false,
                    )?;
                    update_property(
                        tree,
                        self.dialect,
                        edge,
                        "bezug",
                        draft.usage_justification.as_deref(),
                        false,
                    )?;
                }
            }
            FimNodeTarget::Insert {
                parent,
                index,
                initial_child,
                reuse,
            } => {
                let parent_kind = self
                    .kind(parent)
                    .ok_or_else(|| xml_error("parent is not a FIM node"))?;
                let associated = matches!(draft.kind, FimKind::Rule | FimKind::CodeList);
                if associated {
                    if !matches!(
                        parent_kind,
                        FimKind::Schema | FimKind::Group | FimKind::Choice | FimKind::Field
                    ) || (draft.kind == FimKind::CodeList && parent_kind != FimKind::Field)
                    {
                        return Err(FimModelError::Integrity(
                            "rules attach to S/G/F; code-list references attach only to F".into(),
                        ));
                    }
                    if draft.kind == FimKind::CodeList
                        && self.tree.child(parent, "codelisteReferenz").is_some()
                    {
                        return Err(FimModelError::DecisionRequired(
                            "field already has a code-list reference; edit it explicitly".into(),
                        ));
                    }
                } else if !matches!(
                    parent_kind,
                    FimKind::Schema | FimKind::Group | FimKind::Choice
                ) || !matches!(
                    draft.kind,
                    FimKind::Group | FimKind::Choice | FimKind::Field
                ) {
                    return Err(FimModelError::Integrity("only S/G contain F/G".into()));
                }
                if index > self.node_children(parent).len() {
                    return Err(FimModelError::Integrity(
                        "containment insertion index is out of range".into(),
                    ));
                }
                let parents = self.equivalent(parent)?;
                let reuse_draft = reuse.map(|r| self.node_draft(r)).transpose()?;
                if let Some(source) = reuse {
                    self.equivalent(source)?;
                    if self.kind(source) != Some(draft.kind) {
                        return Err(FimModelError::Integrity(
                            "reuse kind does not match the draft".into(),
                        ));
                    }
                    self.check_cycle(source, parent)?;
                    let mut source_draft = reuse_draft.clone().unwrap();
                    source_draft.cardinality = draft.cardinality.clone();
                    source_draft.usage_justification = draft.usage_justification.clone();
                    if source_draft != *draft {
                        return Err(FimModelError::DecisionRequired(
                            "reuse retains the source definition; edit it separately or detach"
                                .into(),
                        ));
                    }
                } else {
                    self.validate_draft(draft, None, true, &[])?;
                }
                if let Some(child) = initial_child {
                    if !matches!(draft.kind, FimKind::Group | FimKind::Choice)
                        || !matches!(
                            self.kind(child),
                            Some(FimKind::Group | FimKind::Choice | FimKind::Field)
                        )
                    {
                        return Err(FimModelError::Integrity(
                            "initial child must be an existing F/G for a new group".into(),
                        ));
                    }
                    self.equivalent(child)?;
                    self.check_cycle(child, parent)?;
                } else if self.dialect == Dialect::Xdf3
                    && reuse.is_none()
                    && matches!(draft.kind, FimKind::Group | FimKind::Choice)
                {
                    return Err(FimModelError::DecisionRequired(
                        "new XDF3 groups require an explicitly chosen existing F/G child".into(),
                    ));
                }
                let empty = FimNodeDraft::new(self.dialect, draft.kind);
                self.validate_associations(draft, &empty, parent)?;
                let tree = Arc::make_mut(&mut self.tree);
                for parent in parents {
                    let container = if associated {
                        parent
                    } else {
                        insert_edge(
                            tree,
                            self.dialect,
                            parent,
                            index,
                            draft.cardinality.as_deref(),
                            draft.usage_justification.as_deref(),
                        )?
                    };
                    let record = if let Some(source) = reuse {
                        attach_clone(
                            tree,
                            source,
                            container,
                            node_anchor(tree, self.dialect, container, node_local(draft.kind)?),
                        )?
                    } else {
                        let local = node_local(draft.kind)?;
                        let before = node_anchor(tree, self.dialect, container, local);
                        let record =
                            tree.insert(container, before, local, self.dialect.namespace(), None)?;
                        update_node(tree, self.dialect, record, draft, None, timestamp)?;
                        if let Some(child) = initial_child {
                            let container = insert_edge(
                                tree,
                                self.dialect,
                                record,
                                0,
                                Some("1:1"),
                                (self.dialect == Dialect::Xdf2).then_some(""),
                            )?;
                            attach_clone(tree, child, container, None)?;
                        }
                        record
                    };
                    if draft.kind == FimKind::CodeList && reuse.is_none() {
                        // XDF3 column keys are siblings of codelisteReferenz, not inside it.
                        update_code_keys(tree, self.dialect, parent, draft.code_list.as_ref())?;
                    }
                    let _ = record;
                }
            }
        }
        Ok(())
    }

    fn check_cycle(&self, source: RecordId, parent: RecordId) -> Result<(), FimModelError> {
        let mut current = Some(parent);
        while let Some(id) = current {
            if id == source || (self.key(source).is_some() && self.key(id) == self.key(source)) {
                return Err(FimModelError::Integrity(
                    "recursive containment reuse is prohibited".into(),
                ));
            }
            current = self.tree.records[&id].parent;
        }
        Ok(())
    }

    fn validate_associations(
        &self,
        draft: &FimNodeDraft,
        old: &FimNodeDraft,
        owner: RecordId,
    ) -> Result<(), FimModelError> {
        let mut seen = BTreeSet::new();
        for rule in &draft.rules {
            if self.kind(*rule) != Some(FimKind::Rule) || !seen.insert(*rule) {
                return Err(FimModelError::Integrity(
                    "rules must be distinct existing R records".into(),
                ));
            }
            self.equivalent(*rule)?;
            if self.fields(*rule).iter().any(|f| f.record == owner) {
                return Err(FimModelError::Integrity(
                    "recursive rule association".into(),
                ));
            }
        }
        for removed in old.rules.iter().filter(|r| !draft.rules.contains(r)) {
            self.check_removal(*removed)?;
        }
        Ok(())
    }

    fn validate_draft(
        &self,
        draft: &FimNodeDraft,
        old: Option<&FimNodeDraft>,
        identity_required: bool,
        targets: &[RecordId],
    ) -> Result<(), FimModelError> {
        let changed =
            |current: &Option<String>, previous: Option<&Option<String>>| previous != Some(current);
        if !matches!(
            draft.kind,
            FimKind::Field
                | FimKind::Group
                | FimKind::Choice
                | FimKind::Rule
                | FimKind::CodeList
                | FimKind::Schema
        ) {
            return Err(FimModelError::DecisionRequired(
                "this node requires a separate envelope/translation workflow".into(),
            ));
        }
        if draft.kind != FimKind::CodeList
            && (old.is_none() || old.is_some_and(|o| o.name != draft.name))
            && draft.name.trim().is_empty()
        {
            return Err(FimModelError::Integrity("supply an editorial name".into()));
        }
        if (identity_required
            || changed(&draft.identity, old.map(|o| &o.identity))
            || changed(&draft.version, old.map(|o| &o.version)))
            && draft.kind != FimKind::CodeList
        {
            validate_identity(draft.kind, draft.identity.as_deref())?;
            validate_version(self.dialect, draft.version.as_deref())?;
            self.unique_identity(draft.identity.as_deref(), draft.version.as_deref(), targets)?;
        }
        let validate_code = |value: &Option<String>,
                             before: Option<&Option<String>>,
                             allowed: &[&str],
                             field: &str|
         -> Result<(), FimModelError> {
            if changed(value, before) && value.as_deref().is_none_or(|v| !allowed.contains(&v)) {
                return Err(FimModelError::Integrity(format!(
                    "invalid or missing {field} code"
                )));
            }
            Ok(())
        };
        let xdf2 = self.dialect == Dialect::Xdf2;
        let fg = matches!(
            draft.kind,
            FimKind::Field | FimKind::Group | FimKind::Choice
        );
        if fg {
            validate_code(
                &draft.schema_element_type,
                old.map(|o| &o.schema_element_type),
                &["ABS", "HAR", "RNG"],
                "schemaelementart",
            )?;
            if old.is_none()
                && draft
                    .input_label
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty())
            {
                return Err(FimModelError::Integrity(
                    "supply the new node's input label".into(),
                ));
            }
            if xdf2 && draft.group_choice && old.is_none_or(|o| !o.group_choice) {
                return Err(FimModelError::Integrity(
                    "XDF2 has no choice group type".into(),
                ));
            }
        }
        if draft.kind == FimKind::Field {
            validate_code(
                &draft.field_type,
                old.map(|o| &o.field_type),
                if xdf2 {
                    &["input", "label", "select"]
                } else {
                    &["input", "label", "select", "hidden", "locked"]
                },
                "feldart",
            )?;
            validate_code(
                &draft.datatype,
                old.map(|o| &o.datatype),
                if xdf2 {
                    &[
                        "bool",
                        "date",
                        "file",
                        "num",
                        "num_currency",
                        "num_int",
                        "obj",
                        "text",
                    ]
                } else {
                    &[
                        "bool",
                        "date",
                        "datetime",
                        "file",
                        "num",
                        "num_currency",
                        "num_int",
                        "obj",
                        "text",
                        "text_latin",
                        "time",
                    ]
                },
                "datentyp",
            )?;
            if !xdf2 {
                validate_code(
                    &draft.prefill,
                    old.map(|o| &o.prefill),
                    &["keine", "optional", "verpflichtend"],
                    "vorbefuellung",
                )?
            }
            if old.is_none_or(|o| o.precision != draft.precision || o.datatype != draft.datatype) {
                validate_precision(self.dialect, &draft.precision)?;
                if matches!(
                    draft.datatype.as_deref(),
                    Some("num" | "num_int" | "num_currency")
                ) && let (Some(min), Some(max)) =
                    (&draft.precision.min_value, &draft.precision.max_value)
                    && decimal_compare(min, max) == Some(std::cmp::Ordering::Greater)
                {
                    return Err(FimModelError::Integrity(
                        "precision minValue exceeds maxValue".into(),
                    ));
                }
            }
            if (old.is_none()
                || old.is_some_and(|o| {
                    o.field_type != draft.field_type || o.datatype != draft.datatype
                }))
                && draft.field_type.as_deref() == Some("select")
                && !matches!(draft.datatype.as_deref(), Some("text" | "text_latin"))
            {
                return Err(FimModelError::Integrity(
                    "select fields require a text datatype".into(),
                ));
            }
            if changed(&draft.max_size, old.map(|o| &o.max_size))
                && let Some(value) = &draft.max_size
                && (xdf2
                    || value.is_empty()
                    || !value.bytes().all(|b| b.is_ascii_digit())
                    || value.bytes().all(|b| b == b'0'))
            {
                return Err(FimModelError::Integrity(
                    "maxSize requires an XDF3 positive decimal integer".into(),
                ));
            }
            if old.is_none_or(|o| o.media_types != draft.media_types) {
                for value in &draft.media_types {
                    let valid = value.split_once('/').is_some_and(|(a, b)| {
                        !a.is_empty()
                            && !b.is_empty()
                            && a.chars().all(|c| c.is_alphanumeric() || c == '_')
                            && b.chars()
                                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '+' | '.'))
                    });
                    if xdf2 || !valid {
                        return Err(FimModelError::Integrity(
                            "mediaType must match the XDF3 type/subtype pattern".into(),
                        ));
                    }
                }
            }
        }
        if draft.kind == FimKind::Rule && !xdf2 {
            validate_code(
                &draft.rule_type,
                old.map(|o| &o.rule_type),
                &["B", "K", "M", "V"],
                "regeltyp",
            )?;
            if changed(&draft.definition, old.map(|o| &o.definition)) && draft.definition.is_some()
            {
                return Err(FimModelError::Integrity(
                    "XDF3 rules have freitextRegel, not definition".into(),
                ));
            }
        }
        if changed(&draft.cardinality, old.map(|o| &o.cardinality)) {
            if let Some(value) = &draft.cardinality {
                Cardinality::parse(value).map_err(|e| FimModelError::Integrity(e.to_string()))?;
            } else if old.is_some_and(|o| o.cardinality.is_some()) || (old.is_none() && fg) {
                return Err(FimModelError::Integrity(
                    "containment requires multiplicity".into(),
                ));
            }
        }
        if changed(&draft.status, old.map(|o| &o.status)) && xdf2 && draft.kind != FimKind::CodeList
        {
            validate_code(
                &draft.status,
                old.map(|o| &o.status),
                &["aktiv", "inaktiv", "inVorbereitung"],
                "status",
            )?;
        }
        let required = |value: &Option<String>,
                        before: Option<&Option<String>>,
                        field: &str|
         -> Result<(), FimModelError> {
            if changed(value, before) && value.is_none() {
                return Err(FimModelError::Integrity(format!(
                    "required {field} cannot be removed"
                )));
            }
            Ok(())
        };
        if draft.kind != FimKind::CodeList {
            if xdf2 {
                required(
                    &draft.description,
                    old.map(|o| &o.description),
                    "beschreibung",
                )?;
                required(&draft.definition, old.map(|o| &o.definition), "definition")?;
                required(
                    &draft.legal_reference,
                    old.map(|o| &o.legal_reference),
                    "bezug",
                )?;
                required(
                    &draft.input_label,
                    old.map(|o| &o.input_label),
                    "bezeichnungEingabe",
                )?;
                if fg {
                    required(
                        &draft.input_help,
                        old.map(|o| &o.input_help),
                        "hilfetextEingabe",
                    )?;
                    required(
                        &draft.output_help,
                        old.map(|o| &o.output_help),
                        "hilfetextAusgabe",
                    )?;
                }
                if draft.kind == FimKind::Field {
                    required(
                        &draft.precision.text,
                        old.map(|o| &o.precision.text),
                        "praezisierung",
                    )?;
                    required(&draft.content, old.map(|o| &o.content), "inhalt")?;
                }
                if draft.kind == FimKind::Rule {
                    required(&draft.rule_script, old.map(|o| &o.rule_script), "script")?
                }
            } else if fg {
                required(
                    &draft.input_label,
                    old.map(|o| &o.input_label),
                    "bezeichnungEingabe",
                )?
            }
            if xdf2 || draft.kind != FimKind::Rule {
                required(
                    &draft.status,
                    old.map(|o| &o.status),
                    "status/freigabestatus",
                )?;
                if changed(
                    &draft.status_list_version,
                    old.map(|o| &o.status_list_version),
                ) && (draft
                    .status_list_version
                    .as_deref()
                    .is_none_or(|v| v.is_empty())
                    || (xdf2 && draft.status_list_version.as_deref() != Some("1.0")))
                {
                    return Err(FimModelError::Integrity(
                        "status listVersionID is required (XDF2 fixed 1.0)".into(),
                    ));
                }
            }
        }
        if old.is_none_or(|o| o.code_list != draft.code_list) {
            if let Some(reference) = &draft.code_list {
                if !matches!(draft.kind, FimKind::Field | FimKind::CodeList) {
                    return Err(FimModelError::Integrity(
                        "code-list references attach only to fields".into(),
                    ));
                }
                if xdf2 {
                    validate_identity(FimKind::CodeList, reference.identity.as_deref())?;
                    validate_version(self.dialect, reference.version.as_deref())?;
                    if reference.code_key.is_some()
                        || reference.name_key.is_some()
                        || reference.help_key.is_some()
                    {
                        return Err(FimModelError::Integrity(
                            "XDF2 code-list references have no code/name/help column keys".into(),
                        ));
                    }
                    if [
                        reference.canonical_identification.as_deref(),
                        reference.genericode_version.as_deref(),
                        reference.canonical_version_uri.as_deref(),
                    ]
                    .iter()
                    .any(|v| v.is_none_or(|s| s.trim().is_empty()))
                    {
                        return Err(FimModelError::Integrity(
                            "XDF2 requires all three Genericode identification components".into(),
                        ));
                    }
                } else if reference.identity.is_some() || reference.version.is_some() {
                    return Err(FimModelError::Integrity(
                        "XDF3 code-list references use Genericode identity, not a FIM C ID".into(),
                    ));
                } else if reference.code_key.is_some() != reference.name_key.is_some()
                    || (reference.help_key.is_some() && reference.code_key.is_none())
                    || [
                        reference.code_key.as_deref(),
                        reference.name_key.as_deref(),
                        reference.help_key.as_deref(),
                    ]
                    .iter()
                    .flatten()
                    .any(|v| v.trim().is_empty())
                {
                    return Err(FimModelError::Integrity(
                        "codeKey/nameKey must be paired nonempty keys; helpKey requires that pair"
                            .into(),
                    ));
                } else if reference
                    .canonical_identification
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty())
                    && reference
                        .canonical_version_uri
                        .as_deref()
                        .is_none_or(|s| s.trim().is_empty())
                {
                    return Err(FimModelError::Integrity(
                        "supply a canonical Genericode identification or version URI".into(),
                    ));
                }
                if draft.kind == FimKind::CodeList
                    && let Some(old) = old
                {
                    let mut intended = old.clone();
                    intended.identity = draft.identity.clone();
                    intended.version = draft.version.clone();
                    intended.code_list = draft.code_list.clone();
                    if intended != *draft {
                        return Err(FimModelError::Integrity("a code-list reference has identity/components/column keys, not general F/G/R properties".into()));
                    }
                    if identity_required && self.dialect == Dialect::Xdf2 {
                        self.unique_identity(
                            draft.identity.as_deref(),
                            draft.version.as_deref(),
                            targets,
                        )?;
                    }
                }
            } else if draft.kind == FimKind::CodeList {
                return Err(FimModelError::Integrity(
                    "supply a code-list reference".into(),
                ));
            }
        }
        Ok(())
    }

    fn unique_identity(
        &self,
        identity: Option<&str>,
        version: Option<&str>,
        targets: &[RecordId],
    ) -> Result<(), FimModelError> {
        if self
            .metadata_index()
            .identities
            .iter()
            .any(|(key, records)| {
                Some(key.1.as_str()) == identity
                    && (version.is_none() || key.2.as_deref() == version)
                    && records.iter().any(|r| !targets.contains(r))
            })
        {
            return Err(FimModelError::DecisionRequired(
                "assigned identity/version already exists; use Reuse or another assigned identity"
                    .into(),
            ));
        }
        Ok(())
    }
}

fn validate_identity(kind: FimKind, identity: Option<&str>) -> Result<(), FimModelError> {
    let prefix = match kind {
        FimKind::Field => "F",
        FimKind::Group | FimKind::Choice => "G",
        FimKind::Rule => "R",
        FimKind::CodeList => "C",
        FimKind::Schema => "S",
        _ => "",
    };
    if identity
        .and_then(|s| s.strip_prefix(prefix))
        .is_none_or(|suffix| suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(FimModelError::Integrity("supply a kind-correct assigned registry identity; IDs are never allocated by the editor".into()));
    }
    Ok(())
}
fn validate_version(dialect: Dialect, value: Option<&str>) -> Result<(), FimModelError> {
    if let Some(value) = value {
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != if dialect == Dialect::Xdf2 { 2 } else { 3 }
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(FimModelError::Integrity(
                "version must match the dialect's decimal version components".into(),
            ));
        }
    }
    Ok(())
}
fn validate_precision(dialect: Dialect, p: &FimPrecision) -> Result<(), FimModelError> {
    if dialect == Dialect::Xdf2
        && [
            p.min_length.as_ref(),
            p.max_length.as_ref(),
            p.min_value.as_ref(),
            p.max_value.as_ref(),
            p.pattern.as_ref(),
        ]
        .iter()
        .any(|v| v.is_some())
    {
        return Err(FimModelError::Integrity(
            "XDF2 precision is scalar text".into(),
        ));
    }
    for (value, positive) in [(&p.min_length, false), (&p.max_length, true)] {
        if let Some(value) = value
            && (value.is_empty()
                || !value.bytes().all(|b| b.is_ascii_digit())
                || (positive && value.bytes().all(|b| b == b'0')))
        {
            return Err(FimModelError::Integrity(
                "precision length must be a decimal integer (maxLength positive)".into(),
            ));
        }
    }
    if let (Some(min), Some(max)) = (&p.min_length, &p.max_length) {
        let min = min.trim_start_matches('0');
        let max = max.trim_start_matches('0');
        if (min.len(), min) > (max.len(), max) {
            return Err(FimModelError::Integrity(
                "precision minLength exceeds maxLength".into(),
            ));
        }
    }
    // Value bounds and patterns are opaque lexical constraints, not executable expressions.
    Ok(())
}

/// Compare exact decimal bounds without rounding or normalizing retained XML.
/// Other lexical bounds remain opaque; neither value nor pattern is executed.
fn decimal_compare(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    fn parts(value: &str) -> Option<(bool, &str, &str)> {
        let negative = value.starts_with('-');
        let value = value.strip_prefix(['-', '+']).unwrap_or(value);
        let (integer, fraction) = value.split_once('.').unwrap_or((value, ""));
        if (integer.is_empty() && fraction.is_empty())
            || !integer
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let integer = integer.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        Some((
            negative && (!integer.is_empty() || !fraction.is_empty()),
            integer,
            fraction,
        ))
    }
    let (left_negative, left_integer, left_fraction) = parts(left)?;
    let (right_negative, right_integer, right_fraction) = parts(right)?;
    if left_negative != right_negative {
        return Some(right_negative.cmp(&left_negative));
    }
    let magnitude = (left_integer.len(), left_integer)
        .cmp(&(right_integer.len(), right_integer))
        .then_with(|| {
            let length = left_fraction.len().max(right_fraction.len());
            left_fraction
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(length)
                .cmp(
                    right_fraction
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(length),
                )
        });
    Some(if left_negative {
        magnitude.reverse()
    } else {
        magnitude
    })
}

fn node_local(kind: FimKind) -> Result<&'static str, FimModelError> {
    match kind {
        FimKind::Field => Ok("datenfeld"),
        FimKind::Group | FimKind::Choice => Ok("datenfeldgruppe"),
        FimKind::Rule => Ok("regel"),
        FimKind::CodeList => Ok("codelisteReferenz"),
        _ => Err(FimModelError::DecisionRequired(
            "new node requires a dedicated envelope workflow".into(),
        )),
    }
}

/// Known slots only: opaque siblings retain their original order and bytes.
fn order(dialect: Dialect, local: &str) -> Vec<&'static str> {
    if local == "identifikation" {
        return vec!["id", "version"];
    }
    if local == "genericodeIdentification"
        || (local == "codelisteReferenz" && dialect == Dialect::Xdf3)
    {
        return vec!["canonicalIdentification", "version", "canonicalVersionUri"];
    }
    if local == "codelisteReferenz" {
        return vec!["identifikation", "genericodeIdentification"];
    }
    if local == "struktur" {
        return vec!["anzahl", "bezug", "enthaelt"];
    }
    if local == "regel" && dialect == Dialect::Xdf3 {
        return vec![
            "identifikation",
            "name",
            "beschreibung",
            "freitextRegel",
            "bezug",
            "stichwort",
            "fachlicherErsteller",
            "letzteAenderung",
            "typ",
            "param",
            "ziel",
            "skript",
            "fehler",
        ];
    }
    let mut slots = if dialect == Dialect::Xdf2 {
        vec![
            "identifikation",
            "name",
            "bezeichnungEingabe",
            "bezeichnungAusgabe",
            "beschreibung",
            "definition",
            "bezug",
            "status",
            "gueltigAb",
            "gueltigBis",
            "fachlicherErsteller",
            "versionshinweis",
            "freigabedatum",
            "veroeffentlichungsdatum",
        ]
    } else {
        vec![
            "identifikation",
            "name",
            "beschreibung",
            "definition",
            "bezug",
            "freigabestatus",
            "statusGesetztAm",
            "statusGesetztDurch",
            "gueltigAb",
            "gueltigBis",
            "versionshinweis",
            "veroeffentlichungsdatum",
            "letzteAenderung",
            "relation",
            "stichwort",
            "bezeichnungEingabe",
            "bezeichnungAusgabe",
        ]
    };
    if local == "regel" {
        slots.push("script");
        return slots;
    }
    if local == "stammdatenschema" {
        slots.extend([
            "bezeichnung",
            "hilfetext",
            "ableitungsmodifikationenStruktur",
            "ableitungsmodifikationenRepraesentation",
            "dokumentsteckbrief",
            "regel",
            "struktur",
        ]);
        return slots;
    }
    slots.extend(["schemaelementart", "hilfetextEingabe", "hilfetextAusgabe"]);
    if local == "datenfeldgruppe" {
        slots.extend(["art", "regel", "struktur"])
    } else if local == "datenfeld" {
        slots.extend([
            "feldart",
            "datentyp",
            "praezisierung",
            "inhalt",
            "vorbefuellung",
            "werte",
            "codelisteReferenz",
            "codeKey",
            "nameKey",
            "helpKey",
            "regel",
            "maxSize",
            "mediaType",
        ])
    }
    slots
}
fn node_anchor(tree: &XmlTree, dialect: Dialect, parent: RecordId, name: &str) -> Option<RecordId> {
    let slots = order(dialect, tree.records[&parent].local());
    let position = slots.iter().position(|s| *s == name)?;
    tree.children(parent).into_iter().find(|r| {
        let e = &tree.records[r];
        e.namespace == dialect.namespace()
            && slots
                .iter()
                .position(|s| *s == e.local())
                .is_some_and(|p| p > position)
    })
}
fn update_property(
    tree: &mut XmlTree,
    dialect: Dialect,
    parent: RecordId,
    name: &str,
    value: Option<&str>,
    code: bool,
) -> Result<(), FimModelError> {
    let existing = tree.child(parent, name);
    match (existing, value) {
        (Some(record), None) => tree.remove(record)?,
        (Some(record), Some(value)) if tree.text(record) == value => {}
        (Some(record), Some(value)) if code => {
            let child = tree
                .children(record)
                .into_iter()
                .find(|r| tree.records[r].local() == "code" && tree.records[r].namespace.is_empty())
                .ok_or_else(|| {
                    FimModelError::DecisionRequired(format!(
                        "repair malformed {name} code through scoped XML fields"
                    ))
                })?;
            tree.set_text(child, value)?;
        }
        (Some(record), Some(value)) => tree.set_text(record, value)?,
        (None, Some(value)) => {
            let record = tree.insert(
                parent,
                node_anchor(tree, dialect, parent, name),
                name,
                dialect.namespace(),
                (!code).then_some(value),
            )?;
            if code {
                tree.insert(record, None, "code", "", Some(value))?;
            }
        }
        (None, None) => {}
    }
    Ok(())
}
fn attach_clone(
    tree: &mut XmlTree,
    source: RecordId,
    parent: RecordId,
    before: Option<RecordId>,
) -> Result<RecordId, FimModelError> {
    // A reused subtree can depend on prefixes declared above its original root.
    // Carry those declarations to the clone without rewriting the source.
    let mut inherited = BTreeMap::new();
    let mut ancestor = Some(source);
    while let Some(record) = ancestor {
        let element = tree
            .records
            .get(&record)
            .ok_or_else(|| xml_error("reuse source missing"))?;
        for attribute in &element.attributes {
            if attribute.name == "xmlns" || attribute.name.starts_with("xmlns:") {
                inherited
                    .entry(attribute.name.clone())
                    .or_insert_with(|| attribute.value.clone());
            }
        }
        ancestor = element.parent;
    }
    let cloned = tree.clone_into(source, parent);
    for (name, value) in inherited {
        if !tree.records[&cloned]
            .attributes
            .iter()
            .any(|a| a.name == name)
        {
            tree.set_attribute(cloned, &name, Some(&value))?;
        }
    }
    tree.ensure_open(parent);
    let e = tree.records.get_mut(&parent).unwrap();
    let position = before
        .and_then(|r| e.children.iter().position(|c| *c == Content::Element(r)))
        .unwrap_or(e.children.len());
    e.children.insert(position, Content::Element(cloned));
    Ok(cloned)
}
fn insert_edge(
    tree: &mut XmlTree,
    dialect: Dialect,
    parent: RecordId,
    index: usize,
    cardinality: Option<&str>,
    justification: Option<&str>,
) -> Result<RecordId, FimModelError> {
    let edges: Vec<_> = tree
        .children(parent)
        .into_iter()
        .filter(|r| {
            tree.records[r].local() == "struktur"
                && tree.records[r].namespace == dialect.namespace()
        })
        .collect();
    let edge = tree.insert(
        parent,
        edges
            .get(index)
            .copied()
            .or_else(|| node_anchor(tree, dialect, parent, "struktur")),
        "struktur",
        dialect.namespace(),
        None,
    )?;
    update_property(
        tree,
        dialect,
        edge,
        "anzahl",
        cardinality.or(Some("1:1")),
        false,
    )?;
    update_property(
        tree,
        dialect,
        edge,
        "bezug",
        justification.or_else(|| (dialect == Dialect::Xdf2).then_some("")),
        false,
    )?;
    tree.insert(edge, None, "enthaelt", dialect.namespace(), None)
}

fn update_node(
    tree: &mut XmlTree,
    dialect: Dialect,
    record: RecordId,
    draft: &FimNodeDraft,
    old: Option<&FimNodeDraft>,
    timestamp: &str,
) -> Result<(), FimModelError> {
    let fresh = old.is_none();
    let xdf2 = dialect == Dialect::Xdf2;
    if draft.kind == FimKind::CodeList {
        if tree.records[&record].local() != "codelisteReferenz" {
            return Err(FimModelError::DecisionRequired("inline Genericode/list messages remain inspectable; use their XML fields, not a reference replacement".into()));
        }
        let reference = draft
            .code_list
            .as_ref()
            .ok_or_else(|| xml_error("missing code-list reference"))?;
        update_reference(tree, dialect, record, reference)?;
        if let Some(parent) = tree.records[&record].parent {
            update_code_keys(tree, dialect, parent, Some(reference))?
        }
        return Ok(());
    }
    if fresh || old.is_some_and(|o| o.identity != draft.identity || o.version != draft.version) {
        let identification = if let Some(r) = tree.child(record, "identifikation") {
            r
        } else {
            tree.insert(
                record,
                node_anchor(tree, dialect, record, "identifikation"),
                "identifikation",
                dialect.namespace(),
                None,
            )?
        };
        update_property(
            tree,
            dialect,
            identification,
            "id",
            draft.identity.as_deref(),
            false,
        )?;
        update_property(
            tree,
            dialect,
            identification,
            "version",
            draft.version.as_deref(),
            false,
        )?;
    }
    if fresh || old.is_some_and(|o| o.name != draft.name) {
        update_property(tree, dialect, record, "name", Some(&draft.name), false)?
    }
    macro_rules! property {
        ($member:ident, $xml:expr, $code:expr) => {
            if fresh || old.is_some_and(|o| o.$member != draft.$member) {
                update_property(tree, dialect, record, $xml, draft.$member.as_deref(), $code)?;
            }
        };
    }
    property!(description, "beschreibung", false);
    property!(definition, "definition", false);
    property!(legal_reference, "bezug", false);
    property!(input_label, "bezeichnungEingabe", false);
    property!(output_label, "bezeichnungAusgabe", false);
    property!(schema_element_type, "schemaelementart", true);
    property!(input_help, "hilfetextEingabe", false);
    property!(output_help, "hilfetextAusgabe", false);
    property!(field_type, "feldart", true);
    property!(datatype, "datentyp", true);
    property!(content, "inhalt", false);
    property!(max_size, "maxSize", false);
    if fresh || old.is_some_and(|o| o.media_types != draft.media_types) {
        let existing: Vec<_> = tree
            .children(record)
            .into_iter()
            .filter(|r| {
                tree.records[r].local() == "mediaType"
                    && tree.records[r].namespace == dialect.namespace()
            })
            .collect();
        for (i, value) in draft.media_types.iter().enumerate() {
            if let Some(r) = existing.get(i) {
                tree.set_text(*r, value)?
            } else {
                tree.insert(
                    record,
                    node_anchor(tree, dialect, record, "mediaType"),
                    "mediaType",
                    dialect.namespace(),
                    Some(value),
                )?;
            }
        }
        for r in existing.into_iter().skip(draft.media_types.len()) {
            tree.remove(r)?
        }
    }
    property!(prefill, "vorbefuellung", true);
    property!(rule_text, "freitextRegel", false);
    property!(rule_script, if xdf2 { "script" } else { "skript" }, false);
    property!(rule_type, "typ", true);
    let status_name = if xdf2 { "status" } else { "freigabestatus" };
    property!(status, status_name, true);
    if (fresh || old.is_some_and(|o| o.status_list_version != draft.status_list_version))
        && let Some(status) = tree.child(record, status_name)
    {
        tree.set_attribute(
            status,
            "listVersionID",
            draft.status_list_version.as_deref(),
        )?
    }
    if !xdf2 && (fresh || old.is_some_and(|o| o.last_changed != draft.last_changed)) {
        let changed = if fresh {
            Some(timestamp)
        } else {
            draft.last_changed.as_deref()
        };
        if let Some(value) = changed {
            validate_timestamp(value)?
        } else {
            return Err(FimModelError::Integrity(
                "XDF3 requires letzteAenderung".into(),
            ));
        }
        update_property(tree, dialect, record, "letzteAenderung", changed, false)?;
    }
    if (fresh || old.is_some_and(|o| o.group_choice != draft.group_choice))
        && !xdf2
        && matches!(draft.kind, FimKind::Group | FimKind::Choice)
    {
        update_property(
            tree,
            dialect,
            record,
            "art",
            draft.group_choice.then_some("X"),
            true,
        )?;
    }
    if fresh || old.is_some_and(|o| o.precision != draft.precision) {
        let p = &draft.precision;
        let present = p.text.is_some()
            || [
                p.min_length.as_ref(),
                p.max_length.as_ref(),
                p.min_value.as_ref(),
                p.max_value.as_ref(),
                p.pattern.as_ref(),
            ]
            .iter()
            .any(|v| v.is_some());
        update_property(
            tree,
            dialect,
            record,
            "praezisierung",
            present.then(|| p.text.as_deref().unwrap_or("")),
            false,
        )?;
        if let Some(precision) = tree.child(record, "praezisierung") {
            for (name, value) in [
                ("minLength", &p.min_length),
                ("maxLength", &p.max_length),
                ("minValue", &p.min_value),
                ("maxValue", &p.max_value),
                ("pattern", &p.pattern),
            ] {
                tree.set_attribute(precision, name, value.as_deref())?;
            }
        }
    }
    if fresh || old.is_some_and(|o| o.code_list != draft.code_list) {
        let existing = tree.child(record, "codelisteReferenz");
        if let Some(reference) = &draft.code_list {
            let r = if let Some(r) = existing {
                r
            } else {
                tree.insert(
                    record,
                    node_anchor(tree, dialect, record, "codelisteReferenz"),
                    "codelisteReferenz",
                    dialect.namespace(),
                    None,
                )?
            };
            update_reference(tree, dialect, r, reference)?;
        } else if let Some(r) = existing {
            tree.remove(r)?
        }
        update_code_keys(tree, dialect, record, draft.code_list.as_ref())?;
    }
    if fresh || old.is_some_and(|o| o.rules != draft.rules) {
        let existing: Vec<_> = tree
            .children(record)
            .into_iter()
            .filter(|r| {
                tree.records[r].local() == "regel"
                    && tree.records[r].namespace == dialect.namespace()
            })
            .collect();
        // Retain unchanged rule subtrees verbatim, including opaque target/script metadata.
        let old_rules = old.map(|o| o.rules.as_slice()).unwrap_or(&[]);
        for (i, rule) in old_rules.iter().enumerate().rev() {
            if !draft.rules.contains(rule)
                && let Some(r) = existing.get(i)
            {
                tree.remove(*r)?
            }
        }
        let mut ordered = Vec::new();
        for rule in &draft.rules {
            if let Some(index) = old_rules.iter().position(|r| r == rule) {
                ordered.push(
                    *existing
                        .get(index)
                        .ok_or_else(|| xml_error("rule association mismatch"))?,
                );
            } else {
                ordered.push(attach_clone(
                    tree,
                    *rule,
                    record,
                    node_anchor(tree, dialect, record, "regel"),
                )?);
            }
        }
        let before = node_anchor(tree, dialect, record, "regel");
        let e = tree.records.get_mut(&record).unwrap();
        e.children
            .retain(|c| !matches!(c, Content::Element(r) if ordered.contains(r)));
        let index = before
            .and_then(|r| e.children.iter().position(|c| *c == Content::Element(r)))
            .unwrap_or(e.children.len());
        for (offset, rule) in ordered.into_iter().enumerate() {
            e.children.insert(index + offset, Content::Element(rule));
        }
    }
    Ok(())
}
fn update_reference(
    tree: &mut XmlTree,
    dialect: Dialect,
    record: RecordId,
    reference: &FimCodeListReference,
) -> Result<(), FimModelError> {
    let genericode = if dialect == Dialect::Xdf2 {
        let identification = if let Some(r) = tree.child(record, "identifikation") {
            r
        } else {
            tree.insert(
                record,
                tree.children(record).first().copied(),
                "identifikation",
                dialect.namespace(),
                None,
            )?
        };
        update_property(
            tree,
            dialect,
            identification,
            "id",
            reference.identity.as_deref(),
            false,
        )?;
        update_property(
            tree,
            dialect,
            identification,
            "version",
            reference.version.as_deref(),
            false,
        )?;
        if let Some(r) = tree.child(record, "genericodeIdentification") {
            r
        } else {
            tree.insert(
                record,
                None,
                "genericodeIdentification",
                dialect.namespace(),
                None,
            )?
        }
    } else {
        record
    };
    update_property(
        tree,
        dialect,
        genericode,
        "canonicalIdentification",
        reference.canonical_identification.as_deref(),
        false,
    )?;
    update_property(
        tree,
        dialect,
        genericode,
        "version",
        reference.genericode_version.as_deref(),
        false,
    )?;
    update_property(
        tree,
        dialect,
        genericode,
        "canonicalVersionUri",
        reference.canonical_version_uri.as_deref(),
        false,
    )?;
    Ok(())
}
fn update_code_keys(
    tree: &mut XmlTree,
    dialect: Dialect,
    field: RecordId,
    reference: Option<&FimCodeListReference>,
) -> Result<(), FimModelError> {
    if dialect == Dialect::Xdf3 {
        for (name, value) in [
            ("codeKey", reference.and_then(|r| r.code_key.as_deref())),
            ("nameKey", reference.and_then(|r| r.name_key.as_deref())),
            ("helpKey", reference.and_then(|r| r.help_key.as_deref())),
        ] {
            update_property(tree, dialect, field, name, value, false)?;
        }
    }
    Ok(())
}
/// Wizard timestamps are caller-supplied RFC3339 date-times; no fixed sample date.
fn validate_timestamp(value: &str) -> Result<(), FimModelError> {
    let valid = || -> Option<()> {
        let (date, time) = value.split_once('T')?;
        let date: Vec<_> = date.split('-').collect();
        if date.len() != 3
            || date[0].len() != 4
            || date[1].len() != 2
            || date[2].len() != 2
            || date.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        let year: u32 = date[0].parse().ok()?;
        let month: usize = date[1].parse().ok()?;
        let day: u32 = date[2].parse().ok()?;
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        if year == 0 || month == 0 || month > 12 || day == 0 || day > days[month - 1] {
            return None;
        }
        let clock = if let Some(clock) = time.strip_suffix('Z') {
            clock
        } else {
            let offset = time
                .char_indices()
                .find(|(_, c)| matches!(c, '+' | '-'))
                .map(|(i, _)| i)?;
            let (clock, zone) = time.split_at(offset);
            let (h, m) = zone[1..].split_once(':')?;
            if h.len() != 2
                || m.len() != 2
                || !h.bytes().all(|b| b.is_ascii_digit())
                || !m.bytes().all(|b| b.is_ascii_digit())
                || h.parse::<u32>().ok()? > 14
                || m.parse::<u32>().ok()? > 59
                || (h == "14" && m != "00")
            {
                return None;
            }
            clock
        };
        let parts: Vec<_> = clock.split(':').collect();
        if parts.len() != 3 || parts[0].len() != 2 || parts[1].len() != 2 {
            return None;
        }
        let (seconds, fraction) = parts[2]
            .split_once('.')
            .map_or((parts[2], None), |(s, f)| (s, Some(f)));
        if seconds.len() != 2
            || !seconds.bytes().all(|b| b.is_ascii_digit())
            || !parts[0].bytes().all(|b| b.is_ascii_digit())
            || !parts[1].bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
            || parts[0].parse::<u32>().ok()? > 23
            || parts[1].parse::<u32>().ok()? > 59
            || seconds.parse::<u32>().ok()? > 59
        {
            return None;
        }
        Some(())
    };
    if valid().is_none() {
        return Err(FimModelError::Integrity(
            "supply an actual RFC3339 caller timestamp for letzteAenderung".into(),
        ));
    }
    Ok(())
}

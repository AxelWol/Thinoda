use thinoda_core::{Document, Sheet, Workbook, fim::*};

fn source(dialect: Dialect) -> Sheet {
    let body = "<g:stammdatenschema><g:identifikation><g:id>S1</g:id></g:identifikation><g:name>Schema</g:name></g:stammdatenschema>";
    FimDocument::parse(format!("<?xml version='1.0'?><!--retain--><g:xdatenfelder.stammdatenschema.0102 xmlns:g='{}'><g:header/>{body}</g:xdatenfelder.stammdatenschema.0102>", dialect.namespace()).as_bytes()).unwrap().into_sheet().unwrap()
}
fn draft(dialect: Dialect, kind: FimKind, id: &str) -> FimNodeDraft {
    let mut draft = FimNodeDraft::new(dialect, kind);
    draft.identity = Some(id.into());
    draft.name = id.into();
    if draft.input_label.is_some() {
        draft.input_label = Some(format!("Input {id}"))
    }
    draft
}
fn insert(
    sheet: &mut Sheet,
    parent: RecordId,
    index: usize,
    child: Option<RecordId>,
    draft: FimNodeDraft,
) {
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index,
                initial_child: child,
                reuse: None,
            },
            draft: Box::new(draft),
            timestamp: "2026-11-01T08:44:14+01:00".into(),
        })
        .unwrap();
}
fn root(sheet: &Sheet) -> RecordId {
    sheet
        .fim
        .as_ref()
        .unwrap()
        .record_for_topic(sheet.root)
        .unwrap()
}
fn edit(
    sheet: &mut Sheet,
    record: RecordId,
    independent: bool,
    draft: FimNodeDraft,
) -> Result<(), FimModelError> {
    sheet.apply_fim_change(FimChange::AuthorNode {
        target: FimNodeTarget::Edit {
            record,
            independent,
        },
        draft: Box::new(draft),
        timestamp: "2026-11-01T08:44:14+01:00".into(),
    })
}
#[test]
fn defaults_order_nonempty_group_and_associations_both_dialects() {
    for dialect in [Dialect::Xdf2, Dialect::Xdf3] {
        let mut sheet = source(dialect);
        let parent = root(&sheet);
        insert(
            &mut sheet,
            parent,
            0,
            None,
            draft(dialect, FimKind::Field, "F1"),
        );
        let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
        let before = sheet.clone();
        if dialect == Dialect::Xdf3 {
            assert!(
                sheet
                    .apply_fim_change(FimChange::AuthorNode {
                        target: FimNodeTarget::Insert {
                            parent,
                            index: 0,
                            initial_child: None,
                            reuse: None
                        },
                        draft: Box::new(draft(dialect, FimKind::Group, "G1")),
                        timestamp: "2026-11-01T08:44:14Z".into(),
                    })
                    .is_err()
            );
            assert_eq!(sheet, before);
        }
        insert(
            &mut sheet,
            parent,
            0,
            Some(field),
            draft(dialect, FimKind::Group, "G1"),
        );
        let document = sheet.fim.as_ref().unwrap();
        let children = document.node_children(parent);
        assert_eq!(
            document
                .record_metadata(children[0])
                .unwrap()
                .identity
                .as_deref(),
            Some("G1")
        );
        assert_eq!(document.node_parent(field), Some(parent));
        assert_eq!(document.node_children(children[0]).len(), 1);
        let field_draft = document.node_draft(field).unwrap();
        assert_eq!(field_draft.field_type.as_deref(), Some("input"));
        assert_eq!(field_draft.datatype.as_deref(), Some("text"));
        if dialect == Dialect::Xdf3 {
            assert_eq!(
                field_draft.last_changed.as_deref(),
                Some("2026-11-01T08:44:14+01:00")
            );
            assert_eq!(field_draft.prefill.as_deref(), Some("keine"));
        } else {
            assert_eq!(field_draft.precision.text.as_deref(), Some(""));
            assert_eq!(field_draft.content.as_deref(), Some(""));
        }
        insert(
            &mut sheet,
            field,
            0,
            None,
            draft(dialect, FimKind::Rule, "R1"),
        );
        let document = sheet.fim.as_ref().unwrap();
        let rule = document.node_draft(field).unwrap().rules[0];
        assert!(document.topic_for_record(rule).is_none());
        assert_eq!(document.node_parent(rule), Some(field));
        assert_eq!(
            document.node_draft(rule).unwrap().rule_type.as_deref(),
            (dialect == Dialect::Xdf3).then_some("V")
        );
    }
}

#[test]
fn absent_optional_opaque_precision_comments_and_noop_bytes_are_retained() {
    let xml = format!(
        "\u{feff}<?xml version='1.0'?><x:xdatenfelder.datenfeld.0104 xmlns:x='{}' xmlns:opaque='urn:opaque'><x:header/><x:datenfeld custom='lexical'><x:identifikation><x:id>F1</x:id></x:identifikation><x:name>Original</x:name><x:praezisierung minValue='very opaque' pattern='[a-z]+' opaque:preserve='yes'><![CDATA[unchanged < & ]]><!--retain--></x:praezisierung><opaque:extension keep='001'/></x:datenfeld></x:xdatenfelder.datenfeld.0104>",
        Dialect::Xdf3.namespace()
    );
    let mut sheet = FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    let original = sheet.fim.as_ref().unwrap().clone();
    let record = root(&sheet);
    let mut form = original.node_draft(record).unwrap();
    assert!(form.input_label.is_none());
    edit(&mut sheet, record, false, form.clone()).unwrap();
    assert_eq!(sheet.fim.as_ref().unwrap().serialize(), xml.as_bytes());
    assert!(sheet.fim.as_ref().unwrap().same_source_revision(&original));
    form.name = "Renamed".into();
    edit(&mut sheet, record, false, form).unwrap();
    let bytes = String::from_utf8(sheet.fim.as_ref().unwrap().serialize()).unwrap();
    assert!(bytes.contains("minValue='very opaque' pattern='[a-z]+' opaque:preserve='yes'"));
    assert!(bytes.contains("<![CDATA[unchanged < & ]]><!--retain-->"));
    assert!(bytes.contains("<opaque:extension keep='001'/>"));
    assert!(!bytes.contains("bezeichnungEingabe"));
    assert!(!bytes.contains("freigabestatus"));
}

#[test]
fn shared_edit_detach_and_parent_edge_multiplicity_are_distinct() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let record = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index: 0,
                initial_child: None,
                reuse: Some(record),
            },
            draft: Box::new(form),
            timestamp: String::new(),
        })
        .unwrap();
    let mut form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    form.name = "Shared".into();
    form.cardinality = Some("000:00012".into());
    edit(&mut sheet, record, false, form).unwrap();
    let document = sheet.fim.as_ref().unwrap();
    let records = document.node_children(parent);
    assert_eq!(document.record_metadata(record).unwrap().usages, 2);
    assert!(
        records
            .iter()
            .all(|r| document.node_draft(*r).unwrap().name == "Shared")
    );
    assert_eq!(
        document
            .node_draft(records[0])
            .unwrap()
            .cardinality
            .as_deref(),
        Some("1:1")
    );
    assert_eq!(
        document.node_draft(record).unwrap().cardinality.as_deref(),
        Some("000:00012")
    );
    let mut form = document.node_draft(record).unwrap();
    form.identity = Some("F2".into());
    form.name = "Detached".into();
    edit(&mut sheet, record, true, form).unwrap();
    let document = sheet.fim.as_ref().unwrap();
    assert_eq!(document.record_metadata(record).unwrap().usages, 1);
    assert_eq!(
        document.node_draft(records[0]).unwrap().identity.as_deref(),
        Some("F1")
    );
}

#[test]
fn invalid_form_and_identity_collisions_roll_back_whole_candidate() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let record = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let before = sheet.clone();
    let original = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    for change in 0..7 {
        let mut form = original.clone();
        form.name = "Should not leak".into();
        match change {
            0 => form.cardinality = Some("2:1".into()),
            1 => form.datatype = Some("invented".into()),
            2 => form.precision.max_length = Some("0".into()),
            3 => form.field_type = Some("select".into()),
            4 => form.last_changed = Some("2026-02-30T08:00:00Z".into()),
            5 => {
                form.precision.min_length = Some("5".into());
                form.precision.max_length = Some("4".into());
            }
            _ => {
                form.datatype = Some("num".into());
                form.precision.min_value = Some("-0.00001".into());
                form.precision.max_value = Some("-0.00002".into());
            }
        }
        if change == 3 {
            form.datatype = Some("num".into())
        }
        assert!(edit(&mut sheet, record, false, form).is_err());
        assert_eq!(sheet, before);
    }
    assert!(
        sheet
            .apply_fim_change(FimChange::AuthorNode {
                target: FimNodeTarget::Insert {
                    parent,
                    index: 0,
                    initial_child: None,
                    reuse: None
                },
                draft: Box::new(draft(Dialect::Xdf3, FimKind::Field, "F1")),
                timestamp: "2026-11-01T08:44:14Z".into(),
            })
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn batch_can_repair_intermediate_reference_and_is_one_atomic_commit() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let record = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Rule, "R1"),
    );
    let rule = sheet
        .fim
        .as_ref()
        .unwrap()
        .node_draft(parent)
        .unwrap()
        .rules[0];
    sheet
        .apply_fim_change(FimChange::InsertElement {
            parent: rule,
            before: None,
            name: "ziel".into(),
            namespace: Dialect::Xdf3.namespace().into(),
            text: None,
        })
        .unwrap();
    let target = sheet
        .fim
        .as_ref()
        .unwrap()
        .node_fields(rule)
        .into_iter()
        .find(|f| f.name == "ziel")
        .unwrap()
        .record;
    sheet
        .apply_fim_change(FimChange::SetAttribute {
            record: target,
            name: "element".into(),
            value: Some("F1".into()),
        })
        .unwrap();
    let mut form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    form.identity = Some("F2".into());
    sheet
        .apply_fim_change(FimChange::Batch(vec![
            FimChange::AuthorNode {
                target: FimNodeTarget::Edit {
                    record,
                    independent: false,
                },
                draft: Box::new(form),
                timestamp: String::new(),
            },
            FimChange::SetAttribute {
                record: target,
                name: "element".into(),
                value: Some("F2".into()),
            },
        ]))
        .unwrap();
    let before = sheet.clone();
    assert!(
        sheet
            .apply_fim_change(FimChange::RemoveElement { record })
            .is_err()
    );
    assert_eq!(sheet, before);
    assert!(
        sheet
            .apply_fim_change(FimChange::Batch(vec![
                FimChange::RenameDefinition {
                    record,
                    name: "Never leak".into()
                },
                FimChange::SetText {
                    record: RecordId(u64::MAX),
                    value: "bad".into()
                },
            ]))
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn scoped_fields_and_derived_index_do_not_change_native_equality() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let document = sheet.fim.as_ref().unwrap();
    let before = serde_json::to_string(document).unwrap();
    let reopened: FimDocument = serde_json::from_str(&before).unwrap();
    let field = document.node_children(parent)[0];
    assert!(
        document
            .node_fields(parent)
            .iter()
            .all(|f| f.record != field && !f.name.ends_with("struktur"))
    );
    assert!(document.node_fields(parent)[0].value.is_empty());
    assert!(
        document
            .node_edge_fields(field)
            .iter()
            .any(|f| f.name == "anzahl")
    );
    assert!(document.node_edge_fields(RecordId(u64::MAX)).is_empty());
    assert_eq!(document.records().len(), 2);
    assert_eq!(*document, reopened);
    assert_eq!(serde_json::to_string(document).unwrap(), before);
}

#[test]
fn precision_attribute_edit_preserves_unrelated_lexical_attributes() {
    let xml = format!(
        "<x:xdatenfelder.datenfeld.0104 xmlns:x='{}' xmlns:o='urn:opaque'><x:header/><x:datenfeld><x:identifikation><x:id>F1</x:id></x:identifikation><x:name>First</x:name><x:praezisierung maxLength = '0012' o:keep='&#x31;' pattern = '[a-z]+'><![CDATA[keep]]><!--opaque--></x:praezisierung></x:datenfeld></x:xdatenfelder.datenfeld.0104>",
        Dialect::Xdf3.namespace()
    );
    let mut sheet = FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    let record = root(&sheet);
    let mut form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    form.precision.max_length = Some("0024".into());
    edit(&mut sheet, record, false, form).unwrap();
    let bytes = String::from_utf8(sheet.fim.as_ref().unwrap().serialize()).unwrap();
    assert!(bytes.contains("maxLength = '0024' o:keep='&#x31;' pattern = '[a-z]+'"));
    assert!(bytes.contains("<![CDATA[keep]]><!--opaque-->"));
}

#[test]
fn shared_parent_detach_and_incoming_edge_update_preserve_parent_equivalence() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    insert(
        &mut sheet,
        parent,
        1,
        Some(field),
        draft(Dialect::Xdf3, FimKind::Group, "G1"),
    );
    let group = sheet.fim.as_ref().unwrap().node_children(parent)[1];
    let form = sheet.fim.as_ref().unwrap().node_draft(group).unwrap();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index: 2,
                initial_child: None,
                reuse: Some(group),
            },
            draft: Box::new(form),
            timestamp: String::new(),
        })
        .unwrap();
    let nested = sheet.fim.as_ref().unwrap().node_children(group)[0];
    let mut form = sheet.fim.as_ref().unwrap().node_draft(nested).unwrap();
    form.identity = Some("F2".into());
    form.cardinality = Some("000:0004".into());
    edit(&mut sheet, nested, true, form).unwrap();
    let document = sheet.fim.as_ref().unwrap();
    let groups: Vec<_> = document
        .records()
        .into_iter()
        .filter(|r| r.identity.as_deref() == Some("G1"))
        .collect();
    assert_eq!(groups.len(), 2);
    for group in groups {
        let nested = document.node_children(group.record)[0];
        let form = document.node_draft(nested).unwrap();
        assert_eq!(form.identity.as_deref(), Some("F2"));
        assert_eq!(form.cardinality.as_deref(), Some("000:0004"));
    }
    assert_eq!(
        document.node_draft(field).unwrap().identity.as_deref(),
        Some("F1")
    );
}

#[test]
fn associated_rule_and_identified_c_reference_can_detach_without_topics() {
    for dialect in [Dialect::Xdf2, Dialect::Xdf3] {
        let mut sheet = source(dialect);
        let parent = root(&sheet);
        insert(
            &mut sheet,
            parent,
            0,
            None,
            draft(dialect, FimKind::Field, "F1"),
        );
        let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
        insert(
            &mut sheet,
            parent,
            0,
            None,
            draft(dialect, FimKind::Rule, "R1"),
        );
        let rule = sheet
            .fim
            .as_ref()
            .unwrap()
            .node_draft(parent)
            .unwrap()
            .rules[0];
        let mut form = sheet.fim.as_ref().unwrap().node_draft(rule).unwrap();
        form.identity = Some("R2".into());
        edit(&mut sheet, rule, true, form).unwrap();
        let mut c = FimNodeDraft::new(dialect, FimKind::CodeList);
        c.code_list = Some(FimCodeListReference {
            identity: (dialect == Dialect::Xdf2).then(|| "C1".into()),
            canonical_identification: Some("urn:local:list".into()),
            genericode_version: Some("1".into()),
            canonical_version_uri: Some("urn:local:list:1".into()),
            ..Default::default()
        });
        insert(&mut sheet, field, 0, None, c);
        let record = sheet
            .fim
            .as_ref()
            .unwrap()
            .records()
            .iter()
            .find(|r| r.kind == FimKind::CodeList)
            .unwrap()
            .record;
        assert!(
            sheet
                .fim
                .as_ref()
                .unwrap()
                .topic_for_record(record)
                .is_none()
        );
        let mut form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
        if dialect == Dialect::Xdf2 {
            form.identity = Some("C2".into())
        } else {
            form.code_list.as_mut().unwrap().canonical_version_uri = Some("urn:local:list:2".into())
        }
        edit(&mut sheet, record, true, form).unwrap();
        let form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
        if dialect == Dialect::Xdf2 {
            assert_eq!(form.identity.as_deref(), Some("C2"))
        } else {
            assert_eq!(
                form.code_list.unwrap().canonical_version_uri.as_deref(),
                Some("urn:local:list:2")
            )
        }
        sheet
            .apply_fim_change(FimChange::RemoveElement { record })
            .unwrap();
        assert!(
            sheet
                .fim
                .as_ref()
                .unwrap()
                .records()
                .iter()
                .all(|r| r.kind != FimKind::CodeList)
        );
    }
}

#[test]
fn column_key_pairing_rule_association_order_and_last_group_child_are_checked() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let mut form = sheet.fim.as_ref().unwrap().node_draft(field).unwrap();
    form.code_list = Some(FimCodeListReference {
        canonical_identification: Some("urn:local:codes".into()),
        code_key: Some("code".into()),
        ..Default::default()
    });
    let before = sheet.clone();
    assert!(edit(&mut sheet, field, false, form).is_err());
    assert_eq!(sheet, before);
    for id in ["R1", "R2"] {
        insert(
            &mut sheet,
            parent,
            0,
            None,
            draft(Dialect::Xdf3, FimKind::Rule, id),
        )
    }

    let mut form = sheet.fim.as_ref().unwrap().node_draft(parent).unwrap();
    form.rules.reverse();
    edit(&mut sheet, parent, false, form).unwrap();
    let document = sheet.fim.as_ref().unwrap();
    let ids: Vec<_> = document
        .node_draft(parent)
        .unwrap()
        .rules
        .into_iter()
        .map(|r| document.record_metadata(r).unwrap().identity.unwrap())
        .collect();
    assert_eq!(ids, ["R2", "R1"]);
    insert(
        &mut sheet,
        parent,
        0,
        Some(field),
        draft(Dialect::Xdf3, FimKind::Group, "G1"),
    );
    let group = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let only_child = sheet.fim.as_ref().unwrap().node_children(group)[0];
    let before = sheet.clone();
    let child_topic = sheet
        .fim
        .as_ref()
        .unwrap()
        .topic_for_record(only_child)
        .unwrap();
    assert!(
        sheet
            .apply_fim_change(FimChange::Remove { topic: child_topic })
            .is_err()
    );
    assert_eq!(sheet, before);
}

#[test]
fn reuse_carries_inherited_namespaces_without_rewriting_original_definition() {
    let xml = format!(
        "<x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'><x:header/><x:stammdatenschema><x:identifikation><x:id>S1</x:id></x:identifikation><x:name>Schema</x:name><x:struktur><x:anzahl>1:1</x:anzahl><x:enthaelt><x:datenfeldgruppe xmlns:f='{}' xmlns:o='urn:opaque'><x:identifikation><x:id>G1</x:id></x:identifikation><x:name>Group</x:name><x:struktur><x:anzahl>1:1</x:anzahl><x:enthaelt><f:datenfeld o:keep='001'><f:identifikation><f:id>F1</f:id></f:identifikation><f:name>Field</f:name><o:extension/></f:datenfeld></x:enthaelt></x:struktur></x:datenfeldgruppe></x:enthaelt></x:struktur></x:stammdatenschema></x:xdatenfelder.stammdatenschema.0102>",
        Dialect::Xdf3.namespace(),
        Dialect::Xdf3.namespace()
    );
    let mut sheet = FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    let parent = root(&sheet);
    let group = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let field = sheet.fim.as_ref().unwrap().node_children(group)[0];
    let form = sheet.fim.as_ref().unwrap().node_draft(field).unwrap();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index: 1,
                initial_child: None,
                reuse: Some(field),
            },
            draft: Box::new(form),
            timestamp: String::new(),
        })
        .unwrap();
    let document = sheet.fim.as_ref().unwrap();
    assert_eq!(document.record_metadata(field).unwrap().usages, 2);
    let bytes = String::from_utf8(document.serialize()).unwrap();
    assert!(bytes.contains("<f:datenfeld o:keep='001'><f:identifikation>"));
    assert!(bytes.contains("xmlns:f=\"urn:xoev-de:fim:standard:xdatenfelder_3.0.0\""));
    assert!(bytes.contains("xmlns:o=\"urn:opaque\""));
}

#[test]
fn deleting_satellite_occurrence_retains_association_on_an_unrelated_parent() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    for id in ["F1", "F2"] {
        insert(
            &mut sheet,
            parent,
            0,
            None,
            draft(Dialect::Xdf3, FimKind::Field, id),
        );
    }
    let fields = sheet.fim.as_ref().unwrap().node_children(parent);
    insert(
        &mut sheet,
        fields[0],
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Rule, "R1"),
    );
    let rule = sheet
        .fim
        .as_ref()
        .unwrap()
        .node_draft(fields[0])
        .unwrap()
        .rules[0];
    let mut form = sheet.fim.as_ref().unwrap().node_draft(fields[1]).unwrap();
    form.rules = vec![rule];
    edit(&mut sheet, fields[1], false, form).unwrap();
    assert_eq!(
        sheet
            .fim
            .as_ref()
            .unwrap()
            .record_metadata(rule)
            .unwrap()
            .usages,
        2
    );
    sheet
        .apply_fim_change(FimChange::RemoveElement { record: rule })
        .unwrap();
    let document = sheet.fim.as_ref().unwrap();
    assert!(document.node_draft(fields[0]).unwrap().rules.is_empty());
    assert_eq!(document.node_draft(fields[1]).unwrap().rules.len(), 1);
    assert_eq!(
        document
            .records()
            .into_iter()
            .filter(|r| r.kind == FimKind::Rule)
            .count(),
        1
    );
}

#[test]
fn formatted_code_containers_read_leaf_values_and_noop_preserves_bytes() {
    let xml = format!(
        "<x:xdatenfelder.datenfeld.0104 xmlns:x='{}'><x:header/><x:datenfeld><x:identifikation><x:id>F1</x:id></x:identifikation><x:name>Field</x:name><x:freigabestatus listVersionID='2022-07-12'>\n  <code xmlns=''>2</code>\n</x:freigabestatus><x:feldart>\n  <code xmlns=''>input</code>\n</x:feldart><x:datentyp>\n  <code xmlns=''>text</code>\n</x:datentyp></x:datenfeld></x:xdatenfelder.datenfeld.0104>",
        Dialect::Xdf3.namespace()
    );
    let mut sheet = FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    let record = root(&sheet);
    let form = sheet.fim.as_ref().unwrap().node_draft(record).unwrap();
    assert_eq!(form.field_type.as_deref(), Some("input"));
    assert_eq!(form.datatype.as_deref(), Some("text"));
    assert_eq!(form.status.as_deref(), Some("2"));
    edit(&mut sheet, record, false, form).unwrap();
    assert_eq!(sheet.fim.as_ref().unwrap().serialize(), xml.as_bytes());
}

#[test]
fn typed_and_advanced_batch_has_exactly_one_undo_step() {
    let mut sheet = source(Dialect::Xdf3);
    let parent = root(&sheet);
    insert(
        &mut sheet,
        parent,
        0,
        None,
        draft(Dialect::Xdf3, FimKind::Field, "F1"),
    );
    let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    let mut form = sheet.fim.as_ref().unwrap().node_draft(field).unwrap();
    form.name = "One typed edit".into();
    let original = sheet.fim.as_ref().unwrap().serialize();
    let mut document = Document::new(Workbook::new(sheet));
    document
        .edit_sheet("Node wizard", |sheet| {
            sheet
                .apply_fim_change(FimChange::Batch(vec![
                    FimChange::AuthorNode {
                        target: FimNodeTarget::Edit {
                            record: field,
                            independent: false,
                        },
                        draft: Box::new(form),
                        timestamp: String::new(),
                    },
                    FimChange::SetAttribute {
                        record: field,
                        name: "custom".into(),
                        value: Some("advanced".into()),
                    },
                ]))
                .map_err(|e| thinoda_core::EditError::Invalid(e.to_string()))
        })
        .unwrap();
    let updated = document.sheet().fim.as_ref().unwrap().serialize();
    assert_ne!(updated, original);
    assert!(document.undo());
    assert_eq!(document.sheet().fim.as_ref().unwrap().serialize(), original);
    assert!(!document.undo());
    assert!(document.redo());
    assert_eq!(document.sheet().fim.as_ref().unwrap().serialize(), updated);
}
#[test]
fn insert_fragment_preserves_namespace_attribute_comment_order_and_anchor() {
    let mut sheet = source(Dialect::Xdf3);
    let schema = root(&sheet);
    let ns = Dialect::Xdf3.namespace();
    let name = sheet
        .fim
        .as_ref()
        .unwrap()
        .node_fields(schema)
        .into_iter()
        .find(|f| f.parent == Some(schema) && f.name == "g:name")
        .unwrap()
        .record;
    let first = format!(
        "<g:relation xmlns:g='{ns}' b='2'  a=\"1\"><!--keep--><g:praedikat><code xmlns=\"\">ABL</code></g:praedikat></g:relation>"
    );
    let second = format!("<x:stichwort xmlns:x=\"{ns}\" xmlns=\"\"><wert>w</wert></x:stichwort>");
    sheet
        .apply_fim_change(FimChange::Batch(vec![
            FimChange::InsertFragment {
                parent: schema,
                before: Some(name),
                xml: first.clone(),
            },
            FimChange::InsertFragment {
                parent: schema,
                before: None,
                xml: format!("\n{second}\n"),
            },
        ]))
        .unwrap();
    let out = String::from_utf8(sheet.fim.as_ref().unwrap().serialize()).unwrap();
    let a = out.find(&first).expect("lexical fragment retained");
    let b = out.find("<g:name>").unwrap();
    let c = out.find(&second).expect("second fragment retained");
    assert!(a < b && b < c, "{out}");
    let reparsed = FimDocument::parse(out.as_bytes()).unwrap();
    assert!(
        reparsed
            .node_fields(reparsed.records().first().unwrap().record)
            .iter()
            .any(|f| f.name == "code" && f.namespace.is_empty() && f.value == "ABL")
    );
}
#[test]
fn insert_fragment_rejects_unsafe_or_ambiguous_xml_and_rolls_back_batches() {
    let mut sheet = source(Dialect::Xdf3);
    let schema = root(&sheet);
    let ns = Dialect::Xdf3.namespace();
    let before = sheet.fim.as_ref().unwrap().serialize();
    let bad = [
        format!("<?xml version='1.0'?><g:relation xmlns:g='{ns}'/>"),
        format!(
            "<!DOCTYPE g:relation [<!ENTITY e SYSTEM 'file:///x'>]><g:relation xmlns:g='{ns}'>&e;</g:relation>"
        ),
        format!("<g:relation xmlns:g='{ns}'>&ext;</g:relation>"),
        format!("<g:relation xmlns:g='{ns}'/><g:relation xmlns:g='{ns}'/>"),
        format!("<!--outside--><g:relation xmlns:g='{ns}'/>"),
        format!("<g:relation xmlns:g='{ns}'/><?pi x?>"),
        "<g:relation/>".into(),
        format!("<g:relation xmlns:g='{ns}'><code>ABL</code></g:relation>"),
        format!("<g:relation xmlns:g='{ns}'><g:x></g:relation>"),
        format!("<g:datenfeld xmlns:g='{ns}'><g:name>n</g:name></g:datenfeld>"),
        format!("<g:relation xmlns:g='{ns}'><g:objekt><g:regel/></g:objekt></g:relation>"),
        "text".into(),
        String::new(),
    ];
    for xml in bad {
        assert!(
            sheet
                .apply_fim_change(FimChange::InsertFragment {
                    parent: schema,
                    before: None,
                    xml: xml.clone(),
                })
                .is_err(),
            "{xml}"
        );
        assert_eq!(sheet.fim.as_ref().unwrap().serialize(), before);
    }
    // A later failing step rolls back the whole composite, including the import.
    let good = format!("<g:stichwort xmlns:g='{ns}'><g:wert>w</g:wert></g:stichwort>");
    assert!(
        sheet
            .apply_fim_change(FimChange::Batch(vec![
                FimChange::InsertFragment {
                    parent: schema,
                    before: None,
                    xml: good.clone(),
                },
                FimChange::InsertFragment {
                    parent: schema,
                    before: Some(RecordId(u64::MAX)),
                    xml: good,
                },
            ]))
            .is_err()
    );
    assert_eq!(sheet.fim.as_ref().unwrap().serialize(), before);
}

#[test]
fn equivalent_definition_groups_exact_copies_and_keeps_conflicts_distinct() {
    let ns = Dialect::Xdf3.namespace();
    let field = |id: &str, name: &str, extra: &str| {
        format!(
            "<g:struktur><g:anzahl>1:1</g:anzahl><g:enthaelt><g:datenfeld>{extra}<g:identifikation><g:id>{id}</g:id><g:version>1.0</g:version></g:identifikation><g:name>{name}</g:name></g:datenfeld></g:enthaelt></g:struktur>"
        )
    };
    let body = [
        field("F1", "Same", ""),
        // Comments and namespace prefixes are not definition content.
        format!("<g:struktur><g:anzahl>0:1</g:anzahl><g:enthaelt><h:datenfeld xmlns:h='{ns}'><!--copy--><h:identifikation><h:id>F1</h:id><h:version>1.0</h:version></h:identifikation><h:name>Same</h:name></h:datenfeld></g:enthaelt></g:struktur>"),
        field("F2", "Left", ""),
        field("F2", "Right", ""),
        field("F3", "Alone", ""),
    ]
    .concat();
    let xml = format!(
        "<g:xdatenfelder.stammdatenschema.0102 xmlns:g='{ns}'><g:header/><g:stammdatenschema><g:identifikation><g:id>S1</g:id></g:identifikation><g:name>Schema</g:name>{body}</g:stammdatenschema></g:xdatenfelder.stammdatenschema.0102>"
    );
    let mut sheet = FimDocument::parse(xml.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    let parent = root(&sheet);
    let document = sheet.fim.as_ref().unwrap();
    let children = document.node_children(parent);
    assert_eq!(children.len(), 5);
    let [same_a, same_b, left, right, alone] = children[..] else {
        unreachable!()
    };
    assert!(document.equivalent_definition(same_a, same_b));
    assert!(document.equivalent_definition(same_b, same_a));
    assert!(document.equivalent_definition(alone, alone));
    assert!(!document.equivalent_definition(same_a, alone));
    // Same identity and version but different content: never collapsed.
    assert!(!document.equivalent_definition(left, right));
    assert!(document.equivalent_definition(left, left));
    assert!(!document.equivalent_definition(left, same_a));
    let unknown = RecordId(u64::MAX);
    assert!(!document.equivalent_definition(unknown, unknown));
    assert!(!document.equivalent_definition(same_a, unknown));
    // The predicate matches the edit rule: exact copies are shared, conflicts
    // require an explicit reconciliation decision.
    let mut form = document.node_draft(same_a).unwrap();
    form.name = "Renamed".into();
    edit(&mut sheet, same_a, false, form).unwrap();
    let document = sheet.fim.as_ref().unwrap();
    assert_eq!(document.node_draft(same_b).unwrap().name, "Renamed");
    assert!(document.equivalent_definition(same_a, same_b));
    let mut form = document.node_draft(left).unwrap();
    form.name = "Changed".into();
    let before = document.serialize();
    assert!(matches!(
        edit(&mut sheet, left, false, form),
        Err(FimModelError::DecisionRequired(_))
    ));
    assert_eq!(sheet.fim.as_ref().unwrap().serialize(), before);
}

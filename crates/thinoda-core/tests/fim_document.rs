use thinoda_core::{Document, Workbook, fim::*};

fn message(body: &str) -> Vec<u8> {
    format!("<?xml version='1.0' encoding='UTF-8'?>\n<!--before--><?audit inert?><x:xdatenfelder.stammdatenschema.0102 xmlns:x='{}'><x:header><x:nachrichtID>local</x:nachrichtID></x:header>{body}</x:xdatenfelder.stammdatenschema.0102>\n<!--after-->",Dialect::Xdf3.namespace()).into_bytes()
}
fn field(id: &str, name: &str) -> String {
    format!(
        "<x:datenfeld><x:identifikation><x:id>{id}</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>{name}</x:name><x:beschreibung><![CDATA[ A < B & C ]]><!--keep--><?keep yes?></x:beschreibung><x:werte><x:wert><x:code>001</x:code><code xmlns=''>002</code></x:wert></x:werte></x:datenfeld>"
    )
}
fn edge(field: &str, cardinality: &str) -> String {
    format!(
        "<x:struktur custom='keep'><x:anzahl>{cardinality}</x:anzahl><x:enthaelt>{field}</x:enthaelt></x:struktur>"
    )
}
fn sample() -> Vec<u8> {
    message(&format!(
        "<x:stammdatenschema><x:identifikation><x:id>S1</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Schema</x:name>{}{}<x:regel><x:identifikation><x:id>R1</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Rule</x:name><x:skript><![CDATA[neverExecute()]]></x:skript></x:regel></x:stammdatenschema>",
        edge(&field("F1", "First"), "0:*"),
        edge(&field("F2", "Second"), "1:1")
    ))
}
fn sheet() -> thinoda_core::Sheet {
    FimDocument::parse(&sample()).unwrap().into_sheet().unwrap()
}

#[test]
fn generic_paste_cannot_infer_fim_identity_from_a_matching_foreign_subtree() {
    let mut target = sheet();
    let before = target.clone();
    let mut foreign = thinoda_core::Sheet::new("Foreign", "Foreign");
    let topic = foreign.insert_child(foreign.root, "First").unwrap();
    let subtree = foreign.copy_subtree(topic).unwrap();
    assert!(target.paste(target.root, &[subtree]).is_err());
    assert_eq!(target, before);
}

#[test]
fn lossless_native_bindings_and_cosmetic_cow() {
    let bytes = sample();
    let sheet = sheet();
    let fim = sheet.fim.as_ref().unwrap();
    assert_eq!(fim.serialize(), bytes);
    assert_eq!(sheet.topic_count(), 3);
    assert_eq!(
        fim.topic_metadata(sheet.children(sheet.root)[0])
            .unwrap()
            .cardinality
            .as_deref(),
        Some("0:*")
    );
    let mut doc = Document::new(Workbook::new(sheet));
    doc.edit_sheet("style", |s| {
        s.topic_mut(s.root).unwrap().style.bold = Some(true);
        Ok(())
    })
    .unwrap();
    assert_eq!(doc.sheet().fim.as_ref().unwrap().serialize(), bytes);
    let saved = serde_json::to_string(doc.workbook()).unwrap();
    let reopened: Workbook = serde_json::from_str(&saved).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened.active().fim.as_ref().unwrap().serialize(), bytes);
    assert_eq!(reopened.active().fim, doc.sheet().fim);
    let masked = reopened.masked_copy();
    assert!(masked.active().fim.is_none());
    assert!(
        !serde_json::to_string(&masked)
            .unwrap()
            .contains("neverExecute")
    );
}
#[test]
fn rename_undo_and_mixed_content_preservation() {
    let mut doc = Document::new(Workbook::new(sheet()));
    let id = doc.sheet().children(doc.sheet().root)[0];
    let original = doc.sheet().fim.as_ref().unwrap().serialize();
    doc.edit_sheet("rename", |s| {
        s.topic_mut(id).unwrap().title = "New < & name".into();
        Ok(())
    })
    .unwrap();
    let bytes = doc.sheet().fim.as_ref().unwrap().serialize();
    assert!(String::from_utf8_lossy(&bytes).contains("New &lt; &amp; name"));
    assert!(
        String::from_utf8_lossy(&bytes).contains("<![CDATA[ A < B & C ]]><!--keep--><?keep yes?>")
    );
    assert!(doc.undo());
    assert_eq!(doc.sheet().fim.as_ref().unwrap().serialize(), original);
    assert!(doc.redo());
    doc.sheet().validate().unwrap();
}
#[test]
fn typed_create_cardinality_duplicate_delete_and_move() {
    let mut sheet = sheet();
    let root = sheet.root;
    sheet
        .apply_fim_change(FimChange::Add {
            parent: root,
            kind: FimKind::Group,
            name: "Local group".into(),
        })
        .unwrap();
    let group = *sheet.children(root).last().unwrap();
    let first = sheet.children(root)[0];
    sheet
        .apply_fim_change(FimChange::Move {
            topic: first,
            parent: group,
            index: 0,
        })
        .unwrap();
    assert_eq!(sheet.parent(first), Some(group));
    assert_eq!(
        sheet
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(first)
            .unwrap()
            .cardinality
            .as_deref(),
        Some("0:*")
    );
    sheet
        .apply_fim_change(FimChange::SetCardinality {
            topic: first,
            value: "0002:*".into(),
        })
        .unwrap();
    let before = sheet.clone();
    assert!(
        sheet
            .apply_fim_change(FimChange::SetCardinality {
                topic: first,
                value: "2:1".into()
            })
            .is_err()
    );
    assert_eq!(sheet, before);
    sheet
        .apply_fim_change(FimChange::Duplicate { topic: first })
        .unwrap();
    assert_eq!(sheet.children(group).len(), 2);
    let duplicate = sheet.children(group)[1];
    sheet
        .apply_fim_change(FimChange::Remove { topic: duplicate })
        .unwrap();
    assert_eq!(sheet.children(group).len(), 1);
    assert!(
        sheet
            .fim
            .as_ref()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|d| d.code == "missing-identity")
    );
}
#[test]
fn advanced_fields_preserve_order_and_reject_mixed_overwrite() {
    let mut s = sheet();
    let root = s.fim.as_ref().unwrap().root_record();
    let fields = s.fim.as_ref().unwrap().fields(root);
    let code = fields.iter().find(|f| f.name == "code").unwrap().record;
    s.apply_fim_change(FimChange::SetText {
        record: code,
        value: "0007".into(),
    })
    .unwrap();
    assert!(String::from_utf8_lossy(&s.fim.as_ref().unwrap().serialize()).contains(">0007</code>"));
    let definition = s.fim.as_ref().unwrap().record_for_topic(s.root).unwrap();
    let before = s.clone();
    assert!(
        s.apply_fim_change(FimChange::SetText {
            record: definition,
            value: "drop everything".into()
        })
        .is_err()
    );
    assert_eq!(s, before);
    let name = s
        .fim
        .as_ref()
        .unwrap()
        .fields(definition)
        .into_iter()
        .find(|f| f.name == "x:name")
        .unwrap()
        .record;
    s.apply_fim_change(FimChange::InsertElement {
        parent: definition,
        before: Some(name),
        name: "x:annotation".into(),
        namespace: Dialect::Xdf3.namespace().into(),
        text: Some("draft".into()),
    })
    .unwrap();
    assert!(
        s.fim
            .as_ref()
            .unwrap()
            .serialize()
            .windows(10)
            .any(|w| w == b"annotation")
    );
}
#[test]
fn generic_book_routes_cannot_detach_or_desynchronize() {
    let mut doc = Document::new(Workbook::new(sheet()));
    let before = doc.workbook().clone();
    assert!(
        doc.edit_book("detach", |w| {
            w.active_mut().fim = None;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(doc.workbook(), &before);
    assert!(!doc.can_undo());
    doc.edit_book("rename", |w| {
        let s = w.active_mut();
        s.topic_mut(s.root).unwrap().title = "Changed".into();
        Ok(())
    })
    .unwrap();
    assert!(
        String::from_utf8_lossy(&doc.sheet().fim.as_ref().unwrap().serialize())
            .contains(">Changed</x:name>")
    );
}
#[test]
fn shared_rename_and_conflicts_are_not_merged() {
    let shared = field("F1", "Shared");
    let body = format!(
        "<x:stammdatenschema><x:name>Root</x:name>{}{}</x:stammdatenschema>",
        edge(&shared, "0:1"),
        edge(&shared, "1:*")
    );
    let mut s = FimDocument::parse(&message(&body))
        .unwrap()
        .into_sheet()
        .unwrap();
    assert_eq!(s.topic_count(), 3);
    let first = s.children(s.root)[0];
    let record = s.fim.as_ref().unwrap().record_for_topic(first).unwrap();
    s.apply_fim_change(FimChange::RenameDefinition {
        record,
        name: "Both".into(),
    })
    .unwrap();
    assert!(
        s.children(s.root)
            .iter()
            .all(|t| s.topic(*t).unwrap().title == "Both")
    );
    assert_eq!(
        s.fim
            .as_ref()
            .unwrap()
            .topic_metadata(first)
            .unwrap()
            .usages,
        2
    );
    let conflicting = body.replacen("<x:name>Shared</x:name>", "<x:name>Conflict</x:name>", 1);
    let mut s = FimDocument::parse(&message(&conflicting))
        .unwrap()
        .into_sheet()
        .unwrap();
    assert!(
        s.fim
            .as_ref()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|d| d.code == "conflicting-identity")
    );
    let record = s
        .fim
        .as_ref()
        .unwrap()
        .record_for_topic(s.children(s.root)[0])
        .unwrap();
    let before = s.clone();
    assert!(
        s.apply_fim_change(FimChange::RenameDefinition {
            record,
            name: "Do not merge".into()
        })
        .is_err()
    );
    assert_eq!(s, before);
}
#[test]
fn rejects_unsupported_unsafe_xml_and_preserves_extension_data() {
    for source in [
        b"<!DOCTYPE a><a/>".as_slice(),
        b"\xff\xfe",
        b"<a/>",
        b"<x:bad/>",
        b"<a><b></a>",
        b"<a>&unknown;</a>",
    ] {
        assert!(FimDocument::parse(source).is_err());
    }

    let bytes = sample();
    let source = String::from_utf8(bytes).unwrap().replacen(
        "<x:header>",
        "<ext:unknown xmlns:ext='urn:unknown' flag='yes'><![CDATA[keep]]></ext:unknown><x:header>",
        1,
    );
    let s = FimDocument::parse(source.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    assert_eq!(s.fim.as_ref().unwrap().serialize(), source.as_bytes());
    assert!(
        s.fim
            .as_ref()
            .unwrap()
            .fields(s.fim.as_ref().unwrap().root_record())
            .iter()
            .any(|f| f.namespace == "urn:unknown")
    );
}

#[test]
fn shared_group_properties_and_structure_are_definition_scoped() {
    let group = format!(
        "<x:datenfeldgruppe><x:identifikation><x:id>G1</x:id><x:version>1.0.0</x:version></x:identifikation><x:name>Shared group</x:name>{}</x:datenfeldgruppe>",
        edge(&field("F1", "Shared field"), "0:1")
    );
    let body = format!(
        "<x:stammdatenschema><x:name>Root</x:name>{}{}</x:stammdatenschema>",
        edge(&group, "0:1"),
        edge(&group, "1:*")
    );
    let mut s = FimDocument::parse(&message(&body))
        .unwrap()
        .into_sheet()
        .unwrap();
    let first_group = s.children(s.root)[0];
    let second_group = s.children(s.root)[1];
    let first = s.children(first_group)[0];
    s.apply_fim_change(FimChange::SetCardinality {
        topic: first,
        value: "1:*".into(),
    })
    .unwrap();
    let second = s.children(second_group)[0];
    assert_eq!(
        s.fim
            .as_ref()
            .unwrap()
            .topic_metadata(second)
            .unwrap()
            .cardinality
            .as_deref(),
        Some("1:*")
    );
    s.apply_fim_change(FimChange::Add {
        parent: first_group,
        kind: FimKind::Field,
        name: "New shared draft".into(),
    })
    .unwrap();
    assert_eq!(s.children(first_group).len(), 2);
    assert_eq!(s.children(second_group).len(), 2);
    s.apply_fim_change(FimChange::Move {
        topic: first,
        parent: first_group,
        index: 1,
    })
    .unwrap();
    assert_eq!(s.children(first_group)[1], first);
    assert_eq!(s.children(second_group)[1], second);
    s.apply_fim_change(FimChange::Remove { topic: first })
        .unwrap();
    assert_eq!(s.children(first_group).len(), 1);
    assert_eq!(s.children(second_group).len(), 1);
    s.validate().unwrap();
}

#[test]
fn independent_variant_and_generic_duplicate_undo_preserve_source_closure() {
    let mut doc = Document::new(Workbook::new(sheet()));
    let first = doc.sheet().children(doc.sheet().root)[0];
    let duplicate = doc.edit_sheet("duplicate", |s| s.duplicate(first)).unwrap();
    assert_eq!(doc.sheet().topic_count(), 4);
    doc.edit_sheet("variant", |s| {
        s.apply_fim_change(FimChange::MakeIndependent {
            topic: duplicate,
            identity: "F999".into(),
            version: Some("2.0.0".into()),
        })
        .map_err(|e| thinoda_core::EditError::Invalid(e.to_string()))
    })
    .unwrap();
    assert_eq!(
        doc.sheet()
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(duplicate)
            .unwrap()
            .identity
            .as_deref(),
        Some("F999")
    );
    assert_eq!(
        doc.sheet()
            .fim
            .as_ref()
            .unwrap()
            .topic_metadata(first)
            .unwrap()
            .identity
            .as_deref(),
        Some("F1")
    );
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(doc.sheet().fim.as_ref().unwrap().serialize(), sample());
    assert!(thinoda_core::clipboard::copy_topics(doc.sheet(), &[first]).is_none());
    assert!(doc.sheet().copy_branch_as_sheet(first).is_err());
}

#[test]
fn structured_rule_dependencies_block_dangling_removal() {
    let source = String::from_utf8(sample()).unwrap().replace(
        "<x:skript>",
        "<x:ziel element='F1' name='target'/><x:skript>",
    );
    let mut s = FimDocument::parse(source.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap();
    assert!(
        s.fim
            .as_ref()
            .unwrap()
            .dependencies()
            .iter()
            .any(|d| d.kind == DependencyKind::RuleTarget)
    );
    let first = s.children(s.root)[0];
    let before = s.clone();
    assert!(
        s.apply_fim_change(FimChange::Remove { topic: first })
            .is_err()
    );
    assert_eq!(s, before);
    let target = s
        .fim
        .as_ref()
        .unwrap()
        .fields(s.fim.as_ref().unwrap().root_record())
        .into_iter()
        .find(|f| f.name == "x:ziel")
        .unwrap()
        .record;
    assert!(
        s.apply_fim_change(FimChange::SetAttribute {
            record: target,
            name: "element".into(),
            value: Some("F9999".into())
        })
        .is_err()
    );
}

#[test]
fn native_deserialization_rejects_tampered_arenas_and_bindings() {
    let s = sheet();
    let mut json = serde_json::to_value(&s).unwrap();
    let record = s.fim.as_ref().unwrap().root_record().0.to_string();
    json["fim"]["tree"]["records"][&record]["parent"] = serde_json::json!(999999);
    assert!(serde_json::from_value::<thinoda_core::Sheet>(json).is_err());
    let mut json = serde_json::to_value(&s).unwrap();
    json["fim"]["bindings"][s.root.0.to_string()] = serde_json::json!(999999);
    assert!(serde_json::from_value::<thinoda_core::Sheet>(json).is_err());
}

#[test]
fn generic_move_delete_routes_and_sheet_independence() {
    let mut doc = Document::new(Workbook::new(sheet()));
    let first = doc.sheet().children(doc.sheet().root)[0];
    doc.edit_sheet("reorder", |s| s.reorder(first, 1)).unwrap();
    assert_eq!(doc.sheet().children(doc.sheet().root)[1], first);
    doc.edit_sheet("remove", |s| s.remove_topics(&[first]))
        .unwrap();
    assert!(!doc.sheet().contains(first));
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(doc.sheet().fim.as_ref().unwrap().serialize(), sample());
    let duplicate = doc.duplicate_sheet(0).unwrap();
    doc.set_active(duplicate);
    doc.edit_sheet("name", |s| {
        s.topic_mut(s.root).unwrap().title = "Independent sheet".into();
        Ok(())
    })
    .unwrap();
    assert_eq!(
        doc.workbook().sheets[0].fim.as_ref().unwrap().serialize(),
        sample()
    );
    assert_ne!(
        doc.workbook().sheets[1].fim.as_ref().unwrap().serialize(),
        sample()
    );
}

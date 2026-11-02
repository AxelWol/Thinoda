//! Offline oracle tests using the pinned official archives, never downloaded here.
//! Run with THINODA_FIM_OFFICIAL_DIR pointing at the directory containing
//! xdf2-official-schemas.zip, xdf3-official-schemas.zip and xdf3-official-examples.zip.
use std::{fs, path::PathBuf};
use thinoda_core::{Sheet, fim::*};
use thinoda_io::fim::ExchangeValidator;

const TIME: &str = "2026-11-01T08:44:14+01:00";

fn validator(dialect: Dialect) -> ExchangeValidator {
    let directory = PathBuf::from(
        std::env::var_os("THINODA_FIM_OFFICIAL_DIR")
            .expect("set THINODA_FIM_OFFICIAL_DIR to locally installed pinned official archives"),
    );
    let zip = fs::read(directory.join(if dialect == Dialect::Xdf2 {
        "xdf2-official-schemas.zip"
    } else {
        "xdf3-official-schemas.zip"
    }))
    .unwrap();
    let supplement = (dialect == Dialect::Xdf3)
        .then(|| fs::read(directory.join("xdf3-official-examples.zip")).unwrap());
    ExchangeValidator::from_official_archives(dialect, &zip, supplement.as_deref()).unwrap()
}
fn conformant(validator: &ExchangeValidator, sheet: &Sheet) {
    let xml = sheet.fim.as_ref().unwrap().serialize();
    let report = validator.validate(&xml).unwrap();
    assert!(
        report.is_exchange_conformant(),
        "{report:#?}\n{}",
        String::from_utf8_lossy(&xml)
    );
}
// Small synthetic fixtures authored against the official type sequence.
fn metadata(dialect: Dialect, id: &str) -> String {
    if dialect == Dialect::Xdf2 {
        format!(
            "<identifikation><id>{id}</id></identifikation><name>{id}</name><bezeichnungEingabe>Input {id}</bezeichnungEingabe><beschreibung/><definition/><bezug/><status listVersionID='1.0'><code xmlns=''>inVorbereitung</code></status><schemaelementart><code xmlns=''>RNG</code></schemaelementart><hilfetextEingabe/><hilfetextAusgabe/>"
        )
    } else {
        format!(
            "<identifikation><id>{id}</id></identifikation><name>{id}</name><freigabestatus listVersionID='2022-07-12'><code xmlns=''>2</code></freigabestatus><letzteAenderung>{TIME}</letzteAenderung><bezeichnungEingabe>Input {id}</bezeichnungEingabe><schemaelementart><code xmlns=''>RNG</code></schemaelementart>"
        )
    }
}
fn seed(dialect: Dialect) -> Sheet {
    let field = format!(
        "<datenfeld>{}<feldart><code xmlns=''>input</code></feldart><datentyp><code xmlns=''>text</code></datentyp>{}</datenfeld>",
        metadata(dialect, "F1"),
        if dialect == Dialect::Xdf2 {
            "<praezisierung/><inhalt/>"
        } else {
            "<vorbefuellung><code xmlns=''>keine</code></vorbefuellung>"
        }
    );
    let source = format!(
        "<?xml version='1.0'?><!--synthetic authoring oracle--><xdatenfelder.datenfeldgruppe.0103 xmlns='{}'><header><nachrichtID>local-authoring-test</nachrichtID><erstellungszeitpunkt>{TIME}</erstellungszeitpunkt></header><datenfeldgruppe>{}<struktur><anzahl>1:1</anzahl>{}<enthaelt>{field}</enthaelt></struktur></datenfeldgruppe></xdatenfelder.datenfeldgruppe.0103>",
        dialect.namespace(),
        metadata(dialect, "G1"),
        if dialect == Dialect::Xdf2 {
            "<bezug/>"
        } else {
            ""
        }
    );
    FimDocument::parse(source.as_bytes())
        .unwrap()
        .into_sheet()
        .unwrap()
}
fn new_draft(dialect: Dialect, kind: FimKind, id: &str) -> FimNodeDraft {
    let mut draft = FimNodeDraft::new(dialect, kind);
    draft.identity = Some(id.into());
    draft.name = format!("New {id}");
    if draft.input_label.is_some() {
        draft.input_label = Some(format!("Input {id}"))
    }
    draft
}
fn insert(
    sheet: &mut Sheet,
    parent: RecordId,
    index: usize,
    initial_child: Option<RecordId>,
    draft: FimNodeDraft,
) {
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index,
                initial_child,
                reuse: None,
            },
            draft: Box::new(draft),
            timestamp: TIME.into(),
        })
        .unwrap();
}

fn exercise(dialect: Dialect) {
    let validator = validator(dialect);
    let mut sheet = seed(dialect);
    conformant(&validator, &sheet);
    let parent = sheet
        .fim
        .as_ref()
        .unwrap()
        .record_for_topic(sheet.root)
        .unwrap();
    let seed = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    insert(
        &mut sheet,
        parent,
        0,
        None,
        new_draft(dialect, FimKind::Field, "F2"),
    );
    conformant(&validator, &sheet);
    let mut group = new_draft(dialect, FimKind::Group, "G2");
    group.group_choice = dialect == Dialect::Xdf3;
    insert(&mut sheet, parent, 1, Some(seed), group);
    conformant(&validator, &sheet);
    let mut rule = new_draft(dialect, FimKind::Rule, "R1");
    rule.rule_script = Some("inert_test_script();".into());
    if dialect == Dialect::Xdf3 {
        rule.rule_text = Some("Human-readable rule, not executed".into())
    }
    insert(&mut sheet, parent, 0, None, rule);
    conformant(&validator, &sheet);
    let field = sheet.fim.as_ref().unwrap().node_children(parent)[0];
    insert(
        &mut sheet,
        field,
        0,
        None,
        new_draft(dialect, FimKind::Rule, "R2"),
    );
    conformant(&validator, &sheet);
    let reference = FimCodeListReference {
        identity: (dialect == Dialect::Xdf2).then(|| "C1".into()),
        version: (dialect == Dialect::Xdf2).then(|| "1.0".into()),
        canonical_identification: Some("urn:local:synthetic:codelist".into()),
        canonical_version_uri: Some("urn:local:synthetic:codelist:2026".into()),
        genericode_version: Some("2026".into()),
        code_key: (dialect == Dialect::Xdf3).then(|| "code".into()),
        name_key: (dialect == Dialect::Xdf3).then(|| "name".into()),
        help_key: (dialect == Dialect::Xdf3).then(|| "help".into()),
    };
    let mut c = FimNodeDraft::new(dialect, FimKind::CodeList);
    c.code_list = Some(reference.clone());
    insert(&mut sheet, field, 0, None, c);
    conformant(&validator, &sheet);
    let document = sheet.fim.as_ref().unwrap();
    let c_record = document
        .node_fields(field)
        .iter()
        .find(|f| f.name.ends_with("codelisteReferenz"))
        .map(|f| f.record);
    assert!(
        c_record.is_none(),
        "scoped fields exclude associated definitions"
    );
    let c_record = document
        .records()
        .iter()
        .find(|r| r.kind == FimKind::CodeList)
        .unwrap()
        .record;
    assert_eq!(
        document.node_draft(c_record).unwrap().code_list,
        Some(reference)
    );
    let mut form = document.node_draft(field).unwrap();
    form.cardinality = Some("000:00012".into());
    form.precision.text = Some("Retained lexical precision".into());
    if dialect == Dialect::Xdf3 {
        form.precision.min_length = Some("000".into());
        form.precision.max_length = Some("0012".into());
        form.precision.pattern = Some("[a-z]+".into());
        form.max_size = Some("2048".into());
        form.media_types = vec!["text/plain".into(), "application/pdf".into()];
    }
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Edit {
                record: field,
                independent: false,
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
    let mut form = sheet.fim.as_ref().unwrap().node_draft(c_record).unwrap();
    if dialect == Dialect::Xdf2 {
        form.identity = Some("C2".into());
    } else {
        form.code_list.as_mut().unwrap().canonical_version_uri =
            Some("urn:local:synthetic:codelist:2027".into());
        form.code_list.as_mut().unwrap().genericode_version = Some("2027".into());
    }
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Edit {
                record: c_record,
                independent: true,
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
    // Rule and reference satellites can be deleted without requiring fake topics.
    sheet
        .apply_fim_change(FimChange::RemoveElement { record: c_record })
        .unwrap();
    conformant(&validator, &sheet);
    let rule = sheet
        .fim
        .as_ref()
        .unwrap()
        .node_draft(parent)
        .unwrap()
        .rules[0];
    sheet
        .apply_fim_change(FimChange::RemoveElement { record: rule })
        .unwrap();
    conformant(&validator, &sheet);
    let rule = sheet.fim.as_ref().unwrap().node_draft(field).unwrap().rules[0];
    let mut form = sheet.fim.as_ref().unwrap().node_draft(rule).unwrap();
    form.identity = Some("R3".into());
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Edit {
                record: rule,
                independent: true,
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
    let form = sheet.fim.as_ref().unwrap().node_draft(seed).unwrap();
    let index = sheet.fim.as_ref().unwrap().node_children(parent).len();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Insert {
                parent,
                index,
                initial_child: None,
                reuse: Some(seed),
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
    let reused = *sheet
        .fim
        .as_ref()
        .unwrap()
        .node_children(parent)
        .last()
        .unwrap();
    let mut form = sheet.fim.as_ref().unwrap().node_draft(seed).unwrap();
    form.name = "Shared field".into();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Edit {
                record: seed,
                independent: false,
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
    let mut form = sheet.fim.as_ref().unwrap().node_draft(reused).unwrap();
    form.identity = Some("F3".into());
    form.name = "Independent field".into();
    sheet
        .apply_fim_change(FimChange::AuthorNode {
            target: FimNodeTarget::Edit {
                record: reused,
                independent: true,
            },
            draft: Box::new(form),
            timestamp: TIME.into(),
        })
        .unwrap();
    conformant(&validator, &sheet);
}

#[test]
#[ignore = "requires locally installed, hash-verified official schema archives"]
fn new_xdf2_fields_groups_rules_and_c_reference_validate_official_exchange_xsd() {
    exercise(Dialect::Xdf2);
}
#[test]
#[ignore = "requires locally installed, hash-verified official schema archives"]
fn new_xdf3_fields_nonempty_choice_groups_rules_and_c_reference_validate_official_exchange_xsd() {
    exercise(Dialect::Xdf3);
}

#[test]
#[ignore = "requires locally installed, hash-verified official schema archives"]
fn xdf3_relation_fragment_insertion_validates_official_exchange_xsd() {
    let validator = validator(Dialect::Xdf3);
    let mut sheet = seed(Dialect::Xdf3);
    let document = sheet.fim.as_ref().unwrap();
    let group = document.record_for_topic(sheet.root).unwrap();
    let field = document.node_children(group)[0];
    let anchor = document
        .node_fields(field)
        .into_iter()
        .find(|f| f.parent == Some(field) && f.name == "bezeichnungEingabe")
        .unwrap()
        .record;
    let relation = format!(
        "<relation xmlns='{}'><!--rare property--><praedikat listVersionID='1'><code xmlns=''>ABL</code></praedikat><objekt><id>G1</id></objekt></relation>",
        Dialect::Xdf3.namespace()
    );
    sheet
        .apply_fim_change(FimChange::Batch(vec![FimChange::InsertFragment {
            parent: field,
            before: Some(anchor),
            xml: relation.clone(),
        }]))
        .unwrap();
    let xml = String::from_utf8(sheet.fim.as_ref().unwrap().serialize()).unwrap();
    assert!(xml.contains(&format!("</letzteAenderung>{relation}<bezeichnungEingabe>")));
    conformant(&validator, &sheet);
}

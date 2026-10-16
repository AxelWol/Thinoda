use std::io::{Cursor, Write};
use ymind_core::{
    Boundary, ChildRange, Color, LineShape, Relationship, Sheet, Summary, TextAlign, Topic,
    TopicShape, Vec2, Workbook,
};
use ymind_io::{ExportOptions, Format, detect_format, export_bytes, import_bytes};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn sample() -> Workbook {
    let mut s = Sheet::new("First sheet", "Central & topic");
    s.theme.flexible_floating_topics = true;
    let a = s.add_child(s.root, None, "Branch <A>");
    let b = s.add_child(s.root, None, "Branch B");
    let child = s.add_child(a, None, "Nested");
    {
        let t = s.topic_mut(a).unwrap();
        t.branch_side = Some(ymind_core::BranchSide::Left);
        t.notes = Some("Notes & details".into());
        t.hyperlink = Some("https://example.com/?a=1&b=2".into());
        t.labels = vec!["work".into(), "important".into()];
        t.markers = vec!["priority-1".into(), "flag-red".into()];
        t.collapsed = true;
        t.style.fill = Some(Color::rgb(10, 20, 30));
        t.style.shape = Some(TopicShape::Diamond);
        t.style.bold = Some(true);
        t.style.text_color = Some(Color::rgb(40, 50, 60));
        t.style.border_color = Some(Color::rgb(70, 80, 90));
        t.style.border_width = Some(2.0);
        t.style.font_family = Some("Noto Sans".into());
        t.style.font_size = Some(14.0);
        t.style.italic = Some(false);
        t.style.underline = Some(true);
        t.style.strikethrough = Some(true);
        t.style.align = Some(TextAlign::Left);
        t.style.line_color = Some(Color::rgb(100, 110, 120));
        t.style.line_shape = Some(LineShape::RoundedElbow);
        t.style.line_width = Some(1.5);
    }
    s.topic_mut(child).unwrap().style.font_size = Some(18.0);
    let floating = s.add_floating("Floating", Vec2::new(200.0, -30.0));
    s.topic_mut(floating).unwrap().notes = Some("float note".into());
    let rid = s.alloc_relationship_id();
    s.relationships.push(Relationship {
        id: rid,
        from: a,
        to: b,
        title: "connects".into(),
        color: Some(Color::rgb(1, 2, 3)),
        control_points: Some([Vec2::new(35.0, -80.0), Vec2::new(-45.0, 90.0)]),
    });
    let bid = s.alloc_boundary_id();
    s.boundaries.push(Boundary {
        id: bid,
        parent: s.root,
        range: Some(ChildRange { start: 0, end: 1 }),
        title: Some("Group".into()),
        color: Some(Color::rgb(3, 4, 5)),
    });
    let summary_topic = s.alloc_topic_id();
    s.topics
        .insert(summary_topic, Topic::new(summary_topic, "Summary"));
    let sid = s.alloc_summary_id();
    s.summaries.push(Summary {
        id: sid,
        parent: s.root,
        range: ChildRange { start: 0, end: 1 },
        topic: summary_topic,
    });
    let mut w = Workbook::new(s);
    w.sheets.push(Sheet::new("Second", "Another root"));
    w.active_sheet = 1;
    w
}
fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut c = Cursor::new(Vec::new());
    let mut z = ZipWriter::new(&mut c);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    c.into_inner()
}

fn public_xmind_content(bytes: Vec<u8>) -> Vec<u8> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut content = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("content.json").unwrap(), &mut content)
        .unwrap();
    zip_with(&[("content.json", &content)])
}

#[test]
fn masked_native_archives_contain_only_masked_public_and_private_payloads() {
    let mut source = sample();
    source.metadata.description = Some("PRIVATE42 metadata".into());
    for sheet in &mut source.sheets {
        sheet.title = "PRIVATE42 sheet".into();
        sheet.theme.font_family = Some("PRIVATE42 font".into());
        for topic in sheet.topics.values_mut() {
            topic.title = "PRIVATE42 title".into();
            topic.notes = Some("PRIVATE42 notes".into());
            topic.image = Some("PRIVATE42 image".into());
            topic.hyperlink = Some("https://example.test/PRIVATE42".into());
            topic.labels = vec!["PRIVATE42 label".into()];
            topic.markers = vec!["PRIVATE42 marker".into()];
            topic.style.font_family = Some("PRIVATE42 topic font".into());
        }
        for boundary in &mut sheet.boundaries {
            boundary.title = Some("PRIVATE42 boundary".into());
        }
        for relationship in &mut sheet.relationships {
            relationship.title = "PRIVATE42 relationship".into();
        }
    }
    let original = source.clone();
    let masked = source.masked_copy();
    for format in [Format::YMind, Format::XMind] {
        let bytes = export_bytes(&masked, format, &Default::default()).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            assert!(!entry.name().unwrap().contains("PRIVATE42"));
            let mut contents = String::new();
            std::io::Read::read_to_string(&mut entry, &mut contents).unwrap();
            assert!(
                !contents.contains("PRIVATE42"),
                "leaked into {}",
                entry.name().unwrap()
            );
        }
        assert_eq!(import_bytes(format, &bytes).unwrap(), masked);
        if format == Format::XMind {
            let public = import_bytes(format, &public_xmind_content(bytes)).unwrap();
            assert!(
                !serde_json::to_string(&public)
                    .unwrap()
                    .contains("PRIVATE42")
            );
            assert_eq!(public.sheets.len(), masked.sheets.len());
        }
    }
    assert_eq!(source, original);
}

#[test]
fn multisheet_xmind_annotation_ids_are_globally_unique_and_references_resolve() {
    let mut book = sample();
    book.sheets.push(book.sheets[0].clone());
    let bytes = export_bytes(&book, Format::XMind, &Default::default()).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let content: serde_json::Value =
        serde_json::from_reader(archive.by_name("content.json").unwrap()).unwrap();
    fn walk(
        value: &serde_json::Value,
        ids: &mut std::collections::HashSet<String>,
        references: &mut Vec<String>,
    ) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(id) = object.get("id").and_then(|id| id.as_str()) {
                    assert!(ids.insert(id.to_string()), "duplicate XMind ID {id}");
                }
                for (key, value) in object {
                    if matches!(key.as_str(), "topicId" | "end1Id" | "end2Id") {
                        references.push(value.as_str().unwrap().to_string());
                    }
                    walk(value, ids, references);
                }
            }
            serde_json::Value::Array(array) => {
                for value in array {
                    walk(value, ids, references);
                }
            }
            _ => {}
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut references = Vec::new();
    walk(&content, &mut ids, &mut references);
    assert_eq!(references.len(), 6);
    assert!(references.iter().all(|reference| ids.contains(reference)));
    let restored = import_bytes(Format::XMind, &public_xmind_content(bytes)).unwrap();
    assert_eq!(restored.sheets.len(), 3);
    assert_eq!(restored.sheets[0], restored.sheets[2]);
}

#[test]
fn native_archive_roundtrips_every_model_field() {
    let w = sample();
    let bytes = export_bytes(&w, Format::YMind, &Default::default()).unwrap();
    let restored = import_bytes(Format::YMind, &bytes).unwrap();
    assert_eq!(restored, w);
}
#[test]
fn native_rejects_newer_major_version() {
    let manifest = br#"{"format":"ymind","version":2}"#;
    let bytes = zip_with(&[("manifest.json", manifest)]);
    assert!(matches!(
        import_bytes(Format::YMind, &bytes),
        Err(ymind_io::IoError::NewerVersion(_))
    ));
}
#[test]
fn xmind_export_import_roundtrips_model() {
    let w = sample();
    let bytes = export_bytes(&w, Format::XMind, &Default::default()).unwrap();
    // Only content.json is retained: metadata.ymindWorkbook is a private shortcut and
    // must not hide mistakes in the public XMind representation or parser.
    let public_archive = public_xmind_content(bytes);
    let restored = import_bytes(Format::XMind, &public_archive).unwrap();
    assert_eq!(restored.sheets.len(), 2);
    let sheet = &restored.sheets[0];
    assert_eq!(sheet.root_topic().title, "Central & topic");
    let branch = sheet.topic(sheet.root_topic().children[0]).unwrap();
    assert_eq!(branch.title, "Branch <A>");
    assert_eq!(branch.style.shape, Some(TopicShape::Diamond));
    assert_eq!(branch.style.line_shape, Some(LineShape::RoundedElbow));
    assert_eq!(branch.style.font_family.as_deref(), Some("Noto Sans"));
    assert_eq!(branch.style.font_size, Some(14.0));
    assert_eq!(branch.style.border_width, Some(2.0));
    assert_eq!(branch.style.text_color, Some(Color::rgb(40, 50, 60)));
    assert_eq!(branch.style.border_color, Some(Color::rgb(70, 80, 90)));
    assert_eq!(branch.style.bold, Some(true));
    assert_eq!(branch.style.italic, Some(false));
    assert_eq!(branch.style.underline, Some(true));
    assert_eq!(branch.style.strikethrough, Some(true));
    assert_eq!(branch.style.align, Some(TextAlign::Left));
    assert_eq!(branch.style.line_color, Some(Color::rgb(100, 110, 120)));
    assert_eq!(branch.style.line_width, Some(1.5));
    assert_eq!(sheet.relationships[0].color, Some(Color::rgb(1, 2, 3)));
    assert_eq!(sheet.boundaries[0].color, Some(Color::rgb(3, 4, 5)));
    assert_eq!(sheet.topics[&sheet.summaries[0].topic].title, "Summary");
}

fn xmind_json_parts(book: &Workbook) -> (serde_json::Value, serde_json::Value) {
    let bytes = export_bytes(book, Format::XMind, &Default::default()).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let content = serde_json::from_reader(archive.by_name("content.json").unwrap()).unwrap();
    let metadata = serde_json::from_reader(archive.by_name("metadata.json").unwrap()).unwrap();
    (content, metadata)
}

fn xmind_from_parts(content: &serde_json::Value, metadata: &serde_json::Value) -> Vec<u8> {
    zip_with(&[
        ("content.json", &serde_json::to_vec_pretty(content).unwrap()),
        ("metadata.json", &serde_json::to_vec(metadata).unwrap()),
    ])
}

#[test]
fn xmind_snapshot_is_content_bound_but_ignores_json_whitespace() {
    let mut book = sample();
    let root = book.sheets[0].root;
    book.sheets[0].topic_mut(root).unwrap().todo = Some(true);
    book.metadata.description = Some("Private lossless field".into());
    let (content, metadata) = xmind_json_parts(&book);
    assert_eq!(metadata["ymindContentSha256"].as_str().unwrap().len(), 64);
    let bytes = xmind_from_parts(&content, &metadata);
    assert_eq!(import_bytes(Format::XMind, &bytes).unwrap(), book);
    let mut legacy = metadata.clone();
    legacy.as_object_mut().unwrap().remove("ymindContentSha256");
    assert_eq!(
        import_bytes(Format::XMind, &xmind_from_parts(&content, &legacy)).unwrap(),
        book
    );
}

#[test]
fn external_xmind_edits_win_over_current_and_legacy_private_snapshots() {
    let book = sample();
    let (mut content, metadata) = xmind_json_parts(&book);
    content.as_array_mut().unwrap().truncate(1);
    content[0]["title"] = serde_json::json!("Edited sheet");
    content[0]["rootTopic"]["title"] = serde_json::json!("Edited outside yMind");
    content[0]["rootTopic"]["notes"] = serde_json::json!({"plain":{"content":"new note"}});
    content[0]["rootTopic"]["children"] =
        serde_json::json!({"attached":[{"id":"new","title":"Replacement"}]});
    content[0]["rootTopic"]
        .as_object_mut()
        .unwrap()
        .remove("summaries");
    content[0]["rootTopic"]
        .as_object_mut()
        .unwrap()
        .remove("boundaries");
    content[0]["relationships"] = serde_json::json!([]);
    for legacy in [false, true] {
        let mut meta = metadata.clone();
        if legacy {
            meta.as_object_mut().unwrap().remove("ymindContentSha256");
        }
        let restored = import_bytes(Format::XMind, &xmind_from_parts(&content, &meta)).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.sheets.len(), 1);
        let sheet = &restored.sheets[0];
        assert_eq!(sheet.title, "Edited sheet");
        assert_eq!(sheet.root_topic().title, "Edited outside yMind");
        assert_eq!(sheet.root_topic().notes.as_deref(), Some("new note"));
        assert_eq!(sheet.topic_count(), 2);
        assert_eq!(
            sheet.topic(sheet.root_topic().children[0]).unwrap().title,
            "Replacement"
        );
        assert!(
            sheet.summaries.is_empty()
                && sheet.boundaries.is_empty()
                && sheet.relationships.is_empty()
        );
        let native = export_bytes(&restored, Format::YMind, &Default::default()).unwrap();
        assert_eq!(import_bytes(Format::YMind, &native).unwrap(), restored);
    }
    assert_eq!(book.sheets[0].root_topic().title, "Central & topic");
}

#[test]
fn invalid_private_snapshots_do_not_hide_valid_public_maps_or_broken_public_json() {
    let (content, metadata) = xmind_json_parts(&sample());
    for snapshot in ["{broken", r#"{"sheets":[],"active_sheet":0}"#] {
        let mut meta = metadata.clone();
        meta["ymindWorkbook"] = serde_json::json!(snapshot);
        let restored = import_bytes(Format::XMind, &xmind_from_parts(&content, &meta)).unwrap();
        assert_eq!(restored.sheets[0].root_topic().title, "Central & topic");
    }
    let mut malformed_hash = metadata.clone();
    malformed_hash["ymindContentSha256"] = serde_json::json!(42);
    assert!(import_bytes(Format::XMind, &xmind_from_parts(&content, &malformed_hash)).is_ok());
    let bytes = zip_with(&[
        ("content.json", b"{broken"),
        ("metadata.json", &serde_json::to_vec(&metadata).unwrap()),
    ]);
    assert!(import_bytes(Format::XMind, &bytes).is_err());
}

#[test]
fn xmind_theme_defaults_are_materialized_per_role_and_inline_styles_win() {
    // Field names/role structure follow xmindltd/xmind-viewer example/content.json.
    // This original fixture deliberately uses different values and topic content.
    let content = serde_json::json!([{
        "title":"Theme fixture",
        "theme":{
            "centralTopic":{"properties":{"svg:fill":"#112233","fo:font-size":28,"fo:font-weight":"600"}},
            "mainTopic":{"properties":{"svg:fill":"#445566","fo:font-size":"20pt","fo:color":"#123456"}},
            "subTopic":{"properties":{"fo:font-size":13,"shape-class":"org.xmind.topicShape.underline"}},
            "floatingTopic":{"properties":{"svg:fill":"#778899","fo:font-size":17}},
            "summaryTopic":{"properties":{"svg:fill":"#AABBCC","fo:font-size":19}}
        },
        "rootTopic":{
            "id":"root","title":"Root",
            "style":{"properties":{"fo:font-size":30}},
            "children":{
                "attached":[{"id":"main","title":"Main","style":{"properties":{"svg:fill":"#FF0000","fo:font-size":22}},
                    "children":{"attached":[{"id":"sub","title":"Sub"}]}}],
                "detached":[{"id":"float","title":"Floating","position":{"x":250,"y":80},
                    "children":{"attached":[{"id":"float-child","title":"Floating child"}]}}],
                "summary":[{"id":"summary","title":"Summary",
                    "children":{"attached":[{"id":"summary-child","title":"Summary child"}]}}]
            },
            "summaries":[{"id":"sum","range":"(0, 0)","topicId":"summary"}]
        }
    },{
        "title":"No inherited sheet theme","rootTopic":{"id":"other","title":"Other"}
    }]);
    let bytes = zip_with(&[("content.json", &serde_json::to_vec(&content).unwrap())]);
    let book = import_bytes(Format::XMind, &bytes).unwrap();
    book.validate().unwrap();
    let sheet = &book.sheets[0];
    let named = |title| {
        sheet
            .topics
            .values()
            .find(|topic| topic.title == title)
            .unwrap()
    };
    assert_eq!(named("Root").style.fill, Some(Color::rgb(17, 34, 51)));
    assert_eq!(named("Root").style.font_size, Some(30.0));
    assert_eq!(named("Root").style.bold, Some(true));
    assert_eq!(named("Main").style.fill, Some(Color::rgb(255, 0, 0)));
    assert_eq!(named("Main").style.font_size, Some(22.0));
    assert_eq!(named("Main").style.text_color, Some(Color::rgb(18, 52, 86)));
    assert_eq!(
        named("Floating").style.fill,
        Some(Color::rgb(119, 136, 153))
    );
    assert_eq!(named("Summary").style.fill, Some(Color::rgb(170, 187, 204)));
    for title in ["Sub", "Floating child", "Summary child"] {
        assert_eq!(named(title).style.font_size, Some(13.0), "{title}");
        assert_eq!(
            named(title).style.shape,
            Some(TopicShape::Underline),
            "{title}"
        );
    }
    assert_eq!(book.sheets[1].root_topic().style, Default::default());
    for format in [Format::YMind, Format::XMind] {
        let output = export_bytes(&book, format, &Default::default()).unwrap();
        assert_eq!(import_bytes(format, &output).unwrap(), book);
    }
}

#[test]
fn xmind_export_uses_xmind_metadata_and_public_content() {
    // XMind mutates creator.name while loading; a string creator rejects the archive.
    let bytes = export_bytes(&sample(), Format::XMind, &Default::default()).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let metadata: serde_json::Value =
        serde_json::from_reader(archive.by_name("metadata.json").unwrap()).unwrap();
    assert_eq!(metadata["creator"]["name"], "yMind");
    assert!(metadata["creator"]["version"].is_string());
    assert_eq!(metadata["dataStructureVersion"], "2");
    let manifest: serde_json::Value =
        serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
    assert!(manifest["file-entries"]["content.json"].is_object());
    assert!(manifest["file-entries"]["metadata.json"].is_object());

    let mut content = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("content.json").unwrap(), &mut content)
        .unwrap();
    // Do not let the private lossless workbook bypass parsing the exported XMind tree.
    let public_archive = zip_with(&[("content.json", &content)]);
    let restored = import_bytes(Format::XMind, &public_archive).unwrap();
    assert_eq!(restored.sheets.len(), 2);
    let sheet = &restored.sheets[0];
    assert_eq!(sheet.root_topic().title, "Central & topic");
    let branch = sheet.topic(sheet.root_topic().children[0]).unwrap();
    assert_eq!(branch.title, "Branch <A>");
    assert_eq!(branch.notes.as_deref(), Some("Notes & details"));
    assert_eq!(branch.markers, vec!["priority-1", "flag-red"]);
    assert_eq!(branch.labels, vec!["work", "important"]);
    assert_eq!(sheet.floating.len(), 1);
    assert_eq!(sheet.relationships.len(), 1);
    assert_eq!(sheet.boundaries.len(), 1);
    assert_eq!(sheet.summaries.len(), 1);
}

#[test]
fn xmind_export_uses_public_shape_style_and_element_classes() {
    let bytes = export_bytes(&sample(), Format::XMind, &Default::default()).unwrap();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let content: serde_json::Value =
        serde_json::from_reader(archive.by_name("content.json").unwrap()).unwrap();
    let sheet = &content[0];
    let branch = &sheet["rootTopic"]["children"]["attached"][0];
    let props = &branch["style"]["properties"];
    assert_eq!(props["shape-class"], "org.xmind.topicShape.diamond");
    assert_eq!(
        props["line-class"],
        "org.xmind.branchConnection.roundedElbow"
    );
    assert_eq!(props["fo:font-family"], "Noto Sans");
    assert_eq!(props["fo:font-size"], "14pt");
    assert_eq!(props["fo:text-align"], "left");
    assert_eq!(props["fo:text-decoration"], "underline line-through");
    assert_eq!(props["border-line-width"], "2");
    assert_eq!(sheet["relationships"][0]["class"], "relationship");
    assert_eq!(
        sheet["relationships"][0]["style"]["properties"]["line-color"],
        "#010203"
    );
    assert_eq!(sheet["rootTopic"]["boundaries"][0]["class"], "boundary");
    assert_eq!(sheet["rootTopic"]["summaries"][0]["class"], "summary");
}

#[test]
fn xmind_json_imports_tree_and_metadata() {
    let content=br##"[{"id":"s1","class":"sheet","title":"Fixture","rootTopic":{"id":"r","class":"topic","title":"Root","structureClass":"org.xmind.ui.logic.right","children":{"attached":[{"id":"a","class":"topic","title":"Child","markers":[{"markerId":"priority-2"}],"labels":["label"],"notes":{"plain":{"content":"memo"}},"href":"https://example.org","branch":"folded","style":{"properties":{"svg:fill":"#112233","fo:font-weight":"bold"}}}]}}}]"##;
    let bytes = zip_with(&[("content.json", content)]);
    let w = import_bytes(Format::XMind, &bytes).unwrap();
    assert_eq!(w.sheets[0].title, "Fixture");
    let t = &w.sheets[0].topics[&w.sheets[0].topics[&w.sheets[0].root].children[0]];
    assert_eq!(t.title, "Child");
    assert_eq!(t.notes.as_deref(), Some("memo"));
    assert_eq!(t.markers, vec!["priority-2"]);
    assert_eq!(t.style.fill, Some(Color::rgb(17, 34, 51)));
}
#[test]
fn xmind_json_imports_boundaries_and_summaries() {
    let content = br##"[{"title":"S","rootTopic":{"id":"r","title":"Root","children":{"attached":[{"id":"a","title":"A"}],"summary":[{"id":"sum","title":"A summary"}]},"boundaries":[{"id":"bd","range":"(0, 0)","title":"Group"}],"summaries":[{"id":"sm","range":"(0, 0)","topicId":"sum"}]}}]"##;
    let bytes = zip_with(&[("content.json", content)]);
    let workbook = import_bytes(Format::XMind, &bytes).unwrap();
    let sheet = &workbook.sheets[0];
    assert_eq!(sheet.boundaries.len(), 1);
    assert_eq!(sheet.boundaries[0].title.as_deref(), Some("Group"));
    assert_eq!(sheet.summaries.len(), 1);
    assert_eq!(sheet.topics[&sheet.summaries[0].topic].title, "A summary");
}
#[test]
fn legacy_xmind_xml_imports_topic_titles() {
    let xml=br#"<?xml version="1.0"?><xmap-content xmlns:xlink="http://www.w3.org/1999/xlink"><sheet title="Legacy"><topic structure-class="org.xmind.ui.logic.right" branch="folded"><title>Root</title><children><topics type="attached"><topic><title>Child</title><marker-refs><marker-ref marker-id="priority-3"/></marker-refs><notes><plain>Memo</plain></notes><labels><label>label</label></labels></topic></topics></children></topic></sheet></xmap-content>"#;
    let w = import_bytes(Format::XMind, xml).unwrap();
    let s = &w.sheets[0];
    assert_eq!(s.root_topic().title, "Root");
    assert!(s.root_topic().collapsed);
    assert_eq!(s.structure, ymind_core::Structure::LogicRight);
    let child = &s.topics[&s.root_topic().children[0]];
    assert_eq!(child.title, "Child");
    assert_eq!(child.notes.as_deref(), Some("Memo"));
    assert_eq!(child.labels, vec!["label"]);
    assert_eq!(child.markers, vec!["priority-3"]);
}

#[test]
fn free_mind_opml_markdown_and_text_import() {
    let mm=br#"<map version="1.0.1"><node TEXT="Root"><node TEXT="A" FOLDED="true" LINK="https://a"><icon BUILTIN="full-2"/></node></node></map>"#;
    let w = import_bytes(Format::FreeMind, mm).unwrap();
    assert_eq!(
        w.sheets[0].topics[&w.sheets[0].root_topic().children[0]].title,
        "A"
    );
    let opml=br#"<opml version="2.0"><head><title>Outline</title></head><body><outline text="Root"><outline text="Child" _note="memo"/></outline></body></opml>"#;
    let w = import_bytes(Format::Opml, opml).unwrap();
    assert_eq!(w.sheets[0].root_topic().title, "Root");
    assert_eq!(w.sheets[0].root_topic().children.len(), 1);
    let md = import_bytes(Format::Markdown, b"# Root\n\n## One\n\n### Nested\n").unwrap();
    assert_eq!(md.sheets[0].root_topic().title, "Root");
    let text = import_bytes(Format::PlainText, b"Root\n\tChild\n\t\tNested\n").unwrap();
    assert_eq!(text.sheets[0].topic_count(), 3);
}
#[test]
fn mindmanager_import_from_document_xml() {
    let xml=br#"<ap:Map xmlns:ap="urn:test"><ap:OneTopic><ap:Topic><ap:Text PlainText="Root"/><ap:NotesGroup><ap:NotesXhtmlData PreviewPlainText="root note"/></ap:NotesGroup><ap:SubTopics><ap:Topic><ap:Text PlainText="Child"/><ap:Hyperlink Url="https://example.org"/></ap:Topic></ap:SubTopics></ap:Topic></ap:OneTopic></ap:Map>"#;
    let bytes = zip_with(&[("Document.xml", xml)]);
    let w = import_bytes(Format::MindManager, &bytes).unwrap();
    let sheet = &w.sheets[0];
    assert_eq!(sheet.root_topic().title, "Root");
    assert_eq!(sheet.root_topic().notes.as_deref(), Some("root note"));
    let child = &sheet.topics[&sheet.root_topic().children[0]];
    assert_eq!(child.title, "Child");
    assert_eq!(child.hyperlink.as_deref(), Some("https://example.org"));
}
#[test]
fn svg_png_jpeg_pdf_and_office_exports_are_valid() {
    let w = sample();
    let opts = ExportOptions {
        sheet: Some(0),
        scale: 1.5,
        ..Default::default()
    };
    let svg = export_bytes(&w, Format::Svg, &opts).unwrap();
    let mut reader = quick_xml::Reader::from_reader(svg.as_slice());
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => panic!("bad svg: {e}"),
            _ => {}
        }
        buf.clear();
    }
    let source = String::from_utf8(svg).unwrap();
    assert!(source.contains("Central &amp; topic"));
    assert!(source.contains("Branch &lt;A&gt;"));
    for f in [Format::Png, Format::Jpeg] {
        let bytes = export_bytes(&w, f, &opts).unwrap();
        let img = image::load_from_memory(&bytes).unwrap();
        assert!(img.width() > 0 && img.height() > 0);
    }
    assert!(
        export_bytes(&w, Format::Pdf, &opts)
            .unwrap()
            .starts_with(b"%PDF-")
    );
    for (f, entry) in [
        (Format::Word, "word/document.xml"),
        (Format::Excel, "xl/worksheets/sheet1.xml"),
    ] {
        let bytes = export_bytes(&w, f, &opts).unwrap();
        let mut z = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut file = z.by_name(entry).unwrap();
        let mut text = String::new();
        use std::io::Read;
        file.read_to_string(&mut text).unwrap();
        assert!(text.contains("Central &amp; topic"));
    }
    assert!(
        String::from_utf8(export_bytes(&w, Format::Csv, &opts).unwrap())
            .unwrap()
            .contains("Branch <A>")
    );
}
#[test]
fn detection_supports_paths_and_signatures() {
    assert_eq!(
        Format::from_path(std::path::Path::new("a.xmind")),
        Some(Format::XMind)
    );
    assert_eq!(
        detect_format(std::path::Path::new("unknown"), b"<opml version=\"2.0\"/>"),
        Some(Format::Opml)
    );
    assert_eq!(
        detect_format(std::path::Path::new("unknown"), b"<map version=\"1.0.1\"/>"),
        Some(Format::FreeMind)
    );
}
#[test]
fn importers_do_not_panic_on_truncated_or_random_bytes() {
    let formats = [
        Format::YMind,
        Format::XMind,
        Format::FreeMind,
        Format::Opml,
        Format::Markdown,
        Format::PlainText,
        Format::MindManager,
    ];
    for format in formats {
        for bytes in [
            &[][..],
            b"PK\x03\x04".as_slice(),
            b"<broken".as_slice(),
            b"not a valid document".as_slice(),
            &[0, 1, 2, 255, 128][..],
        ] {
            let result = std::panic::catch_unwind(|| import_bytes(format, bytes));
            assert!(result.is_ok(), "panic for {format:?}");
        }
    }
}

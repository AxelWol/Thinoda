use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::{Value, json};
use thinoda_core::{
    ApproxMeasure, Card, CardKind, Color, Document, InlineParagraph, ListKind, Sheet, Subtree,
    Task, TaskDependency, Topic, TopicId, Vec2, Weekday, Workbook,
    attachments::Attachment,
    board::{MAX_BOARD_COORDINATE, MAX_CANVAS_OBJECTS, MAX_STROKE_WIDTH},
    clipboard::{
        ClipboardPayload, MAX_BOARD_CLIPBOARD_BYTES, TOPIC_CLIPBOARD_PREFIX, copy_topics,
        decode_topic_payload, encode_topic_payload, paste_floating_subtrees, try_copy_topics,
    },
    layout_sheet,
    notes::NoteContent,
    numbering::{NumberFormat, Numbering},
    resources::{MAX_RESOURCE_BYTES, Resource},
    rich_text::MAX_INLINE_FONT_SIZE,
};

struct Fixture {
    source: Sheet,
    payload: ClipboardPayload,
    branch: TopicId,
    child: TopicId,
    grandchild: TopicId,
    other: TopicId,
    image: String,
    attachment: String,
}

fn fixture() -> Fixture {
    let mut source = Sheet::new("Source", "Central");
    let parent = source.add_child(source.root, None, "Uncopied parent");
    let branch = source.add_child(parent, None, "Résumé 🦀\nsecond line");
    let child = source.add_child(branch, None, "Child");
    let grandchild = source.add_child(child, None, "Grandchild");
    let other = source.add_child(source.root, None, "Other");

    let image = source
        .resources
        .insert(Resource::new("picture.png", "image/png", vec![1, 2, 3]).unwrap())
        .unwrap();
    let attachment = source
        .resources
        .insert(Resource::new("example.txt", "text/plain", b"sample bytes".to_vec()).unwrap())
        .unwrap();
    source
        .resources
        .insert(Resource::new("unused.txt", "text/plain", b"unused".to_vec()).unwrap())
        .unwrap();

    let topic = source.topic_mut(branch).unwrap();
    topic.notes = Some(NoteContent::markdown("**Bold**\n\n- item".into()));
    topic.labels = vec!["label".into(), "α".into()];
    topic.markers.push("flag-red".into());
    topic.marker_colors.insert(0, Color::rgb(20, 40, 60));
    topic.hyperlink = Some("https://example.invalid/path".into());
    topic.style.fill = Some(Color::rgba(10, 20, 30, 200));
    topic.style.font_size = Some(18.5);
    topic.style.bold = Some(true);
    topic.style.padding = Some(Vec2::new(4.0, 7.0));
    topic.child_numbering = Some(Numbering {
        format: NumberFormat::UpperAlpha,
        tiered: true,
        start: 3,
    });
    topic.todo = Some(false);
    topic.image = Some(image.clone());
    topic.image_size = Some(Vec2::new(24.0, 36.0));
    source.topic_mut(child).unwrap().attachment = Some(Attachment {
        resource: attachment.clone(),
        name: "example.txt".into(),
    });
    source.topic_mut(grandchild).unwrap().notes = Some("Plain note".into());

    // Saturday is valid under this source calendar, even though the default
    // calendar does not consider it a working day.
    source
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let task = Task {
        start_date: Some("2024-03-02".parse().unwrap()),
        duration_days: Some(3),
        progress_percent: 35,
        priority: Some(2),
    };
    source.set_task(branch, Some(task.clone())).unwrap();
    source.set_task(child, Some(task)).unwrap();
    source.add_task_dependency(branch, child).unwrap();
    source.validate().unwrap();

    let payload = copy_topics(&source, &[branch, other]).unwrap().unwrap();
    assert_eq!(payload.subtrees[0].root_topic().parent, Some(parent));
    Fixture {
        source,
        payload,
        branch,
        child,
        grandchild,
        other,
        image,
        attachment,
    }
}

fn assert_same_branch(source: &Sheet, source_id: TopicId, target: &Sheet, target_id: TopicId) {
    let original = source.topic(source_id).unwrap();
    let mut pasted = target.topic(target_id).unwrap().clone();
    let pasted_children = pasted.children.clone();
    assert_eq!(original.children.len(), pasted_children.len());
    pasted.id = original.id;
    pasted.parent = original.parent;
    pasted.children = original.children.clone();
    assert_eq!(&pasted, original);
    for (&source_child, target_child) in original.children.iter().zip(pasted_children) {
        assert_same_branch(source, source_child, target, target_child);
    }
}

#[test]
fn nested_topics_resources_and_tasks_roundtrip_and_paste() {
    let fixture = fixture();
    let source_before = fixture.source.clone();
    let payload_before = fixture.payload.clone();
    assert_eq!(fixture.payload.subtrees[0].resources.entries.len(), 2);
    assert!(fixture.payload.subtrees[1].resources.is_empty());

    let encoded = encode_topic_payload(&fixture.payload).unwrap();
    assert!(encoded.starts_with("THINODA-TOPIC-CLIPBOARD-1\n"));
    let decoded = decode_topic_payload(&encoded).unwrap();
    assert_eq!(decoded, payload_before);
    let windows = encoded.replacen('\n', "\r\n", 1);
    assert_eq!(decode_topic_payload(&windows).unwrap(), payload_before);
    assert_eq!(fixture.source, source_before);
    assert_eq!(fixture.payload, payload_before);

    let mut destination = Sheet::new("Destination", "Root");
    destination
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let mut document = Document::new(Workbook::new(destination));
    let before = document.workbook().clone();
    let roots = document
        .edit_sheet("Paste topics", |sheet| {
            sheet.paste(sheet.root, &decoded.subtrees)
        })
        .unwrap();
    assert_eq!(roots.len(), 2);
    let destination = document.sheet();
    assert_same_branch(&fixture.source, fixture.branch, destination, roots[0]);
    assert_same_branch(&fixture.source, fixture.other, destination, roots[1]);
    let child = destination.children(roots[0])[0];
    let grandchild = destination.children(child)[0];
    assert_same_branch(&fixture.source, fixture.child, destination, child);
    assert_same_branch(&fixture.source, fixture.grandchild, destination, grandchild);
    assert_eq!(
        destination.scheduling.tasks[&roots[0]],
        fixture.source.scheduling.tasks[&fixture.branch]
    );
    assert_eq!(
        destination.scheduling.tasks[&child],
        fixture.source.scheduling.tasks[&fixture.child]
    );
    assert_eq!(
        destination.scheduling.dependencies,
        vec![TaskDependency {
            predecessor: roots[0],
            successor: child,
        }]
    );
    assert_eq!(
        destination.resources.get(&fixture.image).unwrap().bytes(),
        &[1, 2, 3]
    );
    assert_eq!(
        destination
            .resources
            .get(&fixture.attachment)
            .unwrap()
            .bytes(),
        b"sample bytes"
    );
    assert_eq!(destination.resources.entries.len(), 2);
    destination.validate().unwrap();
    assert!(document.undo());
    assert_eq!(document.workbook(), &before);
    assert!(document.redo());
    document.workbook().validate().unwrap();
}

#[test]
fn blank_text_and_reused_ids_in_independent_subtrees_are_preserved() {
    for title in ["", "   ", "\t"] {
        let mut source = Sheet::new("Source", "Root");
        let branch = source.add_child(source.root, None, title);
        let payload = copy_topics(&source, &[branch]).unwrap().unwrap();
        assert!(payload.text.trim().is_empty());
        assert_eq!(
            decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap(),
            payload
        );
        let mut empty_text = payload;
        empty_text.text.clear();
        assert_eq!(
            decode_topic_payload(&encode_topic_payload(&empty_text).unwrap()).unwrap(),
            empty_text
        );
    }

    let mut first = Sheet::new("First", "Root");
    let first_root = first.add_child(first.root, None, "One");
    let first_child = first.add_child(first_root, None, "Child A");
    first.set_task(first_root, Some(Task::default())).unwrap();
    first.set_task(first_child, Some(Task::default())).unwrap();
    first.add_task_dependency(first_root, first_child).unwrap();
    let mut second = Sheet::new("Second", "Root");
    let second_root = second.add_child(second.root, None, "Two");
    let second_child = second.add_child(second_root, None, "Child B");
    second.set_task(second_root, Some(Task::default())).unwrap();
    second
        .set_task(second_child, Some(Task::default()))
        .unwrap();
    second
        .add_task_dependency(second_root, second_child)
        .unwrap();
    assert_eq!(first_root, second_root);
    assert_eq!(first_child, second_child);
    let payload = ClipboardPayload {
        subtrees: vec![
            first.copy_subtree(first_root).unwrap(),
            second.copy_subtree(second_root).unwrap(),
        ],
        text: " \r\n\t".into(),
    };
    let decoded = decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap();
    assert_eq!(decoded, payload);
    let mut destination = Sheet::new("Destination", "Root");
    let roots = destination
        .paste(destination.root, &decoded.subtrees)
        .unwrap();
    assert_ne!(roots[0], roots[1]);
    assert_eq!(destination.topic(roots[0]).unwrap().title, "One");
    assert_eq!(destination.topic(roots[1]).unwrap().title, "Two");
    for root in roots {
        let child = destination.children(root)[0];
        assert!(
            destination
                .scheduling
                .dependencies
                .contains(&TaskDependency {
                    predecessor: root,
                    successor: child,
                })
        );
    }
    destination.validate().unwrap();

    // A root's old parent can also have the same ID as a topic in a different
    // subtree; ownership is local to each independently remapped subtree.
    let mut with_external_parent = payload;
    with_external_parent.subtrees[1].topics[0].parent = Some(first_root);
    // This parent is inside the second subtree and must still be rejected.
    assert!(encode_topic_payload(&with_external_parent).is_err());
    with_external_parent.subtrees[1].topics[0].parent = Some(first.root);
    with_external_parent
        .subtrees
        .push(first.copy_subtree(first.root).unwrap());
    assert_eq!(
        decode_topic_payload(&encode_topic_payload(&with_external_parent).unwrap()).unwrap(),
        with_external_parent
    );
}

#[test]
fn subtrees_can_independently_carry_the_same_embedded_resource() {
    let mut source = Sheet::new("Source", "Root");
    let first = source.add_child(source.root, None, "First");
    let second = source.add_child(source.root, None, "Second");
    let reference = source
        .resources
        .insert(Resource::new("shared.png", "image/png", vec![1, 2, 3]).unwrap())
        .unwrap();
    source.topic_mut(first).unwrap().image = Some(reference.clone());
    source.topic_mut(second).unwrap().image = Some(reference.clone());
    source.validate().unwrap();

    let payload = copy_topics(&source, &[first, second]).unwrap().unwrap();
    assert_eq!(payload.subtrees[0].resources.entries.len(), 1);
    assert_eq!(payload.subtrees[1].resources.entries.len(), 1);
    let decoded = decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap();
    assert_eq!(decoded, payload);

    let mut destination = Sheet::new("Destination", "Root");
    let roots = destination
        .paste(destination.root, &decoded.subtrees)
        .unwrap();
    assert_eq!(destination.resources.entries.len(), 1);
    for root in roots {
        assert_eq!(
            destination.topic(root).unwrap().image.as_deref(),
            Some(reference.as_str())
        );
    }
    destination.validate().unwrap();
}

#[test]
fn topic_identity_uses_the_existing_full_u64_model_domain() {
    let root = TopicId(0);
    let child = TopicId(u64::MAX);
    let anchor = TopicId(42);
    let mut root_topic = Topic::new(root, "Zero");
    root_topic.parent = Some(anchor);
    root_topic.children.push(child);
    let mut child_topic = Topic::new(child, "Maximum");
    child_topic.parent = Some(root);
    let mut anchor_topic = Topic::new(anchor, "Anchor");
    anchor_topic.children.push(root);
    let mut source = Sheet::new("Source", "unused");
    source.root = anchor;
    source.topics = [
        (anchor, anchor_topic),
        (root, root_topic),
        (child, child_topic),
    ]
    .into();
    source.scheduling.tasks = [(root, Task::default()), (child, Task::default())].into();
    source.scheduling.dependencies.push(TaskDependency {
        predecessor: root,
        successor: child,
    });
    source.validate().unwrap();
    let payload = copy_topics(&source, &[root]).unwrap().unwrap();
    let decoded = decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap();
    assert_eq!(decoded, payload);
    let mut target = Sheet::new("Destination", "Root");
    let pasted = target.paste(target.root, &decoded.subtrees).unwrap()[0];
    let pasted_child = target.children(pasted)[0];
    assert!(target.scheduling.dependencies.contains(&TaskDependency {
        predecessor: pasted,
        successor: pasted_child,
    }));
    target.validate().unwrap();
}

#[test]
fn floating_card_formatting_is_retained_and_checked() {
    let mut source = Sheet::new("Source", "Root");
    let title = "A🦀é\nSecond";
    let card = source
        .add_card(
            title,
            Vec2::new(80.0, 120.0),
            Card::new(CardKind::Sticky, Vec2::new(160.0, 100.0)),
        )
        .unwrap();
    let topic = source.topic_mut(card).unwrap();
    topic
        .inline_text
        .update_range(title, 0..3, |style| {
            style.bold = Some(true);
            style.font_family = Some("Sans".into());
        })
        .unwrap();
    topic.inline_text.paragraphs.push(InlineParagraph {
        index: 1,
        list: ListKind::Numbered,
    });
    source.validate().unwrap();
    let payload = copy_topics(&source, &[card]).unwrap().unwrap();
    let decoded = decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap();
    assert_eq!(decoded, payload);
    let mut destination = Sheet::new("Destination", "Root");
    let roots = paste_floating_subtrees(&mut destination, &decoded.subtrees, Vec2::ZERO).unwrap();
    assert_eq!(
        destination.topic(roots[0]).unwrap().inline_text,
        source.topic(card).unwrap().inline_text
    );
    assert_eq!(
        destination.topic(roots[0]).unwrap().card,
        source.topic(card).unwrap().card
    );
    destination.validate().unwrap();

    let mut invalid = payload;
    invalid.subtrees[0].topics[0].inline_text.runs[0].end = 100;
    assert!(encode_topic_payload(&invalid).is_err());
    let wire = format!(
        "{TOPIC_CLIPBOARD_PREFIX}{}",
        serde_json::to_string(&invalid).unwrap()
    );
    assert!(decode_topic_payload(&wire).is_err());
}

fn reject_case(
    original: &ClipboardPayload,
    description: &str,
    mutate: impl FnOnce(&mut ClipboardPayload),
) {
    let mut bad = original.clone();
    mutate(&mut bad);
    assert!(
        encode_topic_payload(&bad).is_err(),
        "encoder accepted {description}"
    );
    let wire = format!(
        "{TOPIC_CLIPBOARD_PREFIX}{}",
        serde_json::to_string(&bad).unwrap()
    );
    let mut destination = Sheet::new("Destination", "Root");
    let before = destination.clone();
    let result = catch_unwind(AssertUnwindSafe(|| {
        decode_topic_payload(&wire).and_then(|decoded| {
            destination
                .paste(destination.root, &decoded.subtrees)
                .map_err(|error| error.to_string())
        })
    }));
    assert!(result.is_ok(), "{description} caused a panic");
    assert!(result.unwrap().is_err(), "decoder accepted {description}");
    assert_eq!(destination, before, "{description} mutated the destination");
}

#[test]
fn rejects_malformed_ownership_tasks_rich_fields_and_resources_atomically() {
    let fixture = fixture();
    let base = &fixture.payload;
    reject_case(base, "empty selection", |p| p.subtrees.clear());
    reject_case(base, "missing root", |p| {
        p.subtrees[0].root = TopicId(999_999);
    });
    reject_case(base, "duplicate topic ID", |p| {
        let duplicate = p.subtrees[0].topics[0].clone();
        p.subtrees[0].topics.push(duplicate);
    });
    reject_case(base, "root parent inside subtree", |p| {
        p.subtrees[0].topics[0].parent = Some(fixture.child);
    });
    reject_case(base, "missing child", |p| {
        p.subtrees[0].topics[0].children.push(TopicId(999_999));
    });
    reject_case(base, "duplicate child", |p| {
        p.subtrees[0].topics[0].children.push(fixture.child);
    });
    reject_case(base, "mismatched parent", |p| {
        p.subtrees[0].topics[1].parent = Some(fixture.grandchild);
    });
    reject_case(base, "orphan descendant", |p| {
        p.subtrees[0].topics[1].parent = None;
    });
    reject_case(base, "unreachable cycle", |p| {
        let mut a = Topic::new(TopicId(900_001), "A");
        let mut b = Topic::new(TopicId(900_002), "B");
        a.parent = Some(b.id);
        a.children.push(b.id);
        b.parent = Some(a.id);
        b.children.push(a.id);
        p.subtrees[0].topics.extend([a, b]);
    });
    reject_case(base, "task outside subtree", |p| {
        p.subtrees[0]
            .tasks
            .insert(TopicId(999_999), Task::default());
    });
    reject_case(base, "dependency outside subtree", |p| {
        p.subtrees[0].dependencies[0].successor = TopicId(999_999);
    });
    reject_case(base, "dependency without task", |p| {
        p.subtrees[0].tasks.remove(&fixture.child);
    });
    reject_case(base, "self dependency", |p| {
        p.subtrees[0].dependencies[0].successor = fixture.branch;
    });
    reject_case(base, "duplicate dependency", |p| {
        let duplicate = p.subtrees[0].dependencies[0];
        p.subtrees[0].dependencies.push(duplicate);
    });
    reject_case(base, "dependency cycle", |p| {
        p.subtrees[0].dependencies.push(TaskDependency {
            predecessor: fixture.child,
            successor: fixture.branch,
        });
    });
    reject_case(base, "invalid task progress", |p| {
        p.subtrees[0]
            .tasks
            .get_mut(&fixture.branch)
            .unwrap()
            .progress_percent = 101;
    });
    reject_case(base, "invalid numbering", |p| {
        p.subtrees[0].topics[0]
            .child_numbering
            .as_mut()
            .unwrap()
            .start = 0;
    });
    reject_case(base, "missing marker", |p| {
        p.subtrees[0].topics[0]
            .marker_colors
            .insert(99, Color::WHITE);
    });
    reject_case(base, "invalid position", |p| {
        p.subtrees[0].topics[0].position = Some(Vec2::new(2_000_000.0, 0.0));
    });
    reject_case(base, "invalid image size", |p| {
        p.subtrees[0].topics[0].image_size = Some(Vec2::new(-1.0, 20.0));
    });
    reject_case(base, "invalid card ownership", |p| {
        p.subtrees[0].topics[0].card = Some(Card::new(CardKind::Sticky, Vec2::new(100.0, 80.0)));
    });
    reject_case(base, "bad attachment name", |p| {
        p.subtrees[0].topics[1].attachment.as_mut().unwrap().name = "../file".into();
    });
    reject_case(base, "missing embedded image", |p| {
        p.subtrees[0].topics[0].image = Some("thinoda-resource:missing".into());
    });
    reject_case(base, "resource present only in another subtree", |p| {
        let (first, rest) = p.subtrees.split_at_mut(1);
        let (id, resource) = first[0].resources.entries.pop_first().unwrap();
        rest[0].resources.entries.insert(id, resource);
    });
    reject_case(base, "incorrect resource digest", |p| {
        let entries = &mut p.subtrees[0].resources.entries;
        let (_, resource) = entries.pop_first().unwrap();
        entries.insert("0".repeat(64), resource);
    });
    reject_case(base, "invalid resource version", |p| {
        p.subtrees[0].resources.version = 2;
    });

    // These in-memory errors would otherwise be erased by Serde's skipped
    // empty resource store or its null encoding of non-finite floats.
    let mut invalid = base.clone();
    invalid.subtrees[1].resources.version = 2;
    assert!(encode_topic_payload(&invalid).is_err());
    let mut invalid = base.clone();
    invalid.subtrees[0].topics[0].style.font_size = Some(f32::NAN);
    assert!(encode_topic_payload(&invalid).is_err());
}

#[test]
fn rejects_unknown_fields_duplicate_keys_and_invalid_json() {
    let fixture = fixture();
    let original = serde_json::to_value(&fixture.payload).unwrap();
    let mut cases: Vec<(&str, Value)> = Vec::new();
    let mut root = original.clone();
    root["future"] = json!(null);
    cases.push(("payload field", root));
    let mut subtree = original.clone();
    subtree["subtrees"][0]["future"] = json!({"value": 1});
    cases.push(("subtree field", subtree));
    let mut topic = original.clone();
    topic["subtrees"][0]["topics"][0]["future"] = json!("lost");
    cases.push(("topic field", topic));
    let mut style = original.clone();
    style["subtrees"][0]["topics"][0]["style"]["future"] = json!(false);
    cases.push(("style field", style));
    let mut task = original.clone();
    task["subtrees"][0]["tasks"][fixture.branch.0.to_string()]["future"] = json!(1);
    cases.push(("task field", task));
    let mut note = original.clone();
    note["subtrees"][0]["topics"][0]["notes"]["future"] = json!(true);
    cases.push(("note field", note));
    let mut resource = original;
    resource["subtrees"][0]["resources"]["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["future"] = json!(true);
    cases.push(("resource field", resource));
    for (description, value) in cases {
        let wire = format!("{TOPIC_CLIPBOARD_PREFIX}{value}");
        assert!(
            decode_topic_payload(&wire).is_err(),
            "accepted unknown {description}"
        );
    }

    for keys in [r#""1":{},"1":{}"#, r#""1":{},"01":{}"#] {
        let json = format!(
            r#"{{"subtrees":[{{"root":1,"topics":[{{"id":1,"title":"Root"}}],"tasks":{{{keys}}}}}],"text":""}}"#
        );
        assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{json}")).is_err());
    }
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{{")).is_err());
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{{}} trailing")).is_err());
    let deep = format!("{}0{}", "[".repeat(150), "]".repeat(150));
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{deep}")).is_err());
}

#[test]
fn rejects_invalid_resource_encodings_and_note_versions() {
    let fixture = fixture();
    let original = serde_json::to_value(&fixture.payload).unwrap();
    for data in ["", "invalid base64!"] {
        let mut malformed = original.clone();
        malformed["subtrees"][0]["resources"]["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()["data"] = json!(data);
        assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{malformed}")).is_err());
    }
    let mut invalid_name = original.clone();
    invalid_name["subtrees"][0]["resources"]["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["name"] = json!("../bad");
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{invalid_name}")).is_err());
    let mut invalid_media = original.clone();
    invalid_media["subtrees"][0]["resources"]["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["media_type"] = json!("invalid");
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{invalid_media}")).is_err());
    let mut invalid_note = original.clone();
    invalid_note["subtrees"][0]["topics"][0]["notes"]["format"] = json!("markdown-v999");
    assert!(decode_topic_payload(&format!("{TOPIC_CLIPBOARD_PREFIX}{invalid_note}")).is_err());

    // A resource can be oversized even when the complete wire text fits the
    // clipboard limit; reject it before base64 decoding allocates its bytes.
    let mut oversized = original;
    oversized["subtrees"][0]["resources"]["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["data"] = json!("A".repeat(MAX_RESOURCE_BYTES.div_ceil(3) * 4 + 1));
    let wire = format!("{TOPIC_CLIPBOARD_PREFIX}{oversized}");
    assert!(wire.len() < MAX_BOARD_CLIPBOARD_BYTES);
    assert!(decode_topic_payload(&wire).is_err());
}

#[test]
fn version_and_serialized_size_boundaries_include_the_prefix() {
    let mut payload = fixture().payload;
    payload.text.clear();
    let base_len = encode_topic_payload(&payload).unwrap().len();
    payload.text = "x".repeat(MAX_BOARD_CLIPBOARD_BYTES - base_len);
    let exactly = encode_topic_payload(&payload).unwrap();
    assert_eq!(exactly.len(), MAX_BOARD_CLIPBOARD_BYTES);
    assert_eq!(decode_topic_payload(&exactly).unwrap(), payload);
    assert!(decode_topic_payload(&exactly.replacen('\n', "\r\n", 1)).is_err());

    payload.text.pop();
    let windows = encode_topic_payload(&payload)
        .unwrap()
        .replacen('\n', "\r\n", 1);
    assert_eq!(windows.len(), MAX_BOARD_CLIPBOARD_BYTES);
    assert_eq!(decode_topic_payload(&windows).unwrap(), payload);

    payload.text.push_str("xx");
    assert!(
        encode_topic_payload(&payload)
            .unwrap_err()
            .contains("16 MiB")
    );
    let oversized = "x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1);
    assert!(
        decode_topic_payload(&oversized)
            .unwrap_err()
            .contains("16 MiB")
    );
    assert!(
        decode_topic_payload("THINODA-TOPIC-CLIPBOARD-2\n{}")
            .unwrap_err()
            .contains("supported")
    );
    assert!(decode_topic_payload("THINODA-TOPIC-CLIPBOARD-0\r\n{}").is_err());
    assert!(decode_topic_payload("THINODA-BOARD-CLIPBOARD-1\n{}").is_err());

    // The limit applies to serialized bytes, not just the input string length.
    payload.text = "\n".repeat(MAX_BOARD_CLIPBOARD_BYTES / 2);
    assert!(
        encode_topic_payload(&payload)
            .unwrap_err()
            .contains("16 MiB")
    );
}

fn chain_payload(depth: usize) -> ClipboardPayload {
    let topics = (0..=depth)
        .map(|index| {
            let mut topic = Topic::new(TopicId(index as u64 + 1), format!("Topic {index}"));
            if index > 0 {
                topic.parent = Some(TopicId(index as u64));
            }
            if index < depth {
                topic.children.push(TopicId(index as u64 + 2));
            }
            topic
        })
        .collect();
    ClipboardPayload {
        subtrees: vec![Subtree {
            root: TopicId(1),
            topics,
            tasks: Default::default(),
            dependencies: Vec::new(),
            resources: Default::default(),
        }],
        text: "Nested topics\n".into(),
    }
}

fn star_subtree(count: usize) -> Subtree {
    assert!(count > 0);
    let root = TopicId(1);
    let mut first = Topic::new(root, "Root");
    first.children = (2..=count as u64).map(TopicId).collect();
    let mut topics = vec![first];
    topics.extend((2..=count as u64).map(|id| {
        let mut topic = Topic::new(TopicId(id), "Child");
        topic.parent = Some(root);
        topic
    }));
    Subtree {
        root,
        topics,
        tasks: Default::default(),
        dependencies: Vec::new(),
        resources: Default::default(),
    }
}

fn sheet_for_subtree(subtree: Subtree) -> Sheet {
    let mut sheet = Sheet::new("Source", "Root");
    sheet.root = subtree.root;
    sheet.topics = subtree
        .topics
        .into_iter()
        .map(|topic| (topic.id, topic))
        .collect();
    sheet
}

#[test]
fn copy_preflight_checks_limits_before_building_recursive_outlines() {
    let at_depth = sheet_for_subtree(chain_payload(256).subtrees.remove(0));
    let expected = copy_topics(&at_depth, &[at_depth.root]).unwrap().unwrap();
    assert_eq!(
        try_copy_topics(&at_depth, &[at_depth.root]).unwrap(),
        Some(expected)
    );
    for depth in [257, 5_000] {
        let sheet = sheet_for_subtree(chain_payload(depth).subtrees.remove(0));
        let error = try_copy_topics(&sheet, &[sheet.root]).unwrap_err();
        assert!(error.contains("depth"), "{depth}: {error}");
        let error = copy_topics(&sheet, &[sheet.root]).unwrap_err();
        assert!(error.contains("depth"), "{depth}: {error}");
    }

    let at_count = sheet_for_subtree(star_subtree(MAX_CANVAS_OBJECTS));
    let copied = try_copy_topics(&at_count, &[at_count.root])
        .unwrap()
        .unwrap();
    assert_eq!(copied.subtrees[0].topics.len(), MAX_CANVAS_OBJECTS);
    let excessive = sheet_for_subtree(star_subtree(MAX_CANVAS_OBJECTS + 1));
    assert!(
        try_copy_topics(&excessive, &[excessive.root])
            .unwrap_err()
            .contains(&MAX_CANVAS_OBJECTS.to_string())
    );

    let mut oversized = Sheet::new("Source", "");
    let root = oversized.root;
    oversized.topic_mut(root).unwrap().title = "x".repeat(MAX_BOARD_CLIPBOARD_BYTES);
    assert!(
        try_copy_topics(&oversized, &[oversized.root])
            .unwrap_err()
            .contains("16 MiB")
    );
}

#[test]
fn topic_font_size_bounds_preserve_small_and_large_imports() {
    let base = fixture().payload;
    for size in [
        f32::from_bits(1),
        0.125,
        5.5,
        6.0,
        128.0,
        MAX_INLINE_FONT_SIZE,
    ] {
        let mut payload = base.clone();
        payload.subtrees[0].topics[0].style.font_size = Some(size);
        let encoded = encode_topic_payload(&payload).unwrap();
        assert_eq!(
            decode_topic_payload(&encoded).unwrap(),
            payload,
            "font size {size} must round-trip without clamping"
        );
    }
    let above_limit = f32::from_bits(MAX_INLINE_FONT_SIZE.to_bits() + 1);
    for size in [-12.0, -0.0, 0.0, above_limit, f32::MAX] {
        reject_case(&base, &format!("font size {size}"), |payload| {
            payload.subtrees[0].topics[0].style.font_size = Some(size);
        });
    }
    for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut payload = base.clone();
        payload.subtrees[0].topics[0].style.font_size = Some(size);
        assert!(encode_topic_payload(&payload).is_err());
    }
}

#[test]
fn other_topic_style_dimensions_use_broad_board_safety_bounds() {
    let base = fixture().payload;
    let mut broad = base.clone();
    let style = &mut broad.subtrees[0].topics[0].style;
    style.border_width = Some(50.0);
    style.line_width = Some(200.0);
    style.width = Some(20_000.0);
    style.padding = Some(Vec2::new(2_000.0, MAX_BOARD_COORDINATE));
    let encoded = encode_topic_payload(&broad).unwrap();
    assert_eq!(decode_topic_payload(&encoded).unwrap(), broad);

    type SetDimension = fn(&mut Topic, f32);
    let dimensions: [(&str, f32, SetDimension); 5] = [
        ("border width", MAX_STROKE_WIDTH, |topic, value| {
            topic.style.border_width = Some(value)
        }),
        ("line width", MAX_STROKE_WIDTH, |topic, value| {
            topic.style.line_width = Some(value)
        }),
        ("width", MAX_BOARD_COORDINATE, |topic, value| {
            topic.style.width = Some(value)
        }),
        (
            "horizontal padding",
            MAX_BOARD_COORDINATE,
            |topic, value| topic.style.padding = Some(Vec2::new(value, 1.0)),
        ),
        ("vertical padding", MAX_BOARD_COORDINATE, |topic, value| {
            topic.style.padding = Some(Vec2::new(1.0, value))
        }),
    ];
    for (name, limit, set) in dimensions {
        let above_limit = f32::from_bits(limit.to_bits() + 1);
        for value in [0.0, limit] {
            let mut payload = base.clone();
            set(&mut payload.subtrees[0].topics[0], value);
            assert_eq!(
                decode_topic_payload(&encode_topic_payload(&payload).unwrap()).unwrap(),
                payload,
                "{name} {value} should be retained"
            );
        }
        for value in [-1.0, above_limit, f32::MAX] {
            reject_case(&base, &format!("{name} {value}"), |payload| {
                set(&mut payload.subtrees[0].topics[0], value);
            });
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut payload = base.clone();
            set(&mut payload.subtrees[0].topics[0], value);
            assert!(encode_topic_payload(&payload).is_err(), "{name}");
        }
    }
}

#[test]
fn total_topic_limit_includes_all_independently_remapped_subtrees() {
    let half = MAX_CANVAS_OBJECTS / 2;
    let payload = ClipboardPayload {
        subtrees: vec![star_subtree(half), star_subtree(half)],
        text: String::new(),
    };
    assert_eq!(payload.subtrees[0].root, payload.subtrees[1].root);
    let encoded = encode_topic_payload(&payload).unwrap();
    assert_eq!(decode_topic_payload(&encoded).unwrap(), payload);

    let mut excessive = payload;
    excessive.subtrees.push(star_subtree(1));
    assert!(
        encode_topic_payload(&excessive)
            .unwrap_err()
            .contains(&MAX_CANVAS_OBJECTS.to_string())
    );
    let wire = format!(
        "{TOPIC_CLIPBOARD_PREFIX}{}",
        serde_json::to_string(&excessive).unwrap()
    );
    assert!(wire.len() < MAX_BOARD_CLIPBOARD_BYTES);
    assert!(
        decode_topic_payload(&wire)
            .unwrap_err()
            .contains(&MAX_CANVAS_OBJECTS.to_string())
    );
}

#[test]
fn subtree_depth_bound_blocks_deep_graphs_before_recursive_layout() {
    // Depth is measured in edges from the copied root: 256 is allowed.
    let at_limit = chain_payload(256);
    let decoded = decode_topic_payload(&encode_topic_payload(&at_limit).unwrap()).unwrap();
    assert_eq!(decoded, at_limit);
    let mut target = Sheet::new("Destination", "Root");
    target.paste(target.root, &decoded.subtrees).unwrap();
    target.validate().unwrap();
    assert_eq!(
        layout_sheet(&target, &ApproxMeasure, false).topics.len(),
        258
    );

    for depth in [257, 5_000] {
        let excessive = chain_payload(depth);
        let encoded = catch_unwind(|| encode_topic_payload(&excessive));
        assert!(encoded.is_ok(), "encoding depth {depth} panicked");
        assert!(encoded.unwrap().unwrap_err().contains("depth"));
        let wire = format!(
            "{TOPIC_CLIPBOARD_PREFIX}{}",
            serde_json::to_string(&excessive).unwrap()
        );
        let mut destination = Sheet::new("Destination", "Root");
        let before = destination.clone();
        let decoded = catch_unwind(AssertUnwindSafe(|| {
            decode_topic_payload(&wire).and_then(|payload| {
                destination
                    .paste(destination.root, &payload.subtrees)
                    .map_err(|error| error.to_string())
            })
        }));
        assert!(decoded.is_ok(), "decoding depth {depth} panicked");
        assert!(decoded.unwrap().unwrap_err().contains("depth"));
        assert_eq!(destination, before);
    }
}

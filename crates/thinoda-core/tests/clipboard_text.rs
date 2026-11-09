use thinoda_core::{
    Sheet, Subtree, Topic, TopicId,
    clipboard::{
        ClipboardPayload, MAX_BOARD_CLIPBOARD_BYTES, MAX_TOPIC_CLIPBOARD_DEPTH,
        MAX_TOPIC_CLIPBOARD_TOPICS, resolve_paste, subtrees_from_text, validate_topic_destination,
    },
    outline::{OutlineNode, add_outline, build_tree, parse_indented_text},
};

// Frozen from the pre-checkpoint parser so lexer regressions are not shared by
// the reference and the new clipboard path.
fn old_parse_indented_text(text: &str) -> Vec<OutlineNode> {
    let mut items = Vec::new();
    let mut unit = None;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut width = 0usize;
        for character in line.chars() {
            match character {
                '\t' => width += 4,
                ' ' => width += 1,
                _ => break,
            }
        }
        if width > 0 && unit.is_none() {
            unit = Some(width);
        }
        let mut title = line.trim();
        for bullet in ["- [ ] ", "- [x] ", "- ", "* ", "+ ", "• ", "– "] {
            if let Some(rest) = title.strip_prefix(bullet) {
                title = rest;
                break;
            }
        }
        if let Some(pos) = title.find(". ")
            && pos > 0
            && pos <= 3
            && title[..pos].chars().all(|c| c.is_ascii_digit())
        {
            title = &title[pos + 2..];
        }
        items.push((width, title.trim().to_string()));
    }
    let unit = unit.unwrap_or(4).max(1);
    let levels: Vec<_> = items
        .into_iter()
        .map(|(width, title)| (width.div_ceil(unit), title))
        .collect();
    build_tree(&levels)
}

fn old_plain_text_subtrees(text: &str) -> Vec<Subtree> {
    old_parse_indented_text(text)
        .iter()
        .map(|node| {
            let mut sheet = Sheet::new("tmp", node.title.clone());
            let root = sheet.root;
            for child in &node.children {
                add_outline(&mut sheet, root, child);
            }
            sheet.copy_subtree(root).unwrap()
        })
        .collect()
}

fn nested_text(depth: usize) -> String {
    let mut text = "Root\n".to_string();
    for level in 1..=depth {
        text.extend(std::iter::repeat_n('\t', level));
        text.push_str("Child\n");
    }
    text
}

#[test]
fn plain_text_preserves_legacy_bullets_indentation_and_level_jumps() {
    for text in [
        "",
        "Root\n\tA\n\t\tA1\n\tB\n",
        "  - Root\r\n    - [x] Child\r\n      12. Detail\r\n+ Another\r\n",
        "• 🦀\n\t– Item\n\t\t* More\n\n  + Sibling\n",
        "\tFirst\n \tSecond\n     - Third\n  1. Fourth",
        "  \n- [ ] \n1. \n. \n",
    ] {
        assert_eq!(parse_indented_text(text), old_parse_indented_text(text));
        assert_eq!(
            subtrees_from_text(text).unwrap(),
            old_plain_text_subtrees(text)
        );
    }

    for mut pattern in 0..4usize.pow(6) {
        let mut text = String::new();
        for index in 0..6 {
            text.push_str(&" ".repeat(pattern % 4));
            pattern /= 4;
            text.push_str(match index % 3 {
                0 => "- Item\n",
                1 => "2. Detail\n",
                _ => "• More\n",
            });
        }
        assert_eq!(parse_indented_text(&text), old_parse_indented_text(&text));
        assert_eq!(
            subtrees_from_text(&text).unwrap(),
            old_plain_text_subtrees(&text),
            "{text:?}"
        );
    }
}

#[test]
fn plain_text_limits_actual_depth_not_raw_indentation() {
    let at_limit = nested_text(MAX_TOPIC_CLIPBOARD_DEPTH);
    let subtrees = subtrees_from_text(&at_limit).unwrap();
    assert_eq!(subtrees[0].topics.len(), MAX_TOPIC_CLIPBOARD_DEPTH + 1);
    let too_deep = nested_text(MAX_TOPIC_CLIPBOARD_DEPTH + 1);
    assert!(subtrees_from_text(&too_deep).unwrap_err().contains("depth"));
    assert!(
        resolve_paste(None, Some(&nested_text(4_096)))
            .unwrap_err()
            .contains("depth")
    );

    let jump = format!("Root\n Child\n{}Jump\n Sibling", " ".repeat(600));
    let subtrees = subtrees_from_text(&jump).unwrap();
    assert_eq!(subtrees, old_plain_text_subtrees(&jump));
    assert_eq!(subtrees[0].topics.len(), 4);
}

#[test]
fn plain_text_enforces_exact_byte_and_topic_limits() {
    let exact = "é".repeat(MAX_BOARD_CLIPBOARD_BYTES / 2);
    let subtrees = subtrees_from_text(&exact).unwrap();
    assert_eq!(
        subtrees[0].root_topic().title.len(),
        MAX_BOARD_CLIPBOARD_BYTES
    );
    let mut excessive = exact;
    excessive.push('x');
    assert!(
        subtrees_from_text(&excessive)
            .unwrap_err()
            .contains("16 MiB")
    );
    assert!(
        resolve_paste(None, Some(&" ".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1)))
            .unwrap_err()
            .contains("16 MiB")
    );

    let exact = "Topic\n".repeat(MAX_TOPIC_CLIPBOARD_TOPICS);
    let subtrees = subtrees_from_text(&exact).unwrap();
    assert_eq!(subtrees.len(), MAX_TOPIC_CLIPBOARD_TOPICS);
    assert!(subtrees.iter().all(|subtree| subtree.root == TopicId(1)));
    let excessive = format!("{exact}One more");
    assert!(
        subtrees_from_text(&excessive)
            .unwrap_err()
            .contains(&MAX_TOPIC_CLIPBOARD_TOPICS.to_string())
    );

    let star = format!(
        "Root\n{}",
        "\tChild\n".repeat(MAX_TOPIC_CLIPBOARD_TOPICS - 1)
    );
    let subtrees = subtrees_from_text(&star).unwrap();
    assert_eq!(subtrees.len(), 1);
    assert_eq!(subtrees[0].topics.len(), MAX_TOPIC_CLIPBOARD_TOPICS);
    assert_eq!(
        subtrees[0].topics[0].children.len(),
        MAX_TOPIC_CLIPBOARD_TOPICS - 1
    );
}

#[test]
fn cached_topic_content_is_validated_before_it_is_cloned_for_paste() {
    let mut subtree = subtrees_from_text(&nested_text(MAX_TOPIC_CLIPBOARD_DEPTH))
        .unwrap()
        .remove(0);
    let parent = subtree.topics.last().unwrap().id;
    let id = TopicId(parent.0 + 1);
    let mut child = Topic::new(id, "Too deep");
    child.parent = Some(parent);
    subtree.topics.last_mut().unwrap().children.push(id);
    subtree.topics.push(child);
    let payload = ClipboardPayload {
        subtrees: vec![subtree],
        text: "Copied".into(),
    };

    for text in [None, Some("Copied")] {
        assert!(
            resolve_paste(Some(&payload), text)
                .unwrap_err()
                .contains("depth")
        );
    }
    let external = resolve_paste(Some(&payload), Some("External")).unwrap();
    assert_eq!(external[0].root_topic().title, "External");

    let oversized = ClipboardPayload {
        text: "x".repeat(MAX_BOARD_CLIPBOARD_BYTES + 1),
        ..payload
    };
    assert!(
        resolve_paste(Some(&oversized), None)
            .unwrap_err()
            .contains("16 MiB")
    );
    let external = resolve_paste(Some(&oversized), Some("External")).unwrap();
    assert_eq!(external[0].root_topic().title, "External");
}

#[test]
fn destination_depth_counts_summary_ownership_and_rejects_cycles() {
    let subtrees = subtrees_from_text("One\n\tTwo").unwrap();
    for (descendants, valid) in [(253, true), (254, false)] {
        let mut sheet = Sheet::new("Map", "Root");
        let covered = sheet.add_child(sheet.root, None, "Covered");
        let mut parent = sheet.add_summary(&[covered], "Summary").unwrap();
        for _ in 0..descendants {
            parent = sheet.add_child(parent, None, "Ancestor");
        }
        let result = validate_topic_destination(&sheet, parent, &subtrees);
        assert_eq!(result.is_ok(), valid, "{result:?}");
    }

    let mut malformed = Sheet::new("Map", "Root");
    let child = malformed.add_child(malformed.root, None, "Child");
    malformed.topic_mut(malformed.root).unwrap().parent = Some(child);
    assert!(
        validate_topic_destination(&malformed, child, &subtrees)
            .unwrap_err()
            .contains("cyclic")
    );
}

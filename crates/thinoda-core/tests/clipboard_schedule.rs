use thinoda_core::{
    ApproxMeasure, Document, MAX_SCHEDULED_TASKS, MAX_TASK_DEPENDENCIES, Sheet, Subtree, Task,
    TaskDependency, Topic, TopicId, Vec2, Weekday, Workbook,
    clipboard::{
        copy_board_selection, copy_topics, decode_board_payload, encode_board_payload,
        paste_board_selection, paste_floating_subtrees,
    },
    layout_sheet,
};

fn scheduled_branch() -> Subtree {
    let mut source = Sheet::new("Source", "Root");
    let parent = source.add_child(source.root, None, "Parent");
    let child = source.add_child(parent, None, "Child");
    source
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let task = Task {
        start_date: Some("2024-03-02".parse().unwrap()),
        ..Task::default()
    };
    source.set_task(parent, Some(task.clone())).unwrap();
    source.set_task(child, Some(task)).unwrap();
    source.add_task_dependency(parent, child).unwrap();
    copy_topics(&source, &[parent])
        .unwrap()
        .unwrap()
        .subtrees
        .remove(0)
}

#[test]
fn topic_paste_checks_destination_calendar_atomically_and_supports_undo() {
    let subtree = scheduled_branch();
    let mut sheet = Sheet::new("Destination", "Root");
    let before = sheet.clone();
    let error = sheet
        .paste(sheet.root, std::slice::from_ref(&subtree))
        .unwrap_err();
    assert!(error.to_string().contains("nonworking"));
    assert_eq!(sheet, before);

    sheet
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let mut document = Document::new(Workbook::new(sheet));
    let before = document.workbook().clone();
    let roots = document
        .edit_sheet("Paste", |sheet| {
            sheet.paste(sheet.root, std::slice::from_ref(&subtree))
        })
        .unwrap();
    let child = document.sheet().children(roots[0])[0];
    assert_eq!(document.sheet().scheduling.tasks.len(), 2);
    assert_eq!(
        document.sheet().scheduling.dependencies,
        vec![TaskDependency {
            predecessor: roots[0],
            successor: child,
        }]
    );
    document.workbook().validate().unwrap();
    assert!(document.undo());
    assert_eq!(document.workbook(), &before);
    assert!(document.redo());
    document.workbook().validate().unwrap();
}

#[test]
fn floating_topic_paste_uses_destination_calendar_and_rejects_mismatches() {
    let subtree = scheduled_branch();
    let mut sheet = Sheet::new("Destination", "Root");
    let before = sheet.clone();
    let error = paste_floating_subtrees(&mut sheet, std::slice::from_ref(&subtree), Vec2::ZERO)
        .unwrap_err();
    assert!(error.contains("nonworking"));
    assert_eq!(sheet, before);

    sheet
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let pasted =
        paste_floating_subtrees(&mut sheet, std::slice::from_ref(&subtree), Vec2::ZERO).unwrap();
    assert_eq!(pasted.len(), 1);
    assert_eq!(sheet.scheduling.tasks.len(), 2);
    assert_eq!(sheet.scheduling.dependencies.len(), 1);
    sheet.validate().unwrap();
}

#[test]
fn topic_paste_honors_destination_calendar_exceptions() {
    let subtree = scheduled_branch();
    let saturday = "2024-03-02".parse().unwrap();
    for floating in [false, true] {
        let mut allowed = Sheet::new("Destination", "Root");
        allowed
            .scheduling
            .calendar
            .exceptions
            .insert(saturday, true);
        if floating {
            paste_floating_subtrees(&mut allowed, std::slice::from_ref(&subtree), Vec2::ZERO)
                .unwrap();
        } else {
            allowed
                .paste(allowed.root, std::slice::from_ref(&subtree))
                .unwrap();
        }
        allowed.validate().unwrap();

        let mut denied = Sheet::new("Destination", "Root");
        denied
            .scheduling
            .calendar
            .working_weekdays
            .insert(Weekday::Saturday);
        denied
            .scheduling
            .calendar
            .exceptions
            .insert(saturday, false);
        let before = denied.clone();
        let rejected = if floating {
            paste_floating_subtrees(&mut denied, std::slice::from_ref(&subtree), Vec2::ZERO)
        } else {
            denied
                .paste(denied.root, std::slice::from_ref(&subtree))
                .map_err(|error| error.to_string())
        };
        assert!(rejected.unwrap_err().contains("nonworking"));
        assert_eq!(denied, before);
    }
}

#[test]
fn mixed_board_copy_does_not_export_the_source_calendar() {
    let mut source = Sheet::new("Source", "Root");
    let topic = source.add_floating("Scheduled", Vec2::new(50.0, 50.0));
    source
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    source
        .scheduling
        .calendar
        .exceptions
        .insert("2024-12-25".parse().unwrap(), false);
    source
        .set_task(
            topic,
            Some(Task {
                start_date: Some("2024-03-02".parse().unwrap()),
                ..Task::default()
            }),
        )
        .unwrap();
    let layout = layout_sheet(&source, &ApproxMeasure, false);
    let payload = copy_board_selection(&source, &layout, &[topic], &[])
        .unwrap()
        .unwrap();
    let encoded = encode_board_payload(&payload).unwrap();
    assert!(!encoded.contains("2024-12-25"));
    let payload = decode_board_payload(&encoded).unwrap();

    let mut destination = Sheet::new("Destination", "Root");
    let before = destination.clone();
    let error = paste_board_selection(&mut destination, &payload, Vec2::ZERO).unwrap_err();
    assert!(error.contains("nonworking"));
    assert_eq!(destination, before);
    destination
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let calendar = destination.scheduling.calendar.clone();
    let pasted = paste_board_selection(&mut destination, &payload, Vec2::ZERO).unwrap();
    assert_eq!(pasted.topics.len(), 1);
    assert_eq!(destination.scheduling.calendar, calendar);
    destination.validate().unwrap();
}

fn add_scheduled_topics(sheet: &mut Sheet, count: usize) -> Vec<TopicId> {
    let root = sheet.root;
    (0..count)
        .map(|index| {
            let id = TopicId(index as u64 + 2);
            let mut topic = Topic::new(id, "Task");
            topic.parent = Some(root);
            sheet.topic_mut(root).unwrap().children.push(id);
            sheet.topics.insert(id, topic);
            sheet.scheduling.tasks.insert(id, Task::default());
            id
        })
        .collect()
}

#[test]
fn topic_paste_rejects_destination_schedule_limits_before_mutation() {
    let subtree = scheduled_branch();
    let mut task_limit = Sheet::new("Destination", "Root");
    task_limit
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    add_scheduled_topics(&mut task_limit, MAX_SCHEDULED_TASKS - 2);
    task_limit
        .paste(task_limit.root, std::slice::from_ref(&subtree))
        .unwrap();
    assert_eq!(task_limit.scheduling.tasks.len(), MAX_SCHEDULED_TASKS);
    let before = task_limit.clone();
    let error = task_limit
        .paste(task_limit.root, std::slice::from_ref(&subtree))
        .unwrap_err();
    assert!(error.to_string().contains("too many scheduled tasks"));
    assert_eq!(task_limit, before);

    let mut dependency_limit = Sheet::new("Destination", "Root");
    dependency_limit
        .scheduling
        .calendar
        .working_weekdays
        .insert(Weekday::Saturday);
    let ids = add_scheduled_topics(&mut dependency_limit, 201);
    for (index, predecessor) in ids.iter().enumerate() {
        for successor in &ids[index + 1..] {
            if dependency_limit.scheduling.dependencies.len() == MAX_TASK_DEPENDENCIES - 1 {
                break;
            }
            dependency_limit
                .scheduling
                .dependencies
                .push(TaskDependency {
                    predecessor: *predecessor,
                    successor: *successor,
                });
        }
    }
    assert_eq!(
        dependency_limit.scheduling.dependencies.len(),
        MAX_TASK_DEPENDENCIES - 1
    );
    dependency_limit
        .paste(dependency_limit.root, std::slice::from_ref(&subtree))
        .unwrap();
    assert_eq!(
        dependency_limit.scheduling.dependencies.len(),
        MAX_TASK_DEPENDENCIES
    );
    let before = dependency_limit.clone();
    let error = dependency_limit
        .paste(dependency_limit.root, std::slice::from_ref(&subtree))
        .unwrap_err();
    assert!(error.to_string().contains("too many task dependencies"));
    assert_eq!(dependency_limit, before);
}

//! Original typed workflow examples using the shared scene hierarchy.

use crate::{
    CanvasConnector, CanvasObject, CanvasObjectId, CanvasObjectKind, Color, ConnectorRoute,
    Endpoint, Rect, ShapeKind, Sheet, SheetRootMode, Vec2, Workbook,
    semantic::{
        AutomationMode, Boundary, BoundaryKind, Participant, ParticipantKind, ProcessScope,
        SemanticBinding, SemanticEntity, SemanticRelation, Step, StepKind, UseCase,
    },
    stencil_object::StencilRef,
};

struct Example {
    lanes: &'static [&'static str],
    participant_kinds: &'static [ParticipantKind],
    accountable_role: &'static str,
    escalation_role: Option<(&'static str, usize, usize)>,
    intent: &'static str,
    steps: &'static [ExampleStep],
    links: &'static [ExampleLink],
}

struct ExampleStep {
    label: &'static str,
    lane: usize,
    kind: StepKind,
    rationale: Option<&'static str>,
}

struct ExampleLink {
    from: usize,
    to: usize,
    label: &'static str,
}

const EXAMPLES: [Example; 3] = [
    Example {
        lanes: &[
            "Customer (human)",
            "Support (human)",
            "Service/API (software)",
        ],
        participant_kinds: &[
            ParticipantKind::Human,
            ParticipantKind::Human,
            ParticipantKind::Software,
        ],
        accountable_role: "Support specialist",
        escalation_role: None,
        intent: "Complete a request with a human handoff and a clear result.",
        steps: &[
            ExampleStep {
                label: "Submit request",
                lane: 0,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Check request",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Request complete?",
                lane: 1,
                kind: StepKind::ExclusiveDecision,
                rationale: Some("Support confirms the request is complete before fulfilment."),
            },
            ExampleStep {
                label: "Fulfil request",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Request more information",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Receive result",
                lane: 0,
                kind: StepKind::Task,
                rationale: None,
            },
        ],
        links: &[
            ExampleLink {
                from: 0,
                to: 1,
                label: "Request",
            },
            ExampleLink {
                from: 1,
                to: 2,
                label: "",
            },
            ExampleLink {
                from: 2,
                to: 3,
                label: "Yes",
            },
            ExampleLink {
                from: 2,
                to: 4,
                label: "No",
            },
            ExampleLink {
                from: 4,
                to: 0,
                label: "Additional information",
            },
            ExampleLink {
                from: 3,
                to: 5,
                label: "Result",
            },
        ],
    },
    Example {
        lanes: &[
            "Requester (human)",
            "Reviewer (human)",
            "Approval API (software)",
            "Support (human)",
        ],
        participant_kinds: &[
            ParticipantKind::Human,
            ParticipantKind::Human,
            ParticipantKind::Api,
            ParticipantKind::Human,
        ],
        accountable_role: "Access reviewer",
        escalation_role: Some(("Support specialist", 3, 9)),
        intent: "Grant or refuse access through policy, human review and timeout escalation.",
        steps: &[
            ExampleStep {
                label: "Submit request",
                lane: 0,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Evaluate policy",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Human review required?",
                lane: 2,
                kind: StepKind::ExclusiveDecision,
                rationale: Some("Policy exceptions require review by a human reviewer."),
            },
            ExampleStep {
                label: "Review request",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Review outcome?",
                lane: 1,
                kind: StepKind::ExclusiveDecision,
                rationale: Some("The reviewer explicitly approves or refuses access."),
            },
            ExampleStep {
                label: "Grant access",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Refuse access",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Notify requester",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Review timed out?",
                lane: 2,
                kind: StepKind::ExclusiveDecision,
                rationale: Some(
                    "A timeout without a reviewer response enters the escalation path.",
                ),
            },
            ExampleStep {
                label: "Escalate to support",
                lane: 3,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Record escalation",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
        ],
        links: &[
            ExampleLink {
                from: 0,
                to: 1,
                label: "Access request",
            },
            ExampleLink {
                from: 1,
                to: 2,
                label: "",
            },
            ExampleLink {
                from: 2,
                to: 3,
                label: "Yes",
            },
            ExampleLink {
                from: 2,
                to: 5,
                label: "No",
            },
            ExampleLink {
                from: 3,
                to: 4,
                label: "",
            },
            ExampleLink {
                from: 4,
                to: 5,
                label: "Approve",
            },
            ExampleLink {
                from: 4,
                to: 6,
                label: "Refuse",
            },
            ExampleLink {
                from: 5,
                to: 7,
                label: "Granted",
            },
            ExampleLink {
                from: 6,
                to: 7,
                label: "Refused",
            },
            ExampleLink {
                from: 3,
                to: 8,
                label: "Timeout check",
            },
            ExampleLink {
                from: 8,
                to: 9,
                label: "Yes",
            },
            ExampleLink {
                from: 8,
                to: 4,
                label: "No",
            },
            ExampleLink {
                from: 9,
                to: 10,
                label: "Escalation",
            },
            ExampleLink {
                from: 10,
                to: 7,
                label: "Support result",
            },
        ],
    },
    Example {
        lanes: &[
            "Monitor (software)",
            "On-call (human)",
            "Recovery API (software)",
        ],
        participant_kinds: &[
            ParticipantKind::Software,
            ParticipantKind::Human,
            ParticipantKind::Api,
        ],
        accountable_role: "On-call responder",
        escalation_role: None,
        intent: "Restore service with an explicit retry and human escalation path.",
        steps: &[
            ExampleStep {
                label: "Detect incident",
                lane: 0,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Assess impact",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Restore service",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Recovery verified?",
                lane: 2,
                kind: StepKind::ExclusiveDecision,
                rationale: Some("Recovery is verified before the incident is considered resolved."),
            },
            ExampleStep {
                label: "Verify recovery",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Retry recovery",
                lane: 2,
                kind: StepKind::Task,
                rationale: None,
            },
            ExampleStep {
                label: "Escalate incident",
                lane: 1,
                kind: StepKind::Task,
                rationale: None,
            },
        ],
        links: &[
            ExampleLink {
                from: 0,
                to: 1,
                label: "Alert",
            },
            ExampleLink {
                from: 1,
                to: 2,
                label: "Recovery request",
            },
            ExampleLink {
                from: 2,
                to: 3,
                label: "",
            },
            ExampleLink {
                from: 3,
                to: 4,
                label: "Yes",
            },
            ExampleLink {
                from: 3,
                to: 5,
                label: "No",
            },
            ExampleLink {
                from: 5,
                to: 3,
                label: "Retry",
            },
            ExampleLink {
                from: 3,
                to: 6,
                label: "Repeated failure",
            },
        ],
    },
];

const LEGEND: &str = "Typed editable example. Participants own process scopes; steps, exchanges and use-case links are semantic.";
const INK: Color = Color::rgb(38, 48, 62);

fn shape(
    sheet: &mut Sheet,
    label: String,
    rect: Rect,
    fill: Color,
    header: bool,
) -> CanvasObjectId {
    let mut object = CanvasObject::shape(ShapeKind::Rectangle, rect);
    if let CanvasObjectKind::Shape { label: value, .. } = &mut object.kind {
        *value = label;
    }
    object.style.fill = Some(fill);
    object.style.stroke = Some(INK);
    object.style.text_color = Some(INK);
    if header {
        object.stencil = Some(StencilRef::new("thinoda.general", "header-frame-v2"));
    }
    object.id = sheet.alloc_canvas_object_id();
    let id = object.id;
    sheet.canvas.objects.push(object);
    id
}

pub(super) fn build(id: &str, title: &str, tr: &dyn Fn(&'static str) -> String) -> Workbook {
    let index = match id {
        "software-handoff" => 0,
        "software-approval" => 1,
        "software-incident" => 2,
        _ => unreachable!("unregistered Software template: {id}"),
    };
    let example = &EXAMPLES[index];
    let mut sheet = Sheet::new(title, "");
    sheet.root_mode = SheetRootMode::Hidden;
    let legend_y = 55.0 + example.lanes.len() as f32 * 195.0 + 25.0;
    let root = shape(
        &mut sheet,
        title.to_owned(),
        Rect::from_min_size(Vec2::ZERO, Vec2::new(1100.0, legend_y + 75.0)),
        Color::rgb(238, 241, 245),
        true,
    );
    sheet.canvas.scene.containers.insert(root);
    let mut lanes = Vec::new();
    let mut participants = Vec::new();
    let mut processes = Vec::new();
    let role = sheet
        .add_semantic_entity(SemanticEntity::Role(crate::semantic::Role {
            name: tr(example.accountable_role),
            description: String::new(),
        }))
        .expect("built-in accountable role is valid");
    let escalation_role = example.escalation_role.map(|(name, _, _)| {
        sheet
            .add_semantic_entity(SemanticEntity::Role(crate::semantic::Role {
                name: tr(name),
                description: String::new(),
            }))
            .expect("built-in escalation role is valid")
    });
    for (row, label) in example.lanes.iter().enumerate() {
        let participant = sheet
            .add_semantic_entity(SemanticEntity::Participant(Participant {
                kind: example.participant_kinds[row],
                name: tr(label),
                description: String::new(),
            }))
            .expect("built-in Software participant is valid");
        let process_frame = shape(
            &mut sheet,
            tr(label),
            Rect::from_min_size(
                Vec2::new(20.0, 55.0 + row as f32 * 195.0),
                Vec2::new(1060.0, 180.0),
            ),
            Color::rgb(227, 238, 249),
            true,
        );
        sheet.canvas.scene.containers.insert(process_frame);
        sheet.canvas.scene.parents.insert(process_frame, root);
        let process = sheet
            .add_semantic_entity(SemanticEntity::Process(ProcessScope {
                name: tr(label),
                description: String::new(),
                black_box: false,
            }))
            .expect("built-in Software process is valid");
        sheet
            .bind_semantic_occurrence(process_frame, SemanticBinding::Entity(process))
            .expect("built-in process scope matches its canvas occurrence");
        sheet
            .add_semantic_relation(SemanticRelation::ProcessOwnership {
                process,
                owner: participant,
            })
            .expect("built-in process owner is a participant");
        let lane_frame = shape(
            &mut sheet,
            tr(
                if matches!(
                    example.participant_kinds[row],
                    ParticipantKind::Human | ParticipantKind::Team
                ) {
                    "Human responsibility"
                } else {
                    "Software responsibility"
                },
            ),
            Rect::from_min_size(
                Vec2::new(32.0, 90.0 + row as f32 * 195.0),
                Vec2::new(1036.0, 135.0),
            ),
            Color::rgb(246, 249, 252),
            true,
        );
        sheet.canvas.scene.containers.insert(lane_frame);
        sheet.canvas.scene.parents.insert(lane_frame, process_frame);
        let lane_entity = sheet
            .add_semantic_entity(SemanticEntity::Lane(crate::semantic::Lane {
                name: tr(
                    if matches!(
                        example.participant_kinds[row],
                        ParticipantKind::Human | ParticipantKind::Team
                    ) {
                        "Human responsibility"
                    } else {
                        "Software responsibility"
                    },
                ),
                description: String::new(),
            }))
            .expect("built-in lane is valid");
        sheet
            .bind_semantic_occurrence(lane_frame, SemanticBinding::Entity(lane_entity))
            .expect("built-in lane matches its canvas occurrence");
        let row_role = if row == 1 {
            Some((role, example.accountable_role))
        } else {
            example
                .escalation_role
                .filter(|(_, role_lane, _)| *role_lane == row)
                .and_then(|(name, _, _)| escalation_role.map(|role| (role, name)))
        };
        let performer = row_role
            .map(|(role, _)| crate::semantic::PerformerRef::Role(role))
            .unwrap_or(crate::semantic::PerformerRef::Participant(participant));
        sheet
            .add_semantic_relation(SemanticRelation::LanePerformer {
                lane: lane_entity,
                performer,
            })
            .expect("built-in lane performer exists");
        if let Some((role_id, role_name)) = row_role {
            let role_object = shape(
                &mut sheet,
                tr(role_name),
                Rect::from_min_size(
                    Vec2::new(820.0, 110.0 + row as f32 * 195.0),
                    Vec2::new(220.0, 42.0),
                ),
                Color::rgb(255, 255, 255),
                false,
            );
            sheet.canvas.scene.parents.insert(role_object, lane_frame);
            sheet
                .bind_semantic_occurrence(role_object, SemanticBinding::Entity(role_id))
                .expect("built-in role matches its canvas occurrence");
            sheet
                .add_semantic_relation(SemanticRelation::RoleFulfillment {
                    participant,
                    role: role_id,
                })
                .expect("built-in role is fulfilled by a participant");
        }
        lanes.push(lane_frame);
        participants.push(participant);
        processes.push(process);
    }
    let mut steps = Vec::new();
    let mut semantic_steps = Vec::new();
    let mut lane_columns = vec![0usize; example.lanes.len()];
    for step in example.steps {
        let process_index = step.lane;
        let column = lane_columns[process_index];
        lane_columns[process_index] += 1;
        let node = shape(
            &mut sheet,
            tr(step.label),
            Rect::from_min_size(
                Vec2::new(
                    48.0 + column as f32 * 145.0,
                    130.0 + process_index as f32 * 195.0,
                ),
                Vec2::new(135.0, 58.0),
            ),
            Color::rgb(255, 255, 255),
            false,
        );
        if step.kind == StepKind::ExclusiveDecision
            && let Some(object) = sheet.canvas.object_mut(node)
        {
            object.kind = CanvasObjectKind::Shape {
                shape: ShapeKind::Diamond,
                label: tr(step.label),
            };
        }
        sheet
            .canvas
            .scene
            .parents
            .insert(node, lanes[process_index]);
        let automation = match example.participant_kinds[process_index] {
            ParticipantKind::Human | ParticipantKind::Team | ParticipantKind::Unspecified => {
                AutomationMode::Manual
            }
            ParticipantKind::Software | ParticipantKind::Api => AutomationMode::Automated,
        };
        let semantic_step = sheet
            .add_semantic_entity(SemanticEntity::Step(Step {
                kind: step.kind,
                automation,
                name: tr(step.label),
                description: String::new(),
                rationale: step.rationale.map(tr),
            }))
            .expect("built-in Software step is valid");
        sheet
            .bind_semantic_occurrence(node, SemanticBinding::Entity(semantic_step))
            .expect("built-in step label matches its canvas occurrence");
        steps.push(node);
        semantic_steps.push(semantic_step);
    }
    for link in example.links {
        let semantic_label = if link.label.is_empty() {
            String::new()
        } else {
            tr(link.label)
        };
        let mut connector = CanvasConnector::new(
            Endpoint::Object(steps[link.from]),
            Endpoint::Object(steps[link.to]),
        );
        connector.route = ConnectorRoute::Elbow;
        if !link.label.is_empty() {
            connector.label = tr(link.label);
        }
        connector.color = Some(INK);
        let mut object = CanvasObject::connector(connector);
        object.id = sheet.alloc_canvas_object_id();
        let connector_id = object.id;
        sheet.canvas.scene.parents.insert(connector_id, root);
        sheet.canvas.objects.push(object);
        let from_lane = example.steps[link.from].lane;
        let to_lane = example.steps[link.to].lane;
        let relation = if from_lane == to_lane {
            SemanticRelation::Sequence {
                from: semantic_steps[link.from],
                to: semantic_steps[link.to],
                label: semantic_label,
            }
        } else {
            SemanticRelation::Exchange {
                from_process: processes[from_lane],
                to_process: processes[to_lane],
                label: semantic_label,
                payload_description: String::new(),
                internal: false,
            }
        };
        let relation_id = sheet
            .add_semantic_relation(relation)
            .expect("built-in workflow relation has valid endpoints");
        sheet
            .bind_semantic_occurrence(connector_id, SemanticBinding::Relation(relation_id))
            .expect("built-in relation label matches its connector");
    }
    let use_case = sheet
        .add_semantic_entity(SemanticEntity::UseCase(UseCase {
            name: title.to_owned(),
            intent: tr(example.intent),
            description: String::new(),
        }))
        .expect("built-in Software use case is valid");
    for &step in &semantic_steps {
        sheet
            .add_semantic_relation(SemanticRelation::UseCaseStep { use_case, step })
            .expect("built-in use-case step link has valid endpoints");
    }
    sheet
        .add_semantic_relation(SemanticRelation::Accountability {
            target: processes[1],
            accountable: crate::semantic::PerformerRef::Role(role),
        })
        .expect("built-in accountable role exists");
    for (row, kind) in example.participant_kinds.iter().enumerate() {
        if matches!(kind, ParticipantKind::Software | ParticipantKind::Api) {
            sheet
                .add_semantic_relation(SemanticRelation::Accountability {
                    target: processes[row],
                    accountable: crate::semantic::PerformerRef::Role(role),
                })
                .expect("built-in software process has a human accountable role");
        }
    }
    if let Some((_, _, step_index)) = example.escalation_role {
        sheet
            .add_semantic_relation(SemanticRelation::Accountability {
                target: semantic_steps[step_index],
                accountable: crate::semantic::PerformerRef::Role(
                    escalation_role.expect("escalation role was created"),
                ),
            })
            .expect("built-in escalation step has a human accountable role");
    }
    let boundary = sheet
        .add_semantic_entity(SemanticEntity::Boundary(Boundary {
            kind: BoundaryKind::Trust,
            name: tr("Software trust boundary"),
            description: String::new(),
        }))
        .expect("built-in trust boundary is valid");
    for (row, kind) in example.participant_kinds.iter().enumerate() {
        if !matches!(kind, ParticipantKind::Software | ParticipantKind::Api) {
            continue;
        }
        let boundary_object = shape(
            &mut sheet,
            tr("Software trust boundary"),
            Rect::from_min_size(
                Vec2::new(12.0, 40.0 + row as f32 * 195.0),
                Vec2::new(1076.0, 220.0),
            ),
            Color::rgb(255, 249, 225),
            false,
        );
        if let Some(object) = sheet.canvas.object_mut(boundary_object) {
            object.style.fill = None;
            object.style.stroke = Some(Color::rgb(190, 110, 30));
            object.style.text_color = Some(Color::rgb(145, 75, 15));
            let mut stencil = StencilRef::new("thinoda.general", "header-frame-v2");
            stencil.parameters.insert("headerHeight".into(), 18.0);
            object.stencil = Some(stencil);
        }
        sheet.canvas.scene.parents.insert(boundary_object, root);
        sheet
            .bind_semantic_occurrence(boundary_object, SemanticBinding::Entity(boundary))
            .expect("built-in boundary matches each canvas occurrence");
        for member in [participants[row], processes[row]] {
            sheet
                .add_semantic_relation(SemanticRelation::BoundaryMembership { boundary, member })
                .expect("built-in trust boundary member exists");
        }
    }
    shape(
        &mut sheet,
        tr(LEGEND),
        Rect::from_min_size(Vec2::new(0.0, legend_y), Vec2::new(1100.0, 65.0)),
        Color::rgb(255, 249, 225),
        false,
    );
    sheet.theme.theme_id = "clarity".into();
    sheet.theme.tapered_lines = false;
    sheet
        .validate()
        .expect("built-in Software example is valid");
    Workbook::new(sheet)
}

pub(super) fn strings() -> Vec<&'static str> {
    let mut strings = vec![
        LEGEND,
        "Human responsibility",
        "Software responsibility",
        "Software trust boundary",
    ];
    for example in &EXAMPLES {
        strings.push(example.intent);
        strings.extend(example.lanes.iter().copied());
        strings.extend(example.steps.iter().map(|step| step.label));
        strings.extend(example.steps.iter().filter_map(|step| step.rationale));
        strings.extend(
            example
                .links
                .iter()
                .map(|link| link.label)
                .filter(|label| !label.is_empty()),
        );
        strings.push(example.accountable_role);
        if let Some((name, _, _)) = example.escalation_role {
            strings.push(name);
        }
    }
    strings
}

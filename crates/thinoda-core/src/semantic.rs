//! Typed, sheet-local socio-technical meaning bound to canvas occurrences.

use crate::{
    board::{CanvasObjectId, CanvasObjectKind, Endpoint},
    model::Sheet,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

pub const SEMANTIC_SCHEMA_VERSION: u16 = 1;
pub const MAX_SEMANTIC_ENTITIES: usize = 20_000;
pub const MAX_SEMANTIC_RELATIONS: usize = 40_000;
pub const MAX_SEMANTIC_DRAFTS: usize = 20_000;
pub const MAX_SEMANTIC_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_SEMANTIC_MODEL_BYTES: usize = 16 * 1024 * 1024;

macro_rules! semantic_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

semantic_id!(SemanticEntityId);
semantic_id!(SemanticRelationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParticipantKind {
    Unspecified,
    Human,
    Team,
    Software,
    Api,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub kind: ParticipantKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessScope {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub black_box: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    Task,
    Start,
    End,
    ExclusiveDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomationMode {
    Manual,
    Assisted,
    Automated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub kind: StepKind,
    pub automation: AutomationMode,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseCase {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub intent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryKind {
    Organization,
    SoftwareScope,
    Trust,
    Visibility,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub kind: BoundaryKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticEntityKind {
    Participant,
    Role,
    Process,
    Lane,
    Step,
    UseCase,
    Boundary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SemanticEntity {
    Participant(Participant),
    Role(Role),
    Process(ProcessScope),
    Lane(Lane),
    Step(Step),
    UseCase(UseCase),
    Boundary(Boundary),
}

impl SemanticEntity {
    pub(crate) fn set_name(&mut self, name: String) {
        match self {
            Self::Participant(value) => value.name = name,
            Self::Role(value) => value.name = name,
            Self::Process(value) => value.name = name,
            Self::Lane(value) => value.name = name,
            Self::Step(value) => value.name = name,
            Self::UseCase(value) => value.name = name,
            Self::Boundary(value) => value.name = name,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Participant(value) => &value.name,
            Self::Role(value) => &value.name,
            Self::Process(value) => &value.name,
            Self::Lane(value) => &value.name,
            Self::Step(value) => &value.name,
            Self::UseCase(value) => &value.name,
            Self::Boundary(value) => &value.name,
        }
    }

    pub fn kind(&self) -> SemanticEntityKind {
        match self {
            Self::Participant(_) => SemanticEntityKind::Participant,
            Self::Role(_) => SemanticEntityKind::Role,
            Self::Process(_) => SemanticEntityKind::Process,
            Self::Lane(_) => SemanticEntityKind::Lane,
            Self::Step(_) => SemanticEntityKind::Step,
            Self::UseCase(_) => SemanticEntityKind::UseCase,
            Self::Boundary(_) => SemanticEntityKind::Boundary,
        }
    }

    fn validate_text(&self) -> Result<usize, String> {
        let (name, description, extra) = match self {
            Self::Participant(value) => (&value.name, &value.description, None),
            Self::Role(value) => (&value.name, &value.description, None),
            Self::Process(value) => (&value.name, &value.description, None),
            Self::Lane(value) => (&value.name, &value.description, None),
            Self::Step(value) => (&value.name, &value.description, None),
            Self::UseCase(value) => (&value.name, &value.description, Some(&value.intent)),
            Self::Boundary(value) => (&value.name, &value.description, None),
        };
        validate_name(name)?;
        let mut bytes = validate_text(description)?;
        if let Some(extra) = extra {
            bytes += validate_text(extra)?;
        }
        if let Self::Step(Step {
            rationale: Some(rationale),
            ..
        }) = self
        {
            bytes += validate_text(rationale)?;
        }
        Ok(bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PerformerRef {
    Participant(SemanticEntityId),
    Role(SemanticEntityId),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum SemanticRelation {
    Sequence {
        from: SemanticEntityId,
        to: SemanticEntityId,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
    },
    Exchange {
        from_process: SemanticEntityId,
        to_process: SemanticEntityId,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        payload_description: String,
        #[serde(default)]
        internal: bool,
    },
    ProcessOwnership {
        process: SemanticEntityId,
        owner: SemanticEntityId,
    },
    LanePerformer {
        lane: SemanticEntityId,
        performer: PerformerRef,
    },
    RoleFulfillment {
        participant: SemanticEntityId,
        role: SemanticEntityId,
    },
    UseCaseStep {
        use_case: SemanticEntityId,
        step: SemanticEntityId,
    },
    BoundaryMembership {
        boundary: SemanticEntityId,
        member: SemanticEntityId,
    },
    Accountability {
        target: SemanticEntityId,
        accountable: PerformerRef,
    },
}

impl SemanticRelation {
    pub(crate) fn set_display_label(&mut self, label: String) -> bool {
        match self {
            Self::Sequence { label: current, .. } | Self::Exchange { label: current, .. } => {
                *current = label;
                true
            }
            _ => label.is_empty(),
        }
    }

    pub fn display_label(&self) -> &str {
        match self {
            Self::Sequence { label, .. } | Self::Exchange { label, .. } => label,
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticBinding {
    Entity(SemanticEntityId),
    Relation(SemanticRelationId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticReferenceSlot {
    From,
    To,
    Owner,
    Performer,
    Member,
    Accountable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedReferenceDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<SemanticRelationId>,
    pub slot: SemanticReferenceSlot,
    pub expected: SemanticEntityKind,
    pub reference_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticModel {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entities: BTreeMap<SemanticEntityId, SemanticEntity>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relations: BTreeMap<SemanticRelationId, SemanticRelation>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bindings: BTreeMap<CanvasObjectId, SemanticBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved: Vec<UnresolvedReferenceDraft>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticDiagnosticSeverity {
    Info,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub code: &'static str,
    pub severity: SemanticDiagnosticSeverity,
    pub entity: Option<SemanticEntityId>,
    pub message: &'static str,
}

impl SemanticDiagnostic {
    fn warning(
        code: &'static str,
        entity: Option<SemanticEntityId>,
        message: &'static str,
    ) -> Self {
        Self {
            code,
            severity: SemanticDiagnosticSeverity::Warning,
            entity,
            message,
        }
    }
}

impl Default for SemanticModel {
    fn default() -> Self {
        Self {
            schema_version: SEMANTIC_SCHEMA_VERSION,
            entities: BTreeMap::new(),
            relations: BTreeMap::new(),
            bindings: BTreeMap::new(),
            unresolved: Vec::new(),
        }
    }
}

fn default_schema_version() -> u16 {
    SEMANTIC_SCHEMA_VERSION
}

impl SemanticModel {
    pub fn estimated_heap_bytes(&self) -> usize {
        use std::mem::size_of;

        let entities = self
            .entities
            .values()
            .map(|entity| {
                let text_bytes = match entity {
                    SemanticEntity::Participant(value) => {
                        value.name.len() + value.description.len()
                    }
                    SemanticEntity::Role(value) => value.name.len() + value.description.len(),
                    SemanticEntity::Process(value) => value.name.len() + value.description.len(),
                    SemanticEntity::Lane(value) => value.name.len() + value.description.len(),
                    SemanticEntity::Step(value) => {
                        value.name.len()
                            + value.description.len()
                            + value.rationale.as_ref().map_or(0, String::len)
                    }
                    SemanticEntity::UseCase(value) => {
                        value.name.len() + value.intent.len() + value.description.len()
                    }
                    SemanticEntity::Boundary(value) => value.name.len() + value.description.len(),
                };
                size_of::<SemanticEntity>() + 48 + text_bytes
            })
            .sum::<usize>();
        let relations = self
            .relations
            .values()
            .map(|relation| {
                let text_bytes = match relation {
                    SemanticRelation::Sequence { label, .. } => label.len(),
                    SemanticRelation::Exchange {
                        label,
                        payload_description,
                        ..
                    } => label.len() + payload_description.len(),
                    _ => 0,
                };
                size_of::<SemanticRelation>() + 48 + text_bytes
            })
            .sum::<usize>();
        size_of::<Self>()
            + entities
            + relations
            + self.bindings.len()
                * (size_of::<CanvasObjectId>() + size_of::<SemanticBinding>() + 48)
            + self
                .unresolved
                .iter()
                .map(|draft| {
                    size_of::<UnresolvedReferenceDraft>() + draft.reference_text.len() + 24
                })
                .sum::<usize>()
    }

    pub fn diagnostics(&self, sheet: &Sheet) -> Vec<SemanticDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut owned_processes = HashSet::new();
        let mut performed_lanes = HashSet::new();
        let mut use_cases_with_steps = HashSet::new();
        let mut use_cases_by_step: HashMap<SemanticEntityId, Vec<SemanticEntityId>> =
            HashMap::new();
        let mut accountability_by_target: HashMap<SemanticEntityId, Vec<PerformerRef>> =
            HashMap::new();
        let mut human_fulfilled_roles = HashSet::new();
        let mut process_by_step = HashMap::new();
        let mut exchange_starts = HashMap::new();
        let mut branches_by_step: HashMap<SemanticEntityId, Vec<&str>> = HashMap::new();
        let mut sequence_successors: HashMap<SemanticEntityId, Vec<SemanticEntityId>> =
            HashMap::new();
        let mut sequence_predecessors: HashMap<SemanticEntityId, Vec<SemanticEntityId>> =
            HashMap::new();
        let mut steps_by_process: HashMap<SemanticEntityId, Vec<SemanticEntityId>> = HashMap::new();
        let mut starts_by_process: HashMap<SemanticEntityId, Vec<SemanticEntityId>> =
            HashMap::new();
        let mut ends_by_process: HashMap<SemanticEntityId, Vec<SemanticEntityId>> = HashMap::new();

        for (&object_id, binding) in &self.bindings {
            let SemanticBinding::Entity(entity_id) = binding else {
                let SemanticBinding::Relation(relation_id) = binding else {
                    continue;
                };
                let Some(CanvasObjectKind::Connector(connector)) =
                    sheet.canvas.object(object_id).map(|object| &object.kind)
                else {
                    continue;
                };
                if matches!(
                    self.relations.get(relation_id),
                    Some(SemanticRelation::Exchange { .. })
                ) && let Some(step) = self.step_for_endpoint(connector.from)
                {
                    exchange_starts.insert(*relation_id, step);
                }
                continue;
            };
            if self
                .entities
                .get(entity_id)
                .is_some_and(|entity| entity.kind() == SemanticEntityKind::Step)
                && let Some(process) = self.process_for_endpoint(sheet, Endpoint::Object(object_id))
            {
                process_by_step.insert(*entity_id, process);
            }
        }

        for (&entity_id, entity) in &self.entities {
            let SemanticEntity::Step(step) = entity else {
                continue;
            };
            let Some(process) = process_by_step.get(&entity_id).copied() else {
                continue;
            };
            steps_by_process.entry(process).or_default().push(entity_id);
            match step.kind {
                StepKind::Start => starts_by_process
                    .entry(process)
                    .or_default()
                    .push(entity_id),
                StepKind::End => ends_by_process.entry(process).or_default().push(entity_id),
                _ => {}
            }
        }

        for (&relation_id, relation) in &self.relations {
            match relation {
                SemanticRelation::ProcessOwnership { process, .. } => {
                    owned_processes.insert(*process);
                }
                SemanticRelation::LanePerformer { lane, .. } => {
                    performed_lanes.insert(*lane);
                }
                SemanticRelation::UseCaseStep { use_case, .. } => {
                    use_cases_with_steps.insert(*use_case);
                    if let SemanticRelation::UseCaseStep { step, .. } = relation {
                        use_cases_by_step.entry(*step).or_default().push(*use_case);
                    }
                }
                SemanticRelation::Sequence { from, to, label } => {
                    branches_by_step.entry(*from).or_default().push(label);
                    if let (Some(from_process), Some(to_process)) =
                        (process_by_step.get(from), process_by_step.get(to))
                        && from_process != to_process
                    {
                        diagnostics.push(SemanticDiagnostic::warning(
                            "sequence_crosses_process_scopes",
                            Some(*from),
                            "Sequence connects steps in different process scopes.",
                        ));
                    }
                    if let (Some(from_process), Some(to_process)) =
                        (process_by_step.get(from), process_by_step.get(to))
                        && from_process == to_process
                    {
                        sequence_successors.entry(*from).or_default().push(*to);
                        sequence_predecessors.entry(*to).or_default().push(*from);
                    }
                }
                SemanticRelation::Exchange { label, .. } => {
                    if let Some(step) = exchange_starts.get(&relation_id) {
                        branches_by_step.entry(*step).or_default().push(label);
                    }
                }
                SemanticRelation::Accountability {
                    target,
                    accountable,
                } => {
                    accountability_by_target
                        .entry(*target)
                        .or_default()
                        .push(*accountable);
                }
                SemanticRelation::RoleFulfillment { participant, role } => {
                    if self.entities.get(participant).is_some_and(|entity| {
                        matches!(
                            entity,
                            SemanticEntity::Participant(Participant {
                                kind: ParticipantKind::Human | ParticipantKind::Team,
                                ..
                            })
                        )
                    }) {
                        human_fulfilled_roles.insert(*role);
                    }
                }
                SemanticRelation::BoundaryMembership { .. } => {}
            }
        }

        let reachable_from =
            |starts: &[SemanticEntityId],
             adjacency: &HashMap<SemanticEntityId, Vec<SemanticEntityId>>| {
                let mut reachable = HashSet::new();
                let mut pending = starts.to_vec();
                while let Some(step) = pending.pop() {
                    if reachable.len() >= MAX_SEMANTIC_ENTITIES || !reachable.insert(step) {
                        continue;
                    }
                    pending.extend(adjacency.get(&step).into_iter().flatten().copied());
                }
                reachable
            };
        for (&process, process_steps) in &steps_by_process {
            let starts = starts_by_process
                .get(&process)
                .map_or(&[][..], Vec::as_slice);
            if starts.is_empty() {
                continue;
            }
            let reachable = reachable_from(starts, &sequence_successors);
            for step in process_steps {
                if !reachable.contains(step) {
                    diagnostics.push(SemanticDiagnostic::warning(
                        "step_unreachable_from_start",
                        Some(*step),
                        "Step is unreachable from any start event in its process.",
                    ));
                }
            }
            let ends = ends_by_process.get(&process).map_or(&[][..], Vec::as_slice);
            if !ends.is_empty() {
                let can_reach_end = reachable_from(ends, &sequence_predecessors);
                for step in process_steps {
                    if reachable.contains(step) && !can_reach_end.contains(step) {
                        diagnostics.push(SemanticDiagnostic::warning(
                            "step_cannot_reach_end",
                            Some(*step),
                            "Step has no sequence path to an end event.",
                        ));
                    }
                }
            }
        }

        for (&process_id, entity) in &self.entities {
            let SemanticEntity::Process(process) = entity else {
                continue;
            };
            if process.black_box {
                continue;
            }
            let steps = steps_by_process.get(&process_id);
            if steps.is_none_or(Vec::is_empty) {
                diagnostics.push(SemanticDiagnostic::warning(
                    "process_without_steps",
                    Some(process_id),
                    "Process has no workflow steps.",
                ));
                continue;
            }
            let has_starts = starts_by_process
                .get(&process_id)
                .is_some_and(|steps| !steps.is_empty());
            let has_ends = ends_by_process
                .get(&process_id)
                .is_some_and(|steps| !steps.is_empty());
            if !has_starts && has_ends {
                diagnostics.push(SemanticDiagnostic::warning(
                    "process_without_start_event",
                    Some(process_id),
                    "Process has no start event.",
                ));
            }
            if has_starts && !has_ends {
                diagnostics.push(SemanticDiagnostic::warning(
                    "process_without_end_event",
                    Some(process_id),
                    "Process has no end event.",
                ));
            }
        }

        for (&id, entity) in &self.entities {
            match entity {
                SemanticEntity::Process(_) if !owned_processes.contains(&id) => {
                    diagnostics.push(SemanticDiagnostic::warning(
                        "process_without_owner",
                        Some(id),
                        "Process has no declared owner.",
                    ));
                }
                SemanticEntity::Lane(_) if !performed_lanes.contains(&id) => {
                    diagnostics.push(SemanticDiagnostic::warning(
                        "lane_without_performer",
                        Some(id),
                        "Lane has no declared performer.",
                    ));
                }
                SemanticEntity::Step(step) if step.kind == StepKind::ExclusiveDecision => {
                    let branches = branches_by_step.get(&id).map_or(&[][..], Vec::as_slice);
                    if branches.len() < 2 || branches.iter().any(|label| label.trim().is_empty()) {
                        diagnostics.push(SemanticDiagnostic::warning(
                            "decision_without_labeled_branches",
                            Some(id),
                            "Decision needs at least two labeled outgoing sequence branches.",
                        ));
                    }
                }
                SemanticEntity::Step(step) if step.automation == AutomationMode::Automated => {
                    let mut targets = HashSet::from([id]);
                    if let Some(process) = process_by_step.get(&id) {
                        targets.insert(*process);
                    }
                    if let Some(use_cases) = use_cases_by_step.get(&id) {
                        targets.extend(use_cases.iter().copied());
                    }
                    let performers = targets
                        .iter()
                        .flat_map(|target| {
                            accountability_by_target
                                .get(target)
                                .into_iter()
                                .flatten()
                                .copied()
                        })
                        .collect::<Vec<_>>();
                    let has_human_accountability =
                        performers.iter().any(|performer| match performer {
                            PerformerRef::Participant(id) => {
                                self.entities.get(id).is_some_and(|entity| {
                                    matches!(
                                        entity,
                                        SemanticEntity::Participant(Participant {
                                            kind: ParticipantKind::Human | ParticipantKind::Team,
                                            ..
                                        })
                                    )
                                })
                            }
                            PerformerRef::Role(role) => human_fulfilled_roles.contains(role),
                        });
                    let has_unfulfilled_role = performers.iter().any(|performer| {
                        matches!(
                            performer,
                            PerformerRef::Role(role) if !human_fulfilled_roles.contains(role)
                        )
                    });
                    let diagnostic = if performers.is_empty() {
                        Some((
                            "automated_step_without_accountability",
                            "Automated step has no accountable human, team, or fulfilled role.",
                        ))
                    } else if !has_human_accountability && has_unfulfilled_role {
                        Some((
                            "accountable_role_without_human_fulfillment",
                            "Accountable role has no human or team fulfillment.",
                        ))
                    } else if !has_human_accountability {
                        Some((
                            "automated_step_software_only_accountability",
                            "Automated step is accountable only to software participants.",
                        ))
                    } else {
                        None
                    };
                    if let Some((code, message)) = diagnostic {
                        diagnostics.push(SemanticDiagnostic::warning(code, Some(id), message));
                    }
                }
                SemanticEntity::UseCase(_) if !use_cases_with_steps.contains(&id) => {
                    diagnostics.push(SemanticDiagnostic::warning(
                        "use_case_without_steps",
                        Some(id),
                        "Use case has no linked workflow steps.",
                    ));
                }
                SemanticEntity::Step(_) if !process_by_step.contains_key(&id) => {
                    diagnostics.push(SemanticDiagnostic::warning(
                        "step_without_process_scope",
                        Some(id),
                        "Step is not contained in a process scope.",
                    ));
                }
                _ => {}
            }
        }

        for _ in &self.unresolved {
            diagnostics.push(SemanticDiagnostic::warning(
                "unresolved_reference",
                None,
                "Unresolved semantic references remain.",
            ));
        }
        diagnostics
    }

    pub fn validate(&self, sheet: &Sheet) -> Result<(), String> {
        if self.schema_version != SEMANTIC_SCHEMA_VERSION {
            return Err(format!(
                "unsupported socio-technical schema version {}",
                self.schema_version
            ));
        }
        if self.entities.len() > MAX_SEMANTIC_ENTITIES
            || self.relations.len() > MAX_SEMANTIC_RELATIONS
            || self.unresolved.len() > MAX_SEMANTIC_DRAFTS
        {
            return Err("socio-technical model exceeds count limits".into());
        }

        let mut bytes = 0usize;
        for (id, entity) in &self.entities {
            if id.0 == 0 {
                return Err("socio-technical entity id must be nonzero".into());
            }
            bytes = bytes.saturating_add(entity.validate_text()?);
        }

        for (id, relation) in &self.relations {
            if id.0 == 0 {
                return Err("socio-technical relation id must be nonzero".into());
            }
            bytes = bytes.saturating_add(self.validate_relation(relation)?);
        }

        let mut bound_relations = HashSet::new();
        let mut single_occurrence_entities = HashSet::new();
        for (object_id, binding) in &self.bindings {
            let object = sheet.canvas.object(*object_id).ok_or_else(|| {
                format!("semantic binding refers to missing canvas object {object_id}")
            })?;
            match binding {
                SemanticBinding::Entity(id) => {
                    let entity = self
                        .entities
                        .get(id)
                        .ok_or_else(|| format!("semantic binding refers to missing entity {id}"))?;
                    if !matches!(&object.kind, CanvasObjectKind::Shape { .. }) {
                        return Err("semantic entities must bind to canvas shapes".into());
                    }
                    if object.label() != entity.name() {
                        return Err(format!(
                            "semantic entity {id} label must match its canvas occurrence"
                        ));
                    }
                    match entity.kind() {
                        SemanticEntityKind::Process | SemanticEntityKind::Lane => {
                            if !sheet.canvas.scene.containers.contains(object_id) {
                                return Err(
                                    "semantic processes and lanes must bind to scene containers"
                                        .into(),
                                );
                            }
                            if !single_occurrence_entities.insert(*id) {
                                return Err(format!(
                                    "semantic entity {id} has multiple scene occurrences"
                                ));
                            }
                        }
                        SemanticEntityKind::Step => {
                            if sheet.canvas.scene.containers.contains(object_id) {
                                return Err(
                                    "workflow steps must bind to non-container shapes".into()
                                );
                            }
                            if !single_occurrence_entities.insert(*id) {
                                return Err(format!(
                                    "semantic step {id} has multiple scene occurrences"
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                SemanticBinding::Relation(id) => {
                    if !matches!(&object.kind, CanvasObjectKind::Connector(_)) {
                        return Err("semantic relations must bind to canvas connectors".into());
                    }
                    let relation = self.relations.get(id).ok_or_else(|| {
                        format!("semantic binding refers to missing relation {id}")
                    })?;
                    if !matches!(
                        relation,
                        SemanticRelation::Sequence { .. } | SemanticRelation::Exchange { .. }
                    ) {
                        return Err(
                            "only sequence and exchange relations can bind to connectors".into(),
                        );
                    }
                    if object.label() != relation.display_label() {
                        return Err(format!(
                            "semantic relation {id} label must match its canvas connector"
                        ));
                    }
                    if !bound_relations.insert(*id) {
                        return Err(format!(
                            "semantic relation {id} has multiple connector occurrences"
                        ));
                    }
                }
            }
            for (object_id, binding) in &self.bindings {
                let SemanticBinding::Relation(relation_id) = binding else {
                    continue;
                };
                let object = sheet.canvas.object(*object_id).ok_or_else(|| {
                    format!("semantic binding refers to missing canvas object {object_id}")
                })?;
                let CanvasObjectKind::Connector(connector) = &object.kind else {
                    continue;
                };
                let relation = &self.relations[relation_id];
                match relation {
                    SemanticRelation::Sequence { from, to, .. }
                        if self.step_for_endpoint(connector.from) != Some(*from)
                            || self.step_for_endpoint(connector.to) != Some(*to) =>
                    {
                        return Err(format!(
                            "sequence relation {relation_id} endpoints do not match its connector"
                        ));
                    }
                    SemanticRelation::Exchange {
                        from_process,
                        to_process,
                        ..
                    } if self.process_for_endpoint(sheet, connector.from)
                        != Some(*from_process)
                        || self.process_for_endpoint(sheet, connector.to) != Some(*to_process) =>
                    {
                        return Err(format!(
                            "exchange relation {relation_id} process scopes do not match its connector"
                        ));
                    }
                    _ => {}
                }
            }
        }

        for draft in &self.unresolved {
            bytes = bytes.saturating_add(validate_text(&draft.reference_text)?);
            if let Some(relation) = draft.relation
                && !self.relations.contains_key(&relation)
            {
                return Err(format!(
                    "unresolved reference draft refers to missing relation {relation}"
                ));
            }
        }
        if bytes > MAX_SEMANTIC_MODEL_BYTES {
            return Err("socio-technical model exceeds the text budget".into());
        }
        Ok(())
    }

    fn validate_relation(&self, relation: &SemanticRelation) -> Result<usize, String> {
        let mut bytes = 0;
        match relation {
            SemanticRelation::Sequence { from, to, label } => {
                self.require_kind(*from, SemanticEntityKind::Step)?;
                self.require_kind(*to, SemanticEntityKind::Step)?;
                bytes += validate_text(label)?;
            }
            SemanticRelation::Exchange {
                from_process,
                to_process,
                label,
                payload_description,
                ..
            } => {
                self.require_kind(*from_process, SemanticEntityKind::Process)?;
                self.require_kind(*to_process, SemanticEntityKind::Process)?;
                if from_process == to_process {
                    return Err("an exchange must connect distinct process scopes".into());
                }
                bytes += validate_text(label)? + validate_text(payload_description)?;
            }
            SemanticRelation::ProcessOwnership { process, owner } => {
                self.require_kind(*process, SemanticEntityKind::Process)?;
                self.require_kind(*owner, SemanticEntityKind::Participant)?;
            }
            SemanticRelation::LanePerformer { lane, performer } => {
                self.require_kind(*lane, SemanticEntityKind::Lane)?;
                self.require_performer(*performer)?;
            }
            SemanticRelation::RoleFulfillment { participant, role } => {
                self.require_kind(*participant, SemanticEntityKind::Participant)?;
                self.require_kind(*role, SemanticEntityKind::Role)?;
            }
            SemanticRelation::UseCaseStep { use_case, step } => {
                self.require_kind(*use_case, SemanticEntityKind::UseCase)?;
                self.require_kind(*step, SemanticEntityKind::Step)?;
            }
            SemanticRelation::BoundaryMembership { boundary, member } => {
                self.require_kind(*boundary, SemanticEntityKind::Boundary)?;
                if !self.entities.contains_key(member) {
                    return Err(format!(
                        "boundary membership refers to missing entity {member}"
                    ));
                }
                if boundary == member {
                    return Err("a boundary cannot contain itself".into());
                }
            }
            SemanticRelation::Accountability {
                target,
                accountable,
            } => {
                let target_kind = self.entity_kind(*target)?;
                if !matches!(
                    target_kind,
                    SemanticEntityKind::Process
                        | SemanticEntityKind::Step
                        | SemanticEntityKind::UseCase
                ) {
                    return Err("accountability target must be a process, step or use case".into());
                }
                self.require_performer(*accountable)?;
            }
        }
        Ok(bytes)
    }

    fn require_performer(&self, performer: PerformerRef) -> Result<(), String> {
        match performer {
            PerformerRef::Participant(id) => self.require_kind(id, SemanticEntityKind::Participant),
            PerformerRef::Role(id) => self.require_kind(id, SemanticEntityKind::Role),
        }
    }

    pub fn step_for_endpoint(&self, endpoint: Endpoint) -> Option<SemanticEntityId> {
        let Endpoint::Object(object_id) = endpoint else {
            return None;
        };
        match self.bindings.get(&object_id) {
            Some(SemanticBinding::Entity(entity_id))
                if self
                    .entities
                    .get(entity_id)
                    .is_some_and(|entity| entity.kind() == SemanticEntityKind::Step) =>
            {
                Some(*entity_id)
            }
            _ => None,
        }
    }

    pub fn process_for_endpoint(
        &self,
        sheet: &Sheet,
        endpoint: Endpoint,
    ) -> Option<SemanticEntityId> {
        let Endpoint::Object(object_id) = endpoint else {
            return None;
        };
        let mut current = Some(object_id);
        let mut seen = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) {
                return None;
            }
            if let Some(SemanticBinding::Entity(entity_id)) = self.bindings.get(&id)
                && self
                    .entities
                    .get(entity_id)
                    .is_some_and(|entity| entity.kind() == SemanticEntityKind::Process)
            {
                return Some(*entity_id);
            }
            current = sheet.canvas.scene.parents.get(&id).copied();
        }
        None
    }

    fn require_kind(
        &self,
        id: SemanticEntityId,
        expected: SemanticEntityKind,
    ) -> Result<(), String> {
        if self.entity_kind(id)? == expected {
            Ok(())
        } else {
            Err(format!("semantic entity {id} must be a {expected:?}"))
        }
    }

    fn entity_kind(&self, id: SemanticEntityId) -> Result<SemanticEntityKind, String> {
        self.entities
            .get(&id)
            .map(SemanticEntity::kind)
            .ok_or_else(|| format!("semantic relation refers to missing entity {id}"))
    }
}

fn validate_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("socio-technical names must not be empty".into());
    }
    validate_text(value).map(|_| ())
}

fn validate_text(value: &str) -> Result<usize, String> {
    if value.len() > MAX_SEMANTIC_TEXT_BYTES {
        return Err("socio-technical text exceeds 16 KiB".into());
    }
    Ok(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        board::{CanvasConnector, CanvasObject, CanvasObjectKind, Endpoint, ShapeKind},
        geometry::{Rect, Vec2},
    };

    fn step(name: &str) -> SemanticEntity {
        SemanticEntity::Step(Step {
            kind: StepKind::Task,
            automation: AutomationMode::Manual,
            name: name.into(),
            description: String::new(),
            rationale: None,
        })
    }

    fn workflow_step(
        sheet: &mut Sheet,
        process: CanvasObjectId,
        name: &str,
        kind: StepKind,
        x: f32,
    ) -> SemanticEntityId {
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(x, 40.0), Vec2::new(80.0, 40.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = name.into();
        }
        let object = sheet.add_canvas_object(shape).unwrap();
        sheet
            .reparent_canvas_objects(&[object], Some(process))
            .unwrap();
        let entity = sheet
            .add_semantic_entity(SemanticEntity::Step(Step {
                kind,
                automation: AutomationMode::Manual,
                name: name.into(),
                description: String::new(),
                rationale: None,
            }))
            .unwrap();
        sheet
            .bind_semantic_occurrence(object, SemanticBinding::Entity(entity))
            .unwrap();
        entity
    }

    fn workflow_process(
        sheet: &mut Sheet,
        name: &str,
        black_box: bool,
        x: f32,
    ) -> (CanvasObjectId, SemanticEntityId) {
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(x, 0.0), Vec2::new(240.0, 180.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = name.into();
        }
        let object = sheet.add_canvas_object(shape).unwrap();
        sheet.set_canvas_container(object, true).unwrap();
        let process = sheet
            .add_semantic_entity(SemanticEntity::Process(ProcessScope {
                name: name.into(),
                description: String::new(),
                black_box,
            }))
            .unwrap();
        sheet
            .bind_semantic_occurrence(object, SemanticBinding::Entity(process))
            .unwrap();
        (object, process)
    }

    #[test]
    fn validates_typed_relations_and_rejects_missing_or_wrongly_typed_references() {
        let sheet = Sheet::new("Workflow", "Root");
        let mut model = SemanticModel::default();
        let first = SemanticEntityId(1);
        let second = SemanticEntityId(2);
        model.entities.insert(first, step("Receive request"));
        model.entities.insert(second, step("Review request"));
        model.relations.insert(
            SemanticRelationId(3),
            SemanticRelation::Sequence {
                from: first,
                to: second,
                label: String::new(),
            },
        );
        model.validate(&sheet).unwrap();

        model.relations.insert(
            SemanticRelationId(4),
            SemanticRelation::Sequence {
                from: first,
                to: SemanticEntityId(99),
                label: String::new(),
            },
        );
        assert!(
            model
                .validate(&sheet)
                .unwrap_err()
                .contains("missing entity")
        );
    }

    #[test]
    fn diagnostics_find_unreachable_steps_and_reachable_steps_without_an_end_path() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let (process_object, _) = workflow_process(&mut sheet, "Request handling", false, 0.0);

        let start = workflow_step(&mut sheet, process_object, "Start", StepKind::Start, 20.0);
        let task = workflow_step(&mut sheet, process_object, "Review", StepKind::Task, 120.0);
        let end = workflow_step(&mut sheet, process_object, "End", StepKind::End, 220.0);
        let loop_step = workflow_step(&mut sheet, process_object, "Retry", StepKind::Task, 320.0);
        let unreachable = workflow_step(
            &mut sheet,
            process_object,
            "Unconnected",
            StepKind::Task,
            20.0,
        );
        let mut add_sequence = |from, to| {
            sheet
                .add_semantic_relation(SemanticRelation::Sequence {
                    from,
                    to,
                    label: String::new(),
                })
                .unwrap();
        };
        add_sequence(start, task);
        add_sequence(task, end);
        add_sequence(start, loop_step);
        add_sequence(loop_step, loop_step);

        let diagnostics = sheet.semantic.as_ref().unwrap().diagnostics(&sheet);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "step_unreachable_from_start"
                && diagnostic.entity == Some(unreachable)
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "step_cannot_reach_end" && diagnostic.entity == Some(loop_step)
        }));
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "step_cannot_reach_end" && diagnostic.entity == Some(task)
        }));
    }

    #[test]
    fn diagnostics_report_incomplete_processes_and_unallocated_steps_but_allow_black_boxes() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let (incomplete_object, incomplete_process) =
            workflow_process(&mut sheet, "Incomplete", false, 0.0);
        workflow_step(
            &mut sheet,
            incomplete_object,
            "Start",
            StepKind::Start,
            20.0,
        );
        let (end_only_object, end_only_process) =
            workflow_process(&mut sheet, "End only", false, 300.0);
        workflow_step(&mut sheet, end_only_object, "End", StepKind::End, 320.0);
        let (fragment_object, fragment_process) =
            workflow_process(&mut sheet, "Workflow fragment", false, 600.0);
        workflow_step(&mut sheet, fragment_object, "Review", StepKind::Task, 620.0);
        let (_, empty_process) = workflow_process(&mut sheet, "Empty", false, 900.0);
        let (_, black_box_process) = workflow_process(&mut sheet, "External service", true, 1200.0);

        let mut unallocated = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(900.0, 40.0), Vec2::new(80.0, 40.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut unallocated.kind {
            *label = "Unallocated work".into();
        }
        let unallocated_object = sheet.add_canvas_object(unallocated).unwrap();
        let unallocated_step = sheet
            .add_semantic_entity(SemanticEntity::Step(Step {
                kind: StepKind::Task,
                automation: AutomationMode::Manual,
                name: "Unallocated work".into(),
                description: String::new(),
                rationale: None,
            }))
            .unwrap();
        sheet
            .bind_semantic_occurrence(
                unallocated_object,
                SemanticBinding::Entity(unallocated_step),
            )
            .unwrap();

        let diagnostics = sheet.semantic.as_ref().unwrap().diagnostics(&sheet);
        for (code, entity) in [
            ("process_without_end_event", incomplete_process),
            ("process_without_start_event", end_only_process),
            ("process_without_steps", empty_process),
            ("step_without_process_scope", unallocated_step),
        ] {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code && diagnostic.entity == Some(entity)),
                "missing {code} for {entity}: {diagnostics:?}"
            );
        }
        assert!(!diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                "process_without_steps"
                    | "process_without_start_event"
                    | "process_without_end_event"
            ) && diagnostic.entity == Some(black_box_process)
        }));
        assert!(!diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                "process_without_start_event" | "process_without_end_event"
            ) && diagnostic.entity == Some(fragment_process)
        }));
    }

    #[test]
    fn scene_container_changes_preserve_semantic_bindings_atomically() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let (process_object, _) = workflow_process(&mut sheet, "Request handling", false, 0.0);
        sheet.validate().unwrap();
        let before = sheet.clone();
        assert!(sheet.set_canvas_container(process_object, false).is_err());
        assert_eq!(sheet, before);

        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::new(300.0, 40.0), Vec2::new(80.0, 40.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Review".into();
        }
        let step_object = sheet.add_canvas_object(shape).unwrap();
        let step_id = sheet.add_semantic_entity(step("Review")).unwrap();
        sheet
            .bind_semantic_occurrence(step_object, SemanticBinding::Entity(step_id))
            .unwrap();
        sheet.validate().unwrap();

        let before = sheet.clone();
        assert!(sheet.set_canvas_container(step_object, true).is_err());
        assert_eq!(sheet, before);
    }

    #[test]
    fn binding_roundtrips_and_canvas_label_edits_stay_in_sync() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let mut shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(80.0, 40.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
            *label = "Receive request".into();
        }
        let object_id = sheet.add_canvas_object(shape).unwrap();
        let entity_id = sheet.add_semantic_entity(step("Receive request")).unwrap();
        sheet
            .bind_semantic_occurrence(object_id, SemanticBinding::Entity(entity_id))
            .unwrap();
        sheet.validate().unwrap();

        sheet
            .update_canvas_object(object_id, |object| {
                if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                    *label = "Inspect request".into();
                }
            })
            .unwrap();
        let SemanticEntity::Step(entity) = &sheet.semantic.as_ref().unwrap().entities[&entity_id]
        else {
            panic!("expected a step");
        };
        assert_eq!(entity.name, "Inspect request");

        let restored: Sheet = serde_json::from_slice(&serde_json::to_vec(&sheet).unwrap()).unwrap();
        assert_eq!(restored, sheet);
        assert!(
            sheet
                .remove_canvas_objects(&[object_id])
                .unwrap_err()
                .contains("bindings")
        );
        assert!(
            crate::clipboard::prepare_board_selection(&sheet, &[], &[object_id])
                .err()
                .unwrap()
                .contains("semantic references")
        );
    }

    #[test]
    fn rejects_empty_names_and_unsupported_schema_versions() {
        let sheet = Sheet::new("Workflow", "Root");
        let mut model = SemanticModel::default();
        model.entities.insert(SemanticEntityId(1), step("  "));
        assert!(
            model
                .validate(&sheet)
                .unwrap_err()
                .contains("must not be empty")
        );

        model.entities.clear();
        model.schema_version += 1;
        assert!(model.validate(&sheet).unwrap_err().contains("unsupported"));
    }

    #[test]
    fn connector_edits_update_bound_sequence_endpoints_atomically() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let mut process_shape = CanvasObject::shape(
            ShapeKind::Rectangle,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(320.0, 180.0)),
        );
        if let CanvasObjectKind::Shape { label, .. } = &mut process_shape.kind {
            *label = "Review process".into();
        }
        let process_object = sheet.add_canvas_object(process_shape).unwrap();
        sheet.set_canvas_container(process_object, true).unwrap();
        let process_id = sheet
            .add_semantic_entity(SemanticEntity::Process(ProcessScope {
                name: "Review process".into(),
                description: String::new(),
                black_box: false,
            }))
            .unwrap();
        sheet
            .bind_semantic_occurrence(process_object, SemanticBinding::Entity(process_id))
            .unwrap();
        let mut objects = Vec::new();
        let mut steps = Vec::new();
        for (index, name) in ["Receive request", "Review request"]
            .into_iter()
            .enumerate()
        {
            let mut shape = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(
                    Vec2::new(20.0 + index as f32 * 110.0, 60.0),
                    Vec2::new(80.0, 40.0),
                ),
            );
            if let CanvasObjectKind::Shape { label, .. } = &mut shape.kind {
                *label = name.into();
            }
            let object_id = sheet.add_canvas_object(shape).unwrap();
            let entity_id = sheet.add_semantic_entity(step(name)).unwrap();
            sheet
                .bind_semantic_occurrence(object_id, SemanticBinding::Entity(entity_id))
                .unwrap();
            sheet
                .reparent_canvas_objects(&[object_id], Some(process_object))
                .unwrap();
            objects.push(object_id);
            steps.push(entity_id);
        }
        let unbound_object = sheet
            .add_canvas_object(CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(240.0, 60.0), Vec2::new(80.0, 40.0)),
            ))
            .unwrap();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Object(objects[0]),
                Endpoint::Object(objects[1]),
            )))
            .unwrap();
        let relation = sheet
            .add_semantic_relation(SemanticRelation::Sequence {
                from: steps[0],
                to: steps[1],
                label: String::new(),
            })
            .unwrap();
        sheet
            .bind_semantic_occurrence(connector, SemanticBinding::Relation(relation))
            .unwrap();

        sheet
            .update_canvas_object(connector, |object| {
                if let CanvasObjectKind::Connector(value) = &mut object.kind {
                    std::mem::swap(&mut value.from, &mut value.to);
                }
            })
            .unwrap();
        let SemanticRelation::Sequence { from, to, .. } = sheet
            .semantic
            .as_ref()
            .unwrap()
            .relations
            .get(&relation)
            .unwrap()
        else {
            panic!("bound relation remains a sequence");
        };
        assert_eq!((*from, *to), (steps[1], steps[0]));
        assert!(sheet.semantic.as_ref().unwrap().validate(&sheet).is_ok());

        let original = sheet.canvas.object(connector).unwrap().clone();
        let original_semantic = sheet.semantic.clone().unwrap();
        assert!(
            sheet
                .update_canvas_object(connector, |object| {
                    if let CanvasObjectKind::Connector(value) = &mut object.kind {
                        value.from = Endpoint::Object(unbound_object);
                    }
                })
                .is_err()
        );
        assert_eq!(sheet.canvas.object(connector), Some(&original));
        assert_eq!(sheet.semantic.as_ref().unwrap(), &original_semantic);
    }

    #[test]
    fn connector_edits_update_bound_exchange_processes_atomically() {
        let mut sheet = Sheet::new("Workflow", "Root");
        let mut add_process = |name: &str, x: f32| {
            let mut process_shape = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(x, 0.0), Vec2::new(150.0, 120.0)),
            );
            if let CanvasObjectKind::Shape { label, .. } = &mut process_shape.kind {
                *label = name.into();
            }
            let process_object = sheet.add_canvas_object(process_shape).unwrap();
            sheet.set_canvas_container(process_object, true).unwrap();
            let process_id = sheet
                .add_semantic_entity(SemanticEntity::Process(ProcessScope {
                    name: name.into(),
                    description: String::new(),
                    black_box: false,
                }))
                .unwrap();
            sheet
                .bind_semantic_occurrence(process_object, SemanticBinding::Entity(process_id))
                .unwrap();

            let mut step_shape = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(x + 20.0, 50.0), Vec2::new(80.0, 40.0)),
            );
            if let CanvasObjectKind::Shape { label, .. } = &mut step_shape.kind {
                *label = format!("{name} action");
            }
            let step_object = sheet.add_canvas_object(step_shape).unwrap();
            let step_name = format!("{name} action");
            let step_id = sheet.add_semantic_entity(step(&step_name)).unwrap();
            sheet
                .bind_semantic_occurrence(step_object, SemanticBinding::Entity(step_id))
                .unwrap();
            sheet
                .reparent_canvas_objects(&[step_object], Some(process_object))
                .unwrap();
            (process_id, step_object)
        };
        let (source_process, source_step) = add_process("Source", 0.0);
        let (original_target_process, original_target_step) = add_process("Target", 200.0);
        let (new_target_process, new_target_step) = add_process("Escalation", 400.0);

        let mut connector_value = CanvasConnector::new(
            Endpoint::Object(source_step),
            Endpoint::Object(original_target_step),
        );
        connector_value.label = "Request".into();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(connector_value))
            .unwrap();
        let relation = sheet
            .add_semantic_relation(SemanticRelation::Exchange {
                from_process: source_process,
                to_process: original_target_process,
                label: "Request".into(),
                payload_description: String::new(),
                internal: false,
            })
            .unwrap();
        sheet
            .bind_semantic_occurrence(connector, SemanticBinding::Relation(relation))
            .unwrap();

        sheet
            .update_canvas_object(connector, |object| {
                if let CanvasObjectKind::Connector(value) = &mut object.kind {
                    value.to = Endpoint::Object(new_target_step);
                }
            })
            .unwrap();
        assert!(matches!(
            sheet
                .semantic
                .as_ref()
                .unwrap()
                .relations
                .get(&relation),
            Some(SemanticRelation::Exchange {
                from_process,
                to_process,
                ..
            }) if *from_process == source_process && *to_process == new_target_process
        ));
        assert!(sheet.semantic.as_ref().unwrap().validate(&sheet).is_ok());
    }
}

# Socio-technical diagrams: research, concept and phased execution

Research date: 2026-11-13. Status: **proposed, not implemented or user-approved**.
This document extends the [main plan](PLAN.md) and coordinates with the
[draw.io plan](DRAWIO.md); it does not replace either. No application behavior
is changed by this planning work.

**Nested-swimlane clarification:** [SCENE-CONTAINERS.md](SCENE-CONTAINERS.md)
is the common structural solution for draw.io and this editor: one scene
hierarchy, one layout/resize/collapse engine, one command/lifecycle contract.
Socio-technical meaning is optional binding data, not a second pool/lane tree.

## 1. Recommendation and product goal

Make Thinoda a **workflow-first socio-technical editor**, not merely a palette
of BPMN-looking shapes. People, teams, software services and API participants
should be equally easy to model, connect and inspect. They are peers in the
interaction model, not interchangeable in accountability or decision authority.

Start with a small, explicitly named **Thinoda Socio-technical v1** notation:
collaboration workflows inspired by BPMN, peer participants inspired by S-BPM,
and optional goals, responsibilities and boundary annotations. Do not market
this combination as BPMN, S-BPM, iStar or ArchiMate conformance.

**First release:** one editable collaboration workflow per semantic sheet,
with process scopes/pools, responsibility lanes, typed exchanges and sequence
flows, use-case links, declared boundaries and actionable diagnostics.
**Next release:** a linked participant/context view over the same model.
Service-blueprint and goal/dependency views are later, separately gated work,
not obligations of the first release.

Success means a team can answer from one saved, editable diagram:

- Who participates, who performs work, and who is accountable?
- Which user outcome/use case is served, and why is a decision made?
- What happens locally, and what is communicated across participants?
- Where are software scope, organizational responsibility and trust boundaries?
- What happens when automation fails, an exception occurs or a human intervenes?

The request's "BPML" is interpreted here as a BPMN-like process graph. BPML is
a different process language; implementing its runtime or serialization is not
part of this proposal.

## 2. Research: established approaches and their limits

Sources were checked on the date above. These are paraphrased findings, not
copied specification text, bundled assets or promises of file compatibility.

| Approach | What it contributes | Limitation for this request | Recommendation |
|---|---|---|---|
| [BPMN 2.0.2 collaboration][bpmn] | Participants/pools, responsibility lanes, activities, events, gateways and inter-participant messages | Goals, rationale, trust and use-case traceability need additional modeling; full BPMN is much larger than its familiar symbols | Workflow vocabulary and a later bounded interoperability profile |
| [Subject-oriented BPM / S-BPM][sbpm] | Human and technical subjects communicate as peers; interaction models are separated from subject behavior | Less familiar to many teams; behavior-state modeling and executable semantics would add another substantial editor | Adopt participant parity and distinguish communication from internal behavior; defer formal subject behavior diagrams |
| [ArchiMate][archimate101] | Relates organizational, behavioral and technical architecture through explicit concepts and relationships | Not a substitute for detailed operational workflow; a full metamodel/editor is a separate product-sized effort | Use architectural viewpoint discipline; defer a version-specific profile |
| [Service blueprint][blueprint] | Customer actions, frontstage/backstage work, support processes and interaction/visibility lines | Visibility is not organization, trust or process ownership; spatial layout does not define executable control flow | Later experience-oriented view, with visibility explicitly modeled |
| [iStar 2.0][istar] | Actors/roles, goals, qualities, tasks, resources and dependencies explain who needs what and why | Intention and dependency are not execution order; formal reasoning needs its own semantics | Optional goals/dependency detail later; lightweight rationale and outcome links in v1 |
| [UML use cases and activities][uml] | Subject/system boundary, external actors, use cases; activity modeling can show behavior and partitions | A use-case oval is not a process step; use cases alone do not describe ordering or handoffs | Use-case records linked to workflow steps, not overloaded task shapes |
| [C4 dynamic diagrams][c4dynamic] | Numbered interactions explain one use case with people/software context | Not a rich business workflow, responsibility or goal language | Useful context-view precedent; not the canonical process model |

### Standards and evidence caveats

- OMG's formal BPMN page identifies version 2.0.2 and supplies the normative
  specification and XML schemas. Sequence flow stays within a process/pool;
  it may cross lanes. Message flow connects different participants/pools, not
  two activities in the same pool. A pool can be black-box, without displayed
  internal behavior. These distinctions must survive editing, not just styling.
- S-BPM's professional association explicitly describes subjects as human or
  technical agents. Its published primer distinguishes subject interaction
  diagrams from subject behavior diagrams with send, receive and function states.
  The association page was readable; the linked Springer book returned HTTP 406.
  Detailed behavior semantics must be checked before promising an S-BPM profile.
  Thinoda v1 does not implement S-BPM behavior-state autonomy or execution.
- ArchiMate 4 exists: The Open Group's [2026 publication announcement][archimate4]
  confirms it. The chapter URL supplied in the earlier conversation redirected
  to sign-in. The readable community introduction is useful background, not
  evidence of version-4 conformance. Do not assume a 3.2 palette implements 4;
  normative access and licensing review precede any formal ArchiMate work.
- The iStar guide distinguishes intentional dependencies from actor links and
  task relationships. A plain goal tag is not an implemented iStar model.
- Service-blueprint visibility and interaction lines describe experience, not
  security enforcement. None of these diagrams proves implementation behavior,
  safety, authorization or compliance.

**Conclusion:** BPMN collaboration is the best familiar process backbone;
S-BPM is the closest conceptual match for participant parity. Combining their
useful ideas is a Thinoda-specific notation unless a separate standards profile
is explicitly selected, implemented and verified.

## 3. Verified Thinoda baseline

The current checkout is the implementation baseline. The JSON interpreter
exists; the scene-container hierarchy is still planned. This plan does not
treat the whole draw.io roadmap as delivered.

| Existing mechanism / evidence | Reuse | Gap |
|---|---|---|
| `thinoda-core/src/stencil.rs:1-12,112-164` | Versioned JSON definitions, bounded interpreter, metadata/interchange declarations, shared drawing output | Metadata declarations alone are not typed semantic instances, an inspector or a rule engine |
| `thinoda-core/src/stencil_object.rs:1-6,29-34,73-80` | Generic shape references and fallback; installed built-in package resolution | Pinned document definitions, library evolution and full parameter/port editing remain incomplete |
| `thinoda-core/src/board.rs:345-349,537-567,643-652` | Existing board objects, selection and flat groups | No nested semantic pool/lane hierarchy; flat grouping must not be reinterpreted |
| `thinoda-core/src/board.rs:1217-1258,1299-1345` | Endpoint identities, fixed ports, routes, waypoints, connector labels/markers | No process/message/dependency edge types; current connector model has no general dash field |
| `thinoda-core/src/model.rs:487-510,668-714` | Zones, hidden-root sheets, canvas, optional domain data | Zones are visual frames, not process ownership or overlapping trust-domain membership |
| `thinoda-core/src/document.rs:447,505,696-751` | Transactional sheet/workbook edits, undo and sheet lifecycle | Semantic references must participate in every mutation and lifecycle path |
| `thinoda-io/src/board_export.rs:15-32` | Content-dependent native version selection; current maximum is 26 | New semantic data requires future native and transport gates, not an arbitrary hardcoded next version in this plan |
| `thinoda-core/src/clipboard/board.rs:39-57,137-156` | Bounded, versioned mixed-board payloads | Copy scope, semantic dependency closure and reference remapping need explicit rules |

Paths in this table are relative to `crates/`. Also reuse existing canvas and
export stencil consumers, scene geometry, Data inspector patterns,
localization/theme infrastructure, import consent and export-loss reporting.
`board_semantics.rs` detects freeform features; it is not already a general
diagram metamodel.

## 4. Concept: one meaning, explicit presentations

### 4.1 Minimal vocabulary

| Concept | Meaning and required distinction |
|---|---|
| Participant | Identified human, team, software/service or API participant; unspecified kind is allowed in a draft. Names are not identities. API participants may be peers when they represent an interaction responsibility; an optional provider/interface link makes their abstraction level explicit |
| Role | Responsibility that participants can fulfill; not a synonym for a person or a lane. Multiple participants can play a role and one participant can play several roles |
| Process scope / pool | One local process owned by a participant; black-box scopes have no internal steps. Ownership is explicit, never inferred from geometric overlap |
| Lane | Optional, nestable responsibility partition within one process scope. It references a performer participant/role and does not create another process owner or trust boundary |
| Step | Task, start/end event or exclusive decision in v1. A task may be manual, assisted or automated. A complete step belongs to exactly one scope; an unallocated draft may have none |
| Sequence | Ordered local behavior between eligible steps in the same scope, including across lanes. Loops are allowed and traversals remain bounded |
| Exchange | Directed communication between distinct scopes, attached to a task or black-box pool boundary. Payload/contract description is text, not an executable HTTP call. Same-owner cross-process exchanges are explicitly labeled internal communication |
| Use case / outcome | Stable record with name, intent/description and participating actors; many-to-many links to steps. Not another control-flow node; structured pre/postconditions are deferred |
| Boundary | Named organization, software-scope, trust or visibility classification with explicit membership. Overlap is valid; each category has its own meaning and display convention |
| Accountability and rationale | Explicit accountable human/team/role reference on a scope, task or outcome; rationale text on a decision. A role needs human/team fulfillment to resolve accountability. Software-only or missing accountability on automated work is a modeling diagnostic, never a claim about legal liability |

Lane performers may be human or software contributors to the pool owner's
process. An independently collaborating participant uses a separate pool, not
a lane to bypass the exchange rules. A lane does not have to mean a role in
BPMN generally; this is the narrower Thinoda responsibility interpretation.
Nested lanes inherit the nearest declared performer unless a more specific
assignment is made; conflicting task/lane assignments are diagnosed.

V1 omits parallel/inclusive gateways, token simulation, choreography tasks,
compensation, BPMN transactions, formal iStar decompositions and executable
S-BPM state machines. A request/reply exchange is not an implicit wait or fork.
Display the supported subset in the product and help.
Structured authority/policy/risk models, exchange correlation IDs and formal
social-dependency analysis are deferred; ordinary descriptions can record
context without pretending those formalisms exist.

### 4.2 Authoritative data and editing invariants

Use an optional, **sheet-scoped typed semantic model** alongside the board,
following the precedent of optional Sankey/FIM domain data. Do not build a
generic ontology platform or put critical references into arbitrary JSON bags.
The exact Rust/schema design is a Phase STC0 decision; these contracts are fixed:

1. Semantic entities/relations have stable IDs and one authoritative value for
   each label/property. A generic occurrence binding references an entity or
   relation ID independently of the stencil reference. Bound objects persist
   no separate semantic label; a shared resolver reads it from the model.
   Unbound shapes retain their existing label behavior. This is a cross-cutting
   reader/writer integration, not a metadata-only change. STC0 inventories all
   label/endpoint readers and writers, including inline edit, search/replace,
   import, accessibility, clipboard and undo coalescing. A revision-bound
   text command rejects stale edits while preserving their drafts.
2. One semantic model belongs to one sheet. V1 has one workflow presentation.
   The later context view shares its entities, not a copied graph. Cross-sheet
   catalogs/global identity and automatic conversions between notations are
   explicitly deferred.
3. There is only one structural tree: scene ancestry. In the canonical workflow
   presentation, a step's nearest bound process/lane ancestor defines its
   structural scope/lane; no duplicate semantic parent tree is stored.
   Process participant ownership, lane performer, role fulfillment and
   overlapping boundary memberships remain separate semantic properties.
   Scene cycles are forbidden; workflow sequence cycles are allowed.
4. V1 uses at most one visible workflow occurrence per step and per scope.
   Actors can recur through distinct scopes/roles without becoming new actors.
   When a participant owns multiple local processes, messages between those
   processes are explicitly labeled internal communication in the Thinoda
   profile, not claimed to be BPMN inter-participant Message Flows.
5. A bound connector references exactly one relation. The relation owns its
   endpoint entity IDs; occurrence IDs/ports/waypoints are presentation data.
   Resolve endpoints through the workflow occurrence map rather than persist
   independently mutable copies. Reconnect is one validated semantic command.
   In v1, Delete on its sole workflow occurrence deletes the relation after
   dependency preview; hiding an occurrence is not deletion. Neither arrow
   shape, line width, label text nor stencil ID infers semantics.
   V1 needs only one default connector-label presentation fed by the semantic
   resolver; positioned multi-label/edge-child support is not a prerequisite.
6. Bound shapes use the shared JSON stencil interpreter. Notation packages
   define symbols, ports, display defaults and property UI declarations.
   Rust supplies reusable typed commands and bounded rule primitives; no new
   `ShapeKind` variants or per-symbol renderer branches.
7. Process scopes and nested lanes consume the common
   [scene-container contract](SCENE-CONTAINERS.md), preserving sheet-space
   geometry, independent header/stack axes and shared free/stack layout.
   Boundary overlays do not become scene parents. Existing Zones/flat groups
   retain their behavior. draw.io import, including BPMN-looking stencils,
   stays unbound until an explicit mapping/promotion preview is confirmed.
8. Moving a scope moves its descendants exactly once and reroutes dependent
   connectors atomically. Moving a step to another scope is an explicit
   semantic reparent command with validation/preview. Dragging over a pool
   does not silently change responsibility or convert sequence into a message.
9. Unsupported semantic versions/capabilities reject writable open; do not
   silently drop data or relabel it as ordinary art. A deliberately created
   presentation-only copy can discard semantics with explicit loss consent.
   Missing symbol definitions retain references and show a labeled diagnostic
   fallback; that is not semantic compatibility.
10. Structurally valid relations can still violate modeling rules: a sequence
    with two existing endpoints in different scopes is retained and diagnosed,
    not silently converted. Missing endpoints instead use an explicit
    `UnresolvedReference` draft value containing bounded source identity/text,
    never a dangling live ID. A complete-model check requires all such drafts
    and modeling errors to be resolved; native draft save remains possible.

The native semantic model is the editable master. Initial definitions should
be bundled and version-pinned; documents must retain the required definition
closure or another verified immutable resolution mechanism before release.
No automatic library download or execution of library code is required.
Converting a bound region to ordinary art materializes resolved labels and
endpoints, with a preview of dependent relation conversion or unresolved drafts;
it never simply removes bindings and leaves blank or broken connectors.

### 4.3 Workflow-first experience

New > Socio-technical > Collaboration starts with an empty, explained workflow
or an original worked example. A progressive palette initially shows
Participant, Process, Lane, Task, Decision, Start, End, Exchange and Boundary;
advanced responsibility/outcome fields stay in Data, not permanent canvas clutter.

Create participants, give each an explicit process or black-box scope, add
work, then connect it. Show **Sequence** and **Exchange** as distinct tools
with readable labels, marker/dash patterns and accessible names. Offer valid
targets and explain invalid ones. A service/API uses the same creation,
connection and inspection operations as a human participant.

Offer Change performer, Assign role, Link use case, Mark boundary membership
and Add exception/fallback as normal undoable commands. Accountable owner and
decision rationale are v1 Data controls, not unspecified future metadata.
A user can express failure using a decision, explicit failure exchange, timeout annotation and
human escalation; do not imply a native BPMN boundary timer event is supported.

Diagnostics select the affected elements and offer explicit fixes, not silent
rewrites. Provide an accessible relationship list for keyboard editing without
requiring drag/drop or relying on canvas color. Inspectors, palette, diagnostics,
help and export notices use existing localization and Light/Dark/System behavior.

**Validation has two levels:**

- Structural safety errors (dangling live IDs, scene cycles, invalid enum/version,
  nonfinite coordinates, exceeded budgets) reject transactions/import atomically.
- Modeling issues (unassigned performer, missing outcome, unfinished branch,
  a sequence now crossing scopes) remain visible draft diagnostics. Guided
  tools prevent creating obvious violations, but imported/in-progress models
  can be repaired and saved natively. Formal-profile export may block on them.

Reachability diagnostics use bounded graph traversal/SCC analysis, not
enumeration of execution paths. Unreachable steps and cycles with no route
to an end are warnings about the modeled process, not proof of deadlock.
Collapsed scopes keep their modeled behavior; black-box scopes have none.
Ungroup/reparent/delete uses the shared binding-aware command guard and
previews any unallocated drafts or newly invalid relations.

Privacy matters: hiding a boundary, filtering a participant or using a slide
crop is not redaction. Masked copies must cover all new semantic text, links,
properties and retained extension data.

### 4.4 Example and reference acceptance corpus

Use original fictional **service-access approval**, not a purchased diagram:
Requester submits an access request to an Access API; the service evaluates
policy; a Reviewer handles exceptions; the service grants or refuses access
and notifies Requester. A timeout/failure path escalates to human support.

The outcome is "obtain appropriate access", with an explicit human/team
accountable for policy. Software-scope, external organization and trust
boundaries are independently named. A role is not automatically equivalent to
one named employee. Payload descriptions contain no actual credentials.

```mermaid
flowchart LR
    U[Requester / human]
    A[Access API / software participant]
    R[Reviewer / human role]
    U -->|request access| A
    A -->|review exception| R
    R -->|decision| A
    A -->|grant or refusal| U
```

This is the **context sketch**, not a BPMN diagram or the complete workflow.
The workflow must explicitly show local policy evaluation, decision branches
and failure/escalation behavior. Do not infer this internal sequence from the
four context exchanges.

Three required fixtures: this human-in-the-loop workflow; a black-box external
API failure/retry/escalation scenario; and a service blueprint-style support
journey used initially only to check that visibility is not confused with
trust. Add negative variants for crossed sequence scopes, same-name actors,
overlapping boundaries and removed participants. Blueprint editing is not
part of the initial fixture's acceptance claim.

### 4.5 Views without dishonest conversions

After workflow acceptance, add a context view with independent geometry and
the same participant identities. Aggregate exchanges with a count and drilldown;
do not invent global execution order, merge away conditions, or infer internal
processes from a black-box actor. Context edits may rename participants and
edit existing exchanges. Creating internal tasks or sequence is workflow-only;
context-created communication is explicitly unallocated until assigned there.

Service-blueprint views later require explicit journey stages, visibility,
touchpoints and evidence, not just recolored pools. Goal/dependency views
require explicit goals and dependencies, not sequence arrows in disguise.
Additional notations need independent schemas, mapping/loss rules and gates.

## 5. Lifecycle, interoperability and bounded operation

| Operation | Required contract |
|---|---|
| Native save/load | Feature-based new version gate, strict semantic validation and canonical round trip; ordinary sheets keep existing versions |
| Undo/redo and autosave/recovery | Graph, bindings, containers and geometry commit together; drafts remain tied to document/sheet/entity/revision; account for semantic bytes in history |
| Copy/paste, duplicate, merge | Compute a bounded dependency closure; fresh entity/occurrence/relation IDs for copies; remap every internal reference. Include required actor/role/use-case definitions and explicit unresolved draft references rather than a whole-sheet payload. Closure overflow rejects, never truncates. No name-based deduplication or automatic linking to destination actors |
| Cut and partial selection | Cut uses existing verified clipboard safeguards. Preview expanding selection/removing dependent relations. A step or lane copied without its scope becomes an explicitly unallocated draft on paste, never silently assigned to an unrelated pool |
| Delete, reparent, convert | Preview dependent relations and memberships; default to preserving affected work with unresolved references/unallocated drafts, with explicit cascade as an alternative. Structural references never dangle. Bound-container ungroup needs a chosen target or consented unallocation. Converting to plain art requires loss consent |
| Search, rename, outline/accessibility | Resolve bound text once; diagnostics/relationships remain navigable; hidden view occurrences do not duplicate results |
| Masking and outbound sharing | Apply masking to semantics and retained data, not only painted labels; viewing filters/crops are not data removal |
| SVG/PNG/PDF/print/Pitch | Shared geometry, labels, typed edge styles and legend survive; output is presentation-only and carries no editable semantic guarantee |
| draw.io | Nested scene structure/geometry exchange through SC0-SC3 first, without semantics. Semantic export requires itemized loss reporting. A future versioned extension may preserve the model, only with ID remapping and edited/deleted/reparented-cell conflict detection |
| BPMN XML | Optional, late, declared subset with semantic plus BPMN DI mapping and explicit lane-performer mapping; XML schema validity alone is insufficient. Internal communication and unsupported constructs cannot silently become conforming BPMN Message Flows |
| Other editable formats | Capability preflight rejects or obtains explicit consent for a presentation-only conversion; no success-shaped silent semantic stripping |

Set cumulative entity, edge, boundary-membership, property-byte and reference-
closure budgets in STC0, coordinated with current workbook/clipboard limits.
Validation and derived views use bounded indexed traversals; avoid all-path
enumeration for cyclic processes. Parse imports as inert data, bound XML/ZIP
expansion, do not fetch payload URLs, and do not execute API descriptions.

## 6. Phased execution and exit gates

All phases below are **not started**. Estimates are relative complexity, not
delivery promises. Assign one accountable implementer per phase; use existing
test/CI tooling. Every phase ends with evidence, a written critique and an
owned-path commit. Failure of an exit gate keeps that phase open.

| Phase / size | Prerequisites | Scope and principal locations | Measurable exit gate |
|---|---|---|---|
| STC0 - Contract and integration spikes / M-L | None; coordinated with SC0 | Pin vocabulary/subset, original corpus, common hierarchy/binding design, reader/writer inventory, budgets, compatibility and keyboard interaction design; bounded spikes across core/canvas/IO/app | Demonstrate bound rename/reconnect across edit/search/export/copy/undo, one nested pool/lane move with exchange, and keyboard reparent. Freeze one contract with SC0. Stop/rescope if existing writers bypass authority or safe integration requires an unbounded editor rewrite |
| STC1 - Consume the common scene milestone / L, shared not additive | STC0; SC0-SC3 | Use the single shared tree, nested stack swimlanes, collapse/routing/keyboard commands, connector styling and structural persistence delivered by SC0-SC3 | Pass the scene-only nested draw.io import/edit/export/reimport gate without any semantic model. No second lane engine, schema field or duplicated dash implementation |
| STC2 - Semantic core and safe persistence / L | STC0, STC1 | Optional typed graph/bindings, accountable-owner relations, decision rationale, unresolved-reference drafts, transactions; native/transport gates, masking/history and loss guards | All lifecycle rows in section 5 have focused positive/negative tests against real nested containers; malformed/older envelopes reject without mutation; old plain-board fixtures unchanged. No palette exposes unsafely persisted semantics |
| STC3 - Complete workflow authoring / L | STC2 | Progressive palette, nested lane/performer commands, Data inspector including accountability/rationale, keyboard relationship list, decision branches, exchange labels and diagnostics | Build all workflow reference scenarios without raw JSON; each semantic command undoable; distinguish sequence/exchange, actor/role, boundary/scope and collapsed/black-box; invalid drafts save and repair |
| STC4 - First-class v1 release / M-L | STC3 | Start-page **Software** template group with original socio-technical examples, help/legend, native/web end-to-end coverage, accessibility, localization, presentation output and performance | Pass the release checklist below on native and web; publish exact supported/unsupported features and known limitations; written critique has no unresolved data-loss or semantic-integrity blocker |
| STC5 - Linked context view / M-L | STC4 | Second presentation, shared identity, independent layout and honest aggregation; persisted active view only if required | Rename reflects in both views; workflow conditions survive aggregation; layout/filter changes never mutate behavior; deleting an occurrence is distinct from deleting its entity; clipboard/export/view-state cases covered |
| STC6 - Interoperability profile / L | STC4; relevant DRAWIO fidelity slice for draw.io extension | Decide demand first; bounded BPMN XML subset and/or conflict-safe draw.io semantic extension. These are independent adapters, not one mandatory bundled deliverable | External tool opens supported original fixtures; supported import/edit/export/reimport preserves meaning and DI; unsupported data produces exact blocking/loss diagnostics; edited extension baselines cannot resurrect stale semantics |
| STC7 - Experience and intention extensions / L, separately scoped | STC5; separate profile decision | Choose blueprint OR goal/dependency view based on use; add required explicit data and mappings. Formal S-BPM/ArchiMate are separate future proposals | Version-pinned vocabulary and independent scenario/round-trip tests; no conversion guesses visibility, intentions, authority or execution order |

STC1 names consumption of the shared milestone, not a second implementation
estimate. Container-independent semantic prototypes can overlap SC work, but
STC2 cannot pass its lifecycle gates before STC1. Avoid concurrent ownership
of the same schema files. Do not wait for full table authoring (SC4), unrelated
draw.io libraries, full foreign-format fidelity or every legacy-shape migration.
The bounded SC3 gate is intentionally retained to prove the common scene's
draw.io mapping before semantic adoption; it is not full draw.io acceptance.
STC2's lifecycle coverage includes explicit refusal/loss preflight for deferred
adapters, not implementation of BPMN XML or semantic draw.io extensions early.

**Release boundaries:** STC3 is a usable development slice, not "first class".
STC4 is the first proposed release. STC5 is a linked-view enhancement.
STC6 and STC7 are optional subsequent investments, not prerequisites for v1.

### Software starter templates

User-requested addition: add a **Software** template group to the start page,
using the existing template catalog, category filtering, previews and localization.
Include original examples for human/service handoffs, an approval workflow and
incident response. Demonstrate peer human/software participants, nested
responsibility lanes, scope/boundary distinctions and labeled interactions using
the actual supported editor model, not flattened artwork.

Implement the examples alongside the relevant authoring capabilities in STC3,
with start-page integration and native/web acceptance completed by STC4. Each
template must create an independent, editable document and survive save/reopen,
undo, copy/paste and presentation export. Show a short legend and disclose the
supported Thinoda profile rather than claiming BPMN conformance. Do not expose
semantic examples before their data can be safely persisted.
Do not advertise a context editor or BPMN compatibility before their gates pass.

### V1 release checklist

- All three reference scenarios and negative variants have inspectable source
  models and expected diagnostics. Human and API participants use equivalent
  authoring operations. The example's automated step has a human/team accountable
  reference and the decision has rationale; software-only accountability is
  explicitly diagnosed, including unresolved-role fulfillment.
- Save/reopen, recovery after interruption, undo/redo, cross-document paste,
  sheet duplicate/merge, delete/reparent and native version downgrade refusal
  preserve meaning or fail explicitly. No orphaned references or stale drafts.
- Deliberate cross-scope sequence and invalid exchange endpoints are diagnosed;
  rename and overlapping boundary membership never change topology. Loops,
  disconnected components, empty/black-box pools and repeated labels work.
- Both the native app and web app can complete the create/edit/save/reopen
  journey. Keyboard-only editing and relationship navigation, focus handling
  and screen-reader output receive actual assistive-technology checks; synthetic
  input alone is not evidence of physical touch/pen or screen-reader support.
  Native and web parity is deliberate for v1; if one fails, narrow the stated
  release target explicitly rather than quietly declaring both supported.
- All supported UI languages and Light/Dark/System are exercised. Semantics
  remain understandable without color; incomplete/filtered views say so.
- A masked copy of the access-approval fixture contains none of its distinctive
  original semantic strings in decoded native/clipboard data or extracted
  SVG/PDF text/metadata, including accessibility titles and retained source
  properties. Binary compression is not an adequate masking check.
- SVG/PDF plus one raster output, print and Pitch agree with canvas on nested
  scope bounds, connector types, labels and legend. Clipping is tested, not
  treated as redaction. Editable non-native loss preflight covers semantic data.
  Collapsed scopes have a visible collapsed indicator distinct from black-box.
- Proposed performance acceptance fixture: 200 steps, 30 participants, 400
  relations, 40 overlapping boundary memberships, including loops and parallel
  exchanges. On named reference native hardware and browser, p95 committed-edit
  feedback <=100 ms and p95 pan frame <=33 ms over 100 edits/10 seconds of pan
  at a declared viewport. STC0 must ratify or revise these targets with measured
  evidence before feature work; they are not current performance claims.
  Add a 10x-size stress point to measure degradation and limit behavior without
  promising the same latency at that scale.
- Parser/model/clipboard limit tests use the exact STC0 budgets at limit and
  limit+1. Add mutation/fuzz cases through existing tooling for newly introduced
  boundary parsers, rather than broad unrelated test additions.

## 7. Risk register and stop conditions

| Risk | Mitigation / stop condition |
|---|---|
| Attractive symbols conceal invalid meaning | Typed relations, explicit subset and visible diagnostics; no release with silent sequence/message conversion |
| Unbounded "support every notation" project | Workflow-only v1; optional views/adapters separately approved and gated |
| Duplicate semantic and canvas state drifts | Single-owner labels/properties and tested bindings; reject STC0 architecture if existing edits bypass the authority |
| Shared hierarchy collides with draw.io plans | One scene hierarchy, explicit shared ownership; finish the minimum slice instead of implementing another pool tree |
| API equivalence erases social responsibility | Peer interaction operations, distinct authority/accountability fields, explicit human escalation and policy ownership |
| Metadata leaks through hidden views or clipboard | Dependency closure preview, full masking and export preflight; never describe filtering as anonymization |
| New versions load "successfully" in older readers | All native/recovery/clipboard/private snapshot gates change together; test forged old envelopes |
| Two views promise more consistency than implemented | Delay second view until STC5; aggregation is navigable, not an automatic workflow reconstruction |
| Foreign-format extension becomes stale | Native master; ID mappings plus baselines/conflict diagnostics before editable extension support |
| Performance/a11y deferred until end | Prototype bindings/containers in STC0; test keyboard and latency throughout; actual platform acceptance is a release gate |
| Standards/licensing assumptions creep into product | No conformance claim without versioned mapping, accessible normative evidence and licensing review |

The [rubber-duck review](SOCIO-TECHNICAL-REVIEW.md) records independent
challenges, dispositions and residual decisions. This is a recommended
direction, not authorization to implement the phases or create issues/PRs.

## Sources

[bpmn]: https://www.omg.org/spec/BPMN/2.0.2
[sbpm]: https://i2pm.net/2020/03/28/subject-oriented-business-process-managemet/
[archimate101]: https://archimate-community.pages.opengroup.org/workgroups/archimate-101/
[archimate4]: https://www.opengroup.org.cn/node/12886
[blueprint]: https://www.nngroup.com/articles/service-blueprints-definition/
[istar]: https://arxiv.org/html/1605.07767v3
[uml]: https://www.omg.org/spec/UML/2.5.1
[c4dynamic]: https://c4model.com/diagrams/dynamic

- [OMG BPMN 2.0.2 normative PDF](https://www.omg.org/spec/BPMN/2.0.2/PDF/):
  collaboration/process semantics and serialization; no full conformance claimed.
- [S-BPM primer](https://link.springer.com/content/pdf/10.1007/978-3-642-29133-3_14.pdf):
  interaction versus subject behavior, indexed excerpt checked; full detailed
  behavior review remains a prerequisite for a formal profile.
- The association links the
  [S-BPM book](https://link.springer.com/book/10.1007/978-3-642-32392-8);
  direct retrieval failed with HTTP 406 during this research.
- [ArchiMate 4 chapter URL](https://pubs.opengroup.org/architecture/archimate4-doc/chap04.html):
  redirected to sign-in; not used as evidence for implementable version-4 rules.

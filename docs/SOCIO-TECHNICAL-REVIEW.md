# Rubber-duck review: socio-technical concept and common swimlanes

Date: 2026-11-13. Scope: [concept and phases](SOCIO-TECHNICAL.md),
[common structural solution](SCENE-CONTAINERS.md) and their integration with
the [draw.io plan](DRAWIO.md). This is a design review, not runtime acceptance.

An independent rubber-duck reviewer challenged the first concept, then added
a structural review following the user's nested-swimlane clarification. The
author investigated draw.io documentation/source and the existing example
corpus, and revised the plans. Findings and dispositions below distinguish
resolved design contradictions from still-unproven implementation decisions.

## 1. Verdict and strongest objection

**Recommended direction: workflow-first semantics on one common nested scene
engine, with no standards-conformance claim.**

The initial draft was **not ready for execution**. Its largest problems were
underestimating shared container foundations and text/endpoint integration,
duplicating scope ownership implicitly, and treating the social dimension as
an optional bag of metadata.

The revised design makes those investments explicit. It is not a cheap shape
palette task. SC0/STC0 must prove the command and representation contracts
before feature implementation is authorized. No release, compatibility,
accessibility or performance gate is closed by this review.

## 2. Ranked findings and dispositions

| ID / severity | Challenge | Disposition in revised plan |
|---|---|---|
| B1 / blocker | Semantic labels would conflict with existing `CanvasObjectKind::Shape.label`, connector labels and their many writers | Bound occurrences have a separate generic semantic reference and no independently stored semantic text. Shared read/write resolution and revision-owned drafts are explicit; STC0 inventories inline editing, search/replace, import, clipboard, accessibility and undo. Stop/rescope if this cannot be integrated safely |
| B2 / blocker | Relation endpoints and connector endpoints had no declared authority; deleting/reconnecting a view could corrupt the model | Relation owns semantic endpoints; ports/routes/occurrence resolution are presentation data. Reconnect is one command. V1 Delete removes the sole workflow relation with dependency preview; hiding is distinct. STC5 adds multiple-view deletion gates |
| B3 / blocker | "Reuse the draw.io hierarchy" concealed a substantial unimplemented prerequisite | Replaced by a named common milestone SC0-SC3 with draw.io-only acceptance, consumed by STC1, not duplicated. It includes safe persistence, selection/hit-testing, policies, collapse, routing, native/web authoring and interchange |
| B4 / blocker | Complete ownership conflicted with unallocated drafts; exchanges referenced unsupported message events; same-owner messages and lane ownership were ambiguous | At most one derived scope; no scope is an explicit draft. V1 exchanges attach to tasks or black-box scopes, not unimplemented event types. Lanes partition the owner's process; independent collaborators use pools. Same-owner cross-process communication stays visibly Thinoda-specific |
| B5 / blocker | One future format bump was implicitly covering several independent features | Common capability table separates ancestry, layout/dashes, semantic graph and pinned definitions across native, recovery, clipboard and private snapshots. Assign actual versions only when implementing; older envelopes must not silently accept new data |
| B6 / blocker | "Socio-technical" risked becoming BPMN-lite with human icons | Accountable-owner references and decision rationale are explicit v1 data/UI/gates. Software-only/missing accountability on automation is diagnosed; roles require human/team fulfillment to resolve it. Broader authority/risk/dependency formalisms are deferred |
| C1-C2 / blocker | Visual containment and semantic membership were insufficiently separated, with two implied parent trees | Only `SceneParent` defines structural ancestry. Optional bound process/lane ancestors give that ancestry semantic meaning in the canonical workflow. Participant ownership and overlapping boundary memberships are independent properties, not another tree |
| C3 / blocker | Header orientation, nested layout, resizing, auto-grow, collapse and clipping were underdefined | Shared contract separates header side from stack axis, free/stack/table policies and source-specific growth. It defines bounded layout scheduling, expanded bounds and fold routing. Collapse is explicitly not black-box |
| C4 / blocker | Edge moves across multiple selected containers could apply twice or lose attachments | One deduplicated transaction updates edges; cross-container edge parent uses the nearest common ancestor/layer. Actual terminals survive collapse. Manual controls translate once when both endpoints move; moving one endpoint preserves unrelated controls |
| C5 / blocker | draw.io swimlanes or BPMN-looking symbols might accidentally become process semantics | Import is unbound scene data. Explicit promotion maps containers/performers/edge meanings with preview. draw.io structural round trip is accepted independently of semantic code |
| C6-C7 / important | Dash fields and phase ownership could be implemented twice | Shared scene milestone owns connector styling; STC1 consumes it. One scene/schema owner and an adapter owner are assigned at SC0; semantic lifecycle completion waits for the common milestone |
| C8 / important | Flat groups, Zones and mind-map parents could become a third hierarchy | Preserve their existing meanings. New nested grouping uses scene containers. Zones retain current explicit membership fields; their existing members are not erased or reinterpreted as nested parents |
| C9 / release blocker | Older readers, partial clipboard selection and recovery might discard hierarchy | Structural capability gates precede shared authoring. Container copy includes descendants; a child alone becomes layer-level. Semantic partial paste uses explicit unallocated/unresolved drafts. Budget overflow rejects, never truncates |
| C10 / important | Container keyboard access was deferred to the semantic editor | Containment navigation and keyboard reparent live in SC1/SC2 for draw.io too; STC0 spikes accessible editing and STC3 adds semantic relationships |

These are **design dispositions**, not assertions that code now satisfies them.
B1, B2, B3 and C3 remain high implementation risks with explicit spike gates.

## 3. Secondary critiques and decisions

**N1 - Drafts versus invalid storage.** A sequence can have two valid live
endpoints yet violate the same-scope rule; store it and diagnose its modeled
meaning. Missing endpoints instead have a typed unresolved draft reference.
Scene cycles/dangling live IDs reject structurally. Reachability/SCC analysis
is bounded; a warning about an endless loop is not a verified deadlock proof.
For unknown semantic versions, refuse writable open. Presentation-only
conversion requires a bounded safe reader and loss consent; do not promise
recovery from every corrupt/unknown source.

**N2 - Partial edits and identity.** Copying one lane without its process is
an unallocated draft, not an invented process. Closure expansion is previewed
and bounded. There is no name deduplication or automatic destination linking.
Deleting a participant defaults to preserving affected work using draft
references, with explicit cascade as another choice. Rich "merge into existing
participant" UX is not required for v1.

**N3 - Masking.** The release gate searches decoded native/clipboard data and
extracted SVG/PDF text/metadata for distinctive fixture strings. The check
includes hidden data, accessibility titles and retained source properties.
Raw binary grep alone is insufficient because compression can conceal text.

**N4 - Accessibility and scale.** Spike keyboard target selection/reparenting
before building the palette. Keep actual assistive-technology/platform checks
and the proposed latency fixture, plus a 10x stress point. Named hardware,
browser, viewport and measurements are still required; no synthetic probe or
diagram size establishes runtime quality.

**N5 - Phase dependencies.** The original suggestion that semantic persistence
could finish independently of hierarchy was rejected. Prototype work can
overlap; full STC2 lifecycle acceptance depends on the SC0-SC3 milestone.
SC1 structural persistence cannot expose writable nested authoring before
its own safe version/undo/load paths work.

**N6 - Minimum coherent release.** Kept boundaries and outcome links because
the user explicitly needs them, but narrowed them to categorized membership
and name/intent links. Deferred structured pre/postconditions, exchange
correlation IDs and broader social reasoning. Native/web parity remains an
explicit cost and gate, not a casual promise. Linked context view and foreign
semantic formats are optional after v1.

**N7 - Standards.** Keep BPMN sequence/message distinctions without claiming a
new hybrid is standard BPMN. Nested lanes are not pools. A lane-to-performer
constraint is Thinoda's interpretation, not a universal BPMN requirement.
Same-owner internal communication is visibly non-BPMN. S-BPM inspiration is
not formal state-machine support; gated ArchiMate source access is not a
validated version-4 implementation specification.

**N8 - Documentation truth.** Corrected directly related stale overview claims:
the JSON interpreter exists, but shared scene hierarchy remains planned.
Definition metadata alone is not a semantic inspector. The common document
owns structural refinements; DRAWIO and the socio-technical plan link to it.

One reviewer shorthand described Zones as having "no members". Source
inspection shows `Zone.roots` and `Zone.objects`; that shorthand was not
adopted. The invariant is **no nested scene-parent ownership inferred from
Zones**, not removal of existing Zone functionality.

## 4. Adversarial walkthrough

| Scenario | Required outcome / acceptance owner |
|---|---|
| Inspector rename while an older inline text draft is open | Revision-owned semantic command refuses the stale write, retains the draft and keeps one authority; STC0/STC2 |
| Two participants both named Admin, one human and one API | IDs remain distinct through search, import, paste and sheet duplication; STC2 |
| Ordinary draw.io pool > lane > sub-lane, no promotion | Preserve scene hierarchy/layout and show no socio-technical diagnostics; SC3 |
| Same shape drawn over a lane using overlap mode | Not a scene child and therefore not a member; geometry never assigns ownership; SC2 |
| Header on left but children stacked vertically, or mixed axes at next depth | Separate header and stack policies, no swapped axes or repeated header offsets; SC2/SC3 |
| Swimlane drop grows its parent, table-cell drop does not | Policy-specific behavior, not a global auto-grow heuristic; SC2/SC4 |
| Select pool, nested lane, child and crossing edge, then move | Each descendant/control translates at most once; one undo restores all geometry; SC1/SC2 |
| Reparent task changes the meaning of its existing sequence/exchange | Preview the change, preserve edge types and produce modeling diagnostics or cancel; never auto-convert; STC3 |
| Ungroup bound pool through a generic drawing command | Require target reparent or consented unallocation; no unguarded orphaning; STC3 |
| Collapse a scope whose task has an external exchange | Keep actual task endpoint, project visible routing to ancestor boundary; expand restores attachment; no conversion to black-box; SC2/STC3 |
| Copy task/exchange without target scope or participant | Bounded dependency closure, remapped IDs and typed unresolved/unallocated drafts; no dangling live IDs or guessed owner; STC2 |
| Software chosen as sole accountable owner of an automated policy step | Actionable diagnostic, not silent acceptance as human accountability; role fulfillment checked; STC3 |
| Hide a trust boundary, then export/share the source | Hidden data is still data; loss/redaction notices and masking remain explicit; STC2/STC4 |
| Reparent/delete cells externally after exporting a future semantic extension | Baseline conflict prevents restoring stale ownership/elements; require reconciliation or scene-only copy; STC6 |
| Forge old clipboard/native version around new hierarchy/semantics | Reject without partially mutating destination/document; SC1/STC2 |
| Cyclic process, cyclic parent tree, excessive nested depth | Process cycle is allowed and traversed boundedly; parent cycle/depth overflow rejects structurally; SC1/STC2 |

## 5. Phase-by-phase planning critique

| Phase | Main challenge / condition for proceeding |
|---|---|
| SC0 + STC0 | Highest-leverage work. Resolve legacy card occurrences, text/endpoint authority, supported source policies and one shared owner. Stop instead of quietly weakening identity/lifecycle guarantees |
| SC1 | Hierarchy serialization and generic commands are a substantial feature. Test older-envelope rejection, legacy groups and keyboard movement before exposing it |
| SC2 | Nested resize/collapse is the highest interaction risk. Bound scheduling and real endpoints are more important than adding more visual symbols |
| SC3 | The shared engine must prove value to draw.io without semantic code. Initial appearance or schema-valid XML is not structural/behavioral round-trip acceptance |
| SC4 | Tables reuse the same engine, not another containment model. Defer full row/column authoring rather than delaying workflow v1 indefinitely |
| STC1 | A dependency/milestone consumption marker, not extra implementation of the SC work. Do not double-count its cost or assign a second owner |
| STC2 | Do not call persistence done before copy/delete/reparent/recovery/masking cover real nested scenes. Unknown-version fallback must not become silent loss |
| STC3 | Complete the whole authoring loop, including accountability/rationale and drafts. More icons cannot compensate for missing safe reconnect or keyboard editing |
| STC4 | "First class" is a release assertion requiring platform, accessibility and artifact evidence. Missing gates keep the release open or explicitly narrow its advertised target |
| STC5 | Independent geometry over shared meaning is useful but raises deletion/aggregation ambiguity. Preserve the canonical workflow and distinguish occurrence removal from model deletion |
| STC6 | Only fund an adapter when demand justifies it. BPMN XML and draw.io semantic extensions have different mappings/gates; no arbitrary interoperability guarantee |
| STC7 | Choose a concrete blueprint or intentional-modeling extension rather than adding all notations at once. Each needs actual data, not a recolored workflow |

## 6. Residual decisions and evidence limits

- SC0/STC0 still need named owners, numerical budgets/tolerances, an inventory
  of text/endpoint consumers, actual keyboard prototypes and measured reference
  hardware. These are execution-phase outputs, not unanswered user requirements.
- Header/stack mappings are supported by documentation/source inspection; no
  live draw.io renderer screenshot or behavior recording was produced here.
  SC3 must compare fixtures in a pinned draw.io build.
- Current corpus evidence confirms nested source structures, not that Thinoda
  already preserves them. Table cases must be classified separately from
  stack lanes. Do not label all 88 occurrences as accepted.
- Full formal notation profiles and executable workflows remain excluded.
  Modeling accountability is a communication aid, not a legal/compliance check.

## 7. Final independent delta review

The same reviewer read the new common contract and revised concept, plus the
DRAWIO/PLAN crosslinks. Its verdict was **no remaining blockers to accepting
the documents as a plan**, with several non-blocking wording and execution
decisions. No additional reviewer or parallel implementation was launched.

Author dispositions following that final review:

- **Critical path:** retained bounded SC3 before semantic adoption deliberately
  to prove one common native/draw.io structure. Added the rationale and made
  clear that complete foreign-format save-back is not its gate.
- **Earlier draw.io dependencies:** named the relevant Phase 3 model,
  Phase 4/5 rendering/interpreter and Phase 2/6/7 adapter capabilities. SC0
  itself has no implementation prerequisites; full parent-phase closure is
  not required.
- **Mutation bypass:** SC1 now requires the common binding-hook interface on
  every generic mutation path, tested with unbound no-op and stub bound cases.
- **Collapsed geometry and edge ancestry:** specified hidden-descendant and
  expanded-bounds movement, one undo for induced layouts, stored edge parents
  recomputed on explicit endpoint reparent, stable moved-edge ordering and
  normalized repeated interchange checks.
- **One-tree corner cases:** unbound intermediate groups are transparent;
  v1 semantic bindings target supported free/stack containers, not table
  cells. Reparent previews inherited performer changes. Converting to plain
  art materializes labels/endpoints instead of stripping bindings blindly.
- **Scope and ownership:** a single default bound connector label suffices;
  advanced edge-child fidelity is deferred. SC2 owns immutable definition
  resolution and STC2 consumes it. SC0 decides whether unreleased SC1/SC2
  share one version, and explicitly stops/rescopes card integration if needed.
- **Shared quality gate:** SC2 includes a nested depth-by-child-count latency/
  operation-budget fixture; the later semantic performance gate is not its
  substitute. Added a pointer in the actual DRAWIO Phase 10 section.

These author-applied final clarifications were not submitted for another
independent pass. The plan is coherent at the design level; its spikes,
implementation gates and release acceptance remain open. Implementation
approval was a separate user decision, subsequently granted.

## 8. First implementation checkpoint critique

**Implemented:** a shared immutable object-only hierarchy prototype in
`thinoda-core`, with bounded structural validation, stable traversal and atomic
move/reparent previews. Geometry translation reuses the board helper rather
than introducing a separate transform implementation. The hierarchy borrows
the canvas immutably, so candidate failures cannot mutate a document.

**Rubber-duck questions and dispositions:**

- Does this already deliver nested swimlane editing? No. There is no live
  command, authoring UI, persistence, header/layout/collapse policy or semantic
  binding. SC0 is partial and SC1-SC4/STC release gates remain open.
- Could saving silently flatten newly authored hierarchy? The prototype cannot
  be attached to a workbook or serialized through the current model. No live
  command may adopt it until the complete lifecycle/version gate is implemented.
  A translation preview alone must never be treated as a saved nested document.
- Do overlapping parent/child selections double-translate geometry or manual
  connector controls? The closure and the implicit internal-edge set are
  deduplicated before the existing translation helper runs.
- What if an endpoint is another connector? A dependency queue propagates rigid
  translations independently of source order. A selected edge whose controls
  move while a terminal stays fixed is not classified as rigid; downstream
  manual controls remain fixed instead of acquiring an incorrect translation.
- Can a child escape a locked container, or can one locked child prevent a whole
  parent move? Source/target ancestry is checked for direct edits; a selected
  ancestor carries its individually locked descendants as one unit.
- Do legacy groups or cards create another ownership system? Legacy editor
  behavior is unchanged. The prototype explicitly refuses hierarchy membership
  in flat groups and movement requiring a mixed-group card occurrence. This is
  a documented spike boundary, not the final grouping/card integration.
- Is the bounded implementation accidentally quadratic? Indexed identities and
  lock sets avoid repeated linear canvas lookup during per-object ancestry
  checks. Parent chains stop after 32 links; traversal is iterative. The
  maximum-object fixture is not a substitute for SC2's measured latency gate.
- Is reparent already a complete transaction? No. Geometry-preserving topology
  previews are present, but stored edge-parent normalization, target insertion
  ordering, binding consequences, history and persistence were later gates at
  this first checkpoint. The next checkpoint below advances the preview contract,
  not persisted editing.

This is an author critique, not another independent review. The previous
independent verdict accepted the plan only; it does not certify this code or
close the implementation gates.

## 9. Command-contract checkpoint critique

**Implemented:** stable child ordering across chained previews; reconciliation of
affected connector parents, including connector-to-connector dependencies; and
one shared read-only validation context for reparent/translation previews.
Candidate geometry stays in sheet space and source data stays immutable.

**Rubber-duck questions and dispositions:**

- Does a reparent reset a previous edit's sibling order? No. The candidate
  clones the current hierarchy and appends actually moved roots in prior
  traversal order. Same-parent requests do not reorder siblings.
- Does the edge solver normalize an entire imported document? No. Only edges
  whose endpoint ancestry changes are reconciled. A geometry move leaves all
  imported parents unchanged; unrelated edge parents survive reparenting too.
- Could edge dependencies make the result depend on source array order?
  Ownership uses a dependency queue, but insertion uses prior traversal order.
  Duplicate endpoint dependencies are counted separately. Missing targets and
  connector cycles reject the preview instead of producing partial ownership.
- Can a late consequence violate depth or lock constraints? The full candidate
  is validated after edge reconciliation and before binding validation. A
  locked external edge or old/new ancestor can reject the edit; locked edges
  carried by a selected structural ancestor keep their locks and move as a unit.
- Can ownership be unchanged while paint order changes? Yes, if an explicitly
  selected edge is normalized back to its original owner. The before/after
  child lists disclose this ordering consequence, even with no final owner delta.
- Can bindings observe only part of the edit, or mutate it during validation?
  No. The one-shot callback receives reconciled read-only structure, geometry
  and consequences. A stub-bound policy can detect a nested task's inherited
  scope change through intermediate containers and reject the whole candidate.
- Is this already the production semantic transaction hook? No. Only the two
  immutable preview mutations are wired. Live delete/paste/ungroup, history,
  confirmation UI, actual semantic diagnostics and persisted format gates are
  still required. Unbound convenience methods must not be used by bound commands.

**Critique:** this removes concrete command-contract gaps without shipping an
unsafe intermediate document format. It also exposes a real distinction between
dependency order and paint order that a flat-parent rewrite would lose.
The remaining risk is integration, not missing visual symbols: SC0 is still
partial, and SC1 cannot be declared complete from these previews. This is an
author review, not a new independent rubber-duck sign-off.

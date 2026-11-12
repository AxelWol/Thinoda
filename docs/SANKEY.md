# Sankey family: research, design and implementation plan

Status: in progress, requested 2026-11-11. This extends, rather than replaces,
the [main plan](PLAN.md) and [draw.io plan](DRAWIO.md). A phase is not complete
until its implementation, relevant tests and written Rubber Duck review pass.
The existing browser-storage repair and its release acceptance remain separate.

## 1. Scope and confirmed decisions

Support editable Sankey flows, staged alluvial diagrams, probability flows and
circular/feedback flows. A broader "flow map" need not encode geographic
coordinates; geographic flow maps are not implied by Sankey layout.

The supplied screenshot is a visual reference for smooth, differently sized
bands, labeled rectangles, branching, merging and long connections. It is not
a source dataset: no probabilities can be inferred reliably from its pixels.
Templates use original, explicitly fictional data, not its text or artwork.

The user confirmed this weight model:

> One shared 0-100 flow weight per link, editable from either Parents or
> Children; each side totals at most 100. Add an explicit probability mode for
> conditional branches.

Names, notes and hyperlinks reuse Thinoda topics. A node may have many
incoming and outgoing **graph** links; this does not change the mind-map
tree's single ownership parent. Weight editing must never reparent a topic.
Repeated names do not identify nodes. Alluvial stages can contain distinct
nodes with the same name.

## 2. Research and implications

Sources checked 2026-11-11:

| Source | Finding | Thinoda implication |
|---|---|---|
| [SankeyMATIC manual](https://sankeymatic.com/manual/) | Sources and targets are linked by numeric amounts; thickness represents amount | Keep one authoritative value per directed link, not two endpoint percentages |
| [SankeyMATIC imbalances](https://sankeymatic.com/manual/imbalances.html) | Equal inputs/outputs matter for conserved quantities; funnels may intentionally be unbalanced | Display incoming, outgoing and their difference; do not silently normalize or invent balancing flows |
| [SankeyMATIC scaling](https://sankeymatic.com/manual/scaling.html) | A shared amount-per-pixel scale permits meaningful comparisons | One linear weight-to-width scale within each diagram; no per-node width normalization |
| [SankeyMATIC colors](https://sankeymatic.com/manual/colors.html) | Source, target, uniform and individual link colors are useful | Retain normal Thinoda styles; add an optional quantitative color scale with explicit precedence |
| [D3 Sankey API](https://github.com/d3/d3-sankey) and [implementation](https://github.com/d3/d3-sankey/blob/master/src/sankey.js) | Node size uses the larger of incoming and outgoing totals; endpoint offsets stack link widths; the DAG algorithm rejects cycles | Separate validation, topology, layout and scene geometry; stack bands instead of overlapping all of them at the node center |
| [RAWGraphs alluvial tutorial](https://rawgraphs.io/learning/how-to-make-an-alluvial-diagram) | Alluvial diagrams express category transitions across dimensions/stages | Persist stage identity; distinguish stage/category totals from full cohort-path identity |
| [Plotly Sankey guide](https://plotly.com/python/sankey-diagram/) | Names, values, custom metadata, manual/snap positions, alignment and ordering are separate controls | Keep metadata editing familiar and make auto-layout an explicit undoable action, not an unexpected drag reset |
| [D3 circular Sankey](https://github.com/tomshanley/d3-sankey-circular) | Cycles require a separate routing strategy | Route feedback outside the forward flow area; never feed cycles into an unbounded topological traversal |

These are research references, not promises of compatibility with those
products' private formats or dependencies to embed. Native Rust calculations
must work in the desktop app and Wasm without a web-only chart renderer.
Do not copy third-party example datasets/artwork into the application without
an appropriate license.

## 3. Mathematical contract

### Shared flow mode

- Each directed link has one weight in `[0, 100]`, stored as integer
  thousandths of a percentage point (`0..=100_000`). Caps and editable totals
  use integer arithmetic. Reject excess precision with guidance rather than
  silently rounding stored data; `33.3 + 33.3 + 33.4` must equal 100.
- For each node, incoming and outgoing totals are independently in `[0, 100]`.
  An edit that exceeds either endpoint's total is rejected atomically with
  the endpoint and allowed remainder identified.
- Incoming need not equal outgoing. Show the difference explicitly rather than
  pretending conservation or assigning the gap to another link.
- An unset weight is not zero or an implicit equal split. Distinguish an
  ordinary unweighted connector from a weighted link.
- Unset connectors do not consume weighted capacity. Remaining capacity for an
  edit excludes that link's previous value and checks the proposed value at
  both endpoints. Errors name the endpoint that prevents the edit.
- A zero-weight link remains editable in the Data list but contributes no
  visible band. It must not acquire a misleading minimum visible flow width.
- Link width is `weight * diagram_scale`, using the same scale at both ends
  and for disconnected components. Zoom transforms the scene, not its data.
- These are normalized weights, not unrestricted money/energy units. If a
  template describes energy or budgets, label its normalized basis in notes;
  do not display fictional percentages as measured currency or kilowatt-hours.
- Sources and sinks may have only one populated side. A disconnected node and
  an all-zero graph remain valid, with finite, nondegenerate layout bounds.

### Conditional probability mode

- Mode is per weighted sheet graph, never mixed per edge. Each edge stores
  its absolute flow in shared mode or its conditional percentage in probability
  mode. There is still exactly one authoritative value per link.
- Source probability mass is explicit on indegree-zero nodes only; total
  source mass is at most 100. Adding a parent to an explicit source is rejected
  until the user deliberately removes its source allocation. Never silently
  discard that allocation or inject it into a derived node.
  Nodes represent mutually exclusive flows within the modeled experiment,
  not arbitrarily overlapping events.
- A child's editable conditional percentage belongs to its outgoing link:
  `flow(source -> target) = source_mass * conditional / 100`.
  For example, 50% of a 20% branch is a 10% overall band.
- Incoming contributions add at a merge. Incoming rows display overall mass,
  not the sum of unrelated conditionals. Both lists show the shared link and
  its derived overall value; editing a conditional from an incoming row must
  clearly name the upstream source and its basis.
- Non-source node mass is derived and cannot be overwritten directly. Source
  mass, outgoing conditionals and graph topology are the authoritative data.
- Outgoing conditionals total at most 100 per node. A smaller total means
  explicitly unallocated probability, not silent renormalization.
- Zero incoming mass cannot be divided to derive an invented conditional.
  Explain why a derived value cannot be edited or converted.
- Topological evaluation is bounded. Probability cycles are rejected with an
  actionable explanation; circular diagrams use explicit flow weights,
  not an implicit stationary-probability solver.
- A DAG with nonnegative source masses totaling at most 100 and per-node
  outgoing conditionals totaling at most 100 cannot produce a node mass over
  100. Validate these invariants and recompute affected descendants before
  committing any upstream edit, atomically. Derived masses retain computation
  precision; round only for display, not at every step of propagation.
- Mode changes require an explicit atomic conversion. Shared -> probability
  needs an acyclic graph, source masses derived from source outgoing totals
  summing to at most 100, and no internal node with outgoing greater than
  incoming. Divide outgoing flows by available node mass; a lower outgoing
  total represents unallocated probability. Zero-mass nodes with zero flows
  require no invented conditional. Probability -> shared uses derived overall
  flow. Reject conversions not exactly representable at stored precision with
  an explanation; never silently round or reinterpret existing values.

### Alluvial and circular variants

Alluvial nodes have explicit stages and stable IDs. Require adjacent,
forward-stage links in alluvial mode; reject skipped or backward stages with
guidance to insert an intermediate node or use a different mode. Long links
remain valid in ordinary Sankey mode. Adjacent-stage links describe aggregate
transitions. Preserving an individual cohort across three
or more stages requires path/cohort identity beyond pairwise edge totals:
do not claim cohort-level trajectories from aggregates alone. Such detail
must either be modeled and tested or documented as a remaining limitation in
both the UI and documentation. Initial alluvial support is aggregate-only.

Circular explicit-flow diagrams permit feedback links without implying that
the sum of repeatedly traversed material is unique source probability.
Feedback routing, self-loops and parallel links require finite geometry,
dedicated lanes, bounds and hit testing. Do not silently drop cycles.

## 4. UX and integration

### Creation and navigation

Add a **Sankey** category to the existing starter-template gallery. Keep the
left shape toolbox and Format-panel Stencils tab unchanged. A template opens
an ordinary editable Thinoda sheet with existing node title, notes and links.
The starting selection should explain what can be edited in the Data tab.

Keep existing mind-map and draw.io documents unchanged unless the user
explicitly enables weighted flow behavior. Ordinary thick draw.io connectors
are not evidence of numeric weights and must not be auto-converted.

### Data tab: Weights

For one selected topic, show **Weights** with **Parents (incoming)** and
**Children (outgoing)** sections. Each row identifies the connected node,
shows the editable shared weight or clearly labeled conditional, and provides
navigation/removal and appropriate link customization. Editing from either
side updates the same link. Reuse normal color controls rather than adding an
inconsistent color editor.

Make each section's graph actions directional and discoverable: use **Add
parent** and **Add child**, and identify removal and weight controls by the
selected relationship. A user must be able to create, assign/edit, unassign,
and remove a relationship from the inspector without first switching to a
canvas connector tool.

Show totals, remaining capacity and imbalance next to the relevant section.
Provide searchable target selection to add a parent/child connection. Reject
invalid edits with an explanation and retain a correctable draft; never
silently clamp, normalize or overwrite other links.

In probability mode label **conditional %** and **overall %** separately.
Explain incomplete allocation and mode changes inline. Display disabled
controls with a reason for locked nodes/links or ambiguous selections.

Use explicit numeric labels and a visible scale legend when quantitative
coloring is enabled. Color is supplementary, never the only weight indicator.
Respect keyboard focus, touch-sized controls, light/dark themes, localization
and short/narrow Format panels. Multi-selection must not accidentally apply
one node's links to another.

### Geometry and editing

Smooth ribbons attach in non-overlapping stacks on node sides. Support long
links and split/merge junctions like the reference. Geometry must be shared
by canvas painting, pointer hit tests, bounds, selection, thumbnails and
supported visual exports. A broad band is not selectable only at its center.
Maintain a minimum node label/hit box independent of quantitative node height,
and a minimum pointer hit target independent of visible band width. Neither
interaction accommodation may inflate the depicted flow. Zero-weight links
are reached through the Data list, not painted as nonzero bands.

Auto-layout is deterministic and undoable, with stage spacing and ordering
controls. Preserve intentional manual moves until the user requests layout.
Avoid labels disappearing because a flow is too small: labels and node cards
may exceed quantitative node height, with the distinction made clear.

Reuse existing document limits and add explicit graph-work limits where
necessary. Use bounded iterative graph traversal, finite checked coordinates,
and bounded crossing-reduction iterations. Cache a derived graph/scene for an
unchanged document revision rather than rebuilding it per link per frame.
Reject graphs beyond supported limits with an explanation; never truncate
nodes/flows or freeze the UI while trying to arrange an unbounded import.

### Persistence, clipboard and draw.io

One graph-owned edge record owns each link; endpoint lists are projections.
Explicit weighted metadata opts a graph in, not ordinary connector width.
Version weighted semantics through native storage, recovery, clipboard and
private XMind envelopes. Old documents retain their previous defaults.
Copy/paste remaps node and link IDs together and includes only links whose
endpoints are both selected; external links are not reattached by stale ID.
Settings and stage metadata accompany copied graph data. Validate the complete
destination graph before a paste/import; cap violations reject the operation
with a bounded explanation, not clamping or partial insertion.

Deleting an endpoint removes incident graph links in the same undoable
transaction. Locking either endpoint or the link itself prevents weight edits
and removal. Existing document deletion policies must not leave a dangling
weighted edge. Undo restores the original links, values and IDs atomically.
Invalid imported graphs produce bounded errors, not silently repaired
quantities.

For draw.io, retain normal editable cells/connectors and disclose any
approximation. A Thinoda extension can retain semantic weights/stages/settings,
but other editors may strip it; rendered width alone cannot recover weights.
Native round-trip tests and explicit export-loss preflight are required.
Do not label generic external Sankey SVGs as semantically editable.

## 5. Original starter templates

All examples are fictional, sum-checked and explained in their notes.

| Template | Variant | Purpose and sample flow |
|---|---|---|
| Budget allocation | Shared Sankey | Income 100 -> Essentials 55, Savings 25, Leisure 20; Essentials -> Housing 35, Food 20 |
| Energy distribution | Shared Sankey | Input 100 -> Useful 65, Loss 35; Useful -> Heat 40, Motion 25; names/notes explain normalized units |
| Learning pathways | Staged alluvial | Start: Exploring 60, Focused 40; Middle: Guided 50, Independent 50; transitions 35/25 and 15/25; final stages retain separate category IDs |
| Delivery scenarios | Probability flow | Start 100 -> On time 80, Delayed 20; conditional splits 90/10 and 50/50 -> overall Successful 82, Needs follow-up 18 |
| Recycling loop | Circular shared flow | Feed -> Use 60; Use -> Recycle 20 and Exit 60; Recycle -> Use 20, so Use has 80 in/80 out while unique source input remains 60 |
| Blank weighted flow | Shared Sankey | Minimal editable starter with guidance, not an imported image |

Template values illustrate the feature rather than empirical predictions.
The staged template must not imply that aggregate edges retain full cohort
paths. The circular example is balanced but is not a probability experiment:
recirculated flow is not additional unique source input.

## 6. Phases and acceptance gates

| Phase | Deliverable | Gate | Status |
|---|---|---|---|
| S0: Research and contract | Sources, shared-weight decision, variants, invariants, examples and phased plan | Written Rubber Duck resolves semantic contradictions | Complete; review below |
| S1: Versioned graph model | Stable topic/link IDs, bounded weights, settings, validation, graph commands, deletion/undo | Boundary/invalid input and graph invariant tests; all persistence gates | Implemented locally; independent review pending |
| S2: Shared geometry and layout | Linear widths, stacked ports, crossing-aware layout, manual editing, bounds/hit tests and visual exports | Numeric width-ratio tests, finite bounds, zero/small/long links, split/merge and no regressions | Adjacent-layer weighted barycenter ordering is implemented locally and removes a deliberate crossing in regression; long-edge crossings, routing and rotation acceptance remain |
| S3: Data-tab editing | Parents/Children rows, totals, directional add/remove/navigation, editable and clearable weights, colors and diagnostics | Edits from both sides agree; no partial mutation; keyboard/short viewport coverage | Directional actions and endpoint-qualified weight fields implemented locally; focused UI tests pass; narrow-panel acceptance remains |
| S4: Probability and variants | Explicit sources/conditionals, propagation/merges, alluvial stages, circular flow routing | 20% * 50% = 10%; no fabricated cohort identity; bounded cycles | Implemented locally; independent review pending |
| S5: Starters and interoperability | Sankey gallery, original templates, native/clipboard/recovery and draw.io semantic/loss handling | Template totals; reopen/copy/export/import; unrelated formats unchanged | Five starters implemented locally; independent review pending |
| S6: Integrated acceptance | Documentation, independent Rubber Duck, targeted native/Wasm/browser checks, theme/interaction verification | Findings fixed; feature matrix updated honestly; real-device limitations explicit | Review findings fixed; targeted UI, canvas and lint/build checks pass; full-workspace integration, narrow-panel and device acceptance remain unverified |

Do not mark S1-S5 complete just because a pure graph calculation or screenshot
exists. Full support requires both editable UX and preservation across the
document operations users already rely on. Keep unfinished subfeatures open.

## 7. Required verification

- Totals at 0 and 100; zero versus unset; fractional sums; NaN/infinity,
  negative/out-of-range input and precision overflow; both endpoint caps;
  `33.3 + 33.3 + 33.4`, `0.1 + 0.2`, repeated thirds and exact-100 boundaries.
- Split, merge, duplicate names, parallel links, isolated/all-zero graphs,
  disconnected components, feedback/self-loops and malformed/dangling IDs.
- Shared parent/child editing; corrected invalid drafts; lock state; remove,
  delete, undo/redo; selection changes and current inspector-draft guards.
- Global linear widths and independent endpoint stacks; quantitative coloring
  precedence; finite bounds/hits; manual position preservation; stable layout.
- Probability sources, merges, zero mass, incomplete allocation, multi-source
  cap and mode conversion. No recursion on cycles or graph-depth stack overflow.
- Native save/reopen, supported clipboard remapping and old-reader gates,
  crash recovery, private XMind envelope and draw.io conversion diagnostics.
- Every starter opens, edits, saves and renders in native/Wasm; browser tests
  for Data editing and exported band geometry, including regression against
  an unchanged ordinary mind map/draw.io board.
- Build, Clippy, targeted tests, written critique, light/dark and narrow-panel
  interaction checks before declaring implementation complete. Physical touch,
  assistive-technology and external-viewer acceptance remain explicit gates
  until actually exercised.

## 8. Rubber Duck and implementation checkpoints

### S0 plan review (2026-11-11)

An independent Rubber Duck identified three blocking design gaps and five
additional issues. The plan was corrected before the implementation contract
was finalized:

1. **Circular conservation:** replaced the initial imbalance example with
   Feed 60 -> Use, Use -> Recycle 20 and Exit 60, Recycle -> Use 20. Use is
   80 in/80 out; recirculation is not unique source mass.
2. **Probability authority and sources:** made mode sheet-scoped, conditional
   versus absolute storage explicit, source mass root-only, descendant mass
   read-only, cap invariants clear, and upstream edits atomic with recomputation.
3. **Numeric correctness:** chose integer thousandths for stored weights/caps,
   display-only rounding of derived probabilities, precision rejection and
   fractional-boundary tests.
4. **Stages/cohorts:** alluvial links are adjacent-forward only; long Sankey
   links remain separate, and aggregate-only cohort scope must appear in UI.
5. **Ownership:** one edge record, internal-selection-only clipboard edges,
   destination validation, atomic endpoint deletion and either-endpoint locks.
6. **Capacity:** unset links consume none; remaining capacity excludes the old
   edited value; validation names either blocking endpoint.
7. **Tiny nodes/flows:** separate label/hit targets from quantitative dimensions,
   never drawing artificial positive flow for zero.
8. **Mode conversion:** require explicit representable conversion or an
   actionable rejection; no silent reinterpretation or normalization.

The implementation must still demonstrate these invariants. S0 completion
does not close S1-S6. Record exact validation, final implementation critique
and any remaining limitations here before changing their status.

### Initial implementation checkpoint (2026-11-11)

The initial worktree implementation adds shared-flow, alluvial, probability
and circular settings, a Sankey starter category, Data-tab controls and
weighted scene geometry. It reuses topic metadata and adds native v25, sheet
clipboard v21, board clipboard v18, recovery and private XMind gates. A
validated draw.io extension retains semantics, with explicit appearance and
external-editor preservation warnings. It is not yet a release acceptance
claim; independent implementation Rubber Duck is underway.

Implementation validation reported with `THINODA_AI_SKIP_LIVE=1`:

| Validation | Result |
|---|---|
| `rtk proxy cargo fmt --all --check` and `rtk proxy git diff --check` | Passed |
| `rtk proxy cargo test --locked -p thinoda-core -p thinoda-io --test sankey -- --test-threads=1` | 21 core and 8 IO tests passed |
| `rtk proxy cargo test --locked -p thinoda-app --lib sankey -- --test-threads=1` | 6 passed |
| `rtk cargo test --locked --workspace --lib -- --test-threads=1` | 2,426 passed; 4 ignored |
| `rtk proxy cargo test --locked -p thinoda-app --test data_tab -- --test-threads=1` | 4 passed |
| `rtk cargo clippy --locked --workspace --all-targets -- -D warnings` | Passed |
| `rtk cargo build --locked --workspace` | Passed |
| `rtk cargo clippy --locked --target wasm32-unknown-unknown -p thinoda-web -p thinoda-app -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings` | Passed |
| `rtk proxy trunk build --release --locked` from `crates\thinoda-web` | Passed |
| `rtk proxy npm run test:browser -- sankey.browser.spec.ts startup.browser.spec.ts drawio-link.browser.spec.ts board-export.browser.spec.ts` against the production build | 29 passed; temporary preview stopped |

The complete native integration sweep is now complete: `cargo nextest run
--workspace --no-fail-fast` passed all 3,475 tests; 15 were skipped and 16 were
reported as slow. No test failures were reported.

Initial implementation critique and open acceptance:

- Six starters are present: Blank Sankey, Budget flow, Energy flow, Learning
  stages, Delivery scenarios and Recycling loop. Core regression coverage
  verifies all six instantiate with explanatory notes.
- Probability mode conversion currently preserves edge absolute mass, not all
  source allocation/unallocated supply semantics. For example, probability
  source 100 with one 50% branch can become shared flow 50 and then source
  50 with one 100% branch. Conversion is an atomic dropdown action with Undo,
  not a preview/confirmation. Review must resolve the potential undisclosed
  loss before declaring the conversion contract complete.
- Alluvial stages are edited per node (Assigned/Stage) and summarised in a
  Data-tab Stages overview listing each stage column's nodes (click to
  select) with an undoable Compact stage numbers action. Adding a parent or
  child, or promoting a connector, gives an unstaged endpoint the adjacent
  stage automatically. Adjacent-stage links are enforced; stage names,
  reordering whole columns, full cohort tracing and automatic
  intermediate-stage insertion are not implemented.
- Quantitative coloring now has a non-selectable legend on the map canvas and
  in SVG-based visual exports and editable presentation slides. The scale
  overrides individual link colors while enabled; palette/stops customization
  is not implemented.
- Routing remains deterministic and basic. Four forward/backward
  weighted-barycenter sweep pairs reduce crossings between adjacent ranks,
  weighted by actual flow mass (derived overall mass in probability mode) and
  using the existing vertical order to break ties. Zero-mass links do not
  affect ordering; unset links count equally. Links that skip a rank are not
  included in the heuristic, which does not guarantee a global minimum.
  Ribbons have axis-aligned ports; rotated cards and dense charts require
  follow-up visual/interaction acceptance.
- Generic connector waypoints and markers do not control Sankey ribbon
  geometry. These differences must remain explicit rather than implying
  arbitrary draw.io connectors render as identical weighted bands.
- Draw.io appearance remains approximate, and other editors may strip the
  semantic extension.
- Auto-arrangement supports floating cards, not restructuring the mind-map
  ownership tree. Graph links still support multiple sources/targets.
- If committing an active title/label draft changes the snapshot, the flow
  edit requests a retry instead of overwriting newer metadata.
- Removing the final probability supply records an explicit zero source,
  never an implicit 100% allocation.
- Accessible names and weight edits have harness coverage, but endpoint-
  qualified row action names and complete keyboard-only construction/removal
  browser acceptance remain open. Dedicated partial-selection cut/failure
  coverage also remains open.
- Label backdrops have not established complete painted bounds for tight
  crops or label-only lasso intersections. A full Sankey-specific visual
  PNG/PDF/PPTX matrix has not been exercised.
- Physical touch, assistive-technology and external-viewer acceptance have
  not been established by the automated suites.

### Independent implementation Rubber Duck (2026-11-11)

The independent review found no defect in integer caps or acyclic probability
propagation, and confirmed the balanced circular example and 82/18 probability
result. It also found material integration defects, all since repaired;
the successful initial suites do not override these findings.

| Finding | Required correction | Status |
|---|---|---|
| Stale draw.io extension rejects the entire otherwise-readable file | Validate semantic extension before application; import ordinary cells with an explicit semantics-loss warning when externally edited references are stale | Fixed; re-review verified |
| Newly added unset links are invisible, like explicit zero | Draw an ordinary dashed/selectable no-weight connector for unset; keep true zero band-less and Data-editable | Fixed; re-review verified |
| Cap errors omit endpoint/remainder, rejected drafts disappear and dragging fragments Undo | Name direction/endpoint/capacity, retain correctable draft, and group one editing gesture into one undo operation | Fixed; re-review verified |
| Templates omit explanatory notes and blank starter; alluvial UI omits cohort limitation | Add original notes, the sixth starter, and the aggregate-transition explanation in the UI | Fixed; re-review verified |
| Mode switching can silently lose source allocation | Explicitly disclose/confirm lossy source/unallocated conversion or reject it; test both retained mass and semantic loss | Fixed; re-review verified |
| Probability cut/copy paths lack direct regression | Test cut/paste, source-cap rejection, unchanged failed destination and retained clipboard/Undo recovery | Fixed; re-review verified |
| Ordinary-board reconciliation and inspector cloning add unnecessary work | Early-out without Sankey, hoist repeated settings/lock work, avoid full plain-sheet clone per frame | Fixed; re-review verified |
| Empty canvas bypasses source/stage lock checks | Perform node lock validation outside the canvas-object loop | Fixed; re-review verified |
| Rotated nodes can detach bands | Correct rotated ports or explicitly block unsupported rotation without resetting existing data | Fixed; re-review verified |
| Painted labels exceed geometry bounds/hits | Share/conservatively include label bounds in selection and crop geometry; test a narrow flow with a long label | Fixed; re-review verified |
| Draw.io stroke widths differ from effective band scale | Reuse shared effective scale or disclose exact approximation; never infer semantic weight from ordinary width | Fixed; re-review verified |
| Color-scale precedence and missing standalone legend | State precedence in the inspector; show a non-selectable legend on canvas and in visual exports | Fixed locally; canvas, SVG and editable-slide regressions cover enabled/disabled states |
| Fresh probability sources and invalid alluvial conversions have generic errors | Explicit zero-source initialization and endpoint/stage-specific guidance | Fixed; re-review verified |
| Feedback lanes start at zero instead of content bounds | Route relative to actual graph bounds | Fixed; re-review verified |
| Parent probability rows do not lead to editable upstream conditional | Offer clearly labeled upstream edit/navigation | Fixed; re-review verified |
| Waypoint/marker controls appear to edit ignored ribbon properties | Disable/hide unsupported controls with explanation | Fixed; re-review verified |

Zero-weight links intentionally do not paint a positive-width flow; their Data
rows remain editable. This differs from unset connectors, whose missing value
must be visually apparent. A failed paste after an earlier cut must not
partially change the destination, discard clipboard data or prevent Undo of
the cut; it does not automatically reverse an already committed source edit.

Full crossing minimization, real-device validation and a complete visual export
matrix remain separate acceptance work, not blockers invented by the review.

### Repair re-review and residuals (2026-11-11)

A narrow re-review of every finding against current code and tests reported
all 16 corrections fixed (color precedence initially "mostly fixed"). It raised
three residuals, now repaired:

- Rotated or collapsed-hidden weighted endpoints: native load rejects with
  localized guidance, paste rejects atomically, and draw.io import falls back
  to ordinary editable cells with a warning while preserving rotation. No load
  path silently renders an empty ribbon scene. Regressions cover each path.
- Cap errors no longer claim an unknown "allowed remainder"; transactional
  link edits still report the accurate link-specific remainder.
- With the color scale enabled, unset and zero-mass links use the scale's zero
  color consistently across canvas, export and draw.io; explicit zero remains
  band-less.

Validation after repairs: Sankey core 30, IO 13, app 13 and scene 53 tests;
workspace library tests 2,432 passed / 4 ignored (before the residual pass);
full workspace nextest 3,475 passed / 15 skipped; native and Wasm Clippy with
`-D warnings`; native build; formatting and diff checks; production Trunk
bundle; Chromium Sankey/startup/draw.io/export specs. The standalone legend
was subsequently validated by focused canvas, SVG and editable-PPTX tests.

Deliberate limitations: rotation, collapsed-ancestor hiding and manual
routing/markers are unsupported for weighted flows (rejected with explanation);
lossy mode conversions reject rather than offering a preview wizard; palette
and stop customization is not implemented; draw.io strokes are approximate
without the native extension; layout has no crossing minimization; screen-reader,
touch/pen, high-DPI and external-viewer QA remain unvalidated.

### User-directed inspector continuation (2026-11-12)

The Parents/Children rows already supported adding and removing links and
editing an assigned shared weight, but their generic action labels did not make
the graph direction discoverable. The continuation now uses explicit Add
parent / Add child and relationship-specific remove actions, identifies each
weight input with both its endpoint and field name, and has UI regressions for
both directions, edits, removal and Undo. Full-workspace, narrow-panel and
real-device acceptance remain separate gates.

### Narrow inspector interaction checkpoint (2026-11-12)

A 280 px-wide headless inspector regression now constructs a flow by adding a
node and child relationship, assigns and edits its weight, then removes the
relationship. This verifies the actions remain reachable in a constrained
layout; it does not exercise the complete native Format panel, physical
devices, touch, or assistive technology. Those acceptance gates remain open.

### Crossing-aware layout checkpoint (2026-11-12)

The explicit Arrange flow cards action now builds adjacent-rank layers and
performs four deterministic forward/backward weighted-barycenter sweep pairs.
It starts from the nodes' existing vertical order, so equal barycenters remain
stable; barycenters use overall flow mass (including derived overall
probability), unset links contribute equally, and explicit zero-mass links do
not affect order. A six-node split/merge regression confirms a crossed target
pair is reordered without reversing already aligned source or middle nodes.
This is a local heuristic, not a global crossing optimum; routing/rotation,
dense-diagram and real-device acceptance remain open.

### Rank-skipping layout checkpoint (2026-11-12)

Forward links that skip ranks now take part in the sweeps through virtual
waypoints in each intermediate layer, initially ordered by the y position
interpolated between the link's endpoints. Waypoints influence ordering only;
they take no vertical space in the final placement. Backward and same-rank
(circular feedback) links stay excluded. Links out of root sources, which have
no stored rank, were previously dropped from the sweeps by mistake; they now
use rank 0 like placement does. To keep cost bounded, at most 20,000 waypoints
are created and one position map is updated per reordered layer; longer links
beyond the budget fall back to exclusion. Regressions cover a tie only the
skipped link can break and a 600-node chain with 600 long shortcuts.

### Alluvial stage management checkpoint (2026-11-12)

Every document edit validates adjacent-stage links, so committed alluvial
diagrams never hold unstaged flow endpoints. The gap was therefore the add
path: linking to an unstaged topic was rejected. Adding a parent/child or
promoting a connector now derives the missing stage from its neighbour (child
= parent + 1, parent = child − 1, solving each connected component as a whole
so topic order never matters; unanchored components start at 0) and rejects
cycles, conflicting, below-zero or locked results atomically. A Stages overview lists
columns with their nodes, and Compact stage numbers renumbers used stages
from 0 while preserving adjacency. Core regressions cover derivation,
conflicts, locks and compaction; a headless inspector test covers adding an
unstaged child, compaction and Undo. Column reordering remains open.

### Alluvial stage names checkpoint (2026-11-12)

Each used stage can now have an optional name in the Data > Weights Stages
overview. Blank names fall back to the stage number; topic choices show the
custom stage name when present. Names are keyed by stage number and persist in
the document. The new settings field defaults when loading older documents,
and compaction remaps names together with the stages. Core regressions cover
legacy settings loading, serialization and compaction; the inspector regression
covers editing, compacting and Undo. Reordering stage columns remains open.

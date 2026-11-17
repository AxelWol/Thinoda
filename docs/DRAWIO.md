_Last updated: 2026-11-17 09:13:00 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# draw.io (diagrams.net) native integration: research, gap analysis, concept and plan

Status: 🟡 in progress (D0 complete). Binding contracts that shipped code already
follows (user decisions, ownership/passthrough, JSON stencil DSL, model, UX,
security, guardrails, licensing) are in
[DRAWIO-ARCHITECTURE.md](DRAWIO-ARCHITECTURE.md). This document replaces the
previous 15-phase plan and its progress log, retired on 2026-11-17; git history
keeps them. Tracked in [PLAN](PLAN.md#drawio-diagram-support-2026-11-05) and
[BACKLOG](BACKLOG.md#p1--drawio-diagram-support-user-request-2026-11-05).

## 1. Research (draw.io desktop 32.6.10)

Sources: the installed desktop app (`app.asar`, `drawio/VERSION` = 32.6.10),
the decoded `stencils.min.js`, `app.min.js`, `shapes-14-6-5.min.js`, the
template folder and the example corpus under `docs/draw-io-examples` with its
screenshots. Counts come from regex extraction over minified code and are
accurate to a few entries.

| Area | draw.io 32.6.10 |
|---|---|
| Actions / menus / key bindings | 295 actions, 48 menus, 77 bindings (~110 shortcut-legend entries) |
| Format panel | Diagram, Diagram style (42 colour schemes), Style, Text, Arrange; floating inline toolbar |
| JS shape handlers | 834: 16 mxGraph core, 101 drawio core, 717 library shapes (mockup 100, iOS 63, aws3d 57, electrical 54, archimate3 51, sysml 44, archimate4 36, basic 33, lean_mapping 29, floorplan 28, ...) |
| XML stencils | 9,090 in 204 files, 50 families (aws4 1,050, veeam 580, electrical 529, rack 487, cisco_safe 485, pid 478, ...) |
| Sidebar | 479 palette functions, ~11,570 entries, 63-65 "More shapes" libraries in 5 groups; defaults general, uml, er, bpmn, flowchart, basic, arrows2; tag-index search; scratchpad; custom `.mxlibrary` libraries |
| Edges | 10 edge-style functions, 9 routing choices, 8 edge shapes, 17 perimeters, 34 markers, line jumps (arc/gap/sharp/line), flow animation |
| Structure | Pages, page links, layers, groups, containers, swimlanes/pools, tables (merge, stripes), stack/tree/flow/table child layouts, collapse |
| Layout | ELK flow/tree/radial/organic, org chart, circle, parallels, libavoid orthogonal routing, custom JSON layouts, outline editing |
| Insert | Shapes, text, note, link, image, polygon, freehand, table, layout container, template (155 in 15 categories, CC BY 4.0), Mermaid, PlantUML, CSV, SQL, text, Edit Diagram (raw XML), Edit Shape (stencil XML) |
| Data | Edit Data custom properties, ids, tooltips, links (URL, page, custom actions), placeholders `%name%`, tags, style rules, data sources |
| Appearance | Light/dark/auto/high contrast, adaptive colours, sketch (rough), math (MathJax), fonts incl. Google Fonts, shadow, glass, gradients, fill patterns |
| Views | Outline, layers, tags, find/replace, text outline, ruler, units, page view, grid/guides, presentation and explore modes |
| Files | `.drawio`, compressed XML, `.drawio.svg/.png/.html`, PDF with embedded diagram, VSDX/VSD/VSSX, Gliffy, Lucid import; PNG/JPEG/WebP/GIF/SVG/PDF/VSDX/HTML/XML/URL export |
| Cloud/collaboration | Google Drive, OneDrive, GitHub, GitLab, Trello, Confluence, revisions, comments, shared cursors, AI generation, 18 plugins |

Stencil licensing: draw.io is Apache-2.0. `stencils/LICENSE` forbids using the
icon sets and stencils, or derivatives, as assets in Atlassian products or the
Atlassian marketplace; end-user diagram output is exempt. Templates are CC BY 4.0.
Vendor icon families additionally carry vendor trademarks.

Example corpus usage (391 distinct shapes): 230 `mxgraph.*` names on 1,573 cells
come from XML stencils; 101 `mxgraph.*` names on 1,298 cells are JS-coded
(top: `mockup.forms.rrect` 123, `floorplan.wall` 94, `mockup.containers.rrect` 85,
`arrows2.arrow` 79, `networks2.icon` 49, `bootstrap.rrect` 48,
`basic.partConcEllipse` 44); edge styles `orthogonalEdgeStyle` 608, `segment` 20,
`elbow` 22, `entityRelation` 8, `isometric` 4; 226 distinct style keys.

Translator coverage (`crates/thinoda-io/examples/drawio_stencil_coverage.rs` over
all 9,090 XML stencils): 203 of 204 files convert; aws4 (1,050 shapes) fails as
a whole on the package node limit. Of the 8,040 shapes in converted files, 8,035
evaluate (99.94 %) after the D0 fixes; the remainder is listed in D1. This proves
validity, not visual parity (D15). Converting 20 generic families yields 2,560
shapes, 5.6 MB of JSON, 0.81 MB deflated; D2 bundles the 17 without platform
trademarks.

## 2. Gap analysis

Status: **Native** = modelled and editable; **Approx.** = imported/rendered
approximately with a warning; **Preserved** = kept in a passthrough envelope
(none yet: the envelope is planned in D6); **Missing** = lost or rejected. "Plan" names the phase that closes the gap.

| Area | draw.io feature | Thinoda today | Plan |
|---|---|---|---|
| Files | `.drawio`/`.xml`, compressed, multi-page | Native import to board sheets, export, clipboard | — |
| Files | `.drawio.svg/.png/.html` | Native import and export | — |
| Files | Save back in place to `.drawio` | Missing (Save As/export only) | D10 |
| Files | PDF with embedded diagram | Missing | D13 |
| Files | VSDX/VSD/VSSX, Gliffy, Lucid import | Missing | D13 |
| Files | VSDX, JPEG/WebP/GIF export | Missing (SVG/PNG/PDF exist) | D13 |
| Pages | Pages, rename, reorder, duplicate | Native (sheets) | — |
| Pages | Page links `data:page/id` | Approx. (plain link) | D7 |
| Layers | Named layers, visibility, lock, move to layer | Approx. (flattened; Below/Above bands only) | D8 |
| Shapes | Core 16 + drawio 101 JS shapes | 31 native `ShapeKind`s + 32 JSON definitions; others approximate as rectangles | D5 |
| Shapes | 9,090 XML stencils | 17 generic families bundled (81 packages, 2,238 shapes); vendor and platform families not yet available | D4 |
| Shapes | 717 JS library shapes | Missing (rectangle fallback) | D5 |
| Shapes | Edit Shape (stencil XML), inline `stencil(...)` | Native import of inline stencils; no editor | D12 |
| Libraries | Sidebar sections, search, More Shapes | Library browser with built-in sections, a "draw.io" group of the 17 bundled families (opt-in under More shapes, searchable), imported packages, scratchpad, `.mxlibrary` import; no vendor libraries | D4 |
| Libraries | Vendor libraries (AWS, Azure, GCP, Cisco, ...) | Missing | D4 |
| Connectors | Straight, orthogonal, elbow, curved | Native (Straight, Elbow, Curve) | — |
| Connectors | entityRelation, segment, isometric, sideToSide, topToBottom, loop, libavoid | Approx. (elbow) | D7 |
| Connectors | 34 markers | 12 native; others approximate | D7 |
| Connectors | Line jumps | Missing | D7 |
| Connectors | Edge shapes flexArrow, link, pipe, filledEdge, taperedArrow, wire | Approx. (line) | D7 |
| Connectors | Fixed ports, waypoints, floating ends | Native | — |
| Connectors | Multiple labels per edge | Approx. (merged) | D7 |
| Style | Fill, stroke, width, opacity, rounding | Native | — |
| Style | Dash on shapes, gradient, shadow, glass, fill patterns, flip | Approx. (lost on export) | D9 |
| Text | Font family/size/style, alignment, label position, spacing, wrap | Approx. (cards native; shape labels partial) | D9 |
| Text | Rich HTML labels, sub/superscript, lists | Approx. (plain text) | D9 |
| Data | Custom properties, tooltips, links, placeholders, tags | Approx. (lost on export) | D7 |
| Structure | Groups, containers, swimlanes, stack layouts, collapse | Native (scene hierarchy SC0-SC3) | — |
| Structure | Tables (rows, columns, merge, stripes) | Approx. (container tables SC4 roadmap) | D8 |
| Layout | ELK flow/tree/radial/organic, circle, org chart | Missing for boards (mind-map layout is native) | D11 |
| Insert | Mermaid, PlantUML, CSV, SQL, from text | Missing | D12 |
| Insert | Templates (155) | Thinoda templates only | D12 |
| Editing | Edit Style (raw), Edit Diagram (raw XML), Edit Geometry | Missing | D12 |
| Editing | Copy/paste style, set default style, style rules | Partial (format painter) | D9 |
| Views | Find/replace on board, tags, outline window | Partial (mind-map find; minimap) | D12 |
| Views | Ruler, units, page view, print layout | Missing | D12 |
| Appearance | Dark mode, adaptive colours | Native app theme; adaptive diagram colours approximate | D14 |
| Appearance | Sketch style, math typesetting | Missing | D14 |
| Effects | Flow animation, animation dialog | Missing | D14 |
| Cloud/collab | Cloud storage, comments, cursors, AI, plugins | Out of scope (Thinoda has its own storage and AI) | — |

## 3. Concept: native, not a clone

1. **One board model.** draw.io content becomes ordinary board objects,
   connectors and sheets. Anything not modelled yet is approximated and disclosed
   by import diagnostics; from D6 it also survives in the passthrough envelope
   (architecture §3.1.1).
2. **Shapes are data.** Every new shape, stencil family or JS-coded draw.io shape
   is a versioned JSON definition executed by the shared bounded interpreter
   (architecture §3.1.2). No per-shape Rust variants or renderer branches.
3. **Generic families are bundled, compressed and lazy.** Generated packages for
   generic families ship as deflated JSON, decoded only when a library section
   opens or an imported cell needs one. Objects drawn by bundled family packages
   embed one closure per package and page, so native documents stay portable and
   bundled packages can be regenerated without changing old native files.
4. **Vendor and platform families are opt-in.** AWS, Azure, GCP, Cisco, Android,
   iOS, Material and other trademarked
   families are converted locally from a user-supplied draw.io stencil file or a
   user-triggered download pinned by SHA-256; they are stored as user libraries.
5. **Thinoda UX first.** draw.io features appear through existing surfaces: the
   library browser, contextual top strips, the inspector, the context menu and
   command palette. No draw.io menu bar, Format panel or sidebar clone.
6. **Version gating.** Every new persisted field raises the native format
   version and clipboard version, has a default that preserves older files, and
   is covered by persistence, clipboard and export tests.

## 4. Phased plan and progress record

Each phase ends with fmt, Clippy (native and wasm32, `-D warnings`), targeted
tests, a written critique in this section and a commit staging only owned files.
A task is ticked only when implemented, tested and committed.

### D0 — Research, gap analysis and plan ✅

- [x] Extract and inventory draw.io 32.6.10 (actions, menus, format panel, shapes, stencils, palettes, templates, licenses)
- [x] Scan the example corpus for shape, edge-style and style-key usage
- [x] Add the stencil coverage probe (`drawio_stencil_coverage` example)
- [x] Fix translator background semantics: state ops and extra background geometry run as a foreground prelude in draw.io order
- [x] Fix duplicate stencil names producing duplicate definition ids
- [x] Retire the old plan; move binding contracts to DRAWIO-ARCHITECTURE.md; write this gap analysis and plan
- [x] Rubber-duck review of this plan (findings and resolutions in §5)

### D1 — Translator completeness and fidelity evidence ✅

- [x] Follow `mxStencil.drawNode` semantics: shapes inside `<path>` replace it, state setters inside paths, stray path commands, unknown elements ignored like draw.io (e.g. `fillstrokecolor`), `fillalpha`/`strokealpha` as global alpha, rounded paths disclosed
- [x] Coverage reports failed files separately from failed shapes (aws4's 1,050 shapes exceed the package node limit; D4 splits oversized vendor libraries)
- [x] Coverage gate test: one synthetic shape per element draw.io's renderer knows (geometry, path commands, paint state, text, include, connections)

Visual parity against draw.io reference renders is tracked in D15.

### D2 — Bundled generic stencil families ✅

- [x] License audit per family; bundle only families drawn by jgraph without third-party marks: basic, arrows, flowchart, bpmn, electrical, pid, floorplan, mockup, cabinets, eip, fluid_power, lean_mapping, sitemap, signs, networks, networks2, bootstrap. Android, iOS and Material (gmdl) mimic platform trademarks and move to D4
- [x] Ship `stencils/drawio/NOTICE` with the Apache-2.0 notice, the upstream stencil restriction text and modification provenance; reconcile architecture §3.8
- [x] Deterministic generator: pinned upstream version and per-file SHA-256, sorted files, source-order duplicate suffixes, compact serialization, fixed deflate level, translator revision; package limits checked per family
- [x] Lightweight catalog (aliases `mxgraph.<ns>.<name>`, titles, definition ids, package file) so lookup and search never decode packages; packages carry no import map
- [x] Lazy, cached decode API in thinoda-io; release wasm size budget ≤ 1 MB added by the bundle
- [x] draw.io import resolves catalog aliases and embeds one closure per bundled package per page (never one resource per definition); explicit Thinoda references to bundled packages resolve on re-import
- [x] Tests: catalog integrity, each package validates, decode counts stay lazy, a page with > 128 distinct bundled shapes plus images imports fully, corpus cells resolve, export keeps `shape=mxgraph...`

Interchange contract: like draw.io itself, `.drawio` files reference bundled shapes by name. Native documents embed the definitions they use and never change; a `.drawio` file re-imported after a bundle regeneration draws the regenerated definition. The bundle revision is part of the package and regeneration notes are recorded here.

Regeneration notes:

| Bundle revision | Translator revision | Upstream | Contents |
|---|---|---|---|
| 1 | 1 | draw.io 32.6.10 | 17 families, 81 packages, 2,238 shapes, about 854 KB (catalog and compressed packages) |

Regenerate with `cargo run --release -p thinoda-io --example drawio_stencil_bundle -- <dir>`,
where `<dir>` holds the upstream `src/main/webapp/stencils` XML. The output is
byte-identical for the same inputs. Change the bundle revision whenever the
translator or the family list changes.

Precedence when draw.io import resolves a shape: native exact kinds first; for
approximate native kinds (for example rounded `process`, open-bracket
annotations) only built-in and inline stencils, so the object stays an
editable native kind; otherwise a bundled alias, but only from an explicit
`shape=` key, because draw.io reads a bare first style token as a named style.

### D3 — Library browser for bundled families ✅

- [x] "draw.io" group in the library browser with one collapsible section per family (package captions in multi-package families); a package is decoded only when one of its tiles becomes visible
- [x] Search across catalog titles, shape names and package titles without decoding packages; hidden families are searched too, and bundled results are capped at 240 with a "Refine the search" hint
- [x] Visibility per family persisted next to the built-in visibility settings (`shown_drawio`, opt-in, under "More shapes…")
- [x] Inserted objects embed their whole bundled package as one content-addressed resource, so every later insertion from that package reuses it

### Interleaved user requests ✅

- [x] Template previews on the start page (Software group): thumbnail labels scale with their shapes instead of keeping the 7 px interactive minimum
- [x] Re-enabled connector arrows: the legacy arrowhead now scales with the line width (`legacy_arrow_size`), on canvas and in export, so a toggled-off-and-on arrow is no longer hidden under a thick stroke
- [x] Attribute ribbon audit: arrow buttons, route, colors, width and popups checked against the format sidebar and the style clipboard; no further defects found
- [x] Shape and connector options for multi-selections of one kind: every change applies to all selected objects as one undo step; the heading shows the count, arrows read as on only when all connectors show them, "Edit label" stays single-object, and a locked or grouped member disables the ribbon

### D4 — Vendor and platform libraries

- [ ] Import a draw.io stencil XML file (any family, including vendor families) as a user library through the existing library import, with its aliases
- [ ] Resolver interface so draw.io import also resolves installed user libraries; placeholders upgrade on reopen
- [ ] Split stencil files that exceed package limits (aws4) into several packages
- [ ] Optional user-triggered download of a pinned family (desktop) with SHA-256 verification and display of the applicable terms

### D5 — JS-coded draw.io shapes as JSON definitions

- [ ] Corpus-ranked definitions: mockup forms/containers, floorplan wall/doors/windows, arrows2, basic, bpmn task/event/gateway, infographic, lean_mapping, c4 person, logic gates
- [ ] Parameters bound to draw.io style keys via `drawio.import` rules

### D6 — Ownership and passthrough envelope

- [ ] Source ids, unknown style keys and attributes kept per object and sheet (architecture §3.1.1), with copy/delete semantics and diagnostics
- [ ] Round-trip tests for unknown data through native save, clipboard and draw.io export

### D7 — Connectors, links and data

- [ ] Markers: ER family, async, box, cross, circle, circlePlus, doubleBlock, manyOptional, SysML
- [ ] Line jumps (arc, gap, sharp, line)
- [ ] Routes entityRelation, segment, isometric, sideToSide, topToBottom, loop as editable routes
- [ ] Edge shapes flexArrow, link, pipe, filledEdge, taperedArrow, wire
- [ ] Multiple connector labels
- [ ] Object links (URL, page), tooltips, custom properties, placeholders and tags, editable in the inspector

### D8 — Layers and tables

- [ ] Named layers per sheet with visibility and lock, layers panel, move to layer
- [ ] Tables: rows, columns, merge, stripes on the container-table engine

### D9 — Style and text

- [ ] Dash, gradient, shadow, glass, fill patterns, flip on shapes
- [ ] Shape label font family/size/style, alignment, position, spacing, wrap
- [ ] Rich text labels (bold/italic/underline runs, sub/superscript, lists)
- [ ] Copy/paste style, set default style, style rules

### D10 — Save back to `.drawio`

- [ ] Source-preserving in-place save with loss preflight (architecture §3.5); requires D6

### D11 — Automatic layouts

- [ ] Tree, flow, radial, circle and organic layouts for selected board objects

### D12 — Insert tools and editors

- [ ] Mermaid, PlantUML (text subset), CSV, SQL and text import to board objects
- [ ] draw.io templates in the template gallery (CC BY 4.0 attribution)
- [ ] Edit style (raw key/value), edit geometry, raw diagram XML import/export dialog
- [ ] Find/replace and tags on boards, ruler, units and page view
- [ ] Stencil editor for JSON definitions; draw.io stencil XML is converted on import, not edited

### D13 — Other formats

- [ ] Embedded diagram in PDF import; VSDX, VSSX, Gliffy and Lucid import (VSD binary out of scope); VSDX, JPEG, WebP and GIF export

### D14 — Appearance and effects

- [ ] Adaptive diagram colours, sketch style, math in labels, flow animation

### D15 — Visual parity and hardening

- [ ] Reference renders from draw.io for representative shapes of every bundled family, compared within tolerance
- [ ] Fuzz and size budgets for the bundle decoder and catalog

Out of scope: cloud storage connectors, comments, shared cursors, AI generation
(Thinoda has its own AI), plugins, revision history (Thinoda has its own
recovery) and binary `.vsd`.

### Progress record

| Date | Phase | Result |
|---|---|---|
| 2026-11-17 | Requests | Software template previews readable; re-toggled arrows scale with width (d34d271); shape and connector options edit multi-selections with one undo |
| 2026-11-17 | D3 | Library browser lists the 17 bundled families (2,238 shapes) in an opt-in "draw.io" group; catalog search; one shared resource per inserted package; 11 new family names translated into 8 languages |
| 2026-11-17 | D2 | 17 families bundled (81 packages, 2,238 shapes, about 854 KB); 253 cells in the example corpus now draw their stencils; approximate native kinds stay native; NOTICE and Apache text shipped |
| 2026-11-17 | D1 | 8,040 of 8,040 shapes in converted files evaluate; aws4 (1,050 shapes) still fails as a file and moves to D4 |
| 2026-11-17 | D0 | Research, gap analysis, plan, coverage probe, translator fixes (99.94 % of shapes in converted files), rubber-duck review folded in |

### Critiques

**D0.** The inventory is regex-based over minified code, so counts are
approximate; the gap table is limited to user-visible features and treats cloud
and collaboration features as out of scope. The coverage probe proves that
definitions validate and evaluate, not that they look like draw.io; visual
parity still needs reference renders. Phases D6-D15 are large and each will be
split into committed slices when started.

**D1.** Matching draw.io's renderer instead of refusing unknown elements is a
deliberate contract change: draw.io itself ignores them, so refusing made
Thinoda strictly worse. Hoisting state setters out of paths is exact for
draw.io's SVG canvas, which applies state at paint time. Mapping
`fillalpha`/`strokealpha` to global alpha reproduces a draw.io quirk on 991
shapes rather than the element names' intent; parity wins because users compare
against draw.io. Rounded paths (none in the 32.6.10 stencils) are only
disclosed. 100 % evaluation is not visual parity; D15 still owns reference
renders.

**D2.** Keeping approximate native kinds over bundled stencils trades fidelity
for editability: a rounded draw.io process stays a Thinoda process that users
can restyle, instead of a frozen stencil. Bare style tokens are deliberately not
resolved, matching draw.io, so such a cell keeps its fallback just as it
draws as a plain rectangle in draw.io. Closures are built per package and per page from visible
cells only and inserted when the first object uses them; hidden layers
therefore never leave unused resources that would block draw.io export. Each
page embeds one closure per package holding only the definitions it uses (plus
their includes), which bounds resources to one per package and keeps re-import
stable. `.drawio` interchange stays name-based, so a regenerated bundle can
change how an exported file looks after re-import; native documents are
unaffected. 34 corpus names in bundled namespaces still fall back: they are
JavaScript-coded shapes (mockup containers, `bpmn.event`, `basic.rect`, ...)
owned by D5. Package validity and evaluation are proven; visual parity is not
and remains D15.

**D3.** Families are opt-in so the default panel stays as small as before;
search still covers hidden families, so a shape can be found without knowing
its family. Insertion embeds the whole bundled package rather than a closure:
every shape inserted from that package shares one resource (one per package,
never one per shape), at the cost of up to about 1 MB per package in memory
(networks2 is the largest; on disk the native zip compresses it). Imported
`.drawio` pages keep their smaller per-page closures, so the same package can
appear as two resources in a sheet that mixes both paths. Package, shape and
section-caption titles stay English, as in the catalog; only family names are
localized. The preview cache does not count the shared package bytes per tile,
so large packages do not evict other previews; the panel keeps each decoded
package and its encoded resource for the session (about 5 MB at most).

**Interleaved requests.** Arrows: the root cause was not the toggle itself but
two arrowhead sizes, a fixed canvas size and a width-scaled export size.
Turning an arrow off clears its draw.io marker, so it comes back as the legacy
arrowhead, and the core validation (no marker without its arrow flag) forbids
keeping the marker. Unifying the size fixes both thick and default connectors;
draw.io itself restores its default classic marker in this case. Multi-selection:
only objects of one kind are edited together (shapes with shapes, connectors
with connectors). A mixed selection still has no ribbon, because the common
attributes of a shape and a connector differ in meaning (fill vs. connector
color). The controls show the primary (first selected) object's values rather
than a "mixed" state, so a color swatch can mislead until a value is picked.

## 5. Rubber-duck findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | High | The passthrough envelope does not exist, so "Preserved" overstated support | Statuses corrected to Approx.; new D6 (ownership and passthrough) precedes save-back D10 |
| 2 | High | `add_canvas_object` embeds one closure resource per distinct definition; sheets allow 128 resources shared with images and failed inserts drop objects | D2/D3 embed one closure per bundled package per page; test with > 128 shapes plus images |
| 3 | High | `.drawio` export references shapes by name, so re-import after regeneration may differ | Contract stated in D2: name-based like draw.io; native documents embed definitions |
| 4 | Medium | User and vendor libraries were not visible to draw.io import | D4 resolver interface |
| 5 | Medium | Lazy loading needs metadata that does not require decoding; `include_bytes` still grows wasm | D2 catalog, decode-count tests, ≤ 1 MB wasm budget; D15 fuzz and budgets |
| 6 | Medium | Generation must be reproducible | D2 deterministic generator with pinned version and SHA-256 |
| 7 | High | Licensing and trademarks must gate bundling | D2 license audit and NOTICE; Android/iOS/Material moved to opt-in; architecture §3.8 reconciled |
| 8 | Medium | Coverage denominator hid the failed aws4 file; no visual evidence | §1 reports files and shapes separately; D1 report split; D15 reference renders |
| 9 | Medium | Some gap rows had no task | Every row now has a task or is listed as out of scope (VSD binary, cloud, plugins) |

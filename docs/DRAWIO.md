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
approximately with a warning; **Preserved** = kept in the passthrough envelope
(D6: unknown style keys, attributes and ids of board objects); **Missing** = lost or rejected. "Plan" names the phase that closes the gap.

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
| Shapes | Core 16 + drawio 101 JS shapes | 31 native `ShapeKind`s + 66 JSON definitions; others approximate as rectangles | D5 ✅ |
| Shapes | 9,090 XML stencils | 17 generic families bundled (81 packages, 2,238 shapes); vendor and platform families not yet available | D4 |
| Shapes | 717 JS library shapes | 35 frequent corpus shapes defined (floorplan, mockup, BPMN, C4, lean mapping); icon-table shapes and the rest fall back to rectangles | D5 (partial) |
| Shapes | Edit Shape (stencil XML), inline `stencil(...)` | Native import of inline stencils; no editor | D12 |
| Libraries | Sidebar sections, search, More Shapes | Library browser with built-in sections, a "draw.io" group of the 17 bundled families (opt-in under More shapes, searchable), imported packages, scratchpad, `.mxlibrary` import; no vendor libraries | D4 |
| Libraries | Vendor libraries (AWS, Azure, GCP, Cisco, ...) | Missing | D4 |
| Connectors | Straight, orthogonal, elbow, curved | Native (Straight, Elbow, Curve) | — |
| Connectors | elbow (horizontal/vertical), entityRelation, sideToSide, topToBottom | Native (draw.io routers, first-waypoint hint) | D7 |
| Connectors | segment, isometric, self-loop | Native (draw.io routers; loops automatic, custom loop direction/size warn) | D7 |
| Connectors | libavoid and other routers | Approx. (elbow, warns) | D7 |
| Connectors | 34 markers | 30 native; sysMLReqInt, sysMLProvInt and mermaid markers approximate | D7 |
| Connectors | Line jumps | Native (arc, gap, sharp, line, size); `noJump` and `jumpLayers` warn | D7 |
| Connectors | Edge shapes flexArrow, link, pipe, filledEdge, taperedArrow, wire | Native (D7) | D7 |
| Connectors | Fixed ports, waypoints, floating ends | Native | — |
| Connectors | Multiple labels per edge | Native (D7) | D7 |
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

- [x] Import a draw.io stencil XML file (any family, including vendor families) as a user library through the existing library import, with its aliases
- [x] Resolver interface so draw.io import also resolves installed user libraries; placeholders upgrade on reopen
- [x] Split stencil files that exceed package limits (aws4) into several packages
- [ ] Optional user-triggered download of a pinned family (desktop) with SHA-256 verification and display of the applicable terms — deferred (see the D4 critique)

### D5 — JS-coded draw.io shapes as JSON definitions ✅

- [x] Corpus-ranked definitions (34 new, 35 draw.io names): floorplan wall, wall corner, window, doors (left, right, double) and stairs; mockup margin rects, buttons, scroll bar, checkbox, curly brace and browser window; bpmn event, shape, gateway2 and task/task2 (outlines, symbols, markers, loops); c4 person and person2; stylised arrow, shaded pyramid, circular callout, bar callout; lean_mapping inventory, push and pull arrows, data box, supermarket, outside sources, manufacturing process (plain and shared), FIFO lane
- [x] Parameters bound to draw.io style keys via `drawio.import` rules, including choice keys (`outline`, `symbol`, `gwType`, `taskMarker`, `buttonStyle`, ...) with draw.io's absent defaults; unknown choice values fall back to the absent default
- [ ] networks2 icons, aws4 resource icons and electrical logic gates — deferred (see the D5 critique)

### D6 — Ownership and passthrough envelope ✅

- [x] Source ids, unknown style keys and attributes kept per object and sheet (architecture §3.1.1), with copy/delete semantics and diagnostics
- [x] Round-trip tests for unknown data through native save, clipboard and draw.io export

### D7 — Connectors, links and data

- [x] Markers: ER family, async, box, cross, circle, circlePlus, doubleBlock, manyOptional, SysML
- [x] Line jumps (arc, gap, sharp, line)
- [x] Routes elbowEdgeStyle (horizontal and vertical), entityRelation, sideToSide, topToBottom as editable routes
- [x] Routes segment, isometric, loop as editable routes
- [x] Edge shapes flexArrow, link, pipe, filledEdge, taperedArrow, wire
- [x] Multiple connector labels
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
| 2026-11-17 | D7 connector labels | Relative edge-label children import as positioned connector labels and export as `edgeLabel` children; the main label keeps its x/y/offset; canvas, export, hits, search and replace see every label; inspector "Connector labels" adds, positions and removes them; native v43, board clipboard v36, sheet clipboard v32, new XMind and recovery envelopes |
| 2026-11-17 | D7 edge shapes | flexArrow, taperedArrow, link, pipe, filledEdge and wire bodies port mxArrowConnector and Shapes.js; canvas, export, hits, bounds and lasso share them; inspector "Line shape" with widths and fill, style copy; native v42, board clipboard v35, sheet clipboard v31, new XMind and recovery envelopes |
| 2026-11-17 | D7 segment, isometric, loop | Segmented (every waypoint fixes the next segment) and horizontal/vertical isometric routes port draw.io's SegmentConnector and IsometricConnector; floating self-loops route like draw.io's Loop on every route; route menu shows eleven routes; native v41, board clipboard v34, sheet clipboard v30, new XMind and recovery envelopes |
| 2026-11-17 | D7 edge routes | Horizontal and vertical elbow, side-to-side, top-to-bottom and entity-relation routes follow draw.io's routers and orthogonal rectangle perimeter; first waypoint is the routing hint; route menu and inspector show all eight routes; native v40, board clipboard v33, sheet clipboard v29, new XMind and recovery envelopes |
| 2026-11-17 | D7 line jumps | Connectors jump earlier-painted straight and elbow connectors with draw.io's arc, gap, sharp and line geometry and size; inspector and style copy; native v39, board clipboard v32, sheet clipboard v28, new XMind and recovery envelopes |
| 2026-11-17 | D7 markers | 18 library markers (ER, box, cross, circle, circlePlus, async, openAsync, doubleBlock, SysML) are native, drawn from 1-3 parts; native v38, board clipboard v31, sheet clipboard v27, new XMind and recovery envelopes; menu translated |
| 2026-11-17 | D6 | Board objects keep draw.io source ids, unknown style keys and UserObject attributes; sheets keep diagram ids, model attributes and tombstones. Native v37, board clipboard v30; export never overrides edited keys or reuses deleted ids |
| 2026-11-17 | D5 | 34 JSON definitions cover 35 JS-coded draw.io shapes (floorplan, mockup, BPMN, C4, lean mapping, callouts); choice parameters bind draw.io keyword styles both ways; every option evaluates at three sizes |
| 2026-11-17 | D4 | draw.io stencil XML imports as `user.drawio.<ns>` packages with draw.io aliases; installed libraries resolve opened diagrams; aws4 (1,050 shapes) installs as 2 parts in 0.2 s; pinned download deferred |
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

**D4.** Vendor families stay opt-in and local: users import the stencil file
they obtained (draw.io's `stencils/<family>.xml`), so Thinoda ships no vendor
artwork and its terms stay with the user. The pinned download is deferred: it
needs a network client and a terms screen per vendor in the desktop build, and
importing the downloaded file already gives the same result. The resolver is a
process-wide registry rather than a parameter of every import call. This keeps
the many import entry points unchanged, but tests that install libraries must
run single-threaded (the app suite already does). Built-in and bundled names
win over user aliases, so an imported copy of a bundled family cannot change
how bundled shapes draw. Within user libraries, the earlier one wins. Converting a whole file
first and splitting afterwards holds the whole converted library in memory
once, but keeps include groups exact. A first draft measured only
`validate`, which does not bound bytes, so the stored parts (8.6 MB pretty
JSON) would have failed to reload. Parts are now sized and stored as compact
JSON, and a test reloads them. The bundle generator keeps its own alias code
because it also strips licenses and revisions; the two derive aliases the same
way.

**D5.** The definitions are written from draw.io's JavaScript shape code, not
traced from renders, so they follow its geometry but are not checked visually
(D15). Several are approximations: the BPMN task markers service, user,
manual, script and business rule are simplified icons, and some markers are
not drawn at all. The mockup browser window omits its address text and
navigation arrows. Lean process headers use a `headerHeight` parameter where
draw.io derives the height from the font size. Stairs are unrolled to 80
fixed step lines. An `include` cannot pass parameters, so the event drawing is
inlined into the gateway and task definitions rather than shared. Each
definition's own default (for example a visible circle for BPMN events) can
differ from draw.io's absent value (no outline); import uses draw.io's value,
so opened files draw as in draw.io while new shapes from the library are
visible. Outside label positions are left out of the export styles until D9
supports them, because they would otherwise warn on every insert. Deferred:
`networks2.icon` and `aws4.resourceIcon` draw one of hundreds of embedded icons
selected by a style key, and electrical `logic_gate` depends on operator and
input-count keys. Each needs a table-driven definition rather than a single
shape. These remain rectangle fallbacks with the usual warning. All new titles
are translated into the eight UI languages. Choice options show readable
labels (for example "Intermediate catching") while import and export keep
draw.io's keywords. Parameter labels and option names are English only.

**D6.** Ownership is a conservative superset: every key Thinoda reads or
writes for any object kind is owned and never passed through, so a few keys
that would be safe to keep are dropped. Keys that depend on an owned key
(`dashPattern` on `dashed`, `gradientDirection` on `gradientColor`, ...) are
dropped when the owner is missing or off, so a cleared gradient never comes
back. Only canvas objects carry passthrough; cards, topics and zones are
regenerated, so their ids and unknown keys are lost, as before. Tombstones
cover only cells that became objects, because topic and zone ids change on
every import. Export gives a source id priority over a generated `o{id}`
(except for Sankey connectors, whose extension addresses `o{id}`), so a
renumbered re-import is idempotent. Unmappable cells are still dropped with a
warning; storing them as opaque objects belongs to D10, as do workbook-level
`mxfile` metadata and cut-once provenance. Named style tokens are not kept.
Because almost every imported cell has an id, a file saved after a draw.io
import needs native v37, so older Thinoda versions cannot open it. A shape
`link` is preserved but still warns until D7 makes links editable.

**D7 markers.** Geometry follows draw.io's mxMarker, mxER, mxSysML and
Shapes.js sources at draw.io's `size + strokeWidth + 1` unit. Approximations:
the ER "zero" rings are always unfilled, whereas draw.io fills them with the
background white, so a ring on top of another shape shows the shape through it;
sysMLReqInt, sysMLProvInt and the mermaid markers still import as the classic
arrow with a formatting warning. A marker can now have up to three parts, so
`ConnectorMarkerGeometry` grew (its size bound rose from 64 to 160 bytes) and
`SceneGeometry` boxes it to stay within its 192-byte bound. Fill applies only to closed
shapes (`is_fillable`); cross, ER bars and SysML lines ignore it. The menu grew
to 31 entries and relies on its existing scroll area. Files that use a new
marker need native v38, so older builds reject them.

**D7 line jumps.** The geometry mirrors draw.io's `mxShape.addPoints`, including
its quirks: the jump half-width is `(size − 2) / 2 + strokeWidth`, a crossing
closer than that to the segment end or the previous skipped point is dropped,
and the bump direction depends on the rounded segment direction. Arcs are cubic
curves flattened into eight steps, so they are close but not pixel-identical.
Jumps are computed once per scene from the paint order (Below band, topics,
Above band); draw.io instead uses cell order within a layer and ignores
`jumpLayers`, so mixed-band diagrams can differ. Curves neither jump nor are
jumped, as in draw.io, and Sankey ribbons are excluded. `noJump` and
`jumpLayers` stay in the passthrough with a formatting warning. Hit testing,
lasso and single-object bounds use the plain stroke; only scene bounds include
the bumps. Crossing tests are capped at four million per scene, after which the
remaining connectors draw without jumps. The UI rounds sizes to whole pixels
because draw.io truncates `jumpSize`; fractional native values are kept until
edited. The fuzz seed that expected `endArrow=cross` to warn now uses an unknown
marker, since cross became native with the marker slice.

**D7 edge routes.** The routers port mxEdgeStyle's ElbowConnector,
SideToSide, TopToBottom and EntityRelation, including `Math.round` on the
midpoint, the dropping of route points inside a terminal and the
floating-terminal order (the target end aims at the computed source point when
no route point remains). Rectangles, processes and rounded rectangles without
a rounded perimeter use mxPerimeter.RectanglePerimeter's orthogonal branch
exactly, so a diagonal neighbour ends in a corner. Other outlines use an
orthogonal ray from the nearest centre line, which is close but not draw.io's
per-perimeter rule. Like draw.io, the routers use unrotated bounds, only the
first waypoint is a hint, and entity relations ignore waypoints. The extra
waypoints are kept and the inspector says so, but the canvas still draws
their handles. A hint route without waypoints offers one insert handle at the
path midpoint, and dragging it creates the hint. `elbow` is now an owned key,
so it no longer survives on edges without a routed style. `segment`,
`isometric` and `loop` styles still import as Thinoda's orthogonal elbow with
a formatting warning, as does an entity relation whose `segment` is not 30
(the key stays in the passthrough). Hints on `orthogonalEdgeStyle` edges still
warn, since Thinoda's elbow treats them as waypoints. Files that use a new
route need native v40.

**D7 segment, isometric and loop routes.** `segment_points` ports
SegmentConnector line by line: the first segment's orientation comes from the
hint's alignment with fixed terminal points or the terminal channels, every
waypoint then fixes the next segment, and bends inside floating terminals
are dropped. Like draw.io, the coordinate a segment does not use is ignored, so
the last hint's x is irrelevant when its segment is horizontal. Channel bounds
are assumed to be the terminal bounds (draw.io's `getChannelBounds` is the
same for plain vertices). Only zero-sized terminals (points) count as fixed,
because Thinoda's ports are not pinned through the route. Isometric routes
decompose each leg onto draw.io's -30°/-150° axes and clip their ends from
the centre, not orthogonally. Self-loops do not get a new route: as with
draw.io's `isLoopStyleEnabled`, a connector whose both ends are the same
object or topic, without ports and with fewer than two waypoints, uses the
Loop router's west direction and grid-size segment on every route (curves stay
smooth through the loop bends), and the first waypoint is the loop hint. Before
this slice self-loops drew as a dot at the shape centre. draw.io's loop
`direction` and `segment` are not modelled, so non-default values warn. The
route label is "Segmented" because "Segment" reads the same in several
locales. A segment route offers one insert handle until it has a waypoint;
further segments are added by dragging waypoints, not by draw.io's
per-segment handles. New routes need native v41.

**D7 edge shapes.** The body geometry lives in `scene/edge_shape.rs` and is
derived once, so the canvas, SVG/PDF export, hit testing, bounds and the lasso
agree. flexArrow and link port mxArrowConnector's side and arrow-head
construction, including link's second rail running end to start; tapered
arrows merge classic and block heads into the outline like draw.io and draw
other markers separately. The corpus (requirements-flow, home-lab and
sankey-diagrams) imports 18 edge shapes and round-trips them. Known
approximations:

- Markers on pipe, filledEdge and wire are stroked at the casing width, as in
  draw.io's pipe; other line-shape marker widths were not compared.
- Separate tapered-arrow markers use the stroke colour, and diamond and box
  heads are not merged into the outline.
- `rounded=1` does not round arrow-band joins, and curve routes trim markers
  along the polyline instead of the curve.
- Sankey connectors keep and export `shape` but paint their ribbon.
- A dashed wire is a dashed overpaint in draw.io; Thinoda has no dashes yet
  (D8), so the inner line is solid.
- Line jumps draw only for wire and filledEdge, like draw.io; the inspector
  says so instead of hiding the setting.
- The inspector edits one connector; multi-selection uses style copy.

**D7 connector labels.** Placement ports draw.io's getPoint over the routed
path, so labels follow route changes, and the label editor opens at the
positioned main label. The corpus seed with an edge-label child now keeps it
as a separate label instead of merging the text. Known approximations:

- Label bounds are estimates (12 px lines, 7.2 px per character); exports
  and hit testing share the estimate, but long labels may differ slightly
  from the painted text.
- An unpositioned main label still stays out of scene bounds, as before; the
  lasso ignores labels.
- Child labels inherit the connector's font, colour and background; children
  with their own font size, colour, alignment or style import with a
  Formatting warning, and absolute-positioned children are still merged into
  the main label.
- Extra labels are edited in the inspector, not on the canvas, and cannot be
  dragged yet. A label cannot be emptied there; Remove label deletes it.
- Sankey ribbons place their own flow label, so the section is hidden for
  them.

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

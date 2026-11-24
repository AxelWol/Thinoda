_Last updated: 2026-11-23 22:00:36 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# draw.io (diagrams.net) native integration: research, gap analysis, concept and plan

Status: 🟡 in progress. D0–D3, D6–D8 and D11 are complete; D4 and D5 have
deferred work, and D9 is partially complete. D10 and D12–D15 remain open. Binding
contracts that shipped code already follows (user decisions,
ownership/passthrough, JSON stencil DSL, model, UX, security, guardrails,
licensing) are in
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
| Files | JPEG export | Native visual raster export; opaque, transparent pixels composite over the sheet background, with no editable diagram data | — |
| Files | WebP/GIF export | Native visual export: lossless WebP; static GIF with palette/transparency preflight; neither embeds an editable diagram | — |
| Images | Inline PNG/JPEG, GIF, WebP, SVG | PNG/JPEG remain original; bounded static GIF/WebP and SVG convert to PNG with a resource warning; animated, malformed and external images are not recovered | D13 |
| Files | VSDX export | Missing | D13 |
| Pages | Pages, rename, reorder, duplicate | Native (sheets) | — |
| Pages | Page links `data:page/id` | Native (navigates to the imported page) | D7 |
| Layers | Named layers, visibility, lock, move to layer | Native (implicit base layer; topics, cards and frames stay on it; layers interleave with the map only through Below/Above bands) | D8 |
| Shapes | Core 16 + drawio 101 JS shapes | 31 native `ShapeKind`s + 66 JSON definitions; others approximate as rectangles | D5 (partial) |
| Shapes | 9,090 XML stencils | 17 generic families bundled (81 packages, 2,238 shapes); vendor and platform families not yet available | D4 |
| Shapes | 717 JS library shapes | 35 frequent corpus shapes defined (floorplan, mockup, BPMN, C4, lean mapping); icon-table shapes and the rest fall back to rectangles | D5 (partial) |
| Shapes | Edit Shape (stencil XML), inline `stencil(...)` | Native import of inline stencils; no editor | D12 |
| Libraries | Sidebar sections, search, More Shapes | Library browser with built-in sections, a "draw.io" group of the 17 bundled families (opt-in under More shapes, searchable), imported packages, scratchpad, `.mxlibrary` import; vendor families are not bundled | D4 |
| Libraries | Vendor libraries (AWS, Azure, GCP, Cisco, ...) | User-supplied stencil XML can be installed; optional pinned downloads are deferred | D4 |
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
| Style | Dash on shapes, gradient, shadow, glass, fill patterns, flip | Native (D9) for `dashed`/`dashPattern`/`fixDash`, `gradientColor`/`gradientDirection` (4 sides and radial), `shadow`, `glass`, `fillStyle` hatch/cross-hatch/dots/dashed/zigzag-line and `flipH`/`flipV` on built-in and stencil shapes; sketch-only fill styles approximate, shadow/hatch parameters stay passthrough | D9 |
| Text | Font family/size/style, alignment, label position, spacing, wrap | Native (D9) on shape labels for `fontFamily` (closest installed font on canvas/PNG/PDF, exact name in SVG/PPTX/draw.io), `fontSize`, `fontStyle` bold/italic/underline/strikethrough, `align`, `verticalAlign`, `labelPosition`/`verticalLabelPosition`, `spacing` and per-side spacing, `whiteSpace`, `labelBackgroundColor`, `labelBorderColor` and `horizontal=0`; `overflow`, `textDirection` and `labelWidth` stay passthrough and warn; connector labels and cards keep their own text options | D9 |
| Text | Rich HTML labels, sub/superscript, lists | Partial (D9): bounded editable runs and lists on shapes and connector labels, with selection and caret typography controls and inert draw.io HTML import/export; formatted sticky/text cards support run-level styling, arbitrary RGBA colors and render on canvas and SVG/PNG/JPEG/PDF and round-trip supported draw.io HTML; editable PowerPoint branch/zone slides emit each card title as one styled text box with native paragraphs and lists. One rotated fixture opens, edits and saves in Windows PowerPoint; visual parity and other Office hosts remain unverified. Zones and container-specific labels retain their existing models; unsupported markup warns and is not preserved | D9 |
| Data | Custom properties, tooltips, links, placeholders, tags | Native (D7) for links, tooltips, properties, tags and `%name%` placeholders (own data, then scene containers, `id`, `width`, `height`, `page`); dates, page numbers, file name and lengths stay literal | D7 |
| Structure | Groups, containers, swimlanes, stack layouts, collapse | Native (scene hierarchy SC0-SC3) | — |
| Structure | Tables (rows, columns, merge, stripes) | Native (scene table on the container engine; stripes per cell start slot, grid lines as cell sides) | D8 |
| Layout | ELK flow/tree/radial/organic, circle, org chart | Native selection layouts: tree, layered flow, radial, circle and organic (org chart = vertical tree); no edge routing or ELK options | D11 |
| Insert | Mermaid, PlantUML, CSV, SQL, from text | Missing | D12 |
| Insert | Templates (155) | Thinoda templates only | D12 |
| Editing | Edit Style (raw), Edit Diagram (raw XML), Edit Geometry | Partial (exact geometry in the nearby Arrange section; raw style and XML missing) | D12 |
| Editing | Copy/paste style, set default style, style rules | Partial (copy/paste and reusable quick styles; default styles and rules missing) | D9 |
| Views | Find/replace on board, tags, outline window | Partial (board find/replace, exact tag selection and Outliner; tag visibility and other draw.io views missing) | D12 |
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
   library browser, unified nearby selection menu, the inspector, the context menu and
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

### D5 — JS-coded draw.io shapes as JSON definitions

- [x] Corpus-ranked definitions (34 new, 35 draw.io names): floorplan wall, wall corner, window, doors (left, right, double) and stairs; mockup margin rects, buttons, scroll bar, checkbox, curly brace and browser window; bpmn event, shape, gateway2 and task/task2 (outlines, symbols, markers, loops); c4 person and person2; stylised arrow, shaded pyramid, circular callout, bar callout; lean_mapping inventory, push and pull arrows, data box, supermarket, outside sources, manufacturing process (plain and shared), FIFO lane
- [x] Parameters bound to draw.io style keys via `drawio.import` rules, including choice keys (`outline`, `symbol`, `gwType`, `taskMarker`, `buttonStyle`, ...) with draw.io's absent defaults; unknown choice values fall back to the absent default
- [x] Apply matching shape defaults to unknown edited choice values even when a Thinoda parameter override is embedded
- [x] Electrical `buffer2`: data-defined body and leads, optional negation bubble, endpoint constraints, and editable draw.io parameter round-trip
- [x] Electrical transmission `testPoint`: stroke-coloured circular head, aspect-dependent lead and endpoint constraints in a data-defined stencil
- [x] Electrical `dc_source_3_v2`: elliptical source with size-dependent polarity marks and endpoint constraints in a data-defined stencil
- [x] Inherit edited/imported stroke widths for these three electrical stencils using new definitions; retain the old definitions for existing documents
- [ ] networks2 icons, aws4 resource icons and electrical logic gates — deferred (see the D5 critique)

### D6 — Ownership and passthrough envelope

- [x] Retain source IDs and supported passthrough data for represented objects and pages, with copy/delete semantics and diagnostics
- [x] Round-trip tests for supported passthrough data through native save, clipboard and draw.io export
- [ ] Preserve unmappable cells as visible, selectable opaque objects and retain unsupported workbook/page XML under bounded ownership rules
- [ ] Preserve a source ID on a completed, same-document cut exactly once using the provenance contract in architecture §3.1.1

### D7 — Connectors, links and data

- [x] Markers: ER family, async, box, cross, circle, circlePlus, doubleBlock, manyOptional, SysML
- [x] Line jumps (arc, gap, sharp, line)
- [x] Routes elbowEdgeStyle (horizontal and vertical), entityRelation, sideToSide, topToBottom as editable routes
- [x] Routes segment, isometric, loop as editable routes
- [x] Edge shapes flexArrow, link, pipe, filledEdge, taperedArrow, wire
- [x] Multiple connector labels
- [x] Object links (URL, page), tooltips, custom properties, placeholders and tags, editable in the inspector
  - [x] Object data model (link, tags, properties, placeholder flag), gates, draw.io round trip, inspector "Data" section, hover tooltip
  - [x] Placeholder `%name%` resolution in canvas and exports (search matches the written text)
  - [x] Page links (`data:page/id,`) navigate to the matching sheet
- [ ] Named ports, remaining draw.io connection point sets and keyboard traversal between individual ports
  - [x] Cycle supported connection points for a focused connector endpoint with Home/End, including stencil shapes
  - [ ] Expose named ports and Tab traversal between individual ports
    - [ ] Carry stencil `RenderPort.name` through visible connection candidates and expose it to keyboard and assistive-technology users without losing names when clipping or occlusion filters points
    - [ ] Define and version stable named-port attachment semantics, including duplicate or missing names, legacy coordinate-only connectors, parameter changes, transformations and round trips; retain a visible fallback when a port cannot be resolved

**Named-port critique:** The interpreter retains optional names, but
`stencil_object::ports` currently returns positions only and connector attachments
store coordinates. Definition validation already rejects duplicate names within
one definition, but names are optional and that check does not establish a
nonempty, bounded, durable identity. Multiple ports can also resolve to the
same coordinates. Home/End currently finds the first matching coordinate, so
recovering a name from position alone can report the wrong port. Displaying a
name alone would not preserve its identity if parameterized geometry moves the
port; ambiguous positions, definition changes and incompatible readers need
explicit handling before named attachments can be considered complete.

### D8 — Layers and tables

- [x] Named layers per sheet with visibility and lock, layers panel, move to layer
- [x] Tables: rows, columns, merge, stripes on the container-table engine
- [x] Show at least six layers in a movable Layers dialog and remember its last position
- [x] Make Layers dialog repositioning keyboard-accessible

**Rubber-duck critique:** Constraining the panel to the drawing canvas left too
little room for six rows beneath the ribbon at the minimum native window size,
so it uses the app viewport instead and scrolls if even that viewport is too
small. Dragging the title or empty background moves the panel without stealing
the layer controls' interactions. The focusable title announces movement
instructions, shows a focus ring and uses arrow keys to move in 10-point steps;
Tab leaves it normally and the canvas selection does not move. Position is saved
after a completed drag or successful keyboard movement, validated before
restoration and clamped to the viewport. Tests cover six actionable rows at
multiple interface scales, pointer and keyboard movement, focus traversal,
viewport edges, reopen/restart, scrolling and smaller-window restoration.

### D9 — Style and text

Selection-menu consolidation is tracked in
[Nearby selection menu](CONTEXT-FORMATTING.md): the existing nearby node menu
also owns card, shape, connector and stencil formatting, with capability-based
Text/Style/Arrange/Actions sections instead of competing upper strips.

- [x] Consolidate node, card, shape, connector and stencil formatting into one nearby selection menu; remove duplicated upper floating toolbars
- [x] Expose only applicable Text/Style/Arrange/Actions controls and explicit selected-object, selected-text or new-text scope
- [x] Preserve multi-selection common typography and card fill with undo; retain editor focus, IME and numeric/picker draft ownership
- [x] Complete contextual-menu rubber-duck critique, compact rendered-control acceptance and label-outline regression checks (see linked design)
- [x] Keep the passive nearby menu clear of selected-object connection, extend, resize and rotation handles, including the right-hand quick-connect action
- [x] Dash, gradient, shadow, glass, fill patterns, flip on shapes
- [x] Shape label font family/size/style, alignment, position, spacing, wrap
- [ ] Rich text labels (bold/italic/underline runs, sub/superscript, lists)
  - [ ] Shape labels: bounded inline runs and lists, selection formatting, atomic undo, recovery, draw.io HTML subset, canvas and visual export implemented; ordered editing remains unsafe for ambiguous input
  - [ ] Primary connector labels: the same model, including Sankey labels and draw.io HTML subset; ordered editing remains unsafe for ambiguous input
  - [ ] Extra connector labels: the same model, including positioned Sankey labels and inspector/draft reconciliation; ordered editing remains unsafe for ambiguous input
  - [x] Typing styles at the caret for shape and connector labels
  - [x] Font size and palette colour controls for selected text and caret typing styles
  - [x] Searchable inline font-family selection and draft preview for selected text and caret typing styles
  - [x] Direct numeric font size and arbitrary RGBA hex colour entry for selected text and caret typing styles
  - [x] Preserve existing formatted-card runs and lists during search/replace
  - [x] Render formatted sticky/text-card titles on canvas and reserve layout space for larger runs and list markers
  - [x] Import/export supported draw.io card HTML, including baseline overrides
  - [x] Warn when draw.io HTML relative or non-400/700 numeric font weights are approximated, protecting the original from silent formatting loss
  - [x] Warn when draw.io HTML oblique text is approximated as italic
  - [x] Warn when draw.io HTML font-family fallback lists cannot be represented as a single native font family
  - [x] Preserve ancestor underline and strikethrough across nested draw.io HTML decorations
  - [x] Warn when a nested colour change can alter the colour of an ancestor's decoration
  - [x] Render formatted card titles in SVG, PNG, JPEG and PDF
  - [x] Export styled card runs to editable PowerPoint line shapes
  - [x] Emit formatted card titles as single editable PowerPoint text boxes with native paragraphs and list properties
  - [x] Preserve subsequent initial PowerPoint numbering after empty numbered paragraphs
  - [x] Position editable PowerPoint list paragraphs as aligned blocks using shared rich-layout geometry
  - [x] Add an atomic, validated and undoable card-title/run mutation for a future rich editor
  - [x] Add explicit-style label range replacement with list remapping for future ordered edits
  - [x] Add bounded transactional rich-draft state with directional selection, local history and explicit composition grouping
  - [x] Use the actual buffer range for a single eligible shape/connector text or paste insertion, including repeated text and selected replacement
  - [x] Use the actual buffer range for a single eligible shape/connector Backspace or Delete, including repeated text and selected deletion
  - [x] Preserve remaining rich-label formatting when an eligible cut deletes selected text
  - [x] Preserve rich runs and list paragraphs for an eligible Shift+Enter line break at the actual selection
  - [x] Process pointer-free, non-IME rich-label keyboard batches in event order
  - [x] Add bounded draft-local keyboard undo/redo for shape and connector text, formatting and synthetic IME composition
  - [x] Use actual buffer deletion ranges for an isolated IME surrounding-delete event
  - [x] Use actual buffer ranges for isolated IME preedit/commit replacements
  - [x] Process pointer-free, IME-only batches in event order
  - [x] Process pointer-free mixed IME/keyboard batches in event order, protecting candidate keys and routing F6 focus
  - [x] Cover synthetic editor and outside clicks batched with rich IME input
  - [x] Delete whole grapheme clusters with unmodified Backspace/Delete in label editors
  - [x] Preserve graphemes for eligible text/paste replacement and cut; snap an interior insertion caret to the cluster end
  - [x] Preserve graphemes for Shift+Enter and the start of an eligible IME composition
  - [x] Expand partial grapheme selections for eligible Ctrl, Alt and Command Backspace/Delete
  - [x] Snap interior carets for eligible Ctrl, Alt and Command Backspace/Delete
  - [x] Leave grapheme selections unchanged for ignored empty text/paste and newline text events
  - [x] Handle eligible AccessKit selected-text replacement with tracked rich ranges and grapheme boundaries
  - [x] Handle eligible AccessKit whole-value replacement as a tracked rich edit
  - [x] Order eligible AccessKit whole-value edits with toolbar focus transitions
  - [x] Cover an AccessKit whole-value request before and after an outside click
  - [x] Apply eligible formatted card-title input before an outside canvas click, retaining subsequent input and rejected stale drafts
  - [x] Order eligible text, IME and title-targeted accessibility replacement around a single editor press; preserve ordered Tab transitions to new topics
  - [x] Retain a dirty formatted card-title draft when its source disappears; allow cancellation or restoration and reject recovery against the missing source
  - [x] Toggle bold, italic and underline on a selected range in an existing formatted card title or a freshly opened plain card title, with local undo/redo
  - [x] Add selection toolbar controls for strikethrough, subscript, superscript, bullets and numbering on card titles, with grapheme-safe ranges and local undo
  - [x] Apply card-title inline formatting at the caret for subsequent typing, including toolbar and keyboard shortcuts, ordered IME input, cursor changes and accessibility whole-value replacement
  - [x] Add card-title inline font family, numeric size and palette color controls for selections and caret typing styles; show mixed and inherited values
  - [x] Add searchable card-title font selection
  - [x] Add arbitrary RGBA color entry for card titles
  - [x] Order eligible text, IME and toolbar-number input around F6, Enter or Space toolbar activation in the same frame, for shape, primary and extra connector labels
  - [ ] Replace the remaining final-string reconciliation in existing shape and connector label editors with ordered rich edits, including pointer/IME batches; validate history with real platform input
    - [ ] Preserve event order across multiple editor clicks and text events in one frame; the ignored `two_editor_clicks_keep_batched_text_at_each_clicked_caret` regression currently produces `ABxy` instead of `xABy`
    - [ ] Design ordered input replay with per-event pointer state and text layout at each edit; the current single-press pass removes keyboard events while egui sees frame-aggregated pointer state, so splitting the event list alone cannot reproduce earlier clicks
    - [ ] Evaluate replay from a retained prior-frame egui `InputState`, including first-frame and multi-pass behavior, event consumption and restoring the host input after editor processing
    - [ ] Preserve double/triple-click selection, Shift extension, drag and touch gestures, IME composition, accessibility actions and toolbar/outside-click ownership across replay; validate undo and mixed formatting on shape and connector labels before enabling the regression
  - [ ] Complete rich-card run editing, validate PowerPoint paragraph/list rendering in Office, cover other label kinds and preserve unsupported source HTML
  - [x] Copy/paste object and card styles; save and apply reusable quick styles
- [ ] Default style and declarative style rules

**Oblique critique:** Oblique and italic can use different font faces and angles.
The native inline model records only italic, so imports retain the existing
approximation but report formatting loss for confirmation. Exact oblique
rendering and source-preserving HTML remain open.

**Decoration critique:** An HTML ancestor's underline or strike propagates
through descendants even when they specify `text-decoration:none`. Preserve
those active decorations while keeping same-element overrides, independent
decoration kinds, sibling restoration and Thinoda's inheritance markers intact.
An ancestor's decoration can have a different colour from descendant text;
the model paints both in the descendant colour, so such changes are disclosed
conservatively. Independent decoration colours and source-preserving HTML
remain open.

**Font-weight critique:** CSS `bolder` and `lighter` depend on the inherited
numeric weight, and numeric values other than 400 and 700 can differ visibly;
the current bold/not-bold model cannot retain either distinction. The existing
visual approximation is preserved, but import and clipboard conversion now
report formatting loss instead of presenting it as exact.

**Ordered-input critique:** egui 0.37 exposes `InputState::begin_pass` but not
`PointerState::begin_pass`. Rebuilding successive input states from a retained
prior-frame snapshot is a candidate for replay, not an established fix: using
the already aggregated current state as the starting point would reuse later
click and drag history. Replay must not consume input twice on extra egui
passes, invent event timing or change the host's final pointer and focus state.

**Font-family critique:** CSS fallback lists are not native single-family names;
retaining the written list as one family may fall back to the default font.
Conversion warns rather than claiming fidelity. A single quoted family may
contain a comma and does not trigger this warning.

### D10 — Save back to `.drawio`

- [x] Preserve bounded, safe unknown `<mxfile>` attributes through native workbook storage and draw.io export (native v53); namespace-qualified attributes that cannot be preserved are disclosed
- [x] Include validated workbook-level `<mxfile>` attributes in editable SVG/PNG exports while embedding only the selected sheet
- [x] Disclose document attributes omitted for safety or budget limits while retaining other valid attributes
- [x] Protect document passthrough in versioned XMind snapshots and recovery, including pending ink; reject reserved generated attributes
- [x] Bound retained page passthrough before cloning, disclose dropped unknown model attributes and invalid page IDs, and reject reserved or oversized model attributes on export
- [x] Report ignored document, diagram and graph-level XML before opening a conversion-only copy; keep the original file protected
- [x] Report ignored cell and geometry attributes, children, duplicate geometry parts and unsupported flags rather than silently discarding them
- [x] Import parentless vertices and edges instead of silently treating them as structural roots; warn about missing cell IDs and nonstandard roots
- [x] Warn when structural root or layer data, IDs, styles or normalized names cannot be retained
- [x] Warn when container links, tooltips or enabled placeholder metadata cannot be retained, while preserving supported card and object links
- [x] Disclose group labels, collapsed state and unsupported group styles that cannot be represented
- [x] Disclose explicit zone styles that are not represented by Thinoda frames
- [x] Disclose the default draw.io swimlane header size when conversion changes it
- [x] Disclose default draw.io swimlane foldability when conversion removes it
- [x] Preserve supported links, rich text and colors in Sankey shape-to-card conversions; reject unsupported object metadata and unknown styles, and disclose remaining shape loss
- [x] Retain locked, layered, grouped and contained Sankey shape nodes as ordinary objects rather than lose their deferred board structure
- [x] Reserve retained object tombstones when generating connector-label and table-cell IDs
- [x] Retain bounded source-ID tombstones for imported named layers and warn when the page budget drops them
- [x] Retain bounded source-ID tombstones for imported scene containers and represented table cells, with native save/reopen and budget warnings
- [x] Warn when nested non-vertex structural cells are flattened, retaining their representable descendants and protecting the source
- [x] Warn when hidden or empty edge-child vertices are skipped, retaining their connector and protecting the source
- [x] Warn when connector-label children carry collapsed state, terminals or geometry not represented by label placement
- [x] Warn when a positioned connector-label child specifies an unsupported background color
- [x] Warn when a positioned connector-label child's color or text opacity cannot be interpreted faithfully
- [x] Preserve supported inline runs and list paragraphs when folding absolute connector child labels, within text and formatting limits
- [x] Preserve imported zone source IDs with bounded owner metadata, tombstones, native v54, XMind and recovery; clear ownership on sheet copy and duplication
- [x] Preserve imported card and converted Sankey topic source IDs with bounded owner metadata, tombstones, native v55, XMind and recovery; retain IDs on moves and clear ownership on copies
- [x] Preserve representable flat group source IDs with bounded owner metadata, tombstones, native v56, XMind and recovery; disclose flattened or undersized groups and clear ownership on copies
- [x] Generate zone and group IDs without colliding with retained tombstones or live source IDs, including nested parent references
- [x] Generate mind-map boundary, branch-edge, relationship and summary IDs without colliding with retained tombstones or live source IDs
- [x] Generate topic and Sankey node/edge IDs without source collisions, updating connectors and the version 2 Sankey semantic extension consistently
- [ ] Source-preserving in-place save with loss preflight (architecture §3.5); requires D6

### D11 — Automatic layouts

- [x] Tree, flow, radial, circle and organic layouts for selected board objects
  - [x] Pure, deterministic, bounded core algorithms (≤ 500 items, ≤ 5,000 connections) with a guaranteed non-overlap pass and a fixed top-left anchor
  - [x] One undoable board command per layout in the Board menu, context-menu Layout submenu and command palette, translated in all locales
  - [x] Groups, selected containers with their descendants and floating mind-map branches move rigidly; locked items are skipped as Align does
  - [x] Bends of unlocked connectors between items that moved apart are reset; locked connectors keep theirs
- [ ] Support draw.io's remaining layout algorithms and options, including ELK and org-chart variants
- [ ] Route edges as part of automatic layout, preserving supported attachment and waypoint semantics

**D11 rubber-duck critique.** A pre-implementation review found five
blocking traps, all addressed: a container and its selected child were
separate units and would have moved the child twice (a shared resolver now
merges descendants into their container, and endpoints map through the
hierarchy); floating topics were measured without their branch (the unit now
uses the visible subtree footprint); clearing all bends could edit locked or
internal connectors (only unlocked, ungrouped connectors whose ends receive
different translations are reset); the algorithm returned no errors and had
no complete bound (input is validated, every pass is bounded and failure is
an error, not a partial edit); and size-unaware radial and organic placement
could overlap (ring radii use node diagonals, coincident nodes get a
deterministic direction, and a final uniform scaling about the centroid
provably removes any remaining overlap). Availability and execution use the
same resolver, so a container selected with its own child is one item and the
command is disabled. Tree and flow follow connector direction (`from → to`),
not arrowheads; cycles are broken only for placement. Duck check: layouts
place items, not edges, so routes and fixed ports are unchanged and long
flow edges are not given virtual bends; exactly coincident items fall back
to selection order; each moved item is applied with one sheet move, so very
large selections may be slow even within the 500-item bound. draw.io's ELK
options, org-chart variants and edge routing remain open.

### D12 — Insert tools and editors

- [ ] Mermaid, PlantUML (text subset), CSV, SQL and text import to board objects
- [ ] draw.io templates in the template gallery (CC BY 4.0 attribution)
- [ ] Edit style (raw key/value), edit geometry, raw diagram XML import/export dialog
  - [x] Exact geometry (left, top, width, height, rotation) for one canvas object in the nearby menu's Arrange section
  - [x] Exact geometry for one floating sticky, text or image card in the same section
  - [ ] Raw style key/value editor and raw diagram XML dialog

**D12 geometry rubber-duck critique.** A separate dialog would contradict the
nearby-menu concept, so the fields join the existing Arrange section, which
already holds layering. Values are committed through the existing numeric
draft (drag release, focus loss or a discrete step), so a drag is one undo
step, not dozens. Edits use the existing lock- and hierarchy-aware move,
resize and rotate operations; fields are hidden for connectors, grouped
members (editing one would move its group), multi-selections, and directly
or inherited locked objects, and Rotation is hidden for containers, which
cannot rotate. Left/Top describe the unrotated frame as in draw.io. Cards now expose the same fields in Arrange, keeping the unrotated top-left
fixed while resizing; their existing More style options keep their prior
center-anchored behavior. Grouped and locked cards do not expose these fields,
and Sankey-linked cards do not expose rotation. Stack-managed children and
table cells can reject a value, which is then reported and kept as a
correctable draft rather than hidden.
- [ ] Find/replace and tags on boards, ruler, units and page view
  - [x] Board find covers labels, descriptions, tags and property values; replacement changes labels, descriptions and property values atomically, and locks block only objects that would change
  - [x] Select board objects by exact tag from Edit › Select › Select by Tag, limited to visible objects
  - [ ] Ruler, units and page view
- [ ] Stencil editor for JSON definitions; draw.io stencil XML is converted on import, not edited

**D12 search-and-replace rubber-duck critique:** Requiring every hit to be
unlocked made a locked object found only by a tag block replacement elsewhere,
although tags are never replaced. The lock check now applies only to objects
whose text actually changes, after the candidate edit and before commit, so
failures still leave the sheet untouched. The dialog discloses the replaced
fields instead of claiming only titles and labels; notes, topic labels and tags
remain search-only because replacing them could merge or empty names. Tests
cover atomic rollback on oversized property values and locked targets, and
the disclosure in short windows. Ruler, units and page view are still
missing, so the D12 item stays open.

**D12 tag selection rubber-duck critique.** Find matches tags as substrings, so
`web` also finds `webapp`; Select by Tag selects exact tags only. It lists
and selects only objects the user can see: hidden layers and collapsed
container contents are excluded, so a selection never targets invisible
objects. Selection is not an edit (no undo entry), and pending label drafts
are committed first. Only the first 40 tags are listed, with a hint to use
Find for the rest. Duck check: draw.io's Tags panel also hides objects by tag;
Thinoda leaves visibility to layers rather than adding a second, persistent
hidden state. The context menu keeps its static entries, so the command is in
the main Edit menu.

### D13 — Other formats

- [x] JPEG visual export through the shared raster renderer
  - [x] Flatten requested JPEG transparency onto the selected sheet's theme background and disclose that loss
- [x] Lossless WebP visual export through the shared raster renderer
- [x] Static GIF visual export through the shared raster renderer, with palette and transparency disclosure
- [x] Recover bounded static inline GIF/WebP images on draw.io images and image cards; warn on conversion and failed recovery, including rasterized SVG
- [ ] Embedded diagram in PDF import; VSDX, VSSX, Gliffy and Lucid import (VSD binary out of scope); VSDX export

**Inline image rubber-duck critique:** Converting a static GIF or WebP to PNG
retains decoded pixels but not the original encoding or metadata; SVG
rasterization also loses the editable vector source. All three therefore
require resource-loss consent and source protection. Animation is rejected,
not silently flattened, and malformed, oversized or unavailable images warn
on both image and image-card paths. The shared bounded decoders and existing
per-sheet resource limits apply; animated image support and preserving
original image encodings remain open.

**GIF rubber-duck critique:** GIF is a static, flattened image, not an
animation or editable diagram. Its limited palette and binary transparency
can visibly alter colours and antialiased edges, so export preflight always
discloses that loss, even on a board without other warnings. The encoder
shares the bounded raster and clipping path; Chromium coverage checks the
download and mixed-board rendering. The format-wide loss contract explicitly
includes the GIF warning alongside any board-setting loss; WebP remains
lossless in the same checks. Both formats render supported local images and
reject unsupported embedded resources rather than silently dropping them.
Other D13 formats remain open.

**WebP rubber-duck critique:** WebP uses the existing bounded raster pipeline
and its lossless encoder, so alpha, theme, selected-sheet scope, clipping and
export-loss disclosure follow PNG rather than a second renderer. This is a
flattened visual export, not an editable draw.io container.
Pixel-for-pixel comparisons cover transparent and opaque output; a Chromium
test exercises consent, download and rendering of a mixed board. Other D13
formats and cross-browser verification remain open.

**JPEG rubber-duck critique:** JPEG is already available through the normal
visual export path, so treating it as missing would duplicate existing work.
It is a lossy, opaque image rather than an editable draw.io container.
When transparency is requested, render the sheet background instead of
silently compositing over black, and disclose the loss in preflight. This
preserves light, dark and custom-background contrast, but is not transparent
output. Other D13 gaps and draw.io visual parity remain open.

### D14 — Appearance and effects

- [ ] Adaptive diagram colours, sketch style, math in labels, flow animation

### D15 — Visual parity and hardening

- [x] Keep the new-file `+` visible beside responsive file tabs, including when tabs overflow
- [x] Replace the main ribbon with the outliner ribbon in outliner mode and improve the selected-topic heading
- [ ] Reference renders from draw.io for representative shapes of every bundled family, compared within tolerance
  - [x] Compare three electrical symbols at tall and wide aspect ratios; retain remaining family and tolerance work
  - [ ] Preserve stencil stroke joins, caps and miter limits across canvas and visual exports
    - [x] Carry joins, caps and miter limits into the renderer-neutral stencil primitives
    - [x] Apply them to solid and dashed SVG paths and SVG-based raster and PDF exports, including miter-aware export bounds
    - [x] Apply them to solid and split-dash paths in editable PowerPoint slides
    - [x] Retain stencil shadow joins, caps and miter limits in SVG-based and editable PowerPoint exports
    - [ ] Apply them to the canvas, including stencil shadows; verify PowerPoint host rendering and frame-edge clipping
- [x] Fuzz and size budgets for the bundle decoder and catalog
  - [x] Enforce the shipped one-megabyte catalog/blob budget and the per-package inflation limit
  - [x] Bound aggregate indexed alias bytes before allocating catalog keys
  - [x] Add a production-path catalog and decoder fuzz target with a real bundled seed, corruption tests and CI campaigns
  - [x] Complete and retain a sanitized 30-minute acceptance campaign for the committed candidate

**Bundle-fuzz rubber-duck critique:** Fuzzing a second parser would not protect
the shipped bundle, so the feature-gated harness uses the production catalog
index and bounded decoder. It mutates catalog ranges and compressed bytes; the
real package seed and deterministic malformed-input tests ensure decoding is
actually reached. Range overflow, out-of-bounds data and invalid UTF-8 now log
errors rather than silently returning. The release-size and inflation budgets
are executable tests, not a visual-fidelity claim. A small catalog can repeat a
long namespace across thousands of aliases; a preflight now rejects more than
4 MiB of aggregate alias-key bytes before constructing the index. Duplicate
keys count conservatively, and this bound does not claim to limit all allocator
overhead. The shipped keys total about 79 KiB. A smoke run or a configured CI
job is not the acceptance campaign. That campaign ran on Linux (nightly
`cargo fuzz`, default AddressSanitizer, the CI limits: 1,800 s, 2 GiB RSS,
1 MiB inputs, 5 s per input) against an immutable `git archive` of commit
`5fb194fefc1ec82a35313ed79c8ed5a9577d62b6`: 17,302,468 executions, no crash,
leak or timeout artifact, peak RSS 498 MB, 3,443 corpus entries. The log and
corpus are retained outside the repository with the session evidence. Duck
check: this is one local campaign of one target, not the hosted CI workflow
run, and fuzzing finds no proof of absence; the deterministic tests remain the
regression gate.

**Document-tab rubber-duck critique:** Putting New Tab inside the scrolling
content made it unreachable until the last file was revealed, while the
fixed-width strip wasted header space. New Tab is now outside the clipped names
area, immediately after the final capsule when everything fits and fixed at the
edge when it does not. The header reserves its actions before sizing the strip,
and uses logical widths for breakpoints so zoom cannot make the actions overlap
the tabs. Focus reveal, close confirmation and touch cancellation are retained;
longer names still truncate when necessary and retain their full path in the
tooltip. Scroll limits follow the final capsule rather than decorative padding,
so short names do not enable a meaningless arrow and closing files restores the
unscrolled position. Tests exercise the native Windows header and Quick Access,
overflow, resizing, zoom, keyboard and rejected touch.

**Outliner-ribbon rubber-duck critique:** Keeping both ribbons wastes vertical
space and separates structural actions from their selected topic, so native
Outliner mode replaces the main ribbon rather than stacking a second toolbar.
The heading has ribbon-title typography, spacing and a full-title tooltip; the
same nine undoable actions remain available. A bounded panel and focus-driven
scrolling protect the outline and inspector on small screens. Hiding the normal
ribbon initially broke pending Quick Access dispatch and text-focus snapshots;
both ribbons now share that preparation. The Outliner preserves the saved
collapse setting and Ctrl+F1, while Main Menu retains the full command tree;
Alt+1–8 opens the corresponding command category in a keyboard-navigable Main
Menu popup instead of replacing the Outliner ribbon. Changing categories resets
the popup scroll position; Escape closes it. Zen and non-native hosts retain
their inline controls. The nearby More actions popup is height-bounded and
scrollable: at high scale its former full-height menu covered the Outliner switch.
Scrolled-out submenu rows cannot open over the remaining commands, which keeps
the last actions clickable even in narrow windows. The extra Main Menu button
also reduced the tab budget:
header actions now collapse first, and very narrow native headers with Main Menu
hide Quick Access Undo while keeping its menu/shortcut and New Tab available.
Native layout, mode transitions, collapse, Quick Access, category shortcuts,
modal and text-editor blocking, including the PowerPoint export dialog,
localized controls, FIM, zoom and narrow-screen reachability have regression
coverage.

Out of scope: cloud storage connectors, comments, shared cursors, AI generation
(Thinoda has its own AI), plugins, revision history (Thinoda has its own
recovery) and binary `.vsd`.

### Progress record

| Date | Phase | Result |
|---|---|---|
| 2026-11-22 | D9 multiple-click input audit (open) | Reproduced text being inserted at the final caret when two editor clicks and text events arrive in one frame. The regression is ignored until ordered pointer handling is implemented; this is not a fix. |
| 2026-11-22 | D13 GIF export (partial) | Added static GIF visual export with an always-visible palette/transparency preflight; remaining D13 formats stay open. |
| 2026-11-22 | D13 WebP export (partial) | Added lossless WebP visual export with alpha and the same preflight as PNG; other D13 formats remain open. |
| 2026-11-21 | D12 select by tag (partial) | Edit › Select › Select by Tag selects visible board objects carrying an exact tag; hidden layers and collapsed contents are excluded. |
| 2026-11-21 | D12 exact geometry (partial) | The nearby Arrange section edits left, top, width, height and rotation of one ungrouped, unlocked canvas object, one undo step per completed field. |
| 2026-11-21 | D11 automatic layouts | Vertical/horizontal tree, vertical/horizontal flow, radial, circle and organic layouts for the board selection, as one undoable command with rigid groups, containers and floating branches, lock-aware bend reset and guaranteed separation. |
| 2026-11-21 | D9 toolbar input ordering (partial) | Text or IME input that follows F6, Enter or Space toolbar activation in the same frame is no longer lost or applied before the formatting; a submitted toolbar size field routes later text back to the label, and composition keeps the field focused. Real-platform input and pointer batches remain open. |
| 2026-11-21 | D12 board replace (partial) | Locked tag-only hits no longer block replacement elsewhere; the dialog discloses replaced and search-only fields in every shipped locale. |
| 2026-11-21 | D15 bundle fuzzing (partial) | Added a production-path catalog/decoder target, real bundled seed, corruption tests, release-size and inflation budgets, and CI smoke and acceptance jobs. The sanitized acceptance campaign remains open. |
| 2026-11-21 | D13 JPEG audit | Corrected the gap analysis: JPEG visual export already exists; transparency, editability and remaining format gaps are explicit. |
| 2026-11-21 | D15 Outliner ribbon | Native Outliner replaces the main ribbon with contextual actions, a larger selected-topic heading and shared collapse/Quick Access behavior; the nearby actions popup no longer covers the mode switch in the tested high-scale layout. |
| 2026-11-21 | D15 document tabs | New Tab stays visible during overflow; measured file names use the available header space without overlapping actions, including after resize and zoom. |
| 2026-11-21 | D8 Layers panel | The panel can be dragged, restores its last validated position within the window, and shows at least six layers when space permits. |
| 2026-11-19 | D9 card font search (partial) | Inline font choices can be filtered without losing the title selection; candidate choices are disabled during search-field IME composition. Arbitrary card color entry remains open. |
| 2026-11-19 | D9 card inline typography (partial) | Selection and caret controls set font family, numeric size and palette color; mixed values, inherited resets and same-frame size input with color clicks or save are covered. |
| 2026-11-19 | D9 card typing styles | Card-title inline toolbar and shortcuts now set caret typing styles; cursor changes and whole-value accessibility replacement clear them. |
| 2026-11-19 | D10 default zone folding | draw.io swimlanes are foldable by default; their conversion to nonfoldable frames now warns. |
| 2026-11-19 | D10 default zone header | The draw.io default header size of 40 is disclosed when conversion uses Thinoda's 24-unit frame header. |
| 2026-11-19 | D10 zone style diagnostics | Explicit unsupported zone styling requires import consent instead of being silently dropped. |
| 2026-11-19 | D10 group loss diagnostics | Group labels, collapsed state and unsupported group styles require import consent rather than disappearing silently. |
| 2026-11-19 | D10 group source identity | Representable flat groups own bounded source IDs through native v56, XMind and recovery; copies clear ownership, and flattened or undersized groups warn. |
| 2026-11-19 | D10 topic source identity | Imported cards and converted Sankey nodes own bounded source IDs through native v55, XMind and recovery; moves retain identity, copies clear it, and exports avoid collisions. |
| 2026-11-19 | D10 Sankey structure safety | Sankey shape nodes with locks, named layers, groups, zones or scene parents remain ordinary objects when conversion would lose their structure. |
| 2026-11-19 | D10 Sankey conversion safety | Shape-to-card conversion retains links, rich text, colors and alignment, and discloses remaining formatting loss. Nodes carrying unsupported metadata or unknown styles remain ordinary objects with a stale-extension warning. |
| 2026-11-19 | D10 container metadata diagnostics | Links, tooltips and enabled placeholder metadata on non-object containers are disclosed before conversion; supported card and object links remain intact. |
| 2026-11-19 | D10 zone source identity | Imported zones retain source IDs through native, draw.io, XMind and recovery; copies clear ownership and deleted IDs remain reserved within the page budget. |
| 2026-11-19 | D10 scene-container tombstones | Imported scene containers and represented table cells retain bounded source-ID tombstones, preventing generated IDs from reusing them after deletion. |
| 2026-11-19 | D10 Sankey ID remapping | Sankey nodes and edges avoid source collisions while version 2 extensions retain flows, settings and inactive stage metadata, including in editable images. |
| 2026-11-19 | D10 generated topic IDs | Non-Sankey topics avoid retained and live source IDs while maintaining connector references; conflicting fixed Sankey IDs refuse export. |
| 2026-11-19 | D10 generated map-decoration IDs | Boundary, branch-edge, relationship and summary cells avoid retained tombstones and live source IDs. |
| 2026-11-19 | D10 generated zone/group IDs | Zone and group IDs avoid retained tombstones and live source IDs, with remapped parent links preserving nested geometry. |
| 2026-11-19 | D10 layer tombstones | Imported named-layer source IDs survive as bounded page tombstones, including native save/reopen; dropped IDs warn. |
| 2026-11-19 | D10 structural-cell diagnostics | Unrepresented root/layer geometry, data and styles, regenerated structural IDs and normalized layer names now produce import warnings; supported layer visibility and locking remain warning-free. |
| 2026-11-19 | D10 generated-cell ID reservation | Deleted imported object IDs are reserved against newly generated connector-label and table-cell IDs. |
| 2026-11-19 | D10 nonstandard cell roots | Parentless vertices and edges are imported with a warning instead of being silently discarded; missing cell IDs also warn before a temporary ID is assigned. |
| 2026-11-19 | D10 editable-image document metadata | Editable draw.io SVG and PNG retain validated workbook attributes as well as the selected page; masked copies omit those attributes. |
| 2026-11-19 | D10 cell/geometry import diagnostics (partial) | Unknown cell and geometry XML, malformed flags, repeated geometry roles and non-connectable shapes/groups now warn; recognized non-connectable edge labels remain warning-free. |
| 2026-11-19 | D10 document passthrough budget | Oversized, unsafe or excess unknown `<mxfile>` attributes produce import warnings; individually valid attributes that fit are still retained and round-trip. |
| 2026-11-19 | D10 structural import diagnostics (partial) | Ignored document children, diagram attributes or children, and graph/root-level content produce import warnings; later-page warnings do not contaminate the first-page clipboard report, and the original source remains protected. |
| 2026-11-19 | D10 page passthrough safety (partial) | Unknown page attributes and object-ID tombstones are bounded during import; omissions warn. Reserved generated model attributes and oversized page passthrough are rejected even on empty sheets or direct export. |
| 2026-11-19 | D10 document passthrough (partial) | Safe unknown `<mxfile>` attributes survive draw.io, native v53, private XMind snapshots and recovery with or without pending ink; older or mismatched envelopes reject the metadata rather than dropping it. |
| 2026-11-19 | D9 card-title selection formatting (partial) | The active card-title toolbar applies strikethrough, subscript/superscript and bulleted/numbered paragraphs to selected text. Selection state is grapheme-aware, whole-card styling remains distinct, and each action participates in bounded local undo. Inline font family/size/color, caret formatting, Office visual parity and real input acceptance remain open. |
| 2026-11-18 | D9 accessible outside-click ordering (partial) | A synthetic SetValue before an outside click commits with the label; a request after the click does not modify the old label. Other pointer and assistive-technology paths remain open. |
| 2026-11-18 | D9 ignored text input (partial) | Empty text/paste and standalone newline text events no longer expand a partial grapheme selection or move an interior caret when egui ignores the input. |
| 2026-11-18 | D9 accessible toolbar ordering (partial) | Eligible SetValue batches beginning with toolbar focus preserve the new caret and process subsequent focus and formatting actions in order. Other unfocused controls, pointer/IME batches and physical accessibility clients remain open. |
| 2026-11-18 | D9 accessible whole-value replacement (partial) | Eligible AccessKit SetValue replaces all label text as a tracked rich edit, including empty and unchanged values; focused keyboard ordering and toolbar-focus updates have synthetic coverage. Physical assistive technology and broader event ordering remain open. |
| 2026-11-18 | D9 accessible label replacement (partial) | Eligible AccessKit text replacement uses grapheme-safe selections and tracked rich edits, including empty replacement, pending caret style, ordered keyboard batches and undo. Physical assistive technology and other accessibility actions remain open. |
| 2026-11-18 | D9 modified deletion caret (partial) | Eligible modified Backspace/Delete snap an interior grapheme caret toward the deletion boundary before egui computes its word deletion; local undo and pending typing styles are preserved. Other deletion endpoints and physical input remain open. |
| 2026-11-18 | D9 modified selection deletion (partial) | Eligible modified Backspace/Delete expand partial grapheme selections; shape and connector regressions cover deletion and undo. Collapsed modified deletion and platform input remain open. |
| 2026-11-18 | D9 grapheme-safe line breaks and IME (partial) | Eligible Shift+Enter and initial IME input expand partial selections or snap insertion carets; synthetic tests cover typing styles, empty events and active preedit updates. Physical IME and mixed pointer input remain open. |
| 2026-11-18 | D9 rich-label line breaks (partial) | Eligible Shift+Enter edits use the actual buffer range, preserving rich runs and list paragraph inheritance even for repeated newlines and selected replacements. |
| 2026-11-18 | D9 rich-label cut (partial) | An eligible cut uses the actual selected buffer range to retain formatting on remaining shape and connector label text; egui still emits plain-text clipboard content. Real-host clipboard behavior and rich external clipboard interoperability remain open. |
| 2026-11-18 | D9 rich-label keyboard batching (partial) | Pointer-free, non-IME keyboard events in a rich shape or connector label are processed in order, so eligible insertions and deletions retain their actual ranges even when they share a frame with a finish key. Pointer/IME batches and rich undo still require a complete ordered transaction model. |
| 2026-11-18 | D9 single rich-label deletion (partial) | Shape and connector label editors use the actual buffer range for a single eligible Backspace or Delete, including selected text and repeated characters; pending typing styles are retained at the resulting caret. The following batching slice extends this to eligible same-frame keyboard input. |
| 2026-11-18 | D9 single rich-label insertion (partial) | Shape and connector label editors use the actual egui buffer range for one eligible text or paste event, including repeated text and selected replacements. Other input still falls back to final-string reconciliation; the following deletion slice covers one more case, but ordered transactions remain incomplete. |
| 2026-11-18 | D9 editor input audit | Synthetic egui tests show that the layouter does not observe every operation in a frame, and final-string reconciliation can put formatting on the wrong occurrence of repeated text. The following single-insertion slice addresses one case; ordered rich edits are still required before treating that behavior as complete. |
| 2026-11-18 | D9 PowerPoint list alignment (partial) | Editable list paragraphs use the shared layout's list-block left edge, rather than centering or right-aligning each item independently. Three-slide fixtures opened and exported from Windows PowerPoint for left, center and right alignment; the wrapped fixture broke lines differently from Thinoda. Empty markers remain hidden, host spacing differs, and post-edit alignment is not guaranteed. |
| 2026-11-18 | D9 PowerPoint empty-list numbering (partial) | Windows PowerPoint suppressed markers on empty list paragraphs and counted following items differently from Thinoda. Following numbered paragraphs now carry explicit start values; a rotated fixture displays the subsequent 2 and 3 correctly. Empty markers are still hidden in PowerPoint, wrapping/alignment differs, and explicit starts can require manual renumbering after editing. |
| 2026-11-18 | D9 card editing foundation (partial) | Added a single validated, undoable title-and-runs mutation for floating sticky/text cards; invalid content and locked or unsupported targets are rejected without changing the document. No card editor uses it yet. |
| 2026-11-18 | D9 card baselines (partial) | Sticky/text-card superscript, subscript and explicit normal baselines survive supported draw.io HTML, rendering and persistence. Compatibility gates: native v52, sheet clipboard v41, board clipboard v45, topic clipboard v2, XMind `THINODA-XMIND-CARD-BASELINE-1`, recovery `THINODA-RECOVERY-CARD-BASELINE-1` and `THINODA-RECOVERY-INK-CARD-BASELINE-1`. Card run editing remains open. |
| 2026-11-18 | D9 rich draft transactions (partial) | A bounded core draft accepts ordered operations atomically, retains directional selection and typing style in local history, and groups explicitly opened composition updates into a single undo entry. It is not connected to the canvas or recovery; the event adapter, grapheme handling and IME lifecycle remain open. |
| 2026-11-18 | D9 ordered rich-text foundation (partial) | Range replacement accepts an explicit typing style while preserving list paragraphs; shape and connector caret typing reuse it, including joined-grapheme styling. |
| 2026-11-18 | D9 formatted-card PowerPoint paragraphs (partial) | Editable branch/zone slides now emit a single text box per formatted card, with styled runs, blank paragraphs, native bullet/number properties and card rotation. A rotated fixture opened without repair in Windows PowerPoint, exposed a numbered list, and retained its six paragraphs and list properties after editing and saving. Visual parity and LibreOffice remain unverified; wrapping can differ and viewport slides remain rasterized, so the loss warning remains. |
| 2026-11-18 | D9 formatted-card PowerPoint (partial) | Editable branch/zone slides exported styled runs and visible list markers in line shapes, including card rotation. This earlier representation was superseded by native paragraph/list output above. |
| 2026-11-18 | D9 formatted-card visual export (partial) | SVG, PNG, JPEG and PDF use the same rich-card layout as the canvas, including inline styles and list markers, and no longer report inline-text loss. Rewrapping caused by coordinate rounding is bounded. PowerPoint line-run output is a separate partial slice. Corrected draw.io import to warn and remove unsupported card baseline overrides rather than producing an invalid native document. |
| 2026-11-18 | D9 style reuse (partial) | Copy/paste style for objects and cards, including undo-safe dimension handling, and reusable quick styles are implemented and covered by focused app regressions. Default styles and declarative style rules remain open. |
| 2026-11-18 | D9 formatted-card draw.io interchange (partial) | Sticky/text-card titles import and export supported inert draw.io HTML inline styles and list paragraphs; initially baseline overrides warned and were dropped, until the later baseline slice. Native values are round-trip tested and `InlineTextDropped` is no longer reported for draw.io. Run-level card editing and unsupported source HTML preservation remain open. |
| 2026-11-18 | D9 formatted-card canvas rendering (partial) | Sticky/text card titles render inline runs, font overrides, colours and bullet/number markers through the shared rich-label painter. Core card layout reserves height for rich run sizes and accounts for list markers; the original plain-title painter is unchanged for unformatted cards. The core measurement and line wrapping use bounded glyph-width estimates; run-level editing remains unsupported. |
| 2026-11-18 | D9 formatted-card search (partial) | Search/replace updates existing sticky and text card runs and list paragraphs alongside their titles, including Unicode ranges and inserted paragraph boundaries. Changes remain atomic across locked cards, validation failures and workbook sheets, and are undoable. Canvas editing and draw.io interchange are not yet supported for these cards; canvas rendering is a separate partial slice. |
| 2026-11-18 | D9 rich extra connector labels (partial) | Positioned extra labels, including Sankey labels, support bounded rich runs and lists, canvas and inspector editing, undo and recovery, placeholder/search remapping, the inert draw.io HTML subset, and canvas and SVG/PNG/PDF/editable PowerPoint rendering. Inspector property changes preserve an active draft, merge unaffected label edits and reject conflicting text changes. Compatibility gates: native v51, sheet clipboard v40, board clipboard v44, XMind `THINODA-XMIND-RICH-EXTRA-CONNECTOR-LABELS-1`, recovery `THINODA-RECOVERY-RICH-EXTRA-CONNECTOR-LABELS-1` and `THINODA-RECOVERY-INK-RICH-EXTRA-CONNECTOR-LABELS-1`. Other label kinds and unsupported HTML source preservation remain open. |
| 2026-11-18 | D9 direct typography (partial) | Shape and primary connector label editors accept bounded fractional font sizes and arbitrary RGB/RGBA hex colours for a selection or subsequent typing; invalid values are reported without changing the draft. Apply and Enter keep edits atomic, keyboard/IME access and compact layout remain usable, and draw.io rich HTML round-trips fractional sizes and alpha. |
| 2026-11-18 | D9 inline font families (partial) | Shape and primary connector label editors choose installed, monospace or inherited families for a selection or subsequent typing; a searchable, keyboard-accessible picker fits compact canvases. Draft and pending families are registered for preview before commit, and rapid outside clicks retain the expected atomic-commit behavior. |
| 2026-11-18 | D9 inline typography (partial) | Shape and primary connector label editors adjust selected or subsequently typed font size and text colour, including inherited defaults, mixed-size indication, grapheme-aware selections, accessible keyboard navigation and compact palettes. Changes remain draft-only until an atomic commit; run-limit failures leave existing formatting intact. |
| 2026-11-18 | D9 caret typing styles | Character-style toggles at an empty selection affect subsequent text without editing the document until commit. Shape and primary connector editors preserve typing styles across deletion, list changes, paste and IME input; ordered pointer/keyboard processing places later text at the clicked caret. Rich-text limit failures retain the draft and block save/commit until corrected. |
| 2026-11-18 | D9 rich primary connector labels (partial) | Primary connector labels reuse bounded inline runs and list paragraphs, selection formatting and atomic undo; formatting-only drafts refresh recovery; search, replacement and placeholders remap runs; canvas, SVG/PNG/PDF and editable PowerPoint render them, including the authored portion of Sankey labels. Supported draw.io HTML imports and exports, with stale Sankey extensions rejected when externally edited formatting cannot be represented. Compatibility gates: native v50, sheet clipboard v39, board clipboard v43, XMind `THINODA-XMIND-RICH-CONNECTOR-LABELS-1`, recovery `THINODA-RECOVERY-RICH-CONNECTOR-LABELS-1` and `THINODA-RECOVERY-INK-RICH-CONNECTOR-LABELS-1`. Extra labels were covered in a later slice; unsupported HTML source preservation remains open. |
| 2026-11-18 | D9 label font registration | Installed font families referenced by whole shape-label styles and inline shape or card runs are registered before canvas rendering, so their requested faces are used rather than silently falling back. Missing families retain the existing warning and fallback behavior. |
| 2026-11-18 | D9 rich shape labels (partial) | Shape labels reuse bounded Unicode-scalar inline runs and list paragraphs; selection toolbar and Ctrl/Command+B/I/U edit the draft without changing the document until one undoable commit; drafts and formatting-only changes recover; native v49, sheet clipboard v38, board clipboard v42, XMind `THINODA-XMIND-RICH-LABELS-1`, recovery `THINODA-RECOVERY-RICH-LABELS-1` and `THINODA-RECOVERY-INK-RICH-LABELS-1`; supported draw.io HTML imports as inert text and exports escaped HTML; canvas and SVG/PNG/PDF/editable PowerPoint display the runs and list markers. Other label kinds and unsupported HTML source preservation remain open. |
| 2026-11-17 | D9 label text | Shape labels carry an optional `LabelStyle`: font family and size, bold/italic/underline/strikethrough, horizontal and vertical alignment, label position outside the shape on either axis, overall and per-side spacing, wrap on/off, label background and border colours and vertical (`horizontal=0`) text; one core layout wraps and places the text for canvas, label editor, hit testing, scene bounds and SVG/PNG/PDF/PPTX; Label text style inspector section for single and multi-selection with one undo per edit and style copy/paste; draw.io import maps the keys natively and export writes them back (`whiteSpace=nowrap` for unwrapped labels); native v48, sheet clipboard v37, board clipboard v41, XMind `THINODA-XMIND-LABEL-STYLES-1`, recovery `THINODA-RECOVERY-LABEL-STYLES-1` and `THINODA-RECOVERY-INK-LABEL-STYLES-1` |
| 2026-11-17 | D9 shape style | Native shape style effects on `ObjectStyle`: dash pattern with fixed-length option, linear (north/south/east/west) and radial gradients, drop shadow, glass highlight, fill patterns (hatch, cross-hatch, dots, dashed, zigzag) and horizontal/vertical flip that mirrors built-in, polygon and stencil geometry plus fixed ports about the frame centre (labels stay upright); canvas and SVG/PNG/PDF/PPTX paint them identically (stencil dashes now export too); Shape style inspector section for single and multi-selection with one undo per edit and style copy/paste; draw.io import maps the keys natively and export writes them back; native v47, sheet clipboard v36, board clipboard v40, XMind `THINODA-XMIND-SHAPE-STYLES-1`, recovery `THINODA-RECOVERY-SHAPE-STYLES-1` and `THINODA-RECOVERY-INK-SHAPE-STYLES-1` |
| 2026-11-17 | D8 tables | Native 2-D tables on the scene container engine: a table container owns row heights, column widths, cell slots with row/column spans and row/column stripe colours, and the shared layout pass places its cell containers (content never grows a cell); undoable insert row above/below and column left/right, delete row/column, merge/unmerge, stripes and track resize; Insert table (board toolbar, rows × columns) and a Table inspector section; canvas and SVG/PNG/PDF/PPTX paint stripes; draw.io `tableLayout` tables import natively with rowspan/colspan, stripes, grid lines and cell content and export as table/tableRow/partialRectangle cells; native v46, sheet clipboard v35, board clipboard v39, new XMind and recovery table envelopes; D8 complete |
| 2026-11-17 | D8 layers | Named board layers per sheet above an implicit base layer, with visibility and lock; hidden layers leave canvas, hits, lasso, select all, snapping and every export; locked layers lock their objects; Layers panel (View menu, Board menu and board toolbar) adds, renames, reorders, hides, locks and deletes layers and moves the selection with group members and container children; draw.io layers import named, hidden and locked instead of being flattened or dropped and export as layer cells; native v45, board clipboard v38, sheet clipboard v34, new XMind and recovery envelopes |
| 2026-11-17 | D7 page links | `data:page/id` links navigate to the sheet with the matching imported draw.io page id from the inspector ("Go to page" shows the target sheet title) and the link opener; missing pages warn; D7 complete |
| 2026-11-17 | D7 placeholders | `%name%` placeholders resolve in canvas and exports from object data, scene containers, `id`, `width`, `height` and the page name; draw.io import copies values inherited from non-container ancestors, resolves Zone names, and warns only for templates it cannot keep live (C4, kanban and home-lab now import resolved) |
| 2026-11-17 | D7 object data | draw.io link, tags, placeholders flag and custom properties import into per-object data instead of passthrough and export back; inspector "Data" edits link (Open link for http/mailto), tags, properties and the placeholder flag; canvas hover shows description and link; find matches tags and property values, replace edits values; native v44, board clipboard v37, sheet clipboard v33, new XMind and recovery envelopes |
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

**D9 multiple-click rubber-duck critique:** Processing only the first editor
press and then exposing the frame's aggregated pointer state to egui does not
replay two clicks in event order. Splitting text alone is insufficient: pointer
interaction, selection and click count also need correct per-event handling.
A synthetic regression demonstrates the defect and remains ignored rather
than hiding it or claiming a fix. Drag, touch, IME and real-platform input
still need separate validation.

The egui 0.37 `TextEdit` derives its caret from one response interaction
position and applies `TextCursorState::pointer_interaction` using aggregate
press and click state. Its raw pointer-button events retain event positions and
modifiers but not per-event timestamps or click counts; the per-event click
records are private to egui, and clicks in one input pass share its frame time.
Consequently, replaying raw clicks by assigning a caret would bypass word/line
selection, Shift extension and granular dragging. A complete fix needs an
ordered input contract that preserves those semantics (or an upstream egui
facility), plus mixed text, touch and IME tests; the existing failure remains
open until that contract is implemented.

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
shape. These remain rectangle fallbacks with the usual warning. The separate
electrical `buffer2` is implemented as an immutable data-defined stencil,
including its optional negation bubble and endpoint constraints; it does not
implement the variable-input gates. Its rectangular hit/perimeter outline is
invisible, so its shadow behavior still needs visual comparison with draw.io.
For `logic_gate`, draw.io 32.6.10 derives both the lead positions and connection
constraints from `numInputs` (up to 1,000), and selects AND, OR or XOR geometry
from `operation`; `negating` and `negSize` also affect the output. Thinoda's
stencil connections currently use fixed numeric positions and are limited to
256 per definition. Unrolling a few leads would silently change connection
semantics, while simply raising the limit would not make ports parameterized.
A complete data-defined solution needs bounded repeated geometry and dynamic
connection generation, with explicit budget and compatibility rules; until
then the warned fallback must remain. Source:
[draw.io 32.6.10 `mxElectrical.js`](https://github.com/jgraph/drawio/blob/v32.6.10/src/main/webapp/shapes/mxElectrical.js).
The separate transmission `testPoint` is data-defined too: the head follows
the stroke colour rather than the fill colour, and a lead appears only when
the height exceeds the width. Its rectangular hit/perimeter outline preserves
both endpoint constraints; like the buffer, visual shadow parity is still
unverified. The separate `dc_source_3_v2` uses an elliptical perimeter and
draw.io's minimum-sized polarity marks, including for non-square bounds;
reference-image comparison remains open beyond the bounded check below.

**Electrical stroke-width duck check.** The original three definitions used
the interpreter's fixed one-unit default, so imported and edited stroke widths
were ignored. New `-v2` definitions explicitly inherit the stored object width,
including when both colours use automatic defaults; all 69 previous definitions
remain unchanged so their saved hashes and appearance still resolve.
Explicit legacy references keep their old behavior, including on draw.io
round-trip; changing those existing objects requires a separate migration.
The new root-object policy is capability-gated so older interpreters cannot
silently evaluate it with different stroke semantics. Older builds without
the new definitions still fall back to their legacy shape.
Using draw.io 32.6.10 and one unlabelled 100×80 instance of each symbol at
stroke width 2, the raster probe showed a thinner Thinoda outline before this
change. With image padding aligned, mean RGB difference fell from 6.826 to
5.292 on a 0–255 scale; antialiasing and geometry still differ. This is not a
visual acceptance threshold or evidence for effects or other instance-unit
definitions. A follow-up draw.io 32.6.10 comparison used the same three unlabelled symbols at
60×120 and 120×60, black two-unit outlines, and a white background. After a
13-pixel padding alignment, the whole-image mean RGB differences were 4.431
and 4.348 respectively on a 0–255 scale. The three horizontal regions
(test point, DC source, buffer) measured 0.924/4.453/9.410 for tall and
1.283/5.561/6.407 for wide. These averages include whitespace and are not a
tolerance or proof of per-symbol parity; the larger buffer differences merit
geometry and antialiasing investigation. Effects, other sizes and other
instance-unit definitions remain to be audited.

**D15 stroke geometry critique.** The tall buffer's draw.io SVG and Thinoda
SVG have matching triangle vertices, but draw.io emits a miter join while
Thinoda emits a round join; a one-pixel horizontal realignment reduces the
buffer-region mean difference from 9.410 to 3.095. The interpreter retains
join, cap and miter limit in `StrokePaint`. Renderer-neutral `StencilStroke`
now retains all three, and SVG and SVG-based raster and PDF exports apply
them to solid and dashed paths. Export bounds measure the painted stencil so
long miters are not clipped; a thick, acute buffer regression verifies this.
Editable PowerPoint branch and zone slides now emit the corresponding DrawingML
caps, joins and miter limits, including on split dash segments. Out-of-range
miter limits fail explicitly instead of producing invalid Office XML; ordinary
paths retain their previous round geometry. Viewport slides remain rasterized.
Stencil shadows in visual exports now carry the same join, cap and miter limit
as their source stroke; they remain solid, as before, and bounds measurement
includes the shifted geometry. The change leaves built-in shadows and shadow
opacity unchanged. SVG shadow-only raster comparisons cover different joins
and caps; a thick miter with a shadow exercises export bounds, and the editable
PowerPoint test checks the shadow line itself. Canvas strokes and shadows still
need equivalent geometry and antialiasing. PowerPoint shadow paths are
serialized; the shadow-enabled fixture opens in Windows PowerPoint without a
repair prompt and exposes the separate shadow paths, but visual verification
remains open because the captured slide surface was blank. The electrical
buffer fixture opened in Windows PowerPoint without a repair prompt and displayed
its pointed joins. Editable freeform polygons and polylines now use tight path
bounds and local coordinates instead of a slide-sized bounding box; selecting the
buffer triangle in PowerPoint showed handles around the triangle. Multipart
stencils still export as separate shapes rather than a single editable group.
Horizontal and vertical paths use a one-EMU minimum extent; serializer tests
cover negative offsets and reject bounds that overflow, but off-slide rendering
has not been verified in PowerPoint.
Other joins, caps, hosts and slide-edge clipping remain unverified. Open XML SDK
validation of the new fixture runs in CI because the local .NET runtime is
unavailable.
Ellipses also use sampled polygon paths in
Thinoda rather than native SVG ellipses. These are separate sources of visual
difference, not evidence that the stencil definition is wrong. With the same
whole-image translation search before and after the SVG change, mean RGB
difference fell from 3.703 to 3.129 for tall and from 4.202 to 2.603 for wide.
These whitespace-inclusive values are diagnostic, not visual acceptance.
For explicitly referenced stencils, a present but unknown choice uses the
matching draw.io shape's declared absent value rather than reviving a stale
Thinoda override; missing keys still preserve that override for older files,
and mismatched shapes cannot supply another definition's defaults. Invalid
numeric values keep their existing behavior rather than guessing draw.io's
semantics.
All new titles
are translated into the eight UI languages. Choice options show readable
labels (for example "Intermediate catching") while import and export keep
draw.io's keywords. Parameter labels and option names are English only.

**D6.** Ownership is a conservative superset: every key Thinoda reads or
writes for any object kind is owned and never passed through, so a few keys
that would be safe to keep are dropped. Keys that depend on an owned key
(`dashPattern` on `dashed`, `gradientDirection` on `gradientColor`, ...) are
dropped when the owner is missing or off, so a cleared gradient never comes
back. At D6 only canvas objects carried passthrough; card, topic and zone IDs
and unknown keys were lost. D10 later added zone source-ID ownership, while
unknown zone keys and card and topic IDs remain unpreserved. At D6,
tombstones covered only cells that became objects; D10 later added named-layer
and zone IDs. Export gives a
source id priority over a generated `o{id}`. D10 subsequently added numeric
Sankey ID remapping while retaining the version 2 extension. Unmappable cells are still dropped with a
warning; storing them as opaque objects and cut-once provenance belong to D10.
D10 later added bounded workbook-level `mxfile` attributes. Named style tokens are not kept.
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

**D7 object data.** Each attribute now has one owner: draw.io data
attributes live in object data, everything else stays passthrough, and
validation rejects a name in both, so export never writes one twice.
Known approximations:

- The tooltip is the object's description (`alt_text`); draw.io's separate
  `tooltip` attribute and the description are one field.
- Files saved as native v37-v43 keep link and placeholders in passthrough
  attributes; they still export, but the inspector does not show them until
  the diagram is re-imported.
- Export writes link, tags, placeholders and properties before other
  passthrough attributes, so attribute order differs from the source file;
  `placeholders="0"` is dropped.
- Only http(s) and mailto links open; page links navigate (see D7 page
  links); other draw.io links (custom actions) are kept but not followed.
- Tags are found but not replaced, since replacing could merge or empty tags.
- Board cards still map links to the topic hyperlink.
- Properties are renamed by removing and re-adding them.

**D7 placeholders.** Labels keep the written `%name%`; canvas, SVG/PNG/PDF and
PowerPoint resolve it at paint time with draw.io's pattern, `%%name%`
escape and lookup order, and draw.io export writes the raw text back.
Known approximations:

- Ancestors are Thinoda scene containers. On import, values a draw.io
  ancestor holds that does not become a container (Zones, groups, layers) are
  copied into the object's own properties, so later edits to that ancestor do
  not propagate.
- Zone names resolve once on import and topic or card text stays literal; both
  report the placeholder warning, as do dates, times, page numbers, page
  count, file name, edge length, unit sizes and `placeholder` bindings, which
  stay literal.
- `width` and `height` are the stored frame, not rotated bounds; `id` is the
  draw.io cell id when known.
- Find and replace work on the written text, not the resolved text.

**D7 page links.** `data:page/id,<id>` follows to the sheet whose imported
`<diagram id>` matches, from the inspector's "Go to page" button and the
link opener; an unknown page shows a warning instead of a browser launch.
Export keeps imported page ids, so links round-trip. Known approximations:

- Only sheets imported from draw.io carry a page id; sheets created in
  Thinoda get one on export, so new page links must be typed as
  `data:page/id,<id>` until a page picker exists.
- Links resolve against the open document only, not other files.

**D8 layers.** `CanvasLayer.layers` holds the named layers painted above an
implicit base layer (`CanvasObject.layer: None`), and `objects` stays stably
sorted by layer so the existing paint, hit and z-order code needs no layer
awareness; z-order commands move objects only within their layer. Hidden
layers are filtered when the scene is built, so canvas, hits, lasso, select
all, snapping and SVG/PNG/PDF/PPTX exports skip them, and the selection drops
objects that become hidden. Known approximations:

- The base layer cannot be renamed, hidden, locked or reordered; a draw.io
  first layer that is named, hidden or locked therefore imports as a named
  layer above an empty base layer.
- Layers apply to board objects only. Topics, cards and Zones stay on the base
  layer; draw.io cards, frames or nested content on another layer import there
  with a warning, and on export an object on a named layer leaves its Zone
  because draw.io gives each cell one parent.
- Layers still split into the Below and Above bands around the mind map
  rather than interleaving with it.
- There is no active layer: new objects go to the base layer and are moved
  with "Move selection here". Deleting a layer moves its objects to the layer
  below; a locked layer cannot be deleted.
- Pasted objects keep their layer only when the target sheet has a visible,
  unlocked layer with the same name; otherwise they land on the base layer.
- Connectors attached to objects on a hidden layer still render from the
  stored geometry.

**D8 tables.** `scene.tables` maps a table container to a `SceneTable` with
row heights, column widths, every cell's slot and span, and stripe colours.
Cells are ordinary free scene containers parented to the table, so selection,
content, locks, z-order and exports reuse the container engine; the shared
layout pass derives every cell frame from the grid and moves cell content with
its cell, and validation rejects overlapping, out-of-range or non-rectangular
spans, collapsed tables or cells, stack layouts on the table, growing cell
layouts and more than 256 rows or columns or 16 384 slots. Known
approximations:

- Stripes use the colour of a cell's start slot, so a merged cell takes one
  stripe; rows paint over columns as in draw.io, and only cells without a
  visible fill of their own are striped. The inspector toggles a default
  light stripe; other colours come from draw.io.
- draw.io paints table grid lines on the table; on import they become the
  sides of each partial-rectangle cell, so a later border change is per cell.
- A table header is the container's Free layout header; draw.io `startSize`
  maps to it and `horizontal=0` to a left header.
- Tables whose rows have different cell counts, rotated or collapsed tables,
  and tables inside Zones keep the previous generic container import. Content
  of draw.io cells hidden under a merge is dropped with a Formatting warning.
- Resizing the table frame scales its tracks proportionally; there is no
  on-canvas track divider dragging yet, only the inspector's Row height and
  Column width fields.
- Export does not write `adaptiveColors`, so re-importing an exported page
  can warn about Formatting for fixed colours.

**D9 shape style.** `ObjectStyle` gains `dash`, `gradient`, `fill_pattern`,
`shadow`, `glass`, `flip_h` and `flip_v`; validation rejects them on anything
but shapes. Fill and stroke opacity were already folded into colour alpha and
rotation already existed, so they are reused rather than duplicated. One
shared `shape_style` module mirrors outlines before rotation, splits dashes and
builds gradient, pattern and glass geometry for canvas and every export, and
the scene mirrors fixed connector ports with the shape. Known approximations:

- draw.io flips act on the screen axes; Thinoda stores flips in the shape's
  own frame, so a quarter-turned `direction` swaps `flipH` and `flipV` on
  import. Export writes the stored flips, which is equivalent because
  `direction` is folded into the geometry.
- The shadow is a fixed offset (2, 3) at 25 % black without blur;
  `shadowColor`, `shadowOpacity`, `shadowOffsetX/Y` and `shadowBlur` stay in
  passthrough.
- Glass is a white highlight over draw.io's glass region and is drawn only on
  filled shapes.
- Fill patterns use a gap of four stroke widths (at least 4) at -41°; draw.io's
  sketch parameters (`hachureGap`, `hachureAngle`, `fillWeight`) stay in
  passthrough. Sketch-only styles (`hachure`, `zigzag`, `sunburst`, ...) map
  to the nearest native pattern with a Formatting warning; unknown values and
  diagonal gradient directions stay in passthrough and warn.
- PowerPoint gradients span the shape's bounding box; viewport slides are
  flattened to an image as before.
- Effects map only on built-in and stencil shapes; text, image, container and
  table cells keep the keys in passthrough. The inspector offers dash presets
  only and shows the first shape's values for a multi-selection.

**D9 label text.** One renderer-neutral layout in `thinoda-core::label_style`
breaks lines and places the text block, so the canvas, label editor, hit
testing, scene bounds and every export agree; outside labels extend scene
bounds and select their shape. Known approximations:

- Line breaking uses an estimated glyph advance (0.55 em, 1.25 line height)
  rather than real font metrics, so wrapped lines can differ from draw.io's
  browser layout for proportional fonts.
- The requested family is stored and written exactly to SVG (first in a
  fallback list), PowerPoint and draw.io. The canvas uses the closest family
  Thinoda has loaded (otherwise the proportional UI font), and PNG/PDF render
  from the SVG with the installed fonts. The canvas simulates bold with a
  double paint (italic, underline and strikethrough use egui's text format).
- Labels without a style keep their previous rendering (12 px unwrapped on the
  canvas, 14 px wrapped in exports); draw.io files imported without the
  default stylesheet get an explicit 11 px label.
- `overflow`, `textDirection` and `labelWidth` stay in passthrough with a
  Formatting warning; `textOpacity` remains folded into the text colour.
  Whole-label styles for connectors, text inside tables and swimlane headers (where
  `horizontal` turns the header) are unchanged, and built-in library entries
  still omit outside label positions from their insert styles.
- The inspector shows the first shape's values for a multi-selection and
  edits only the changed field on every shape.

**D9 rich shape labels (partial).** The editor applies bold, italic, underline,
strikethrough and sub/superscript to selected graphemes, and bullet or numbered
formatting to selected paragraphs. F6 reaches the toolbar; Ctrl/Command+B/I/U
work in the editor. Text and formatting commit together and conflict detection
keeps a stale draft available. Canvas runs use measured advances, SVG spans
flow in one text element, and PowerPoint keeps each line as an editable text
box with multiple runs. Known limitations:

- The core line-break and bounds estimates still use approximate glyph widths;
  wrapping, backgrounds, hit bounds and mixed-font baselines need visual parity
  work. The editor approximates bold and baseline offsets.
- Per-selection font-family and arbitrary-colour controls and extra connector
  labels were added in later slices. Card, zone and container/table-specific
  title import paths are not covered by this shape-label implementation.
- HTML is parsed inertly and bounded; unsupported tags, attributes, nested
  lists, whitespace and resource-limit overflows produce formatting warnings.
  Unsupported source HTML is **not** retained for a later save. Thinoda's
  markers for styled line breaks, empty list items and inherited decorations
  can be discarded by other editors. D10 must preserve unsupported source
  rather than treating this conversion as lossless.

**D9 label font registration.** The app's existing per-document font loader now
includes whole shape-label families and inline shape/card runs. Unavailable
families still warn and fall back without changing the stored name; web builds
remain limited to fonts available to their renderer.

**D9 rich primary connector labels (partial).** The existing formatting toolbar,
shortcuts and draft conflict rules now also apply to primary connector labels.
Their rich layout is centered on the routed anchor; Sankey editing uses the
ribbon anchor, and its generated value remains outside authored inline runs.
Scene hit and crop bounds include resolved placeholders. The draw.io subset is
escaped and parsed inertly; edited generated Sankey suffix formatting makes the
native extension stale instead of losing that edit. Known limitations:

- Extra labels and per-selection family and arbitrary colour controls were
  added later. Whole-label connector font, spacing and alignment controls are
  not yet implemented.
- Bounds and line breaking still estimate glyph sizes. A small draw.io reference
  comparison exposed and corrected list-item alignment; fonts and label-backdrop
  gaps still differ, and broad visual parity remains D15 work. Unsupported HTML
  is warned about but its original source is not preserved; D10 still owns that
  requirement.
- Recovery now detects formatting-only changes even after earlier draft edits,
  at the cost of retaining the last observed rich draft while editing.

**D9 formatted card-title editing (partial).** Sticky-note and text-card titles
that already contain inline runs or list paragraphs can now be edited without
flattening their formatting. Ordered text-buffer edits reconcile runs and list
metadata, the editor renders those runs and list markers, and bounded local
undo/redo groups IME composition. Commits use the atomic rich card-title edit;
recovery snapshots validate document ownership and the original title and
formatting before preserving the draft. Existing unformatted title editing is
unchanged. Focused native tests cover repeated text, style/list preservation,
commit and undo. Selected title text also has toolbar controls for
strikethrough, subscript/superscript and bullet/number lists, alongside
bold/italic/underline shortcuts. These use grapheme-expanded ranges and
bounded local undo, and expose selected formatting state. Plain titles with
active text-edit history must be committed and reopened before inline
formatting. Inline font family/size/color, caret styles, other label kinds,
unsupported HTML preservation, Office parity and real IME/accessibility
acceptance remain open; D9 is not complete.

**D9 card selection formatting (partial).** Command/Ctrl+B, I and U toggle
bold, italic and underline for selected text in a formatted card title or a
freshly opened plain title. The active card-text toolbar now exposes
strikethrough, sub/superscript and bullet/number list controls, with selected
format state. Grapheme boundaries and inherited card styling are respected;
each operation uses local undo and commits atomically with the title. Duck
check: inline font family/size/color, caret typing styles and IME/platform
acceptance remain open. A plain title with active text-edit history must be
committed and reopened before formatting, so its existing undo history is not
silently discarded. Formatting is refused during IME composition.

**D9 formatted-card safety follow-up.** Deleting all or part of a formatted
title no longer indexes stale run boundaries during intermediate layouts;
selected-text replacement inherits the selected run rather than the preceding
text. Save-and-resume rebases formatting and refuses a different document.
Recovery has a focused snapshot round-trip and stale-format/foreign-owner
test. IME composition also closes when a commit changes no text.

**D9 card input ordering (partial).** Pointer-free keyboard, IME and
title-targeted accessible text-value actions in a focused formatted title are
processed event by event before the visible editor pass. IME candidate keys
are protected, undo/redo during composition is refused, and a completed
composition is one local undo step. Synthetic tests cover mixed input,
undo-before-typing and accessible replacement around a partial grapheme
selection. Eligible input before a canvas press is applied before pointer
handling; for a single editor press, text, IME and title-targeted accessibility
input after the press uses the clicked caret. Tab transitions to new topics
keep subsequent input in order, including before an outside canvas press.
Separate ordinary text events create separate local undo steps.

**D9 card graphemes (partial).** Single-event text/paste, line-break,
deletion, cut, accessible selected-text replacement and initial IME input
use the board-label editor's grapheme boundaries. Focused synthetic tests
cover replacement and deletion of a partial combining-character selection,
an empty accessible replacement, caret snapping and initial IME insertion.

Duck check: this outside-click protection runs at the canvas boundary. The
app's existing native/web raw-input splitter separately orders title input
before host controls; synthetic formatted-title text and IME tests now cover
the close-tab path, not every host action. Multi-press batches, other
accessibility targets, and input after a rejected commit still need broader
coverage. Title transitions are capped at 32 per input batch with an error.
Full grapheme parity and real-platform IME, screen-reader and visual acceptance
remain open.

**D9 unavailable card source (partial).** A dirty formatted title draft is no
longer discarded when its card is deleted or becomes unavailable to layout.
The user is warned once and can cancel the edit; restoring the source allows
the draft to resume. A missing source is rejected for recovery rather than
applied to another card. Duck check: this retains the draft in the current
view, not an independently durable orphan-recovery record; document reset and
other programmatic host paths still require separate review. An IME candidate
Escape is not treated as cancellation, but composition updates while the source
is unavailable are not incorporated into the retained draft.

**D9 caret typing styles.** The toolbar and Ctrl/Command shortcuts now toggle
the style inherited at the caret. Typed or pasted graphemes receive that style;
moving the caret clears the pending override. Batched keyboard and in-editor
pointer events are handled in order, and deleting text or toggling lists keeps
the pending style. A text/run limit failure leaves the draft in the editor and
blocks commit, save and recovery snapshots until the edit is corrected or
undone; the unrepresentable draft cannot itself be persisted. Typing style is
transient and is not saved until text is entered.

**D9 toolbar input ordering (partial).** Text that arrived in the same frame
as a toolbar activation was either lost or inserted before the formatting it
followed, because focus was forced back to the label before the activation was
handled. Eligible batches containing F6, Enter or Space are now processed in
order, so the activation completes before later text is applied. IME candidate
keys are protected only while composing; a toolbar size field keeps focus during
composition and returns later text to the label after submission. Tests cover
shapes, primary and extra connector labels, plain text and IME. Duck check:
these are synthetic egui event batches, not real Windows, macOS or browser IMEs,
and pointer batches still use final-string reconciliation, so the parent items
remain open.

**D9 inline typography (partial).** The label toolbar adjusts selection and
caret font size in one-unit steps and provides the shared 12-colour palette.
Default clears that property's override without disturbing other formatting;
mixed sizes are shown as such and a step starts from the inherited size. Page
navigation retains the text selection. Narrow canvases use compact swatches.
Direct numeric sizes and arbitrary colours were added in a later slice.

**D9 inline font families (partial).** The searchable family picker applies an
installed family, monospace or the inherited default to selected graphemes or
pending typing. Named faces are registered while the edit is still a draft, so
the editor previews them before committing. Keyboard focus can reach every
filtered choice and scrolls it into view. The list lays out all matching
families to support focus traversal, so its work grows with the number of
matches; filtering reduces it. Missing installed faces retain their existing
fallback and warning behavior. Other label kinds and unsupported HTML source
preservation remain open.

**D9 direct typography (partial).** The size field accepts finite values from
6 to 256, including fractions. The custom colour field accepts RGB, RRGGBB and
RRGGBBAA hex values. Enter or Apply transfers a valid value to the label draft;
invalid input stays visible while editing and does not change it. Keyboard
input preceding an outside click is processed in order; an invalid submission
followed immediately by that click is reported. Leaving without applying
discards the unfinished field value. This supplies arbitrary colours but not a
graphical spectrum picker.

**D9 rich extra connector labels (partial).** Each positioned label has its own
bounded runs and paragraphs and can be edited directly on the canvas or from
the inspector. Draft conflicts are checked against the original label vector;
unrelated inspector changes preserve the committed draft, while removing another
label adjusts its index and conflicting text changes are rejected. Removing
the active label remains undoable. Empty text is valid only when a list marker
is visible. Label placement and hit bounds use the routed connector or Sankey
ribbon centreline, including resolved placeholders. Known limitations:

- Whole-label connector typography and placement fidelity remain separate work;
  line breaks and mixed-font bounds still use the existing estimates.
- Unsupported HTML is warned about but discarded. Other label kinds and safe
  source preservation still need their own implementations.

**D9 formatted-card search (partial).** Replacement uses the same range and list
remapping as shape and connector labels, inheriting the formatting at each
changed replacement's start; identical replacements retain mixed formatting.
Locked cards and values exceeding the rich-text bounds reject the whole
operation. Direct plain title editing still rejects formatted cards; run-level
editing is not exposed, viewport PowerPoint slides remain flattened, and the
supported draw.io HTML subset is handled by a separate interchange slice. This
does not claim complete rich-card support.

**D9 formatted-card canvas rendering (partial).** Sticky and text cards use the
shared rich-label layout and painter, including list markers, inline family,
size, colour and decoration overrides. Layout reserves the maximum
run line-height so larger formatting is not clipped by the previous plain-text
measurement. Duck check: this improves vertical sizing but the shared bounded
glyph-width estimate is not font shaping; run-level card editing remains open.

**D9 formatted-card draw.io interchange (partial).** Sticky/text-card import
and export reuse the existing bounded HTML parser/writer, preserving the
supported run and list model and not carrying executable markup. Duck check:
the source passthrough must not overwrite edits made to the native formatted
text, and unsupported HTML remains warned/lost rather than being represented
as if it round-tripped. Card baseline overrides now use the same inert model
and versioned persistence as other supported run styles.

**D9 card baselines (partial).** Supported subscript, superscript and explicit
normal baselines are retained by draw.io interchange and the shared renderers.
Duck check: a new field value changes validation compatibility even though the
JSON schema already contained it, so native files, clipboard payloads, XMind
snapshots and recovery use newer gates; downgraded payloads are rejected.
Canvas run editing and external PowerPoint paragraph/list validation remain open.

**D9 card editing foundation (partial).** The sheet and document expose one
validated mutation for title text and inline data, with locks and one undo
step. Duck check: this is not an editor; selection, IME, draft recovery,
concurrent edits and ordered operations must be handled before wiring it to
the canvas. Plain title editing continues to reject destructive changes to
formatted cards.

**D9 explicit typing-style replacement (partial).** The bounded range operation
now combines explicit run styling with list remapping, and existing shape and
connector typing uses it. Duck check: this is one validated operation, not a
transactional input stream; callers still own grapheme handling, selection,
IME and undo. The existing final-string reconciliation remains insufficient
for rich-card editing.

**D9 rich draft transactions (partial).** Core draft transactions preserve
ordered replacements, directional selection, formatting and bounded undo/redo;
composition can be committed as one entry or cancelled without destroying
redo. A simulated multi-update IME sequence, surrounding deletions and
commit undo as one transaction, including a rejected oversized range.
Duck check: this model alone does not establish correct egui input,
grapheme boundaries, per-event batching, normalization of egui's oversized
surrounding-deletion ranges, composition detection, conflict checking or
recovery. It is deliberately not yet wired to the title editor.

**D9 local label history (partial).** Shape, primary and extra connector label
editors now keep bounded text-and-format snapshots, directional selections and
caret styles for keyboard undo/redo. Typing is grouped, synthetic IME updates
are grouped through commit, and undo is blocked during an active composition.
Invalid rich input can be undone to the last valid snapshot. A plain draft
exceeding the rich-text limit retains egui's plain history even after becoming
short; formatting is explicitly unavailable until that draft is finished and
reopened, rather than silently discarding that history.
Duck check: this is not full ordered editing or physical IME acceptance.
Mixed pointer/IME batches and other unsupported reconciliation paths remain open;
the bounded history evicts old states and is not connected to card editing.

**D9 IME surrounding deletion (partial).** An isolated, pointer-free IME
surrounding-delete event now remaps formatting using the buffer's ordered
deletion ranges, including clamped oversized ranges, rather than a final-text
diff. Synthetic repeated text, pending caret style, composition undo/redo and
empty preedit/commit cancellation are covered. Isolated preedit and commit
replacements also use actual buffer ranges; pointer-free batches containing
IME and keyboard events are processed in order, and candidate keys remain
protected for the whole IME frame. Synthetic F6 events respect composition
and transfer subsequent input to the focused control. Synthetic editor
clicks before or after IME input and an outside-click commit retain the
expected formatting. Duck check: modified F6 and other pointer batches need
further work, and the tests do not establish
real IME or browser acceptance.

**D9 grapheme deletion (partial).** Unmodified Backspace/Delete expand a
collapsed or partially selected label range to whole Unicode graphemes before
egui edits it; local undo restores the original directional selection.
Synthetic shape and connector tests cover combining marks, flags and ZWJ
emoji. Isolated text/paste replacements and cuts expand partial selections;
an insertion caret inside a cluster snaps to its end, retaining its typing
style. Shift+Enter and the start of an isolated IME composition use the same
boundary policy, including pending caret style. Synthetic tests cover empty
initial IME events and active preedit updates that join adjacent graphemes.
Duck check: modified deletion, active IME surrounding ranges, mixed
pointer/input batches and physical input remain separate policy and acceptance
work.

**D9 ignored input (partial).** Empty text/paste and standalone newline text
events do not trigger grapheme selection changes because egui ignores them.
Duck check: this is limited to these synthetic input cases and does not
establish complete pointer, clipboard or IME behavior.

**D9 modified selection deletion (partial).** Eligible isolated Ctrl, Alt and
Command Backspace/Delete expand a partial selected grapheme before egui deletes
it; local undo restores the original text and formatting. Duck check: other
modifier combinations, pointer batches and platform input remain unverified.

**D9 modified deletion caret (partial).** For eligible isolated Ctrl, Alt or
Command Backspace/Delete, an interior caret snaps to the end of its grapheme
for backward deletion or the start for forward deletion, then egui computes
the normal deletion from that boundary. Duck check: egui's computed *other*
endpoint is not expanded, so this is not a general grapheme-safety guarantee
for modified deletion. Pointer batches and real platform shortcuts remain open.

**D9 accessible label replacement (partial).** An AccessKit
`ReplaceSelectedText` request for the active label uses the same grapheme
boundaries and tracked rich edit path as text insertion, including an empty
value deleting a selection. Requests for other nodes are not captured.
Synthetic shape and connector tests cover ordering, formatting and undo.
Duck check: this does not establish screen-reader or physical-device behavior,
other accessibility actions, or complete mixed pointer/IME ordering.

**D9 accessible whole-value replacement (partial).** Eligible AccessKit
`SetValue` requests use the tracked whole-selection edit, including empty or
unchanged values, rather than a final-string diff. Focused keyboard batches
retain event order; a synthetic toolbar-focus test covers a single request.
Duck check: multi-event ordering without editor focus is only partially covered
below; physical accessibility clients and other actions remain open.

**D9 accessible toolbar ordering (partial).** A pointer-free, non-IME batch
starting with a toolbar button focused processes an eligible `SetValue` and
subsequent focus or formatting events in order. The new caret replaces the
stale pre-update toolbar selection. Duck check: other unfocused controls,
modified focus shortcuts, pointer/IME batches and physical accessibility
clients remain open.

**D9 accessible outside-click ordering (partial).** A synthetic `SetValue`
before an outside click is committed with the label, while one after the click
does not modify the old label. Duck check: this single click pattern does not
establish general pointer batching or physical accessibility behavior.

**D9 formatted-card visual export (partial).** SVG and its PNG/JPEG/PDF
derivatives use the shared canvas rich-label geometry and style inheritance,
including rotated cards. Natural-size exports measure visible SVG bounds so
oversized titles are not cropped to fixed card dimensions; explicit viewports
and zone clips retain their clipping semantics. Duck check: line widths are
estimates rather than shaped advances, so wrapping can differ for unusual
fonts; the shared rich-label renderer has the same limitation.

**D9 formatted-card PowerPoint (partial).** Branch/zone slides write one
editable DrawingML text box per formatted card, with original paragraphs,
styled runs and native list properties. A rotated Windows PowerPoint fixture
opened without repair and its numbered list remained editable after saving.
A second rotated fixture showed that PowerPoint hides markers on empty
paragraphs and otherwise restarts the following numbering. Explicit starts on
all following numbered items preserve their initial values (2 and 3 in the
checked fixture), without adding hidden characters to the text. Rich layout
lines now expose their source paragraph index, and PowerPoint uses the shared
estimated left edge to position each editable list block. Windows PowerPoint
fixtures confirmed left, centered and right-aligned blocks open and export;
a wrapped item broke at different words from Thinoda.
Duck check: the margin is fixed at export and is not recalculated when a user
edits list content; host font metrics, tabs and wrapping can still differ.
DrawingML margin limits can clip extreme offsets, and adding the public
`LabelLine::paragraph_index` field affects external struct literals. Explicit
number starts can require manual adjustment after edits, empty markers remain
hidden, and other hosts remain unverified. Viewport slides are rasterized;
retain the loss warning.

**D9 style reuse (partial).** Object/card copy-paste and named quick styles are
already implemented and regression-tested. Duck check: do not treat named
quick styles as default-style behavior or declarative style rules; those need
separate precedence, persistence and undo contracts.

**D10 document metadata slice (partial).** Safe unknown `<mxfile>` attributes
are bounded, validated and retained across native workbook save/reopen and
draw.io export; namespace-qualified attributes are dropped with an import
warning because their namespace declaration is not represented. Attributes that
exceed the document budget are disclosed rather than failing the entire import.
Private XMind
and recovery envelopes are versioned, and generated attributes cannot appear in
passthrough. Duck check:
this is not sufficient for in-place saves. Opaque unsupported cells, other
workbook/page-level XML, container compression policy and save conflict/recovery
semantics still need explicit ownership and loss handling before an imported
source may be overwritten.

**D10 editable-image metadata (partial).** Single-sheet editable SVG and PNG
exports now carry the workbook's validated document attributes in their embedded
diagram. Duck check: document attributes may include sensitive source metadata
even when only one sheet is exported; masking removes them. The preview pixels
do not expose the attributes, but the editable payload does. This does not
preserve unrepresented XML or enable source overwrite.

**D10 page passthrough safety (partial).** Imported unknown model attributes
and tombstones are capped before storage, with omissions and invalid diagram
IDs reported. Page validation rejects generated model attributes and oversized
data even when a sheet has no objects. Duck check: warnings retain the source
for conversion-only imports, but existing topic/zone ID regeneration and
unsupported cells remain incompatible with safe source overwrite.

**D10 structural diagnostics (partial).** Unknown top-level children, diagram
attributes and children, graph children, root attributes and unrepresented cell
and geometry XML are disclosed instead of silently discarded. Duck check: this
identifies loss; it does not preserve the omitted XML or establish complete
semantic parity for all accepted cell and geometry values.

**D10 nonstandard roots (partial).** Parentless vertex and edge cells now
enter the ordinary object importer and produce a loss warning; missing cell
IDs also warn. Duck check: this repairs a silent omission, not the source
hierarchy; malformed or unsupported structural cells and their metadata still
need complete representation and diagnostics before in-place save.

**D10 structural-cell diagnostics (partial).** Root and layer properties that
the current model discards, including geometry, custom styles and regenerated
IDs, now produce import warnings; normalized names are disclosed as well.
Duck check: these warnings do not preserve structural data and are not an
exhaustive semantic audit. Complete layer and source fidelity remain open.

**D10 nested structural cells (partial).** Nested non-vertex cells are
flattened while their children are imported, so even an empty cell loses its
source identity and hierarchy. Import now reports this loss and requires the
existing conversion consent; tests also check that the original file cannot be
overwritten. This is a conservative warning, not opaque-cell preservation or
permission to save in place.

**D10 edge-child diagnostics (partial).** Hidden and empty edge-child vertices
are not represented as connector labels and now require conversion consent.
Their connector remains usable, but the omitted cells and their source
identities are not preserved. Visible nonempty supported labels retain their
existing warning-free path; opaque preservation remains open.

**D10 edge-label geometry diagnostics (partial).** A connector label's
collapsed state, cell terminals, source or target points, waypoints and
alternate bounds are not represented by label placement. Their presence now
requires conversion consent while the readable label remains available.
Duck check: this detects those losses rather than preserving the geometry;
other source-identity and opaque-data gaps still prevent in-place save.

**D10 edge-label backgrounds (partial).** Explicit child-label background
colors now produce a formatting warning because extra connector labels cannot
store their own backgrounds. Duck check: this conservatively warns even when a
specified color happens to match the canvas background; the label remains
editable, but the specified background is not retained.

**D10 edge-label text opacity diagnostics (partial).** Positioned child
labels now apply the same color and opacity loss checks as other cells, so
invalid or out-of-range values cannot silently become an ordinary opaque
label. Duck check: the warning protects the source but does not preserve the
unrecognized source value on the label.

**D10 absolute edge-child rich text (partial).** Folding an absolute child
into its connector label now retains supported inline styles and list
paragraphs, including Unicode offsets and bounded truncation. Existing
formatting is retained when the combined formatting exceeds model limits.
Duck check: folding still loses the child's independent position and identity,
and exceeding limits loses text or formatting; import warnings continue to
protect the source. This does not establish round-trip fidelity for in-place
save.

**D10 generated-cell ID reservation (partial).** Generated connector-label
and table IDs now avoid retained object and layer tombstones, so these new cells cannot
silently inherit a deleted imported object's ID. Duck check: topic and zone
source IDs are not retained;
topic and zone tombstones are not yet retained. This is not sufficient
to enable in-place save.

**D10 layer tombstones (partial).** Named layers with retained source IDs now
participate in the bounded page tombstone set, which survives native save and
undo snapshots. A budget omission warns instead of claiming retention. Duck
check: tombstones do not survive a draw.io export as deleted cells; neither
source overwrite nor complete ID
fidelity is enabled.

**D10 zone/group ID allocation (partial).** Generated zone and group IDs avoid
retained tombstones and live source IDs, including when a page budget dropped
the corresponding tombstone. Parent references use the allocated IDs and nested
geometry remains relative to the correct parent. Duck check: zones and flat
groups have since gained source ownership; nested groups remain flattened.

**D10 map-decoration IDs (partial).** Generated boundaries, branch edges,
relationships and summaries now avoid retained tombstones and live source IDs.
Duck check: their own source IDs are not retained.

**D10 topic and Sankey ID allocation (partial).** Generated topic and Sankey
IDs avoid retained and live source IDs, with connector terminals and the version
2 Sankey extension remapped together. Settings-only stage nodes are included
in the extension. Duck check: imported cards and zones have since gained source
ownership, but in-place save remains disabled; collapsed settings or endpoint
topics must be expanded for export.

**D10 scene-container tombstones (partial).** Imported scene containers and
represented table cells now enter the bounded tombstone set, which survives
native save/reopen and deletion. Their live source IDs continue to belong to
the canvas objects. Duck check: the budget can omit a tombstone with a warning;
the deleted ID is then not protected. At that point cards, zones, groups and
unrepresented table rows lacked owner-bound source IDs. Adding their IDs as anonymous
tombstones before adding ownership causes ID churn on every round trip, so that
approach was rejected; zones, imported cards and flat groups have since gained
ownership. In-place save remains disabled.

**D10 zone source identity (partial).** Zones own their source cell IDs, and
their IDs are reserved as bounded page tombstones for deletion. Native v54,
versioned XMind snapshots and recovery retain them; sheet clipboard and sheet
duplication clear zone ownership. Export reuses a unique source ID and updates
member parent references, falling back to a fresh ID for conflicts. Duck check:
page-budget omissions warn and may leave deleted IDs unprotected;
table-row and unrepresented-cell ownership still need work. Unknown zone XML is
not yet preserved. In-place save remains disabled.

**D10 container metadata diagnostics (partial).** The importer now warns when
non-object cells discard links, tooltips or enabled placeholder metadata.
Card links and object data remain supported. Duck check: this detects the loss
rather than preserving the metadata; unknown container XML and complete loss
preflight still block in-place save.

**D10 Sankey conversion safety (partial).** Shape-to-card conversion keeps
supported links, rich text, colors and alignment. A node with unsupported
object data, tooltip or unknown cell styles/attributes invalidates the Sankey
extension instead of losing that content. Candidate conversion rolls back
before reporting the stale extension; successful conversion reports the lost
shape appearance. Both paths retain their supported data and source identities
through native save/reopen and draw.io re-export. Duck check: fallback
intentionally uses ordinary diagram objects rather than Sankey semantics; full
lossless conversion remains open.

**D10 topic source identity (partial).** Imported cards and convertible Sankey
shape nodes retain a validated source ID on their topic. Moves retain ownership;
clipboard, duplication and masked-copy paths clear it. Native v55, versioned
XMind snapshots and recovery retain ownership, and deleted owners leave bounded
tombstones. Export reuses unique IDs, remapping Sankey nodes and semantic
references together. Duck check: Sankey requires canonical numeric `t` IDs;
unrepresentable or conflicting IDs are remapped. Nested groups, opaque cells
and full source-preserving save remain open.

**D10 group source identity (partial).** Representable flat groups own bounded
source IDs, including through native v56, private XMind snapshots and recovery.
Group copies and masked copies clear ownership; deletion retains bounded page
tombstones. Export avoids collisions with all live owners and remaps Sankey IDs
when necessary. Duck check: nested groups flatten and empty or one-member
groups are omitted, so those identities warn rather than round-trip; omitted
tombstones can permit reuse after deletion.

**D10 group loss diagnostics (partial).** Import now discloses discarded group
labels, collapsed state and group styles other than the supported lock flag.
Duck check: this warns rather than preserving the original content or styles;
group geometry and other container fidelity remain subject to further review,
and in-place save stays disabled.

**D10 zone style diagnostics (partial).** Explicit zone styles outside the
represented frame subset now report formatting loss, including header size,
orientation, collapsibility, stroke width and unknown styles. Duck check:
styles are disclosed rather than preserved; implicit defaults, complete zone
fidelity and source-preserving save still need further work.

**D10 default zone header (partial).** A missing `startSize` uses draw.io's
40-unit default, unlike Thinoda's 24-unit export; that difference now requires
import consent. Duck check: this does not preserve the original header size or
resolve other implicit defaults; source-preserving save remains disabled.

**D10 default zone folding (partial).** draw.io normally treats a swimlane as
a foldable container unless `collapsible=0`; Thinoda zones are not foldable.
That lost interaction now triggers consent even when the source omits the
style. Duck check: this diagnoses rather than implements folding, and does not
cover every implicit style or interaction.

**D9 card typing styles (partial).** Inline card-title controls and keyboard
shortcuts can set a typing style at the caret; input, IME composition, grapheme
joins and draft/document undo retain the appropriate runs. Moving the caret or
an accessibility whole-value replacement clears the pending style. Duck check:
the existing plain-editor history guard still requires reopening after untracked
plain edits, and font family, size and color run controls for cards remain open.

**D9 card inline typography (partial).** The card-title toolbar now applies
family, numeric size and palette color to selected text or subsequent typing,
with inherited resets and mixed-value detection. The saved companion cursor
protects the selection across the font popup. Existing custom colors are shown;
native and draw.io round trips retain the resulting runs. Duck check: the font
menu is not searchable and arbitrary color entry is not yet available for
cards. Synthetic focus and touch tests do not replace real-platform IME,
accessibility and visual acceptance; the plain-editor history guard remains.

**D9 card font search (partial).** The inline font menu filters installed
families and inherited/monospace choices, retains the title selection and
guards synthetic IME composition in its search field. Duck check: filtering
does not install missing fonts, and platform IME and accessibility testing
remain open.

**D9 card RGBA entry (partial).** Custom color controls edit red, green, blue
and alpha for whole-card text, selected text or subsequent typing; existing
rich-draft and document undo boundaries remain intact. Committing a custom
color can explicitly pin even the inherited value; Auto retains inheritance.
Duck check: the picker uses egui's color-space mode and accessibility metadata;
platform screen-reader, pointer and IME acceptance is still required. Color32
conversion unpremultiplies translucent channels, with the precision limits of
an 8-bit premultiplied representation.

**D9 selection-handle clearance.** Placement uses the actual handle hit
rectangles for shapes, connectors, topics, cards, relationships and images.
The nearest free position is found by a bounded sweep; constrained layouts
offer a compact Format button, and hide it if even that would block a handle.
Duck check: an intentionally expanded popover can temporarily cover the canvas;
nested menus and Escape must retain selection and focus. Extremely dense views
can have no unobstructed nearby position. Right-click still exposes context
actions, but formatting may require zooming out, resizing or repositioning.

**D10 Sankey structure safety (partial).** Candidate conversion now checks
deferred locks, groups and zone memberships as well as layers and scene
parents before removing a shape. Deferred membership is indexed once so the
check scales with the imported board size. Duck check: ordinary-object fallback
retains the structure but disables the Sankey extension until a lossless mapping
exists.

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

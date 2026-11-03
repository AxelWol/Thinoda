_Last updated: 2026-11-03 01:53:16 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Freeform boards: research, concept and plan

Status: **implementation in progress**. Board data, native format support,
export analysis, templates, and layered canvas rendering are implemented;
editing, accessibility, presentation, and export parity are still being integrated.
Companion document: [CLOUD-STORAGE.md](CLOUD-STORAGE.md) (cloud folder and
storage-provider gap analysis). Roadmap entries live in
[BACKLOG.md](BACKLOG.md#freeform-boards-and-flexible-map-roots) and
[PLAN.md](PLAN.md#r14---freeform-boards-and-flexible-map-roots).

## 1. Goal

Thinoda should also work as a **whiteboard**, not only as a tree-shaped mind map,
similar to Apple Freeform and Mural:

- free objects on an unbounded canvas (sticky notes, text, shapes, lines, ink,
  images, frames)
- maps **without** a central topic (only free objects), and maps with
  **several** root topics
- more **non-mind-map templates** ("Boards") in the New Map gallery

Hard constraints (repository conventions, see [PLAN.md](PLAN.md)):

- Don't break existing maps, file compatibility or exports. Every export either
  keeps the content or lists exactly what it loses, or refuses.
- Extend existing packages and models, don't build parallel systems.
- Native (Windows, macOS, Linux) and web (wasm) parity, all 10 UI languages,
  Light/Dark/Auto, keyboard and screen-reader access.

## 2. Sources

- Apple Freeform user guide (German edition, local copy in
  `R:\cloud\OneDrive\Freeform Docs`, converted to text for this research). The
  feature names below are paraphrased, not quoted.
- Public Mural product information (mural.co features, templates and
  facilitation pages, checked 2026-10-28).
- Thinoda source at v0.20.0: `thinoda-core` (`model.rs`, `layout.rs`,
  `templates.rs`, `document.rs`, `presentation.rs`), `thinoda-canvas`
  (`render.rs`, `view.rs`, `topic_drag.rs`, `relationship_editing.rs`),
  `thinoda-io` (`formats.rs`, `native_resources.rs`), `thinoda-app`
  (`template_gallery.rs`, `ui.rs`).

## 3. Thinoda's research baseline

The following inventory describes the application at the start of this
research; implementation progress is tracked in the status and roadmap above.

| Building block | Where | Use for boards |
|---|---|---|
| Floating topics with a stored position (`Topic.position`, `Sheet.floating`) | `model.rs` | Basis for sticky notes, text cards and image cards |
| Zones: named frame rectangle plus `roots` of floating trees | `model.rs` `Zone` | Closest existing "frame/area" and multi-root construct |
| Relationships with cubic control points (topic to topic only) | `model.rs` `Relationship` | Routing code for free connectors |
| Boundaries, summaries | `model.rs` | Unchanged |
| Topic shapes (rounded rect, rect, ellipse, capsule, diamond, underline, none), line styles | `model.rs` | Card shapes |
| Presentation slides (source = branch or zone), Pitch player | `presentation.rs` | Basis for Freeform "scenes" |
| Images and attachments as resources (native v3+) | `native_resources.rs` | Image cards, moodboards |
| Rubber-band selection, pan/pinch zoom, `Transform` | `thinoda-canvas` | Shared by all tools |
| Snapshot undo (whole active sheet, 300 steps) | `document.rs` | Must be budgeted for ink |
| Template catalogue (28 tree templates, 8 categories) | `templates.rs`, `template_gallery.rs` | New "Boards" category |

At that baseline, arbitrary shapes, ink/pen input, free connectors, z-order,
groups, locking, rotation, snapping guides, rootless or multi-root sheets, and
non-tree templates were missing.

## 4. Feature inventory and verdicts

Verdicts: **Fits** = reuse an existing system with small extensions.
**Adapt** = new, but fits the architecture after the groundwork in section 6.
**Later** = useful, depends on earlier phases or a spike. **Out** = out of scope,
with the reason.

### 4.1 Canvas and navigation

| Feature (Freeform / Mural) | Verdict | Notes |
|---|---|---|
| Unbounded canvas, pan, pinch zoom | Fits | Already present |
| Dot or line grid background (toggle) | Fits | Theme-aware paint layer, per-sheet setting |
| Snap to grid | Adapt | Needs object geometry from the shared scene |
| Alignment, size and spacing guides while dragging | Adapt | Same; topics take part only when floating |
| Zoom to fit / zoom to selection | Fits | Extend bounds to canvas objects |
| Scenes (saved, ordered views; present, print) | Adapt | Extend Presentation with a viewport source (section 5.6) |
| Mini-map / navigator | Later | Reuses scene bounds |
| Follow a collaborator's view | Out | Needs real-time collaboration (PLAN R12) |

### 4.2 Objects

| Feature | Verdict | Notes |
|---|---|---|
| Sticky notes (colour, size, text, lists) | Fits | Floating topic with a `Card` presentation (5.2) |
| Text boxes | Fits | Floating topic, card presentation without fill |
| Shape library with search | Adapt | New canvas object `Shape`; start with ~20 basic and flowchart shapes |
| Edit shape points, combine/subtract shapes | Out (for now) | Boolean path operations need a vector geometry library; revisit after F4 |
| Lines and arrows (straight, angled, curved) | Adapt | Canvas object `Connector` reusing relationship routing |
| Connectors that stick to objects and follow them | Adapt | Typed endpoints: topic, canvas object, free point |
| Diagram mode (auto-create next shape) | Later | After connectors and shapes |
| Tables | Later | Large feature; Tree Table exists for structured data |
| Images: place, resize, crop/mask, rounded corners, shadow | Fits / Adapt | Image card = floating topic with image; crop/mask later |
| Remove image background | Out | Needs on-device ML; not cross-platform |
| Link cards with previews | Adapt | Topic link plus optional cached title; no automatic network fetch without consent |
| Files and PDFs on the board | Later | Attachments exist; PDF page preview needs a renderer |
| Frames / areas (Mural) | Fits / Adapt | Extend Zone: visual frame, title, optional clipping |
| Icons, emoji | Fits | Topic markers (`Topic.markers`) already exist |
| Object descriptions for screen readers | Adapt | Required for every non-topic object (`alt_text`) |

### 4.3 Drawing (ink)

| Feature | Verdict | Notes |
|---|---|---|
| Freehand pen with mouse, touch, pen | Adapt | Canvas object `Ink`; phased (F5) |
| Pen styles: pen, marker, pencil, fountain pen, watercolour, crayon | Adapt / Later | Start with pen + highlighter (opacity); textured brushes later |
| Width, colour, opacity | Adapt | Per stroke |
| Pressure sensitivity | Later | egui reports `Touch.force` only; no pen type, tilt, eraser or hover. Optional, never required (5.5) |
| Object eraser | Adapt | Removes whole strokes |
| Pixel eraser (split strokes) | Later | Stroke splitting after F5 |
| Lasso selection | Adapt | Shared selection model |
| Fill tool | Later | Needs closed-path detection |
| Convert drawing to shape | Later | Shape recognition heuristics |
| Handwriting recognition / search | Out | No cross-platform on-device recogniser |
| Vector pen with Bézier points | Later | After shape point editing |

### 4.4 Arranging and editing

| Feature | Verdict | Notes |
|---|---|---|
| Move, resize, rotate (Shift = 15°/45° steps) | Adapt | Rotation only for canvas objects and cards, not tree topics |
| Bring forward / send backward / to front / to back | Adapt | Within layer bands (5.3) |
| Group / ungroup | Adapt | Canvas objects and floating cards |
| Lock / unlock | Adapt | Locked objects ignore drag and delete |
| Align and distribute | Adapt | Selection-wide |
| Duplicate (Alt-drag, Alt-arrow) | Fits | Copy/paste with ID remapping |
| Copy and paste style | Fits | Copy/Paste Style exists for topics (`style.copy`, `style.paste`); extend to objects |
| Find and replace across text | Fits | Extend search to canvas object text and alt text |
| Undo / redo | Fits | With a byte budget (5.4) |
| Keyboard shortcuts (lock, group, z-order) | Adapt | Use the shared command model (R13) |

### 4.5 Collaboration and facilitation (Mural)

| Feature | Verdict | Notes |
|---|---|---|
| Real-time co-editing, cursors, follow | Out | PLAN R12 (CRDT); not part of this package |
| Comments on objects | Later | Topic notes exist; threaded comments need identity |
| Voting / dot voting | Later | Can be done locally as stickers counting votes |
| Timer | Later | No timer exists today; simple to add but low value without collaboration |
| Private mode, summon, super lock | Out | Collaboration features |
| Outline view of the board | Fits | Outline lists frames and cards in reading order |
| AI clustering and summaries of stickies | Adapt | Existing AI review flow; consent rules apply |
| Large template library | Adapt | Section 7 |

### 4.6 Boards management, sharing, export

| Feature | Verdict | Notes |
|---|---|---|
| Board list with folders, favourites, recently deleted | Out | Thinoda is document/file based; recents and the OS file manager cover this |
| Share via iCloud link | Out | See [CLOUD-STORAGE.md](CLOUD-STORAGE.md) |
| Export PDF, print | Fits | After the shared scene renderer includes canvas objects |
| Export image | Fits | Same |

## 5. Concept

### 5.1 Flexible roots

Today every sheet has a mandatory `root: TopicId` (`Sheet::new` creates topic 1),
and about 770 places in the code read `.root`. Changing it to `Option<TopicId>`
would touch most crates at once. Instead:

- Add `SheetRootMode { Central, Hidden }` (default `Central`, current behaviour).
- In `Hidden` mode the existing root becomes an **anchor**: it stays in the model
  so the type invariant holds, but it carries no visible content. New anchors
  have an empty title; notes, tasks, labels, images, relationships, summaries,
  boundaries, numbering and presentation sources are forbidden. For compatibility
  with earlier v6 boards, an existing anchor title remains hidden and is preserved
  on save; showing the central topic reveals it. Hidden anchors cannot be edited.
- **Several roots** = several floating trees (existing `floating`), optionally
  grouped in zones. No new tree model.
- Before `Hidden` can be enabled, root-dependent code moves to central queries:
  - `semantic_roots(sheet) -> Vec<TopicId>`: `[root]` in Central mode, the
    floating tree roots (in reading order) in Hidden mode.
  - `visible_topic_ids(sheet)`: never contains the anchor.
  - `primary_content_root(sheet) -> Option<TopicId>`: for features that need
    exactly one root.
  - `project_to_outline(sheet, policy)`: one projection used by text, Markdown,
    OPML, FreeMind, XMind, DOCX/PPTX outline exports, AI context and the outline
    view.
- Root-dependent areas to migrate (from the critique): layout entry point,
  hit-testing, rendering, keyboard navigation and focus restore, outline,
  search, statistics, numbering, Tree Table, clipboard, presentation generation
  (branch sources), SVG/PNG/PDF bounds, AI context, accessibility tree, every
  outline export.
- Export policy for Hidden or multi-root sheets, chosen per format and shown in
  the export preflight: keep several roots (formats that allow it, for example
  XMind detached topics), **synthesize a named export root** (default for
  single-root outline formats, reported as a change), or refuse.
- Switching modes: "Hide central topic" is only allowed when the root has no
  content or its content is moved into a new floating topic first (one undoable
  edit). "Show central topic" restores Central mode.

### 5.2 Objects: hybrid model

The critique showed that a fully separate `CanvasItem` list would duplicate text
editing, styles, resources, links, clipboard, search, localisation and exports
that topics already have. Forcing ink and shapes into topics would break tree
semantics. Therefore:

| Object | Model | Why |
|---|---|---|
| Sticky note, text card, image card, link card | **Floating topic** with `TopicPresentation::Card { kind }` (colour, fixed size, optional rotation) | Reuses text editing, rich notes, resources, search, export, AI, a11y |
| Frame / area | **Zone** extended with persistent empty frames, `clip` and frame colour | Zones already own floating trees and are Pitch sources; freeform frames can also start empty |
| Shape | New `CanvasObject::Shape` | No text tree semantics; may carry a short label |
| Ink stroke | New `CanvasObject::Ink` | Point data, not text |
| Free connector | New `CanvasObject::Connector` with `Endpoint::{Topic(id), Object(id), Point(pos)}` | Generalises relationships without changing existing ones |

New data lives in `Sheet.canvas: CanvasLayer { objects, groups }`. Every
object has: `id` (shared allocator, `bump_next_id` must scan objects and
groups, and slides too), `rect`, `rotation`, `band`, `order`, `group`,
`locked`, `style`, `alt_text`.

Rules to specify before coding: rotated hit geometry, group transforms, lock
inheritance, frame containment versus grouping, group cycles (rejected),
endpoint deletion (connector becomes a free point and the change is reported),
copy/paste ID remapping, connector re-routing when topic layout changes, and
conversion between a card (topic) and a plain shape (explicit command, lossy
parts reported).

### 5.3 One scene for drawing, selection and export

Rendering today paints fixed layers (zones, boundaries, branch lines, summaries,
topics, relationships), and `Layout::hit_test` only knows topics. Add a derived
**Scene** (`thinoda-core`) built from layout + canvas layer:

- `ScenePrimitive { id: SceneId, band, order, geometry, paint }`
- Bands, bottom to top: `Background` (grid) < `Frames` (zones) < `BelowMap`
  (shapes and ink sent behind) < `Map` (existing tree layers, unchanged
  internal order) < `AboveMap` (shapes, ink, connectors by default) < `Overlay`
  (selection, guides). Z-order commands move objects within a band or between
  `BelowMap` and `AboveMap`; they never interleave with tree topics.
- The same scene feeds painting, hit-testing (top-most first), lasso and
  rubber-band selection, bounds and zoom-to-fit, the accessibility tree, SVG/PNG/
  PDF/PPTX export, print and scenes.

### 5.4 Ink, undo and performance

- Raw pointer samples are buffered in the canvas widget, outside `Document`.
  On pointer-up the stroke is simplified with Ramer–Douglas–Peucker, validated
  and committed as **one** edit. Smoothing is available in the input pipeline
  but is not enabled by default.
- Stroke points are stored as shared immutable data (`Arc<[Point]>`), so the
  snapshot undo copies references instead of point arrays. If benchmarks still
  fail, canvas edits get delta undo.
- Undo gets a byte budget in addition to the 300-step limit.
- Hard limits (validated on load and paste): objects per sheet, points per
  stroke, total points, coordinate and size ranges, label length, group depth,
  decoded image size.
- Rendering culls off-screen strokes and caches bounded, screen-space
  tessellated meshes keyed by geometry, transform, clip, style and
  tessellation settings. Changes such as panning invalidate affected entries.
  Each paint protects currently visible meshes in both object bands before
  admitting misses; visible strokes beyond the budget still retessellate.
  The cache is not a guarantee of the frame-time target.
- F0 benchmark gate: 1,000 and 10,000 strokes, plus a 50-image board, on native
  and web: frame time, memory, undo memory, save/load time, recovery journal
  size.

#### F0 benchmark report (native, CPU only)

`cargo run --release -p thinoda-core --example board_benchmark` (Windows x86_64,
release, 20 samples, p50 / p95 in ms). Each board has a hidden root, 64 points
per stroke and 50 image-card placeholders without embedded bytes. This baseline
does not measure realistic image memory, upload or recovery. The scene row
covers layout plus scene build. Undo is 300 undoable object moves through
`Document`, including the snapshot, validation and trimming. Hit-test is 1,000
queries.

| Strokes | Scene | Hit-test ×1000 | Undo push ×300 | Serialize | Deserialize | JSON | Snapshot estimate | History after 300 |
|---|---|---|---|---|---|---|---|---|
| 1,000 | 0.22 / 0.27 | 0.52 / 0.60 | 59 / 62 | 2.9 / 3.9 | 3.9 / 4.1 | 2.1 MB | 0.21 MB | 63 MB |
| 10,000 | 2.1 / 2.5 | 4.8 / 8.6 | 620 / 841 | 32 / 39 | 43 / 63 | 20.5 MB | 1.9 MB | 267 MB (budget-trimmed) |

The estimate counts ink point data once, because it is shared by `Arc`. Each
snapshot then costs about 0.2 KB per stroke. At 10,000 strokes the
256 MiB byte budget, not the 300-step limit, decides how much history is kept
(about 140 steps). A single edit costs about 2–3 ms; the sheet clone plus full
validation dominate that cost. Delta undo for canvas edits stays the fallback if
F5 shows this is too slow. Web and GPU frame-time measurements belong to F5.

#### F5 realistic-image baseline

A separate pre-integration release harness measured 20 samples each on Windows
x64 and in Chromium/Wasm. Its boards contained 0, 1,000 or 10,000 strokes plus
50 distinct JPEG crops totalling 7.7 MB; these were real embedded resources,
unlike the F0 placeholders. Times below are p50 / p95 milliseconds.

| Environment | Strokes | Overview paint + tessellation | Tessellation alone | Archive load/decode |
|---|---:|---:|---:|---:|
| Windows | 1,000 | 2.96 / 3.54 | 2.53 / 3.14 | 98.87 / 101.38 |
| Windows | 10,000 | 38.74 / 40.43 | 30.06 / 30.51 | 155.56 / 159.50 |
| Chromium/Wasm | 1,000 | 2.9 / 3.0 | 2.2 / 2.3 | 4,659.3 / 5,137.6 |
| Chromium/Wasm | 10,000 | 40.4 / 42.4 | 27.7 / 29.0 | 4,725.4 / 4,737.8 |

These are CPU paint and tessellation measurements, not GPU presentation or
input-to-frame latency. Wasm decode is in memory and excludes the file picker.
A later diagnostic found that enabling Chrome's debugger slowed the same Wasm
SHA workload from 16.6 ms to 170.9 ms, returning to 21.2 ms after disabling it.
An isolated three-sample comparison of the pre-optimization 1,000-stroke
archive import measured medians of 385.7 ms with the debugger off, 4,709.1 ms
with it enabled, and 426.1 ms after disabling it again. The diagnostic used
`Runtime.evaluate`, excluded the picker and rendering, and is not a percentile
or a post-optimization result. The earlier baseline's debugger state was not
recorded, so its cause cannot be established from this comparison.
The photo fixture consists of 50 crops of one
[public-domain NASA image](https://svs.gsfc.nasa.gov/30613). The browser
recovery and synchronous VFS writes failed on localStorage quota; this baseline
does not establish recovery or end-to-end file-flow acceptance. At the time of
this baseline, redo and embedded image buffers were not counted in the history
budget.

#### Post-merge ink-cache experiment

A smaller canvas/core-only Windows release harness repeated the original
50-image paint workload for 20 warm samples per run. It excludes archive,
storage, recovery and undo, and its dependency features differ from the
earlier full harness, so compare only these runs. With 1,000 strokes, combined
paint and tessellation was 3.05 / 3.54 ms p50 / p95 before the candidate and
4.27 / 5.14 ms afterward. With 10,000 strokes, it was 56.83 / 63.07 ms before
and 112.53 / 379.33 ms afterward; a candidate repeat was 65.93 / 73.30 ms.

The candidate reduced the 10,000-stroke paint component but not the combined
measurement, and the samples were noisy and not interleaved. It was therefore
not applied. These headless CPU timings are not GPU frame times, input latency
or iPad results. Windows application control also blocked build scripts in the
separate experimental target directory, so focused invalidation and
memory-bound tests for that candidate remain unverified. A quiet interleaved
comparison and permitted test environment are still required before
reconsidering it.

#### Shared ink-point cache checkpoint

The mesh cache now shares the stroke's immutable point allocation rather than
copying it. It compares unchanged allocations by identity and still compares
values when allocations differ; copy-on-write edits, style and transform changes
continue to invalidate the mesh. The cache budget conservatively counts the
shared points because the cache can retain them after a document closes.

On Windows, the same canvas/core-only, 50-JPEG harness used above ran two
interleaved baseline/candidate pairs with 20 warm samples per run. Overview
paint plus tessellation p50 was 2.59/3.41 ms baseline versus 2.98/3.00 ms
candidate for 1,000 strokes, and 57.00/56.26 ms versus 58.61/55.83 ms for
10,000. After-cold process private commit for 10,000 was 129.69/128.72 MB
baseline versus 126.07/125.51 MB candidate. This is a whole-process
measurement, not an isolated cache allocation. It supports a modest memory
reduction but does not demonstrate a frame-time improvement or meet the F5
performance gate; GPU, browser and physical-device measurements remain open.

#### Visible ink-cache admission checkpoint: critique

The cache now protects visible resident strokes across both scene bands before
admitting new meshes. Misses can evict off-screen entries, but not a visible
entry that will be painted later in the same pass. Empty or alternate views do
not clear the cache merely by rendering. Geometry, color, zoom, pan, clip,
pixel scale and tessellation settings remain part of the validity check, and
the entry and conservative byte budgets remain bounded.
Admission also checks whether a new mesh can fit alongside the protected
entries before evicting any off-screen entries; a rejected large mesh therefore
does not needlessly empty the rest of the cache.

On Windows, three interleaved pairs of the same canvas/core-only, 50-JPEG
release harness used 20 warm samples per run. At 10,000 visible strokes,
overview paint plus tessellation p50 was 51.92–52.99 ms before and
38.13–39.66 ms afterward; p95 was 54.89–55.78 ms and 40.01–43.09 ms.
At 1,000 strokes, p50 was 2.43–2.53 ms before and 2.15–2.35 ms afterward.
This headless CPU result excludes GPU presentation, input latency, recovery
and browser execution. It does not meet the overall F5 frame-time gate or
establish physical-device performance.

In the first 10,000-stroke candidate run, the separate p50 measurements were
8.01 ms for paint and 30.22 ms for tessellation, with approximately 1.92 million
vertices and 7.57 million indices. Tessellation dominates that particular CPU
fixture; this is not a measurement of GPU frame time. Two subsequent interleaved
pairs compared that candidate with the final protected-byte accounting: p50 at
10,000 strokes was 38.34/39.08 ms before and 38.43/37.07 ms afterward; p95 was
39.81/46.83 ms and 52.51/39.27 ms. The noisy samples do not establish an
overhead bound.

Protecting a full visible cache can leave additional visible strokes
uncached; switching to a disjoint view may evict old entries and later require
retessellation. These are deliberate bounded-memory tradeoffs, not lost
strokes. Focused tests cover over-capacity bands, disjoint and empty views,
reused IDs, invalidation, entry/byte limits, and admission that cannot fit even
after off-screen eviction; real multi-window and device workloads remain open.

### 5.5 Input devices

- Mouse and touch are supported drawing inputs, but real-device and
  cross-platform acceptance remain open. Pressure is **optional metadata**:
  egui 0.37 delivers `Touch.force` but no pen identity, tilt, eraser end or
  hover, so drawing must never depend on it.
- Spike before promising pressure: Windows Ink, macOS tablet events, Linux
  libinput, browser Pointer Events (`pressure`, `pointerType = "pen"`) through
  winit/eframe. Palm rejection and "draw with pen only" are out until the spike
  shows reliable pen identification.

### 5.6 Scenes

Extend `SlideSource` with `Viewport { rect }` next to `Branch` and `Zone`.
Scene bounds come from the shared scene, so canvas objects inside the rectangle
are shown, exported to PDF and played in Pitch. Viewport slides are new
semantics and require native v6.

### 5.7 Tools and UI

- Board toolbar (floating, collapsible, all locales): Select, Hand, Sticky,
  Text, Shape (library popover with search), Connector, Pen, Highlighter,
  Eraser, Lasso, Frame, Image. `Esc` returns to Select.
- Use a recognizable action icon for every tool and an expand/collapse icon at
  the top. Expanded rows show icons and localized labels; collapsed rows retain
  the icons, localized tooltips, accessible names, and keyboard access. Bound
  the toolbar to the available canvas height and scroll rather than hiding
  actions on short windows or at high UI scale. The toolbox starts collapsed
  to avoid obscuring canvas content; either state can be changed by the user.
- Show contextual action icons for the active mode. For ink, expose colour,
  width, opacity, and the supported pen/highlighter stroke types; erasers expose
  their radius. These controls remain usable in either toolbar presentation.
- Inspector sections for card colour, shape fill/stroke, ink width/colour/
  opacity, rotation, lock, alt text.
- Keep the working custom Fill-Color Picker and also reuse the existing quick
  styles and Shape/Text controls for selected sticky notes, text cards and
  shapes. Apply the relevant visual properties to each compatible object
  without replacing its content, geometry or kind.
  Mixed selections and locked groups must follow the existing atomic edit
  rules; unsupported properties must not appear to have been applied. Changes
  must survive native save/reopen, undo/redo and visual export without changing
  ordinary mind-map styling.
  Cards support fill and text colours; shapes support fill, border and label
  colours, while shape typography is not editable. Quick Styles also apply a
  supported border width to shapes. Locked groups and unsupported items are
  skipped, and each action is a single undo step.
- Keyboard parity: create objects at the viewport centre, move with arrows
  (Shift = larger steps), resize/rotate with modifiers, cycle z-order, group/
  lock commands, Tab order = reading order (frames, then top-to-bottom,
  left-to-right), screen-reader announcements for objects and connector ends.
- Light/Dark/high-contrast palettes for sticky colours; reduced motion respected.
- Text in cards and shapes supports IME and spellcheck like topic titles.

### 5.8 File format and exports

- One function `required_native_version(workbook)` replaces version decisions
  that are spread across `formats.rs` and `native_resources.rs` today.
- **Native v6** is written only when the workbook uses any new meaning that an
  older reader would lose: canvas objects, card presentation, Hidden root mode,
  viewport scenes, connector endpoints, frame clipping. Older Thinoda versions
  already reject versions newer than they know, so they refuse instead of
  silently dropping data on re-save. Plain maps keep writing their current
  version.
- Readers, writers, resource hydration, recovery journal, complete-sheet
  clipboard, merge/import validation and downgrade tests all follow the same
  rule.
- **Export preflight** gets one central capability analysis for every format.
  Reports list counts and types of dropped objects, hidden-root projection,
  omitted or synthesized roots, unsupported rotation/opacity, connector
  endpoints converted to relationships, viewport cropping and lost resources.

| Format | Board content |
|---|---|
| Thinoda native | Everything (v6 when needed) |
| SVG, PNG, JPEG, PDF, print | Visible scene with frame clipping, subject to the limitations below |
| PPTX | Viewport scenes are rasterized; branch/zone slides remain editable where supported |
| XMind | Cards as floating topics; shapes/ink/connectors reported as lost, or refused if the user selects strict mode |
| FreeMind, OPML, Markdown, text, DOCX, XLSX, CSV, MindManager, TextBundle | Outline projection of cards and topics; drawn objects reported as lost |
| FIM | Unchanged; board content reported as lost |

#### F0-IO status (implemented in `thinoda-io`)

- `board_export::required_native_version` is the single version rule
  (2 base, 3 resources, 4 formatted notes, 5 FIM, 6 board semantics). The
  native writer and resource packaging use it; the reader accepts up to v6 and
  rejects board data in manifests that declare less than v6. Plain maps keep
  their previous version and identical bytes (golden tests).
- `export_losses(workbook, format, options)` returns itemised `ExportLoss`
  values (shapes, ink, connectors, cards converted, rotation, viewport slides,
  zones, floating topics, synthesized root, zone clip, board settings). A
  shared preflight reports relevant losses before save, export and print;
  viewport slides flattened in PowerPoint are reported separately.
- Outline formats export `project_to_outline` (synthesized root, no anchor).
- SVG/PNG/JPEG/PDF/print and editable PPTX paint the scene: coloured zones,
  Below objects, map, Above objects, styled cards, shapes, ink (highlighter
  alpha), connectors with arrowheads and labels. Export bounds include canvas
  objects; the grid is not exported.
- Visual frame clipping is applied by the canvas and SVG-backed exports.
  Map connectors use the child topic's clip, boundaries the parent topic's
  clip and summaries the summary topic's clip. Relationships are clipped only
  when both endpoints belong to the same clipped frame; cross-frame
  relationships remain visible even if two frames have identical bounds.
  Clipping is visual and is **not secure redaction**.
- Board-semantic SVG summaries include the tip-to-topic connector; plain
  legacy maps retain the historical summary path for byte compatibility.
- Viewport scenes can be captured, edited, played and exported. PowerPoint
  viewport slides are flattened to images; editable branch/zone slides do not
  fully preserve frame clipping and the preflight reports that loss.
- XMind titles a Hidden root with the sheet title; imports run board
  validation and reject malformed or oversized data without panicking.
- Known gaps: ink pressure is ignored; card rotation and rotated shape labels
  stay upright in editable PPTX; the XMind central topic keeps the anchor's
  non-title fields.

## 6. Phased plan

Each phase ends with its own rubber-duck review, tests for native and web, all
locales, Light/Dark, accessibility checks and updated user docs.

| Phase | Content | Acceptance |
|---|---|---|
| **F0 Groundwork** | Root query APIs (5.1) and migration of all root callers; derived Scene with bands; typed IDs and `bump_next_id` coverage; limits and validation; `required_native_version`; central export capability analysis; ink/undo benchmark | No behaviour change for existing maps (full test suite, golden exports unchanged); contract tests prove the anchor never appears in render, search, outline, AI, a11y, statistics, export or presentation; benchmark report checked in |
| **F1 Flexible roots** | `SheetRootMode::Hidden`, multi-root via floating trees and zones, mode switch commands, export projection policies | Rootless and multi-root sheets round-trip in native v6; every export shows the projection; v5 readers refuse v6 files |
| **F2 Cards and frames** | Card presentation for floating topics (sticky, text, image), zone frames, grid, snapping and guides, align/distribute, responsive icon toolbox, existing quick styles and Shape/Text colours | Sticky wall usable by mouse, touch and keyboard; existing style controls work for selected cards; toolbox actions remain accessible in expanded and icon-only states at short heights and high UI scale; screen-reader labels; SVG/PDF show cards and frames |
| **F3 Shapes and connectors** | Shape library (basic + flowchart), existing quick styles and Shape/Text colours, connectors with typed endpoints, z-order within bands, groups, lock, rotation | Flowchart can be built, restyled and exported to SVG/PDF; endpoint deletion behaves as specified; undo of group operations |
| **F4 Board templates** (implemented) | Data-driven template definitions and the template MVP (section 7) | Templates localised in all UI language catalogs, theme-aware, validated in tests |
| **F5 Ink** | Pen, highlighter, object eraser, lasso, simplification, caches, mode-specific ink controls | Colour, width, opacity and pen/highlighter type are available in either toolbox state; benchmark gate from F0 met on native and web; no pressure dependency |
| **F6 Scenes and polish** | Viewport scenes in Pitch and PDF, mini-map, pressure (if the spike succeeds), pixel eraser, more templates | Scenes play and export; pressure optional |

**Current implementation status:** The F0 core model and IO, F4 templates,
export preflight, board alignment guides, zoom-to-selection, responsive toolbox,
frame-identity clipping fix and pending-ink host safeguards have focused
regression coverage. Board objects support Quick Styles, the existing Shape
and Text controls, and the custom Fill-Color Picker; focused styling and
undo coverage is in place. F1/F2 and F3 remain open for broader integration
acceptance; F5 remains open for the realistic performance gate; F6 remains
open for final scene/export acceptance. Local browser large-board storage and
recovery, complete history accounting, and cross-platform verification remain
open. The final production candidate passed 46 headless Chromium cases,
including mixed-board editing, mini-map input, SVG/PNG/PDF/PowerPoint export,
browser-language selection and browser recovery failures. Eleven focused
startup, retry and offline checks passed in each of desktop WebKit and Firefox on Windows; an
earlier build with identical Wasm also passed focused touch checks there.
Cloud-provider code is merged upstream behind a default-off feature and stays
disabled until publisher registrations are in place; see
[CLOUD-STORAGE.md](CLOUD-STORAGE.md).

The production browser accessibility tree currently exposes the egui canvas
and an unlabeled IME input, but not the board controls or their egui
accessibility names. Focused egui accessibility tests therefore do not close
the browser screen-reader gate. The observed gap and required follow-up are
recorded in [WEB-STARTUP.md](WEB-STARTUP.md#browser-accessibility-checkpoint).

### F2 image-card accessibility checkpoint: critique

- Image cards now retain a localized type in their accessible name even when
  their title is changed. Focus, selection, renaming and localized labels have
  focused egui coverage.
- A card title is not a substitute for an image description. The browser
  accessibility bridge and real screen-reader acceptance remain open.

### History resource-accounting checkpoint: critique

- Retained undo and redo snapshots now account for distinct shared ink and
  embedded-resource allocations once, as well as an estimate of owned metadata.
  Undoable workbook reloads include workbook metadata and share resource, ink
  and pressure allocations with sheet snapshots; previews retain the source
  history budget without inheriting history. Focused tests cover these paths,
  shared and replaced resources, equal-content distinct allocations, redo
  invalidation, cloning and budget trimming.
- The budget is an approximation, not a process-memory cap. The nearest undo and
  redo entries are deliberately kept even if they exceed it; live-document and
  rendering allocations are outside this history estimate. Allocator overhead,
  spare capacity and some nested non-board data, including FIM trees, are not
  fully estimated either.
- This checkpoint does not establish large-board browser recovery, cloud reload,
  or a new performance baseline. Those remain separate acceptance work.

### F1/F2 core checkpoint: critique

- Hiding a central topic must preserve even an untitled connector endpoint and
  retain legacy hidden-anchor titles without exposing them as board content.
  Selection, search, outline, and masking use content-aware traversal.
- New empty frames survive edits and undo; older empty zones retain their
  original deletion semantics. Scene clipping is shared by hit testing and
  visible bounds, including detached summary trees.
- Locked canvas objects must reject edits and arrangement as well as movement
  and deletion; otherwise a locked group could still be modified indirectly.
- This is not phase acceptance: application controls, frame clipping in visual
  export, keyboard/screen-reader parity, and integration tests remain open.

### Core integration checkpoint: critique

- Mixed card/object groups use one selection and lock invariant. A locked member
  blocks indirect edits to the rest of its group, including card presentation,
  object geometry, stacking and text replacement.
- Board clipboard payloads remap internal references and resources, convert
  external connector ends to their resolved positions, and validate before
  mutation. Plain-text clipboard ownership and host integration still require
  end-to-end verification.
- Connector-to-connector geometry is resolved iteratively; cycles are rejected.
  Allocating after an imported maximum identifier finds an unused ID instead of
  overflowing. Invalid paste and oversized content must fail atomically.
- Undo accounting tracks shared ink allocations incrementally rather than
  rescanning the entire history. A three-sample Windows release check measured
  624 ms p50 for 300 moves on the 10,000-stroke case (the earlier repeated-scan
  implementation took approximately 6.1 s). This is a CPU microbenchmark, not
  a web, GPU, interaction-frame or recovery-size acceptance result.
- This checkpoint does not complete the phases: board controls, export clipping,
  browser/native interaction, and cloud reload history still need integration
  and validation.

### Toolbox checkpoint: critique

- Icons are bundled with the application and use the shared command-button focus,
  selected-state and accessible-name behavior. Icon-only controls retain their
  localized tooltips and keyboard activation.
- The toolbar uses available canvas height before scrolling. Very short canvases
  reduce row height so the pinned toggle does not make the remaining actions
  unreachable; this is a fallback for constrained space, not a new default
  touch-target size.
- Ink and eraser controls are shown for the active mode in either presentation.
  Switching to one of these modes reveals its controls even if the tools were
  previously scrolled; the toolbox starts collapsed so that board content is
  not hidden by default. Board fit reserves room for the compact toolbox. A
  selected-topic toolbar is repositioned away from the toolbox and frame
  titles, or omitted when it cannot fit, where the normal menus remain
  available.
- Light/dark desktop previews and focused interaction tests cover these states.
  Browser, screen-reader and cross-platform acceptance remain part of the full
  integration, not evidence that the whole phase is complete.

### F3 geometry checkpoint: critique

- Mixed groups preserve relative positions through rotation and undo. Locked
  members prevent indirect group edits; connectors attached outside a rotated
  group remain attached, although their routed paths can reflow.
- Frames are identified by their zone IDs, not their rectangles. Two distinct
  frames with identical bounds must not accidentally clip a relationship as if
  both ends belonged to one frame.

### F4 templates checkpoint: critique

- Eight board templates use the same validated primitives and translated
  creation-time strings. The additional layouts do not imply structured
  Kanban, table or collaboration semantics.

### F5 ink checkpoint: critique

- Ink gestures commit atomically, and pending or rejected gestures are exposed
  for retry or cancellation rather than silently discarded. Pending ink is not
  crash-recoverable until it is committed.
- Pixel erasing can split strokes while retaining their metadata and undo
  behavior; pressure remains optional and does not affect rendering.
- Touch de-duplication now distinguishes a mouse press owned by ink from a
  non-ink synthetic press. This keeps a single-finger context menu open after
  release without forwarding a canceled ink gesture; multi-touch cancellation
  and subsequent clicks remain covered by automated regressions. Real touch
  device acceptance remains open.

### F6 presentation checkpoint: critique

- Viewport presentations and the mini-map reuse scene geometry. Visual clipping
  is not redaction, and editable PowerPoint output reports clipping it cannot
  preserve. Browser storage limits and realistic rendering performance still
  require integration acceptance.
- The Pitch stage now fits its containing player rather than the height of the
  browser window. An automated production-browser check captures a rootless
  board containing both a sticky and a floating tree and verifies that both
  appear within the player. Browser SVG, PNG and PDF regressions inspect the
  downloads after consent and verify rendered board content and source
  preservation. A separate PowerPoint regression checks consent, cancellation,
  the flattened viewport image, its board content and the unchanged source.
  These do not substitute for the remaining format, physical-device and
  cross-platform presentation acceptance.
- A production-browser regression checks that the mini-map pans without
  creating a shape underneath it and that hiding the mini-map restores canvas
  input. It passed in desktop Chromium, Firefox and WebKit at one viewport;
  physical touch and screen-reader acceptance remain open.

### Integrated editing checkpoint: critique

- Topic/card and shape/connector label editors process queued text before an
  Enter or Tab commit. Escape retains each editor's existing behavior: it
  commits topic/card titles and cancels shape/connector labels. IME candidate
  keys do not commit, and failed commits retain the draft instead of creating a
  child. Focused same-frame and undo regressions cover these paths.
- A production-browser workflow saves and inspects a rootless board with a
  sticky, styled shape, labeled connector, frame and ink, then undoes the ink.
  The picker used by browser automation is not a real OS picker, and this
  workflow does not close screen-reader or physical-device acceptance.
- Browser input can batch a canceled touch and a replacement contact into one
  frame. Gesture routing now separates the canceled and replacement events,
  preserves interrupted ink for retry or explicit discard, and keeps
  multi-touch and in-progress label drafts protected. Focused canvas tests
  cover the ordered events and duplicate touch/pointer reports; simulated
  input is not physical-device acceptance.
- Both clicks and drags retain the document, revision and camera guard captured
  at pointer-down. A click is canceled rather than creating an object on a
  different view or a replacement document with the same revision. Focused
  regressions exercise both stale-click cases.
- In-memory document identities distinguish new documents and clones even if a
  sheet allocation is reused. Creation, shape/connector-label, pending
  connector, rotation and ink ownership also retain their sheet checks; ordinary edits,
  undo/redo and undoable reload keep the document identity. Deterministic
  address-collision tests and repeated-replacement regressions cover the
  distinction. These identifiers are runtime-only, not document data.

### Recovery safety checkpoint: critique

- An incomplete discovery could previously leave a corrupt snapshot in the
  recovery list while the app's **Discard All** action still deleted it.
  Bulk discard now fails closed after incomplete discovery or a later failed
  restore; **Keep recovery data** dismisses the dialog without deleting the
  snapshots. The actions are covered at narrow and desktop sizes in all
  supported UI languages.
- Keeping data is not a repair or raw-export tool. An unreadable snapshot can
  still require external recovery. An open document retries failed cleanup
  after saving even when its content state has not changed; persistent errors
  or closing first can still leave saved work to be offered again.
- Browser leases remain cooperative and non-atomic; quota failures, large
  snapshots, real devices and concurrent-tab races remain separate acceptance
  work. The asynchronous recovery draft is not integrated.

Not in this package: real-time collaboration (R12), comments with identities,
boolean shape operations, handwriting recognition, tables, board library
management.

## 7. Non-mind-map templates

A new gallery category **Boards**. Templates become data-driven (versioned,
localised strings, theme-aware colours, validated) instead of growing the
hard-coded switch in `templates.rs`.

**Template MVP (F4):**

Status: implemented in `crates/thinoda-core/src/templates/boards.rs` (data:
frames, cards, floating trees, shapes, connectors; one builder) and listed in
the gallery category **Boards** (the existing categories hold the mind-map
templates). All content strings are catalog keys translated at creation time
(`templates::instantiate_localized`); every UI language catalog covers them.
Sticky colours are pastel fills with dark text and frame tints are translucent
mid-tones, so both read on Light and Dark backgrounds. Tests check validation,
determinism, an empty hidden anchor, no overlaps, cards inside their frames and
catalog coverage.

| Template | Built from |
|---|---|
| Blank canvas | Hidden root, dot grid, no content |
| Multi-root map | Hidden root, three independent floating trees (two options and a decision) |
| Brainstorm sticky wall | Heading text card; Ideas, Questions and Parking lot frames with stickies |
| Retrospective (Went well / To improve / Actions) | Three tinted frames with stickies and a heading card |
| Kanban board | To do / Doing / Done frames with cards (cards move between frames by dragging; no column semantics yet) |
| Business Model Canvas | Nine frames in the canonical layout, each with a prompt text card |
| Flowchart | Start/End terminators, process steps, a decision, labelled connectors (Yes/No) on a line grid |
| SWOT board | Four tinted frames with one prompt sticky each |

Canvas shapes, connectors and frame tints are now painted by the editor and
the SVG/PDF scene renderer. Interactive authoring and complete export parity
remain in progress; card colours are also written to the topic fill for
compatibility with the topic renderer.

**Later, each with the capability it waits for:**

| Template | Needs |
|---|---|
| Affinity diagram | Frames + stickies (F2); AI clustering optional |
| Lean Canvas, Empathy map | Frames (F2), text-heavy responsive regions |
| Eisenhower matrix | Frames (F2); Matrix structure exists for tree variants |
| Swimlane | Shapes and connector routing (F3) |
| User journey map, Roadmap | Structured axes or lanes |
| Storyboard, Moodboard | Image cards, scalable resource storage |
| Venn diagram | Shape opacity and intersections |
| Meeting board | Frames, stickies, optional timer |

Existing tree templates stay unchanged.

## 8. Rubber-duck critique

A full rubber-duck review of the first draft (round 1) scored it **2/10, with
blocking issues**. Its main point: the draft was really three large
architecture changes (mixed scene, semantic roots, asynchronous storage)
presented as small additions. Summary and resolutions:

| # | Finding | Severity | Resolution in this document |
|---|---|---|---|
| 1 | A hidden root leaks everywhere: `layout_sheet` always starts at `sheet.root`; text/Markdown/OPML/FreeMind exports traverse from it; presentations need a valid main branch. Hiding it in paint is not enough; multi-root content in `floating` would vanish from outline exports | Blocking | Root query APIs and full migration in F0 before Hidden mode; anchor gets a validated empty role; per-format projection policy; contract tests (5.1, 6) |
| 2 | A separate `Vec<CanvasItem>` duplicates the topic system, and a per-item `z` cannot interleave with fixed tree layers | Blocking | Hybrid model: cards are floating topics, only true primitives are new objects; derived Scene with layer bands (5.2, 5.3) |
| 3 | Snapshot undo clones the whole sheet per edit (300 steps): ink would explode memory; `bump_next_id` misses new IDs | Blocking | Buffered input, one commit per stroke, `Arc` point data, byte-budgeted undo, limits, allocator coverage, F0 benchmark gate (5.4) |
| 4 | Format versions are decided in two places; v6 must be gated on every new meaning, not only on "objects present"; generic loss warnings are not enough | Major | `required_native_version`, central capability analysis, itemised reports (5.8) |
| 5 | 18 board templates are not a plausible first release; many need missing capabilities | Major | Template MVP of four; the rest listed with prerequisites (7) |
| 6 | Pressure via `Touch.force` is not real pen support | Major | Pressure is optional; spike first (5.5) |
| 7 | Reusing Presentation doesn't make scenes free; slides filter a topic-only layout | Major | `SlideSource::Viewport` using scene bounds; v6 (5.6) |
| 8 | Many contracts missing: tab order, screen reader, keyboard creation/rotation, IME/spellcheck, search in objects, resource clean-up, thumbnails, print pagination, AI consent, deterministic save order, high contrast, reduced motion | Major | Added to 5.7, 5.8 and the F-phase acceptance; deterministic ordering = objects sorted by band, order, id |
| 9 | Cloud storage must be a separate program with its own contract | Blocking (for cloud) | Moved to [CLOUD-STORAGE.md](CLOUD-STORAGE.md); not on the freeform path |

**Residual risks** (accepted, tracked):

- The F0 root migration is large (about 160 files). Mitigation: do it as
  mechanical, separately reviewed commits behind unchanged golden exports.
- egui text editing inside rotated cards may need custom work; fall back to
  editing unrotated in place.
- Web performance with many strokes is unknown until the F0 benchmark runs.
- XMind round-trip of boards will always be lossy.

_Last updated: 2026-11-05 11:56:47 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Freeform boards: research, concept and plan

Status: **implementation in progress**. Board data, native format support,
export analysis, templates, and layered canvas rendering are implemented;
editing, accessibility, presentation, and export parity are still being integrated.
Companion document: [CLOUD-STORAGE.md](CLOUD-STORAGE.md) (cloud folder and
storage-provider gap analysis). Roadmap entries live in
[BACKLOG.md](BACKLOG.md#freeform-boards-and-flexible-map-roots) and
[PLAN.md](PLAN.md#r14---freeform-boards-and-flexible-map-roots).

### Zone membership

The **Zone** main menu and board context submenu group **Create Zone**,
**Add to Zone…**, **Remove from Zone**, **Edit Zone**, **Remove Zone**, and
**Manage Zones**. Select floating topics, board objects, or a mixed selection,
then choose **Add to Zone…** and select an existing Zone. Items keep their
positions; membership transfers from their previous Zone. Grouped board items
are included together; locked items cannot be reassigned.

**Remove from Zone** detaches the selected items without deleting or moving
them. Empty Zones remain available for reuse. Select a Zone frame to edit or
remove it; **Remove Zone** keeps all its contents. Membership changes are
undoable as one operation. Adding items does not resize the frame: use
**Edit Zone** to adjust its dimensions if needed.

The model API `Sheet::set_zone_members(id, roots, objects)` replaces complete
membership atomically and retains both target and donor frames when empty.
`Zone.objects` stores explicit object IDs; unassigned objects still use spatial
containment for clipping, while assigned objects use their owning Zone's clipping.
Moving a Zone expands rigid board groups and rejects locked or invalid moves
without changing any contents. Explicit object membership requires native format
v9 so older applications refuse the file rather than silently dropping ownership.

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
| Zones: named frame rectangle plus `roots` of floating trees, frame outline and optional independent fill | `model.rs` `Zone` | Closest existing "frame/area" and multi-root construct; separate fills require native v7 |
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
| Scenes (saved, ordered views; present, print) | Adapt | Viewport scenes are implemented; export and host acceptance remain open (section 5.6) |
| Mini-map / navigator | Fits | Implemented using scene bounds; real-device and screen-reader acceptance remain open |
| Follow a collaborator's view | Out | Needs real-time collaboration (PLAN R12) |

### 4.2 Objects

| Feature | Verdict | Notes |
|---|---|---|
| Sticky notes (colour, size, text, lists) | Fits | Floating topic with a `Card` presentation (5.2) |
| Text boxes | Fits | Floating topic, card presentation without fill |
| Shape library with search | Adapt | Implemented as canvas `Shape` objects with a searchable basic and flowchart library |
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
| Freehand pen with mouse, touch, pen | Adapt | Implemented as canvas `Ink`; real input-device acceptance remains open |
| Pen styles: pen, marker, pencil, fountain pen, watercolour, crayon | Adapt / Later | Pen and highlighter are implemented; textured brushes remain later |
| Width, colour, opacity | Adapt | Implemented per stroke |
| Pressure sensitivity | Later | egui reports `Touch.force` only; no pen type, tilt, eraser or hover. Optional, never required (5.5) |
| Object eraser | Adapt | Implemented; removes whole strokes |
| Pixel eraser (split strokes) | Adapt | Implemented with atomic stroke splitting and undo |
| Lasso selection | Adapt | Implemented with the shared selection model |
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
| Copy and paste style | Fits | Implemented for topics/cards and, within the same object kind, shapes, connectors and ink; locked selections remain protected |
| Find and replace across text | Fits | Implemented for topic titles, shape/connector labels and canvas descriptions; notes and labels are search-only |
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
| Frame / area | **Zone** extended with persistent empty frames, `clip`, automatic accent outline and optional independent background | Zones already own floating trees and are Pitch sources; freeform frames can also start empty |
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
  The cache is not a guarantee of an interactive frame-time budget.
- F0 benchmark gate: 1,000 and 10,000 strokes, plus a 50-image board, on native
  and web: frame time, memory, undo memory, save/load time, recovery journal
  size.

The original gate does not specify numerical frame-time or input-latency budgets
or representative hardware. These must be defined before a measurement can
establish a pass or failure; the recorded CPU results alone do not close F5.

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
reduction but does not demonstrate a frame-time improvement or establish that
F5 performance acceptance is met; GPU, browser and physical-device
measurements remain open.

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
and browser execution. It does not establish that the overall F5 performance
gate is met or establish physical-device performance.

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

#### Indexed board accessibility registration: critique

The board's accessibility pass used repeated linear lookups for each scene
item, label and group lock. It now builds per-pass indexes and keeps the
sorted scene-item references, while retaining every accessibility target and
the existing reading order. Group mobility accounts for direct member locks;
selection indexes are refreshed when focus changes the selection during the
same pass. Workbooks with duplicate object, zone, card-state, floating-topic or
group IDs use the previous mobility and scene-lookup paths so host-constructed,
unvalidated sheets retain those first-match semantics. This is not a general
guarantee for arbitrary malformed sheets.

On Windows, a release-mode canvas test ran two interleaved baseline/candidate
pairs, with three warmups and 20 measured CPU-side `egui::Context::run_ui`
passes per run. Each hidden-root board had 64 points per stroke and 50
image-card placeholders, without image bytes. Values are ranges of per-run
p50 / p95 milliseconds, not combined percentiles:

| Strokes | AccessKit | Baseline p50 / p95 | Indexed p50 / p95 |
|---|---|---|---|
| 1,000 | off | 1.175–1.198 / 1.226–1.260 | 0.568–0.582 / 0.636–0.861 |
| 1,000 | on | 1.646–1.699 / 1.868–1.910 | 1.059–1.096 / 1.180–1.330 |
| 10,000 | off | 92.803–93.860 / 95.685–95.787 | 11.186–11.421 / 13.024–13.523 |
| 10,000 | on | 100.242–102.553 / 103.899–106.436 | 13.765–14.053 / 15.410–15.902 |

A separate production-Wasm comparison used headless Chromium 156 on
SwiftShader, fresh isolated contexts and synthetic alternating wheel events.
It validated each saved board before measuring 70 animation-frame intervals
after warmup, with two interleaved pairs per fixture. For 1,000 strokes,
baseline p50 / p95 was 50.2–50.6 / 57.8–60.0 ms and indexed was
50.5–51.2 / 56.4–61.4 ms; this comparison does not establish an improvement.
For 10,000 strokes, baseline was 269.4–274.4 / 297.1–315.0 ms and indexed
was 184.2–185.5 / 201.2–212.5 ms. These are animation-frame intervals under a
software renderer, not hardware-GPU presentation or input-to-display latency.
A screenshot hash change is only a sanity check, not proof of panning.

The index is rebuilt each pass and uses additional temporary memory; it does not
hide off-screen targets or solve the missing browser accessibility bridge.
The duplicate-ID fallback deliberately retains the slower path. Focused
coverage compares direct and mixed-group locks, descriptions, duplicate
first-match behavior and same-frame focus/selection. The full canvas suite,
targeted app/style/export tests and production-browser board regressions passed;
screen-reader, realistic embedded-image, memory, recovery, hardware-GPU and
physical-device acceptance remain open. The original F0/F5 performance gate
still lacks numerical budgets and representative devices, so it is not closed.
Detailed samples and artifact hashes are in
[board-accessibility-index-2026-11-03.json](benchmarks/board-accessibility-index-2026-11-03.json).

#### Embedded ink/image fixture checkpoint: critique

The browser benchmark can now opt into 50 distinct synthetic 256 × 192 PNG
resources on the existing 1,000- and 10,000-stroke boards. The measured fixture
contained 1,340,248 encoded image bytes; 9,830,400 bytes of RGBA pixels is an
arithmetic estimate, not a memory measurement. Default placeholder archives
remain byte-for-byte identical to the committed generator. After browser save,
the fixture checks every resource's ID, bytes and metadata, image-card
references and layout, and each stroke's ordered ID, f32 geometry, pressure
and style. Negative tests reject changed resources and stroke data.

Four browser checks passed separately in headless Chromium, Firefox and WebKit:
both placeholder and embedded fixtures at each stroke count loaded and saved.
The browser independently decoded all 50 saved PNGs, and a screenshot marker
showed that some image pixels appeared on the board. Neither check proves that
Thinoda painted every image. Browser recovery, memory, accessibility and
physical input were not measured.

As a harness smoke test, two interleaved pairs used the **same** release build
for both URLs, headless Chromium 156 and SwiftShader. Each run recorded 70
animation-frame intervals after warmup while dispatching synthetic wheel
events. For the embedded fixtures, per-run p50/p95 ranges were 53.7–60.9 /
61.0–73.1 ms at 1,000 strokes and 204.8–248.2 / 229.5–272.5 ms at 10,000.
These noisy same-build samples do not demonstrate an improvement, GPU
presentation time, input latency or a pass against the undefined F5 budgets.
The screenshots, browser decoding and saved archive checks have different
scopes; none substitutes for representative photos, larger image payloads,
hardware-GPU tests or real-device acceptance.

The tested Wasm SHA-256 was
`e862f968ddac41465649d119a5a2313deb65f19e1b1406f82ce5e41a0d6cfc46`.
It was built from uncommitted User Journey Map work atop `9c4bacc`, later
committed as `f2ab3f6`; an exact match to that commit was not established.
Raw [embedded](benchmarks/board-embedded-smoke-2026-11-04.json) and
[placeholder](benchmarks/board-placeholder-smoke-2026-11-04.json) samples
retain the fixture hashes and limitations.

For a repeat, the ignored native test is
`cargo test --release -p thinoda-canvas --lib measure_large_board_canvas_pass -- --ignored --nocapture`.
Generate browser fixtures with
`python crates\thinoda-web\tests\benchmarks\make-board-fixtures.py crates\thinoda-web\tests\fixtures\affinity-diagram.thinoda <fixture-directory>`,
then serve baseline and candidate release builds locally and run
`node crates\thinoda-web\tests\benchmarks\compare-board-frames.cjs <fixture-directory> <baseline-url> <candidate-url> <output-json>`.
Add `--embedded-images` to both commands for embedded resources. To run the
separate browser round-trip checks against a locally served release build, set
`THINODA_WEB_URL` to its loopback URL and `THINODA_BENCH_BROWSER` to `chromium`,
`firefox` or `webkit`, then run
`node crates\thinoda-web\node_modules\@playwright\test\cli.js test --config crates\thinoda-web\tests\benchmarks\board-embedded.playwright.config.cjs`.
The fixture unit and compatibility tests also run in CI. The browser harnesses
block service workers and cross-origin requests. Use matching toolchains, a
quiet machine and the same benchmark test in both native builds; neither
harness replaces device or assistive-technology testing.

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
  Colour swatches and graphical width presets complement the exact controls;
  swatches preserve colour alpha and do not change the independent opacity.
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
- Editable PowerPoint cards preserve rotation for their shapes, text,
  annotations, pictures and paths. Shape labels rotate about their parent
  shape's center in the canvas and SVG-backed and editable PowerPoint exports.
  Canvas card contents share their card's rotation, including image and
  annotation rendering. Shape-label and card-title editors stay upright at
  their rotated positions for usable text input.
- XMind titles a Hidden root with the sheet title; imports run board
  validation and reject malformed or oversized data without panicking.
- Known gaps: ink pressure is ignored, and the XMind central topic keeps the
  anchor's non-title fields.

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
open. The production candidate at source checkpoint `03cbac1` passed 56
headless Chromium cases, including
mixed-board editing, mini-map input, SVG/PNG/JPEG/PDF/PowerPoint export,
image-card export, browser-language selection and browser recovery failures.
The same source passed the full local native `cargo xtask ci` with live AI
disabled and tests serialized. Seventeen board and image-card scenarios,
eleven startup/offline checks and three raw recovery checks passed on that
production bundle in each of desktop WebKit and Firefox on Windows. The
real browser-quota recovery check passed in all three engines. PDF rendering
checks use a separate Chromium viewer.
Cloud-provider code is merged upstream behind a default-off feature and stays
disabled until publisher registrations are in place; see
[CLOUD-STORAGE.md](CLOUD-STORAGE.md).

The production browser accessibility tree currently exposes the egui canvas
and an unlabeled IME input, but not the board controls or their egui
accessibility names. Focused egui accessibility tests therefore do not close
the browser screen-reader gate. The observed gap and required follow-up are
recorded in [WEB-STARTUP.md](WEB-STARTUP.md#browser-accessibility-checkpoint).

### Browser focus probe checkpoint: critique

- An isolated synthetic probe can forward a simple Tab into one semantic DOM
  button and return to one text editor. It is not connected to Thinoda or real
  document content.
- Mixed input can strand focus, queued actions can drop immediate text, and
  text selection or an interrupted synthetic IME composition can be altered.
  Rapid pointer return can also select and replace a word. The
  [browser characterization](WEB-STARTUP.md#isolated-accesskit-feasibility-probe)
  documents the exact event ordering and its limits.
- No production bridge should ship until the focus protocol preserves text and
  selection through same-frame input, stale targets and composition, and real
  browsers, physical devices and screen readers have been checked.

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
  mutation. Real system-clipboard ownership and host integration still require
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

### Browser clipboard checkpoint: critique

- Production-browser regressions use synthetic clipboard events and an isolated
  in-page Clipboard API to exercise a mixed rootless board. They verify rich
  copy/paste, connector endpoint remapping, embedded-image bytes, cross-document
  paste and undo, including two distinct image resources through undo and redo.
  Replacing the clipboard with identical plain text must not resurrect rich
  objects, and a rejected cut write must leave the board intact. All twelve
  cases passed across desktop Chromium, Firefox and WebKit.
- The virtual clipboard deliberately does not touch the user's system clipboard.
  These checks do not establish trusted keyboard-event handling, real clipboard
  permissions or sanitization, operating-system ownership, cross-application
  interchange, or screen-reader behavior; those host checks remain open.

### Resource-union checkpoint: critique

- Merging embedded resources previously revalidated and rehashed the growing
  destination for every incoming item. The union now validates the original
  store once, checks each incoming resource, and maintains a bounded byte total
  while constructing a private clone. Public single-item insertion still
  validates the current store; mutable metadata and public store entries are
  never covered by a cached validation claim.
- Legacy floating-subtree paste validates every source store, including its
  digest keys, then merges their resources in source order. The first existing
  metadata wins on deduplication. Malformed input, entry/byte-limit failures
  and missing references remain atomic, and both board and ordinary topic
  paste retain embedded bytes and one-step undo/redo. For payloads containing
  multiple independent errors, the first reported error can differ because
  subtree validation now precedes the aggregate union.
- Core and I/O suites, focused board UI tests, native and Wasm strict lint,
  a production web build and the three-engine clipboard regressions passed.
  Some existing I/O tests remain ignored; the browser clipboard is synthetic.
- A release Wasm harness compared the previous and revised implementation in
  desktop browsers on Windows. The same 50 distinct JPEG resources totalled
  7,677,622 bytes. After two warmups, eight pairs per case were interleaved;
  results were checked outside the timed interval. The partial store already
  contained 25 of the resources. Median times in milliseconds were:

  | Browser engine | Empty union, before / after | Partial union, before / after | Archive import, before / after |
  |---|---:|---:|---:|
  | Chromium | 352.45 / 13.75 | 452.45 / 20.60 | 46.95 / 45.00 |
  | Firefox | 349.50 / 13.00 | 447.00 / 21.00 | 46.50 / 46.50 |
  | WebKit | 534.50 / 21.00 | 659.50 / 30.50 | 56.00 / 56.00 |

  The rounded samples, fixture and artifact hashes, engine versions and
  methodology are retained in
  [the benchmark record](benchmarks/resource-union-wasm-2026-11-03.json).
  This measures the in-memory resource operation, not a complete paste or
  first-start experience. Archive-import timing is essentially unchanged;
  these results do not close the realistic F5 performance gate or establish
  GPU, native-host, real-clipboard, screen-reader or physical-iPad acceptance.

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
  selected-topic toolbar avoids visible cards, frame titles and the board and
  editing toolbars, or is omitted when it cannot fit, where the normal menus
  remain available.
- Light/dark desktop previews and focused interaction tests cover these states.
  Browser, screen-reader and cross-platform acceptance remain part of the full
  integration, not evidence that the whole phase is complete.

### Selected-topic placement checkpoint: critique

- Placement measures the toolbar before painting it and searches both axes
  against visible card bounds, including rotation and frame clipping, measured
  multiline frame titles and the other board toolbars. A sweep with a range
  coverage tree avoids a quadratic candidate-by-obstacle scan; the fast path
  keeps the preferred position when it is clear. A stale size is clipped and
  made non-interactive until reflow. Ordinary mind maps retain their prior
  anchor behavior.
- If no safe rectangle exists, the selected topic remains selected and its
  existing menu actions remain usable; the toolbar returns when space does.
  The offline guide explains this fallback in all supported languages. The
  search intentionally does not treat freeform shapes, ink, connectors or
  unrelated ordinary topics as obstacles, so they can still be covered.
- Native regressions exercise first paint, both toolbox states, DPI scaling,
  rotated and clipped cards, title measurement, resizing, toolbar and menu
  activation, a 10,000-obstacle search and 2,000 small exhaustive comparisons.
  A production-browser screenshot regression fails on the previous build and
  passes in desktop Chromium, Firefox and WebKit; it checks note visibility and
  a saved command-palette action. These synthetic checks do not establish
  frame-time or input-latency budgets, browser accessibility, real iPad
  behavior or visual acceptance on other native platforms.

### F3 geometry checkpoint: critique

- Mixed groups preserve relative positions through rotation and undo. Locked
  members prevent indirect group edits; connectors attached outside a rotated
  group remain attached, although their routed paths can reflow.
- Frames are identified by their zone IDs, not their rectangles. Two distinct
  frames with identical bounds must not accidentally clip a relationship as if
  both ends belonged to one frame.

### F4 templates checkpoint: critique

- Seventeen board templates use the same validated primitives and translated
  creation-time strings. Affinity and Meeting Board notes can be moved among
  persistent visual frames, the Eisenhower quadrants accept user-created
  content, and Swimlane provides three role frames and editable linked shapes.
  These layouts do not automatically assign membership or manage tasks;
  Meeting Board does not include a timer. The additional layouts also do not
  imply structured Kanban, table or collaboration semantics.
- The new starter content is checked in all ten supported languages; gallery
  previews fit every frame in light and dark themes. Template creation is
  exercised in English and German, and localized board templates round-trip
  through native v6 with byte-stable re-export. A production-browser regression
  also selects and saves Meeting Board in desktop Chromium, Firefox and WebKit,
  checking its cards and persistent frames. These checks do not replace
  physical-device or screen-reader acceptance.
- Meeting Board reuses existing v6 primitives, so it adds no new file format;
  its starter text is translated only when the document is created. The
  gallery builds visible previews on demand and caches them separately per
  template, language and theme. This does not establish iPad first-start
  performance; see [WEB-STARTUP.md](WEB-STARTUP.md).
- Swimlane's lane names, shapes and connectors can be edited. Its lanes are
  visual guides, not role assignments: moving a shape into a lane does not
  create membership, and changing a lane frame does not move its shapes. The
  example arrows stay attached when their shapes move, but later edits can
  reroute lines across other objects; no obstacle-avoidance or workflow engine
  is implied. Core geometry, all-language content, gallery previews, native v6
  round-trips and SVG/raster/PDF generation have focused coverage. Production
  desktop Chromium, Firefox and WebKit regressions create the template, move a
  shape and inspect the saved lanes and connector endpoints; they do not prove
  physical-device behavior or broad export appearance parity.
- Venn Diagram uses two translucent ellipses below separate text cards and a
  sticky note. The intersection is visual: no set membership is inferred, and
  moving a circle does not move its labels or the sticky. The ellipse
  descriptions are translated at creation but do not follow later label edits;
  edit a circle's Description separately if its label changes. Applying a Quick
  Style retains its opacity. Thumbnail tests cover the translucent circles in
  both themes. Board thumbnails now paint objects once using the normal scene
  renderer, preserving band order and frame clipping; their simplified grid and
  frames remain preview-specific. Native-v6, SVG/PNG compositing and PDF
  generation have focused coverage. Production desktop Chromium, Firefox and
  WebKit create, move, restyle and save a circle; the light-theme production
  thumbnail was also visually inspected in a desktop browser. Browser tests
  do not assert thumbnail pixels. These checks do not prove physical-device,
  screen-reader or Office acceptance.
- Empathy Map supplies a heading, four visual Says/Thinks/Does/Feels frames and
  independent prompt stickies. A moved or deleted note does not change the
  persistent frames or gain category membership. The prompts are editable
  starting points, not inferred facts about a person. The fixed-size cards and
  frames are a basic scaffold, not richer responsive text regions. Tests cover
  localized creation, geometry, native round-trips and SVG/PNG/PDF generation.
  Production desktop Chromium, Firefox and WebKit create the board, move a note
  and inspect the saved file; these checks do not replace physical-device,
  screen-reader or broader export acceptance.
- Storyboard reuses three visual frames, three initially empty image cards and
  independent action notes. Selecting a card and choosing Local Image embeds the
  selected image rather than creating another card; undo and redo restore both
  the card reference and resource. Its frames do not group content, sequence
  timed scenes or create presentation slides. Fixed card/frame dimensions are a
  simple scaffold, not responsive layout. Native round-trip and SVG/PNG tests
  check the embedded bytes and image output. An independent PDFium rasterization
  check verifies the three scene fills and title regions, action-note colors
  and embedded image position in both themes; it is ignored by default because
  it requires Python, pypdfium2 and Pillow. Production desktop Chromium,
  Firefox and WebKit regressions create the template, upload a synthetic JPEG,
  consent to PDF export and inspect its scene colors, note placement and image
  using a separate Chromium PDF viewer. They also verify that export preserves
  the saved document and resource, and that undo/redo restore them. This is a
  focused single-page fixture, not evidence for arbitrary images, screen
  readers, physical devices, realistic image-storage scaling or broader export
  acceptance.
- User Journey Map is a visual scaffold with three independent stage frames,
  row labels for user actions, feelings and opportunities, and editable prompt
  stickies. The heading identifies the person and scenario. Frames and labels
  do not create an axis, stage membership or automatic grouping: moving or
  resizing a frame does not carry its cards. Users must adjust the fixed-size
  scaffold themselves for longer text or additional stages. Core and gallery
  checks cover geometry, all ten languages, thumbnails, save/reopen and
  undo/redo; SVG text, PNG fills and PDF generation are checked in both themes.
  Desktop Chromium, Firefox and WebKit create the template and inspect the
  saved archive's stages, independent cards and prompt positions. These checks
  use synthetic input at one viewport; richer responsive layouts,
  physical-device and screen-reader acceptance remain open.
- Moodboard is a bounded starter with six independent, initially empty image
  cards, one ideas sticky and an instruction card. It reuses Local Image rather
  than introducing a second resource or persistence path. Moving one card does
  not rearrange others, and there is no automatic collage, image cropping or
  image-search service. Native tests cover all-language content, independent
  movement, two distinct uploads with separate undo/redo, native save/reopen,
  SVG embedding, PNG image colors and PDF generation in both themes. PDF content
  and real-image workloads are not established by a file signature. Desktop
  Chromium, Firefox and WebKit create the starter, upload one generated image,
  inspect the saved archive, undo/redo, and reopen it via the browser file-input
  path. These checks do not establish File System Access handle behavior.
  Fixed-size placeholders are not a responsive collage; scalable storage,
  richer image arrangement, physical devices, screen readers and broad export
  acceptance remain open.

### F5 ink checkpoint: critique

- The colour and width popovers use the existing shared palette and exact
  controls rather than inventing unsupported brushes. The chosen settings apply
  to future gestures; a stroke already being drawn keeps its captured style.
  A short-window regression exposed floating scrollbars covering swatch hit
  targets, so ink and Shape-library popovers reserve the scrollbar's full hit
  width. Another regression checks that a tall window does not clip the palette.
  Native keyboard, light/dark, compact-window, per-gesture and undo/redo checks
  cover the slice; physical touch, screen readers and realistic performance
  remain separate acceptance gates.
- Ink gestures commit atomically, and pending or rejected gestures are exposed
  for retry or cancellation rather than silently discarded. Pending ink is not
  crash-recoverable until it is committed.
- Pixel erasing can split strokes while retaining their metadata and undo
  behavior. Sixteen focused canvas tests cover geometry, limits, atomicity,
  grouping and undo. A production-browser regression checks the saved
  fragments, their original endpoints, styles and IDs, and exact one-step
  undo/redo in desktop Chromium, Firefox and WebKit. It uses synthetic mouse
  input at one viewport and does not establish physical pen/touch or
  screen-reader acceptance. Pressure remains optional and does not affect
  rendering.
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
- Editable branch/Zone PowerPoint slides now rotate card components and
  multiline shape labels around their owning card or shape, preserving image
  bytes and source data. The canvas rotates shape labels around the same pivot;
  its upright editor tracks the rotated label, and offscreen-label culling
  respects frame clips. Native geometry tests cover multiple angles and zoom
  levels, both slide ratios, keyboard editing and undo. Ten regenerated Office
  packages pass Microsoft Open XML SDK validation, including a rotated board.
- Canvas card titles, images, to-dos, markers and annotations now rotate about
  the card center, with screen-space frame clips retained. Title editing stays
  upright at the rotated title position; links and checkboxes use local-space
  hit tests, while image resize handles track the rotated corner and preserve
  cancellation and undo. Thumbnails fit rotated cards and object-only boards.
  Thumbnail fitting reuses the scene for painting rather than rebuilding it.
  Native headless regressions cover several angles and zoom levels, text and
  image visibility, touch cancellation, clipping and keyboard/IME input.
- Branches, relationships and free connectors use rotated card outlines for
  their anchors, including when a free connector targets a collapsed
  descendant. Relationship previews use the same geometry as the committed
  layout; control offsets stay in sheet coordinates. Unrotated layouts retain
  their previous anchors. A regression checks those endpoints in SVG and
  editable PowerPoint geometry at both supported aspect ratios; the rotated
  Office schema fixture now includes a branch, relationship and free connector.
- Rotation does not reflow a card's children, so rotated cards can overlap
  nearby topics. The temporary subtree-drag connector is a schematic
  center-to-center preview, not a promise of the final border anchors.
- The label-culling bounds are deliberately generous estimates rather than
  font-specific measurements, and canvas text wrapping still differs from
  export. The upright title editor is an interaction overlay and can extend
  beyond a visually clipped frame; frame clipping is not secure redaction.
  Real touch, screen-reader and cross-platform visual acceptance remain open.
- The rotated-board fixture opened in Windows PowerPoint without repair. A PDF
  exported by PowerPoint rendered its rotated sticky, photo and diamond, along
  with the branch, relationship and free connector. This single-fixture check
  is not a systematic visual comparison. In a separate fixture copy, a rotated
  sticky's text edit and a nudge of a native path persisted after saving in
  PowerPoint. This does not cover every object or other Office hosts. Editable
  components remain separate objects, and editing them does not update the
  Thinoda document.
- The Pitch stage now fits its containing player rather than the height of the
  browser window. An automated production-browser check captures a rootless
  board containing both a sticky and a floating tree and verifies that both
  appear within the player. Browser SVG, PNG, JPEG and PDF regressions inspect
  the downloads after consent and verify rendered board content and source
  preservation. A separate PowerPoint regression checks consent, cancellation,
  the flattened viewport image, its board content and the unchanged source.
  These do not substitute for the remaining format, physical-device and
  cross-platform presentation acceptance.
- A focused rootless image-card regression checks that SVG embeds the original
  JPEG and references it, and that SVG, PNG, JPEG and PDF render all four
  colored fixture quadrants in their expected positions. It compares native
  board content, resource metadata and image bytes before and after export.
  The mixed-board JPEG check allows compression differences and locates the
  frame by dense pixel extents instead of treating isolated compression
  artifacts as its edges. Image-card and mixed-board SVG/PNG/JPEG/PDF
  regressions passed in desktop Chromium, Firefox and WebKit on Windows; the
  downloaded PDFs are rendered by a separate Chromium viewer in every case.
  This synthetic 64-pixel image does not establish large-image,
  EXIF-orientation, physical-device or screen-reader acceptance.
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

### Experimental asynchronous recovery discovery

- The isolated draft in `docs/work-in-progress/freeform/recovery` now scans
  retained history in pages and audits session/document stores before and after
  claiming previews. Opaque or changed records are not discarded or returned as
  verified previews and produce warnings. The draft always retains a
  coordination warning, so its draft adapter cannot enable editing on the
  strength of the audit alone.
- Duck check: an atomic snapshot is not a write barrier. An old or
  uncoordinated same-origin writer can still insert a record after the final
  audit and before its result is used. Retained tombstones and empty sessions
  also still require complete inspection; the deadline can be exhausted on
  every retry. A resumable cursor is not safe without revalidation against
  independent writes. Do not integrate or migrate this draft until these
  safety and liveness boundaries have a verified solution.
- Its standalone JavaScript harness passed 73 checks each in desktop Chromium
  and Firefox. Desktop WebKit passed 13 focused checks but timed out on the
  retained-history stress cases. A separate malformed-record rescue test
  cannot reach rescue: WebKit rejects the synthetic IndexedDB Blob fixture.
  The harness surfaces that write error rather than hanging on the abort.
  The Rust/app draft, physical Safari/iPad, quota/eviction and real multi-tab
  acceptance are not established. Production continues to use the existing
  synchronous recovery path.

### Recovery failure visibility checkpoint: critique

- Failed draft preparation, serialization or queueing no longer only reach the
  log: the app shows its existing localized recovery warning, retaining both the
  draft and any previous complete snapshot. A failed native background write or
  stopped worker is also reported. Startup reports an unavailable service.
- A warning remains active while any document has an unresolved snapshot error
  or a queued retry has not been persisted. Queueing during an in-flight write
  retains the newer snapshot for a subsequent flush; successfully writing an
  older snapshot cannot falsely clear the warning.
- An earlier production web build passed 36 recovery and localization scenarios
  across desktop Chromium, Firefox and WebKit, including real localStorage
  quota failure, retry, stale ownership and startup failure. After the final
  source correction, the production build passed 30 selected recovery and
  localization scenarios across the same three desktop engines. Native
  regressions exercise draft serialization, failed queueing, real worker write
  failure, worker termination and concurrent newer snapshots.
- Serialized native suites passed 131 platform, 509 application-library and
  77 application integration tests; two application-library tests were ignored.
  Strict native and WebAssembly Clippy passed with integrated cloud services
  both disabled and explicitly enabled. The production configuration remains
  disabled by default. All 25 documentation metadata tests, formatting and
  whitespace checks passed.
- This is not a storage-capacity, transaction or automatic worker-restart fix.
  Pending edits may require conflict resolution before they can be saved;
  saving or downloading and independent backups remain necessary. The warning
  can be dismissed, and an indefinitely blocked write is not detected by a
  watchdog. Serialization-error and worker-spawn failures are not artificially
  injected by these checks. Cleanup failures are still logged and retried while
  the document remains open; a closed document can leave a stale recovery copy.
  Browser localStorage quotas, non-atomic cross-tab leases, physical-device
  testing and safe large-board recovery remain open.

### Recovery cleanup and menu checkpoint: critique

- Failed removal now has a distinct warning rather than suggesting that a
  successful save also removed every recovery copy. A clean open document
  retries its own cleanup; closed-document retries check for a reopened key and
  retain the recovery manager's ownership checks. Pending snapshot bytes remain
  governed by the existing remove/flush contract, and newer edits are never
  removed by the retry. Failed recovery handoffs retain their original source
  and only schedule cleanup for copies that were actually attempted.
- Closed-document retries are round-robin, at most one per housekeeping poll,
  with a 256-key in-memory bound. Overflow is visibly flagged but is not
  automatically retried; persistent failure, lost ownership, a crash or exit
  before retry can leave a stale copy for the next launch. The warning can be
  dismissed. This is not a durable cleanup journal, a transactional browser
  lease, or a fix for localStorage quota and large-board recovery.
- Board now shares its command list between the first-level main menu and the
  canvas context menu. Styles groups the requested topic/style commands, Board
  sits between Select and Arrange, and New Sheet from Topic is last. Native-menu
  and ribbon snapshots now include the selected board objects, preventing a
  stale action from affecting a different board selection. The underlying
  command availability, shortcut bindings and edit transactions are reused.
- The canvas context menu was later regrouped by target (topic, board object,
  blank canvas, Zone, annotation) into Add, Structure, Format, Arrange, Layers,
  Zones, Select, AI, View and Draw submenus with Command Palette… as an escape
  hatch and Delete last; Board commands appear directly (Group/Ungroup,
  Lock/Unlock) instead of in a Board submenu. Optional, local-only
  context-menu usage counts can be enabled in Settings > Editor.
- The localized menu matrix covers short windows and scaling, popup bounds,
  first/last command reachability, context submenu order and command dispatch.
  It exercises a selected board shape's lock action and verifies its resulting
  state. The final production candidate passed seven recovery/ownership cases
  in each of desktop Chromium, Firefox and WebKit. Native library suites passed
  521 application tests (two ignored) and 131 platform tests; strict native
  all-target application and WebAssembly web Clippy passed.
- Actual operating-system menus, assistive technology and physical touch devices
  still require acceptance; the browser canvas accessibility gap is unchanged.

### Photo orientation checkpoint: concept and critique

- Camera JPEGs can contain rotated or mirrored EXIF orientation even when their
  stored pixel dimensions are landscape. A native regression reproduces an
  incorrectly mirrored preview. Decode orientation before sizing and painting;
  cover all eight transforms and keep the original resource bytes in the document.
- Visual exports must not rely on inconsistent host interpretation of EXIF or
  apply an orientation twice. SVG and editable PowerPoint now embed a lossless,
  orientation-normalized PNG only when a transform is required; unchanged images
  keep their source bytes. SVG bounds aggregate image bytes to 64 MiB, and
  PowerPoint bounds media against its 128 MiB archive budget and reuses a shared
  image within each slide. Inadequate budgets fail explicitly. Native documents,
  XMind resources and clipboard retain the original bytes.
- Duck check: normalization preserves decoded pixels but removes image metadata
  and embedded color profiles. A localized export preflight warns about this
  potential loss; saving as Thinoda keeps the original. It is not a color-managed
  conversion. PNG and JPEG fixtures with all eight EXIF values check preview
  pixels, SVG payload and dimensions, raster output, PDF generation, PowerPoint
  media and dimensions, and native/clipboard round-trips. The PNG writer's final
  chunk can fail during cleanup, so the byte limit is checked after finalization
  as well as during writes. A generated rotated upload also verifies default
  sizing, source bytes and undo/redo for both topic and rootless-board images.
- Real camera/profile files, physical iPad/browser behavior, and rendering in
  external PowerPoint and XMind clients remain acceptance work. XMind keeps the
  original metadata rather than claiming that every external viewer interprets it.

### Contextual board editing: concept and critique

- Replace the shape-name list with an icon grid derived from Thinoda's actual
  shape geometry. Search and accessible localized names remain available; the
  grid scrolls rather than clipping controls at small sizes.
- Sticky and text-card formatting needs a clear distinction between whole-card
  styling and selected text ranges. Existing plain titles and topic style fields
  must not be silently promoted to an unsupported rich-text representation.
  Rich lists, fonts and alignment require explicit model, editing, import/export,
  clipboard, undo and migration contracts.
- Connector controls should reuse the existing straight, elbow and curve routes,
  endpoint arrows, width and color rather than inventing alternate geometry.
  Locked or stale selections must not be edited.
- Pen controls should expose real pen/highlighter and eraser settings. The
  reference shows additional brushes; do not imply those render faithfully until
  their stroke model, rendering, persistence and export are implemented.
- Context controls must avoid obscuring selected content and toolbars, remain
  navigable by keyboard, announce localized names, and respect short viewports,
  dark mode and pending text/ink drafts. A visual match alone is not functional
  or screen-reader acceptance.
- Duck check: an icon grid is not an accessible shape picker merely because its
  pictures are recognizable. Search, focus order, selected state, localized
  names, concave silhouettes and access to every item must survive filtering,
  scrolling and narrow windows.
- Duck check: opening formatting while a text editor owns focus can accidentally
  commit or discard its draft. Resolve selection and lock ownership at the
  transaction boundary; preserve unfinished text and IME composition, and never
  represent whole-card changes as selected-range formatting.
- Duck check: changing a connector or tool during an active gesture can leave
  stale handles or mutate an unfinished stroke. Reuse the existing gesture
  guards and verify undo/redo, save/reopen, clipboard and every supported visual
  output rather than inferring correctness from the control state alone.

### Notes strikethrough checkpoint and critique

- The existing Notes inspector and its staged enlarged editor add a localized
  strikethrough action using Markdown markers and the established draft/undo
  transaction. Repeating the action removes its markers when the parsed result
  is safe. Bold, italic and strikethrough now leave selected outer whitespace
  outside their delimiters; a safe whitespace-only selection inserts an empty
  pair at its end for subsequent typing.
- Duck check: a pair of markers is not proof of rendered formatting. Before
  applying a nonempty bold, italic or strikethrough action, the preview parser
  must recognize the span. Before and after must have the same projected text,
  unchanged non-target inline styles and Markdown structure, and no target-style
  change outside the selected source. This includes link destinations and
  headings, not only the more limited formatted-export parser. The action
  rejects unsafe combinations without changing the source; a localized warning
  retains the selection and returns focus. Common nested bold/italic examples
  remain reversible, and italic no longer removes a bold marker merely because
  it contains an asterisk. Escaped markers are not stripped as markup.
- Duck check: returning focus after rejecting an action could discard an
  uncommitted inspector buffer that had just blurred. All three actions now
  restore that buffer and keep it dirty rather than committing or discarding it.
  Empty or whitespace-only insertion is rejected inside recognized code, HTML
  or adjacent formatting-marker runs, and its visible marker characters are
  excluded when checking that existing text, styles and structure stay intact.
  Formatting and marker removal both require the source and result to fit the
  16 KiB, 2,048-event and depth-32 preview bounds; no caret-only exception
  assumes unknown context in an over-limit note. The checks are deliberately
  conservative and can reject valid Markdown combinations, particularly where
  decoded text cannot be mapped precisely to a partial source selection.
  Multi-paragraph formatting, over-limit formatting and richer editing remain
  open; manual source editing is unaffected.
- Duck check: asterisk-run parity uses byte offsets at valid character
  boundaries. The count before a position cannot exceed that prefix's byte
  length; tests cover escaped and unescaped runs after both ASCII and
  multibyte text rather than assuming character indices are byte indices.
- Duck check: explicitly formatting a legacy plain note promotes the whole note
  to Markdown. The action now checks that pre-existing literal punctuation
  cannot become a link, heading, list, code or other Markdown structure. If
  needed it escapes ASCII punctuation, remaps the scalar-indexed selection and
  validates the intended change against the bounded parser. Unsupported
  conversions retain the plain draft, selection and localized warning; for
  example, leading indentation that becomes a code block can be refused.
  Escapes may appear throughout the editable source, and applying a formatting
  action is not a general plain-to-Markdown conversion command.
- This check compares text with whitespace normalized, because the existing
  Markdown projection and preview can normalize leading/trailing whitespace,
  blank lines and soft/hard breaks. It does not promise byte-identical rendered
  whitespace or preserve an unformatted note's original source spelling after
  promotion; Undo restores the original plain note. The source-first editor
  still uses character offsets rather than a grapheme/IME transaction model,
  and formatting cannot be represented in all exports.
- Plain-promotion tests cover literal emphasis, links, HTML-like text, headings,
  list markers, escaped punctuation, Unicode selections and expansion limits.
  Map, Outliner and staged enlarged-editor tests exercise draft ownership,
  selection mapping, native save and Undo/Redo; they do not establish physical
  IME or screen-reader behavior.
- Duck check: list buttons previously inserted or removed markers without
  checking what Markdown would render. Formatting one line could absorb an
  unselected following line into a list, alter an existing item's numbering,
  or put literal markers inside a code block. Both list actions now compare the
  bounded plain-text projection and non-list syntax, preserve list membership
  and numbering outside the affected lines, and require parsed list items for
  the affected lines. Rejection uses the existing localized warning and keeps
  the draft, selection and plain/Markdown format unchanged.
- This check is intentionally conservative: for example, listing the first
  line of an adjacent two-line paragraph can be rejected rather than silently
  absorbing the second line. Nested-list edits and over-limit notes can also be
  rejected; manual Markdown source editing remains available. Creating a list
  can intentionally change paragraph and soft-break layout around the selected
  lines, even when projected text is unchanged. The comparison operates on
  parser events and does not resolve the whitespace, physical IME and
  screen-reader limitations above.
- The list tests include formatted and plain Notes in Map and Outliner, the
  staged enlarged editor, links and inline styles, nested lists, Unicode,
  twelve numbered items and 17,280 mixed three-line candidate operations. The
  generated cases check parsed item coverage for accepted operations; they are
  not a substitute for real input-device or assistive-technology acceptance.
- Native egui coverage exercises Map and Outliner, focused Unicode drafts,
  selected whitespace, nested styles, rejection, undo/redo, recovering and
  saving a rejected focused draft, all language catalogs and the enlarged
  editor's apply boundary. A user-authored example
  also reaches FreeMind as a strikethrough run; existing Word, Excel and
  FreeMind format tests cover their supported note subsets. Physical input,
  screen readers and external Office applications remain separate acceptance.

### Graphical Shape picker checkpoint

- The Shape library now presents the existing shapes as a searchable grid. Each
  tile draws the model's actual outline and detail strokes, has a localized name
  and tooltip, and shows the active shape. Arrow keys, Tab, Enter, Space and
  Escape work alongside pointer and touch input; picking a tool does not edit
  the document. The popup reflows and scrolls inside short or narrow windows.
- Duck check: an icon can misrepresent a concave shape if it is filled as a
  convex polygon, so previews draw the actual outlines rather than a convex fill.
  Selection, filtered search, focus and scroll behavior have native egui
  regression coverage. GPU-rendered light/dark previews were inspected at wide
  and compact sizes; this is not browser screen-reader or physical-touch
  acceptance. The other contextual editing slices remain open.

### Whole-card formatting checkpoint and critique

- Selected sticky notes and text cards expose bold, italic, underline, alignment,
  font family, size and color presets for the **whole card**. The strip is
  reachable from the contextual topic toolbar and canvas context menu, names its
  scope in every supported language, and scrolls horizontally in narrow windows.
  Custom colors remain available through the existing Text color control. This
  did not add selected-range formatting or lists. The subsequent active-title
  checkpoint below enables these whole-card controls during title editing
  without surrendering the draft or IME input.
- Duck check: a dock can reduce the canvas enough for the floating insert
  toolbar to obscure a selected card. While formatting is open, its insert and
  marker commands move into an accessible overflow instead; the regular toolbar
  returns when the strip closes. Compact and wide light/dark GPU previews and
  keyboard, pointer and overflow-command tests cover the transition.
- Duck check: closing a dock during an unfinished resize or ink stroke changes
  the canvas coordinates underneath the gesture. An unchanged owner keeps the
  dock in place but disabled until the gesture or draft finishes; document,
  sheet, revision, selection and lock checks still reject stale transactions.
  Tests cover resize commit/cancel, pending ink, drafts, locks, undo/redo and
  separate document viewports.
- SVG output now includes italic and text decorations and positions each line
  according to the existing alignment; the editable PowerPoint path also writes
  paragraph alignment. Native round-trips, SVG and PowerPoint XML, PNG/JPEG
  pixels, and rasterized PDF output are covered. Bundled Ubuntu and Hack italic
  faces provide raster and PDF fallbacks when system fonts are unavailable.
  Font shaping still measures the non-italic face and exported viewers may
  substitute fonts, so exact italic geometry and cross-host rendering are not
  claimed. Physical touch, screen readers and external Office acceptance remain
  open.

### Styled card-title checkpoint and critique

- The whole-card formatting strip adds a localized strikethrough toggle using
  the existing style transaction, undo and persistence. Sticky-note and text
  card title drafts now display their resolved font family, size, color,
  alignment, italic, underline and strikethrough. Bold uses the same offset
  copy as canvas painting. The document still stores plain titles and one
  whole-card style. Controls during a title draft are covered by the subsequent
  active-title checkpoint below.
- Duck check: painting the text-edit galley twice also paints its selection
  backgrounds twice. The bold copy is instead laid out without selection and
  painted beneath the editor's normal text, selection and IME feedback, clipped
  to the editor. Opaque selection backgrounds can cover that copy, so selected
  bold text may look lighter until the selection collapses. Decorations retain
  their chosen color in a selection; user-selected colors can have low contrast.
  Font measurement still uses the non-italic face, so exact wrap and geometry
  parity with the non-editing canvas are not claimed.
- Duck check: on a short window the global floating insert toolbar covered the
  first lines of a card title. It is conservatively hidden while an active card
  editor intersects its expected bounds or the bounds are not known; it returns
  when the measured bounds clear the editor or the edit ends. The main menus
  and shortcuts remain available. This does not claim that every board overlay
  avoids every card.
- Native egui tests cover both card kinds, all combinations of the four text
  effects, light/dark themes, left/center/right alignment, several zoom levels,
  wrapped trailing whitespace, selection, clipping, Unicode copy/paste, empty
  drafts, synthetic IME input, conflicting drafts and undo/redo. They also cover
  compact toolbar overlap and window/UI-scale changes. GPU-rendered selected
  and unselected drafts were inspected in light/dark wide and compact views.
  Real IME, touch, screen readers and physical-device acceptance remain open.

### Active card-title formatting checkpoint and rubber-duck critique

- Sticky-note and text-card title drafts keep their resolved appearance while
  the docked whole-card formatting controls change bold, italic, underline,
  strikethrough, alignment, family, size or color. The document still stores
  plain text plus a single card style: selections are not separately styled and
  list editing is not implemented. Alt+F10 transfers keyboard focus between
  the title and the strip; pointer actions and popup controls retain the draft.
- Duck check: formatting must not overwrite a different card, a conflicting
  draft or an unfinished gesture. Controls revalidate the document instance,
  sheet, root, topic, revision, selection and locks at the transaction boundary;
  pending inspector/ink edits, IME composition and canvas gestures disable
  unsafe actions. The saved caret/selection is restored only to the same edit
  session, while a deliberate new editor click takes precedence. Style edits
  are independently undoable from the eventual title commit.
- Duck check: applying a numeric size on every accepted edit changes Escape
  semantics. The value is applied live, and Escape leaves the numeric editor
  without rolling back an already applied change; Undo reverses it. Save and
  close process pending title or size input before operating on the document.
  A save captures only the preceding input. When later typing is already queued,
  the title editor resumes its caret and selection only if the same document,
  sheet, topic, edit session and committed title still match; later edits remain
  dirty and are not included in the earlier save.
- Duck check: egui can receive text, focus-changing keys, pointer events and a
  close request in one batch, or repeat a discarded layout pass. Input is split
  at focus transitions and queued per viewport, preserving touch packets and
  event order. An egui input plugin covers immediate native document windows;
  the backend close hook also prevents closing before queued child-window input
  and drafts are handled. Recreated viewports, repeated passes, hidden-root
  repaints and backend logic-only calls have regression coverage. This queue is
  transient and is not a substitute for crash recovery of an uncommitted draft.
- Duck check: a deferred tab-close index can point to another document after
  another window closes or moves a tab. Both deferred command paths now capture
  the original document instance and owning viewport, resolve its current index
  at execution, and warn rather than closing a replacement or moved target.
  Tests cover earlier-tab removal, target removal and target movement.
- Duck check: a pointer-created card can lose the focus requested during the
  creation click when egui finishes that pass. A single retry is scoped to the
  same viewport and a later frame, and runs only while the window has focus,
  no new pointer transition occurred, and no formatting companion or minimap
  owns interaction. Repeated layout passes and immediately following text are
  covered; a subsequent pointer action is not stolen. Ordinary topic drafts
  continue to use their existing modal-focus behavior.
- Duck check: a conflicting title cannot be committed, but retaining its draft
  must not trap focus or consume another control's keys or IME composition.
  The minimap and unrelated editors keep focus, and a failed commit is not
  retried every frame while another control owns focus. Returning to the canvas
  still restores the conflicting editor for correction.
- Focused native tests exercise 92 card-formatting/input cases, including
  selection restoration, Unicode/IME events, touch cancellation, stale owners,
  separate undo, save and close races, compact layouts and immediate viewports.
  The 159 board cases and 372 canvas cases also pass (two canvas cases are
  ignored), as do strict native and WebAssembly Clippy checks including the
  default-off cloud feature. A fresh production build passed all 69 configured
  Chromium browser cases; the 13 card-formatting, mixed-export and language
  cases also passed in desktop Firefox and WebKit. Card-formatting tests inspect
  the downloaded native archives. These are synthetic inputs: physical IME,
  AltGr keyboards and touch, screen-reader operation, real iPad behavior and
  broader external Office/export acceptance remain open. The browser
  accessibility bridge is still incomplete. Direct programmatic
  `open_path`/`request_open` calls do not participate in the UI event-ordering
  wrapper.

### Selected-range text and lists: bounded model foundation; controls disabled

The title remains a `String`, with a separate bounded `InlineText` value for
Sticky/Text cards. The editor keeps a separate string draft; layout's
`TextMeasure` receives a string and one font, while visual export caches shaped
text by that same single style. The core model now validates sparse inline
overrides and paragraph list metadata, but toolbar controls remain disabled:
a rich selection still must survive editing, layout, save and export.

Implementation contract and remaining work:

1. Keep the existing plain title as the text source. The core model now stores
   bounded sparse inline overrides and bulleted/numbered paragraph metadata
   (without nesting or checklists).
   Style offsets are Unicode scalar positions, never UTF-8 byte offsets or
   browser UTF-16 offsets. Validation limits formatted titles to 10,000
   characters, 1,024 runs and 1,024 list paragraphs; font families are limited
   to 128 bytes and sizes to 6–256 points. `None` inherits whole-card style,
   while `Some(false)` explicitly disables a property.
2. Native format v8 is required only when inline content is present; older
   documents retain their previous version. The importer rejects inline data
   in pre-v8 archives and legacy unversioned JSON. Non-native export preflight
   reports inline formatting/list loss. Until rich-aware editing is complete,
   plain title edits and search/replace reject affected cards without changing
   the title or formatting. Clipboard/recovery propagation and external format
   fidelity remain open.
3. Keep whole-card formatting explicitly separate from selected-text styling.
   Bind selection, pending input and composition to the same document, topic,
   revision, edit session and viewport. Preserve formatting through insertion,
   deletion, paste, undo/redo and save/close ordering; do not infer edits from a
   single before/after string diff, which is ambiguous for repeated text or
   multiple edits. Lists need explicit paragraph semantics rather than merely
   painting a bullet.
4. Measure and render mixed fonts, sizes and decorations together in the canvas
   and export paths, including wrapping, clipping and rotation. Extend export
   preflight and clipboard fallbacks so unsupported formatting or list
   structure is reported, not silently discarded. Editable PowerPoint requires
   real text runs and paragraph properties; SVG-derived PDF and raster output
   need the same visual layout, not a plain-text substitute.

Rubber-duck blockers: egui's existing text undo stores plain text, so rich
draft undo and IME updates need a coherent edit-operation history rather than
parallel, unsynchronized strings and spans. Character offsets alone do not
guarantee sensible grapheme-cluster selection or bidirectional text behavior;
the model range operation currently relies on its caller to supply an
appropriate range. Mixed-style measurement changes layout and cache keys, and
every title-writing path—not just the canvas editor—must preserve or explicitly
replace formatting. Clipboard HTML may be sanitized, and external applications
may accept only plain text. Specify and test caret-only formatting, list
editing, accessibility and lossy-format consent before exposing these controls.
Native and browser screen-reader, real IME and physical-device acceptance
remain separate gates. The model foundation is implemented, but no
selected-range or list controls are enabled.

An editor prototype can investigate egui 0.37's `TextBuffer` interface: typing,
paste and IME invoke character-indexed insert/delete operations, which avoids
guessing edits from a final string. It is not a complete solution: egui's
built-in undo/redo calls `replace_with` on a plain-string snapshot, IME
preedit repeatedly deletes and reinserts a composition, and default
backspace/delete operate on individual scalar values. A rich-aware history,
composition transaction and grapheme policy must be verified before wiring the
adapter to persisted content.

The standalone experiment under `work-in-progress/freeform/rich-text` exercises
explicit scalar-indexed replacements and broader style-run operations,
including invalid ranges and budget rejection. It now also explores snapshot
history, directional selections, caret typing styles and explicit IME
preedit/commit/cancel transactions. Independent randomized character-level
oracles exercise 2,500 span operations and 2,500 mixed history/composition
operations; a further 10,000 operations exercise changing history budgets.
That experiment is not compiled into Thinoda. Production now also has an atomic
scalar-range replacement primitive that preserves and shifts unaffected style
runs. It refuses to change paragraph boundaries while list metadata exists.
The bounded sparse-style/list model, range style updates, validation,
history-memory accounting, and native/export loss gates are production code.
Egui event/undo integration, empty-commit versus cancellation, grapheme
behavior, paragraph-list editing, clipboard, rendering and accessibility are
not production implementations.

Duck check: the replacement primitive is a pure candidate operation, so a
failed range, style, or list-boundary check cannot partially change the source.
It preserves scalar-indexed runs, but it does not choose the typing style or
make scalar boundaries grapheme-safe; those remain editor responsibilities.
When list metadata exists, edits touching or inserting a newline are rejected
rather than guessing how list membership should propagate. This improves the
core safety boundary but does not make the current plain-text editor rich-aware.

Duck check: an IME's committed text can differ from its last preview, so the
prototype requires explicit final text and keeps failed updates atomic. It
also models surrounding-text deletions, with provisional changes grouped into
the active composition. Egui 0.37 clears a preedit on empty preedit/commit
after the initial preedit has already replaced selected text; the prototype's
cancel-restores-original policy is not the same behavior. Egui saturates
surrounding-deletion cursor arithmetic but may pass an oversized range to the
buffer, while the prototype rejects invalid ranges. The app's synthetic egui
characterization tests cover these operations and whole-string undo/redo; they
do not establish real IME event sequences. The event mapping, cancellation and
treatment of surrounding edits need explicit real-platform decisions.

Duck check: an entry-count limit alone does not bound snapshot memory. The
standalone history now estimates retained undo/redo entry bytes, including text
and span capacities and style-reported allocations; a font-name example,
budget-boundary tests and explicit overflow rejection cover that accounting.
It preserves the nearest undo and redo even if they exceed the budget, reports
eviction and over-budget retention, and does not trim merely because undo/redo
move entries between stacks. That last behavior differs from the existing
document history and needs a product decision. The estimate excludes allocator
overhead, stack capacity, current and composition drafts, and temporary copies;
shared allocations or inaccurate style accounting also affect it. It is not a
hard or peak-memory bound. Caret-style inheritance, eviction policy and real
input acceptance remain open. A passing scalar-level oracle does not establish
grapheme, bidirectional or egui-editor correctness.

Duck check: a test-only bounded `TextBuffer` guard snapshots the draft, selection
and a separate copy of egui's shared undoer before rendering. Synthetic
over-limit paste, preedit and surrounding-deletion failures restore text and
styles, including directional selection. That is not a production solution:
egui can already report the rejected response as changed and expose the
partially edited widget value until the next repaint. If several input events
share a frame, rolling back the frame also discards earlier valid input.
Plain-text `replace_with` still cannot reconstruct style history; the toy
implementation loses the inserted style on redo. Per-event atomicity, visible
error handling, composition ownership and rich undo need a different design.
These simulated events do not establish physical IME or browser behavior.

The title-write audit identifies `Sheet::set_title` and bulk search/replace,
tree-table and Outliner editing, generated FIM labels that assign titles
directly, and template and copy paths. Snapshot undo copies the current fields,
but these plain-string write paths need explicit preserve, replace or reject
semantics before spans can be persisted. Native archives currently gate
versions 2–6 by content; raw legacy JSON bypasses the archive manifest. Sheet
clipboard prefixes, separate subtree/board clipboard payloads and recovery
snapshots have independent compatibility boundaries. A rich-text version
must cover each path, including malformed data and content under older declared
versions, without changing output for existing plain documents.


### Compact Settings notification checkpoint and rubber-duck critique

- The previous committed source can place a persistent recovery warning over a
  Settings switch in a 320×240 window. While nonmodal Settings is open, the
  dialog now reserves space for a bottom notification strip instead. Notices
  remain accessible by scrolling and dismissible; a modal dialog continues to
  use the existing behind-modal layer rather than covering its controls.
- Duck check: a fixed reservation also shortens Settings for a single transient
  notice. The strip takes at most 30% of the window height, capped at 128
  points, and both the dialog and a stack of notices can scroll. A focused
  dismiss button scrolls into view; this avoids hiding a recovery warning to
  make room. It is not a general collision-avoidance system for every overlay.
- The original alignment-preference and language-selector regressions pass
  without weakening their assertions. Native interaction tests cover all eleven
  language settings, light and dark themes, 240×200, 320×240 and 1000×700
  windows, warning dismissal, and a Settings-to-shortcuts modal transition.
  Stacked notices are also exercised with keyboard Tab and Enter. These tests
  do not establish physical-device or screen-reader acceptance, and the web
  accessibility bridge remains incomplete. The Chromium harness forces a
  consistent device scale so its fixed canvas coordinates are not distorted
  by a mismatch between emulated and host pixel ratios.
- The final Windows x64 `cargo xtask ci` passed with live AI disabled and tests
  serialized. Strict default and opt-in WebAssembly Clippy and opt-in native
  app Clippy also passed. This does not substitute for other native hosts.

### Selected-connector checkpoint and critique

- With exactly one connector selected in Select or Connector mode, the board
  toolbox provides straight, elbow and curved routes; start and end arrows;
  width presets and an exact value; and automatic, swatch and custom color.
  The existing connector model, endpoints, label, description and object style
  are preserved. Changes are undoable and survive native and clipboard
  round-trips. The Auto choice removes only the connector's explicit color,
  retaining any inherited object stroke.
- Duck check: a popup or provisional numeric/color edit can outlive its target.
  Document instance, revision, viewport, sheet and selection ownership are
  rechecked at the transaction boundary. Locked/grouped objects, mixed
  selections, canvas gestures, pending drafts and blocking UI states cannot be
  edited; stale popups and drafts close without applying their values. Drag and
  keyboard edits commit once and cancel on Escape, rejected touch or changed
  ownership. An unpremultiplied color draft retains exact RGB through zero
  opacity, including after the custom picker is reopened.
- Duck check: keyboard focus can move below a short toolbar, and expanding the
  custom color editor can leave it below the popup's viewport. Focus changes and
  expansion now reveal the relevant controls. Native egui tests and inspected
  light/dark GPU previews cover wide and compact layouts; these do not replace
  actual screen-reader or physical touch/iPad acceptance.
- SVG and editable PowerPoint now use the same explicit-then-inherited stroke
  precedence and label color as the canvas, including object opacity and
  arrowheads. Automatic fallback parity was addressed in the subsequent visual
  export appearance checkpoint; external Office rendering is still unverified.

### Visual export appearance checkpoint and critique

- GUI SVG, PNG, JPEG, PDF, print, Pitch and PowerPoint exports use the document
  view's resolved automatic board stroke. An explicit connector color wins over
  its inherited object stroke, which wins over the automatic color. Unstyled,
  unfilled shapes use the canvas's two-unit outline; filled shapes without an
  explicit outline no longer acquire interior detail strokes. Pitch editor,
  player, presenter map, PowerPoint preview and board thumbnails paint using
  the current palette. The inactive canvas snapshot and drag ghost use the
  same view accent rather than switching to gray.
- Duck check: appearance is not document content. The source workbook, native
  serialization and nonvisual formats remain unchanged. A new optional export
  context carries a resolved color rather than modifying the workbook or adding
  a field to existing public options. Callers that do not supply it, including
  the CLI, retain historical export colors, widths and shape details.
- Duck check: consent and output work may outlive a view change. Pitch captures
  appearance before its loss-consent dialog and passes it to the worker. Print
  captures it with the workbook when its editor opens, so later settings or
  theme changes do not silently change that snapshot. Ordinary GUI export uses
  the document view at export time. An intentionally transparent UI accent
  resolves to the same light/dark gray fallback as the canvas.
- Duck check: egui colors are premultiplied. The captured accent is converted
  to unpremultiplied sRGB before the export renderer applies object opacity;
  explicit and inherited colors keep their existing precedence. Raster
  antialiasing, low-alpha rounding, zoom-dependent minimum strokes and color
  management may still differ, so pixel-identical rendering is not promised.
  Print's light/dark controls change the page theme, not the captured stroke;
  an accent chosen for a different page theme can therefore have low contrast.
- Duck check: the initial checkpoint left two page-theme mismatches. The
  subsequent page-theme checkpoint below addresses them; real-host acceptance
  remains separate.
- Focused regressions cover both themes, transparent accents, opacity including
  zero, labels and arrows, explicit/inherited/automatic strokes, shape details,
  unchanged sources and historical defaults. They inspect SVG and editable
  PowerPoint XML, pixels in PNG/JPEG and flattened PowerPoint, slide previews,
  print snapshots and worker output. PDFium rasterization checks map,
  presentation and print PDFs in both themes. These tests are not physical
  printer, external Office, hardware-GPU, screen-reader or iPad acceptance.

### Visual export page-theme checkpoint and critique

- GUI image, PDF and PowerPoint exports now use the document view's resolved
  theme, including changes to the operating system theme while the System
  setting is active. PowerPoint export now uses dark pages in dark mode,
  matching its preview instead of silently exporting light pages. This is an
  intentional change to GUI output; nonvisual formats and the default
  programmatic and CLI export options retain their existing behavior.
- Duck check: page appearance is distinct from document content and the
  selected aspect ratio or loss consent. The exporter still captures the
  active document view at export time, keeps those options unchanged and does
  not mutate the source. Print retains its independent page-theme controls and
  its previously captured stroke color.
- Focused tests simulate light-to-dark-to-light system changes, inspect SVG
  backgrounds, compare the live PowerPoint preview with a rendered slide,
  and inspect both editable slide backgrounds and flattened viewport images
  inside generated decks in both themes. They do not replace acceptance in
  installed Office, on physical printers, or on real browser/OS hosts.

### Topic and frame drag-preview checkpoint and critique

- Topic and frame ghosts no longer translate unrelated shapes, ink or
  free-standing connectors. A ghost paints board connectors only when both
  topic endpoints are moving with it. The main canvas and ghost share the
  already-built scene, and frame-group expansion and descendant collection are
  captured at gesture start rather than repeated on every paint.
- Duck check: frame membership is not geometric containment. Moving a frame
  moves its floating roots and the complete board groups of its root cards,
  even when other group members are outside the frame. The preview outlines
  those additional cards, visible descendants and grouped objects, while
  ungrouped objects merely inside the frame remain stationary. The core move
  operation remains authoritative; resize does not move group members or
  reject a locked group, while a locked-group move is rejected without a
  partial edit.
- Duck check: a frame's clip moves with that frame, but other frame clips stay
  fixed. Ghost connector clipping is recalculated from its destination
  geometry. A topic drag retains its current clipping owner in the preview;
  reparenting can change that owner and reflow the layout on drop.
- Duck check: group outlines are schematic, not rendered copies. Connectors
  with a stationary endpoint or a connector endpoint are not translated as
  rigid outlines, and ungrouped links involving additional group members may
  update only after drop. The real connector geometry remains authoritative.
  The canvas still scans board objects during painting; this checkpoint does
  not establish a large-board performance threshold.
- Regression coverage compares rendered ghosts with and without unrelated
  board objects, checks stationary and moving clips, grouped descendants and
  connector outlines, and exercises move, resize, locked rejection, cancel
  and undo through the canvas. Physical touch and cross-platform visual
  acceptance remain open.

Not in this package: real-time collaboration (R12), comments with identities,
boolean shape operations, handwriting recognition, tables, board library
management.

## 7. Non-mind-map templates

A new gallery category **Boards**. Templates become data-driven (versioned,
localised strings, theme-aware colours, validated) instead of growing the
hard-coded switch in `templates.rs`.

**Board templates (F4):**

Status: implemented in `crates/thinoda-core/src/templates/boards.rs` (data:
frames, cards, floating trees, shapes, connectors; one builder) and listed in
the gallery category **Boards** (the existing categories hold the mind-map
templates). All content strings are catalog keys translated at creation time
(`templates::instantiate_localized`); every UI language catalog covers them.
Sticky colours are pastel fills with dark text and frame tints are translucent
mid-tones, so both read on Light and Dark backgrounds. Tests check validation,
determinism, an empty hidden anchor, disjoint card/tree subtrees, disjoint
frames, intended Venn circle overlap, disjoint shapes in other templates,
cards inside their assigned frames and catalog coverage.

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
| Affinity diagram | Idea stickies and persistent frames for sorting and naming themes |
| Eisenhower matrix | Four persistent, labeled priority-quadrant frames |
| Meeting board | Purpose heading and independent prompt stickies in visual Agenda, Notes, Decisions and Actions frames; no timer or task automation |
| Swimlane | Three visual role frames and five editable process shapes linked by elbow connectors; no role or workflow automation |
| Venn diagram | Two overlapping translucent circles with independent labels and a shared sticky; no set-membership logic |
| Empathy map | Four visual sections and independent prompt stickies; no inferred thoughts, feelings or category membership |
| Storyboard | Three visual scene frames with empty image cards and independent action notes; no grouping, timing or slide semantics |
| User Journey Map | Three visual stage frames and independent prompts for user actions, feelings and opportunities; no stage membership or timeline semantics |
| Moodboard starter | Six independent empty image cards and an ideas note; no automatic collage layout |

Canvas shapes, connectors and frame tints are now painted by the editor and
the SVG/PDF scene renderer. Interactive authoring and complete export parity
remain in progress; card colours are also written to the topic fill for
compatibility with the topic renderer.

**Later, each with the capability it waits for:**

| Template | Needs |
|---|---|
| Lean Canvas, richer Empathy map | Text-heavy responsive regions |
| Richer journey maps, Roadmap | Structured axes or lanes |
| Richer Moodboard | Scalable resource storage and richer image arrangement |

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

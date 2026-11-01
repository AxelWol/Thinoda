# Freeform boards: research, concept and plan

Status: **concept and plan only, no code yet** (user-directed, 2026-10-28).
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
- Native (Windows, macOS, Linux) and web (wasm) parity, all 13 locales,
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

## 3. What Thinoda already has

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

Missing today: arbitrary shapes, ink/pen input, free connectors, z-order,
groups, locking, rotation, snapping guides, rootless or multi-root sheets, and
any non-tree template.

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
  so the type invariant holds, but it carries no content. Validation forbids
  title, notes, tasks, labels, images, relationships, summaries, boundaries,
  numbering and presentation sources on the anchor.
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
| Frame / area | **Zone** extended with `show_frame`, `clip`, title style | Zones already own floating trees and are Pitch sources |
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
  On pointer-up the stroke is simplified (Ramer–Douglas–Peucker plus smoothing),
  validated and committed as **one** edit.
- Stroke points are stored as shared immutable data (`Arc<[Point]>`), so the
  snapshot undo copies references instead of point arrays. If benchmarks still
  fail, canvas edits get delta undo.
- Undo gets a byte budget in addition to the 300-step limit.
- Hard limits (validated on load and paste): objects per sheet, points per
  stroke, total points, coordinate and size ranges, label length, group depth,
  decoded image size.
- Rendering caches tessellated strokes per zoom bucket; off-screen strokes are
  culled.
- F0 benchmark gate: 1,000 and 10,000 strokes, plus a 50-image board, on native
  and web: frame time, memory, undo memory, save/load time, recovery journal
  size.

### 5.5 Input devices

- Mouse and touch drawing work everywhere. Pressure is **optional metadata**:
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
- Inspector sections for card colour, shape fill/stroke, ink width/colour/
  opacity, rotation, lock, alt text.
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
| SVG, PNG, JPEG, PDF, print | Everything visible, via the shared scene |
| PPTX | Scenes become slides; objects as pictures or native shapes (later) |
| XMind | Cards as floating topics; shapes/ink/connectors reported as lost, or refused if the user selects strict mode |
| FreeMind, OPML, Markdown, text, DOCX, XLSX, CSV, MindManager, TextBundle | Outline projection of cards and topics; drawn objects reported as lost |
| FIM | Unchanged; board content reported as lost |

## 6. Phased plan

Each phase ends with its own rubber-duck review, tests for native and web, all
locales, Light/Dark, accessibility checks and updated user docs.

| Phase | Content | Acceptance |
|---|---|---|
| **F0 Groundwork** | Root query APIs (5.1) and migration of all root callers; derived Scene with bands; typed IDs and `bump_next_id` coverage; limits and validation; `required_native_version`; central export capability analysis; ink/undo benchmark | No behaviour change for existing maps (full test suite, golden exports unchanged); contract tests prove the anchor never appears in render, search, outline, AI, a11y, statistics, export or presentation; benchmark report checked in |
| **F1 Flexible roots** | `SheetRootMode::Hidden`, multi-root via floating trees and zones, mode switch commands, export projection policies | Rootless and multi-root sheets round-trip in native v6; every export shows the projection; v5 readers refuse v6 files |
| **F2 Cards and frames** | Card presentation for floating topics (sticky, text, image), zone frames, grid, snapping and guides, align/distribute | Sticky wall usable by mouse, touch and keyboard; screen-reader labels; SVG/PDF show cards and frames |
| **F3 Shapes and connectors** | Shape library (basic + flowchart), connectors with typed endpoints, z-order within bands, groups, lock, rotation | Flowchart can be built and exported to SVG/PDF; endpoint deletion behaves as specified; undo of group operations |
| **F4 Board templates** | Data-driven template definitions and the template MVP (section 7) | Templates localised in all 13 locales, theme-aware, validated in tests |
| **F5 Ink** | Pen, highlighter, object eraser, lasso, simplification, caches | Benchmark gate from F0 met on native and web; no pressure dependency |
| **F6 Scenes and polish** | Viewport scenes in Pitch and PDF, mini-map, pressure (if the spike succeeds), pixel eraser, more templates | Scenes play and export; pressure optional |

Not in this package: real-time collaboration (R12), comments with identities,
boolean shape operations, handwriting recognition, tables, board library
management.

## 7. Non-mind-map templates

A new gallery category **Boards**. Templates become data-driven (versioned,
localised strings, theme-aware colours, validated) instead of growing the
hard-coded switch in `templates.rs`.

**Template MVP (F4):**

| Template | Built from |
|---|---|
| Blank canvas | Hidden root, grid on, no content |
| Brainstorm sticky wall | Hidden root, one frame, a few sample stickies |
| Retrospective (Went well / To improve / Actions) | Three frames with headers, stickies |
| Business Model Canvas | Nine frames with headers and hint text |

**Later, each with the capability it waits for:**

| Template | Needs |
|---|---|
| Kanban board | Card movement between frames, column semantics |
| Affinity diagram | Frames + stickies (F2); AI clustering optional |
| Lean Canvas, Empathy map | Frames (F2), text-heavy responsive regions |
| Eisenhower matrix, SWOT board | Frames (F2); Matrix structure exists for tree variants |
| Flowchart, Swimlane | Shapes and connector routing (F3) |
| User journey map, Roadmap | Structured axes or lanes |
| Storyboard, Moodboard | Image cards, scalable resource storage |
| Venn diagram | Shape opacity and intersections |
| Multi-root comparison | Multi-root sheets (F1) |
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

_Last updated: 2026-11-06 07:56:21 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# draw.io (diagrams.net) support: research, concept and phase plan

Status: 🟡 in progress — Milestone A (codec, import, export) landed 2026-11-05; see [§4.1](#41-implementation-progress). Tracked in [BACKLOG](BACKLOG.md#p1--drawio-diagram-support-user-request-2026-11-05) and [PLAN](PLAN.md#drawio-diagram-support-2026-11-05).

## 1. Problem & goal

Users want to open, edit, save, import, export and copy/paste draw.io diagrams in
Thinoda, without leaving the app and without breaking existing documents, formats or
exports. "Full support" is defined measurably by three tiers, tracked in a published
**support matrix** (`docs/DRAWIO.md`) with release gates:
- **Lossless**: 100 % of cells/attributes/styles round-trip for every file (gate: all
  corpus + fuzz-derived files).
- **Rendered**: all core mxGraph handlers, all 34 markers, all edge styles, all bundled
  generic libraries render faithfully (gate: ≥ 98 % of handlers used by bundled libraries
  and core sidebar palettes; visual diff vs. draw.io reference SVGs within tolerance).
- **Editable**: every rendered element editable via Thinoda UI (gate per phase).
Anything outside a tier is reported in diagnostics as "preserved, shown approximately".

**Confirmed decisions (user):**
1. draw.io content is **converted into Thinoda's native board model**; core is extended
   (waypoints, ports, layers, dash/fonts, generic stencil shapes …) and anything not yet
   modeled is kept in a **lossless passthrough envelope** so it survives round-trips.
2. **`.drawio` is a first-class editable format**: Open edits in place, Save writes
   `.drawio` back; loss preflight only when something cannot be represented.
3. **Stencils**: bundle Apache-2.0 generic libraries; vendor icon sets (AWS, Azure, GCP,
   Cisco, …) via an **optional, user-triggered library download**, faithful placeholder
   until available.

## 2. Research summary

### 2.1 Live investigation (app.diagrams.net, build 32.6.10)
- Compression verified from `Graph.compress/decompress`:
  `base64( deflateRaw( encodeURIComponent(xml) ) )`; decode reverses and applies
  `zapGremlins` (strip control chars). Optional zlib variant (`deflate`) exists.
- 832 registered shape handlers (`mxCellRenderer.defaultShapes`), ~200 sidebar palettes.
- 34 arrow markers: classic, classicThin, block, blockThin, open, openThin, oval, baseDash,
  doubleBlock, diamond, diamondThin, manyOptional, dash, box, cross, circle, circlePlus,
  halfCircle, async, openAsync, mermaidExtension, mermaidDiamond, ERone, ERmandOne, ERmany,
  ERoneToMany, ERzeroToMany, ERzeroToOne, sysML* (6).
- Edge styles: orthogonal, elbow, entityRelation, loop, sideToSide, topToBottom, segment,
  isometric, sequence (+ `curved=1`, straight = none).
- Perimeters: rectangle, ellipse, rhombus, triangle, hexagon(2), center, lifeline,
  orthogonal, backbone, callout, parallelogram, trapezoid, step, cloud, outline, …
- Default vertex style: `shape=label; perimeter=rectanglePerimeter; fontSize=12;
  fontFamily=Helvetica; align=center; verticalAlign=middle; fillColor/strokeColor/fontColor=default`.
  Default edge: `shape=connector; endArrow=classic; rounded=1; fontSize=11;
  labelBackgroundColor=default`.
- **Colors may be `default`, `none`, `#rrggbb`, or `light-dark(a,b)`**; the model has an
  `adaptiveColors` setting (dark-mode adaptation).
- Named stylesheet entries: text, edgeLabel, label, icon, swimlane, group, ellipse, rhombus,
  triangle, line, image, roundImage, rhombusImage, arrow, fancy, color presets, plain-*.
- Sketch mode (`sketch=1`, rough.js, `fillStyle` hachure/solid/zigzag/cross-hatch/dashed/
  zigzag-line), `fontStyle` bitmask bold=1 italic=2 underline=4 strike=8.
- Export surface: PNG/SVG/PDF/JPEG/VSDX/HTML/XML/URL, animated GIF; embedded-diagram PNG via
  `zTXt` chunk keyword `mxfile` (legacy `tEXt`), SVG via root `content` attribute.

### 2.2 File format essentials (jgraph/drawio + mxgraph, Apache-2.0)
- Containers: `.drawio`/`.dio`/`.xml` (`<mxfile host agent version etag modified type
  compressed>` with N `<diagram id name>` pages, each compressed text **or** literal
  `<mxGraphModel>`); legacy bare `<mxGraphModel>`; `.drawio.svg`; `.drawio.png`;
  `.drawio.html`; PDF with embedded XML; `#R<compressed>` URLs.
- `mxGraphModel` attributes: dx, dy, grid, gridSize, guides, tooltips, connect, arrows,
  fold, page, pageScale, pageWidth, pageHeight, background, backgroundGradient, math,
  shadow, adaptiveColors, backgroundImage.
- Cells: id 0 = root, id 1.. = layers (children of 0, `visible`, `locked` style), vertices
  & edges with `parent` (layer/group/container/edge for labels), `source/target`,
  `collapsed`, `connectable`, `visible`. Geometry: x/y/w/h, `relative`, `sourcePoint/
  targetPoint/offset`, `<Array as="points">` waypoints, `alternateBounds`.
- Edge labels = child vertices of the edge (relative geometry: x ∈ [-1,1] along edge +
  pixel `offset`).
- `<UserObject>`/`<object>` wrappers: `label`, arbitrary custom attributes (Edit Data),
  `placeholders="1"` with `%name%`, `tooltip`, `link` (URL, `data:page/id,…`, actions),
  `tags`.
- Style string: `[stylename|key=value];…` — leading tokens are named styles, `none`
  removes inherited keys; unknown keys must be preserved.
- Stencils: `shape=mxgraph.<ns>.<name>` resolved from XML stencil sets
  (`<shape><background><foreground><connections>` path language) or JS-coded shapes;
  `shape=stencil(<deflate-base64 XML>)` inline stencils.
- Security: `html=1` labels are untrusted HTML (sanitise / convert, never render raw);
  deflate/base64 bombs; deep nesting; huge cell counts; external image URLs (no auto-fetch).

### 2.3 Thinoda today (verified in code)
- Board model (`crates/thinoda-core/src/board.rs`): `CanvasObjectKind = Shape | Ink |
  Connector` (331-341); 29 `ShapeKind`s; `ObjectStyle {fill, stroke, text_color,
  stroke_width, opacity}` (218-242); connector `{from/to: Topic|Object|Point, route:
  Straight|Elbow|Curve, waypoints, arrow_start/end: bool, label, color, width}`; flat
  groups (no nesting), lock flags, single `CanvasLayer` with `Band::{Below,Above}` paint
  bands; cards (Sticky/Text/Image) are floating topics; zones are sheet-level.
- **Missing for draw.io**: exact routing, ports/fixed connection points, arrow-type enum, dash
  patterns, fonts/size/alignment per shape, gradients, shadows, rounded/arcSize params,
  flip, named layers (visibility/lock), nested groups/containers, swimlanes/tables, free
  text & image objects, hyperlinks/custom properties/tooltips on objects, stencil shapes,
  rich labels on shapes, line jumps, unknown-data preservation (serde ignores unknown).
- I/O (`crates/thinoda-io/src/lib.rs`): central `Format` enum with IMPORTABLE/EXPORTABLE,
  `from_path` (extension only → `.drawio.svg` double extensions need special handling),
  `detect_format` (`.xml` is sniffed for FIM first, then falls back), `ImportReport` +
  `ImportWarning` consent flow, `ExportLoss` preflight (`board_export.rs`),
  `NATIVE_FORMAT_VERSION = 10` with `required_native_version` gating.
- Deps already present: `quick-xml 0.42`, `base64 0.23`, `image` (png/jpeg), `resvg 0.48`.
  Raw-deflate/zlib is only *transitive* (via `zip`) → a **direct** dependency (e.g.
  `miniz_oxide`) must be added, wasm-checked and license-noted.
- Document lifecycle (`crates/thinoda-app/src/doc.rs`): `is_native` = Thinoda|XMind
  (20-21) → only these are writable in place; other imports are conversion-only with the
  source path protected (`from_import_report` 263-295); `prepare_save` (614-625) allows
  native + MindManager. **XMind is the precedent for a writable foreign format.**
- `import_read_policy` (lib.rs 495) bounds only known formats; others fall back to
  unbounded `fs::read` (570). Existing XML importers reject DOCTYPE (fim.rs 190,
  mindmanager.rs 420).
- `ImportWarning` is a small `Copy` enum mapped exhaustively in ui.rs (~7439) and
  cloud_program.rs (2172-2179) — no room for counts/details/actions.
- Canvas and IO have **separate** board renderers (thinoda-canvas/src/render.rs ~1080/1178,
  thinoda-io/src/render/board.rs ~184), both calling `unit_outline` — not one shared path.
- Undo memory accounting (thinoda-core/src/document.rs 161-223) and `mask_canvas`
  (board.rs 1294-1307, used by `Workbook::masked_copy`) only know today's text fields.
- Copy/paste assigns fresh native ids (thinoda-core/src/clipboard/board.rs 28-29).
- Groups are flat; objects have a single global `rect`; no parent hierarchy.
- UI: `board_ui.rs` (shape library 1395, connector controls 1038, route tiles, color/width
  pickers), `menus.rs`/`native_menus.rs`, `export_preflight.rs`, `board_clipboard.rs`
  (board payload + HTML/plain representations), `i18n.rs` (11 languages incl. Auto).
- CI: fmt, clippy (`-D warnings`, also wasm32 for web crates), tests, importer mutation
  tests, fuzz smoke (XMind/FreeMind/OPML), Playwright browser tests.

## 3. Concept

### 3.1 Architecture

```mermaid
flowchart LR
  subgraph thinoda-io
    C[drawio::container<br/>mxfile / svg / png / html / url] --> X[drawio::codec<br/>bounded XML + inflate/base64]
    X --> M[drawio::mx<br/>typed mxGraph AST + StyleMap]
    M --> I[drawio::import<br/>AST → Sheet]
    E[drawio::export<br/>Sheet → AST] --> M
    S[drawio::stencil<br/>parser + resolver] --> I
  end
  subgraph thinoda-core
    B[versioned board model<br/>+ DiagramExt passthrough]
    G[stencil geometry<br/>path language]
  end
  I --> B
  B --> E
  S --> G
  subgraph thinoda-canvas / app
    R[render + edit tools] --> B
    UI[palettes, inspector,<br/>layers, preflight]
  end
```

- New module tree `crates/thinoda-io/src/drawio/` (container, codec, mx, style, import,
  export, stencil, sanitize, tests) — mirrors existing `xmind.rs`/`mindmanager.rs` pattern.
- **Typed mxGraph AST** (`MxFile → MxDiagram → MxGraphModel → MxCell{id, parent, value:
  Label|UserObject, style: StyleMap, geometry, …}`) is an internal intermediate; it is
  *not* persisted. `StyleMap` keeps **ordered** key/value pairs + leading named tokens so
  export can reproduce untouched styles in their original order.

#### 3.1.1 Round-trip ownership contract (resolves passthrough lifecycle)
Three storage levels, each with a single authoritative owner:
| Level | Type | Contents |
|---|---|---|
| Workbook | `DrawIoDocumentExt` (new optional `Workbook` field) | mxfile attrs (host, agent, version, etag, type, compressed), container kind (`.drawio`/`.drawio.svg`/`.drawio.png`), compression policy, page order, unknown top-level children, source fingerprint |
| Sheet | `DrawIoPageExt` | `<diagram id name>`, mxGraphModel attrs not first-class, unknown model children, **tombstones** of deleted source cell ids |
| Object | `DrawIoCellExt` | `source_id`, `style_rest` (ordered unmapped keys), `attrs_rest` (unknown attrs/children, bounded), **owned-key mask** |

Rules (each backed by a test):
1. **Ownership table** per mapped style/attribute (`fillColor→style.fill`, `gradientColor→
   style.gradient`, …) is code, not convention. A key is either first-class *or* in
   `style_rest`, never both (validated). Editing a first-class field never touches
   `style_rest`; first-class fields that become unset are emitted as draw.io defaults.
   Keys whose meaning *depends* on an owned key (e.g. `gradientDirection` when gradient is
   removed, `arcSize` when `rounded=0`) are listed in the table as **dependents** and
   dropped together with their owner.
2. **Ids**: export uses `source_id` when present and unique; new objects get fresh ids
   (`<sheet-prefix>-<n>` collision-checked against all cells and tombstones). Copies keep
   `style_rest`/`attrs_rest` (unknown styling survives) but **clear `source_id`**, except
   one case: the board clipboard payload gains a provenance envelope
   `{ document_uuid, sheet_id, transfer_token, disposition: Copy|Cut }`; `source_id` is
   kept **only** when a completed Cut is pasted once into the same document (token
   matches, not yet consumed). Duplicate, Copy, stale/second paste of a cut, OS-serialized
   payloads and cross-document pastes all clear it. Tests for each case.
3. **Deletion** removes the object and its passthrough; its id goes into tombstones (only
   to avoid id reuse). Unknown cells that could not be mapped at all are imported as
   **opaque preserved objects** (selectable, movable, deletable placeholder with bounds) —
   never invisible hidden data — so the user can see and delete them.
4. **Reparent/group/ungroup** re-computes geometry in the new parent's space (§3.2.1); the
   passthrough follows the object.
5. **Undo**: passthrough blobs are immutable `Arc<…>`, shared across snapshots and counted
   once in history memory accounting.
6. **Privacy**: `masked_copy`/`mask_canvas` masks labels, tooltips, links, properties,
   HTML labels and drops `attrs_rest` text content.
7. **Budgets**: cumulative caps per object, sheet and workbook for passthrough bytes;
   exceeding them is an import diagnostic, not silent truncation.
8. **Non-draw.io documents** never get these fields; a native sheet saved to draw.io gets
   fresh ids and default envelope.

- **Native format gates:** v10 covers connector waypoints, v11 covers open and
  block connector markers, v12 adds classic and thin markers, v13 adds marker
  sizes, v14 adds marker fill overrides, v15 adds diamond markers, v16 adds
  oval markers, and v17 adds half-circle markers. Every later field or
  passthrough representation that a v17 reader cannot preserve must require a
  higher native version, with
  `required_native_version` choosing the oldest version that can represent the
  complete workbook. Older builds must reject that version instead of silently
  dropping data. Version sheet/board clipboard, recovery and private XMind
  envelopes separately whenever an older reader could otherwise ignore new
  data; reject forged older envelopes too.

### 3.2 Core model extensions (all `#[serde(default, skip_serializing_if)]`)

#### 3.2.1 Scene hierarchy & geometry (resolves parent-tree/relative-geometry gap)
- New `parent: Option<SceneParent>` on `CanvasObject` where `SceneParent = Layer(id) |
  Object(id)` (container/group/vertex **or connector** — edge labels and edge-attached
  children). `None` = default layer = today's behaviour.
- `CanvasGroup` stays for Thinoda's existing flat groups (no semantic change). draw.io
  groups/containers are **objects** of kind `Container { style: Group|Container|Swimlane|
  Table|TableRow|Cell, collapsed, alternate_size }`, so nesting is a tree with
  cycle/depth validation (max depth bound). Thinoda's group commands on draw.io sheets
  create `Container{Group}` objects; on non-draw.io sheets they keep using `CanvasGroup`.
- Geometry modes: `rect` stays **sheet-space** for every object (so all existing
  hit-testing, selection, zones and exports keep working); a derived `local_geometry`
  is computed only at draw.io import/export (parent offset composition; relative
  geometry for edge labels/children). Moving a container moves descendants (sheet-space
  update in one undoable command).
- Edge labels: `ConnectorLabel { id: LabelId, text, rich, position ∈ [-1,1], offset,
  style, source_id }` list on the connector (multiple labels, stable ids); the existing
  single `label: String` is the serialized form while there is exactly one unpositioned
  default label (v≤9 byte-identity, 3.2.3 pattern). A label cell with extra
  children/styling that can't map becomes an opaque preserved child.
- Paint order: within a layer, draw.io order = depth-first tree order; Thinoda stores
  objects in that order inside `CanvasLayer.objects` and z-order commands operate among
  siblings of the same parent (tested).
- Rotation: both systems rotate clockwise around the frame centre (board.rs
  `normalize_rotation`) — direct mapping. `direction` (north/south/east/west) is a
  separate stencil-orientation field applied to geometry, not to the frame; `flipH/V`
  likewise. Perimeter (`perimeter=…`) is a per-shape property used by the router.
- `visible=0` → object `hidden` flag (rendered nowhere, listed in Layers panel);
  `connectable=0` → no ports offered.
- Edge-to-edge connections: `Endpoint::Object` may reference a connector (nearest point
  on its path); edge-to-label connections use new `Endpoint::Label { connector, label:
  LabelId }`. Both validated (no self/cyclic edge chains that cannot be routed),
  round-trip tested with multiple labels and an edge attached to a specific label.

#### 3.2.2 Field extensions
| Area | Extension |
|---|---|
| Shapes | `ShapeKind` stays; add `CanvasObjectKind::Stencil { name: StencilRef, label }` (namespaced id like `mxgraph.flowchart.document`, core JS-shape id, or inline-stencil resource id); built-in draw.io core shapes map to existing `ShapeKind` **only when geometry is pixel-identical** (golden-tested), else to `Stencil`. |
| Text | `CanvasObjectKind::Text` (free text) and `Image { resource | external_url, aspect }`; `LabelStyle { font_family, font_size, bold/italic/underline/strike, align, v_align, wrap, spacing, label_position, background, border }`; rich labels reuse the v8 inline-text model; HTML→rich text mapping table (b/i/u/s/font/color/br/p/ul/ol/li/a/sub/sup) with anything else kept as preserved HTML + plain-text fallback. Fonts: family name kept; missing fonts fall back via existing font stack, noted in diagnostics. |
| Connection style | Per object `ConnectionStyle { perimeter: Option<Perimeter>, perimeter_spacing, ports: PortPolicy (Default per kind / Constraints(list) / None), connectable }`; `None` = default derived from `ShapeKind`/stencil (Phase 1 uses only these derived defaults). Imported non-default values affect routing, hit-testing, export (tested). |
| Style | `ObjectStyle` += `dash: Option<DashPattern>`, `fill_opacity`, `stroke_opacity`, `gradient: Option<Gradient>`, `shadow`, `rounded + arc_size`, `flip_h/v`, `direction`, `fill_style`, `sketch`. Colors: `ThemedColor = Fixed(Color) | Default | None | LightDark(Color, Color)`. |
| Connectors | `waypoints: Arc<[Vec2]>` (stored and interactively editable); `route` += `Orthogonal`, `EntityRelation`, `Isometric`, `Segment`, `Loop`, `SideToSide`, `TopToBottom` (Elbow kept); `start_marker/end_marker: Option<Marker { kind: ArrowKind(34), filled, size }>`; `Endpoint::Port { object, x, y, dx, dy, perimeter }`; labels per 3.2.1; `jump: Option<LineJump>`; dash. |
| Structure | Named **layers** `Vec<BoardLayer { id, name, visible, locked }>` in `CanvasLayer`; objects reference layer via `parent`. |
| Metadata | Per-object `link: Option<Link>` (`Url` w/ scheme allow-list, `Sheet(SheetId)`, `Action(raw)` preserved-only), `tooltip`, `properties: Vec<(String,String)>`, `tags`, `placeholders`. |
| Sheet | `DiagramPage { page_size, page_scale, grid, background, background_image, math, adaptive_colors }` in `BoardSettings`. |

#### 3.2.3 Connector marker migration (compatibility-critical)
- Keep `arrow_start`/`arrow_end` bools **as the serialized form** for legacy
  triangles; add optional `start_marker`/`end_marker` fields for exact draw.io
  shapes. `None` retains the historical Thinoda triangle and is distinct from
  draw.io's notched `Classic`. Custom deserialization rejects inconsistent
  booleans and markers. The eventual in-memory API derives booleans from
  `Marker`; until then both are validated. Files without new marker data retain
  their legacy representation, and older builds reject new marker variants
  through version gates rather than silently ignoring them.
- Same pattern for every other extended field: legacy representation preserved, new
  field present only when it carries information, `required_native_version` detects
  every new field (construct one workbook per new field and assert its minimum
  supported version, plus rejection of forged manifests with older versions).

Mind-map topics/cards remain untouched; draw.io import produces **hidden-root board
sheets** (`SheetRootMode::Hidden`) so mind-map features stay unaffected.


### 3.3 Format detection & file handling
- New `Format::DrawIo` (`.drawio`, `.dio`), `Format::DrawIoSvg` (`*.drawio.svg`),
  `Format::DrawIoPng` (`*.drawio.png`); `from_path` gains a **full-file-name** check
  before the extension check so `x.drawio.svg` != plain SVG export.
- `detect_format`: for `.xml`, keep FIM sniff first, then sniff `<mxfile>`/`<mxGraphModel>`
  root; plain `.svg`/`.png` with embedded `mxfile` content are offered as draw.io on
  **import/open** (sniff), while Export to plain SVG/PNG stays unchanged.
- `import_read_policy` gets explicit draw.io entries (outer file-size limits per container,
  never the unbounded `fs::read` fallback); inflate is bounded by absolute size + ratio.
- Direct dependency `miniz_oxide` (raw deflate + zlib), verified on wasm32, recorded in
  THIRD_PARTY_NOTICES; `cargo deny`/license check updated.
- Read-only (import-only) sources: `.drawio.html`, PDF-embedded, `#R` URLs; `.vsdx`
  deferred (backlog).

### 3.4 UX & UI integration (Thinoda look & feel, not a draw.io clone)

#### 3.4.1 Shape placement & on-canvas editing — draw.io behaviour (user requirement)
Today: picking a shape arms `BoardTool::Shape(kind)`; every click/drag creates a shape,
but selection handles, rotation, label editing and connector creation are suppressed
until the user switches back to Select (`controls()` gates on `BoardTool::Select`,
board_input.rs ~884). New behaviour, for **every board sheet** (not only draw.io), for all
shape-library items (29 built-ins, bundled/vendor stencils, library images):
1. **Pick**: click a library tile → shape is *armed* (tile highlighted; a translucent
   ghost of the shape at default size follows the pointer, snapped to grid). Or **drag**
   the tile onto the canvas → ghost while dragging, created at drop point (drag-drop
   creates exactly one shape and does not change the armed state). Keyboard/AccessKit:
   Enter/Space on a tile inserts at a free spot near the viewport centre (right of the
   current selection if any), selects it and arms the shape (screen-reader announcement).
2. **Hover = edit**: while armed, whenever the pointer is over a shape (the new one *or*
   any existing shape) or over its affordance zone, the ghost hides and that shape shows
   draw.io-style affordances: drag body = move; corner/edge handles = resize; rotate
   handle = turn; double-click / F2 / start typing on the selected shape = edit label;
   four **direction arrows** = click to clone+connect in that direction, drag to draw a
   connector and drop on another shape/port to connect (modifiers as in draw.io:
   Ctrl/Cmd+click = clone only, Shift+click = connector only, Ctrl/Cmd+drag onto empty
   space = connected clone at the drop point); port dots on hover. Clicking a shape
   selects it — it never creates on top.
3. **Click empty space = create**: a click on empty canvas creates another instance of
   the armed shape centred at the (snapped) point and selects it; click-drag on empty
   canvas keeps today's drag-to-size creation.
- **Leave armed mode**: Esc, clicking the armed tile again, choosing another tool, or V.
  A compact hint chip on the board toolbar shows "Click empty space to add · Esc to finish"
  (localised, dismissible, announced once).
- **Hit priority** (tested): handles > direction arrows/ports > object body (incl. small
  hover tolerance) > zones/frames > empty canvas. Topics, cards, ink and frames count as
  "not empty" (click selects, never creates on top).
- **Touch/pen** (no hover): tap empty = create; the created/selected shape shows the same
  affordances; tap a shape = select; long-press on an object opens the context menu as
  today. Direction arrows: tap = default clone+connect; press-and-hold on an arrow
  (0.6 s without moving) opens the quick-shape popover. This replaces the planned
  "more" chevron: the hold is limited to the arrow, so the object long-press context
  menu is unaffected, and the arrows stay uncluttered; tested.
- **Undo**: separate gestures are separate history entries (create, move, rotate, label
  edit). Each **compound quick action** — click-empty create, clone+connect, quick-shape
  insert+connect, drop-on-empty create+connect — is **one atomic edit / one undo entry**;
  acceptance tests assert a single Undo restores the exact prior workbook and selection.
- Sticky/Text/Frame/Connector tools keep their current behaviour (they enter text editing
  or connector drawing immediately); the armed-shape model is reused later for stencils.
- Existing tests that assert "no controls while a creation tool is active" are updated to
  the new contract; new canvas tests cover every row above incl. touch and AccessKit.

#### 3.4.1a Connector & quick-action parity with draw.io (user requirement)
Observed in app.diagrams.net (user screenshots) and to be fully inventoried from the live
app + its shortcuts reference in Phase 0 (each gesture recorded with modifier variants):
- **Shape-aware perimeters**: connectors end exactly on the visible outline (ellipse,
  rhombus, triangle, hexagon, cylinder, parallelogram, callout, stencil outlines,
  rotated/flipped shapes) — today Thinoda clips to the (rotated) bounding rect
  (`clip_to_rect`/`clip_to_rotated_rect`, layout.rs 686/709). New `Perimeter` per
  `ShapeKind`/stencil (draw.io perimeter names), `perimeterSpacing`, used by router,
  hit-testing and export.
- **Floating vs fixed ends**: drop on the shape body/outline → *floating* (end slides
  along the perimeter, outline highlighted); drop on a connection point (×) → *fixed*
  port. Endpoint handles show state like draw.io (filled = connected, outline-highlight =
  floating, × = fixed point); Alt while dragging toggles; connection points (`×`) appear
  on hover over any shape while dragging an end (built-ins: 4/8/16 default points per
  shape kind; stencils: `<constraint>`s).
- **Direction arrows on hover** (light-blue ↑→↓←): click = clone + connect in that
  direction (respecting spacing/grid, avoiding overlaps); drag = draw connector, drop on a
  shape/port to connect or on empty space to create a connected clone; hold/hover on an
  arrow → **quick-shape popover** (recent + related shapes, e.g. circle/rect/ellipse/
  diamond as in screenshot 2) → click inserts that shape connected in that direction.
  Modifier variants follow draw.io's own arrow tooltip (implemented in 1b): click =
  clone + connect (or connect to an existing neighbour in that slot), Ctrl/Cmd+click =
  clone without connector, Shift+click = connector without clone, drag = connector to the
  drop target or a free end, Ctrl/Cmd+drag = connected clone at the drop point. Keyboard:
  each arrow is a Tab stop after the selection's resize/rotate handles; Enter/Space (with
  the same modifiers) activates it and keeps focus on the new shape's arrow for chaining.
  Quick-shape popover (1b part 2): resting the mouse 0.6 s on an arrow, holding it down
  the same time (touch), or Shift+F10 on a focused arrow opens a 5×2 tile picker beside
  the arrow — the source's own kind first, then common draw.io shapes; hovering or
  focusing a tile ghosts that shape and its connector at the target slot; click/Enter
  inserts it connected (one undo entry). Arrow keys/Tab move between tiles, Esc closes
  and refocuses the arrow, a click outside closes without acting on the board, and the
  mouse-opened picker closes when the pointer leaves the arrow and picker.
  Alt+Shift+Arrow and similar draw.io shortcuts stay unmapped because Alt+Arrow already
  duplicates in Thinoda.
- **Connect from outline**: hovering near a shape's outline highlights it; dragging from
  there starts a connector (no tool switch). Edge-to-edge connections supported.
- **Selected-edge floating toolbar** (screenshot 1): line style (straight/curved/…),
  end-arrow picker, waypoint style (straight/orthogonal/elbow/curved/entity relation),
  add/edit label (A), "…" opens the full connector inspector. Positioned near the edge
  midpoint, never covering the edge; keyboard reachable; reuses existing board toolbar
  widgets/icons. Selected **shape** quick toolbar likewise (fill, stroke, text, style
  presets, "…"), following the same visual language. *Phase 1d places both in the
  contextual section of the left board toolbar instead; see the Phase 1d progress note.*
- **Virtual bends**: midpoint dots on each segment of the selected edge; drag = create
  waypoint; orthogonal edges drag whole segments; double-click waypoint = remove;
  "Clear waypoints" in toolbar "…".
- **Selection handles** (screenshot 2): 8 resize handles, rotate handle top-right,
  Shift = keep aspect, Alt = from centre, grid/guide snapping with smart guides.
- **Labels on edges**: double-click edge = add label at that position; drag label along
  the path (stores position ∈ [-1,1] + offset).
- **Reconnect**: drag an end from one shape to another; dragging a connected shape keeps
  edges attached and re-routes.
- **Deletion semantics** (Phase 8): *Delete* keeps today's behaviour — incident
  connectors survive with ends converted to free points (board.rs ~1855-1913); *Delete
  all* (Ctrl+Delete, as draw.io) also removes incident connectors, transitively removing
  edges attached to removed edges/labels; locked items block the command with a message;
  one atomic undo entry. Command tests for incoming/outgoing/shared/edge-chain cases.
- Every action is undoable, localized, AccessKit-named and keyboard-reachable; touch uses
  the same affordances on the selected item (no hover needed).

#### 3.4.2 Files & dialogs
- **Open/Import**: draw.io files in the normal Open dialog filter ("draw.io diagram"),
  drag-and-drop onto window/web drop zone, recent files with a draw.io badge. Multi-page
  files become sheets (tabs) with original page names.
- **Import report**: structured `ImportDiagnostic { code, severity, count, page,
  examples, preserved: bool, action: Option<DiagnosticAction> }` (e.g. "3 AWS icons shown
  as placeholders — *Download AWS library*", "2 math labels shown as source"). The coarse
  `ImportWarning` is *derived* from diagnostics, so the existing exhaustive matches
  (ui.rs, cloud_program.rs) keep compiling. Transport: `ImportReport` gains
  `diagnostics: Vec<ImportDiagnostic>`; `OpenDoc::from_import_report` stores them in a
  transient `pending_import_diagnostics` consumed by the report dialog (not persisted);
  identical path for desktop, wasm, cloud/VFS loaders; `DiagnosticAction` dispatches an
  app `Cmd` (e.g. open Libraries manager). Tests per loader + action callback.
  **Preserved-but-unrendered** items are
  informational (no destructive-consent prompt); only real data loss requires consent.
- **Save loss preflight**: reuse `export_preflight.rs`; lists only items that cannot
  round-trip (mind-map topics → boxes+edges, ink → freehand path stencils, …); one-click
  "Save as .thinoda instead".

#### 3.4.3 Editing surfaces
- **Shape library**: extend `shape_library` with sections (Thinoda, General, Flowchart,
  Basic, Arrows, UML, BPMN, ER, Network, Mockup, Tables/Containers) + "More libraries…"
  manager; search over names/tags; recents row; keyboard navigable; tiles render real
  stencil geometry; tiles are drag sources (3.4.1).
- **Inspector**: extend board context controls in the same visual language: Style,
  Text, Arrange (position/size/rotation/flip/z-order/layer), Connector (route tiles,
  34-marker grid, sizes, line jumps, waypoint reset), Data (link, tooltip, properties,
  tags). Advanced: "draw.io style" text field, validated on apply, writes through the
  ownership table (3.1.1).
- **Connectors**: waypoint handles (drag to add/move, double-click remove), ports on
  hover, fixed vs floating (Alt toggles), Tab between ports.
- **Layers panel**: compact side panel (View menu + board toolbar), visibility/lock,
  rename, reorder, move selection to layer; hidden when only the default layer exists.
- **Clipboard**: copy also as draw.io XML; paste of draw.io XML / compressed payloads /
  `#R` links inserts at cursor; size-capped like existing board clipboard payloads.
- **Dark mode**: `default`/`light-dark()` resolve against Thinoda's Light/Dark/Auto theme.
- **Accessibility**: localized accessible names for tiles, arrows, ports, markers; all
  controls keyboard-operable; announcements for create/connect/layer changes.
- **Localization**: every new string in `i18n.rs` for all 10 languages.
- **Web parity**: same flows in wasm (file picker/download, drag-drop, clipboard API).

### 3.5 Editable-format document contract (doc.rs)
- `is_native` stays Thinoda|XMind; new `DocFormat::is_editable_foreign()` = XMind|DrawIo*
  used by `from_import_report` (keep source path only for editable formats),
  `prepare_save`/`finish_save` (write same container + compression policy from
  `DrawIoDocumentExt`), autosave & crash recovery (recovery snapshots retain a
  versioned native workbook and restore origin metadata), cloud/VFS save paths.
- Source fingerprint (size+mtime+hash) captured on open; Save detects external change
  and offers *Overwrite / Save as… / Reload*.
- Mixed workbooks (user adds a mind-map sheet to a draw.io document): sheets are
  exported per 3.4.2 preflight; nothing silently dropped.
- Embedded containers (`.drawio.svg/.png`) re-render their image part on save.
- Tests: every other import format still opens protected/conversion-only; XMind
  behaviour unchanged; save-reopen-save is stable (idempotent bytes for uncompressed).

### 3.6 Security acceptance criteria
- Reject DTD/entities (same as fim.rs/mindmanager.rs); bounded XML depth/attr/text sizes.
- Cumulative caps: cells, pages, passthrough bytes, data-URI bytes and inline-stencil
  bytes checked before allocation (declared/encoded lengths). **Inflate** is incremental
  into fixed-size chunks with a hard output ceiling and ratio check enforced *during*
  decoding (checked arithmetic, shared budget across nested containers e.g. PNG zTXt →
  mxfile → compressed diagram); tests assert abort before a measured peak-memory bound
  (native + wasm).
- HTML labels parsed into an inert internal rich-text representation; unknown markup kept
  as escaped text; never rendered as HTML, never scripts/styles/iframes.
- Embedded SVG images sanitized (no scripts/external refs) or rasterized via resvg with
  external loading disabled; never auto-fetch images, fonts or libraries.
- Link scheme allow-list (http, https, mailto, internal page links); `data:action/…` and
  others preserved but not executable; opening links asks as today.
- Adversarial corpus (zip-bomb deflate, billion-laughs, deep nesting, huge style strings,
  cyclic parents, id collisions) in unit tests + `drawio_import` fuzz target in CI smoke.

### 3.7 Non-breaking guardrails
- All model additions optional/defaulted; existing documents serialize byte-identically
  (golden tests over all existing fixtures). Each new version is required only when
  its new data is present.
- `Format` additions don't change existing extension mapping (sniffing only on open).
- **Exhaustive-consumer inventory** (Phase 0) of `CanvasObjectKind`, `ShapeKind`,
  `Endpoint`, `ConnectorRoute`, `Format`, `ExportLoss`, `ImportWarning`, `BoardTool`;
  each phase that extends one updates all listed consumers.
- Rendering is not shared today (canvas render.rs vs io render/board.rs) → Phase 4
  introduces renderer-neutral scene primitives used by both; parity tests compare canvas
  scene, SVG/PNG/PDF/print/presentation/thumbnail, hit-testing and bounds.
- Undo memory accounting counts `Arc` passthrough once; `masked_copy` masks new text.
- In-progress UI stays behind an internal setting until its phase completes; each phase
  ships green CI.

### 3.8 Licensing
- draw.io/mxgraph and generic stencils: Apache-2.0 → THIRD_PARTY_NOTICES; only stencil
  XML data is bundled, Rust code written from spec; per-library provenance manifest
  (source repo commit, file SHA-256, license per file).
- Vendor sets (AWS, Azure, GCP, Cisco, IBM, …) not bundled; optional user-triggered
  download from pinned URL + SHA-256, vendor terms shown first; PRIVACY.md updated.

## 4. Phase plan

Each phase: implement → tests → CI gates (fmt, clippy native+wasm32 `-D warnings`, tests,
importer mutation tests, fuzz smoke, browser tests when UI touched) → written critique →
commit with explicit pathspecs (never `git add -A`).

| # | Phase | Depends on | Outcome |
|---|---|---|---|
| 0 | Foundations | — | ADR/issue, corpus, provenance, consumer inventory, benchmarks, docs |
| 1 | Shape placement, quick actions & perimeters | 0 | draw.io-style pick / hover-edit / click-to-create, direction arrows, quick-shape popover, floating toolbars, true-outline connectors |
| 2 | Codec, AST & security | 0 | Bounded read/write of every container to typed AST |
| 3 | Core scene model & round-trip contract | 0 | Hierarchy, version-gated fields, ownership/passthrough, marker migration |
| 4 | Renderer-neutral primitives & parity | 3 | One scene description for canvas + all exports |
| 5 | Stencil engine & generic baseline | 3, 4 | Stencil parser, core JS shapes, constraints → ports, bundled generic libs |
| 6 | Import MVP | 2, 3, 4, 5 | Open `.drawio` → board sheets, diagnostics, page links |
| 7 | Editable document contract & save-back | 6 | Save `.drawio` in place, preflight, round-trip guarantee |
| 8 | Connector editing UX | 1, 7 | Waypoints, ports, markers, routes, jumps |
| 9 | Style, text, data UX + library sections | 1, 5, 7 | Inspector, rich labels, links/properties, picker sections |
| 10 | Layers, containers, swimlanes, tables | 7 | Structure editing |
| 11 | Embedded containers & clipboard | 7 | `.drawio.svg/png` read+write, html/pdf/`#R`, clipboard interop |
| 12 | Vendor library download | 5, 9 | Optional libraries manager |
| 13 | Advanced fidelity | 7 | Sketch, math, placeholders, adaptive colors, page view, tooltips |
| 14 | Hardening & release | all | Perf budgets, fuzz, a11y, i18n, docs |

**User-directed continuation:** update the target-specific canvas context menus so
Image card is in Tools, Zones immediately follows Styles, and Board is followed
by separate Grid and Align menus. Cover placement, command availability, dispatch,
localization and short-window scrolling. In parallel, resolve the reported
Windows pen-ink interruption and add an atomic SVG representation alongside the
existing Copy Sheet data where the platform supports it. Rebuild and launch the
current Windows app for review after these changes; record platform and physical
device limitations rather than claiming unverified acceptance.

**Context-menu slice — critique:** Tools now includes Image card alongside the
other drawing and card tools; applicable target menus place Zones after Styles
and Grid and Align after Board. The existing commands, icons and target-specific
availability are reused. The extra groups make some menus longer, so the existing
scrolling and localized short-window checks remain important. Blank canvas,
topics and board objects have different applicable actions; Zone and annotation
menus retain their specialized layouts. Physical touch and assistive-technology
acceptance are still open.

Phase 1 is independent of draw.io file support and delivers the requested interaction
change early; it only uses today's `Endpoint::Object` connections (port-precise
connections arrive in Phase 8 without changing the gestures).

### Phase 0 — Foundations (`p0-foundations`)
- GitHub issue + ADR (CONTRIBUTING requires discussion for major formats/deps/assets).
- Fixture corpus `crates/thinoda-io/tests/fixtures/drawio/` authored in app.diagrams.net
  with own content (all containers, compression modes, legacy model, multi-page, layers,
  groups, containers, swimlane, table, all 34 markers, all edge styles, waypoints,
  ports, edge labels, UserObject/properties/placeholders, links, html, math, sketch,
  light-dark, data-URI + external images, inline stencil, vendor shape).
- Provenance manifest for bundled stencil libraries; exhaustive-consumer inventory;
  **interaction inventory** of draw.io (every hover/drag/click/modifier/keyboard gesture
  for shapes, edges, arrows, popovers, toolbars — screenshots + notes) as acceptance
  checklist for Phases 1 and 8;
  benchmark targets (e.g. 5k cells / 5 MB: open < 1.5 s desktop, < 4 s web; save < 1 s;
  peak memory < 3x file+model; pan p95 frame < 16 ms desktop / < 33 ms web).
- `docs/DRAWIO.md` + BACKLOG/PLAN entries + feature→field→phase mapping appendix.

### Phase 1 — Shape placement, quick actions & perimeters (`p1-shape-placement`)
- Implement 3.4.1: armed state + ghost preview, tile drag source/drop target (desktop +
  web), keyboard insert-at-free-spot, hover affordances while armed (lift the
  `BoardTool::Select` gate in `controls()` for armed shape mode with the hit-priority
  table), click-empty create, exit rules, hint chip, touch behaviour, undo granularity.
- From 3.4.1a, everything that needs **no model change**: shape-aware perimeters for the
  29 built-in `ShapeKind`s (derived defaults, nothing persisted), direction arrows
  (click/drag/drop variants), quick-shape popover, connect-from-outline, edge-to-edge drop
  onto an edge path (existing `Endpoint::Object` already accepts any object id — verify;
  else deferred to Phase 8), selected-edge floating toolbar (straight/elbow/curve routes
  and start/end arrow toggles that exist today), shape quick toolbar, add/edit the
  existing single edge label (positioned label dragging → Phase 8), reconnect.
- i18n + AccessKit names; update affected tests (board_tests.rs, board_ui.rs
  shape_library tests, fim_authoring) and add new ones; browser test for web drag-drop;
  FREEFORM.md/user docs updated.

### Phase 2 — Codec, AST & security (`p2-codec-ast`)
- `drawio::codec` (bounded raw/zlib inflate via `miniz_oxide`, base64 tolerant,
  percent-decode, zapGremlins; writer identical to draw.io); `drawio::mx` typed AST
  with ordered `StyleMap`; geometry incl. points/offset/alternateBounds.
- `import_read_policy` entries; all 3.6 criteria; `drawio_import` fuzz target.
- Tests: AST round-trip over corpus; golden compressed string from the live app.

### Phase 3 — Core scene model & round-trip contract (`p3-core-model`)
- 3.1.1 + 3.2 in `thinoda-core`: `SceneParent`, containers, edge labels, layers,
  Text/Image/Stencil/opaque kinds, style/connector/metadata fields, `DrawIoDocumentExt`/
  `DrawIoPageExt`/`DrawIoCellExt`, ownership table, tombstones, marker migration.
- `required_native_version` gates every field at the first version that supports
  it: v10 covers waypoints, v11 covers open/block markers, v12 covers classic
  and thin markers, v13 covers marker sizes, v14 covers marker fill overrides,
  v15 covers diamond markers, v16 covers oval markers, v17 covers half-circle
  markers, and subsequent additions need a higher version.
  Reject forged older manifests and version the other persistence envelopes too;
  `validate.rs` checks cycles, depth, layers/ports/parents and ownership.
- Undo accounting, `mask_canvas`, clipboard provenance envelope (3.1.1 rule 2) updated.
- Tests: golden byte-identity of existing fixtures; edit/delete/dup/group/ungroup/
  cross-doc paste/undo/save-reopen lifecycle tests.

### Phase 4 — Renderer-neutral primitives & parity (`p4-render-parity`)
- Scene primitive layer (paths, fills incl. gradient/hatch, strokes incl. dash, markers,
  text runs, images, clips) consumed by canvas and thinoda-io renderers.
- Rendering for every 3.2 field; `ExportLoss` entries for PPTX/other approximations.
- Parity tests canvas vs SVG/PNG/PDF/print/presentation/thumbnails, hit-testing, bounds.

### Phase 5 — Stencil engine & generic baseline (`p5-stencils`)
- Stencil XML path language → primitives; inline `stencil(…)`; `<constraint>` → ports.
- Rust ports of JS-coded core shapes needed by generic libraries (process, document,
  card, tape, cube, step, note, callout, flexArrow, umlActor, lifeline, …).
- Bundled generic libraries (compressed, lazy, size budget in CI); library index.
- Unknown stencil → placeholder (bounds + label + icon), data preserved.

### Phase 6 — Import MVP (`p6-import`)
- Formats/sniffing (3.3); pages → hidden-root sheets; AST → model mapping incl.
  hierarchy, labels, layers, links (page link ↔ `SheetId` map); `ImportDiagnostic`.
- Open dialog filter, drag-drop, recents, web picker; mutation tests + fuzz smoke.

### Phase 7 — Editable document contract & save-back (`p7-save-back`)
- 3.5 in doc.rs; `drawio::export` (ownership merge, stable ids, compression policy,
  page-link rewrite on rename/duplicate/delete); preflight; Save As variants.
- Round-trip property tests: draw.io → Thinoda → draw.io semantically equal for every
  corpus file; Thinoda-native sheet → draw.io → Thinoda stable.

### Phase 8 — Connector editing UX (`p8-connectors`)
- Remaining 3.4.1a items that need further model extensions: fixed connection points (×) incl.
  default per-kind point sets and stencil constraints, floating/fixed toggle (Alt),
  orthogonal segment dragging, all route styles in the edge
  toolbar, 34-marker picker, line jumps, multiple edge labels, smart guides; direction
  arrows/quick-shape popover become port-aware; stencil perimeters; persisted
  `ConnectionStyle` editing; positioned/multiple label dragging; Delete vs Delete all.

### Phase 9 — Style, text & data UX (`p9-style-text-data`)
- Inspector sections, rich label editing on shapes, link/tooltip/properties/tags
  editors, advanced style field, `BoardStyleClipboard` extension, library sections in the
  shape picker (stencil tiles inherit Phase 1 pick/drag/hover behaviour).

### Phase 10 — Layers, containers, swimlanes, tables (`p10-structure`)
- Layers panel; containers (move children, collapse/expand, drop-into highlight);
  swimlane & table editing.

### Phase 11 — Embedded containers & clipboard (`p11-embedded-clipboard`)
- `.drawio.svg` / `.drawio.png` read+write; html/PDF/`#R` import; clipboard copy as
  draw.io / paste from draw.io (native + web), size-capped.
- PDF import must handle both URI-encoded `/Info` `/Subject` metadata and
  `application/vnd.jgraph.mxfile` embedded attachments, as documented in the
  [draw.io file format](https://github.com/jgraph/drawio/wiki/File-Format). Bound aggregate
  cross-reference, object-stream and attachment decoding, including nesting and
  cycles, before using a parser on untrusted files; do not identify metadata by
  scanning arbitrary PDF stream bytes.
- Parser evaluation: `zpdf-parser` 0.18.0 has per-filter and cache limits, but
  its [open and object-resolution paths](https://github.com/Xero-Team/zpdf/blob/8779a86/crates/zpdf-parser/src/lib.rs)
  also perform automatic repair scans and may re-decode streams after cache
  admission stops. Its [cross-reference traversal](https://github.com/Xero-Team/zpdf/blob/8779a86/crates/zpdf-parser/src/xref.rs)
  does not expose a caller-controlled aggregate decoding budget. These controls
  alone do not satisfy the importer's strict, aggregate-bounded requirement.

### Phase 12 — Vendor library download (`p12-vendor-libraries`)
- Libraries manager; pinned manifest (URL, version, SHA-256, license URL); cache dir /
  IndexedDB; placeholders re-render after download; offline; PRIVACY/NOTICES.

### Phase 13 — Advanced fidelity (`p13-fidelity`)
- Sketch (deterministic seed) & hatch fills, math via `thinoda_core::equations`,
  placeholders `%name%`, adaptive colors, page view/background image, tooltips, page links.

### Phase 14 — Hardening & release (`p14-hardening`)
- Benchmarks vs Phase 0 targets, support-matrix tier gates (§1), fuzz CI, a11y audit,
  i18n completeness, user guide,
  CHANGELOG (release-please conventional commits).

### 4.1 Implementation progress

**Milestone A — codec, import and native-model export (2026-11-05).** A vertical slice
of Phases 2, 6, 7 and 11 that maps draw.io onto *today's* board model, before the
Phase 3 scene-model extensions exist:

- `crates/thinoda-io/src/drawio.rs`: bounded XML DOM (no DTDs or unknown entities,
  depth/element/page/size caps), raw-deflate and zlib inflation with an output cap,
  `.drawio` / `.dio` / bare `mxGraphModel` / `.drawio.svg` (`content`) /
  `.drawio.png` (`tEXt`, `zTXt`, `iTXt`) containers and content sniffing.
- Import: pages → hidden-root sheets; nested and relative geometry, groups, swimlanes /
  containers → frames, notes → sticky cards, text, lines, edges with deferred, free and
  labelled endpoints, rotation and `direction`, colours and opacity, links. Anything
  without a native equivalent (dashes, waypoints, rich HTML, images, unknown stencils,
  hidden or orphan cells, custom properties) becomes a placeholder or is dropped and is
  reported through the existing `ImportWarning` consent dialog; the source file is never
  overwritten.
- Export (`File › Export › draw.io` and Save As `.drawio`): uncompressed
  `mxfile` with one page per sheet. Board content round-trips idempotently
  (`thinoda*` style keys carry cards, ink, bands and frame clipping). Mind-map
  topics, relationships, boundaries and summaries are flattened to shapes and connectors;
  the board loss preflight lists *Mind-map topics and links flattened to shapes*, *Topic
  notes, labels, markers or tasks omitted*, *Presentation slides omitted*, inline text
  formatting and board grid/snap settings. Sheets with equations or embedded resources
  are refused, as for other editable formats.
- Opening a `.drawio` file imports a copy (like MindManager): Save asks for a
  destination and cannot overwrite the imported source. In-place, lossless save-back
  needs the Phase 3 ownership/passthrough model and remains Phase 7 work. Autosave stays
  limited to native formats.

At this milestone, Phases 1, 3–5 and 8–14, ink pressure, clipboard interop,
`.drawio.png` drag-and-drop and the Phase 0 fixture corpus remained open.
Subsequent checkpoints below record partial progress.

**UX addition — GitHub corner mark (user request, 2026-11-05).** Like diagrams.net's
lower-right GitHub kitten, Thinoda shows the GitHub mark (Primer Octicons
`mark-github`, MIT; notice in THIRD_PARTY_NOTICES) at the far right of the status bar
and of the home screen's template actions. It is a real button: muted until hovered or
focused, a translated tooltip and accessible name (*Thinoda on GitHub*, all UI
languages) plus the URL, Enter/Space or click opens <https://github.com/AxelWol/Thinoda>
through the guarded URL opener, and it never edits the document. Status bars narrower
than 480 pt omit it so sheet tabs and status controls keep their room; it stays
reachable from the home screen and About. Covered by `tests/github_badge.rs` and
`tests/bottom_bar.rs`.

**Phase 1a — armed-shape placement (draw.io behaviour).** Picking a shape tile now
*arms* it instead of hijacking every click:

- While a shape is armed, hovering existing content (board objects, frame edges and
  title bands, mind-map topics, selection handles) shows the normal Select cursor and
  hover feedback; pressing there selects, moves, resizes or rotates exactly like the
  Select tool. Only a click or tap on empty space (a frame *interior* counts as empty)
  creates the armed shape, as one undoable edit. A translucent, snapped ghost outline of
  the shape follows the pointer over empty space (no crosshair over content).
- Exit rules: Esc or V returns to Select (so type-to-edit cannot start with "v" while
  armed); clicking the armed tile again disarms it. With a board selection, Enter/Space
  edit its label instead of creating another shape.
- A translated hint chip under the toolbar (*Click empty space to add · Esc to finish*)
  explains the mode. It is dismissible for the session, and hidden on canvases narrower
  than 520 pt or shorter than 160 pt. egui has no live-region API, so it is not announced;
  the armed tile's selected state remains the accessible signal.
- Touch: a tap on content selects, a tap on empty space creates; drags behave as in
  Select mode.
- Tests: `armed_shape_*` in `board_tests.rs` (mouse, touch, keyboard, undo
  granularity) and `armed_shape_hint_*` in `board_ui.rs`.

**Phase 1b — direction arrows (draw.io behaviour).** A single, unlocked, ungrouped
shape shows four accent arrows (↑→↓←) just outside its bounds when it is selected or
hovered (arrows hide when the shape is smaller than 40 pt on screen; cards, connectors
and topics get none):

- Click: if another shape or topic sits in the next slot in that direction (one shape
  extent plus an 80 pt gap, draw.io's default edge length), connect to it and select it;
  otherwise insert an unlabelled clone there (stepping further while the slot is
  occupied) connected by a new connector, and select the clone so clicks can chain.
  Ctrl/Cmd+click inserts only the clone; Shift+click inserts only a connector ending in
  the neighbour or a free point.
- Drag: a dashed preview follows the pointer and outlines the target under it. Release on
  a shape/topic connects to it; release on empty space leaves a free end, or with
  Ctrl/Cmd drops a connected clone centred on the (snapped) point (ghost outline while
  dragging). Releasing on the source does nothing.
- Each compound action is one undo entry (`Document::add_connected_canvas_object`).
- Keyboard and assistive tech: the selection's arrows are Tab stops after its resize and
  rotate handles ("Add connected shape above/to the right/below/to the left").
  Enter/Space activate them (Ctrl/Cmd and Shift variants included) and focus the same
  arrow on the result. Hover-only arrows are not focusable and use separate widget ids.
  The quick-shape picker footer (translated) explains the click, modifier and drag
  variants and how to open the picker, replacing the earlier tooltip.
- Touch: tapping an arrow behaves like a click.
- The rotate handle moved to the top-right corner (draw.io's position) so it no longer
  collides with the up arrow.
- Tests: `direction_arrow_*` and `hover_arrows_*` in `board_tests.rs`, and
  `connected_object_is_one_undoable_edit_and_rolls_back_on_failure` in `document.rs`.

**Phase 1b part 2 — quick-shape popover.** Resting the mouse 0.6 s on an arrow,
pressing and holding it (touch path; a release after the hold no longer clicks), or
Shift+F10 on a focused arrow opens an egui popup beside the arrow, aligned to its
direction. It lists ten tiles (the source's own kind first, then rectangle, rounded
rectangle, ellipse, diamond, triangle, hexagon, parallelogram, cylinder, cloud,
document), each an AccessKit button named after the translated shape. Hovering or
focusing a tile paints a ghost of that shape plus a dashed connector at the slot the
arrow would fill. Clicking a tile or pressing Enter/Space inserts the clone with that
shape, connected, as one "Add connected shape" undo entry; keyboard use then focuses
the same arrow on the new shape. Tab/Shift+Tab and the arrow keys move between tiles
(wrapping horizontally, by row vertically). Esc closes and refocuses the arrow; a click
outside closes it without acting on the board; a mouse-opened picker closes when the
pointer leaves the arrow and picker. The picker also closes when the document, sheet,
camera or tool changes or an editor opens. Resting again on the same arrow needs the
pointer to leave it first. Tests: `quick_picker_*` in `board_tests.rs`.

**Phase 1c — shape-aware perimeters.** Free connectors attached to a shape now end on
its visible outline instead of its bounding box, like draw.io's shape perimeters.
`scene::EndTarget::clip` casts a ray from the shape's centre towards the other end and
takes the outermost crossing with the rotated outline from `shape_geometry`. Using the
outermost crossing means concave shapes (star, cloud, callout, plus) end on their outer
edge. Shapes whose box centre is not strictly inside the outline (the right triangle)
cast from the outline's area centroid. Rectangle, process and predefined-process keep
the rotated-rect clip, as do topics, cards, images and text. Straight, elbow and curve
routes all use it. Hit-testing, rendering, SVG/PDF export and template layout go
through `scene::connector_path`, so they all follow. Shapes have no flip fields yet;
flips and stored `Perimeter` overrides arrive with the Phase 3 model. Tests:
`connectors_end_on_shape_perimeters` (every `ShapeKind`, rotated) and
`polygon_clip_takes_the_outermost_crossing` in `scene/tests.rs`.

**Phase 1d — contextual edge and shape controls.** Selecting a single connector or
shape fills the contextual section of the left board toolbar. draw.io shows these on
a floating toolbar near the selection. Thinoda already shows connector controls in the
left toolbar, so shapes use the same place rather than a second floating bar. This
keeps one visual language and one keyboard path, and nothing covers the drawing. A
floating variant can follow if users miss it.
- **Shapes**: "Change shape" opens the shape library with the current shape selected;
  "Fill color" (with "No fill"), "Outline color" (with "Auto") and "Outline width".
  The size, label, description and opacity stay unchanged. Choosing a width while the
  outline is automatic pins the automatic color, because the automatic outline is
  always drawn at 2 px.
- **Shapes and connectors**: "Edit label" starts inline editing, as F2 does. "More
  formatting options" toggles the Format panel, like draw.io's "…".
- The color and width menus are shared with the connector controls. Each change is one
  undo step. Locked or grouped items show the controls disabled. Changes are rejected
  when the document, sheet, selection or revision changed since the frame was drawn.
  Open menus close when the selection changes or label editing starts.
Tests: `shape_context_tests.rs`; `connector_context_tests.rs` still passes unchanged.

**Phase 1e — library drag-and-drop and keyboard insert.** The Shape library opened
from the toolbar now also inserts shapes directly, like draw.io's sidebar:
- **Drag** a tile (mouse or pen) onto the board. The menu closes as soon as the drag
  starts so the board is visible. A snapped ghost of the shape follows the pointer, and
  releasing creates one default-size shape centred on the point and selects it, as one
  undo step. The armed tool does not change. Esc cancels the drag (an armed tool stays
  armed). Releasing over the toolbar, a popup, the minimap or any other overlay creates
  nothing. Touch swipes keep scrolling the grid, so touch inserts by tapping a tile to
  arm it and then tapping empty space.
- **Shift+Enter** on a focused tile inserts the shape at a free spot without arming it.
  The spot is to the right of the current selection (same centre line) or at the
  viewport centre, then the nearest grid cell ring around it that overlaps no topic or
  object (zones and connectors are ignored); the new shape is selected and revealed, so
  repeated Shift+Enter builds a row. *Deviation from 3.4.1:* plain Enter/Space and
  AccessKit activation keep only arming the tile, as Phase 1a and the existing library
  tests require; Shift+Enter is the keyboard counterpart of dragging.
- With a shape armed, Enter/Space on the board (nothing selected) also uses the free-spot
  search instead of dropping onto content at the viewport centre.
- A translated caption under the search field (*Drag onto the board or press
  Shift+Enter to insert*) explains both; it is omitted when the popup is too small, so
  the tiles keep their room. "Change shape" uses the same grid without dragging or the
  caption, and Shift+Enter there still just chooses.
- Tests: `library_drop_*`, `inserted_shapes_land_in_free_space_beside_the_selection` and
  `armed_keyboard_insert_avoids_content_at_the_viewport_centre` in `board_tests.rs`, and
  `shape_insert_tests.rs` (Shift+Enter, plain activation, real mouse drag from the tile).

**Phase 1f — connect from the outline, edge-to-edge and reconnect.** Hovering just
outside a single, unlocked, ungrouped shape (an 8 px screen band beyond the body's hit
tolerance) highlights its outline in the accent colour and shows a crosshair; the shape's
direction arrows stay visible. Dragging from that band draws a dashed connector preview
that behaves exactly like an arrow drag: release on a topic, card, shape or connector to
connect to it, on empty space to leave a free end, or with Ctrl/Cmd on empty space to
drop a connected clone; each is one undo step. A press inside the body still moves the
shape, and selection handles still take priority. With a shape armed the band counts as
content, so it never creates a shape there (no ghost) and a drag from it still connects.
- **Edge-to-edge:** dropping a connector end, arrow drag or outline drag on another
  connector attaches as `Endpoint::Object(connector)` (cycles stay rejected). The end now
  meets the target halfway along its path (`board::polyline_midpoint`, shared with the
  export renderer's label anchor) instead of at its bounding-box centre. Today's straight,
  elbow and curve routes are point-symmetric, so both agree; the change keeps ends on the
  path once Phase 8 adds waypoints.
- **Reconnect:** verified that dragging the existing *Connector From/To* handles onto a
  shape, connector or empty space already re-targets the end as one "Reconnect connector"
  undo step (`connector_creation_and_endpoint_edits_are_undoable`); no change needed.
- Tests: `dragging_from_a_shape_outline_draws_a_connector_and_the_body_still_moves`,
  `outline_drag_onto_a_connector_attaches_edge_to_edge` and
  `armed_shapes_treat_the_outline_band_as_content` in `board_tests.rs`;
  `edge_to_edge_ends_meet_a_bent_target_on_its_path` and
  `polyline_midpoint_follows_length` in core.

**Phase 1e browser drag checkpoint and critique.** A debug-build regression
drags a triangle from the library onto a board with an existing shape in
headless Chromium, Firefox and WebKit. It checks the drop position, selection,
direct-typing label editing including the first character, unchanged tool
behavior, separate undo/redo for insertion and labeling, and no insertion after
Escape or a drop over the toolbar. The keyboard-creation test also clears the
previous sticky selection before arming a shape, matching the selection
contract. Together with the native gesture tests, this closes the previously
missing focused web drag check. The browser test uses a fixed viewport and
coordinates because egui controls are not yet exposed as DOM accessibility
targets; layout changes may require updating it. It exercises synthetic mouse
input, not physical mouse, pen or touch devices. Assistive technology and
broader device, browser and release-build acceptance remain open.

**Phase 11 (clipboard slice) — paste from draw.io, copy as draw.io.** draw.io copies a
selection as `encodeURIComponent(<mxGraphModel>…)` plain text and reads either that form
or raw XML back. Thinoda now matches both directions:
- **Paste:** *Paste* (menu, Ctrl/Cmd+V, context menu) recognises clipboard text that
  starts with `<mxGraphModel`, `<mxfile` or their `%3C`-encoded forms when no Thinoda rich
  data is present. `thinoda_io::drawio_clipboard::decode` runs it through the hardened
  import codec (same 16 MiB cap) and the result lands centred in the visible board area as
  one "Paste from draw.io" undo step, selected. If the importer had to simplify anything, a
  warning toast says so; malformed data shows an error toast and leaves the sheet alone.
  FIM sheets refuse the paste.
- **Copy:** *Edit ▸ Copy as draw.io* (also in the board item context menu and the ribbon's
  Clipboard group) exports the selected board items through the export codec and writes the
  `encodeURIComponent` form as plain text, so pasting in diagrams.net yields editable
  cells. Conversion losses are listed in a persistent, localized warning after the
  clipboard write; pending text edits are committed first, but the conversion does not
  otherwise change the source document. Ordinary *Copy* is unchanged (readable text plus
  Thinoda's private rich format).
- Limits: container membership from draw.io (Thinoda zones) is not part of a board
  clipboard fragment, so pasted children arrive as free items; multi-page `mxfile` text
  pastes its first page.
- Tests: `drawio_clipboard` unit tests in thinoda-io (detection, plain/encoded decode,
  round trip, rejection, JS-compatible encoding) and
  `drawio_clipboard_text_pastes_as_one_undoable_selection`,
  `copy_as_drawio_writes_plain_encoded_model_that_pastes_back` and
  `malformed_drawio_text_reports_an_error_without_editing` in the app.

**Embedded PNG drop checkpoint and critique (partial Phase 11).** Dropping a
file explicitly named `.drawio.png` now opens its embedded diagram through the
same bounded, conversion-only import and warning flow as File › Open, on desktop
and in the browser. The original source is not overwritten. A file with that
suffix but no embedded diagram reports an open error rather than inserting an
image. Ordinary PNG/JPEG drops retain their image behavior, and the image picker
can still insert a `.drawio.png` as a picture. Browser diagram drops use the
existing 64 MiB document-upload limit and durable document staging; image drops
retain their separate 8 MiB per-image limit.

The suffix deliberately disambiguates the drop: an ordinary `.png` containing
draw.io data is still inserted as an image when dropped, but can be imported as
a diagram through File › Open. This is not a general choice dialog or lossless
save-back. Native drop and staging tests cover source
preservation, invalid files, ordinary images and limits; browser tests use
synthetic file drops in Chromium, Firefox and WebKit. They exercise a generated
`zTXt` fixture and a labelled PNG exported from the published diagrams.net editor
(that export uses percent-encoded `tEXt`); codec tests also cover compressed and
uncompressed `iTXt` and Unicode. A filename collision and a leading-dot name
remain document drops even when browser staging changes their suffix. The
published fixture is a single simple shape, not evidence of complete format
fidelity. Physical file-manager and device acceptance remain open.

**Editable image export checkpoint and critique (partial Phase 11).** Export now
offers **draw.io SVG (editable)** and **draw.io PNG (editable)**. Each contains
the current sheet's ordinary visual preview and a draw.io conversion of that
same sheet, embedded in the SVG root's `content` attribute or in a PNG `mxfile`
text chunk. Other sheets are not included. Before choosing a destination the
GUI always discloses this scope and warns that embedded data can extend beyond
the preview and is not redacted; the CLI emits the same disclosure. The existing
conversion-loss report and content safeguards also apply to the exported sheet.
The source workbook and imported source file are not overwritten or marked
saved. These formats are export-only: opening one imports a converted copy, and
neither Save As nor lossless save-back is implemented.

The published diagrams.net editor accepted and allowed editing a generated SVG
and PNG containing a simple shape. Its PNG reader rejected an otherwise valid
PNG when `mxfile` followed `IDAT`; placing the metadata directly after `IHDR`
made it editable. The writer now checks its rendered container, size and PNG
checksums, and a regression asserts that ordering. Compound-extension detection
also preserves support for filenames containing non-Unicode characters. Browser
tests cover disclosure and cancellation, downloads, metadata, source preservation
and re-opening in Chromium, Firefox and WebKit. Their import wait is restricted
to the canvas so a similarly colored sidebar swatch cannot give a false positive.
The stable shortcut-registry contract was also updated to include both new export
commands after a later targeted check exposed the missing entries.
This single-shape external-editor check
does not establish fidelity for complex files, external editors, physical
devices or all existing content; unsupported FIM provenance, equations and
resources can block export, and conversion losses remain possible. Multi-sheet
embedded exports, lossless round trips and save-back remain future work.

**Legacy embedded image compatibility and critique (partial Phase 11).**
Opening SVG diagrams now accepts URI-encoded XML and base64-encoded XML or
URI text, in addition to the previously supported compressed form. Diagram
detection parses the decoded XML root, including declarations, rather than
mistaking similarly named elements for diagrams. PNG import also recognizes
the legacy `mxGraphModel` metadata keyword alongside `mxfile` in `tEXt`,
`zTXt` and `iTXt` chunks; invalid `iTXt` compression flags and methods are
rejected. Existing input and inflation limits still apply. Ordinary `.png`
drops remain images; explicitly named `.drawio.png` drops remain documents.

Synthetic regression cases cover these encodings, Unicode, bare graph models,
invalid metadata and ordinary-image detection. The URI-encoded, four-page
[`svgfile.svg`](https://github.com/jgraph/drawio-diagrams/blob/dev/diagrams/svgfile.svg)
from the published draw.io examples also imported and validated all four pages.
That check establishes compatibility with this container variant, not complete
conversion fidelity: unsupported shapes and styles can still produce warnings.

**Published link import checkpoint and critique (partial Phase 11).** **File >
Import draw.io link…** and the command palette accept a self-contained `#R`
fragment or an HTTP(S) URL containing one. The dialog decodes URI-encoded XML
or compressed diagram data locally, without opening or fetching the URL; it
imports all supported pages into an unsaved, pathless native copy. A link is
limited to 64 MiB, and the existing XML, decompression and page limits apply.
The transient link text is not added to recent files or retained in the dialog's
undo history after import or cancellation. Ordinary paste remains ordinary
paste. When conversion drops or simplifies content, the existing confirmation
previews up to eight drawing labels on the active page and lists the warnings
separately before the copy is created; cancellation preserves the current
document and the link's source is never changed.

The codec tests include a simple link generated by the published diagrams.net
editor with **Publish > Link > Include a copy of my diagram**, plus multi-page,
compressed, malformed and oversized cases. The app tests cover retry, cancellation,
conversion consent, viewport ownership, IME input, size limits and ordinary
paste. Chromium, Firefox and WebKit coverage exercises the command, warning
cancellation, source preservation and a successful converted import without
network requests. This review also caught a duplicate icon beside the existing
file import, an over-expanded input dialog and a misleading empty outline
preview for board-only diagrams; the new command has a distinct link badge, its
editor is height-bounded with nearby actions, and the confirmation separates
drawing labels from warnings. The checks do not establish fidelity for complex
diagrams or every published-link variant:
remote-reference (`#U`) and cloud links, PDF containers, lossless
round-tripping and save-back remain unimplemented.

**Embedded HTML import (partial Phase 11).** File Open, Import and drop accept
`.html` and `.htm` containing one `div.mxgraph` viewer with inline diagram XML.
The source is parsed as inert data, without executing scripts or loading its
viewer, images or remote diagram URLs. The result uses the existing conversion
warnings and opens as a dirty native copy; the HTML source is not overwritten.
Remote-only viewers, missing or malformed metadata, and multiple viewers are
rejected rather than silently fetching or discarding diagrams. Input, HTML
parser memory and element counts, and the existing XML and inflation limits
are bounded. Embedded PDF and lossless save-back remain separate work.

**HTML slice critique.** The published-editor fixture covers an actual inline
viewer, while synthetic cases exercise multiple pages, Unicode and entities,
inert markup, malformed inputs and parser limits. Source protection and the
existing conversion-warning consent are reused rather than introducing a
separate save-back path. Chromium, Firefox and WebKit open and drop the
published fixture, save a native copy, and verify that neither the viewer
script nor an external image is requested. This intentionally supports a
single self-contained viewer, not arbitrary HTML or remote URLs; legacy
semicolonless HTML character references are not guaranteed. The streaming
parser's memory budget is not a global process-memory limit: decoding the JSON
and XML can require additional bounded copies.

**Legacy HTML compatibility and critique (partial Phase 11).** Older viewers
store compressed or URI-encoded diagram data in their first nested `div`
instead of `data-mxgraph`. The importer also accepts an HTML-escaped XML model
there. The parser uses source offsets for the text so character references
split across input chunks are decoded together. It requires explicitly closed
viewer and payload elements and a text-only payload: ambiguous, malformed or
nested markup is rejected rather than reading unrelated page content. Multiple
viewers, decompression beyond the existing limit, and remote-only viewers remain
unsupported. The imported file still opens as an unsaved converted copy with
warning consent and source protection. Synthetic legacy wrappers and an
independently compressed graph exercise both encoding paths. Chromium, Firefox
and WebKit open the published current-format fixture and drop a compressed
legacy viewer, save native copies, and make no external requests. These checks
do not prove compatibility with every historical HTML export or fidelity for
every shape.

**Corpus validation (docs/draw-io-examples).** `crates/thinoda-io/tests/drawio_corpus.rs`
runs every `.drawio`/`.xml` file in the user-supplied example corpus (89 files from
jgraph/drawio-diagrams and the drawio.com docs: templates, floor plans, mockups, AWS/Azure,
BPMN, C4, UML, network and infographic diagrams). For each file it checks format
detection, import with report, `Workbook::validate`, the same source URI-encoded as
a `#R` fragment (identical workbook and warnings), draw.io export → re-import (same page
count, no lost items), native `.thinoda` round trip, SVG export and clipboard decoding,
each under `catch_unwind`. Result: 89/89 pass, no panics. The corpus folder is not
committed (third-party content, ~3 MB); the test skips when it is absent and honours
`THINODA_DRAWIO_CORPUS`. Run: `cargo test -p thinoda-io --release --test drawio_corpus -- --nocapture`.

Gap analysis from the corpus (8,180 vertices, 1,876 edges) sets the fidelity order:
1. **Images** — 200 image cells in 21 files: 32 inline `data:` URIs (PNG/SVG), the rest
   remote or `img/lib/...` built-in libraries. Inline images are importable; remote URLs
   stay labelled placeholders (never fetched).
2. **Unmapped built-in shapes** — partialRectangle, folder, cube, message, delay,
   endState, waypoint, umlActor/Lifeline/Frame, `mxgraph.arrows2.*`, `mxgraph.basic.*`.
3. **Vendor stencils** — floorplan (1,070), mockup (461), cabinets, infographic,
   salesforce, AWS, BPMN, network (Phases 5 and 12).
4. **HTML labels** — near-universal styled spans (Phase 9 rich text).
5. **Custom data** — `object`/`UserObject` attributes such as C4 metadata (Phase 9).

**Inline images (fidelity slice 1).** `shape=image` cells (and `image=` on other vertices)
with a `data:` URI become Thinoda image cards: PNG/JPEG bytes are validated and stored as
sheet resources; SVG (base64 or percent-encoded) is rasterised with usvg at 2× display
size (64–1024 px) using the bundled fonts and a resolver that never reads files or the
network, and SVGZ is refused. The card is transparent (draw.io images have no fill), fits
the picture to the cell unless `imageAspect=0`, and keeps the cell's top-left so edges stay
attached. Remote, `img/lib/...`, GIF and undecodable sources stay labelled placeholders
whose alt text is the source URL (data URIs are not echoed) and raise the Resources
warning. Export writes `shape=image;thinodaCard=image;image=data:<mime>,<base64>` plus the
display size, and the export gate allows resources for draw.io only when every resource
is a PNG/JPEG picture of an image card. Tests: `inline_png_and_svg_images_become_transparent_image_cards`,
`remote_and_unsupported_images_stay_placeholders_with_their_source`,
`image_cards_round_trip_their_pixels_and_size`; corpus still 89/89.

**Shape label editing (user-reported bug, fixed).** Typing on a selected shape
swallowed the first character (the seeded character was selected and replaced by the next
keystroke), the editor used the default UI text colour (unreadable on light fills in dark
mode) and was aligned top-left while the rendered label is centred. Fix: keep the cursor
after the seeded text, use the renderer's contrast colour and composited fill, centre the
editor both ways and match the label font size (`render::board_label_colors` is shared by
the renderer and the editor). Tests: `typing_on_a_selected_shape_keeps_every_character`,
`label_editor_text_contrasts_with_the_shape_fill_in_both_themes`.

**Built-in shape aliases (fidelity slice 2).** Measured on the corpus, the most frequent
unmapped built-ins now import as the nearest Thinoda shape instead of an "unknown"
rectangle. Exact: `partialRectangle` (all sides), `mxgraph.basic.rect`,
`mxgraph.bootstrap.rect`/`rrect`, `delay`. Approximated, reported as a Formatting loss:
`folder`, `cube`, `message`, `umlFrame` → rectangle; `endState`,
`mxgraph.basic.partConcEllipse` → ellipse; `mxgraph.arrows2.arrow` → arrow; a
`partialRectangle` with hidden sides drops its outline rather than drawing invented lines.
`waypoint` becomes a 6 px dot in the stroke colour at the cell centre, and edges still
attach to it. Dedicated outlines (folder tab, cube depth, envelope, UML actor/frame tab,
partial sides) need new `ShapeKind`s and stay with Phase 4/13. Tests:
`built_in_shape_aliases_map_to_their_thinoda_equivalents`,
`waypoints_become_small_dots_in_the_stroke_color`; corpus still 89/89.

**Connector waypoints (fidelity slice 3; partial Phases 3 and 8).** Explicit
draw.io connector points import in sheet coordinates and export relative to
their parent; the native model retains them through save, undo/redo,
copy/paste, connector moves and group rotation. The points are shared between
history snapshots, with limits of 1,024 per connector and 200,000 per sheet.
Native format v10 is required only when at least one connector has points;
older documents without them retain their previous version and serialization.
Waypoint-bearing sheet and board clipboard data use versions 6 and 3,
respectively, and recovery and private XMind snapshots use waypoint-aware
envelopes. Readers reject forged older clipboard and recovery versions. Older
Thinoda versions cannot restore the versioned private XMind snapshot and
instead fall back to XMind's public content, where canvas objects are omitted;
the XMind export preflight already warns about that loss.

Straight routes pass through their points; curves use sampled quadratic spans;
elbow routes use a horizontal-first orthogonal approximation. Rendering,
hit-testing, visual export, labels and the label editor use the same resolved
path. Edge-to-edge attachments meet the path midpoint, and deleting the
target leaves the endpoint there. No-point connectors keep their previous
routing. Import reports a Formatting warning when draw.io routing hints need
approximation; the original routing style of a curved orthogonal edge is not
preserved. Malformed waypoint coordinates are omitted with an import warning.
draw.io file export currently detaches connector-to-connector
attachments at the target's path midpoint and requires consent through a
localized loss warning; the public export API also rejects that loss unless
explicitly authorized. Copy as draw.io reports that loss after copying in a
persistent warning, including attachments to connectors outside the copied
selection; its report-returning API exposes losses in the prepared fragment,
while the text-only encoder refuses those lossy conversions. Callers must report
any losses incurred while preparing a fragment. Exact mxGraph routing, fixed
ports and lossless style preservation remain future work.

Regression coverage includes coordinate and native-version round trips,
parent-relative groups, limits, transforms, history, clipboard, path hit
testing, label editing, attachment deletion, persistence versioning and export
preflight. The 89-file example corpus passes with 564 waypoint-bearing edges;
the corpus test additionally checks waypoint paths and native workbook
equality, not only page and item counts.

**Interactive waypoint editing (partial Phase 8).** A selected, unlocked
connector shows its explicit waypoints and smaller insertion handles on
straight, elbow and curved paths. Drag an insertion handle to add a point or an
existing handle to move it; double-click an existing point to remove it. A
drag previews the route without changing the document and commits one undoable
edit on release. The connector-route popup and inspector can clear all points.
Controls have localized names and instructions: use Tab to focus them,
Enter or an arrow key to insert, arrow keys to move, and Delete to remove.
Endpoint controls retain their existing order and behavior. Waypoint-limit,
locked, stale-control and invalid-coordinate errors are localized.

**Waypoint editing critique.** Clipped or offscreen controls are not offered;
insertion handles close to existing controls are hidden to reduce overlap, so
some short segments require zooming in. Coincident explicit points remain
separately keyboard-addressable, but their pointer targets can overlap.
Insertion follows logical point order even when clipping or coincident points
collapse rendered vertices; generated elbow bends are not independent
waypoints. Keyboard movement that takes a handle out of the viewport or a
clipping frame returns focus to the connector. The existing no-point curve uses
a cubic approximation, whereas the
first waypoint switches it to sampled quadratic spans, so adding a point can
change its appearance. Elbow-segment dragging, exact draw.io routing and
physical-device and real assistive-technology acceptance remain open. Gesture
cancellation and revision guards prevent a stale preview from modifying a
changed document; locked connectors cannot be edited.

**Connector approximation warnings and critique (partial Phases 3, 4 and 8).**
The first safeguard reported a formatting warning for non-classic marker shapes,
non-default marker sizes or fills, fixed entry/exit points and offsets, port
references and direction constraints, and non-default perimeter spacing.
File and link imports use the existing confirmation and source protection;
clipboard pastes remain undoable and show a simplification warning. The
supplied `board-visual-tutorial.drawio` demonstrates open arrowheads, custom
sizes and fixed ports. Default-sized, filled classic markers and plain
connectors keep their existing no-warning round trip. Empty arrow names and
leading-semicolon default suppression now correctly remove the end arrow; an
explicit `defaultEdge` restores it in style order. This is a safeguard, not
marker or port support by itself.

**Open and block connector markers (partial Phases 3, 4 and 8).** Default-size
open and filled block arrows at either end now retain their type through draw.io
file, editable SVG/PNG, link and clipboard conversions. The localized connector
inspector changes each enabled arrow's style; toolbar toggles, style copy and
undo preserve valid marker state. The core scene supplies shared marker geometry
and stroke shortening to canvas and vector/raster/presentation exports,
including hit and lasso selection. Native v11, sheet clipboard v7, board
clipboard v4, separate marker-aware recovery envelopes (including pending
ink), and private XMind snapshots reject older readers and forged older tags.
Documents without custom
markers retain their earlier serialization and version. All 89 supplied
diagram files round-trip their 140 imported open/block marker edges through
draw.io; the live published editor's marker coordinates and line insets also
match the tested straight and diagonal defaults. The underlying proportions
can also be checked against the
[mxGraph marker implementation](https://github.com/jgraph/mxgraph/blob/master/javascript/src/js/shape/mxMarker.js).

**Critique at this slice.** This covered only two published marker types. The
then-existing classic arrow was a legacy triangle, not draw.io's notched arrow;
the following slice addresses that distinction. Short overlapping markers still
collapse the connecting stroke, curved/elbow routing remains approximate, and
pixel-identical parity, physical-device and screen-reader acceptance have not
been established. The in-memory representation keeps booleans alongside
optional kinds with validation, rather than the eventual derived marker API in
§3.2.3. Lossless save-back and editable ports remain open.
As with other private-only board content, a Thinoda XMind snapshot is ignored
when public content or embedded images were changed externally; importing then
uses the public XMind representation, which cannot restore those markers.
Unreadable legacy snapshots without recognizable marker fields can also fall
back to public content; the original XMind file should be retained.

**Classic and thin connector markers (partial Phases 3, 4 and 8).** Default-size
classic, classicThin, openThin and blockThin arrows are distinct editable marker
kinds. An implicit draw.io default end arrow becomes an explicit notched classic
marker. Native v12, sheet clipboard v8, board clipboard v5, extended recovery
envelopes (with and without pending ink), and private XMind marker snapshots
protect these kinds from older readers. Open/block-only documents remain v11;
old native triangle arrows retain their original rendering and serialization.
The inspector labels the legacy triangle separately from exact classic arrows.
All 89 supplied diagram files round-trip their 1,028 imported marker-bearing
edges with these representations. Browser journeys in Chromium, Firefox and
WebKit exercise offline link import, keyboard activation, legacy markers and
the native v12 gate.

The shared geometry includes the classic notch and thin widths; canvas fill uses
triangulation rather than a convex-polygon shortcut. Native triangle arrows are
approximated by draw.io classic arrows outside Thinoda, and the export/clipboard
loss report now discloses this. Thinoda-specific style hints retain the legacy
triangle on a subsequent import if the corresponding arrow remains unchanged;
changing its marker, size or fill takes precedence over that hint.

**Critique at this slice.** Six of the published marker types have
default-size representations, not the complete marker family. At this
checkpoint custom sizes, unfilled markers, other shapes and fixed ports
required approximation warnings; the later marker-size and hollow-marker
slices address the first two. Warnings remain generic. Older draw.io exports
lack the legacy hints, and a third-party editor can remove them, so legacy
identity cannot always be recovered from draw.io alone. Geometry, routing,
XMind private-data and accessibility limitations described above remain. The
source draw.io file and a native copy should still be retained; lossless
save-back remains unimplemented.

**Marker sizes (partial Phases 3, 4 and 8).** A scan of all 89 supplied diagrams,
including compressed pages, found non-default sizes on 219 classic arrow ends,
36 open, 19 blockThin, 17 classicThin and 13 block ends.
In the published editor the start and end size fields are independent, default
to 6 pt, accept fractional values and clamp interactive input to 0–999 pt.
Thinoda now preserves bounded per-end sizes for supported marker shapes and uses
them for canvas rendering, selection, lasso and visual exports. The inspector
exposes each size independently and preserves it when switching between
supported styles. Native v13, sheet clipboard v9, board clipboard v6, recovery
envelopes with and without pending ink, and private XMind snapshots protect
the new data from older readers. Default-sized markers keep their older formats.
The published editor's marker paths were also checked at size 12.5 and stroke
width 2 for all six supported styles, including their line-trimming positions.
The affected library suites passed 1,855 tests with four ignored, the board
export and I/O integration suites passed 61 with one ignored, and all 89
supplied diagrams passed the corpus round trip. Native and WebAssembly Clippy
passed with warnings denied. The production web build passed all 12 link-import
journeys across Chromium, Firefox and WebKit, including sized-marker native
save and the existing conversion-consent cases.

**Critique and remaining work.** Sizes on a disabled or unsupported marker
remain unrepresented and produce a formatting warning when non-default;
out-of-range or malformed values also warn and are approximated. Legacy
Thinoda triangles have no adjustable size. Sizes use finite `f32` values, so
arbitrary source decimal precision is not lossless. At this checkpoint, the six
supported shapes did not cover hollow arrows; the following slice addresses
that subset. Other marker families, fixed ports and the remaining routing and
save-back limitations above remain.

**Hollow connector markers (partial Phases 3, 4 and 8).** Classic and block
arrows, including their thin variants, preserve independent draw.io `startFill`
and `endFill` settings. They paint a closed outline without filling its interior,
while the shortened connector stroke, hit testing and lasso selection respect
that hollow geometry. Open arrows always paint as open; their saved fill choice
is retained when switching to a closed arrow. The localized inspector exposes
independent fill controls with undo and style-copy support. Native v14, sheet
clipboard v10, board clipboard v7, recovery envelopes with and without pending
ink, and private XMind snapshots protect fill overrides from older readers.
Documents without overrides keep their earlier format versions.

The affected library suites passed 1,866 tests with four ignored, and the
strengthened inspector, context-toolbar, interchange and export tests were
rerun separately. All 89 supplied diagrams passed the corpus round trip.
SVG and presentation paths, PNG pixels and rasterized PDF pixels verify
unfilled marker interiors; the PDF check also verifies the outline. The editable
PowerPoint output is checked for closed, unfilled geometry. Native and
WebAssembly Clippy passed with warnings denied, and the production web build
passed all 15 link-import journeys across Chromium, Firefox and WebKit.

**Critique and remaining work.** Only fill values `0` and `1` on supported
markers are represented. Malformed values, non-default fills on disabled or
unsupported markers, other marker families, fixed ports, and the routing and
lossless save-back limitations above still require warnings or further work.
At this checkpoint, a scan of explicit arrow ends in the supplied corpus found
17 `oval`, eight `diamondThin` and four `halfCircle` markers not yet
represented; the following slice addresses diamonds.
Selecting a legacy triangle or disabling an arrow intentionally clears its
draw.io fill choice; undo restores it. XMind's public representation does not
preserve these private-only board objects after external edits. The shared
four-point filled-marker triangulation was only checked against a concave classic
arrow; the following slice checks it against convex diamonds too. Actual
PowerPoint rendering, physical-device and real screen-reader acceptance have
not been established.

**Diamond connector markers (partial Phases 3, 4 and 8).** Filled and hollow
`diamond` and `diamondThin` ends use distinct draw.io tip offsets, widths and
shortened connector strokes, with independent per-end sizes and fill choices.
The localized inspector supports both styles. Native v15, sheet clipboard v11,
board clipboard v8, recovery envelopes with and without pending ink, and private
XMind snapshots v5 protect these markers from older readers. Other documents keep
the oldest version capable of representing their contents.

The geometry was compared with the published editor for horizontal and
diagonal connectors, including a non-default size and stroke width. The canvas
mesh is checked for complete coverage of both classic and diamond outlines;
hit testing and lasso selection distinguish hollow interiors from their
outlines. The affected Rust library suites passed 1,872 tests with four ignored;
the board-export, I/O and corpus suites also passed. All 89 supplied diagrams
passed the corpus round trip, including all six thin-diamond ends in the UML
package example. SVG and presentation paths, PNG pixels, rasterized PDF pixels
and editable PowerPoint geometry were checked for hollow markers; PNG and
PowerPoint checks also cover filled diamonds. A production web build and all
18 link-import journeys passed across Chromium, Firefox and WebKit.

**Critique and remaining work.** This slice adds two of the published marker
types, not the complete family. At this checkpoint oval and half-circle
markers, fixed ports, advanced routing and lossless draw.io save-back remain
outstanding; the following slice addresses ovals. The
four-point canvas mesh now covers both the concave classic marker and the convex
diamond; future marker shapes still require their own geometry and coverage
checks. Actual PowerPoint rendering, physical-device and real screen-reader
acceptance are not established.

**Oval connector markers (partial Phases 3, 4 and 8).** Filled and hollow
`oval` ends are circular markers centered on their connector endpoints. Their
diameters use the independent marker sizes; the connector stroke stops at each
circle's inner edge, matching the published editor. The localized inspector
supports size and fill changes with undo. Native v16, sheet clipboard v12,
board clipboard v9, recovery envelopes with and without pending ink, and
private XMind snapshots v6 protect these markers from older readers.

The geometry was compared with the published editor using different start and
end sizes and a non-default stroke width. Tests cover filled and hollow canvas
rendering, circle-aware hit and lasso selection, small and zero sizes,
source-preserving import, and the compact inspector's mouse and accessibility
scrolling. The affected Rust library suites passed 1,881 tests with four
ignored; the relevant I/O, board, presentation and corpus integration suites
passed 131 tests with three ignored. All 89 supplied diagrams passed the
corpus round trip, including the oval markers in the rockpool and UML component
examples. SVG and presentation output, PNG and rasterized PDF pixels, and
editable PowerPoint geometry were checked for filled and hollow markers. Native
and WebAssembly Clippy passed with warnings denied. The production web build
passed all 21 link-import journeys across Chromium, Firefox and WebKit.

**Critique and remaining work.** This slice does not implement other circular or
half-circle marker families, fixed ports, advanced routing or lossless draw.io
save-back. A zero-size oval is invisible, matching draw.io; it does not shorten
the connector stroke. When markers overlap on a short connector, Thinoda
collapses the remaining stroke to a point; that case has not been compared with
draw.io. Actual PowerPoint rendering, physical-device behavior and real
screen-reader acceptance have not been established.

**Half-circle connector markers (partial Phases 3, 4 and 8).** The `halfCircle`
marker is an open, unfilled pair of quadratic curves, regardless of its stored
fill setting. Its draw.io radius and connector-stroke inset are marker size plus
stroke width plus one, including for a zero-size marker. The inspector retains
the fill setting for a direct change to a fillable marker. Native v17, sheet
clipboard v13, board clipboard v10, recovery marker envelopes v7 with or
without pending ink, and private XMind marker snapshots v7 protect the new kind
from older readers; documents without it retain their earlier versions.

The shared geometry emits quadratic SVG and editable PowerPoint paths. Canvas
painting and selection use a bounded polyline approximation, with a conservative
error allowance for both subdivision and coordinate rounding. Curve extrema
and samples are computed at higher precision, including near the board's
coordinate limits. Lasso work is charged to the existing geometry budget.
Polygon, circle and curve data share tagged storage so the new marker does not
inflate every scene item; a regression test bounds the geometry's size.

Geometry checks cover horizontal and diagonal connectors, zero and custom
sizes, high coordinates, hit testing and bounded lasso selection. All 89
supplied diagrams passed the corpus round trip, including the four half-circle
ends in the UML component example. SVG and presentation paths, PNG pixels,
rasterized PDF pixels and editable PowerPoint geometry were checked. The
affected library suites passed 1,894 tests with four ignored; the board-export
and corpus integration suites passed 50 with two ignored. The marker PDF
rasterization test passed separately. Native and WebAssembly Clippy passed
with warnings denied. The production web build passed all 24 link-import
journeys across Chromium, Firefox and WebKit. All four deterministic importer
mutation tests passed. Linux AddressSanitizer smoke campaigns also passed for
XMind (15,645 runs), FreeMind (35,438) and OPML (27,662), each with a
20-second time limit and isolated corpus copies.

**Critique and remaining work.** This adds only one of the remaining marker
families. The conservative selection tolerance may select within its bounded
approximation error outside the exact curve. Short connectors with overlapping
markers still collapse the stroke to a point, which can differ from draw.io.
Fixed ports, advanced routing, the remaining markers and lossless draw.io
save-back are not implemented. Actual PowerPoint rendering, physical-device
behavior and real screen-reader acceptance remain unverified. These short
importer smokes are not the longer acceptance campaigns and do not replace the
planned draw.io fuzz target.

## 5. Out of scope / backlog
- VSDX import/export, Gliffy/Lucid/Miro import, Mermaid/PlantUML generation, auto-layouts,
  draw.io plugins, animated GIF export, real-time collaboration.

## 6. Risks
- **Model blast radius** → defaults, golden tests, consumer inventory, phase gating.
- **Interaction regression (Phase 1)** → hit-priority table + exhaustive canvas tests; the
  change touches every board, so it lands first and isolated.
- **Perimeter change alters existing documents' look**: connectors to non-rect shapes
  will end on the true outline in canvas and exports → intended improvement, called out
  in CHANGELOG; golden export snapshots updated deliberately in Phase 1.
- **Overlay clutter** (arrows + toolbars + handles) → show only for hovered/selected
  item, fade delays as in draw.io, never cover the object; hidden during pan/zoom.
- **Stencil breadth** (800+ handlers) → prioritise by corpus; unknown → placeholder.
- **Binary/wasm size** → compressed lazy libraries, CI budget.
- **HTML/SVG security** → 3.6.
- **Round-trip drift** → ownership table + property tests.

## 7. Rubber Duck findings & resolutions
| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | blocker | Passthrough lifecycle undefined (edit/delete/copy/undo) | 3.1.1 ownership contract + lifecycle tests (Phase 3) |
| 2 | blocker | No parent tree / relative geometry / edge-label children | 3.2.1 scene hierarchy; sheet-space `rect` kept |
| 3 | major | doc.rs only treats Thinoda/XMind as writable | 3.5 editable-format contract (Phase 7) |
| 4 | major | No workbook-level home for mxfile data | `DrawIoDocumentExt`/`DrawIoPageExt` |
| 5 | major | Stencils/constraints/page links needed before import | Phase 5 before Phase 6; link map in Phase 6/7 |
| 6 | major | bool→marker migration underspecified | 3.2.3 dual representation + exhaustive version-gate tests |
| 7 | major | Renderers not unified; exhaustive matches | 3.7 inventory + Phase 4 primitives |
| 8 | major | `ImportWarning` too coarse | `ImportDiagnostic`, derived coarse warning |
| 9 | major | Security criteria vague | 3.6 acceptance criteria + adversarial corpus |
| 10 | major | Undo memory / masking / clipboard ignore passthrough | Arc accounting, masking, caps (Phase 3) |
| 11 | major | Unbounded read fallback; deflate only transitive | 3.3 read policy + direct `miniz_oxide` |
| 12 | minor | Phase 0 lacked ADR, provenance, measurable budgets | Phase 0 deliverables |

**Round 2** (after adding draw.io shape placement + connector quick actions):
| # | Severity | Finding | Resolution |
|---|---|---|---|
| R1 | blocker | Edge labels lacked identity; label drag/edge-to-label not modelable in Phase 1 | `LabelId`, `Endpoint::Label`; positioned label drag moved to Phase 8 |
| R2 | blocker | Cut-vs-copy `source_id` rule contradicted clipboard (fresh ids, no provenance) | Clipboard provenance envelope with one-shot cut token |
| R3 | major | Perimeter/spacing had no persistent field | `ConnectionStyle`; Phase 1 uses derived defaults only |
| R4 | major | Delete vs Delete-all unassigned & conflicts with current detach | Defined semantics, Phase 8 |
| R5 | major | Diagnostics had no transport to UI | `ImportReport.diagnostics` + transient `OpenDoc` field, per-loader tests |
| R6 | major | Undo rules contradicted compound quick actions | Compound actions = one atomic undo entry |
| R7 | major | "Full fidelity" vs placeholders | Three-tier support matrix with release gates (§1) |
| R8 | major | "Cap before decoding" not implementable for deflate | Incremental chunked inflate with in-flight ceiling |
| R9 | minor | No touch gesture for quick-shape popover | Press-and-hold on the arrow itself (implemented in 1b part 2 instead of the chevron); object long-press unchanged |

**Waypoint slice follow-up:**
| # | Severity | Finding | Resolution |
|---|---|---|---|
| W1 | blocker | The plan assigned all later fields to v10 even though a waypoint-aware v10 reader now exists; it could silently discard later fields. | Require new native and alternate-envelope versions for subsequent unsupported data; reject forged older versions. |
| W2 | major | draw.io export and copying could silently detach connector-to-connector attachments, including targets outside the selection. | Resolve the fallback at the path midpoint; require export consent at the API and app layers, or show a persistent clipboard warning. |
| W3 | major | A future private XMind snapshot with an incompatible body could fall back before its version was checked. | Check the version tag first and reject unknown versions. |
| W4 | minor | Adding Copy as draw.io made the expanded Edit menu taller, so its Settings test attempted to click an off-screen item. | Check that both commands can be revealed, then scroll Settings into view before activating it. |
| W5 | major | A clipped endpoint can coincide with a waypoint and later revisit the same coordinate, making coordinate-only insertion indices ambiguous. | Carry logical indices through routing and deduplication; test straight and elbow paths that revisit the clipped endpoint. |
| W6 | major | Keyboard movement can take a focused waypoint outside the viewport or clipping frame, leaving no focusable handle. | Return focus to the owning connector and cover both boundaries. |
| W7 | major | An assistive-technology click on an insertion handle had no effect despite its button role. | Insert and focus the point on AccessKit activation; clicking an existing point focuses it without deleting it. |

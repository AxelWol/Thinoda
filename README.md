# yMind

A free, fast, cross-platform mind mapping app written in Rust — an XMind-style editor with a
GPU-accelerated canvas, crash-safe autosave, broad import/export and an optional local AI assistant.

Runs on **Windows** (x64 and ARM64), **macOS** (11+, Apple silicon and Intel) and **Linux** (Ubuntu LTS).

## Features
- Mind map, logic, brace, org, tree, timeline, fishbone and matrix structures; floating topics,
  relationships, boundaries, summaries, markers, labels, notes, links, to-dos, numbering
- Map > Theme > Global Font and Style > Text > Font Family use installed system
  font faces, not just stored names. Per-topic Auto inherits the map font.
  Canvas layout, painting, inline editing and Map Shot share the selected face;
  unavailable families warn and keep their requested names. Native files retain
  the settings, and public XMind JSON/XML receive the effective font names.
  Fonts are not embedded or redistributed; install them on each target machine.
- Outliner, Zen mode, command palette (Ctrl+Shift+P), XMind-compatible shortcuts
- Home offers rendered previews of 17 original templates across eight categories.
  Select a card, then Create; Cancel returns to the previous document without
  changing it. The last eight used template IDs are stored locally, separately
  from recent files. Ctrl+N still creates a blank map directly.
- Copy/Paste Style (Ctrl+Alt+C/V) shares explicit topic-style overrides across open
  documents; Reset Style restores automatic theme inheritance. Multi-topic paste/reset
  is one undo step and does not replace text or hierarchy.
- Indent/Outdent in Edit, the topic context menu and command palette moves one topic
  under its previous sibling or after its parent while retaining its annotations.
  Previous/Next Sheet (Ctrl+PageUp/Down) wraps between sheets and commits pending title edits.
- Sheet menu and sheet-tab context menu offer Copy/Paste of complete sheets through
  the OS clipboard (including across yMind instances), and Save Sheet As for a
  separate `.ymind`/`.xmind` copy without changing the source save state.
  New Sheet from Topic copies a branch with its owned annotations and internal
  relationships; relationships to topics outside the branch remain in the original sheet.
- Select submenu offers visible siblings, descendants, inversion and central-topic
  navigation. Arrange offers natural child-title sorting and floating-topic edge/
  center alignment or equal center spacing, each in one undo step.
  Sorting is rejected if it would split a Summary/boundary's source membership.
- Tools > Merge Map Files appends complete `.ymind`/`.xmind` sheets in one undo step.
  All inputs must load successfully; source files stay untouched and destination
  metadata is retained. External image references are not embedded.
- Tools > Map Shot (F7) captures the visible map using native GPU readback, without
  app chrome, selection outlines or search dimming. Drag a crop or enter pixel
  coordinates; square crops and a configurable frame are available. Copy an image
  to the system clipboard or save PNG; Escape/Cancel discards the capture without
  changing the map. Use Export for the complete sheet, including offscreen topics.
- Tools > Create Masked Copy opens a separate unsaved preview with generic sheet,
  topic and relationship names, removing notes, labels, links, images, markers,
  custom font names, boundary titles and metadata. Inspect every sheet before
  saving/exporting; structure, colors and task states remain, so anonymity is not
  guaranteed. The original document and its save destination are retained.
- Floating editing toolbar with a `+` insert menu, selection-aware topic toolbar, child/sibling plus connectors,
  and contextual right-click actions
- Hierarchy drags show a translucent visible subtree, including owned Summary trees,
  boundaries and internal relationships, while dimming its source. Valid parents
  are highlighted; self/descendant and outside-canvas drops are rejected visibly.
  Escape, popups and stale document/selection changes cancel without edits.
  Viewport-edge auto-pan keeps the ghost under the pointer. Dropping on a parent
  appends once and expands it; blank drops detach to floating by default.
  Map > Advanced Layout > Flexible Floating Topics lets floating roots overlap
  topics without attaching, and prevents attached topics from detaching into blank
  space. Move mixed floating/attached selections separately in this mode. The
  per-sheet setting is undoable and defaults off for existing maps.
  Outer quarters of attached topics select before/after sibling slots,
  with a highlighted insertion line and proposed connector. Split-map root drops
  choose the pointer's left/right side; Style > Branch side offers Auto/Left/Right
  for main topics. Explicit sides survive balancing and sibling additions. Native
  files and unchanged yMind XMind snapshots retain them; public XMind side interchange
  and exact native gesture/visual comparison remain open.
- Insert > Labels, the topic/context menus and Style > Edit labels open the same
  anchored editor. Add comma-separated labels, reuse current-sheet suggestions or
  remove chips; Apply commits once, while Escape/Cancel/outside click discards the
  draft. Labels are trimmed and deduplicated case-sensitively, retaining order by
  default. Optional Auto sort uses Unicode code-point order; its local preference
  is remembered only on Apply. Existing labels containing commas remain intact.
- Select relationship lines, boundary outlines or Summary braces to delete them with
  Delete or a context action, with undo/redo. Covered branches remain intact; deleting
  a Summary removes its own summary topic and children.
- Selected relationships retain their line color beneath a selection overlay.
  Drag round endpoints onto topics to reconnect, or square Bezier controls to reshape.
  Previews do not change the map until release; Escape cancels, and each change undoes
  once. Focused controls move by 1 map unit with arrows (Shift: 10); endpoint arrows
  cycle visible topics. Style also edits the title, endpoints, color and relative
  control coordinates, with Reset curve restoring automatic routing. Native files
  and unchanged yMind-exported XMind files preserve custom routing; public XMind
  curve-control interoperability remains unfinished.
- Relationship creation connects exactly two selected topics immediately. With one
  source selected, pick an existing target or click blank canvas to create a connected
  floating topic and edit its title. Creation and connection undo together; Escape
  cancels picking, and failed connections leave no orphan topic.
- Find (Ctrl+F) in the active sheet or all sheets, including optional notes/labels,
  with sheet-aware navigation. Replace (Ctrl+H) updates topic titles in one undo step,
  including replacements across all sheets; notes and labels are search-only.
- Shared right sidebar: Format Panel for Style/Map and the smiley for grouped Symbols;
  click the active selector again to hide/reopen it. Map theme previews wrap to fit,
  and Style/Map tabs stay visible while their contents scroll independently.
  Style reflects the selected topic's resolved properties, including inherited/custom
  colors with checked swatches and hex values; Map controls apply to the entire sheet.
- The Map structure selector opens a viewport-bounded, scrollable preview gallery
  with nine collapsible families covering all 17 existing structures. Original sample
  maps use the actual canvas renderer in light/dark mode; layout changes preserve rich
  content and selection, fit the view and undo in one step.
- New-map Clarity theme: plain bold center, colored main topics, pale subtopics and
  thin connectors inspired by the supplied visual references; existing saved themes
  and per-topic style overrides remain unchanged.
- Light/dark theme following the system, English and German UI, personal settings
- Settings categories for General, Editor, AI and Cloud. Editor preferences independently
  control the topic toolbar and add buttons, set initial map zoom and optionally reuse
  the last structure/theme for new blank maps without changing named templates.
  The zoom menu offers 10–500% presets, the current value and Fit Map.
  The topic-count menu shows current-sheet topic, word and Unicode character statistics.
- Import: XMind, FreeMind/Freeplane, OPML, Markdown, text, MindManager
- XMind reopening honors edits made by other applications rather than silently
  restoring a stale private snapshot. Unchanged yMind exports retain lossless
  reopening; supported public theme defaults and inline overrides are imported.
- Export: XMind, PNG, JPEG, SVG, PDF, Word, Excel, CSV, Markdown, OPML, TextBundle
- Crash safety: atomic saves plus a recovery journal that restores unsaved work after a crash
- Cloud: opens/saves directly in OneDrive, iCloud Drive, Dropbox, Google Drive, Box, Nextcloud folders
- AI assistant (optional): brainstorm subtopics, break down into to-dos, explain, summarize,
  generate a whole map — with any OpenAI-compatible server

## AI setup (LM Studio)
1. Start LM Studio, load a chat model (e.g. `google/gemma-4-e4b`) and enable the local server.
2. In yMind open **Settings › AI Assistant**, set *Server URL* to `http://127.0.0.1:1234/v1`
   (or a LAN server such as `http://192.168.1.216:1234/v1`), leave *Model* empty for automatic
   selection and click **Test Connection**.
3. Select a topic and press **Ctrl+Shift+I** or use the **AI** menu.

Ollama (`http://127.0.0.1:11434/v1`) and OpenAI (`https://api.openai.com/v1` + API key, or env
`YMIND_AI_API_KEY`) work the same way.

## Build & test
```sh
cargo run -p ymind-app            # start the app
cargo xtask ci                    # fmt + clippy + all tests
cargo test -p ymind-app --features gpu-tests --lib map_shot_gpu_readback # requires a GPU adapter
cargo xtask package               # installers in target/packages
ymind --convert map.xmind map.md  # CLI conversion
```
Linux build dependencies: `libgtk-3-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev
libxcb-xfixes0-dev libwayland-dev`.

The live AI test uses `YMIND_AI_TEST_URL` (or the known LM Studio hosts) and is skipped when no
server answers or when `YMIND_AI_SKIP_LIVE=1`.

## Packages
| Platform | Formats | Command |
|---|---|---|
| Windows x64 / ARM64 | NSIS installer (`.exe`) | `cargo xtask package --target aarch64-pc-windows-msvc` |
| macOS | `.app`, `.dmg` | `cargo xtask package --formats app,dmg` |
| Ubuntu LTS | `.deb`, AppImage | `cargo xtask package --formats deb,appimage` |

Tagging `v*` runs the release workflow, which builds every package and drafts a GitHub release.

## Docs
[Plan](docs/PLAN.md) · [Backlog](docs/BACKLOG.md) · [Architecture](docs/ARCHITECTURE.md) ·
[XMind research](docs/XMIND-RESEARCH.md) · [User-guide checklist](docs/XMIND-USER-GUIDE.md)

## License
MIT OR Apache-2.0

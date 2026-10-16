# yMind

A free, fast, cross-platform mind mapping app written in Rust — an XMind-style editor with a
GPU-accelerated canvas, crash-safe autosave, broad import/export and an optional local AI assistant.

Runs on **Windows** (x64 and ARM64), **macOS** (11+, Apple silicon and Intel) and **Linux** (Ubuntu LTS).

## Features
- Mind map, logic, brace, org, tree, timeline, fishbone and matrix structures; floating topics,
  relationships, boundaries, summaries, markers, labels, notes, links, to-dos, numbering
- Outliner, Zen mode, command palette (Ctrl+Shift+P), XMind-compatible shortcuts
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
- Tools > Create Masked Copy opens a separate unsaved preview with generic sheet,
  topic and relationship names, removing notes, labels, links, images, markers,
  custom font names, boundary titles and metadata. Inspect every sheet before
  saving/exporting; structure, colors and task states remain, so anonymity is not
  guaranteed. The original document and its save destination are retained.
- Floating editing toolbar with a `+` insert menu, selection-aware topic toolbar, child/sibling plus connectors,
  and contextual right-click actions
- Select relationship lines, boundary outlines or Summary braces to delete them with
  Delete or a context action, with undo/redo. Covered branches remain intact; deleting
  a Summary removes its own summary topic and children.
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

_Last updated: 2026-10-25 14:39:11 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

<p align="center"><img src="assets/icon.svg" alt="Thinoda logo" width="128"></p>

# Thinoda

*Give your thoughts shape.*

A free, fast, cross-platform mind mapping app written in Rust — an XMind-style editor with a
GPU-accelerated canvas, crash-safe autosave, broad import/export and an optional local AI assistant.

Runs on **Windows** (x64 and ARM64), **macOS** (11+, Apple silicon and Intel) and **Linux** (Ubuntu LTS).
A static **web app** build runs the same code in the browser via WebAssembly and is
published on [Thinoda on GitHub Pages](https://axelwol.github.io/Thinoda/) — see
[Thinoda for the web](docs/WEB.md).

## Privacy

Thinoda's maintainers do not collect or process your personal data or mind map
content through the app. Editing and storage happen locally on your device.
Optional AI connections, cloud-synced folders and web hosting involve the
services you choose to use. See the [Privacy Policy](PRIVACY.md) for details.

## Download
Installers for every platform are attached to each
[GitHub release](https://github.com/AxelWol/Thinoda/releases/latest):

| Platform | File |
|---|---|
| Windows x64 / ARM64 | `Thinoda_<version>_*-setup-<target>.exe` (NSIS installer) |
| macOS Apple silicon / Intel | `Thinoda_<version>_*-<target>.dmg` (or `.app.tar.gz`) |
| Ubuntu LTS (x64) | `.deb` or `.AppImage` for `x86_64-unknown-linux-gnu` |
| Ubuntu LTS (ARM64) | `.deb` or `.AppImage` for `aarch64-unknown-linux-gnu` |

`<target>` is `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`,
`aarch64-apple-darwin` (Apple silicon), `x86_64-apple-darwin` (Intel) or
`x86_64-unknown-linux-gnu`, or `aarch64-unknown-linux-gnu` (Linux ARM64).

Each platform also has a `SHA256SUMS-<target>.txt` checksum manifest. Windows
installers are not code-signed yet and SmartScreen may block first launch.
The macOS release workflow Developer ID-signs, Apple-notarizes and staples both
the app and DMG for Apple silicon and Intel. Older releases and local builds may
still use ad-hoc signatures, which seal contents but do not establish developer trust.

### Installing on Windows with WinGet
The Windows installer's final page offers separate labeled options to run Thinoda
and create a desktop shortcut. Its language selector offers English, German,
Spanish, French, Danish, Dutch, Portuguese (Portugal and Brazil), Italian and Polish,
including maintenance, upgrade/downgrade, uninstall and application-running messages.

```powershell
winget install AxelWol.Thinoda
```
Automatic submissions to [winget-pkgs](https://github.com/microsoft/winget-pkgs)
are temporarily paused in `.github/workflows/winget.yml`, including calls from
the release workflow. Download current installers from GitHub Releases instead.
Resuming submissions requires re-enabling the workflow and its guarded job;
the `WINGET_TOKEN` secret is still required.

### Installing on Ubuntu
Requires Ubuntu 22.04 LTS or newer on x64 (`dpkg --print-architecture` prints `amd64`),
including Ubuntu on WSL 2 with WSLg. The new native ARM64 release target requires
Ubuntu 24.04 LTS or newer (`arm64`); its downloads become available with the first
release built by the updated workflow.

```bash
# Recommended: .deb (menu entry, icon, clean uninstall with `sudo apt remove thinoda`)
cd /tmp
wget https://github.com/AxelWol/Thinoda/releases/latest/download/SHA256SUMS-x86_64-unknown-linux-gnu.txt
wget "https://github.com/AxelWol/Thinoda/releases/download/v<version>/thinoda_<version>_amd64-x86_64-unknown-linux-gnu.deb"
sha256sum -c SHA256SUMS-x86_64-unknown-linux-gnu.txt --ignore-missing
sudo apt install ./thinoda_<version>_amd64-x86_64-unknown-linux-gnu.deb

# Portable alternative: AppImage (needs FUSE 2: libfuse2t64 on 24.04+, libfuse2 on 22.04)
chmod +x thinoda_<version>_x86_64-x86_64-unknown-linux-gnu.AppImage
./thinoda_<version>_x86_64-x86_64-unknown-linux-gnu.AppImage
```

For ARM64, use `SHA256SUMS-aarch64-unknown-linux-gnu.txt` and the
`thinoda_<version>_arm64-aarch64-unknown-linux-gnu.deb` download instead.
The portable download is
`thinoda_<version>_aarch64-aarch64-unknown-linux-gnu.AppImage`; it needs
`libfuse2t64` on Ubuntu 24.04.

If version 0.1.0 stops with `Library libxkbcommon-x11.so could not be loaded`,
install the windowing libraries it did not declare yet:
`sudo apt install libxkbcommon-x11-0 libvulkan1`.

## Languages
Settings > General > Language is an accessible drop-down with **Auto**, English,
Deutsch, Español, Français, Dansk, Nederlands, Português (Portugal),
Português (Brasil), Italiano and Polski. Selection changes the UI and future topic
placeholders immediately, independently of light/dark appearance, and is preserved
on restart. Auto recognizes regional OS locales (including `pt-BR` separately from
`pt-PT`); unsupported OS languages use English. Existing document text is never
rewritten on a language change.

Menus, settings, sidebars, template/category titles, built-in theme/shape names,
application notification text, print previews and searchable offline help use the
same embedded catalogs on every advertised architecture. Brazilian Portuguese
shares common Portuguese phrases and has explicit regional vocabulary overrides.
Starter template content remains English and editable; format/product names,
user-created names, technical diagnostic details and OS/driver messages retain
their original spelling/language.

Both Windows NSIS targets include translated standard pages and all custom
installer controls. Both macOS app/DMG targets declare the supported bundle
localizations; Finder's drag-to-Applications installation has no custom installer
wizard. Linux DEB and AppImage launchers include translated desktop descriptions;
package-manager interfaces belong to the host distribution. Native file/print
dialogs, Finder and package managers follow the host OS language and installed
language packs, not Thinoda's override. No claim is made that Thinoda translates
those system-owned controls. WinGet publishing remains paused.

## Features
- Mind map, logic, brace, org, tree, timeline, fishbone and matrix structures; floating topics,
  relationships, boundaries, summaries, markers, labels, notes, links, to-dos, numbering
- Map > Theme > Global Font and Style > Text > Font Family use installed system
  font faces, not just stored names. Per-topic Auto inherits the map font.
  Canvas layout, painting, inline editing and Map Shot share the selected face;
  unavailable families warn and keep their requested names. Native files retain
  the settings, and public XMind JSON/XML receive the effective font names.
  Fonts are not embedded or redistributed; install them on each target machine.
- Native Windows/macOS/Linux builds also discover installed fonts on demand for
  characters missing from the configured face, including mixed Japanese, Chinese
  and Korean titles and text drafts. Configured fonts keep priority; document font
  names and undo history are unchanged. Coverage depends on installed fonts (for
  example, `fonts-noto-cjk` on Ubuntu); this does not install or download fonts.
  Browser builds do not gain access to OS fonts through this native fallback.
  Native Settings > Editor > Default CJK font optionally puts an installed face
  after configured fonts and before automatic discovery. Auto removes this
  preference; choose a face covering your script. It does not override existing
  glyphs or change document fonts/history. Browser builds omit this control.
  Native IME acceptance and exact vector-export font matching remain separate work.
- Map > Branches offers automatic or explicit map-wide stroke width (0.5-16).
  Per-topic line widths still override it. Confirm numeric drafts with Enter or
  blur; Escape cancels. Invalid values leave the map and undo history unchanged.
- Outliner, Zen mode, command palette (Ctrl+Shift+P), XMind-compatible shortcuts.
  Escape exits Zen after closing any active editor, search or popup first;
  it does not rely on F11, which browsers may reserve for fullscreen.
- Search & Replace grows to show its controls without internal scrolling when
  space permits; short/high-scale windows retain bounded scrolling.
- Settings > Editor > Show alignment guides optionally shows nearby matching
  topic-group edges/centers during free-placement dragging. It is off by default,
  does not snap positions, and does not change document styles or Undo history.
- Style, Map and Symbols sidebars reveal newly focused controls during keyboard
  navigation. Focus elsewhere does not scroll the inspector, and manual scrolling
  is preserved until focus changes.
- The searchable keyboard-shortcut reference scrolls within short/high-scale
  windows while keeping Close visible. Global bindings come from the dispatch
  table, with platform-specific hints in the reference and command palette.
  Main/context menu hints use the same command-aware binding lookup and platform
  formatter, including nested Select/Arrange actions. Hints follow the current
  Map/Outliner view; the reference names that view and lists only its bindings.
  Outline retains its existing topic-row focus rules and Ctrl/Cmd+Y redo.
  Intercepted bindings are not advertised: Fit Map remains clickable and appears
  unassigned because Ctrl/Cmd+Shift+H currently opens Find & Replace.
  Bindings remain read-only.
- The command palette keeps search visible above scrollable results on small
  windows. Arrow keys reveal the selected result; Escape closes without clearing
  the map selection or changing a pending title edit.
- Save/Save As/Export require an active map. Save All also works from Home for
  background documents and keeps Home open afterward. In-progress topic titles
  are committed before Save All or save-on-close checks; cancelling close keeps
  that text as an undoable edit. Save All pauses for file conflicts or save
  consent, then resumes automatically; a pending Quit waits for every save.
  Cancel or a save failure stops the batch without closing. Earlier successful
  saves stay saved; the remaining documents keep their edits.
- **Print** in the main menu / **Ctrl+P** (Command+P on macOS) opens a themed,
  localized paper preview of the current sheet, including offscreen topics.
  Choose A4/A3/Letter/Legal, portrait/landscape, 0–40 mm margins, 150/300 dpi,
  light/dark map colors, optional map background and grayscale. The complete map
  fits one page; collapsed branches remain collapsed. The UI theme and printed
  map colors are independent (default: light map on white paper to save ink).
  **Continue to print…** opens native printer selection and OS/driver preferences:
  Windows **PrintDlg** with **Properties**, macOS **AppKit NSPrintPanel / NSPrintOperation**,
  Linux **GTK 3 PrintOperation** with installed desktop print backends (normally CUPS).
  No browser, external document viewer, shell command or automatic default-printer
  submission is used. Dialogs run modally on the main/window thread; the map cannot
  be edited until they return. Native controls use the OS/desktop language, theme
  and accessibility, independently of Thinoda's language and light/dark preview.
  Available advanced options (copies, duplex, trays, quality, etc.) depend on the
  installed printer driver/backend; Thinoda does not emulate unsupported options.
  Cancellation is reported separately from errors. “Submitted” means the OS
  accepted the job, **not** confirmation that a physical page was printed.
  **Save print PDF…** still writes a paper-sized PDF without a printer.
  The complete preview raster, including its margins and colors, is centered and
  uniformly fitted inside the selected printable area, avoiding hardware-edge
  clipping when the driver reports correct bounds. Paper/orientation changes in
  the OS dialog scale this fixed page, not reflow the map. To change the source
  layout, cancel and change Thinoda's preview settings. Hardware margins can
  slightly reduce the printed page; differing paper aspect ratios add whitespace.
  GTK also respects the selected page-setup margins and reported hardware margins;
  backends without hardware-margin information use the selected page setup.
  Source **150/300 dpi** controls raster detail; device DPI/quality and additional
  scaling, N-up, color management or grayscale are driver settings. Thinoda's
  grayscale pixels remain grayscale regardless of a color driver setting.
  Output uses the complete-sheet export renderer, not the GPU viewport, with
  lossless native raster data / high-quality JPEG in PDF. Large maps may need A3
  or 300 dpi; multi-page tiling is not implemented. Real dialog/driver behavior
  needs native smoke testing on each OS; tests never submit actual print jobs.
  Linux (including AppImage) loads the host's GTK 3 and its CUPS print backend,
  rather than bundling a partial GTK stack. Physical printers require these
  installed system libraries and a configured printer (Ubuntu:
  `libgtk-3-0`, which includes the CUPS backend, with a working CUPS configuration).
  Missing runtimes/symbols are reported as errors. The winit
  window is not a GtkWindow: its event loop is blocked during the GTK modal dialog,
  but some compositors may not group that dialog under Thinoda's taskbar window.
- Help > Quick Start / User Guide provides searchable offline translated guidance
  for editing, styles, file recovery, interchange and privacy. It works without an
  open map and does not edit the canvas behind it. Send Feedback opens GitHub's
  public issue chooser without uploading files; Project News opens commit history.
- Help > About > Legal documents provides the complete licence, third-party
  notices and Impressum offline. The selectable original texts are unchanged;
  the viewer uses the application's language and light/dark appearance.
  CommonMark/GFM headings, emphasis, lists, links, code and striped tables are
  formatted with 1.5× natural text line height. Wide tables can scroll horizontally;
  Copy copies the exact original Markdown. Links between bundled legal documents
  stay offline. Remote images are not fetched (alternative text is shown), and
  raw HTML is displayed as inert text rather than executed.
- F1 opens a searchable keyboard reference with category and unassigned-command
  filters. Map/insertion hints and aliases come from dispatch, including Redo
  Ctrl/Cmd+Y, Subtopic Insert, Delete Backspace and priority keys. These contextual
  bindings are not active in every view or focused text field.
  Cut/Copy/Paste shortcut metadata describes existing clipboard events; it does
  not add new palette actions or intercept clipboard keys.
  It is read-only; custom rebinding and conflict editing are not yet available.
  Ctrl+Q requests normal window closing,
  retaining the save prompt for unsaved documents.

- Home offers rendered previews of 35 original templates across eight categories.
  Select a card, then Create, or double-click a card to create immediately.
  Cancel returns to the previous document without
  changing it. The last eight used template IDs are stored locally, separately
  from recent files. Ctrl+N still creates a blank map directly.
  Section chevrons open a category directly; All templates returns to the
  overview. Each view retains its own scroll position. Navigation clears the
  pending selection and never creates or changes a map.
  The main menu's New from Template submenu uses the same eight localized
  categories, with each template directly selectable in its category.
  Scenario maps include problem solving, architecture, workshops, research,
  language learning, retrospectives, everyday planning, comparisons and creative
  production. Gallery titles are localized in every supported UI language; starter document content
  is English and editable. These are original Thinoda maps, not XMind template copies.
- Copy/Paste Style (Ctrl+Alt+C/V) shares explicit topic-style overrides across open
  documents; Reset Style restores automatic theme inheritance. Multi-topic paste/reset
  is one undo step and does not replace text or hierarchy.
- Style > Quick Styles previews four original presets and named styles saved from
  a topic's resolved appearance. Saved colors and typography stay fixed across
  themes; content and hierarchy are never copied. Apply to multiple selected
  topics in one undo step, or use the command palette. Save from Style, Edit or
  the topic context menu; Remove requires confirmation. The local profile holds
  up to 32 unique names (1-80 characters); duplicate names are rejected, not
  silently replaced. Reset Style restores automatic theme inheritance.
- Map > Custom Themes offers a staged editor with actual-renderer previews, a
  built-in base, 1-16 palette colors, light/dark backgrounds, fonts and defaults
  for central, main, sub, floating and Summary topics. Apply Theme changes the
  sheet in one undo step; Save to Library stores a separate named profile copy
  without changing the map. Up to 32 unique themes can be saved, edited or removed
  with confirmation. Themes are embedded in documents; library removal does not
  change existing maps. Explicit topic and map overrides take precedence; Reset
  Style inherits role defaults, and selecting a built-in theme removes custom
  defaults. Optional recent-theme reuse also supports custom themes.
  Native files and unchanged Thinoda XMind snapshots preserve authoring data.
  Public XMind export materializes supported light-mode topic properties, not an
  editable XMind theme registry or verified native visual parity.
- Indent/Outdent in Edit, the topic context menu and command palette moves one topic
  under its previous sibling or after its parent while retaining its annotations.
  The Outliner also exposes sibling/child insertion, topic editing, move/reparent,
  and both subtree or topic-only deletion controls; these use the same undoable
  canvas actions as the map view. Its action rows replace the floating Map
  insertion toolbox, so the two toolbars do not overlap. Format and Symbols remain
  available from the top bar; the Notes/Hyperlink shortcuts open their Map inspector.
- Insert > Notes / Hyperlink opens the selected topic's inspector field using the
  existing Ctrl+Shift+N / Ctrl+K shortcuts. These commands require one selected
  topic; they commit an in-progress topic-title edit before switching views.
  Previous/Next Sheet (Ctrl+PageUp/Down) wraps between sheets and commits pending title edits.
- Sheet menu and sheet-tab context menu offer Copy/Paste of complete sheets through
  the OS clipboard (including across Thinoda instances), and Save Sheet As for a
  separate `.thinoda`/`.xmind` copy without changing the source save state.
  New Sheet from Topic copies a branch with its owned annotations and internal
  relationships; relationships to topics outside the branch remain in the original sheet.
- Select submenu offers visible siblings, descendants, inversion and central-topic
  navigation. Arrange offers natural child-title sorting and floating-topic edge/
  center alignment or equal center spacing, each in one undo step.
  Sorting is rejected if it would split a Summary/boundary's source membership.
- Tools > Merge Map Files appends complete `.thinoda`/`.xmind` sheets in one undo step.
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
- Touch navigation in the shared desktop/web canvas: drag empty background with
  one finger to pan; use two fingers to pan and pinch simultaneously, keeping the
  map point under the gesture center. Pinch uses the same 10%-500% map-zoom limits
  as other zoom controls. Trackpad scroll and pinch can also apply together.
  Adding a second finger cancels a pending topic/relationship drag; navigation
  remains separate from editing until all fingers lift. Canceled touch input
  discards pending drops rather than moving topics. Mouse background dragging
  retains rubber-band selection; keyboard and visible navigation controls remain
  available. Tap a topic to select it, double-tap to edit its title, or hold one
  finger for context actions. Multitouch holds cannot open context menus, even
  after returning to one finger until all contacts lift.
  Full device/gesture acceptance is still in progress; see
  [touch and trackpad coverage](docs/WEB.md#touch-and-trackpad-coverage).
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
  files and unchanged Thinoda XMind snapshots retain them; public XMind side interchange
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
  and unchanged Thinoda-exported XMind files preserve custom routing; public XMind
  curve-control interoperability remains unfinished.
- Relationship creation connects exactly two selected topics immediately. With one
  source selected, pick an existing target or click blank canvas to create a connected
  floating topic and edit its title. Creation and connection undo together; Escape
  cancels picking, and failed connections leave no orphan topic.
- Find (Ctrl+F) in the active sheet or all sheets, including optional notes/labels,
  with sheet-aware navigation. Replace (Ctrl+H) updates topic titles in one undo step,
  including replacements across all sheets; notes and labels are search-only.
  Controls wrap and scroll into view with keyboard focus in narrow/short windows;
  replacement drafts survive resizing. The compact card may temporarily cover
  the inspector. Outline shortcuts yield while search is open, so Tab navigates
  controls rather than adding topics.
  Find also offers **Filters** for markers, labels, priority, independent To-Do
  completion and scheduled Task completion. Values within one group match any;
  different selected groups must all match. Priority checks both topic markers
  and scheduled Task priority. **View All Notes** lists note previews with
  sheet/topic names; its search text matches notes only. Results are read-only,
  with explicit topic buttons to navigate; Replace is disabled in notes mode.
  The first 100 rows are listed; Previous/Next traverses all matches.
  **Dim others** preserves matching topics and ancestor context in both views.
  **Clear search and filters** removes criteria without changing documents,
  history or saved folding. Explicit result navigation retains the existing
  undoable ancestor expansion for a collapsed target.
  In Outline, structural shortcuts apply only to topic rows; toolbar/sidebar
  focus keeps its own keyboard behavior, while non-text Undo/Redo stays available.
- Shared right sidebar: Format Panel for Style/Map and the smiley for grouped Symbols;
  click the active selector again to hide/reopen it. Map theme previews wrap to fit,
  and Style/Map tabs stay visible while their contents scroll independently.
  Style reflects the selected topic's resolved properties, including inherited/custom
  colors with checked swatches and hex values; Map controls apply to the entire sheet.
- Symbol colors: choose a color (picker or keyboard-editable hex) before inserting
  from either Symbols surface. Placed symbols expose individual color, default-color
  reset, measured contrast and **Remove Symbol** controls in the Symbols panel;
  right-click a particular canvas symbol for the same editor/removal action.
  Canceled or multitouch releases cannot apply symbol insertion/removal or
  default-color/contrast actions; swiping the Symbols inspector only scrolls.
  In that context menu, **Symbol color** opens a scrollable color submenu that
  remains usable in short windows; the sidebar retains its color swatch picker.
  Changes are undoable, keep the one-per-category semantics and preserve legacy
  default rendering. Contrast uses WCAG relative luminance against the composited
  topic fill/sheet background (including boundary overlays): graphics require 3:1,
  month/week labels 4.5:1. **Improve contrast** explicitly applies black or white;
  no automatic theme-driven document recoloring or overall WCAG compliance is claimed.
  Insertion feedback uses the primary topic; each candidate tooltip reports the
  lowest ratio across the selected topics, so mixed backgrounds may still warn.
  Native files, topic/sheet clipboard, duplication and unchanged XMind files reopened
  in Thinoda retain colors. XMind has no supported per-marker color field: other
  XMind applications show standard icons, and external edits invalidate the private
  Thinoda payload and can discard custom colors. PNG/JPEG/SVG/PDF and printing
  render custom colors (text glyph appearance still depends on available fonts).
  FreeMind, OPML, Markdown, plain text, TextBundle, Word, Excel and CSV cannot
  preserve these colors: GUI export requires confirmation **before** opening a save
  dialog, and the I/O API refuses by default. CLI conversion must explicitly pass
  `--allow-symbol-color-loss` to produce these lossy formats. External plain-text
  clipboard content has no color fields; internal topic clipboard retains full fidelity.
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
  Settings stays inside short/narrow windows with both-axis scrolling and focus
  reveal; API-key actions and long cloud-folder paths no longer push actions offscreen.
  Keyboard Shortcuts in Settings opens the existing searchable reference; closing
  it returns to the same preferences category. Shortcut rebinding is not yet available.
  Settings switches, segmented choices and color swatches reject canceled or
  multitouch releases, including delayed synthesized clicks. Ordinary clicks and
  keyboard activation remain available.
  Style, structure, theme and template preview cards use the same protection;
  rejected double taps cannot create a map from a template.
  The zoom menu offers 10–500% presets, the current value and Fit Map.
  The topic-count menu shows current-sheet topic, word and Unicode character statistics.
  Sheet tabs and bottom controls scroll independently in narrow windows; New Sheet
  stays visible. Fixed left/right arrows browse sheet names without switching the
  active sheet, and disable at either end. Names are clipped away from the arrows,
  New Sheet and status controls. Long tab names are shortened visually, with full
  names in tooltips. Touch/mouse dragging, mouse wheels, horizontal scrolling and
  keyboard focus keep the actions reachable; focused or newly active sheets reveal
  themselves without pinning manual browsing. On small screens the status controls
  also scroll independently, rather than covering sheet names.
  Open-document tabs now use the same fixed-arrow, clipped-name pattern at the top;
  Close Tab and New Tab stay pinned while file names scroll. Browsing does not
  switch documents, and the narrow header prioritizes tabs over quick-action icons
  that remain available in the main menu.
  Canceled or multitouch releases cannot switch/create sheets, trigger rename,
  scroll arrows or sheet context actions. Single-finger holds still open the
  sheet menu; ordinary taps and keyboard navigation remain available.
  Status view/zoom/Fit actions and Zoom/Statistics opening also reject canceled
  or multitouch releases, including zoom-menu presets and Fit.
  The sidebar remembers its Format/Symbols family and last Style/Map tab across
  restarts, independently of whether the panel is hidden.
  The bottom bar shows the current Map/Outliner view; click it or use
  Ctrl+Shift+O (Command+Shift+O on macOS) to switch without discarding a title draft.
  Deep Outline trees scroll in both directions. Keyboard selection and focused
  title editors reveal themselves without pinning manual scrolling; scroll
  positions are scoped to each document and sheet. One-finger swipes scroll the
  Outline without selecting or editing rows. Multitouch/canceled releases cannot
  activate its disclosure buttons, topic rows or editing toolbar, including when
  the synthesized pointer release arrives after the last touch contact.
  In short windows, Outline actions scroll separately from topics and the selected
  summary is shortened visually with full text on hover, keeping the list reachable.
- Import: XMind, FreeMind/Freeplane, OPML, Markdown, text, MindManager `.mmap`
  and standalone `.xmmap`
- XMind reopening honors edits made by other applications rather than silently
  restoring a stale private snapshot. Unchanged Thinoda exports retain lossless
  reopening; supported public theme defaults and inline overrides are imported.
- Export: XMind, PNG, JPEG, SVG, PDF, Word, Excel, CSV, Markdown, OPML, TextBundle,
  and a bounded native MindManager `.mmap` subset. MindManager conversion uses
  explicit loss/sheet-scope warnings and protects imported source files;
  [supported features and verification limits](docs/MINDMANAGER.md).
- Graphical exports share font-measured title wrapping and reserve whitespace
  inside node borders, including multiline text and shaped nodes. PDF export
  preserves vector shapes, connectors and outlined text instead of embedding a
  page-sized bitmap, so zooming stays sharp. Outlined PDF text is not selectable.
  This works on Windows, macOS, Linux and the web; raster scale only affects
  PNG/JPEG. The separate **Save print PDF...** path remains paper-sized raster output.
- Crash safety: atomic saves plus a recovery journal that restores unsaved work after a crash
- Cloud: opens/saves directly in OneDrive, iCloud Drive, Dropbox, Google Drive, Box, Nextcloud folders
- AI assistant (optional): brainstorm subtopics, break down into to-dos, explain, summarize,
  generate a whole map — with any OpenAI-compatible server

## AI setup (LM Studio)
1. Start LM Studio, load a chat model (e.g. `google/gemma-4-e4b`) and enable the local server.
2. In Thinoda open **Settings › AI Assistant**, set *Server URL* to `http://127.0.0.1:1234/v1`
   (or a LAN server such as `http://192.168.1.216:1234/v1`), leave *Model* empty for automatic
   selection and click **Test Connection**.
3. Select a topic and press **Ctrl+Shift+I** or use the **AI** menu.

Ollama (`http://127.0.0.1:11434/v1`) and OpenAI (`https://api.openai.com/v1` + API key, or env
`THINODA_AI_API_KEY`) work the same way. API keys saved in Settings are kept in the operating
system credential store (Windows Credential Manager, macOS Keychain, or Linux Secret Service),
not in `settings.toml`.

## Build & test

JavaScript tooling and PWA tests use **Node.js 24**, selected by the root
`.node-version` file. There are no npm dependencies or Node runtime requirements
for the shipped desktop app; the web app runs WebAssembly/JavaScript in the browser.

```sh
cargo run -p thinoda-app            # start the app
cargo run -p thinoda-app --example isolated_native # disposable desktop acceptance profile
cargo xtask ci                    # fmt + clippy + all tests
node --test crates/thinoda-web/tests/pwa.test.cjs # Node.js 24
cargo test -p thinoda-app --features gpu-tests --lib map_shot_gpu_readback # requires a GPU adapter
cargo xtask package               # installers in target/packages
thinoda --convert map.xmind map.md  # CLI conversion
```
The `isolated_native` example calls the production desktop entry point with a
temporary `THINODA_HOME`, set before services or threads start. Settings, recovery
and single-instance state are isolated from your normal Thinoda profile. Its
printed home directory is removed on normal exit; save any evidence outside it
before closing. Explicitly opened/saved external files and the OS credential
store are not isolated: use test maps only, leave AI disabled and do not enter
credentials. This is a manual native acceptance host, not an automated UI test.

Web app (WebAssembly, needs `rustup target add wasm32-unknown-unknown` and
[Trunk](https://trunkrs.dev)): `cd crates/thinoda-web && trunk serve`, or
`trunk build --release` for the static site. Browser alternatives for files,
printing, recovery and the AI key are described in [docs/WEB.md](docs/WEB.md).
Linux build dependencies: `pkg-config libfontconfig1-dev libgtk-3-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev
libxcb-xfixes0-dev libwayland-dev`.
Native printing also needs a C compiler (MSVC on Windows, Xcode/Clang and the
AppKit SDK on macOS, GCC/Clang on Linux). Job-free native Linux integration checks:
`python3 -m unittest discover -s .github/scripts -p test_native_printing.py -v`.
They compile the bridge without a GTK link and check missing-display errors.
With `xvfb-run` installed they open the real GTK printer dialog and **only cancel**
it. Rust geometry, conversion, thread checks, modal UI and EN/DE tests never send jobs.

Release packaging is ordered: macOS Intel first; Windows x64/ARM64 and Linux x86
next (in parallel); Linux ARM64 after Linux x86; Apple Silicon after Windows and
both Linux builds. Publication requires all six targets to pass. A shared reusable
packaging workflow preserves signing, notarization, verification and per-target
checksums. JavaScript GitHub Actions use their Node 24 runtime versions (including
the nested Pages uploader); shell-only composite actions do not require Node.
The current hosted runners support these actions; self-hosted runners would need
at least Actions Runner 2.327.1 (2.329.0 for authenticated Git in container actions).

The live AI test uses `THINODA_AI_TEST_URL` (or the known LM Studio hosts) and is skipped when no
server answers or when `THINODA_AI_SKIP_LIVE=1`.

CPU performance baseline:
```sh
cargo run --release -p thinoda-core --example layout_benchmark -- --topics 10000 --samples 30
cargo test -p thinoda-core --example layout_benchmark
```
The benchmark reports JSON p50/p95/max layout, undoable leaf-rename and undo times
for balanced and wide maps. Use `--output` to save a report and
`--layout-budget-ms` to fail on a chosen p95 limit. This uses approximate text
measurement without a GPU or viewport and is **not** a 60-fps claim. CI uploads
per-platform reports; see [the plan](docs/PLAN.md) for the measured Windows baseline
and remaining GPU profiling work.

`cargo test -p thinoda-io --test import_mutations` runs deterministic truncation,
byte-mutation, structured XML/JSON and oversized-ZIP-entry smoke checks. Every
accepted import must validate and round-trip through native storage. These
bounded tests run with the workspace suite; they do not replace long-running,
coverage-guided fuzzing. The separate nightly `cargo-fuzz` targets and bounded CI
smoke commands are documented in [`fuzz/README.md`](fuzz/README.md).

## Packages
| Platform | Formats | Command |
|---|---|---|
| Windows x64 | NSIS installer (`.exe`) | `cargo xtask package --target x86_64-pc-windows-msvc --formats nsis` |
| Windows ARM64 | NSIS installer (`.exe`) | `cargo xtask package --target aarch64-pc-windows-msvc --formats nsis` |
| macOS | `.app`, `.dmg` | `cargo xtask package --formats app,dmg` |
| Ubuntu LTS | `.deb`, AppImage | `cargo xtask package --formats deb,appimage` |

Run each command on its corresponding operating system with the required Rust
target and native build tools installed. Commands without `--target` build for
the host architecture. These commands build packages; they do not install the app.
macOS packaging signs the complete `.app` bundle before creating its DMG. The
release workflow verifies both downloadable formats with `codesign --verify
--deep --strict`, checks the DMG with `hdiutil verify`, and verifies and smoke-tests
an app copied from the mounted DMG before uploading any macOS artifacts.

## Releasing
Versions follow [Semantic Versioning](https://semver.org) and are automated by
[release-please](https://github.com/googleapis/release-please):

1. Merge pull requests to `main` with
   [Conventional Commit](https://www.conventionalcommits.org) titles:
   `feat:` bumps minor, `fix:` bumps patch and `feat!:` or `BREAKING CHANGE:` bumps
   major. While Thinoda is `0.x`, breaking changes bump minor.
2. The **Release Please** workflow keeps an open release PR up to date. It bumps
   the version in `Cargo.toml` and `Cargo.lock` and adds the entries to
   [CHANGELOG.md](CHANGELOG.md).
3. Merging the release PR tags `vX.Y.Z` and creates the GitHub release. It then
   runs the release workflow to build and test all five targets and attach the
   installers and checksums.

One-time repository setting: **Settings > Actions > General > Allow GitHub
Actions to create and approve pull requests** must be enabled for release-please.
Commits pushed with `GITHUB_TOKEN` do not trigger other workflows, so the release
PR's lockfile commit does not run CI by itself. Close and reopen the PR or push
another commit to get CI checks.

Pushing a `v*` tag manually also runs the release workflow, which builds every
package and publishes a GitHub release.
To rebuild and publish an existing tag, use **Actions > Release > Run workflow**
and enter the tag.
The workflow uses native Windows x64/ARM64, macOS Apple silicon/Intel and Ubuntu
22.04 x64 runners, testing each target before packaging with cargo-packager 0.11.8.
Running it without a tag produces artifacts only.
Downloads have target-specific filenames and SHA-256 manifests; macOS `.app`
bundles are provided as `.app.tar.gz` to preserve permissions and symlinks, alongside
the `.dmg`. Workflow artifacts are retained for 30 days. Releases are published only
after all five builds succeed, with the installers attached as release assets.
Windows installers are not configured with production code-signing certificates.
macOS releases require these repository Actions secrets (also forwarded by Release Please):

| Secret | Value |
| --- | --- |
| `MACOS_CERT_P12` | Base64-encoded Developer ID Application `.p12` certificate and private key |
| `MACOS_CERT_PASSWORD` | Password used to export the `.p12` |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
| `KEYCHAIN_PASSWORD` | Random password for the temporary build keychain |
| `APPLE_API_KEY_P8` | Base64-encoded App Store Connect `.p8` API key |
| `APPLE_API_KEY_ID` | App Store Connect API key ID |
| `APPLE_API_ISSUER` | App Store Connect issuer ID |

Both macOS runners import the certificate into a temporary keychain. The pinned
packager signs the executable and complete app bundle with hardened runtime and
secure timestamps, then notarizes and staples the app before embedding it in the
signed DMG. The workflow also notarizes and staples the DMG before creating downloads
and checksums. Extracted and installed apps must pass signature, stapling and
Gatekeeper checks before upload. Missing credentials or rejected notarization fail
the release; credentials are removed even on failure. Confirmed Apple CloudKit
connection-loss errors during app packaging, stapling or ticket/Gatekeeper
validation get up to five attempts with 15/30/60/120-second backoff.
DMG retries reuse the accepted submission instead
of resubmitting it. Other errors and exhausted retries still fail the release;
signature, stapling and Gatekeeper checks are never bypassed. Local `cargo xtask package`
builds retain ad-hoc signing and do not require these secrets.

Release DMGs are created directly as compressed HFS+ images from a temporary
staging directory, rather than customizing an attached writable image. The staged
signed app, Applications shortcut, volume icon and license are closed before
creation; the embedded license prompt is added before signing/notarizing the DMG.
This removes the old `create-dmg` writable-mount/eject race without opening Finder
or changing system-wide indexing settings. As before, CI does not customize a
Finder window layout.

Download verification attaches only read-only, hidden volumes and executes the
installed copy, never the app on the volume. Cleanup correlates the exact image
path with its device using `hdiutil info -plist`, including partial attach failure.
Busy detaches log scoped `lsof` diagnostics and retry after 2/4/8/16 seconds; only
that owned read-only device may then be force-detached. Pre-existing, unexpected
or ambiguous attachments are not touched. A remaining attachment or failed
cleanup fails the job; verification errors are not converted into success.
Native macOS tests cover license resources, installation/signature preservation,
and no leftover attachment after successful and failed checks.

## Docs
[Plan](docs/PLAN.md) · [Backlog](docs/BACKLOG.md) · [Architecture](docs/ARCHITECTURE.md) ·
[Web app](docs/WEB.md) ·
[XMind research](docs/XMIND-RESEARCH.md) · [User-guide checklist](docs/XMIND-USER-GUIDE.md)

## Project
[Changelog](CHANGELOG.md) · [Code of conduct & contributing](CODE_OF_CONDUCT.md) ·
[Security policy](SECURITY.md) · [Third-party notices](THIRD_PARTY_NOTICES.md) ·
[Impressum](IMPRESSUM.md)

## License
Copyright © 2026 Axel Wolters. Licensed under the
[European Union Public Licence v1.2 (EUPL-1.2)](LICENSE.md).

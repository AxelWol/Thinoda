# yMind backlog

Prioritized ideas beyond the shipped phases. P1 = next, P2 = soon, P3 = later.
Execution order, dependencies, acceptance criteria and release gates are defined in
[PLAN.md](PLAN.md#remaining-work-delivery-plan-2026-10-16). Reference inventories below
are not lists of wholly missing features; audit and reuse existing implementations.

## Recently completed
- XMind external-edit coherence: lossless private snapshots are bound to canonical
  public `content.json` with SHA-256. Changed public maps take precedence over stale
  metadata, including topic/sheet deletion and replacement; JSON formatting changes
  alone do not discard the snapshot. Older undigested exports are accepted only
  when their generated public representation matches. Invalid private data is
  logged and ignored, never used to hide corrupt public JSON. This is a coherence
  check, not authentication or encryption. Public-only content still cannot
  preserve fields that the public exporter does not yet support.
  Supported per-role theme properties (central/main/sub/floating/Summary) now merge
  beneath inline overrides and are materialized as topic styles. This follows the
  [official viewer example](https://github.com/xmindltd/xmind-viewer/blob/master/example/content.json);
  it does not add editable imported themes, style-ID registries, assets or full
  native visual fidelity. Rich unmodified and externally edited round trips,
  invalid/legacy snapshots, role precedence and native reopening are covered.
- Tools > Map Shot (F7) uses native GPU canvas readback, not the approximate
  export renderer. A clean frame omits menus, toolbars, selection and search
  dimming; DPI-aware cropping retains only the canvas. The frozen review supports
  pointer-region selection, accessible pixel coordinates, square crops, frame
  color/thickness, PNG save and native image clipboard output. Escape/Cancel
  discards the capture, including pending/late callbacks; timeout/unsupported
  renderer, invalid dimensions and output failures are explicit. Captures are
  limited to 32 Mi pixels. The document, selection, camera and save destination
  are unchanged (pending title edits commit before capture). This captures the
  visible map only; use Export for a complete sheet. F7 is used on every platform;
  the conflicting macOS reference shortcut remains assigned to the command palette.
  Unit/UI coverage includes exact pixels/alpha/PNG, fractional DPI, bounds,
  cancellation/timeouts, numeric and drag controls, narrow footer and German UI;
  an opt-in wgpu test verifies actual GPU readback. Native macOS/Linux clipboard
  verification and broader rich-content export fidelity remain open.
- Home template chooser uses rendered, appearance-aware previews of the exact
  documents Create opens, rather than generic map icons. Eight category filters
  plus All, grouped sections, selected-card feedback and a fixed Create/Cancel
  footer are implemented. Selecting a card does not create a document; changing
  the filter clears that selection. Cancel returns by document key, not stale tab
  index. Closing another tab preserves the current document or Home.
  Recent template IDs are persisted locally, deduplicated and capped at eight,
  independently of recent files. Five original Knowledge Base/Learning Plan/
  Weekly Plan/Decision Review/Creative Workflow maps bring the catalog to 17.
  Previews and keyboard activation are shared reusable UI/canvas helpers.
  Native/XMind round trips cover every template; English/German selection,
  cancellation, category filtering, restart persistence and narrow/wide bounds
  are covered. Further reference-specific content and section navigation remain open.
- Map structure preview gallery: current-layout thumbnail/name opens an anchored,
  bounded scrollable popup with nine collapsible families and responsive one-to-three
  column cards for all 17 supported structures. Original samples use the real core
  layout and canvas painter, cached separately for light/dark. Selection is highlighted;
  keyboard/accessibility activation, cancellation, family collapsing, narrow/high-scale
  bounds, all-layout rich reopening and atomic undo/redo are covered in EN/DE.
  Popup keys no longer insert/change topics in the underlying map or outliner;
  outliner leaf placeholders are noninteractive and branch controls have names.
- Tools > Merge Map Files appends complete `.ymind`/`.xmind` sheets atomically,
  preserving source files, destination metadata and all current model fields.
  Invalid/missing inputs abort before merging; one undo removes all appended
  sheets. Public XMind boundary/Summary/relationship IDs are now globally unique
  across duplicated/merged sheets. External images remain references until R4
  embedded-asset support; source workbook metadata is not merged.
- Tools > Create Masked Copy produces a separate unsaved preview, with generic
  sheet/topic/relationship titles and no notes, labels, links, images, markers,
  custom font names, boundary titles or source metadata. Original documents retain
  their paths, content and history. Cancel leaves the original alone; the preview
  uses ordinary Save/Export and prompts on unsaved close. Structure, colors and
  task states remain visible, so this is not a guarantee of anonymity.
  Tests inspect decompressed native/XMind public and private archive payloads.
- Select/Arrange submenus in Edit and topic context menus, with localized palette
  entries and shared canvas command availability: visible sibling/descendant
  selection, inversion, select none/central topic, Move Up/Down, natural ascending/
  descending child-title sorting, six floating-topic edge/center alignments and
  horizontal/vertical equal-center distribution. Alignment requires at least two
  floating roots, distribution three; mixed tree/floating selections are disabled.
  Arrangement is one undo step and independent of camera zoom/pan. Sorting remaps
  Summary/boundary source ranges to preserve membership, or rejects the edit
  without mutation if that membership would become noncontiguous.
- Complete-sheet Copy/Paste through a versioned OS text clipboard payload, including
  floating topics, styles, notes and nested structural annotations. Works across
  yMind instances; canvas Paste recognizes the payload, while Paste Sheet explicitly
  rejects unrelated/invalid data. Payloads are validated and capped at 16 MiB;
  late responses, document switches and empty clipboard responses are handled.
  Pasting adds a sheet in one undo step. This is yMind clipboard interoperability,
  not compatibility with XMind's private clipboard format.
- Save Sheet As writes only the active sheet to `.ymind` or `.xmind`, retaining
  workbook metadata without retargeting or marking the source document saved.
  The source path and its canonical aliases are protected from accidental overwrite.
  Sheet menu, tab context menu and palette expose these actions with EN/DE labels;
  rich-content reopen, invalid-input, cancellation and error regressions are covered.
- New Sheet from Topic copies one selected topic's subtree, its recursively owned
  summaries/boundaries and internal relationships without changing the source.
  Unrelated floating trees and external relationships stay in the original sheet;
  a toast explains external relationship exclusion. Creation is one undo step.
- Individual relationship/boundary/Summary selection and deletion via canvas
  outlines/braces, Delete/Backspace and named contextual actions, with undo/redo.
  Covered branches are preserved. Summary deletion removes its owned topic/children
  and their annotations; the context menu explains that scope. Tests cover
  pointer selection at multiple zooms, light/dark context menus and native/XMind
  saved-file reopening after deletion.
- Working General, Editor, AI and Cloud settings categories with scrolling.
  Editor has independently persisted topic-toolbar/add-button visibility, initial
  canvas zoom and opt-in last structure/theme defaults for new blank maps.
  Explicit templates retain their own structure/theme. Calendar, Sharing,
  Shortcuts, smart guides and font preferences remain below; no inactive shells added.
- Zoom dropdown with the reference presets, one selected current/non-preset value
  and Fit Map; map zoom remains separate from UI scale.
- Topic-count statistics popover for the current sheet: all topic titles, including
  collapsed/floating/summary topics; words split on whitespace, characters count
  Unicode scalar values. Notes/labels are excluded and the counting scope is explicit.
- Find across the active sheet or all sheets with sheet-aware navigation, optional
  notes/labels and sheet-scoped dimming. Replace All changes topic titles atomically
  across the chosen scope with one undo/redo step; notes/labels remain search-only.
- Summary braces connect to their independent editable topic, highlight the selected
  source range including visible descendants, and follow left/right split-map sides.
  Editing/children, sibling edits and undo/redo are covered. Range resizing and
  mixed-side ranges still need verified reference semantics.
- About yMind includes Axel Wolters' author credit and contact email in EN/DE.
- Fixed XMind rejecting exports: archive metadata now uses a creator object and
  data-structure version 2, matching XMind's expected format. Confirmed the old
  failure and successful corrected-file opening in the installed Windows XMind app.
  Added public archive contract/content coverage independent of the private workbook.
- Improved public XMind topic/line style class mappings, font/alignment/decorations/
  border/connector properties, relationship/boundary colors and structural element
  classes, including legacy topic-style parsing. Coverage imports public content
  without the private workbook payload. Style-ID registries, fixed widths, image/to-do
  export and legacy relationships/boundaries/summaries/floating topics remain gaps;
  these style changes do not imply verified native visual fidelity.
- Style shows the selected topic's resolved colors, including inherited/custom
  values outside the preset palette, with a checked swatch and hexadecimal readout.
  Missing fill/border colors are explicit. Map shows the effective sheet background
  and visible labels for its switches; its sheet-wide scope is clarified.
- Fixed Map-panel theme cards overflowing outside the sidebar. Style/Map navigation
  now stays above the scroll area, with separate scroll positions for each tab.
- Shared Format/Symbol sidebar selectors, with remembered Style/Map tab and preserved
  topic selection. Symbols reuse the existing catalog and undoable marker edits;
  multiple selected topics, per-group replacement and removal are supported.
  Sticker/Illustration/Pitch panes, legend/custom groups and task synchronization remain.
- Entering Zen mode displays a localized toast explaining that F11 exits it.
- Fixed the Windows Search crash caused by a dangling accessibility label reference.
- Floating topic toolbar for AI expansion, subtopics, relationships, links, notes, to-dos and style.
- Floating main editing toolbar with a compact insert popover, keeping the title bar uncluttered.
- Direction-aware child/sibling plus connectors; root-safe contextual menus and blank-canvas selection handling.
- Find & Replace with Ctrl+H, accessible search navigation, and refresh on document/sheet changes.
- Session-wide explicit topic-style Copy (Ctrl+Alt+C) and Paste (Ctrl+Alt+V), plus
  Reset Style back to inherited/default styling, including atomic multi-selection
  paste and reset with one undo/redo step each. Available from the Edit menu,
  topic context menu and command palette, with EN/DE labels; the command-palette/
  menu availability rules match selection state.
- Indent (moves a single selected tree topic under its previous sibling) and Outdent
  (moves it after its former parent), restricted to one selected non-root tree topic
  with no new shortcuts assigned yet. Owned nested Summary/boundary/relationship
  annotations move with the topic; a move that would create an annotation cycle
  (e.g. into its own owned summary topic) is rejected, and the vacated
  source range's boundary/summary membership is cleaned up correctly.
- Ctrl+PageUp / Ctrl+PageDown switch to the previous/next sheet with wraparound at
  the first/last sheet. An in-progress inline topic-title edit is committed before
  creating, duplicating, deleting, renaming, switching to, or reordering a sheet.
- These commands have localized menus and command-palette entries; topic commands
  also have context actions, with regression test coverage. The style/navigation
  slice passed full `cargo xtask ci`. The later sheet-workflow slice passes targeted
  core/app/IO/platform/UI suites and workspace clippy; its full-workspace attempt
  stopped on a Windows Application Control block of the core test executable.
  See PLAN.md for the exact scope. Live AI remains deliberately disabled.

## XMind parity (from the user guide)
- Implemented structural-object deletion for relationships (Beziehungen), boundaries
  (Umrandung) and summaries (Zusammenfassung), including selection feedback,
  Delete/context actions, undo/redo and saved-file reopening. Summary deletion uses
  the existing model ownership rule: its own topic tree is deleted, covered sibling
  branches are not. No XMind-specific alternative deletion behavior is implied.
- P1 Summary ("Zusammenfassung") interaction/visual parity, clarified by the
  supplied screenshot: a brace spans a range of sibling branches, including their
  descendant extents, and connects to a separate editable summary topic outside
  the range. This is a structural annotation, not AI text summarization and not a
  boundary. The reference shows a range selection frame with handles and a selected
  summary topic with contextual actions and a plus affordance.
  Reuse the existing summary range/topic model, insertion action and layout rather
  than introducing another content type. Verify range-handle behavior before
  implementing resizing; the screenshot alone does not establish its exact rules.
  Cover descendant-aware brace bounds, direction/layout changes, editing/children
  on the summary topic, sibling insertion/reordering/deletion, undo/redo and
  save/import/export fidelity.
- P2 XMind SDK interoperability: add support for workflows using
  [xmindltd/xmind-sdk-python](https://github.com/xmindltd/xmind-sdk-python).
  Audit the SDK's supported file versions and API coverage against yMind's existing
  Rust import/export before choosing an optional Python bridge or compatible
  file-exchange integration. Keep the native editor independent of a Python runtime.
  Add SDK-generated fixtures and bidirectional round-trip checks for topics,
  hierarchy, sheets, styles, markers, notes, links and relationships; document
  unsupported content and fidelity limits explicitly. Verify maintenance/runtime
  compatibility and preserve the SDK's MIT notices if code is redistributed.
- P2 Review additional XMind ecosystem sources for interoperability, typography,
  authoring and companion-app workflows (research references, not adopted dependencies):
  - [xmindltd/xmind-generator](https://github.com/xmindltd/xmind-generator):
    evaluate JavaScript-generated `.xmind` fixtures and round-trip compatibility
    with yMind import/export; compare supported content and style fidelity.
  - [xmindltd/nevermind](https://github.com/xmindltd/nevermind):
    assess the font's license/redistribution terms, script coverage, readability,
    fallback behavior and cross-platform rendering before considering optional use.
  - [xmindltd/xmindmark](https://github.com/xmindltd/xmindmark):
    assess the markup format and tools for text-based map authoring, import/export
    and interoperability with existing Markdown/outline workflows.
  - [xmindltd/xmind-cli](https://github.com/xmindltd/xmind-cli):
    inventory commands and supported formats; evaluate useful yMind CLI parity
    and conversion fixtures without assuming matching behavior.
  - [xmindltd/xmind-embed-viewer](https://github.com/xmindltd/xmind-embed-viewer):
    investigate embedding/export options for future companion/web apps, including
    whether local/offline viewing is supported versus a hosted sharing dependency.
    Keep document uploads and third-party transmission explicitly user-controlled.
  - [Xmind Ltd. GitHub organization](https://github.com/xmindltd):
    track related projects and specifications as additional research sources.
  Verify each repository's current maintenance, license, runtime requirements and
  actual supported file versions before integration. Assign implementation to the
  relevant interoperability, typography, sharing or companion package after review.
- P1 Print (Ctrl+P) with page setup and fit-to-page
  - Reference dialog: left live page preview with page count; right scrollable,
    grouped settings; fixed Cancel/Print footer.
  - Printer selection, copies, content scope (Current map shown), printer-supported
    paper sizes, landscape/portrait and scaling (Fit page shown). Verify other
    scope/scaling options rather than guessing from closed dropdowns.
  - Background-color and monochrome toggles, reflected in the preview and output.
    The displayed paper size is a selection, not a required default.
  - XMind's attribution toggle is product branding, not a requirement to print its
    name or add a watermark to yMind. Decide any yMind attribution separately.
  - Cover no printer, unavailable printer, driver capability changes, invalid copy
    counts, multi-page navigation, cancellation and spool errors; never send a real
    print job as an automated smoke check without explicit user approval.
- Cross-sheet Find and title replacement are implemented; notes/labels are explicitly
  search-only. Rich-field replacement needs separately specified behavior.
- P1 Pitch mode (presentation from branches, F5) and slide export
- Delivered Map Shot (F7): visible-canvas region capture to clipboard/PNG (see above).
- P2 Zone (Ctrl+Alt+Z) and callouts editing UI polish
- P2 Gantt view for to-dos with start/end dates, progress and export
- P2 Equations (LaTeX → rendered topic content)
- P2 Attachments and pictures embedded in the `.ymind`/`.xmind` package
- P2 Password-protected files (AES-GCM, Argon2 key derivation)
- P2 Quick styles, global font, custom theme editor. Copy/Paste style and Reset
  Style are implemented session-wide (Ctrl+Alt+C/V, menu/context/palette); see
  Recently completed.
- P2 Skeleton/structure picker gallery (map-file merging and masked copies are
  implemented; see Recently completed)
  The structural gallery is implemented for the 17 existing structures; unverified
  visual skeleton presets and the distinct Grid family remain open below.
  - Map-tab layout selector: current layout thumbnail/name opens an anchored,
    scrollable gallery with collapsible families and a three-column preview grid
    in the reference. Implemented for existing structures with selected-state feedback
    and viewport-adaptive columns; additional presets still require reference research.
  - Observed families: Mind map, Logic chart, Brace map, Org chart, Tree chart,
    Timeline, Fishbone, Tree table, Matrix and Grid. Audit existing structures first;
    Grid is an additional family to investigate, not a synonym for Matrix.
  - Distinguish structural arrangements from visual skeleton variants (topic shapes,
    connector geometry and borders). Determine each preset's changes before applying
    it; do not infer model semantics from thumbnails alone.
  - Changing an existing sheet layout must preserve topics, hierarchy, relationships,
    rich content and explicit styles unless a documented preset intentionally changes
    them. Apply as one undoable edit, re-layout and keep selection/camera usable.
  - Map-tab reference also shows theme palette selection/add, multicolor and single-
    color variant previews, background color, global font and branch-line width.
    Reuse existing theme settings and verify partially visible controls.
  - Generate previews from original sample maps, not copied XMind thumbnails;
    commercial badges do not establish yMind feature restrictions. Test every preset,
    undo/redo, saved-file round trips, large/deep trees and keyboard popup navigation.
- P3 Audio notes (record/play)
- P3 Customizable shortcuts (Preferences > Shortcuts); detailed requirements below.

## Main menu parity (user-supplied XMind screenshots, 2026-10-16)
Reference inventory, not an implementation-status checklist. Reuse existing commands
and the feature backlogs below; do not duplicate actions or expose nonfunctional items.
Disabled entries in the screenshots are contextual states, not evidence of missing features.

- P1 Menu layout: rounded hamburger popup with separators, shortcut hints, hover
  highlighting and cascading submenus. Top-level quick actions: Undo, Redo, New blank
  map, Home, Open local file, Save, Save As, Export, Print and Settings; followed by
  File, Edit, Insert, Tools, View and Help submenus.
- P1 File inventory: New blank map, New, New tab, Home, Open local file, Recent
  documents, Save, Save As, File cache, Set file password, Import, Export, Share,
  Print, Close window and Quit. Verify what File cache manages and preserve recovery
  data until its lifecycle is understood. Treat tab/window closure and application
  exit distinctly, retaining unsaved-work prompts.
- P1 Edit inventory: existing Undo/Redo, Cut/Copy/Paste, Duplicate, Delete and
  Delete single topic still need a broader parity audit. Select/Arrange now expose
  visible sibling/descendant/inverted selection, central-topic navigation, natural
  title sorting and floating-topic align/distribute (see Recently completed).
  Verify additional reference choices and expand/fold scope. Indent/Outdent
  (single selected tree topic, no new shortcuts) and Copy/Paste/Reset style
  (session-wide, Ctrl+Alt+C/V) are implemented with matching menu/context-menu/
  palette availability; the rest of this inventory, and command-consistency parity
  generally, is not fully done.
- P1 Insert inventory: Child topic, Topic after/before, Parent topic, Floating topic,
  Callout, Relationship, Summary, Boundary, Note, Label, To-do, Task, Link submenu,
  Attachment, Audio note, Symbol, Sticker, Illustration, Local image, Equation,
  New sheet and New sheet from topic. Research the distinction between the displayed
  To-do and Task commands rather than assuming they are synonyms. New sheet from
  topic is implemented as a non-destructive single-branch copy; see Recently completed.
- P2 Tools inventory: Map Shot (F7) and Merge Map Files are delivered; Create/customize theme remains.
  Merge Map Files is implemented for current model fields; embedded assets remain
  with R4. Share implementation with the existing capture and theme backlog items.
- P1 View inventory: mutually exclusive Mind map/Outline views, Zoom in/out, Actual
  size, Fit map, Show branch only, Zen, Gantt, Pitch submenu, Navigation pane submenu,
  Format panel submenu, Toolbar submenu, Show tab bar, Hide sheet bar and Merge all
  windows. Verify submenu contents and multi-window behavior before implementation.
- P2 Help inventory: News, User guide, Feedback, Update and restart, Keyboard shortcuts,
  Quick start and About. Sign-in and Redeem gift card are observed XMind account/
  commerce actions, not requirements to add accounts or monetization to yMind.
  Never trigger updates/restarts, feedback transmission or authentication merely
  by reproducing the menu.
- P1 Command consistency: menus, context menus, toolbars, palette and shortcuts must
  dispatch the same commands with the same selection/document availability rules.
  Cover keyboard submenu traversal, Escape, focus restoration, viewport-edge
  placement/scrolling and EN/DE labels in both appearances.
- P2 Reference shortcut audit: record Windows mappings without silently replacing
  yMind bindings. The screenshots show Ctrl+H for Home, Ctrl+F for Find & Replace,
  Ctrl+Shift+P for Settings, Ctrl+R for the central topic, Ctrl+Shift+R for Relationship,
  Ctrl+Shift+B for Boundary, Ctrl+Shift+N for Note, Ctrl+Shift+L for Label,
  Ctrl+Shift+I for Local image and Ctrl+Alt+N for New sheet.
  Layout-dependent keys (# and umlaut keys in the reference) need physical/logical
  key and macOS/Linux verification. Preserve current shortcuts pending an explicit
  compatibility-keymap decision.
- P2 Bottom-left sheet-tab context menu: Rename, Copy, Paste, Duplicate, Delete,
  New sheet and Save sheet as a separate file are implemented, with last-sheet
  protection and undo for document changes. Copy uses a validated, versioned yMind
  OS text payload; separate saving retains all current sheet-model fields, not other
  sheets. Future embedded assets need integration when their storage contract exists.
  Reference separator polish, native keyboard access and viewport-edge placement
  still need auditing.
  Previous/next sheet navigation (Ctrl+PageUp/Ctrl+PageDown, with wraparound and
  commit of an in-progress inline title edit before switching) is implemented
  separately from this context-menu inventory; remaining audits stay open.

## Bottom-right controls and alternate views (user screenshots, 2026-10-16)
- P1 Compact floating bottom-right action bar: topic count, zoom dropdown, Gantt,
  Map/Outline toggle, four-corner view action and presentation action, separated into
  groups. Use accessible tooltips/names and responsive placement without covering
  scrollbars or editable topics. Verify the four-corner action (fit/fullscreen) and
  presentation action behavior before assigning commands based only on their icons.
- Implemented statistics popover at the topic count: current-sheet topic, word and
  character totals using topic titles only, whitespace-separated words and Unicode
  scalar values. Selection-scoped statistics and alternative counting conventions
  are not implied; the popover explains its scope.
- Implemented zoom dropdown: reference presets 500%, 400%, 300%, 200%, 150%, 120%, 100%, 80%,
  50%, 20% and 10%, plus the current non-preset value (112% in the screenshot) with
  a checkmark, followed by Fit map. Keep map zoom distinct from UI scaling and
  preserve an intentional zoom anchor; test clamping and current-value deduplication.
- P2 Gantt action opens a separate window. Reference shows a left task table
  (Task name, Start date, Duration), a right dated timeline grid, zoom minus/slider/plus,
  and export/print icons. Empty state explains selecting a topic to add a task.
  Reuse the task/calendar model and existing Gantt backlog; verify unlabeled toolbar
  actions, date scale, task editing, dependency controls and cross-window synchronization.
  Closing the Gantt window must not close or lose the map.
- P2 Outline parity: prominent root heading, bold main topics, indented descendants,
  disclosure triangles for branches and bullets for leaves, plus a separated section
  for floating-topic trees. Bottom-right controls adapt to Outline, including return
  to Map, zoom, Gantt and presentation. Preserve hierarchy, selection and edits between
  views; verify floating-tree ordering and whether outline folding is shared with Map.
- P2 Coverage: live statistics after edits, both view-toggle directions, keyboard
  navigation, popup placement, light/dark appearances, narrow windows, mixed-script
  text and Gantt window lifecycle. Icon-only references are not verified behavior.

## Template gallery parity (user screenshots, 2026-10-16)
- Implemented generated previews, responsive grids, scrolling, fixed header/category
  filter, grouped sections, Recently used templates and fixed Cancel/Create actions.
  Selection enables Create without immediate creation. Section navigation chevrons
  and additional reference-specific content still need verified implementation.
- Implemented category filters: All, Basic, Knowledge management, Meeting and planning,
  Study and education, Project management, Entertainment and life, Analysis and
  decision, Creativity. Recent templates are separate from recent documents.
- P2 Expand original template content: basic Mind map/Logic/Brace/Org; problem solving,
  framework and architecture maps; business plans, workshops and dissertation plans;
  school timetable, note-taking, language learning and AR applications; project
  planning, BCG analysis, organization and nine-box matrices; weekly plans, shopping
  lists, mood tracking and service comparisons; cause/effect, comparisons and social
  media marketing; creative workflows, recipes and film production.
  Several reference titles are truncated: verify full names/content before specifying
  exact equivalents. Do not copy proprietary template text, thumbnails or paid assets.
  Five original maps now cover Knowledge Base, Learning Plan, Weekly Plan,
  Decision Review and Creative Workflow; this does not complete the expansion inventory.
- P2 Verify category filtering, recent-template persistence, keyboard selection,
  Cancel without document creation and creation from each template; cover narrow
  windows, display scaling and long localized names.

## Settings parity (user-supplied XMind screenshots, 2026-10-16)
These are reference requirements, not a claim that the settings are implemented.
The screenshots show selected values, not every available option or factory default.
Reuse existing persisted settings where applicable rather than introducing duplicates.

- P1 Extend the working General/Editor/AI/Cloud Preferences categories with Calendar,
  Sharing and Shortcuts as their features ship; refine icon navigation and grouped
  cards. Topic-toolbar and add-button visibility are independently configurable.
  The reference shows "Always" for the toolbar; verify additional modes before defining them.
- P2 General: retain system/light/dark appearance and language selection; add
  locale-aware first weekday with a system-following choice.
- P2 General: wire spelling-as-you-type to actual dictionaries and suggestions;
  distinguish automatic update installation from checking for updates.
- P2 General: optional usage statistics with explicit consent, a documented data
  inventory and no document contents; no telemetry is authorized by these screenshots.
- P2 Editor: smart-guide visibility remains. Initial canvas zoom and opt-in reuse of
  the last structure/theme for new blank maps are implemented; skeleton-specific
  variants still need the layout-gallery research.
- P2 Editor: default CJK font and mixed-script fallback for all open documents,
  with layout remeasurement and Windows/macOS/Linux font availability checks.
- P2 Calendar: selectable base/work calendars, month navigation, working-weekday
  toggles, non-working-day shading and dated exceptions; integrate with Gantt scheduling.
  Calendar management operations behind the reference overflow menu still need research.
- P3 Sharing: configurable destination visibility for email, blog, Facebook, X,
  Pinterest and Evernote. Verify current service APIs before committing integrations;
  require explicit user action before connecting accounts or publishing content.
- P3 Shortcuts: category filter, command search, editable bindings, unassigned state
  and reset-all. Validate conflicts and reserved platform keys; update menu hints and
  the command palette from the same bindings.
- P3 Shortcut command coverage from the reference: home, new blank map, new/template,
  merge windows, new sheet, sheet from topic, and delete a single topic while
  retaining children still need this customizable-shortcuts UI row. Previous/next
  sheet (implemented with fixed Ctrl+PageUp/Ctrl+PageDown) and copy/paste/reset
  style (implemented with fixed Ctrl+Alt+C/V; Reset has no shortcut) now exist as
  commands but still need an editable-binding row in Preferences > Shortcuts.
  The screenshot assigns Ctrl+H to Home, conflicting with yMind's existing Find &
  Replace binding; preserve current behavior until an explicit keymap choice is made.
- P2 Tests: settings persistence/migration, immediate application across open tabs,
  keyboard navigation, EN/DE layout, light/dark contrast, calendar exceptions and
  shortcut conflict/reset behavior.

## Canvas visual and interaction parity (user screenshots, 2026-10-16)
- P1 Drag-and-drop hierarchy refinement (partially delivered): full visible subtree
  ghosts now include owned Summary trees, boundaries and internal relationships;
  their original content is dimmed and proposed parents highlighted. Escape, popups,
  changed document/sheet/selection and lost drags cancel without edits. Self/owned
  descendant and outside-canvas drops fail visibly instead of detaching. Central
  and Summary roots are protected. Edge auto-pan is bounded to 480 logical pixels
  per second with a 50 ms frame cap. Top-most movable roots commit atomically,
  retain external relationships and expand an accepted parent. Eight new cases
  cover actual ghost content, collapsed subtrees, ownership, one-step undo/redo,
  cancellation, invalid drops and zoom/pan. Implemented sibling insertion slots:
  the outer quarters of an attached topic choose before/after in its connector axis;
  the center reparents. Reverse split-map ordering is respected. The shared target
  paints the slot/proposed connector and commits multi-topic order atomically.
  Main-topic drops on a split-map root choose left/right from the pointer, and
  Style > Branch side provides Auto/Left/Right. Explicit sides persist and override
  balancing without changing old automatic maps or other structures. Dropping into
  an unchanged slot is a no-op that preserves Summary/boundary ownership and history.
  Remaining: exact native reference comparison and public XMind side interchange.
  Preserve subtree contents and cross-topic relationships; distinguish hierarchy
  changes from independent dashed relationships ("associations").
  Verify exact target/ordering rules against XMind rather than inferring them from
  the static screenshots. Cover invalid self/descendant drops, root protection,
  cancellation, undo/redo, zoom/pan coordinates and viewport-edge auto-pan, plus
  a keyboard alternative. Official Topic/Floating Topic guides confirm dragging
  regular topics to blank space and floating topics back into branches; existing
  yMind gestures are retained. Flexible Floating Topics setting parity remains open.
  Sources: https://xmind.com/user-guide/topic-editing-new ,
  https://xmind.com/user-guide/floating-topic
- P1 Reference styling: plain bold central topic, rounded colored main topics,
  pale rounded first-level subtopics, deeper underlined topics, thin colored curved
  main branches and compact rounded sub-branch junctions. Clarity theme now provides
  this baseline for new templates; exact typography/spacing and visual comparison remain.
  Preserve saved themes/styles.
- P1 Presentation zones: named rectangular frames with a separate title badge,
  visible border and selection outline, as shown in the supplied screenshot.
  Support create/rename/move/resize/delete with undo; define presentation ordering,
  framing/zoom and Pitch navigation. Keep zones distinct from topic boundaries and
  test relationships crossing zone edges. The screenshot establishes appearance;
  clipping and navigation semantics still need verification. The official Floating
  Topic guide restricts topic grouping into Zones to floating topics; central topics
  and their subtopics cannot be included. Preserve that distinction when defining
  membership, rather than treating Zones as ordinary boundaries.
  Source: https://xmind.com/user-guide/floating-topic
- Implemented Relationships: retain the branch/custom color when selected; use a separate
  selection overlay with circular endpoint anchors, square Bezier control handles
  and thin guide lines. Endpoint reconnection and relative Bezier reshaping preview
  without modifying the document and commit in one undo step on release. Escape,
  invalid targets and stale-document previews cancel without overwriting work.
  Focused control arrows move by 1 map unit (Shift: 10); endpoint arrows cycle visible
  topics in layout order, excluding the opposite endpoint. Style provides title,
  endpoint, color and numeric control editing plus Reset curve. Shared geometry
  feeds layout/export; hit testing follows the rendered segments at different
  zoom/pan settings, including large curves. Native files and unchanged yMind XMind
  exports preserve routing through the validated snapshot. Public XMind control-point
  interchange and actual native keyboard/accessibility QA remain open.
- Implemented relationship creation parity from the official Floating Topic guide:
  exactly two selected topics connect immediately; one selected source starts picking
  an existing topic or a blank canvas location. Blank locations create a localized
  floating topic and its relationship atomically, then open title editing. Escape/
  outside clicks do not create targets; missing sources and self-targets fail visibly without
  orphan topics, and rapid double-clicks do not create a second topic. Six canvas
  cases and two app cases cover these flows, camera coordinates and one-step undo.
  Source: https://xmind.com/user-guide/floating-topic
- P2 Floating topics: match rounded colored cards and dashed colored relationships.
  Match the separate dashed gray rounded boundary around a floating topic shown in
  the reference; do not conflate a document boundary with a transient selection outline.
- P2 Visual fixtures: light/dark reference maps covering deep branches, floating topics,
  selected relationships, boundaries, narrow viewports and mixed-script text.
- Implemented Labels: shared anchored popover with a pointer, heading, comma-separated
  entry, removable chips, current-sheet suggestions and Auto sort. Insert, palette,
  topic/context/insert toolbars and Style > Edit labels share one document-bound
  draft. Apply commits once; Cancel/Escape/outside dismissal leaves source labels
  unchanged. Empty/duplicate labels are removed case-sensitively after trimming,
  with original order by default; opt-in sorting uses Unicode code-point order.
  The local sort preference is saved on Apply only. Existing comma-containing
  labels remain single chips. Changed/closed/switched documents, sheets, selection
  or content invalidate the draft visibly. Keyboard focus, undo/redo, native/XMind
  reopening, EN/DE controls, narrow/high-DPI bounds, long labels and large suggestion
  inventories are covered. Exact on-map tag rendering/reference polish remains open.
- P2 Expand the floating "+" content menu to match the reference grouping: Note,
  To-do and a Link submenu; Attachments and Audio note; Stickers, Illustrations,
  Local image and Equation. Reuse existing note/task/link actions. Verify Link
  submenu destinations rather than guessing; implement missing content types before
  exposing enabled actions. Use licensed/original sticker and illustration assets.
  XMind's "PRO" badges are product-specific and do not establish yMind licensing.
  Cover menu accessibility, dismissal/focus, insertion undo and packaged-file round trips.
- P1 Right-sidebar navigation: two adjacent buttons at the top-right of the canvas.
  The smiley selects the Symbol / Sticker / Illustration panel family; the panel
  icon selects the Style ("Stil") / Pitch / Map family. Show the active family
  and active tab distinctly, using one shared sidebar region rather than six
  unrelated panes. Preserve topic selection while switching families/tabs.
  yMind retains its existing repeated-click hide/reopen behavior and remembers the
  Style/Map tab within the session; switching families shows the target immediately.
  Reference parity for these gestures still needs verification. Symbol content is
  implemented; Sticker/Illustration/Pitch and their tab navigation remain.
- P1 Style pane reference: topic-style preview/preset selector followed by grouped
  Shape (fill, border and width/fit), Text (font family, size, weight, color,
  emphasis and alignment), Structure, Branch (geometry, line/end style, thickness
  and color) and Numbering controls, plus an apply-to-related-topics action.
  Reuse existing formatting commands; verify unlabeled dropdowns and the scope of
  "related topics" before defining behavior. Do not assume the displayed font is
  licensed or installed across platforms. Pitch and Map tab contents need their
  own references; the screenshot establishes their navigation, not their controls.
- P1 Right-side symbol pane: selection-aware insertion from Symbol, Sticker and
  Illustration tabs, opened with the smiley button. Provide scrollable,
  collapsible groups, an optional legend and a "New group" action. Reuse the existing
  marker model/actions rather than adding a separate icon system.
- P2 Symbol library coverage: colored tags, priorities 1-7, task progress through
  completion, flags, stars, people and general symbols (heart, thumbs up/down,
  pin, light bulb, lightning, hourglass, phone, pencil, music, game, 100, plane,
  runner, exclamation and question). Use original/licensed assets and accessible
  names; keep meaning distinguishable without relying on color alone.
- P2 Specify per-group replacement versus multiple-symbol behavior, removal,
  custom-group persistence and legend naming. Keep task/progress symbols consistent
  with to-do data; cover undo/redo, selection changes, no-selection disabling and
  `.ymind`/`.xmind` round trips. The screenshot does not establish these semantics.
- P2 Sticker pane: category dropdown with an All choice, collapsible Business and
  Education sections and a scrollable thumbnail grid (four columns in the reference).
  Insert into the current topic selection; define single/multiple-selection behavior
  explicitly. Keep stickers distinct from semantic markers. Provide licensed/original
  vector assets, accessible names, scalable rendering and persistence/export support;
  test asset loading failures, removal/replacement, undo and switching selected topics.
- P2 Illustration pane: a dedicated Illustration tab alongside Symbol and Sticker,
  with a search field and scrollable collection previews for attaching illustrations
  to the selected topic. Each collection has a heading, item count, navigation chevron
  and a three-thumbnail preview row in the reference.
  Observed collection styles include minimalist, dreamy, botanical, futuristic city,
  gradient stationery, black-and-white pixel art, watercolor animals and 3D animals.
  These guide original/licensed asset selection, not copying XMind's artwork or
  requiring its exact collection sizes or paid-content badges.
  Reuse shared asset loading, accessibility, undo and packaged-file storage where
  appropriate, while keeping illustration and sticker content types distinct.
  Collection-detail navigation, search matching, placement, resizing and
  multiple-selection behavior still need verification or a product decision.

## AI
- P1 Streaming responses (SSE) with live preview and cancel button
- P1 Store API keys in the OS keychain (Windows Credential Manager, macOS Keychain,
  Secret Service) instead of `settings.toml`
- P2 Semantic search/clustering with an embedding model (e.g. `nomic-embed-text`)
- P2 "Rewrite topic", translate branch, AI-assisted Pitch outline
- P3 Prompt library editable by the user

## AI — user-guide research addendum (2026-10-16)
Verified against xmind.com/user-guide pages not previously digested in
XMIND-USER-GUIDE.md/XMIND-RESEARCH.md; each item cites its source page. yMind's AI
is local/OpenAI-compatible only (LM Studio, Ollama, llama.cpp, OpenAI); treat every
item below as a capability gap to design for that transport, not as a requirement
to add XMind's own cloud account, billing or hosted AI service.

- P2 Confirmed absent: a persistent AI chat panel beside the map (opened from a
  toolbar button next to the title) with three selectable response-depth modes
  (Quick / Balanced / Deep) for simple edits vs. everyday mapping vs. multi-step
  research, operating on the active sheet or a topic selection. yMind's existing
  AI actions (brainstorm, breakdown, explain, summarize, generate map) are
  command-triggered one-shots without a chat surface or depth selector.
  Source: https://xmind.com/user-guide/xmind-ai-chatbot
- P2 Confirmed partial: "Grow Ideas" expands a selected topic/subtopic with Auto or
  On-Demand (prompted) modes, matching yMind's existing brainstorm-subtopics action,
  but XMind also documents an optional online-search mode that cites a source link
  for web-grounded suggestions. yMind has no live web-search-augmented generation;
  treat external web search as an explicit, user-visible, opt-in data egress if added.
  Source: https://xmind.com/user-guide/grow-ideas
- P2 Confirmed absent: AI "Reorganize" restructures an existing map/branch in one
  click (Auto or On-Demand with typed guidance), distinct from the deterministic,
  non-AI Map Style actions (Auto Balance Map, Compact Map, Justify Topic Alignment)
  already tracked. Source: https://xmind.com/user-guide/reorganize
- P2 Confirmed absent: AI "Map Refine" adds related content across an entire map and
  reorganizes it for completeness (Auto or On-Demand), with an explicit Apply/View-Now
  review step before changes are kept. This is a superset of Reorganize (content
  addition, not just layout) and of the existing to-do/summarize actions; implement
  as a reviewable, undoable batch edit, never silently applied.
  Source: https://xmind.com/user-guide/map-refine
- P3 Confirmed absent: "Image to Mind Map" generates a structured map from an
  uploaded photo (JPG/PNG, documented 10 MB limit), using OCR for text-bearing
  images and visual-content analysis for images without text, with an output-
  language choice. yMind's AI only accepts text prompts today; this needs a
  vision-capable model and explicit scope/privacy decision before implementation.
  Source: https://xmind.com/user-guide/image-to-mind-map
- P3 Confirmed absent: AI-generated topic images ("Make Image": prompt + selectable
  image-generation model + style presets + "Inspire" prompt regeneration, inserted
  as a topic illustration with automatic background removal for preset styles) and
  a standalone "Remove Image Background" action for any already-embedded image.
  Both need an image-generation-capable backend distinct from yMind's current
  text-only OpenAI-compatible client; scope and cost/privacy implications need an
  explicit product decision before implementation.
  Sources: https://xmind.com/user-guide/make-image ,
  https://xmind.com/user-guide/remove-image-background
- P3 Confirmed absent: "Pitch Video" renders a mind map as an AI-generated video
  (theme, aspect ratio, duration and background-music choices, downloadable and
  shareable) beyond the already-tracked static Pitch-mode slide/PDF export.
  Needs a video-generation backend and explicit scope decision; do not conflate
  with the existing P1 Pitch mode/slide-export backlog item.
  Source: https://xmind.com/user-guide/pitch-video
- P3 Research reference, not adopted: the "Xmind ChatGPT App" is a hosted,
  OAuth-connected ChatGPT plugin that creates/lists/reads/edits Xmind maps from a
  chat conversation. It depends on XMind's own account/cloud service; track only
  as prior art for yMind's own MCP-server backlog item above (XMind SDK/ecosystem
  review), not as a requirement to integrate with a third-party chat product or account.
  Source: https://xmind.com/user-guide/chatgpt-apps
- P3 Platform-specific, future companion-app scope only: "Draw to Map" (iPad + Apple
  Pencil sketch digitized into an editable map, original sketch kept as the central
  topic's image) and "Record to Map" (iOS/iPadOS 26 "Apple Intelligence" voice
  recording with live transcription saved as the central topic's note, then
  converted to a structured map) are undocumented in yMind's desktop-first scope.
  Track as future companion-app research only; no current application change.
  Sources: https://xmind.com/user-guide/draw-to-map ,
  https://xmind.com/user-guide/record-to-map

## Platform & collaboration
- P3 Quick Entry-equivalent background capture (XMind "Quick Entry", documented
  macOS-only): a menu-bar-resident popup, launchable without opening the main
  window, with a single text field and a "Create Map" action that converts the
  note into a new map, plus an optional "start at system login" preference.
  yMind has no background/menu-bar quick-capture surface on any platform; verify
  a cross-platform equivalent (tray icon on Windows/Linux) before committing to
  parity with the macOS-only original. Source: https://xmind.com/user-guide/quick-entry-for-mac-new
- P1 Code signing + notarization; auto-update via `cargo-packager-updater`
- P2 macOS universal binary; Flatpak and `.rpm` packages; winget/Homebrew manifests
- P2 Real cloud APIs (OneDrive/Google Drive/Dropbox) beyond synced folders, conflict resolution UI
- P3 Real-time co-editing (CRDT, e.g. `yrs`) — basis for a companion web/mobile app
- P3 Plugin API (WASM) for importers/exporters

## Quality
- P1 Visual regression tests (kittest snapshot images) per theme
- P2 Accessibility audit with screen readers (NVDA, VoiceOver, Orca)
- P2 Fuzzing of the XMind/FreeMind/OPML importers (`cargo fuzz`)
- P2 Performance budget: 10 000-topic map at 60 fps, benchmark in CI
- P3 More UI languages (FR, ES, JA, ZH)

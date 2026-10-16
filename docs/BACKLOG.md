# yMind backlog

Prioritized ideas beyond the shipped phases. P1 = next, P2 = soon, P3 = later.

## Recently completed
- Fixed the Windows Search crash caused by a dangling accessibility label reference.
- Floating topic toolbar for AI expansion, subtopics, relationships, links, notes, to-dos and style.
- Floating main editing toolbar with a compact insert popover, keeping the title bar uncluttered.
- Direction-aware child/sibling plus connectors; root-safe contextual menus and blank-canvas selection handling.
- Active-sheet Find & Replace with Ctrl+H, accessible search navigation, and refresh on document/sheet changes.

## XMind parity (from the user guide)
- P1 Print (Ctrl+P) with page setup and fit-to-page
- P1 Extend Find & Replace to search across all sheets (active-sheet search/replacement is implemented)
- P1 Pitch mode (presentation from branches, F5) and slide export
- P1 Map Shot (F7): region capture to clipboard/PNG
- P2 Zone (Ctrl+Alt+Z) and callouts editing UI polish
- P2 Gantt view for to-dos with start/end dates, progress and export
- P2 Equations (LaTeX → rendered topic content)
- P2 Attachments and pictures embedded in the `.ymind`/`.xmind` package
- P2 Password-protected files (AES-GCM, Argon2 key derivation)
- P2 Quick styles, copy/paste style, global font, custom theme editor
- P2 Mask data (anonymize topic text before sharing)
- P2 Merge XMind files, skeleton/structure picker gallery
- P3 Audio notes (record/play)
- P3 Customizable shortcuts (Preferences > Shortcuts); detailed requirements below.

## Settings parity (user-supplied XMind screenshots, 2026-10-16)
These are reference requirements, not a claim that the settings are implemented.
The screenshots show selected values, not every available option or factory default.
Reuse existing persisted settings where applicable rather than introducing duplicates.

- P1 Organize Preferences into General, Editor, Calendar, Sharing and Shortcuts tabs,
  with icon navigation, rounded grouped cards, consistent spacing and accessible focus.
- P1 Editor: configurable selected-topic floating toolbar visibility and a separate
  switch for the topic-add connector buttons. The reference shows "Always" for the
  toolbar; verify other modes before defining their behavior.
- P2 General: retain system/light/dark appearance and language selection; add
  locale-aware first weekday with a system-following choice.
- P2 General: wire spelling-as-you-type to actual dictionaries and suggestions;
  distinguish automatic update installation from checking for updates.
- P2 General: optional usage statistics with explicit consent, a documented data
  inventory and no document contents; no telemetry is authorized by these screenshots.
- P2 Editor: initial canvas zoom (distinct from UI scaling), opt-in reuse of the
  last structure/skeleton and color theme for new blank maps, and smart-guide visibility.
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
  merge windows, new sheet, sheet from topic, previous/next sheet, copy/paste/reset
  style and delete a single topic while retaining children.
  The screenshot assigns Ctrl+H to Home, conflicting with yMind's existing Find &
  Replace binding; preserve current behavior until an explicit keymap choice is made.
- P2 Tests: settings persistence/migration, immediate application across open tabs,
  keyboard navigation, EN/DE layout, light/dark contrast, calendar exceptions and
  shortcut conflict/reset behavior.

## Canvas visual and interaction parity (user screenshots, 2026-10-16)
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
  clipping, membership and navigation semantics still need verification.
- P2 Relationships: retain the branch/custom color when selected; use a separate
  selection overlay with circular endpoint anchors, square Bezier control handles
  and thin guide lines. Drag endpoints to reconnect and handles to reshape; support undo,
  hit testing at different zoom levels and keyboard-accessible alternatives.
- P2 Floating topics: match rounded colored cards and dashed colored relationships.
  Match the separate dashed gray rounded boundary around a floating topic shown in
  the reference; do not conflate a document boundary with a transient selection outline.
- P2 Visual fixtures: light/dark reference maps covering deep branches, floating topics,
  selected relationships, boundaries, narrow viewports and mixed-script text.
- P2 Labels: anchored topic popover with a pointer, heading, comma-separated entry
  and an "Auto sort" toggle, matching the supplied reference. Reuse existing topic
  labels and the format-panel editor; specify sort order, duplicate/whitespace handling
  and persistence before implementation. Cover focus, dismiss/commit, undo, keyboard
  access and placement near viewport edges; verify actual tag rendering separately.
- P2 Expand the floating "+" content menu to match the reference grouping: Note,
  To-do and a Link submenu; Attachments and Audio note; Stickers, Illustrations,
  Local image and Equation. Reuse existing note/task/link actions. Verify Link
  submenu destinations rather than guessing; implement missing content types before
  exposing enabled actions. Use licensed/original sticker and illustration assets.
  XMind's "PRO" badges are product-specific and do not establish yMind licensing.
  Cover menu accessibility, dismissal/focus, insertion undo and packaged-file round trips.
- P1 Right-side symbol pane: selection-aware insertion from Symbol, Sticker and
  Illustration tabs, opened from the canvas pane toggle. Provide scrollable,
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

## Platform & collaboration
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

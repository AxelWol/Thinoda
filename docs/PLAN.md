# yMind — phased plan

Goal: a fast, native, cross-platform XMind clone written in Rust, with a reusable UI library for
future companion apps. Sources for the feature set: [XMIND-RESEARCH.md](XMIND-RESEARCH.md)
(website, local Xmind 26 install, role/solution pages) and [XMIND-USER-GUIDE.md](XMIND-USER-GUIDE.md)
(official user guide, used as the verification checklist).

Legend: ✅ done · 🟡 partial · ⬜ open

## Phase 0 — Foundation ✅
- ✅ Cargo workspace (`core`, `io`, `platform`, `ui`, `canvas`, `ai`, `app`, `xtask`), edition 2024
- ✅ egui/eframe 0.37 on wgpu (Direct3D 12 / Metal / Vulkan, GL fallback)
- ✅ `cargo xtask ci` (fmt + clippy `-D warnings` + tests), icon generation
- ✅ Product research + user-guide digest

## Phase 1 — Core model & editing ✅
- ✅ Workbook → sheets → topic tree, floating topics, relationships, boundaries, summaries
- ✅ Transactional edits with one-step undo/redo, rollback of failed edits
- ✅ 15+ structures (map, logic, brace, org, tree, timeline, fishbone, matrix …) with layout engine
- ✅ Markers, labels, notes, hyperlinks, to-dos, numbering, collapse/expand, search
- ✅ Templates and color themes

## Phase 2 — UI library & canvas ✅
- ✅ `ymind-ui`: design tokens, light/dark themes (follows system), widgets, command palette,
  toasts, dialogs, icon set, font loading — no app dependencies, reusable by companion apps
- ✅ `ymind-canvas`: GPU-rendered mind map view, pan/zoom, multi-selection, drag & drop,
  inline editing, keyboard navigation, XMind-guide shortcuts (Tab, Enter, Ctrl+B, Ctrl+K, …)
- ✅ App shell: tabs, toolbar, format panel (Style/Map), outliner, Zen mode, settings, EN/DE i18n
- ✅ Selection-aware topic toolbar, direction-aware child/sibling connectors, root-safe context menus
- ✅ Floating main editing toolbar and `+` insert popover for notes, links, tasks and style
- ✅ Accessible Find (Ctrl+F) and Replace (Ctrl+H) in the active sheet; cross-sheet scope remains in the backlog

## Phase 3 — Files, interop, platform ✅
- ✅ Native `.ymind`, XMind (`.xmind` Zen/2020+ JSON and legacy XML), FreeMind/Freeplane, OPML,
  Markdown, plain text, MindManager import
- ✅ Export: SVG, PNG, JPEG, PDF, Word, Excel, CSV, Markdown, OPML, TextBundle, XMind
- ✅ Crash safety: atomic saves, continuous recovery journal, panic hook, restore dialog
- ✅ Clipboard (topics + text), single instance, personal settings, recent files
- ✅ Cloud folders: OneDrive, iCloud Drive, Dropbox, Google Drive, Box, Nextcloud, ownCloud detection
- ✅ CLI conversion (`ymind --convert in.xmind out.md`)

## Phase 4 — AI assistant ✅
- ✅ `ymind-ai`: OpenAI-compatible client (LM Studio, Ollama, llama.cpp, OpenAI), auto model pick,
  outline parser tolerant to reasoning models (`<think>`), background jobs
- ✅ Brainstorm subtopics (Ctrl+Shift+I), break down into to-dos, explain → note,
  summarize branch, generate a whole map from a prompt
- ✅ Settings › AI with connection test; live-tested against LM Studio (CUDA)

## Phase 5 — Testing & packaging 🟡
- ✅ Unit/integration tests and 27 headless e2e tests (egui_kittest) incl. AI against a mock server,
  live LM Studio integration test (skips when no server is reachable)
- ✅ cargo-packager config: NSIS (Windows x64/ARM64), `.app` + DMG (macOS arm64/x64),
  `.deb` + AppImage (Ubuntu LTS); file associations for `.ymind/.xmind/.mm/.opml`
- ✅ GitHub Actions: CI matrix (Windows x64 + ARM, macOS, Ubuntu) and tag-driven release
- ⬜ Code signing / notarization (Authenticode, Apple Developer ID)
- ⬜ Visual regression snapshots

## Phase 6 — XMind parity extras ⬜
See [BACKLOG.md](BACKLOG.md): Pitch mode, Gantt view, Map Shot, Zone, equations, audio notes,
attachments, password-protected files, print, cross-sheet find & replace, quick styles, custom themes, …

## Remaining-work delivery plan (2026-10-16)

This roadmap expands Phase 6 and the unfinished release work into dependency-ordered
work packages. [BACKLOG.md](BACKLOG.md) remains the detailed requirement inventory;
repeated entries there are delivered once by their owning package below.
Existing phase checkmarks describe foundations, not full XMind parity.
All packages below are planned, not started, unless a baseline is explicitly noted.
No application changes, account connections, publishing or release deployment are
authorized by this planning document.

### Baseline and constraints

- Latest source includes Clarity styling, the Zen exit toast, a responsive template
  grid and a Home-screen explanation for Format Panel availability. The running
  release predates these changes.
- Search crash regression and 27 app e2e tests pass. The final canvas suite previously
  compiled but execution was blocked by Windows Application Control; require a
  permitted CI/native run before claiming the full suite passes.
- Format Panel hide/show worked in native and headless checks. Permanent hiding is
  not reproduced; retain it as a reported reliability issue until its triggering
  state is identified, not a confirmed fixed defect.
- Preserve saved documents, explicit styles, recovery data and current key bindings.
  Migrate schemas/settings compatibly and report unsupported interop explicitly.
- Reusable controls belong in `ymind-ui`; model/editing in `ymind-core`, rendering/
  interaction in `ymind-canvas`, formats in `ymind-io`, OS integrations in
  `ymind-platform`, AI transport in `ymind-ai`, composition in `ymind-app`.
- Reproduce functional interaction patterns with original/licensed assets and fonts.
  Do not copy XMind artwork, template contents, branding or commercial restrictions.

### Delivery order and dependencies

| Package | Outcome | Prerequisites | Delivery band |
|---|---|---|---|
| R0 | Verified baseline, reference decisions and regression harness | None | Foundation |
| R1 | Consistent commands, menus and window/sheet navigation | R0 | Daily editing |
| R2 | Two-family sidebar, preferences shell and bottom controls | R1 | Daily editing |
| R3 | Precise canvas editing, relationships and visual parity | R1; R2 for pane integration | Daily editing |
| R4 | Rich topic content and searchable asset panes | R2; R3 for placement | Rich maps |
| R5 | Categorized preview templates and theme authoring | R2, R3; R4 for asset templates | Rich maps |
| R6 | Cross-sheet tools, sheet exchange, capture and print | R1, R3; R4 for rich rendering | Rich maps |
| R7 | Zones, Pitch and presentation exports | R2, R3, R6 | Presentations |
| R8 | Work calendars, scheduled tasks and separate Gantt view | R2, R6 | Planning |
| R9 | Secure AI credentials, streaming and advanced assistance | R0; R7 for Pitch assistance | AI |
| R10 | Protected files, cloud APIs and sharing | R4, R6; credential foundation from R9 | Connected files |
| R11 | Signed native distribution and safe updates | R0; final acceptance of included packages | Release |
| R12 | Collaboration, plugins and additional languages | R1, R4, R10; format contracts stabilized | Companion foundation |

Quality work is continuous, not a final cleanup phase. R11 signing/toolchain preparation
and R9 credential work can begin after R0; R7, R8 and R9 can progress independently once
their prerequisites are satisfied. Do not postpone a P1 fix merely to finish every
feature in an earlier band. Deliver incremental releases rather than waiting for R12.

### R0 - Baseline, reference verification and quality scaffolding

1. Run full permitted CI, record native platform/architecture results and distinguish
   compile checks from execution. Reproduce the Format Panel report across Home,
   Map/Outline, Style/Map tabs, restored settings, window sizes and display scaling.
2. Inventory existing commands/model support against each backlog entry before adding
   code. Verify unknown XMind submenus and gestures in the local app/user guide.
3. Establish deterministic light/dark screenshot fixtures with controlled fonts,
   viewport sizes and scaling. Include deep trees, floating topics, boundaries,
   relationships and mixed-script text.
4. Add importer fuzz targets (XMind, FreeMind, OPML), malformed/archive-limit fixtures,
   recovery/fault-injection checks and repeatable performance benchmarks.
5. Define benchmark hardware, backend, viewport and interactions for the 10,000-topic,
   60-fps goal; record p95 frame time (target <=16.7 ms) and layout/edit latency.
   Set an explicit layout-latency budget after measuring the baseline.

**Exit:** baseline failures have reproducible cases or explicit blockers; visual
fixtures and benchmark commands run in CI; a short decision record exists for each
unknown needed by the next package. Never bypass OS application-control policy.

### R1 - Commands, menus, shortcuts and sheet/window lifecycle

1. Audit a single source of command identity, availability, execution and shortcut
   hints across menus, toolbar, context menu and command palette; extract shared
   metadata only where needed to remove inconsistency.
2. Build the hamburger quick actions and File/Edit/Insert/Tools/View/Help hierarchy
   from the backlog inventory. Wire supported commands now; add new feature entries
   with their owning package, not success-shaped placeholders.
3. Complete missing tree editing/select/arrange/align commands, style clipboard/reset,
   sheet-from-topic and central-topic navigation. Preserve root/last-sheet invariants.
4. Complete bottom-left sheet Rename/Copy/Paste/Duplicate/Delete/New/Save separately,
   previous/next sheet navigation, and document/tab/window close distinctions.
   Audit multi-window merging before implementing it; preserve unsaved-work prompts.
5. Deliver searchable/category-filtered shortcut editing, unassigned bindings,
   conflicts, reserved keys and reset-all. Preserve current bindings by default;
   offer an XMind-compatible preset only after approval of conflicting mappings.
6. Add Help/User guide/Quick start/About and explicit feedback/news links. Research
   File cache without deleting recovery data. Account/gift-card commerce is excluded
   unless separately requested; updater actions arrive with R11.

**Exit:** command actions and enabled states agree across surfaces; keyboard submenu
navigation and Escape restore focus; all document-changing actions have correct
undo/redo or explicit save semantics. Native keyboard-layout coverage includes
Windows DE/EN and platform-appropriate macOS/Linux modifiers.

### R2 - Sidebar, preferences and compact navigation

1. Implement the top-right smiley and panel selector for shared sidebar families:
   Symbol/Sticker/Illustration and Style/Pitch/Map. Verify collapse/remembered-tab
   behavior first; preserve topic selection and keep a reliable reopen affordance.
2. Refine Style/Map panels: preset preview, shape/fill/border/width-fit, font/weight/
   emphasis/alignment, structure, branch geometry/end/thickness/color, numbering
   and explicitly scoped apply-to-related-topics. Reuse existing edit transactions.
3. Introduce General/Editor/Calendar/Sharing/Shortcuts preferences navigation.
   Ship working General/Editor controls: appearance/language, initial canvas zoom,
   opt-in recent structure/theme, smart guides and independent topic-toolbar/
   add-button visibility. Never present unimplemented switches as functional.
4. Implement spelling dictionaries/suggestions and CJK/mixed-script font fallback,
   with remeasurement across open tabs and actual installed-font checks.
5. Add the floating bottom-right bar: statistics popover, zoom presets/current-value/
   Fit, Map/Outline switch and verified view actions. Wire Gantt/Pitch when ready.
   Define statistics scope and Unicode counting before exposing character totals.
6. Polish Outline hierarchy, disclosures/bullets, separated floating trees and
   view-specific controls. Keep edits, selection and folding behavior intentional.

**Exit:** panels reopen from every supported view; settings survive migration/restart;
all controls remain accessible at narrow sizes/high DPI and in EN/DE/light/dark.
Calendar, Sharing and updater behavior finish in R8/R10/R11.

### R3 - Canvas interaction and visual completion

1. Refine reparenting with subtree ghost/dimmed origin, target connector and insertion
   slot previews; sibling ordering, main-branch sides, cancellation and edge auto-pan.
   Verify detach gestures; reject self/descendant/root-invalid drops atomically.
2. Add relationship endpoint reconnect and Bezier handles/guides without replacing
   relationship colors with selection color. Preserve links across tree moves.
3. Polish floating topics, independent boundaries and callouts; distinguish each
   from transient selection outlines and later presentation zones.
4. Add the anchored Labels popover and shared label editor, defining trimming,
   deduplication, autosort and commit/cancel semantics.
5. Match reference typography, spacing, rounded sub-branch junctions and theme
   hierarchy, extending Clarity without rewriting saved styles.

**Exit:** edit operations round-trip and undo in one step; hit testing works through
zoom/pan; equivalent keyboard actions exist; visual fixtures cover viewport edges,
long text and explicit style overrides.

### R4 - Rich content and asset panels

1. Design versioned embedded-asset references/storage and bounded decoding once for
   images, attachments, stickers, illustrations and audio. Define supported formats,
   size limits, missing/corrupt-asset behavior and import/export fidelity.
2. Deliver semantic Symbol groups, legend and custom groups, using current markers;
   define per-group replacement/multiplicity and synchronize task-progress markers.
3. Deliver categorized Sticker grids and searchable Illustration collections with
   counts/previews/detail navigation, original/licensed assets and attribution.
   Specify selection scope, placement, resizing and removal before coding.
4. Add local images/attachments, equation rendering and audio recording/playback
   with explicit OS microphone permission handling. Integrate all into the grouped
   "+" menu and Insert menu only when each content type works.
5. Implement verified Link destinations; reuse Note/To-do editing rather than
   parallel content systems.

**Exit:** insert/replace/remove/undo works; assets survive recovery and `.ymind` reopen;
supported `.xmind` exchange and exports preserve content or report a precise loss.
Malformed, oversized and missing assets produce visible errors, not silent omission.

### R5 - Templates, skeletons and theme tools

1. Replace icons with generated previews from the actual renderer. Add chooser
   header/category filter, recent templates, sections, selection and Cancel/Create.
   Preserve the existing responsive grid; separate template history from file history.
2. Cover all eight categories in the backlog with original useful examples, starting
   with Basic and existing SWOT/meeting/project templates. Verify truncated reference
   titles rather than claiming exact equivalents.
3. Add skeleton/structure gallery, quick styles, global font and custom theme editor;
   maintain per-topic override precedence and opt-in reuse of recent choices.
   Use the Map-tab anchored preview selector for changing existing sheets as well as
   choosing new-map defaults. Group Mind map, Logic, Brace, Org, Tree, Timeline,
   Fishbone, Tree table, Matrix and the newly observed Grid family; audit supported
   structures and implement missing Grid semantics only after reference verification.
   Separate structural layout from visual skeleton presets, generate thumbnails,
   and apply each choice in one undoable transaction preserving content/relationships.
   Include palette/add-theme, color variants, background, global font and branch-width
   controls; keep popup columns responsive and current selection visible.

**Exit:** every template validates and opens, previews match created maps, Cancel
creates nothing, recent/filter state persists correctly and licensed assets are tracked.

### R6 - Search, file tools, export, capture and print

1. Extend Find & Replace across sheets with sheet/topic identity, navigation,
   replacement counts and well-defined transactional undo; preserve active-sheet mode.
2. Complete standalone sheet save/clipboard fidelity and merge-XMind-files with ID/
   asset remapping, explicit conflicts and relationship preservation.
3. Deliver Map Shot region selection, clipboard/PNG output and cancel; align export
   rendering with rich canvas content and light/dark backgrounds.
4. Deliver native Print/page setup, preview/fit-to-page and page handling on each OS.
   Match the supplied two-column dialog: live preview/page count, scrollable grouped
   printer/copies/content/paper/orientation/scaling controls, background/monochrome
   switches and fixed Cancel/Print footer. Query real printer capabilities, validate
   copy counts and surface printer/spool failures. Share rendering rather than
   screen-scraping the app; verify preview/output parity with file-backed test output,
   not unapproved physical print jobs. Do not copy XMind's attribution watermark.
5. Add mask/anonymize-before-sharing as a derived output/preview, never destructive
   edits to the source. Define handling of notes, labels, filenames and metadata.

**Exit:** round-trip/merge tests have no dangling IDs/assets; capture/export/print
match fixtures; canceled dialogs do not modify the document; sensitive metadata
handling is explicit and documented.

### R7 - Presentation zones and Pitch

1. Verify zone membership/clipping/order/framing semantics and Pitch/Map panel
   controls. Implement named zone create/rename/move/resize/delete with undo.
2. Add zone ordering/framing and branch-based Pitch, F5 entry, navigation and an
   accessible exit hint/control. Keep zones distinct from boundaries.
3. Add slide export using the supported presentation renderer; define the output
   format explicitly before implementing rather than implying PPTX support.

**Exit:** zone/Pitch state survives save/recovery, cross-zone relationships render
correctly, navigation/exit works without a mouse and exported slides match framing.

### R8 - Calendars, task scheduling and Gantt

1. Resolve To-do versus Task semantics, dates/duration/progress, dependency rules,
   locale/week-start and working-day arithmetic before schema changes.
2. Add base/work calendars, working-weekday toggles and dated exceptions with
   persisted settings and calendar-management actions verified from references.
3. Add the separate Gantt window, synchronized task table/timeline, scheduling edits,
   zoom and empty-state guidance; add supported dependency operations and export/print.

**Exit:** date and non-working-day arithmetic is deterministic (including leap days
and locale boundaries); edits synchronize with Map/Outline and undo; closing Gantt
does not close maps; scheduling state survives file round trips and recovery.

### R9 - AI safety, responsiveness and advanced assistance

1. Move API keys to Windows Credential Manager/macOS Keychain/Linux Secret Service;
   migrate existing plaintext values only after successful storage, then remove them.
   Surface unavailable/locked keychain errors; never fall back silently to plaintext.
2. Add SSE streaming, preview and cancellation/timeouts with bounded parsing and
   no partial document mutation until a deliberate apply operation.
3. Add rewrite/translation, editable prompt library and semantic search/clustering
   with explicit model selection and embedding invalidation on document edits.
4. Add AI Pitch outlines once R7 exists. Keep external document transmission opt-in
   and document which content is sent to the selected endpoint.

**Exit:** mock SSE/keychain/error tests cover interruption and malformed responses;
cancel preserves work; optional live LM Studio checks use the user-authorized local
endpoint and are reported separately from deterministic CI.

### R10 - Protected files, cloud synchronization and sharing

1. Design a versioned password-protected envelope using reviewed AEAD/KDF primitives
   (AES-GCM/Argon2 proposal), password change/remove and recovery/backup policy.
   Review threat model and interoperability before committing a file format.
2. Add real OneDrive/Google Drive/Dropbox adapters incrementally with OAuth through
   official APIs, secure token storage, offline states, retry/version checks and
   conflict resolution that never overwrites local work silently.
3. Add configurable sharing destinations from the reference where service support
   is viable; start with explicit local export/email handoff. Require user action
   for account connection, upload and publication.
4. Define optional usage-statistics inventory/consent and implement only after an
   explicit product/privacy decision. Default off; exclude document content/secrets.

**Exit:** protected-file tamper/wrong-password cases fail safely; no credentials enter
files/logs; cloud offline/concurrent-edit/token-revocation tests preserve both versions;
sharing shows destination/content before transmission. Dedicated security review and
native QA are required for this package.

### R11 - Distribution, signing and updates

1. Prepare signing credentials outside source control, Apple notarization and
   Windows Authenticode; confirm supported signing services/architectures.
2. Validate Windows x64/ARM64, macOS arm64/x64 and universal, Ubuntu LTS `.deb`/
   AppImage; add Flatpak/`.rpm` and winget/Homebrew manifests. Test native installs,
   file associations, clipboard, GPU backends, theme following and uninstall behavior.
3. Add a signed update feed/updater after validating `cargo-packager-updater`
   suitability. Separate check/download/automatic-install preferences; prompt before
   restart and preserve unsaved maps/recovery across update failures.
4. Run upgrade/downgrade compatibility checks, publish release notes and a truthful
   platform matrix. Cross-compilation alone is not native validation.

**Exit:** signed/notarized packages install and launch on each advertised target;
update verification rejects tampering, failed updates recover, and user files/settings
are retained. Requires accounts/certificates and permitted native hardware/runners.

### R12 - Companion-ready extensibility and localization

1. Prototype CRDT co-editing against transactional edits/undo, then define transport,
   identity/permissions, offline merge and schema compatibility. Gate production
   collaboration on R10's storage/conflict contracts rather than embedding CRDTs early.
2. Define a versioned sandboxed WASM importer/exporter API with resource limits,
   explicit capabilities, error reporting and compatibility tests.
3. Add FR/ES/JA/ZH translations with font/IME/shortcut and text-expansion checks,
   reusing the finalized UI strings and mixed-script support.

**Exit:** deterministic multi-client convergence and offline-rejoin tests, isolated
plugin fault/limit tests, and native localization checks. A web/mobile companion
application itself is separate scope; these interfaces enable it.

### Cross-cutting completion and release gates

- Each work package: update its backlog status, acceptance cases and user guide;
  keep incomplete features visibly incomplete. Use small reviewable changes.
- Automated: targeted unit/integration/kittest checks while iterating, then
  `cargo xtask ci` for the release candidate (fmt, clippy and workspace tests).
  Run visual snapshots, recovery/format fixtures, fuzz smoke and benchmarks relevant
  to the feature; no fixed total test count substitutes for coverage.
- Native: verify Windows first, then macOS and Ubuntu before cross-platform claims.
  Use temporary maps, do not overwrite user files, and coordinate app interaction
  with the user. Rebuilding the running executable requires normal close/save handling.
- Accessibility: NVDA/VoiceOver/Orca audits for menus, panels, canvas alternatives,
  dialogs and alternate windows; address regressions before releasing their package.
- Dedicated QA: required for recovery/interop migrations, native printing/multi-window
  lifecycle, protected files, cloud conflicts, collaboration and installers/updates.
  Focused self-checks suffice for isolated visual controls with regression coverage.
- Suggested release cuts: daily-editing polish (R1-R3); rich maps/output (R4-R6);
  presentation/planning (R7-R8); AI/connected files (R9-R10); companion foundation
  (R12). Each public cut passes the applicable R11 distribution gate.

### Decisions and external prerequisites

Research before implementation: unknown submenu actions; four-corner icon; toolbar/
sidebar collapse rules; drag targets; label/statistics scope; marker multiplicity;
asset placement; task/calendar arithmetic; zone/Pitch semantics and export format.
Escalate to the user only when research leaves a material product choice.

User/product decisions required before changing scope: conflicting shortcut preset,
telemetry consent/data policy, external service connections and any monetization.
External prerequisites: licensed content/fonts, service/API availability, signing
accounts/certificates and native Windows ARM64/macOS/Linux test capacity.
Unavailable integrations remain tracked with their blocker, not silently dropped.

### Immediate next work

First slice implemented (Windows x64 source verification):
- R0 baseline: full `cargo xtask ci` passes, including canvas execution. Native
  macOS/Linux/ARM64, screenshot fixtures and performance measurements remain.
- Map-panel regression reproduced: theme cards extended beyond the sidebar.
  A width-aware grid fixes the overflow; fixed Style/Map tabs and independent
  scroll states prevent scrolling away the navigation or inheriting another tab's offset.
- R2 foundation: one shared sidebar with Format and Symbols selectors. Existing
  repeated-click hide/reopen behavior is retained, the Style/Map tab is remembered
  within the session, and selection survives switching. Symbols use existing marker
  groups and undo transactions; no-selection actions are disabled.
- R1 integration for this slice: Symbols is wired through toolbar, View and command
  palette; topic-style/property commands restore the formatting family.
- Coverage: 32 main app e2e cases plus an isolated German-sidebar e2e case, including
  theme click/undo, horizontal bounds, narrow/high-scale scrolling, repeated
  hide/reopen and multi-topic marker replacement/removal/undo.
- Rebuilt the Windows x64 release executable and NSIS installer. This slice has
  headless UI verification; native verification of the new binary remains.
- Follow-up property inspector fix: resolved topic colors outside the preset palette
  now have selected swatches and hexadecimal readouts, absent fill/border values are
  explicit, and Map shows the effective sheet background and labeled switches.
  Selection/pointer/undo regressions pass (34 main e2e cases, one German-sidebar
  case, 14 UI unit tests). Formatting and workspace clippy pass; the follow-up full
  CI run was blocked by Windows Application Control on the app's empty binary test
  executable. Targeted affected suites and the Windows x64 package build pass.
- XMind export compatibility: reproduced rejection in native XMind, whose error
  was `Cannot create property 'name' on string 'yMind'`. Corrected metadata to
  `creator: {name, version}` with `dataStructureVersion: "2"`, following the
  official xmind-generator archive format. A newly exported six-topic Unicode
  sample opens in installed Windows XMind with its hierarchy/text visible.
  Regression checks validate metadata/manifest and parse public content without
  yMind's private lossless payload. This verifies opening, not full visual fidelity.

Continue R1 command-availability consistency and R2 preferences/bottom controls,
alongside reference verification. Sticker/Illustration/Pitch panes and persistent
family/tab preferences are not implemented; assets/legend/custom groups/task-marker
synchronization stay with R4/R7. No entire R0/R1/R2 package is complete.

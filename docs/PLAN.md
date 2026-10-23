_Last updated: 2026-10-23 06:58:00 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Thinoda — phased plan

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
- ✅ `thinoda-ui`: design tokens, light/dark themes (follows system), widgets, command palette,
  toasts, dialogs, icon set, font loading — no app dependencies, reusable by companion apps
- ✅ `thinoda-canvas`: GPU-rendered mind map view, pan/zoom, multi-selection, drag & drop,
  inline editing, keyboard navigation, XMind-guide shortcuts (Tab, Enter, Ctrl+B, Ctrl+K, …)
- ✅ App shell: tabs, toolbar, format panel (Style/Map), outliner, Zen mode, settings, EN/DE i18n
- ✅ Selection-aware topic toolbar, direction-aware child/sibling connectors, root-safe context menus
- ✅ Floating main editing toolbar and `+` insert popover for notes, links, tasks and style
- ✅ Accessible Find (Ctrl+F) in the active sheet or all sheets, optional notes/labels,
  and atomic title replacement (Ctrl+H) across the chosen scope

## Phase 3 — Files, interop, platform ✅
- ✅ Native `.thinoda`, XMind (`.xmind` Zen/2020+ JSON and legacy XML), FreeMind/Freeplane, OPML,
  Markdown, plain text, MindManager import
- ✅ Export: SVG, PNG, JPEG, PDF, Word, Excel, CSV, Markdown, OPML, TextBundle, XMind
- ✅ Crash safety: atomic saves, continuous recovery journal, panic hook, restore dialog
- ✅ Clipboard (topics + text), single instance, personal settings, recent files
- ✅ Cloud folders: OneDrive, iCloud Drive, Dropbox, Google Drive, Box, Nextcloud, ownCloud detection
- ✅ CLI conversion (`thinoda --convert in.xmind out.md`)

## Phase 4 — AI assistant ✅
- ✅ `thinoda-ai`: OpenAI-compatible client (LM Studio, Ollama, llama.cpp, OpenAI), auto model pick,
  outline parser tolerant to reasoning models (`<think>`), background jobs
- ✅ Brainstorm subtopics (Ctrl+Shift+I), break down into to-dos, explain → note,
  summarize branch, generate a whole map from a prompt
- ✅ Settings › AI with connection test; live-tested against LM Studio (CUDA)

## Phase 5 — Testing & packaging 🟡
- ✅ Unit/integration tests and 44 headless app e2e tests plus an isolated German UI test
  (egui_kittest), incl. AI against a mock server,
  live LM Studio integration test (skips when no server is reachable)
- ✅ cargo-packager config: NSIS (Windows x64/ARM64), `.app` + DMG (macOS arm64/x64),
  `.deb` + AppImage (Ubuntu LTS); file associations for `.thinoda/.xmind/.mm/.opml`
- ✅ GitHub Actions: CI matrix (Windows x64 + ARM, macOS, Ubuntu) and tag-driven release
- ⬜ Code signing / notarization (Authenticode, Apple Developer ID)
- ⬜ Visual regression snapshots

## Phase 6 — XMind parity extras ⬜
See [BACKLOG.md](BACKLOG.md): Pitch mode, Gantt view, Map Shot, Zone, equations, audio notes,
attachments, password-protected files, print, custom themes, …

## Remaining-work delivery plan (2026-10-16)

This roadmap expands Phase 6 and the unfinished release work into dependency-ordered
work packages. [BACKLOG.md](BACKLOG.md) remains the detailed requirement inventory;
repeated entries there are delivered once by their owning package below.
Existing phase checkmarks describe foundations, not full XMind parity.
Package descriptions below are acceptance targets, not completion claims; the
"Immediate next work" section records delivered slices and current gaps.
No application changes, account connections, publishing or release deployment are
authorized by this planning document.

### Planning deliverable verification

This roadmap task is complete when repository documentation, unfinished backlog
scope, and the current GitHub issue/PR inventory are reconciled into a prioritized,
dependency-aware plan. It does **not** require implementing roadmap packages,
merging branches, or running application CI. Those are execution and release
gates in R0 and the applicable feature packages below; do not treat their absence
as a defect in the planning deliverable.

### Baseline and constraints

- Issues #47 (topic size after drag), #48 (hover cursor), and #50 (long text in
  graphical exports) are closed and their fixes are merged to `main`, including
  PR #54 for #47. On 2026-10-20 this branch incorporated `origin/main` at
  `07fec3c` in merge `f3cbdc8`; the previously missing baseline is now present.
- Current GitHub work-item inventory (refreshed 2026-10-20 after the merge):
  there are **no open issues or pull requests**. The former WIP work is merged:
  - [#56](https://github.com/AxelWol/Thinoda/pull/56), Linux CI recovery
    persistence assertion: merged at `07fec3c`. Retain its regression under
    **R0 baseline/CI reliability**.
  - [#57](https://github.com/AxelWol/Thinoda/pull/57), Windows print-preview
    tests timing out on slower rendering: merged at `68c988c`. Retain its
    test-only wait adjustment under **R0 baseline/CI reliability**, with R6
    print behavior as the related feature area.
  Open issue and PR listings both returned empty. Remaining work is tracked in
  the repository backlog and package acceptance criteria below.
- The accepted delivery scope is the complete R0-R12 roadmap across Windows
  x64/ARM64, macOS x64/ARM64, Linux Ubuntu LTS packages, and applicable
  `wasm32-unknown-unknown` browser workflows. Every feature must be tested in
  light and dark appearance (including Auto/system behavior) and all 13 locales:
  EN, DE, ES, FR, DA, NL, PT-PT, PT-BR, IT, PL, JA, Simplified Chinese (`zh-CN`),
  and Traditional Chinese (`zh-TW`).
- New CJK locales require complete contextual catalogs, IME and font-fallback
  checks, layout remeasurement, shortcut and installer/package coverage. Do not
  claim a locale or platform as supported from catalog presence or compilation
  alone.
- The startup splash implementation is present in `thinoda-app/src/startup.rs`.
  R0 owns its acceptance verification (launch-file progress/errors, recovery,
  close behavior and platform-specific startup); it is not a new feature to
  implement unless that verification finds a concrete defect.
- Latest source includes Clarity styling, the Zen exit toast, a responsive template
  grid and a Home-screen explanation for Format Panel availability. The running
  release predates these changes.
- The current Windows x64 `cargo xtask ci` run passes formatting, all-target
  clippy, workspace tests and benchmark correctness tests with live AI disabled.
  This supersedes earlier local execution blockers, not the separate native
  macOS/Linux/ARM64 or browser acceptance gates.
- Format Panel hide/show worked in native and headless checks. Permanent hiding is
  not reproduced; retain it as a reported reliability issue until its triggering
  state is identified, not a confirmed fixed defect.
  R0 did reproduce a distinct narrow-window Outline toolbar overlap intercepting
  the Map tab; wrapped action rows fix that case. See the current execution slice
  below for the coverage and remaining layout limitations.
- Preserve saved documents, explicit styles, recovery data and current key bindings.
  Migrate schemas/settings compatibly and report unsupported interop explicitly.
- Reusable controls belong in `thinoda-ui`; model/editing in `thinoda-core`, rendering/
  interaction in `thinoda-canvas`, formats in `thinoda-io`, OS integrations in
  `thinoda-platform`, AI transport in `thinoda-ai`, composition in `thinoda-app`.
- Reproduce functional interaction patterns with original/licensed assets and fonts.
  Do not copy XMind artwork, template contents, branding or commercial restrictions.

### Delivery order and dependencies

#### Active priorities by end-user value (2026-10-22)

This ranks remaining work, not whole packages as unfinished. Reuse the delivered
slices below; close each task with observable behavior before moving on.
Data-loss, security, launch and release regressions override the ranking.

| Rank | Phase / packages | Prioritized remaining tasks | End-user benefit |
|---|---|---|---|
| 1 | Reliable daily editing / R0-R3 | Outline Redo and focused inspector save/recovery corrections delivered below; next verify remaining native picker/save/close, IME and assistive-technology boundaries, compact navigation and keyboard focus. | Prevent surprising edits and lost work in the most frequent interactions. |
| 2 | Discoverable, adaptable commands / R1-R2 | Finish command parity; define versioned shortcut overrides, capture/conflict/reserved-key handling, apply/reset and persistence; validate keyboard-only navigation. Do not silently migrate inactive legacy values or remap the Fit collision. | Users can find commands and adapt shortcuts without breaking editing or browser controls. |
| 3 | Trustworthy file, output and installation workflows / R6, R11 | User-requested web PWA installation/settings/icons/offline shell; verify native pickers, cross-sheet search and sheet exchange; close export/print fidelity and MindManager reverse-roundtrip gaps; verify install/update artifacts and recovery. | Easier access to the app and confidence that maps can be saved and exchanged faithfully. |
| 4 | Richer maps with less effort / R3-R5 | Finish relationship/placement and rich-content editing, searchable assets, template previews and theme authoring; cover undo and roundtrip before expanding catalogs. | Faster creation of readable, useful maps. |
| 5 | Presentation and planning / R7-R8 | Complete zones/Pitch and presentation export first; then calendars, task scheduling and Gantt with deterministic date rules. | Turn existing maps into communication and execution tools. |
| 6 | Optional intelligent and connected work / R9-R10 | Credential/cancellation safety first, then responsive AI; protected files and synchronization only after conflict/recovery contracts. | Assistance and access across devices without compromising local work. |
| 7 | Extensibility and language reach / R12 | Stabilize format/plugin contracts, collaboration convergence and remaining accepted locales. | Broader audiences and integrations without weakening compatibility. |
| 8 | OS-integrated shell / R13, P4.1-P4.7 | Feasibility and shared command model first; then Windows ribbon, macOS global menus/multi-window, Ubuntu integration and WASM fallback; finish native acceptance. | Familiar platform behavior built on reliable editing and lifecycle contracts. |

P4 remains explicitly scheduled, not completed or replaced by popup mitigations.
Its dependencies and per-step review/native acceptance gates below still apply.
Performance, localization, accessibility, signing and regression checks accompany
each affected slice rather than waiting for their own final phase.

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
| R13 / P4 | OS-integrated menus, bounded menus and native window lifecycle | R1 command contracts, R2 responsive shell, R6 file/window safety; R12 locale expansion for full-language acceptance | R12 and above |

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
6. Reconcile the closed UX issues #47/#48/#50 and current CI maintenance PRs #56/#57
   against the exact tested revision. Keep per-platform, WASM, locale, and
   light/dark/Auto results separate; a running CI job is not a pass.
7. For every R1-R12 package, record feature applicability for native targets
   versus WASM. Verify browser equivalents where available (local virtual
   storage, browser downloads/printing, and web-safe interactions); explicitly
   document and hide native-only controls rather than implying browser parity.

**Open-work linkage:** the repository has no open issues or PRs at the refreshed
2026-10-20 inventory check. PRs #56 and #57 are merged and their regressions remain
assigned to R0 for CI reliability; #57 also informs R6 print acceptance.
All other unfinished scope is in the repository's own [backlog](BACKLOG.md) and
is assigned to R1-R12 in the delivery-order table below and the detailed package
sections. Thus no untriaged open issue is omitted from the proposed order.

**Exit:** baseline failures have reproducible cases or explicit blockers; visual
fixtures and benchmark commands run in CI; a short decision record exists for each
unknown needed by the next package. Never bypass OS application-control policy.

### R1 - Commands, menus, shortcuts and sheet/window lifecycle

**User-directed priority, 2026-10-22:** make Escape a reliable Zen exit with
focused Map/Outline topics, including the shared WASM UI where F11 may be owned
by browser fullscreen. Preserve editor/search/popup/modal cancellation precedence,
selection and document state. Increase Search & Replace's effective height so
normal viewports show all controls without scrolling; retain bounded overflow
for short/scaled windows. Both are delivered in the follow-up below rather than
deferred behind shortcut customization or P4.

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
   The searchable/category-filtered read-only reference, unassigned-command
   visibility and map/insertion aliases derived from dispatch are delivered.
   Rebinding/conflict editing and native keyboard-layout audits remain open.
   The default-dispatch conflict audit below is a prerequisite, not an editable
   keymap or OS-reserved-key validator. Stable built-in identities are delivered
   below. Resolve the recorded keymap decisions and define the versioned override
   format/migration before activating saved overrides.
6. Help/User Guide/Quick Start/About and explicit feedback/news links are delivered.
   Research
   File cache without deleting recovery data. Account/gift-card commerce is excluded
   unless separately requested; updater actions arrive with R11.

**Exit:** command actions and enabled states agree across surfaces; keyboard submenu
navigation and Escape restore focus; all document-changing actions have correct
undo/redo or explicit save semantics. Native keyboard-layout coverage includes
Windows DE/EN and platform-appropriate macOS/Linux modifiers.

**Safety audit follow-ups delivered:** title-only draft detection, ordered Save All
conflict/consent continuation (including pending Quit), and the command-palette
Escape/short-window issue are delivered below. Save All preserves document identity
and remaining edits on cancellation/error. Native picker/Save As Copy interaction,
actual OS window-close acceptance, full native keyboard-layout and
assistive-technology coverage remain open.

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
   with remeasurement across open tabs and actual installed-font checks,
   retaining explicit default-CJK-font preferences and native/browser IME acceptance
   as separate requirements. Native automatic fallback is delivered below; browser
   font provisioning and IME acceptance are still open. The optional native
   default-CJK-font preference is also delivered below.
5. Add the floating bottom-right bar: statistics popover, zoom presets/current-value/
   Fit, Map/Outline switch and verified view actions. Wire Gantt/Pitch when ready.
   Define statistics scope and Unicode counting before exposing character totals.
6. Polish Outline hierarchy, disclosures/bullets, separated floating trees and
   view-specific controls. Keep edits, selection and folding behavior intentional.

**Exit:** panels reopen from every supported view; settings survive migration/restart;
all controls remain accessible at narrow sizes/high DPI and in EN/DE/light/dark.
Calendar, Sharing and updater behavior finish in R8/R10/R11.

### R3 - Canvas interaction and visual completion

1. Delivered visible-subtree ghosts/dimmed origins, parent highlighting, cancellation,
   stale-source protection, bounded edge auto-pan and atomic protected drops.
   Existing detach gestures are retained and supported by the official guides.
   Delivered shared insertion-slot/connector previews, atomic sibling ordering and
   persistent manual main-branch sides with Style Auto/Left/Right alternatives.
   Delivered per-sheet Flexible Floating Topics with safe free placement and
   conversion gating. Remaining: public XMind side interchange and
   native reference comparison.
2. Delivered relationship endpoint reconnect, Bezier handles/guides and a dedicated
   Style editor without replacing relationship colors with selection color.
   Relative controls persist with one-step undo; keyboard alternatives exist.
   Public XMind curve controls and native keyboard/accessibility QA remain open.
   Preserve links across tree moves during the remaining hierarchy-drag work.
3. Polish floating topics, independent boundaries and callouts; distinguish each
   from transient selection outlines and later presentation zones.
4. Delivered the shared anchored Labels popover with trimming, case-sensitive
   deduplication, opt-in Unicode sorting, suggestions and explicit Apply/Cancel.
   Exact on-map tag rendering/reference polish remains part of visual completion.
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

**Exit:** insert/replace/remove/undo works; assets survive recovery and `.thinoda` reopen;
supported `.xmind` exchange and exports preserve content or report a precise loss.
Malformed, oversized and missing assets produce visible errors, not silent omission.

### R5 - Templates, skeletons and theme tools

Template double-click creation is delivered alongside selection/Create/Cancel,
including category, overview and recent cards, without replacing unsaved maps.

1. Replace icons with generated previews from the actual renderer. Add chooser
   header/category filter, recent templates, sections, selection and Cancel/Create.
   Preserve the existing responsive grid; separate template history from file history.
2. Cover all eight categories in the backlog with original useful examples, starting
   with Basic and existing SWOT/meeting/project templates. Verify truncated reference
   titles rather than claiming exact equivalents.
3. Skeleton/structure gallery, real global/topic font selection and quick-style
   libraries and embedded custom-theme editor are delivered;
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
   Continue the bounded MindManager interoperability work here: verify residual
   native-resaved round trips where a legitimate reference is available, retain
   explicit conversion warnings, and document unverified/lossy fields. R0 owns
   the baseline/reference inventory; do not claim full MindManager compatibility.
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

1. [x] Move API keys to Windows Credential Manager/macOS Keychain/Linux Secret Service;
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
5. User-directed web PWA slice (2026-10-22): installable scoped manifest, web-only
   Settings install action with browser/Apple manual alternatives, Windows/Linux/
   macOS/iOS/iPadOS icon assets, offline shell and non-disruptive updates.
   Keep storage/backups and installation capability limitations explicit.
   Verify subpath deployment, install/dismiss/error/standalone states, safe icon
   zones, offline reload and old-worker lifetime. Do not equate installation with
   native file associations, multi-window parity, unlimited storage or OS QA.

**Exit:** signed/notarized packages install and launch on each advertised target;
update verification rejects tampering, failed updates recover, and user files/settings
are retained. Requires accounts/certificates and permitted native hardware/runners.

### R12 - Companion-ready extensibility and localization

1. Prototype CRDT co-editing against transactional edits/undo, then define transport,
   identity/permissions, offline merge and schema compatibility. Gate production
   collaboration on R10's storage/conflict contracts rather than embedding CRDTs early.
2. Define a versioned sandboxed WASM importer/exporter API with resource limits,
   explicit capabilities, error reporting and compatibility tests.
3. ES/FR/DA/NL/PT-PT/PT-BR/IT/PL catalogs and the persistent language dropdown
   are implemented alongside EN/DE/Auto, including offline help, application
   messages, Windows installer controls, macOS bundle declarations and Linux
   desktop descriptions. Shared catalogs ship in all architecture builds; native
   system controls keep OS translations. Add complete JA, Simplified Chinese
   (`zh-CN`) and Traditional Chinese (`zh-TW`) catalogs and native autonyms as
   accepted product scope. Validate font/IME support, layout/text expansion,
   shortcuts, installer controls and platform metadata before advertising support.

**Exit:** deterministic multi-client convergence and offline-rejoin tests, isolated
plugin fault/limit tests, and native localization checks. A web/mobile companion
application itself is separate scope; these interfaces enable it.

### Phase P4 - OS-integrated menus and windows (R12 and above)

User-directed addition, 2026-10-20. **P4** is this new future workstream, not the
already-delivered legacy **Phase 4 - AI assistant** above and not a P1/P2/P3
priority rating. Track it as **R13** in the delivery order. Continue the existing
R0-R12 work first; do not introduce a ribbon or native menu backend in the current
R0 regression slice. Shared command and menu-overflow fixes may land earlier in
R1/R2 and must be reused here. R13 does not depend on completing CRDTs/plugins.

Use one command identity/availability/shortcut/execution model across native
menus, ribbon, hamburger, palette and context menus. A workbook remains a document
with one or more sheets/maps; menus must not create a competing file hierarchy.
The shared hierarchy is File, Edit, Sheet, Insert, AI, Tools, View and Help.
Put Settings in Edit on every platform; retain the standard macOS application
menu Settings entry and shortcut as an alias to the same command. Add OS-required
Application/Window entries and standard actions where appropriate, with accurate
enabled states and no nonfunctional placeholders.
For Windows the ribbon tabs follow these shared categories, including Edit;
"Office-like" describes the collapsible tab/group interaction, not Office's exact
tab taxonomy or File/Options placement. Settings in Edit is a user-directed
choice even where a host normally uses another location. Standard host aliases
may coexist without changing command identity. Context menus expose relevant
subsets rather than repeating every top-level category.
This deliberately differs from Windows Office's File/Options and Ubuntu
desktop-specific Preferences placement; P4.1 audits and documents that settled
choice rather than reopening it. Command parity does not require identical
group layouts, but Windows tab names must remain the eight shared categories.
P4.4 cannot enter multi-window acceptance until the IPC/startup mechanism selected
by P4.1 is implemented, allowing the primary process to create additional native
viewports instead of only forwarding files into its existing window.

| Step | Deliverable and dependency | Acceptance before marking finished |
|---|---|---|
| P4.1 | Audit all main menus, nested menus and every context menu (canvas/topics/relationships, sheets/tabs, Outline, symbols, assets and text controls). Record native integration feasibility for egui/eframe, accessibility and event routing on each target. Define ownership/focus for multiple windows; audit the current single-instance forward-and-exit IPC and single-viewport startup. | Inventory maps actions to existing commands; confirm OS conventions from current vendor documentation and record intentional Settings-placement deviations; record Ubuntu desktop/session assumptions, implementation choices and unavailable native test hosts. Select a multi-window mechanism (including primary-process creation/routing of additional native viewports and recovery ownership) before P4.4. Full planning rubber-duck review before implementation. |
| P4.2 | Shared grouped menu model and bounded popup layout, reusing R1 metadata. Group file, editing, selection, arrangement, sheet/map, AI, tools and help actions into named submenus/separators; move Settings into Edit. | Main and all context menus fit the available work area above/below the anchor, including nested popups; oversized groups scroll with keyboard access to every item. Long translations, narrow/short windows, multi-monitor edges and 100/150/200% scaling do not lose actions. Escape/arrow/Home/End navigation and focus restoration work; availability/undo agree across surfaces. |
| P4.3 | Windows x64/ARM64: original Office-like tabbed ribbon with the shared category names and semantic command groups, an accessible collapse/expand control, persisted collapse state, keyboard access keys and responsive overflow. Reuse command handlers; do not copy Office assets/branding. Depends on P4.2. | Expanded, collapsed and temporary-open states are usable with mouse/keyboard/screen reader; long labels never increase ribbon height beyond available space or cover canvas controls; high contrast and light/dark/Auto follow the platform. File actions and settings remain discoverable. Explicitly reconcile the floating editing toolbar and "+" popover with the ribbon (retain as contextual shortcuts or merge), respecting existing visibility settings and preventing competing/duplicated primary toolbars. |
| P4.4 | macOS Intel/Apple silicon: real global menu bar at the top of the screen, not an in-window imitation; standard application and Window menus, File/Edit/Sheet/Insert/AI/Tools/View/Help and native shortcuts. Support multiple document windows. Depends on P4.1/P4.2 and R6 safety contracts. | Commands target the focused window/document, standard text editing remains native, open/reopen and new-window actions route correctly, and Close Window differs from Quit. Save/Cancel/Discard, Save All, last-window behavior, recovery and concurrent edits cannot lose or overwrite unsaved work. VoiceOver and native multi-window QA required. |
| P4.5 | Ubuntu LTS: integrate with the supported desktop's standard in-window menu/action conventions (GNOME/Wayland first, X11 also tested), not an assumed mandatory global-menu service. Support an optional global-menu exporter only where actually available; keep a working in-window fallback. Depends on P4.2. | DEB/AppImage menus, keyboard mnemonics, desktop activation, focus and host theme/accessibility settings work in the documented desktop/session combinations. Verify Orca and avoid making optional desktop extensions a launch requirement. |
| P4.6 | WASM: retain hamburger as the compact default, with the same grouped hierarchy, bounded/scrollable submenus and command palette. Evaluate an optional wide-screen in-app menu bar; do not imitate OS-global menus or require a ribbon. Depends on P4.2. | Responsive pointer/touch/keyboard behavior, browser zoom, short viewport and focus tests; browser-reserved shortcuts remain usable. Use browser file/download/print equivalents and explicit native-only exclusions. No fake native-window or Quit actions; separate tabs are not claimed as macOS multi-window parity. |
| P4.7 | Cross-platform release/acceptance and documentation, after P4.2-P4.6. | Add/update command-parity, geometry, persistence, localization, theme, window-routing, unsaved-work and recovery tests; run native OS tests plus browser execution, not compilation alone. Update user guide, shortcut help, architecture, web/platform limitations and backlog. Full rubber-duck review and dedicated multi-window QA before release. |

**Language/appearance gate for every step:** all 13 accepted locales
(EN/DE/ES/FR/DA/NL/PT-PT/PT-BR/IT/PL/JA/zh-CN/zh-TW), Auto language,
light/dark/Auto appearance, localized shortcuts and long labels. Use OS-owned
translations and menu appearance for standard native controls; translate
app-owned actions and respect accessibility/high-contrast preferences. Until
the three CJK locales ship, report the ten-locale results separately and keep the
full-language gate open. Native coverage remains Windows x64/ARM64, macOS
x64/ARM64 and supported Ubuntu LTS; WASM requires real browser behavior checks.

**Review gate:** perform a full rubber-duck review of this plan before P4
implementation, and another after **each** finished P4.1-P4.7 step, examining
correctness, command parity, missing actions, accessibility, platform conventions,
localization/appearance, window/data-loss risks and test evidence. Address findings
or record explicit blockers before advancing; do not replace per-step reviews with
one final review. Step checkmarks and native-support claims require execution
evidence. All P4 steps are currently open.

**Planning review, 2026-10-20:** full rubber-duck review completed with no blocking
findings. Incorporated its clarifications: literal shared category names for the
Windows ribbon, intentional Edit/Settings placement with native aliases,
single-instance/viewport architecture as a prerequisite to multiple windows, and
an explicit floating-toolbar reconciliation gate. Implementation-step reviews
remain mandatory; none of P4.1-P4.7 is marked delivered.

### Cross-cutting completion and release gates

- Each work package: update its backlog status, acceptance cases and user guide;
  keep incomplete features visibly incomplete. Use small reviewable changes.
- Automated: targeted unit/integration/kittest checks while iterating, then
  `cargo xtask ci` for the release candidate (fmt, clippy and workspace tests).
  Run visual snapshots, recovery/format fixtures, fuzz smoke and benchmarks relevant
  to the feature; no fixed total test count substitutes for coverage.
- Support matrix: run locale-catalog/completeness tests for all 13 locales and
  focused light/dark/Auto UI checks; run applicable behavior on Windows x64/ARM64,
  macOS x64/ARM64, Linux, and WASM browser builds/smoke tests. State which checks
  executed natively and which only compiled.
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

**User-directed distribution follow-up, 2026-10-23 - Node 24 and ordered builds:**
all repository Node scripts use Node.js 24 from `.node-version`; CI reads the
same file and PWA tests reject a different major. JavaScript Actions now use
vendor-declared Node-24 versions: checkout/setup-python/setup-node/upload-artifact
v7, download-artifact v8, deploy-pages v5, release-please v5 and release publishing
v3. Rust cache v2 already declares Node 24. Pages upload v5 delegates to a
Node-24 artifact uploader; Rust toolchain and the paused WinGet action (including
its cargo-binstall dependency) are shell-only composites. No forced runtime
override, npm dependencies or publishing-permission expansion is introduced.

Release packaging now enforces macOS Intel first, then Windows x64/ARM64 and
Linux x86 in parallel, Linux ARM64 strictly after Linux x86, and Apple Silicon
after Windows and both Linux builds. All six targets must pass before
publication. A single reusable target workflow retains signing credentials,
cleanup, native tests, signature/notarization checks and architecture-specific
downloads/checksums. ARM64 uses the native `ubuntu-24.04-arm` runner and
`aarch64-unknown-linux-gnu`, with DEB/AppImage output and Ubuntu 24.04 FUSE
dependencies; normal CI also tests this native runner.

The supplied Intel log shows accepted notarization followed by CloudKit
`NSURLErrorDomain Code=-1005` during DMG validation, not evidence of rate limiting.
That final validation call had bypassed the existing bounded ticket retry helper;
it now retries only the recognized Apple CloudKit network failure. It does not
resubmit accepted notarization, skip validation or retry invalid signatures.
Regressions cover validation recovery/exhaustion, six distinct artifact sets,
dependency ordering and Node runtime contracts. PRs #66 and #67 are merged after
passing checks. The authorized v0.9.0 website deployment (run `39917339881`)
succeeded and the live site returns HTTP 200. The native release (run
`39917340014`) completed successfully for all six targets in the requested order,
including both macOS signing/notarization/verification stages. The v0.9.0 release
publishes all ten downloads and six SHA256 manifests; downloading the published
assets and checking every digest succeeded. Real Linux ARM64 installation and
other platform-specific runtime acceptance remain open; packaging success is not
proof of an installed application's behavior.

**Delivered priority-1 follow-up, 2026-10-23 - Labels composition safety:**
reproduced candidate-confirmation Enter adding provisional IME text as a label.
Labels now reuses the title editor's composition-key guard, with independent
per-editor state, before menus can dismiss the popup. Candidate Enter/Tab/Escape
and Shift variants neither add labels nor discard drafts, including same-frame
final commits. Focus is retained during composition; dismissal restores ordinary
Tab navigation and Escape cancellation. Both Map/Outline tests cover final CJK
text, empty preedit/commit dismissal, Apply and one-step Undo. Actual native and
browser IME/font/assistive-technology acceptance remains open.

**Delivered priority-1 follow-up, 2026-10-23 - color-swatch keyboard focus:**
shared color swatches now distinguish keyboard focus from the selected-color
border, expose disabled state correctly, and keep each color's identity stable
when a non-preset color appears or disappears. Keyboard selection of a preset
no longer loses focus when the old custom swatch is removed. Both themes, real
Tab traversal, Enter/Space, single activation and disabled controls are covered.
Existing inspector, custom-theme, quick-style and symbol-color workflows remain
covered; actual OS/browser assistive-technology acceptance remains open.

**Delivered priority-1 follow-up, 2026-10-23 - Settings control focus:**
shared segmented tabs and toggle switches now paint a two-logical-pixel,
theme-text-colored keyboard-focus outline. Disabled segmented tabs report their
disabled state to accessibility clients. Both themes, Enter/Space activation,
single state changes and disabled controls have regression coverage, alongside
Settings/sidebar integration checks. This does not replace native screen-reader
or browser keyboard acceptance.

**Delivered priority-1 follow-up, 2026-10-23 - toolbar keyboard focus:**
shared icon and captioned toolbar buttons now paint a two-logical-pixel,
theme-text-colored focus outline, independent of custom accent/selection colors.
Keyboard-only Tab/Enter/Space regressions verify focus, single activation and
disabled semantics in both themes and selection states. Captioned controls also
respect a disabled parent. Map/Outline regressions activate Undo/Redo from the
keyboard after focused Notes Save. This improves shared toolbar affordances;
it does not close the full OS/browser/assistive-technology acceptance matrix.

**Delivered priority-1 follow-up, 2026-10-22 - title composition key safety:**
an event-level regression reproduced Enter committing a provisional topic title
before the IME final text arrived. Map and Outline now reuse a per-title-edit
composition guard, invoked before their text widget and commit/insert handling.
Enter, Tab and Escape (including Shift variants) belong to candidate navigation
while composing and on the final commit/dismissal frame. Outline locks editor
focus so candidate Tab cannot move focus and commit the title. Empty preedit or
commit clears composition; normal editing keys then resume. Tests cover both
views, CJK final text, candidate-key isolation, same-frame final commit, dismissal,
subsequent ordinary Tab insertion and document Undo. This is an input-routing
fix, not proof of platform-specific IME, font rendering or AT acceptance; those
remain in the priority-1 native/browser matrix.

**Delivered priority-1 follow-up, 2026-10-22 - inspector draft safety:**
a new real-key regression reproduced saving a focused Notes field without its
typed text. Its private egui buffer was invisible to saving, dirty detection and
recovery until focus loss. Notes and Hyperlink now keep a document-owned draft,
scoped to the sheet index/root and topic as well as document identity; duplicated
sheets may share topic IDs and must not alias. Dirty detection and idle timing
include the draft. Commands, Save/Save All and native close handling commit it
in one undoable step; autosave's normal save path includes it too. Separate-sheet
output overlays a copy without marking/retargeting the source. Recovery overlays
a copy and tracks draft revisions even when the underlying model revision has
not changed. It does not create undo entries just to take a snapshot.

Regressions exercise both Map/Outline, real Save keys while the field retains
focus, ordinary one-step Undo/Redo, Hyperlink Escape cancellation, insertion/
priority-key isolation, native close-request cancellation and dialog Cancel,
repeated recovery loads, identical-document ownership, duplicated-sheet targets,
failed writes and changed/missing sources. Stale source conflicts surface
explicitly and retain the draft rather than overwriting newer content.
Command-boundary flushes clear committed UI buffers, and unchanged buffers
follow model updates. Direct and toolbar Undo of an uncommitted draft, and
Save after Undo while the field remains focused, cannot resurrect stale text;
toolbar Undo/Redo availability includes pending inspector changes.
The existing inspector focus/locale/scale and Save All continuation coverage is
preserved. Native picker interactions, actual OS window-manager/IME/AT acceptance
remain open; this is a delivered data-loss fix, not completion of R0-R3 or P4.

**Current continuation, 2026-10-22:** follow the value-ranked task list above.
Stable shortcut IDs are locally committed at `9052a01`. The daily-editing
slice at `8566fc7` corrected Outline Ctrl/Cmd+Shift+Z from Undo to Redo while preserving the
existing Ctrl/Cmd+Y primary hint, ordinary Undo and current focus guards.
Tests exercise actual Map/Outline frames with Windows/macOS/Linux modifier
models, both reference aliases, menu hints and sidebar focus. This intentionally
fixes a diagnosed default-key bug; it is not a shortcut preset migration or
activation of saved overrides. Fit's separate collision still needs a keymap
decision.

The user-requested PWA slice is implemented alongside it: web-only Settings
installation, manual browser/Apple alternatives, scoped manifest and generated
192/512 standard, 512 maskable and 152/167/180 Apple touch icons. Trunk generates
a content-versioned app-shell worker including matching WASM/JS/snippets.
Only shell assets are cached; old clients are not force-reloaded, and maps remain
in the existing browser storage with its limits and backup requirements.
Full workspace CI (live AI disabled), WASM lint, Node install/error/cache tests,
safe-zone/icon and build-generation tests pass. An isolated Edge profile
executed the built WASM site under `/dist/`,
validated its manifest and cache scope, clicked the actual Settings install
action and closed the accessible panel with Escape, passed browser installability checks, then successfully
reloaded the app offline. This did not
install software or prove other platforms' installation/icon/AT acceptance.
See [WEB.md](WEB.md#install-thinoda-as-an-app-pwa); R11 platform acceptance and
all P4 native integration gates remain open.

R0 execution resumed on 2026-10-20 after merging main through `07fec3c`.
The startup harness now covers all ten shipped locales plus Auto, explicit
light/dark and both simulated system appearances, at 640 logical pixels and at
1280 pixels with 150% application scaling. It checks first-frame splash presence,
logo visibility and readiness without an artificial minimum delay.
A persisted-sidebar matrix covers both saved visibility states, the same two
viewport/scaling configurations and light/dark/System settings in every shipped
locale plus Auto. Real clicks exercise Style/Map selection, Map/Outline changes,
Home explanations and reopening, while checking saved visibility and unchanged
selection/workbook contents.

That matrix reproduced Outline's unwrapped action rows intercepting the Map-tab
click at narrow widths. Wrapping both rows resolves the interaction regression;
this is not evidence that every earlier Format Panel report has the same cause.
An exploratory stricter geometry check also observed Italian action bounds
extending into the sidebar's accessible tab bounds after Map-panel relayout at
640 pixels, even with wrapping; full localized panel/toolbar geometry remains an
R2 acceptance item, not a claimed fix here. Native close/startup, real DPI/IME,
system-language selection on each OS, GPU screenshots and browser execution
remain separate checks. JA/zh-CN/zh-TW are still planned, not shipped or covered
by these runtime matrices. R0 as a whole is not complete.

Verification for this slice: Windows x64 `cargo xtask ci` passes with live AI
disabled. WASM clippy passes with warnings denied for `thinoda-web`,
`thinoda-app`, `thinoda-platform`, `thinoda-ai` and `thinoda-io`; this is a
compile/lint result, not a browser execution result. The requested rubber-duck
review found no blocking issues.

R0 startup-close follow-up, 2026-10-20: deterministic first-splash close tests
cover light/dark/System settings with a real orphan snapshot and a current
recovery session under a temporary profile. Closing before consuming any launch
result does not cancel close, cleans only the empty current session, and leaves
the earlier snapshot byte-identical/loadable and launch sources unchanged.
A separate three-file import case closes after the first dirty document opens:
the application cancels the native close request, retains the Quit save prompt
through startup completion, and Cancel returns to the documents without changing
or discarding any unsaved maps. These are headless viewport-event tests, not
native window-manager acceptance. Targeted startup/sidebar/shortcut tests and
all-target application clippy pass. A full finished-step rubber-duck review
found no blocking issues; actual OS close/renderer behavior remains open.

R1 compact menu-navigation slice: the root hamburger is reduced to
File/Edit/Sheet/Insert/AI/Tools/View/Help. File retains the existing document
actions and Quit; Settings moves into Edit, with matching command-palette and
shortcut-browser categories. Dismissing a popup with Escape consumes the event
before canvas handling, preserving map selection. A sequential shipped-locale
and Light/Dark/System matrix checks root bounds at 640x360/100% and
1280x720/200%; leaf navigation is checked in a larger viewport, not represented
as short-viewport submenu acceptance. Existing nested template, Arrange and
German print/sidebar tests pass. Application all-target and WASM clippy pass.
Finished-step rubber-duck review found stale Settings categorization; corrected
with a regression assertion. Full R1 command consistency/keyboard traversal and
R2/P4.2 context-menu bounds remain open. The initial scroll-area experiment
clipped nested commands and was removed from the R1 commit; the R2 follow-up below
fixes the underlying popup sizing rather than changing the command hierarchy.

**Delivered R1 follow-up - topic-field command consistency:** Insert now exposes
the existing Notes and Hyperlink commands with their existing Ctrl+Shift+N/Ctrl+K
bindings. Main menu, insertion toolbox, topic context menu and filtered palette
share the single-valid-topic availability rule already used by Labels. Global
shortcuts/direct execution enforce the same rule and report invalid context
explicitly instead of opening a primary-topic editor for a multiple selection.
Opening either field commits a pending inline title edit before switching from
Outline to Map, avoiding reliance on a disappearing editor's lost-focus handler.
No shortcut or valid-selection editing behavior is replaced.
Tests cover Home/no selection/multiple/stale/object selections and modal blockers,
unchanged state plus warnings on rejection, and Map/Outline draft commit/undo.
The shipped-locale/Light/Dark/System/100-200% matrix exercises the real Insert
route with a hidden inspector, focused buffered field edits, undo/redo and
disabled Insert/More states. Planning and finished-step reviews found no
remaining blockers. Full command-metadata/shortcut-rebinding and native keyboard
layout acceptance remain open; this is not completion of R1.

**Delivered R1 follow-up - active-file command availability:** Save/Save As/Export
require an active document in File, the top Export toolbar and the filtered
palette. Direct dispatch, including shortcuts, rejects missing-document context
with the existing localized warning before any file picker or export consent
opens. Save All stays available on Home when background documents exist, and now
restores the exact original tab/Home state instead of activating the last tab.
The native two-file test saves and reopens real dirty documents from Home; the
shipped-locale/theme matrix checks disabled File/toolbar states and real palette
results on empty Home, Home with background documents and an active document.
All advertised export formats reject missing-document execution without dialogs
or map mutation. Planning/finished-step reviews completed; no remaining blockers
in this slice. Save All's error/consent/Quit continuation and pending-title dirty
detection are handled by the subsequent follow-ups below.

**Delivered R0/R1 follow-up - pending-title save/close safety:** Save All flushes
all open documents' inline title drafts before deciding which documents are dirty.
Tab close commits only its target (including a background tab), and a native
window-close request commits all titles before deciding whether to prompt.
Non-close frames and invalid tab indices do not flush drafts; unchanged titles
do not create undo entries or unnecessary prompts. Cancelling the prompt keeps
the title text as an ordinary undoable edit, rather than reopening its inline
editor, consistent with other editor-to-dialog transitions.
All three original failures were reproduced before fixing them. Map/Outline
tests cover real two-file save/reopen from active tabs and Home, one-step title
undo/redo, background-target close with untouched non-target drafts, and all
Save/Cancel/Don't Save choices for tab/native window close. Native close checks
assert actual CancelClose output for title-only changes and allow accepted
Save/Don't Save to close; untouched/unchanged drafts stay clean.
Planning and finished-step reviews found no remaining blockers. Targeted
application suites (133 cases), native all-target clippy and WASM clippy pass.
`OpenDoc::is_dirty` and idle pending-editor autosave/recovery are unchanged;
pending-editor crash snapshots still need their own acceptance work. Interrupted
Save All continuation is handled by the following slice.

**Delivered R0/R1 follow-up - resumable Save All and pending Quit:** the user
explicitly selected automatic continuation after conflict/save-consent resolution.
A queue of stable document keys replaces the loop that could overwrite a pending
conflict. Only one document waits for a decision; later documents are not written.
Conflict and whole-document MindManager consent can transition in either direction
without losing the queue or pending Quit. Each pause, completion and abort restores
the original document or Home by identity, even after reordering.
Cancel, picker cancellation and write failure stop the batch and pending Quit.
Already-saved files remain saved; remaining documents retain their edits, and
unconsumed overwrite consent is cleared. Missing queued documents warn and abort
rather than targeting the tab that inherited their index. Save/Save As/Save All/
Export/Quit/tab-close dispatch cannot replace the batch. Native close during a
pending batch sends CancelClose without stacking another dialog or upgrading a
manual Save All to Quit. Autosave waits while the batch is paused; recovery remains
active. Quit only closes after every queued save succeeds and no dirty document
remains, including documents created outside the original queue.
Real temporary-file save/reopen tests cover active/Home origins, reorder/removal,
conflict/consent cycling, partial success/cancel, actual write failure, overwrite
consent revocation, a full autosave interval, new dirty-document veto and actual
Close/CancelClose output. Existing title, file-command, tab-close and MindManager
regressions pass. Planning and finished-step rubber-duck reviews found no blockers;
workspace CI with live AI disabled and WASM lint pass. Native file-picker/Save As
Copy interaction and actual renderer/window-manager close behavior remain manual
acceptance work; no multi-window or pending-editor crash-recovery parity is claimed.

**Delivered R2 follow-up - compact Outline actions:** at 320x240, a long selected title
placed Add Sibling at y628 outside the page. The selected summary now truncates
with its full text available on hover/accessibility. A vertically scrollable
header caps the summary and existing action rows at 40% of available Outline
height, while adaptive frame margins preserve space for topics. Focus reveal is
scoped to the actual action, disclosure, topic or editor response rather than a
layer-wide focus lookup, preventing sibling header/body scroll interference.
The 66-case shipped-locale/Auto, light/dark and measured viewport/scale matrix
checks all nine action hit rectangles and caption paint/clip bounds, independent
root visibility, alternating header/body focus, unchanged document/history, and
actual Move Down/Undo. Planning and finished-step rubber-duck reviews found no
blockers; full workspace CI with live AI disabled and WASM lint pass.
Full native input/accessibility and exceptionally tiny viewports remain manual
acceptance work, not a claim of complete Outline parity.

**Delivered R1/R2 follow-up - responsive Find and Replace:** at 320x240 the old
fixed-width overlay put the replacement field at x=-27. The search card now
uses the measured central canvas edge when there is room and constrains itself
to the viewport; at narrow widths it may temporarily cover the inspector rather
than putting controls outside the window. Query/replacement fields use available
width, action/options rows wrap and a height-bounded scroll region reveals
keyboard focus. Long sheet titles truncate with the full title on hover.
Search focus survives the sizing/reopen pass without pinning later navigation.
The new keyboard matrix also reproduced a document-safety defect: Tab on a
search navigation button in Outline inserted a child. Non-editing Outline
shortcuts now yield while Find/Replace is open.
The 66-case shipped-locale/Auto, light/dark, viewport/scale matrix exercises both
Map/Outline, inspector visible/hidden, Tab order, focus reveal, actual button
paint/clip containment, resize/draft retention, replacement/one-step undo and
Escape/Close filter cleanup. Existing search reopening and cross-sheet regression
coverage is retained. Planning/finished-step rubber-duck reviews found no
blocking issues; temporary inspector overlap is an intentional compact-mode
trade-off, not hidden-layout parity. Full workspace CI with live AI disabled and
WASM lint pass. This does not close the daily-editing band: shortcut rebinding,
spelling/CJK font work, canvas reference polish and native input/accessibility
acceptance remain before claiming R1-R3 complete.

**Delivered R2 follow-up - inspector keyboard reachability:** at 800x400 the
Hyperlink input accepted accessibility/keyboard focus while its rectangle stayed
at y1112-1138. Style/Map/Symbols scroll regions now reveal newly focused controls.
The shared helper verifies actual widget ancestry with a bounded parent walk,
not just a shared background layer; this prevents Outline/sidebar scroll regions
from moving one another. Existing menu, search and Settings focus reveal uses the
same stricter ownership check. Unchanged focus does not override manual scrolling.
The 66-case shipped-locale/Auto, light/dark and measured viewport/scale matrix
covers Style Hyperlink/Tab/Edit labels/Notes, Map controls, first/last Symbols,
visible hit and paint/clip bounds, same-layer Outline isolation, retained manual
scrolling and unchanged document revision. An actual Notes draft commits on blur
and restores the original workbook with one Undo. Adjacent menu, search, Settings
and compact Outline regressions pass. Full CI exposed a coupled command handoff:
opening Notes could reveal the input but yield focus to the canvas. Pending
Hyperlink/Notes requests now survive disabled/layout passes until actual focus
is observed, then release normally for Tab and blur. The existing topic-field
command matrix passes with diagnostic assertions retained. Planning, finished
and focused follow-up rubber-duck reviews found no blockers; full workspace CI
with live AI disabled and WASM lint pass.
Native keyboard/assistive-technology
acceptance remains open; this does not close the daily-editing band.

**Delivered R2/R3 follow-up - visual alignment guides:** Settings > Editor >
Show alignment guides is an opt-in persisted view preference, shared by native
and browser canvases. During free-placement topic dragging, at most one vertical
and one horizontal guide mark the nearest matching left/center/right or
top/center/bottom coordinate within four logical screen points. The moving
group uses its visible topic bounds (including visible descendants); collapsed
topics and all dragged members are excluded as stationary candidates.
Equal-distance ties follow stable layout order. Guides use the clipped canvas
painter and disappear on drop/cancel; hierarchy-drop previews retain priority.
This is visual feedback only: no snapping, no position correction, no document
style/history changes, and no reassignment of the command-palette shortcut.
The existing persisted `snap_to_grid`/`smooth_zoom` fields are not activated or
advertised by this slice. Keyboard Arrange alignment commands remain available.
Tests cover exact threshold behavior at 50/100/200% canvas zoom, multi-root group
bounds, nearest/tie selection, hidden/self exclusion, actual pointer-drag paint
on/off, unchanged pre-drop documents, unsnapped drop geometry, Escape, one-step
Undo and reparenting with guides enabled. A 66-case shipped-locale/Auto,
light/dark/System and compact/normal Settings matrix checks focus, keyboard
activation, disk persistence, propagation across documents and unchanged revisions.
This does not claim full XMind guideline parity, equal-spacing/distance guides,
snap behavior or manual native visual/accessibility acceptance.
The configured reviewer could not start; the user explicitly approved direct
planning/finished review for this slice. No rubber-duck review is claimed.
Full workspace CI with live AI disabled, final Settings interaction regression
and native all-target/WASM lint pass. R1-R3 remain open: spelling, shortcut
rebinding/conflict handling, remaining canvas/reference polish and native input/
accessibility acceptance are not moved into a completed phase.

**Delivered R2 follow-up - preferred native CJK fallback:** Settings > Editor now
offers an optional installed-font preference; Auto restores automatic fallback.
It follows all configured/default/named faces but precedes native on-demand
discovery, so it supplies missing glyphs rather than replacing document fonts.
The installed list is not a coverage guarantee: choose a font covering the
desired script. The selected font's collection index is preserved. Changes apply
on Home and remeasure open documents when shown, without document/history edits.
Unavailable names warn once and remain stored; older settings default to Auto.
Browser builds omit the control and ignore, but preserve, this native preference.
The localized picker fits compact viewports even though the surrounding Settings
body can scroll horizontally; full selected names remain available on hover.
Font registration now merges deferred writes within a pass, preventing a theme
preview's additional named faces from discarding a simultaneous preference update.
Deterministic tests cover real missing-glyph availability, configured precedence,
switch/reset, idempotency and same-pass registration. The 44-case locale/Auto,
light/dark and viewport/scale matrix covers picker focus/activation, missing-name
warnings, reset and unchanged maps/revisions; Home startup is also covered.
Settings roundtrips and opt-in installed Windows CJK checks cover persisted names,
collection-backed faces, preference changes during active title drafts/Undo and
two-document cache invalidation. Full workspace CI with live AI disabled and WASM
lint pass; the installed Windows acceptance test also passes explicitly.
The planning reviewer could not start (`Requested model not available for session`);
the user explicitly approved direct planning/finished review for this slice.
This is not a rubber-duck review claim. Spelling, IME, browser font provisioning,
macOS/Linux native acceptance and vector-export matching remain open.

**Delivered R2 follow-up - native mixed-script font fallback:** the workspace
disabled eframe's default features without re-enabling `system_fonts`. This left
Japanese/Chinese/Korean characters missing even with suitable installed fonts.
Enable the existing egui 0.37 native provider rather than adding document scans,
script tables or another cache. It enumerates fonts in a background thread,
looks up missing glyphs on demand (the first request can wait for enumeration),
validates coverage and preserves collection indices. Configured/named faces keep
priority; document names, saved state and undo history are not rewritten. All
native text surfaces share this provider, including uncommitted title drafts.
Deterministic embedded-font coverage checks initial measurement, cache reuse,
configured-family precedence and providers surviving font-definition updates.
The opt-in Windows acceptance test uses installed CJK fonts to verify real glyph
coverage in proportional/monospace/named families, measured layout, draft/undo and
document/Outline switching. It passes on the development Windows host; CI does
not assume every runner has optional CJK language fonts. Run it explicitly with
`cargo test -p thinoda-app --test font_families -- --include-ignored --test-threads=1`.
Fonts are neither downloaded nor redistributed. macOS/Linux native CJK acceptance,
IME, browser font provisioning and exact vector-export
matching remain open; this slice does not close R2 or add a locale.
Planning and finished-step rubber-duck reviews found no blockers. The final review
identified the Linux Fontconfig build prerequisite; both CI and release workflows
now install `pkg-config`/`libfontconfig1-dev`, and DEB metadata declares
`libfontconfig1`. Generated runtime dependency notices are updated. Full workspace
CI with live AI disabled, WASM lint, notice checks and packaging-script tests pass
locally. PR #62 run `39624209070` passed Windows x64/ARM64, macOS, Ubuntu and web
CI at `7d45d41`, including the Fontconfig prerequisites. This verifies native
compilation/regressions, not manual Linux/macOS CJK/IME acceptance. Later commits
on the PR must receive their own green checks before merge.

**Delivered R1 follow-up - Outline keyboard ownership:** Tab from the Outline
action toolbar reproduced an unintended child insertion, and global topic keys
could override the focused control. Structural keys and arrow selection now
require a topic-row/disclosure owner captured before rendering; toolbar/sidebar
widgets keep their own keyboard behavior. Stable topic-scoped title/disclosure
IDs survive reordering. Pointer selection explicitly focuses the row, entering
Outline focuses the primary topic, and navigation/action/edit-Escape restores
topic focus without moving focus away from a toolbar-origin Undo/Redo.
Global non-text Undo/Redo remains available; activated buttons cannot be
overridden by the later structural shortcut handler.
The existing 99-case locale/Auto, light/dark/System and scale/panel matrix now
checks toolbar Tab/Delete/F2/arrows without edits, Enter activating only Move
Down, pointer/arrow row focus, topic Tab insertion, edit Escape and sidebar-focus
Undo. Adjacent Outline, search, title, menu and sidebar regressions pass; full
workspace CI with live AI disabled and WASM lint pass.
Planning/finished-step rubber-duck reviews found no blockers. An unrendered
focused topic fails closed rather than dispatching a structural edit; native
accessibility/input acceptance and complete daily-editing parity remain open.

**Current release handoff (2026-10-21):** PR
[#60](https://github.com/AxelWol/Thinoda/pull/60) merged the verified responsive
Settings/Outline and macOS packaging fixes. Release Please PR
[#61](https://github.com/AxelWol/Thinoda/pull/61) passed CI, including its synced
lockfile, and merged as `5afae7d`, creating v0.7.0. Build/publication run
[`39610005813`](https://github.com/AxelWol/Thinoda/actions/runs/39610005813)
completed successfully: all five native package jobs and publication passed.
[v0.7.0](https://github.com/AxelWol/Thinoda/releases/tag/v0.7.0) contains eight
downloads (Windows x64/ARM64 installers, macOS Intel/ARM DMGs and app archives,
Linux DEB/AppImage) and five target-specific checksum manifests. Every published
download was fetched and its SHA-256 matched its manifest. Both macOS targets
passed signing, notarization and installed-copy verification. WinGet was skipped;
no package-manager publication is claimed. The later responsive-search
(`311219a`) and Outline focus-ownership (`e1bcc27`) slices are post-release
work, intentionally excluded from that tag. Main's v0.7.0 metadata is integrated
into ongoing work. Current daily-editing work is still open; spelling,
shortcut editing, canvas reference polish and native acceptance
are not silently deferred into a "completed" phase.

**Prior publishing incident (2026-10-21):** v0.6.0 Release Please run
[`39566899330`](https://github.com/AxelWol/Thinoda/actions/runs/39566899330)
attempt 2 failed Intel macOS DMG creation because `hdiutil` could not eject
`disk2` ("Resource busy"), before asset publication. The original-tag failed-job
retry was restarted; attempt 4 completed with another Intel CloudKit connection
loss during installed-copy ticket validation, after DMG creation and notarization
succeeded. Publication is still blocked and assets remain empty. This preserves
v0.6.0 source identity rather than attaching newer feature-branch binaries; assets are not
claimed published until the release/publish jobs and downloads are verified.

**R11 release reliability follow-up - DMG lifecycle:** attempt 2 job
`119186008681` failed the external writable-image detach, not application code
or our later installed-copy check. The
[pinned packager source](https://github.com/crabnebula-dev/cargo-packager/blob/cargo-packager-v0.11.8/crates/packager/src/package/dmg/mod.rs)
uses [create-dmg at `28867ba`](https://github.com/create-dmg/create-dmg/blob/28867ba3563ddef62f55dcf130677103b4296c42/create-dmg):
read/write attach without `-nobrowse`, only three busy-detach attempts with
2/4-second waits, and no open-handle diagnostics/failure cleanup. CI skips the
Finder customization AppleScript, so an open Finder script/window is not proven
as the cause. No log identifies the holding process. Intel uses macOS 15
(`20260824.0482.1`); the successful ARM job uses macOS 26
(`20261013.0424.1`). OS/image/background-service timing is a plausible difference,
not proof of a CPU-specific defect.

The release script now requests only the app from cargo-packager (its independent
app signing/notarization is confirmed in upstream `package/app/mod.rs`) and
creates a compressed HFS+ DMG directly from closed staging files. Preserve the
signed app, Applications link, icon, license file and embedded license prompt;
resource changes precede image signing/notarization. No explicit writable mount,
Finder automation, global Spotlight changes or broad process kills are used.
The remaining read-only verification mount has image-correlated cleanup,
partial-attach/timeout handling, scoped handle diagnostics, bounded EBUSY retries,
and last-resort force-detach of only the owned read-only device. A post-detach
check must confirm no attachment remains. Existing/potentially foreign mounts
fail closed. Cleanup and verification failures remain visible through exception
chaining.

The latest installed-copy validation failure also requires the existing
CloudKit-only retry policy on ticket validation/Gatekeeper checks, not just
stapling; non-network/signature failures still fail immediately.
Planning and finished-step rubber-duck reviews found no blockers. All 71 script
tests pass locally (9 native-only skips). Added native macOS tiny-image coverage
checks embedded license resources, Applications link, installed app signature
and no leftover image on successful/failed verification. Artifact-only run
[`39598190225`](https://github.com/AxelWol/Thinoda/actions/runs/39598190225)
at `afdf9b7` passed all five native test/package jobs. Intel and Apple silicon
passed native image lifecycle tests, app/DMG signing, notarization, stapling,
download/installed-copy verification and artifact upload. Publish/WinGet were
intentionally skipped. This does not repair the immutable v0.6.0 tag tooling or
claim published assets. Rebuilding that exact source with corrected tooling
requires an explicitly separate tooling/source checkout, not retagging or
attaching feature-branch application binaries.

**Delivered R2 follow-up - deep Outline reachability:** a 30-level branch at
640x700 remained outside the viewport even after an explicit scroll request.
The topic list now scrolls in both axes, with offsets scoped to document key,
active sheet index and root. Fixed frame spacing and existing editing actions
remain unchanged. Title editors use a finite viewport-derived width rather than
infinity, so they can be revealed within the horizontal region. New primary
selection and newly focused controls reveal themselves; stable selection does
not override manual scrolling. First observation preserves the stored offset.
The 66-case sequential shipped-locale/Auto, light/dark and measured viewport/
scale matrix covers 50-level branches, full title paint/clip containment, pointer
selection, arrow-key reveal, editor focus/typing/commit/undo and independent
new-document/restored-document scroll positions. Existing Outline toolbar,
title-lifecycle and e2e regressions pass (70 cases across four suites), as do
native all-target and WASM lint. Planning/finished rubber-duck reviews found no
blockers. This does not claim sheet-reorder scroll identity, very short
action-toolbar layout, full Outline visual parity or native input/accessibility
acceptance.

**R11 release reliability follow-up - Apple ticket lookup:** investigation of
Release Please run `39566899330` (v0.6.0) and artifact-only Release run
`39562303098` found Intel macOS stapling failures after Apple accepted
notarization: CloudKit dropped the connection (`NSURLErrorDomain -1005`, stapler
68). Windows x64/ARM64, Linux and Apple silicon packages succeeded in the release
run, but publication was correctly blocked; the release currently has no assets.
The earlier release-PR CI lockfile failure (`39563802807`) was superseded by green
follow-up and main CI runs, not an application regression.
Packaging and the previously unguarded DMG staple now share opt-in, bounded
CloudKit-only retries (15/30/60/120 seconds, five attempts). The helper merges
stdout/stderr and keeps failure logs; no verification runner or credential behavior
changes. DMG submission is not repeated, final validation remains mandatory and
rejected notarization, signature errors and exhausted retries still fail.
Deterministic recovery/exhaustion/nonretry tests include actual subprocess stderr
capture. Live fixed-branch packaging verification is tracked separately; this
does not yet claim repaired v0.6.0 assets or full R11 signing/distribution acceptance.
The first fixed-branch artifact run (`39581323365`) exposed a separate Settings
regression before packaging on all five targets: the minimum body width and
default window width were both 440, clipping the Editor toggle behind the frame.
The default now reserves measured frame/scrollbar space in addition to the shared
body width. Direct pointer coverage stays unscrolled, and the Settings matrix adds
normal 1280x800 startup across all shipped locales/themes (88 cases in total).
Full local workspace CI with live AI disabled and WASM lint pass; planning and
finished-step rubber-duck reviews found no blockers. Remote packaging is now
verified: artifact-only run
[`39585598204`](https://github.com/AxelWol/Thinoda/actions/runs/39585598204)
at `df8d1b7` passed native tests and packaging on Windows x64/ARM64, macOS
Intel/Apple silicon and Linux x64. Both macOS jobs passed signing, notarization,
stapling and installed-package verification; all five artifacts were uploaded.
Publish/WinGet were intentionally skipped. This live smoke test does not prove
Apple cannot exceed the retry budget; deterministic tests cover retry behavior.
The fixes remain on the feature branch, and v0.6.0 release assets have not been
repaired or replaced.

**Delivered R2 follow-up - shortcut-reference entry in Settings:** a plain
Keyboard Shortcuts action beside the existing category tabs reuses `Cmd::Shortcuts`
and its searchable/filterable reference. No new empty category or pretend rebinding
control is introduced; the reference-only notice remains visible. Settings stays
open but disabled below the modal. Close/Escape returns to the same category,
retaining unsaved AI key text and settings. Existing command behavior, including
committing pending map title edits before showing the reference, is unchanged.
The 88-case shipped-locale/Auto, light/dark, Home/Map/Outline and category matrix
clicks the entry and types search/key-draft text at a measured logical 320x240 with
200% scaling. It checks modal disabling, the reference-only notice, both dismissal
routes and unchanged document/history/selection/preferences without credential,
network or picker calls. Existing shortcut and Settings viewport suites and native/
WASM lint pass. Planning/finished rubber-duck reviews found no blockers. Shortcut
rebinding, conflict/reserved-key rules and native keyboard/accessibility acceptance
remain open; the broader preferences navigation is not claimed complete.

**Delivered R2 follow-up - reachable Settings:** the nonmodal preferences window
is constrained to the measured viewport with a stable, locale-independent ID,
both-axis scrolling and focus reveal. Its minimum content width remains 440
points; narrow viewports scroll horizontally rather than squeezing existing
labelled controls. Title/close controls remain outside the scroll region.
At 320x180, attempted scrolling reproduced negative-coordinate API-key buttons
and, with a long preferred-folder path, an unreachable Choose action. The nested
right-to-left key row is replaced with labelled wrapping controls, and the default
folder uses a stacked label/path/action; detected folder rows wrap. Credentials,
drafts, nonmodal behavior and event-driven settings saves retain their semantics.
The 66-case sequential locale/theme/viewport-scale matrix covers all four
categories, actual button paint/clip bounds, real Tab focus reveal, long preferred
and detected paths, Cloud Use, resize/live scale, close and unchanged workbook/
history. AI draft retention is checked without invoking credential storage,
network testing or file pickers. Existing settings/sidebar regressions and native
all-target/WASM lint pass. Planning and finished-step rubber-duck reviews found
no blockers; the finished review's keyboard-coverage suggestion was addressed
with real Tab traversal. GPU/OS input and picker acceptance remain manual work.
This does not complete the broader preferences shell or shortcut rebinding.

**Delivered R2 follow-up - compact Map/Outliner switching:** the bounded bottom
controls begin with a current-view icon/label button reusing `ToggleOutliner`.
Pointer, Enter and Space agree with toolbar/menu/palette/shortcut semantics;
the alternate-view hover hint uses egui's platform shortcut formatting. Accessible
names include the View category to distinguish the existing Outliner toolbar
button. Long labels truncate to the actual control-region width, retaining their
full accessible names; new keyboard focus reveals the control in its own region.
The existing status-visibility preference, Zen and Home hide the control, and
existing palette/modal chrome disabling is inherited. View changes do not commit
or discard pending inline titles, mutate the workbook/history or clear selection.
Sequential tests cover all ten shipped languages plus Auto, light/dark and
320x240 at 200%, 640x480 at 150%, and 1000x700 at 100% UI scaling, asserting actual
logical viewport sizes and label paint/clip bounds, pointer/keyboard toggling,
shortcut parity, focus reveal, draft retention and visibility/modal behavior.
The paint assertion reproduced a long-label overflow; limiting the button width
and truncating fixed it. Existing bottom/menu/Outline/editing regressions pass
(69 cases across five suites), as do native all-target and WASM lint.
Planning and finished-step rubber-duck reviews found no blockers. This completes
the compact view-switch affordance, not the floating-bar redesign, Gantt/Pitch or
native GPU/touch/assistive-technology acceptance.

**Delivered R2 follow-up - remembered sidebar choices:** persisted Format/Symbols
family and Style/Map tab are restored at startup independently of panel visibility.
Switching to Symbols or hiding the panel retains the remembered formatting tab;
explicit topic formatting, Notes and Hyperlink intentionally select/remember Style.
The platform settings own the typed schema, with existing app enum imports
preserved through reexports. Old files default to Format/Style, while unknown
future string values safely fall back without quarantining the settings file or
resetting unrelated preferences. Event-driven saves synchronize runtime choices
before writing; no new idle-frame disk writes are introduced.
Real temporary config save/load/restart tests cover EN/DE and light/dark, old and
future config, hidden-panel reopening, Home rejection, UI tab selection and the
three Style-entry commands. Workbook/history/selection stay unchanged, and a
config sentinel detects unintended idle saves. Existing sidebar lifecycle,
topic-field and settings tests plus 60 platform library tests pass; native
all-target and WASM lint pass. Planning/finished rubber-duck reviews found no
blockers. Sticker/Illustration/Pitch panes and other preferences remain open.

**Delivered R2 follow-up - responsive sheet/bottom navigation:** reproduced twenty
long sheet names pushing Zoom to x=3604 in a 640-point viewport. Sheet tabs and
status/zoom controls now have explicitly bounded independent horizontal regions;
New Sheet retains a fixed slot. Long tabs truncate to their region, with full
tooltips and accessible names. Hidden scrollbars preserve the compact row height;
ordinary mouse wheels map to horizontal scrolling within the hovered region, and
horizontal-device scrolling and touch-drag support use egui's scroll handling.
Controls use natural left-to-right insertion/Tab order. Newly focused responses
reveal themselves only in their own region; unchanged focus does not pin manual
scrolling. Active-tab reveal tracks document identity, active sheet, region width
and sheet-name geometry, covering rename/undo/resize and document switching.
Returning to an existing document intentionally reveals its active sheet rather
than preserving browsing of unrelated sheet tabs. The existing status-visibility
preference still hides the right-hand controls and expands sheet navigation;
it does not remove the sheet bar.
Regression coverage includes pointer/keyboard first/last-tab access, tab context
actions, independent scrolling and Tab/Shift+Tab order, title rename/undo, creation,
resize and document return, unchanged sheet content/history on navigation and
stable bar height. A sequential 198-case matrix covers all ten shipped languages
plus Auto, light/dark, 100/150/200% scaling and measured 320x240/640x480/1000x700
logical viewports, including actual zoom/statistics/truncated-title paint and clip
bounds. Existing menu tests now share a semantic Menu-ancestor selector, avoiding
ambiguity with newly named background New Sheet/Fit buttons. Planning and finished
rubber-duck reviews found no blockers; full workspace CI with live AI disabled and
WASM lint pass. Actual native GPU/touch/screen-reader checks,
the floating view bar and remaining R2 preferences are not claimed complete.

**Delivered R1 follow-up - global-shortcut catalog consistency:** one ordered
binding table now drives global dispatch and the palette/reference's global
shortcut hints. All 24 existing global bindings are represented exactly once;
Close Tab, Next Document and Command Palette were previously absent from the
catalog. The live catalog resolves Close Tab to the current active document,
including normal dirty-close prompting, rather than embedding tab zero.
Existing catalog IDs are preserved by appending the new commands. Dispatch
precedence, text-input style-copy/paste guards, modal ownership, Escape and
file-drop handling remain unchanged. Global hints use egui's platform formatter;
contextual and menu hints display Cmd/Option rather than Ctrl/Alt on macOS.
Three-platform simulated tests cover catalog completeness/uniqueness, exact
global hints, menu labels and real global dispatch; actual palette keyboard
activation closes the active clean tab and preserves the active dirty tab with
its save prompt. Shipped-locale/appearance/scale viewport regressions remain in
place. These simulations do not replace native keyboard-layout/OS acceptance.
The configured reviewer could not start; the user approved direct planning and
finished-step review. No rubber-duck review is claimed.
Full workspace CI with live AI disabled and WASM lint pass.
This is groundwork, not the complete shared command model or shortcut editor:
menu layout/labels and Outline bindings still have separate definitions, saved
`shortcuts_overrides` remain inactive, and conflict validation, reserved-key
handling, rebinding/reset UI and P4 native menu backends remain open.

**Delivered R1 follow-up - contextual shortcut inventory and aliases:** the map
and host insertion dispatchers now expose their existing ordered binding tables.
Palette contextual hints derive from those tables instead of duplicated strings;
the read-only reference lists every map/insertion alias, including Redo Ctrl/Cmd+Y,
Subtopic Insert, Delete Backspace, topic-only deletion, central-topic Home,
zoom aliases, boundary aliases and priority 1/2/3. Move Up/Down and Select None
now show their real bindings instead of appearing unassigned. Edit/navigation
reference rows also derive their keys from dispatch. Escape remains a separate
Cancel reference entry: it clears interaction state as well as selection, and
is not converted into an ordinary action. Clipboard events and type-to-edit also
retain their separate event paths. Existing palette IDs, key order, action guards
and Undo behavior are unchanged. This is a map/insertion inventory, not a claim
that every binding applies in Outline or focused text controls.
Three-platform simulated inventory tests check every alias, deduplication,
catalog/reference agreement and stable unique IDs. Real key tests cover
Tab/Insert, Delete/Backspace, both Redo keys, Home/Ctrl-R, priority edits with
Undo, modal isolation and preservation of pending title input. The compact
shipped-locale/appearance/scale matrix now measures long alias paint/clip bounds,
in addition to filters, keyboard traversal, live resize and footer reachability.
The configured reviewer could not start; the user explicitly approved direct
planning/finished review for this slice. No rubber-duck review is claimed.
Full workspace CI with live AI disabled passed; final reference/viewport
regressions, native all-target lint and WASM lint also passed.
Menu layout/labels, Outline-specific shortcuts, conflict/reserved-key auditing,
custom rebinding/reset and native input/accessibility acceptance remain open.

**Delivered R1 follow-up - command-aware menu shortcut hints:** main File/Edit/
Insert/AI/Tools/View/Help buttons and canvas topic-context buttons now resolve
shortcut hints by command identity using the shared global/contextual binding
inventory. Nested Select/Arrange entries reveal their existing bindings too.
Catalog and menu hints share egui's platform formatter rather than duplicating
key strings or replacing Ctrl/Alt text manually. Non-character key names and
macOS modifier symbols follow egui; existing catalog IDs, dispatch order,
command handlers, availability, context accessible names and popup wrappers are
unchanged. Clipboard Ctrl/Cmd+X/C/V are explicit metadata for existing OS events,
also visible in the read-only reference; they are not added to key dispatch or
the command palette. Direct review narrowed an initial palette addition to
metadata only, avoiding an unrequested clipboard focus/view behavior change.
Tests simulate Windows/macOS/Linux in 640x240, 640x360 and 1200x800 viewports,
checking actual hint paint/clip bounds, catalog/reference parity, availability,
nested Select/Arrange and context labels. Existing locale/theme/scale matrices
retain popup geometry, focus/activation, Escape and unchanged documents.
Clipboard metadata tests preserve copy/cut output, OS paste requests, real paste
events and Undo. Host-dependent expectations in adjacent menu tests now use the
same platform formatter instead of assuming Ctrl on every target.
The configured reviewer could not start; the user approved direct planning/
finished review for this slice. No rubber-duck review is claimed.
Full workspace CI with live AI disabled passed before the final clipboard
metadata-only narrowing; final menu/reference/viewport/palette regressions and
native all-target/WASM lint passed after that narrowing.
This is not completion of R1 or P4: menu layout/labels, Outline-specific shortcut
metadata, custom rebinding/conflict/reserved-key/reset handling and native
keyboard-layout/accessibility acceptance remain open. Default hint metadata is
immutable; inactive saved shortcut overrides are not applied.

**Delivered R1 follow-up - view-aware Outline shortcut metadata:** the eight
existing Outline action bindings now supply dispatch and live menu, palette and
reference hints. Map defaults remain compatible for static catalog consumers;
runtime metadata uses the current view without changing command IDs, handlers,
availability, key order or focus guards. Outline advertises Ctrl/Cmd+Y for Redo,
Tab for child insertion, Delete and F2, not Map-only aliases. Shared insertion/
priority bindings remain visible in both views. Up/Down descriptors are reused
by the original visible-row traversal, never geometric canvas navigation.
The read-only reference names its current view using existing localized labels;
Map-only clipboard-event and canvas-Escape rows are omitted from Outline.

Three-platform simulations cover binding completeness, IDs, aliases and actual
menu hint paint. Outline regressions exercise collapsed-row navigation, Delete,
sidebar-focus Undo/Redo and focus preservation; the existing locale/theme/scale
toolbar and compact-reference matrices retain their coverage. The configured
reviewer could not start; the user approved direct planning/finished review for
this slice. No rubber-duck review is claimed. Full workspace CI with live AI
disabled and WASM lint pass. This closes the metadata follow-up,
not R1: editable bindings, modifier/reserved-key conflict auditing (including
existing Outline modified-key matching), presets/reset, native keyboard-layout/
IME/accessibility acceptance and OS-integrated P4 menus remain open.

**Delivered R1/R2 user follow-up - Zen Escape and Search & Replace height:**
Zen now exits on Escape with a focused Map canvas or Outline topic, not only
when no widget owns focus. Existing F11 dispatch remains compatible, but the
ten-locale entry hint advertises Escape so web users need not rely on a browser
fullscreen key. Topic/text edits, relationship creation, pointer gestures,
search, settings, popups, palette and modals retain Escape ownership before Zen;
exiting Zen consumes the event before it can clear the canvas selection.
Focused search previously lost Escape to text/canvas handling. Search dismissal
now consumes it before the center view, clears the filter and leaves Zen active;
a subsequent Escape exits Zen without changing the document/selection.

Search & Replace inherited egui Area's earlier compact height, causing clipped
Replace All content even with room available. The content now recalculates its
maximum height from actual remaining viewport space every frame, allowing
replacement controls to expand naturally instead of increasing a fixed constant.
Overflow scrolling is retained only when content genuinely exceeds available
height. The shipped-locale/light/dark matrix includes normal and high-scale
viewports with simultaneous Search/Replace/Replace All paint visibility,
replacement toggle expansion, short-window focus reveal, edits/Undo and dismissal.
Zen tests exercise real focused topics, inline editors, search, palette/modal,
relationship cancellation and context-menu Escape priority. Direct planning/
finished review was user-approved after reviewer startup failed; no rubber-duck
review is claimed. Full workspace CI with live AI disabled and WASM lint pass.
Browser fullscreen and native OS input acceptance remain
separate from these shared UI regressions.

**Delivered R1 prerequisite - default shortcut conflict audit:** read-only
diagnostics now compose the real global -> insertion -> Map/Outline action
tables in dispatch order. They distinguish an exact duplicate from a declared
gesture shadowed by a less-specific earlier command, ignore same-command
aliases, and use egui's actual logical modifier matching with host Ctrl/Cmd
normalization. A separate diagnostic resolves an arbitrary gesture without
executing it. Scope is an idle, focused topic with no modal/popup/search/editor;
Outline visible-row traversal, clipboard events, type-to-edit and OS/browser
reserved keys are not falsely modeled as command bindings.

The current Map inventory has one cross-command declared collision:
Ctrl/Cmd+Shift+H advertises Fit but the earlier Ctrl/Cmd+H Find & Replace handler
consumes it. Outline has no declared collision, but probing Ctrl/Cmd+Shift+Z
confirms its existing Undo handler consumes that undeclared modifier variant;
the same gesture is an explicit Redo binding in Map. These are recorded
behavioral decisions, not silently changed defaults. Fit remains available by
menu/palette. Future editor work must choose the modifier-specificity policy,
reconcile hints for unreachable bindings, assign stable persisted command
identities (catalog row numbers are not a migration contract), and separately
validate reserved host/browser keys before applying overrides.

Tests compare every declared binding plus extra Shift/Alt variants against
egui InputState consumption on three simulated hosts, distinguish physical
Ctrl from Cmd on macOS, and reproduce Find & Replace interception and Map/
Outline Undo/Redo behavior through real app frames. Targeted audit/UI tests
and native all-target/WASM lint pass. The configured reviewer could not start;
the user approved direct planning/finished review for this audit slice.
No rubber-duck review is claimed. No dispatch, hints, preferences or saved
overrides are changed; R1 shortcut editing and P4 remain open.

**Delivered R1 follow-up - reachable shortcut hints:** main/context menus,
runtime palette and reference aliases now exclude declared gestures intercepted
by a different command in the actual default dispatch order. The static legacy
catalog uses the same rule with Windows Map defaults; runtime hints use the
current view and host modifier semantics. Fit Map no longer advertises
Ctrl/Cmd+Shift+H, and appears under Unassigned in the read-only reference.
It remains available through menu/palette; no dispatch order, key mapping,
availability, command ID or saved override changes. Raw inventories and the
conflict report retain the declared collision for the future keymap decision.
Clipboard-event and navigation reference metadata are retained outside the
command-key audit. Immutable dispatch tables are cached rather than rebuilt
for every hint/alias lookup.

Three-host inventory tests verify intercepted gestures are absent while other
aliases/IDs remain consistent. Tests cover Fit's unassigned state, enabled
main/context menu entries, real click execution, actual painted menu hints and
existing compact reference geometry. Targeted shortcut/audit/menu/reference
and Fit execution tests plus native all-target/WASM lint pass. Direct planning/
finished review was user-approved after reviewer startup failed; no rubber-duck
review is claimed. Editable bindings, keymap decisions, reserved keys and native
input/accessibility/P4 acceptance remain open.

**Delivered R1 prerequisite - stable shortcut target identities:** built-in
commands now have explicit, nonlocalized IDs such as `file.save` and
`topic.delete-only`, separate from existing numeric palette UI IDs. A lookup
registry covers the static palette, global/Map/Outline bindings, reference-only
editing/navigation/priorities/clipboard targets, and supported export formats.
It does not add palette actions or enable shortcuts. Close Tab normalizes to one
identity and resolves the active tab at lookup time; availability/focus checks
remain the existing dispatcher's responsibility. Generic Paste resolves to the
OS request command, never persisted clipboard content. Arbitrary file paths,
templates, custom styles/themes, marker-instance edits and clipboard payloads
cannot silently alias a generic target.

The namespace is pinned in regression coverage and survives display reordering,
all shipped locales and Map/Outline changes. Tests cover complete command/binding
coverage, uniqueness, active/Home Close Tab resolution, unsupported payloads and
unchanged palette IDs/count. Settings roundtrip preserves stable, legacy,
numeric and unknown override entries exactly; they remain inactive. Existing
numeric IDs and the legacy `save` key are not guessed into the new namespace.
Targeted command-ID/shortcut/menu/settings tests and native all-target/WASM lint
pass. The configured reviewer could not start; direct planning/finished review
was performed under autopilot, not claimed as a rubber-duck review. Remaining:
versioned override schema and migration, key capture, specificity/conflict and
reserved-key decisions, apply/reset UI, native input/accessibility and P4.

**Delivered R1/R2 follow-up - shortcut-reference viewport safety:** the shortcut
dialog no longer assumes a 100-point minimum list or a fixed 300-point chrome
budget. Its frame is bounded by the current logical viewport and actual frame
margins. One scroll surface contains the reference and filters; a measured Close
row remains visible outside it. The footer is reserved before content layout but
its interactive button is registered last, preserving Search/Category/Unassigned/
Close Tab order and reverse traversal. Newly focused controls scroll into view,
without pinning manual scrolling. Search/category/unassigned changes reset the
reference scroll.
The original overflow was reproduced at a 240-point logical height. The sequential
shipped-locale/Light/Dark/System matrix covers 100/150/200% application scaling down
to 320x180 logical points, explicitly checking physical-size/scale conversion,
modal/category-popup bounds, Close hit/paint bounds, first/last reference hit/paint/
clip reachability, real filters/search, Tab/Shift+Tab, live resize, Close/Escape and
Outline state preservation. Existing Map/Outline shortcut tests remain green.
The finished-step review caught a footer-first Tab-order regression; it was
reproduced and fixed with a regression assertion before committing. Rebinding,
native keyboard layouts/assistive technology and smaller-than-tested viewports
remain outside this slice.

**Delivered R1/R2 follow-up - command-palette bounds and modal isolation:** the
palette now reserves its search field above viewport-bounded results, accounting
for actual frame margins. Keyboard selection scrolls into view without pinning
manual browsing; query changes reset results. The existing twelve-result cap,
ranking, shortcuts and Enter behavior remain unchanged.
The Escape failure was reproduced: the canvas consumed Escape before egui's
modal could dismiss. Palette-open now disables app chrome/Outline input and uses
the Map paint-only snapshot, preserving selection and pending title drafts.
egui's own modal handles Escape; no duplicate Escape policy was introduced.
Direct palette entry rejects other active dialogs/settings/about, and remains
able to toggle itself closed. Regression tests also verify that a background
menu click dismisses the existing modal without opening the menu or palette.
Shipped locales, Light/Dark/System and explicitly verified logical sizes down to
320x180 cover Home/Map/Outline, frame/hit/paint/clip bounds and first/twelfth result
reachability. Safe fake-command tests verify Down through twelve results, manual
scrolling, Up, refiltering and exactly-once Enter activation. Real File-command
palette tests now close with Escape. Planning and finished reviews completed;
modal-precedence feedback is covered by shortcut, direct-dispatch and background
click tests. Native screen-reader/keyboard-layout execution remains open.
Windows x64 `cargo xtask ci` passes after these changes with
`THINODA_AI_SKIP_LIVE=1`; WASM clippy also passes. These checks do not replace
native GPU/browser execution or the outstanding platform acceptance matrix.

R2 main-menu overflow follow-up: the hamburger root and every nested main-menu
popup use viewport-height-bounded vertical scrolling, subtracting the actual menu
frame margins. The popup's initially estimated height no longer becomes its
permanent scroll limit. Newly focused commands scroll into view; unchanged focus
does not pin subsequent manual scrolling. Existing command availability, nested
template/Arrange paths and dismissal behavior remain intact.
The expanded matrix tests all shipped locales and Light/Dark/System at
640x360/100%, 960x540/150%, 1280x720/200% and 1280x480/200% (240 logical pixels
high), without enlarging the window to reach leaf actions. It checks popup area
rectangles with half-physical-pixel rounding tolerance, first/last command
reachability, nested template/Arrange commands, assistive focus, Up/Down scrolling,
Enter activation and unchanged document/selection. Application all-target and WASM
clippy pass; Windows x64 `cargo xtask ci` passes with live AI disabled.
Finished-step rubber-duck review has no blocking findings.
Context/toolbar roots are handled by the follow-ups below; nested color pickers,
text-control menus, all-command keyboard traversal, native accessibility and
actual native/browser execution remain open. R2 and P4.2 are
not complete; reuse these sizing/focus helpers when auditing the remaining menus.

**Delivered R2 mitigation - context-menu overflow (user screenshot, 2026-10-20):**
the German topic context menu extends below the app window, hiding later commands.
Applied the proven viewport-height/focus-aware scroll helper to canvas/topic/object/
symbol context menus, sheet-tab context menus and custom quick-style menus, plus
their Select/Arrange/AI submenus. Preserve current labels, availability and direct
command paths during this safety fix; use separators for semantic groups. Exercise
short windows, edge anchors, 100/150/200% app scaling, shipped locales and appearance
modes, scroll-to-last/first and keyboard activation with no accidental document
changes. Include actual popup geometry and existing context-action regression tests.
Planning and finished-step rubber-duck reviews found no blockers. The new sequential
locale/theme/scaling matrix verifies topic and sheet menus, nested Arrange/AI,
corner anchors, disabled commands, keyboard activation, undo and unchanged
document/selection; existing context action, symbol and quick-style suites pass.
Follow with the broader P4.2 grouping/accessibility audit (including text-control
menus owned by egui); do not claim native OS menus or full P4.2 acceptance here.
Toolbar-triggered symbol/marker, export, zoom and statistics popovers remain a
separate audit (toolbar roots are now covered by the follow-up below).
The user-supplied review explicitly requires opening the nested symbol picker after
scrolling the parent, checking its anchor, editing a color, contrast actions,
Escape/focus isolation and document/selection preservation with Undo; the
delivered follow-up below covers this instance-menu case.
Toolbar roots are not covered by this context-menu slice: marker/symbol insertion,
Export, New-from-Template categories, Zoom and topic statistics require separate
P4.2 evidence. The follow-up below covers named toolbar roots and the earlier
main-menu matrix covers template categories; neither closes nested popup or
native/browser accessibility acceptance.

**Delivered R2 follow-up - nested symbol-instance color picker (2026-10-22):**
the requested interaction test reproduced a functional failure, not merely an
untested edge: egui's standard color button opens a memory-backed root popup,
replacing the context menu's open state and making both disappear. Symbol instance
menus now use an actual egui submenu with the shared height/focus scroll helper
and outside-click dismissal, containing the existing egui color editor.
Sidebar/insertion color buttons are unchanged. The sequential shipped-locale/Auto,
light/dark, 100/150/200% matrix opens the real symbol context menu, shrinks the
viewport to 120 logical points, proves the parent actually scrolls, then opens
the nested picker and checks bounded geometry and unchanged anchor position when
focus moves to the child layer. It changes an actual numeric color channel,
verifies the picker stays open, dismisses with Escape and restores the workbook
with one Undo; a separate contrast action also restores with one Undo.
Selection remains unchanged. Extremely short app chrome and other color-picker
surfaces are not accepted by this fixture; it specifically tests open-menu resize
and scrolling. Text-control menus and native/browser/accessibility checks remain
open. The configured reviewer could not start; the user explicitly approved direct
planning/finished review for this slice. No rubber-duck review is claimed.
Direct review caught the shared sidebar/instance rendering path; the submenu is
explicitly context-only, and sidebar color-well roles are regression-checked.
Full workspace CI with live AI disabled, final targeted symbol/context/toolbar
regressions and native all-target/WASM lint pass.

**Delivered R2 follow-up - toolbar popup bounds:** top-bar Export, Map insertion
Marker/More, and status-bar Zoom/Statistics now reuse the same viewport-height
scrolling/focus helper. Removed the symbol insertion menu's independent 560-point
limit, which could exceed short windows. Menu responses, localized accessible
names, color controls and existing commands remain intact; Export now has an
explicit localized accessible name rather than only an icon/tooltip.
The sequential shipped-locale/Light/Dark/System matrix covers 100/150/200% scaling,
240-logical-point short windows and a normal-height case, actual foreground
popup bounds, first/last item reachability, Marker/More width constraints, marker
application with undo, keyboard zoom activation and Escape/state preservation.
Popup-frame checks allow one physical pixel for independent frame/origin rounding;
action reachability is checked against the unexpanded viewport.
Planning and finished-step rubber-duck reviews have no remaining blockers.
Nested egui color pickers, text-edit context menus, native/GPU and assistive-
technology acceptance remain separate checks; this does not complete R2/P4.2.

**Delivered R2 mitigation - Map pane maximum-resize corruption (user screenshot,
2026-10-20):** reproduced with actual pointer dragging in the Windows headless
app harness. The built-in theme grid inherited horizontal cell layout: preview
and caption expanded cards beyond their intended width, making the panel's
clamped boundary disagree with its content origin. Reused the existing fixed-size
preview-card control, with width-aware columns, wrapped captions and keyboard
activation. This bounds cell allocation even with long localized names instead
of hiding the problem with a larger panel or a second horizontal scrollbar.
The regression matrix covers shipped locales, Light/Dark/System, 100/150/200%,
minimum/maximum/repeated drag, in-drag and released geometry, tabs, window shrink,
hide/reopen and restoring egui panel memory into a new app harness. It checks
actual caption paint/clip bounds as well as hit rectangles, scrolls every theme
into view in the smaller window, and preserves workbook/selection/revision.
German keyboard theme application and undo are covered. Planning/finished-step
rubber-duck reviews completed; test nonvacuity feedback was incorporated.
Native GPU visual acceptance and actual on-disk host restart remain separate
R0/R2 checks; cloned egui memory is not claimed as a native restart test.

**Delivered R2 mitigation - Outline/structure toolbar collision (user screenshot,
2026-10-20):** the floating insertion toolbox is now Map-only, matching the existing
contextual topic-toolbar gate. Outline retains its wrapped sibling/child/edit/
move/reparent/delete action rows without a foreground toolbox covering them.
The regression matrix covers shipped locales, Light/Dark/System, 100/150/200%
scaling in a 640-point-wide window, visible/hidden inspector, actual pointer
actions, mode transitions and unchanged selection/revision on view-only changes.
It checks visible foreground layers, button hit bounds and caption paint/clip
bounds; insert/move/indent/delete each exercise model-specific outcomes and
undo/redo. Returning to Map restores the toolbox; switching with More insert
actions open does not retain/reopen its popup. Hidden-inspector cases verify
Format/Symbols reopening and the existing Notes shortcut, which opens Map/Style.
Planning and finished-step rubber-duck reviews completed with no remaining
blockers. Native GPU/assistive-technology acceptance remains a release gate,
not a claim made by these deterministic headless paint checks.

R0 importer robustness slice: eight valid seeds across all seven import formats
(native ZIP and raw JSON both included) receive systematic boundary truncations
and 128 deterministic byte mutations per seed. Every accepted workbook validates
and round-trips through native storage; unexpected panics fail normally. Structured
XML entities/attributes/depth through 64 and malformed XMind JSON receive the same
invariant checks. Forged declared ZIP entry lengths above 200 MiB exercise the
existing decompression guard without large allocations. These bounded smoke
checks run in normal workspace CI. At the time this first regression slice landed,
coverage-guided fuzz targets, longer campaigns and deeper resource-limit
exploration remained open; the dedicated fuzz follow-up is recorded below. The
regression slice itself added no dependencies or production parser behavior changes.

Coverage-guided smoke targets now exercise XMind's JSON/XML package and raw XML
paths plus FreeMind and OPML parsing. Each has a small original seed corpus and
dictionary; a separate nightly Linux CI job gives each target 20 seconds, caps
inputs at 1 MiB and RSS at 2 GiB, and kills an input taking over five seconds.
The fuzz-only dependency graph and exact local commands are documented in
[`fuzz/README.md`](../fuzz/README.md). This bounded smoke is not a long campaign,
does not prove arbitrary inputs safe, and does not replace the existing oversized
archive-entry guard tests. It introduces no shipped dependencies or parser
behavior changes.
Formatting, all-target clippy, 391 workspace cases and three benchmark correctness
tests pass with live AI disabled. The last Windows installer remains the app-feature
build from `125c0a4`; the benchmark and importer-smoke slices do not change app behavior.

R0 CPU performance baseline delivered: deterministic 10,000-topic six-way and
wide maps report nearest-rank p50/p95/max layout, undoable leaf-rename transaction
and undo times as JSON. One layout warm-up is excluded; fixture construction and
validation, serialization and file I/O are outside timed operations. Layouts are
checked for complete topic counts/finite overall bounds, and every undo restores
the leaf title/history. Optional positive finite p95 layout budgets fail explicitly
after the report is written. CLI bounds and percentile/fixture tests accompany the
benchmark; CI is configured to upload each runner's report without imposing one
hardware budget on heterogeneous hosts. Those native CI runs are not claimed here.

Local Windows x64 baseline, 2026-10-16: AMD Ryzen 7 PRO 9755, 8 cores/16 threads,
rustc 1.99.1, release build, ApproxMeasure, light mode, 30 samples. Initial p95:
balanced layout 23.68 ms / rename 3.81 ms / undo 0.0015 ms; wide layout 28.71 ms /
rename 3.34 ms / undo 0.0013 ms. A repeat with a local-only 50 ms layout budget
passed at 24.46/34.93 ms layout p95. This sets an initial regression envelope on
this machine, not a portable performance guarantee or the 16.7 ms frame target.
There is no GPU backend, viewport, real font shaping, pan/zoom frame sampling or
IME measurement in this benchmark. Native frame-p95/60-fps profiling and deeper
mixed-content/structural-edit workloads remain open.
`cargo xtask ci` passes formatting, all-target clippy, 387 workspace cases and
three benchmark correctness tests. A deliberately impossible budget was checked:
the process failed and still wrote both complete case results with
`passed_budget: false`. This developer-only slice does not change the packaged app.

R1 shortcut-reference slice delivered: F1/Help opens a searchable EN/DE browser
from the palette catalog with category/unassigned filters and navigation aliases.
Fixed footer, scroll reset and modal isolation cover narrow windows, Map/Outliner
and Home. The UI explicitly states rebinding is not available; conflict/reserved
key editing and reset-all remain open. Advertised Ctrl+Q now requests native
window close, with the existing unsaved-document prompt preserved. All 387
workspace cases and all-target clippy pass with live AI disabled.
Windows EXE/NSIS rebuilt with the shortcut reference and Quit fix.

R1 offline help delivered: searchable EN/DE User Guide and Quick Start are wired
through Help and the command palette, with six offline sections and accurate
interchange, recovery, cloud and AI privacy limitations. Home and narrow windows
are covered. Modal input isolation preserves map/outliner edits and selection;
search resets scroll, Escape closes the guide and popup Escape closes the popup
first. Feedback opens the public issue chooser without document/log uploads;
Project News opens repository history rather than claiming an updater or releases.
All 384 workspace cases and all-target clippy pass with live AI disabled.
Windows EXE/NSIS rebuilt with offline help.

R5 original-template expansion: 18 scenario maps expand the gallery from 17 to 35,
covering knowledge frameworks/architecture/problem solving, business/workshops,
research/timetables/notes/languages, retrospectives, shopping/reflection/services,
cause/effect/comparison/social-media planning and recipes/film production.
Scenario-specific branches and editable prompts are original Thinoda content.
Gallery names are EN/DE; document starter content remains English. Every registered
template participates in actual preview/Create, keyboard selection, native/XMind
round-trip and responsive layout checks. Timetable is a topic hierarchy, not a
calendar. AR, BCG/organization/nine-box matrix semantics, truncated reference
titles and native reference equivalence remain open; R5 is not finished.
All 379 workspace cases and all-target clippy pass with live AI disabled.
Windows EXE/NSIS rebuilt with all 35 templates.

R5 template-section navigation delivered: eight accessible EN/DE chevrons/counts
open category views, and fixed All templates returns to the overview. Per-view
scroll state restores the overview position rather than inheriting a deep offset
in the category view. Navigation clears selection, does not create maps or alter
recents, and preserves source documents. Keyboard activation and all categories
at 480/1280 widths are covered. All 378 workspace cases and all-target clippy pass
with live AI disabled; Windows EXE/NSIS refreshed. Additional reference-specific
template content, Grid-family and visual skeleton work remain open.

R5 custom-theme authoring delivered: staged EN/DE editor and actual-renderer
preview support a built-in base, 1-16 colors, light/dark backgrounds, global font
and five role defaults. Apply embeds the theme in one undo step; Save to Library
persists a separate profile copy without changing the sheet. Library edits save
under a new name; duplicate names are rejected, removal is confirmed, and failed
profile writes preserve the library and map. Existing documents remain independent
of library removal. Explicit map/topic overrides, built-in reset and opt-in recent
custom-theme reuse are covered. Native/private XMind snapshots retain the editor
data; supported public JSON/XML light-mode topic properties are materialized.
Editable imported XMind theme registries and native visual parity are not claimed.
Modal footer geometry is bounded and toast notifications render below modal
controls, preventing pointer interception. All 377 workspace cases and all-target
clippy pass with live AI disabled. Reference-specific templates, Grid-family and
visual skeleton semantics still keep R5 open. Windows EXE/NSIS rebuilt with the
custom-theme editor and library.

R5 quick-style library delivered: four original actual-renderer presets and
named snapshots of resolved appearance reuse the existing topic style model.
Multi-topic application is atomic/undoable across open documents without copying
content. Style, Edit/context and palette entry points share command availability.
EN/DE save/removal dialogs consume keyboard input without editing the canvas or
outliner underneath; removal is confirmed. Up to 32 named styles persist in the
local profile with validation, duplicate rejection and transactional failure
rollback. Settings reload, exact capture across all themes/modes, persistence,
native/XMind applied-style roundtrips and keyboard/undo behavior are covered.
Formatting and all-target clippy pass. All 364 cases pass across the integrated
partial run and standalone IO/platform/UI suites (live AI disabled). The full
workspace invocation stops after 290 passing cases because Windows Application
Control refuses to launch its IO test executable; no policy was changed.
Windows EXE/NSIS rebuilt with quick styles.
The custom-theme editor was subsequently delivered above; this does not finish R5.

R5 map-wide branch width delivered: Automatic retains theme/level defaults;
explicit 0.5-16 widths apply across topic roles without replacing per-topic
overrides. The EN/DE Map control buffers numeric edits until confirmation and
supports Escape cancellation; invalid drafts neither dirty maps nor add history.
Shared single-line sidebar fields now consume confirmation/cancel keys so Enter
does not leak into canvas topic insertion. Native/private snapshots retain the
setting, and public XMind JSON/XML carries effective widths.
All 352 workspace cases and all-target clippy pass with live AI disabled.
Windows EXE/NSIS rebuilt with map-wide width controls and safe numeric editing.
Quick-style libraries and the custom-theme editor are delivered above.

R2/R5 real global/topic fonts delivered: selectors enumerate installed system and
user fonts, registering requested faces lazily in the reusable UI crate. Canvas
measurement, painting, inline editing and Map Shot use the same family; font
changes invalidate cached geometry even when the number of registered faces is
unchanged. Missing fonts produce visible warnings, preserve names and do not
dirty maps. Public XMind JSON/XML receives effective names, while native files
retain inheritance. All 348 workspace cases and all-target clippy pass with live
AI disabled. Fonts are not embedded; CJK fallback/IME, exact vector-export text
metrics and non-Windows font-host QA remain open.
Windows EXE/NSIS rebuilt with real font support.

Native XMind geometry verification is currently blocked: both a new basic map
and the previously repaired local sample remain on loading screens in the
installed XMind app. No public branch-side/curve-control fields were inferred.

R3 Flexible Floating Topics delivered: the undoable per-sheet Advanced Layout
switch defaults off for old maps. Enabled floating roots can overlap topics
without attaching; attached roots retain reparent/reorder but cannot detach onto
blank space. Mixed floating/attached selections fail explicitly without partial
moves. EN/DE control, per-sheet isolation, native/XMind snapshot persistence,
zoom/pointer behavior, one-step undo and stale-setting cancellation are covered.
This is not collision avoidance or Branch Free-Positioning; those remain open.
All 344 workspace cases and all-target clippy pass with live AI disabled.
Windows EXE/NSIS rebuilt with the setting and drag behavior.

R3 hierarchy insertion/side delivery: attached-topic outer quarters choose sibling
slots along the connector axis and centers retain reparenting. Shared target data
drives the accent insertion line, proposed connector and atomic multi-topic move;
reverse split-map sides and same-parent index adjustments retain requested order.
No-op slots preserve Summary/boundary ownership without adding undo history.
Split-map root drops set a persistent left/right main-branch side from pointer X.
Style offers Auto/Left/Right for selected main topics, localized in EN/DE. Explicit
sides override balancing and survive additional siblings; old automatic maps and
non-split structures retain their layouts. Native and unchanged Thinoda XMind snapshot
round trips preserve sides; no speculative public XMind direction field was added.
All 340 workspace cases and all-target clippy pass with live AI disabled. Coverage
includes every structure's slot axis, multi-topic insert positions, exact pointer
commit at multiple zooms, side persistence/reset, old serde defaults, native/XMind
reopening and German UI. Native gesture QA remains open.
Windows EXE/NSIS rebuilt with insertion ordering and branch-side persistence.

R3 relationship creation parity delivered from the official Floating Topic guide:
two selected topics connect immediately; a single selected source can connect to an
existing target or create a floating target by clicking blank canvas. The latter
opens localized title editing and commits creation/connection in one transaction.
Invalid sources roll back both operations; self-target errors are visible. Escape,
outside clicks and rapid double-clicks do not create unintended extra topics.
The EN/DE hint describes both target choices and is omitted for immediate two-topic
connections. Six canvas cases plus two app integration cases cover creation, exact
camera coordinates, undo/redo, errors and toolbar/command wiring.

Latest release-candidate checks pass: workspace formatting, all-target clippy and
all 330 workspace cases, including both CLI subprocess cases and isolated German UI.
Live AI tests are disabled with `THINODA_AI_SKIP_LIVE=1`; service authorization and
native installation/accessibility QA are not implied by these results. Earlier
Application Control blocks below are historical, not a failure of this latest run;
no system policy was changed or bypassed.
Windows EXE/NSIS were rebuilt with relationship editing, subtree drag feedback and
the completed relationship-creation flows.

R3 hierarchy-drag feedback delivered: the complete visible owned subtree is painted
at 55% opacity while its source is dimmed, including Summary trees, boundaries and
internal relationships. External relationships stay on the original sheet and
survive reparenting. Drag ownership is cached and bound to document revision, sheet
and selection. Escape, popups and lost drags cancel; invalid self/owned-descendant
or outside-canvas drops report errors without mutation. Central/Summary roots
cannot start a move. Parent drops append atomically and expand collapsed targets;
blank drops preserve existing detach/move behavior. Stationary edge drags pan at
up to 480 logical pixels/second with a 50 ms frame cap and retain pointer alignment.
Eight new cases cover these contracts, visible ghost text, collapsed content,
ownership and undo/redo at multiple zoom levels. All 210 core/canvas/IO/platform/UI,
31 app-library/relationship/German and 60 non-CLI e2e cases pass; formatting/clippy
pass. Full hierarchy ordering, insertion connectors/slots and branch-side placement
are not marked complete.

R3 relationship editing delivered: selected lines retain their original color
beneath a separate overlay with endpoint circles, control squares and guide lines.
Endpoint/control drags preview in view state and commit once on release; Escape,
invalid targets, popup isolation and stale-document revisions leave source content
untouched. Focused control arrows move 1 map unit (Shift: 10); endpoint arrows cycle
visible topics in layout order. Style offers title, endpoints, color, four relative
coordinates and automatic-routing reset. Non-finite/out-of-range coordinates fail
visibly without edits. Shared geometry feeds layout/export, and hit testing uses
the rendered curve segments, including large curves at multiple zoom levels.
Native and unchanged Thinoda XMind round trips retain custom controls; public XMind
control-point interchange is not implemented. Native keyboard/accessibility QA,
hierarchy drag refinement and the rest of R3 remain open.

Relationship delivery verification: all 202 core/canvas/IO/platform/UI cases pass,
including three new model cases and eight new canvas cases; both new app interaction
cases pass. Integrated CI passes formatting/clippy, the app library (28), all editing
e2e including both CLI cases (62), isolated German UI (1), AI and the core/canvas/IO
suites, then Windows Application Control blocks the platform unit-test executable
before launch (4551). The same platform suite passed in the standalone run; this
does not make integrated CI fully green or justify changing system policy.
Windows EXE/NSIS were rebuilt with relationship editing.

R3 Labels delivered: every label-editing entry point uses a single-topic anchored
draft with a pointer, removable chips, comma-separated additions, sheet-local
suggestions and optional Unicode code-point sorting. Apply commits once and saves
the local sort preference; Escape/Cancel/outside dismissal does not change labels
or preferences. The original document key, sheet index, selection and document
revision are checked before rendering/applying, so drafts cannot overwrite another
tab/sheet or newer edits. Comma-containing imported labels remain intact.
Seven targeted interaction cases cover normalization, one-step undo/redo, native/
XMind round trips, source-binding invalidation, EN-independent popup entry points,
sorting persistence and viewport/long-content limits; the isolated German case
also exercises the editor. Actual native keyboard/IME and exact on-map tag
rendering still need their parity audits.
The standalone app library (28), non-CLI editing e2e (60), German UI (1) and
core/platform (113) cases pass, as do workspace formatting/clippy. Windows EXE/
NSIS were rebuilt. The two CLI subprocess cases were deliberately excluded from
that e2e invocation and remain subject to the application-control limitation below.

R6 interoperability coherence delivered: canonical-public-content SHA-256 binds
the optional lossless Thinoda snapshot to the map it represents. External edits now
win over stale metadata; whitespace-only JSON changes preserve lossless reopening.
Old exports without the digest require an exact generated public-content match.
Malformed/invalid private snapshots are logged and ignored. Public theme-role
properties merge beneath inline overrides and persist as explicit topic styles;
editable imported theme registries and full content fidelity remain open.

Verification: four new IO regressions cover external structural edits/deletions,
old/current/corrupt snapshots, lossless/pretty-printed reopening and all supported
topic theme roles. All 191 core/canvas/IO/platform/UI cases and isolated German UI
pass. Workspace formatting/clippy pass. The subsequent Labels integrated CI was
blocked by Windows Application Control before launching the app unit-test binary
(4551); the standalone targeted cases passed. Earlier CLI subprocess policy blocks
are not reclassified as passing. Do not change/bypass application-control policy.

R6 Map Shot delivered: Tools/palette/F7 requests a fresh clean canvas frame and
native GPU readback. The preview keeps only visible canvas pixels, with fractional
DPI conversion, drag/numeric region controls, square cropping, frame settings,
native image clipboard output, PNG save and Escape/Cancel. Pending captures are
bounded, time out visibly and discard late callbacks after cancellation. The map,
selection, camera and source path are retained; an inline title is committed before
capture. Full-sheet export remains a separate workflow and its existing renderer
fidelity limitations are not hidden by this screen-capture implementation.

Map Shot verification: seven focused unit/UI cases, isolated German UI and an
opt-in actual wgpu readback test pass. Formatting/workspace clippy and all app,
canvas/core and IO-unit stages pass. The full `cargo xtask ci` attempt was blocked
by Windows Application Control (4551) before its IO integration binary executed;
the normal standalone IO/platform/UI run then passed 73 tests. No system policy
was changed. This does not finish native Print or R6's richer output work.

R5 home-template slice delivered: 17 actual built-in documents now supply cached
light/dark thumbnails. A shared reusable preview card provides wrapped labels,
selection/focus feedback, disabled state and consumed Enter/Space activation;
both the structure popup and home chooser reuse it and the canvas thumbnail painter.
The fixed category header and Create/Cancel footer surround scrollable grouped
sections. Eight categories plus All filter the catalog; filter changes clear the
selection, and card selection alone never creates a document. Cancel returns to
the prior document by stable key even after tab indices shift. Closing an unrelated
tab no longer changes the active document or exits Home.

Recently used template IDs are independent of recent files, locally persisted,
deduplicated and capped at eight; unknown IDs are logged and excluded. Five original
knowledge/study/life/analysis/creative maps expand the catalog without copying XMind
assets or changing Ctrl+N/blank-map default behavior.
Every template is checked against its preview and native/XMind save/reopen results;
filters, selection-before-create, cancellation, persistence/restart, EN/DE controls,
narrow/wide bounds and reusable keyboard semantics are covered.

Validation: formatting/workspace clippy pass. All 62 main app cases, 14 app unit
cases, isolated German UI, 76 core cases and standalone IO/canvas/platform/UI
suites pass. The latest full `cargo xtask ci` attempt passed its app/core stages
but Windows Application Control blocked its IO unit-test executable (4551).
The normal standalone IO suite subsequently passed; system policy was not changed.
Live AI is explicitly disabled. The earlier full-gallery CI success below is
historical and is not a claim that this latest full invocation finished.
Section navigation and theme authoring were subsequently delivered above.
Reference-specific template expansion and richer assets remain open; R5 is not complete.

R5 structure-gallery slice delivered: the Map selector shows the current layout
thumbnail/name and a bounded, anchored scrollable popup with nine collapsible families
covering all 17 existing arrangements. One-to-three columns adapt to viewport width.
Previews use original sample maps and the same core layout/canvas renderer, cached
separately for light/dark; no copied XMind artwork or invented visual presets.
Choosing a different layout preserves content/styles/annotations and selection in
one undo step and fits the resulting view. Popup keyboard input is isolated from
underlying map/outliner edits, including Escape on the closing frame.
Outliner leaf spacers no longer expose unnamed empty buttons.

Full `cargo xtask ci` passes for the integrated gallery and previous merge/masked-copy
slices, including 60 main app cases, German UI, 76 core cases, canvas/IO/platform/UI
and CLI checks. Live AI is explicitly disabled. The earlier policy-blocked invocation
below remains historical; this later integrated invocation completed normally.
Grid-family semantics, visual skeleton variants, custom themes and home-template
gallery work remain open; no full R5/R6 completion is claimed.

R6 file-tool slice delivered: Merge Map Files parses every selected native/XMind
source before appending complete sheets in one transaction, retaining destination
metadata and leaving source files untouched. Empty/invalid inputs do not partially
merge. Global public XMind annotation IDs are namespaced by owning topic/sheet,
with reference-resolution coverage independent of private workbook metadata.
Embedded-asset merging remains dependent on R4; current images are references.

Create Masked Copy opens a new unsaved document for review and ordinary Save/Export,
without retargeting or editing the source. It replaces sheet/topic/relationship
titles and removes notes, labels, links, images, markers, boundary titles, custom
font names and metadata, retaining hierarchy, colors and task states. A localized
review dialog explains that structural information can remain identifying.
Decoded native/XMind archive tests check public and private payloads; source
immutability, cancel, dirty-copy lifecycle and native reopening are covered.

This slice passes workspace formatting/clippy, all 58 main app cases, app unit
tests, isolated German UI tests and the core/IO/canvas/platform/UI suites. The
full `cargo xtask ci` attempt stopped at a Windows Application Control block
(4551) of its German UI test executable, after the preceding app/AI tests passed.
The isolated German executable subsequently passed without changing system policy.
Live AI was explicitly disabled. This does not close the remaining R0–R12 work.

R1 Select/Arrange delivery: shared action availability now drives menus and palette,
with visible sibling/descendant/inverted selection, central-topic navigation,
natural child-title sorting and floating-topic edge/center alignment or equal-center
distribution. Arrangements use measured topic boxes (not screen coordinates), preserve
subtrees/relationships, and undo atomically. Sorting remaps contiguous annotation
ranges or rejects a split before mutation. EN/DE menu/context, exact geometry,
visibility, mixed-selection guards, save/reopen and undo regressions are covered.
Additional reference choices, custom shortcuts and native accessibility audits remain.
The integrated `cargo xtask ci` run passes for this slice, including all workspace
tests and CLI tests with live AI explicitly disabled.

Latest R1 editing slice delivered:
- Session-wide explicit topic-style Copy/Paste (Ctrl+Alt+C/V) and Reset Style,
  shared availability across menus/context/palette, atomic multi-topic undo and
  EN/DE labels. Automatic theme inheritance is not baked into the clipboard;
  text fields retain their keyboard input.
- Single-topic Indent under the previous sibling and Outdent after the parent,
  with menu/context/palette actions. Reparenting retains owned boundaries,
  relationships and recursively owned Summary trees, rejects annotation cycles
  before mutation, and cleans annotations when their last source is removed.
  No unverified XMind shortcut or multi-selection semantics are implied.
- Previous/Next Sheet wraps with Ctrl+PageUp/Down; inline topic edits commit before
  new/duplicate/delete/rename/switch/move operations. Last-sheet deletion remains
  prohibited, view state resets on switching and duplicate/move failures are surfaced.
- Full `cargo xtask ci` passes, including CLI tests that were policy-blocked in a
  previous run. Live AI is explicitly disabled for deterministic verification.
  Broader R1 command audits and window lifecycle work remain open.
- Official manual gap research added ten source-cited, nonduplicate requirements
  to the backlog, separating desktop gaps from future companion/research-only ideas.
- R1 sheet workflows now include a full-sheet, versioned OS clipboard payload
  (16 MiB limit, validated input, cross-instance and ordinary canvas Paste),
  explicit Paste Sheet, and native separate-sheet saving that protects source
  files and preserves the source document's dirty/path/history state.
  Sheet menu, tab context menu and palette share localized commands. Late or
  invalid clipboard responses cannot silently insert text topics or affect a
  different active document. Native clipboard OS delivery still needs platform QA.
- New Sheet from Topic copies a selected branch and recursively owned annotations
  in one undo step without changing the original. Internal relationships are retained;
  external ones remain in the source, with explicit notification. Rich reopening,
  root/Summary/floating extraction and undo are covered. Clipboard and separate
  save cover current model fields; future embedded assets require R4 integration.
- Sheet-workflow verification: formatting and workspace clippy pass; 54 main UI
  cases including CLI plus the German UI case, 73 core tests and the IO/platform/UI
  suites pass. The latest full-workspace CI attempt stopped when Windows Application
  Control blocked its core test executable (4551), without executing it; no policy
  bypass was attempted. The earlier style/navigation slice's full CI result above
  remains historical, not a claim that this latest full invocation completed.

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
  was `Cannot create property 'name' on string 'Thinoda'`. Corrected metadata to
  `creator: {name, version}` with `dataStructureVersion: "2"`, following the
  official xmind-generator archive format. A newly exported six-topic Unicode
  sample opens in installed Windows XMind with its hierarchy/text visible.
  Regression checks validate metadata/manifest and parse public content without
  Thinoda's private lossless payload. This verifies opening, not full visual fidelity.

- Next delivered slice: working General/Editor/AI/Cloud settings navigation with
  scrolling; independent persisted topic-toolbar and connector-add-button switches;
  initial canvas zoom including first-frame centering without unwanted auto-fit
  for non-default values; opt-in last structure/theme for blank maps without
  overriding explicitly chosen templates. Additional reference toolbar modes,
  smart guides, calendars, sharing and editable shortcuts remain open.
- Bottom controls: zoom presets 10–500%, current non-preset value and Fit; a
  current-sheet statistics popover reuses title word counts and explicitly defines
  Unicode scalar character counts. No selection/notes/labels statistics implied.
- Summary parity slice: descendant-aware source-range selection frame, brace-to-topic
  connector and split-map side handling, with independent topic editing/children.
  Range-resize handles and mixed-side sibling range rules remain unverified.
- Cross-sheet Find: optional all-sheet scope, sheet-aware navigation/filtering and
  optional notes/labels. Replace All changes titles in one workbook transaction;
  notes and labels are search-only and this is stated in the UI.
- XMind public-format fidelity: explicit topic-shape/line classes, topic font/border/
  alignment/decorations/connector styles, relationship/boundary colors and element
  classes; legacy topic styles and self-closing property elements. Public JSON
  round trips no longer rely on the private workbook payload. Themes/style references,
  fixed widths, images/to-dos and legacy structural annotations still have gaps.
  These new style mappings have format-contract coverage, not new native-XMind
  visual verification.
- User follow-ups: tracked individual relationship/boundary/Summary deletion as P1;
  added Axel Wolters' credit and contact email to About in EN/DE.
- Integrated verification: `cargo xtask ci` passes with all workspace tests,
  formatting and warning-free clippy, including 43 main app e2e cases, the isolated
  German UI case, 31 canvas tests and 19 IO tests. Earlier transient Windows
  Application Control failures did not recur in this final run; no policy bypass.
  Built the Windows x64 NSIS installer and a separate executable at
  `target\x86_64-pc-windows-msvc\release\thinoda.exe`, leaving the user's running
  `target\release\thinoda.exe` and open file dialog untouched. The running process
  remains the previous build until the user closes it and launches the new binary.
  Native macOS/Linux/ARM validation and signing/notarization credentials remain
  external prerequisites.
- Structural-object deletion delivered: a typed object selection now handles
  relationships, boundaries and Summary braces consistently. Fixed the old
  relationship Delete guard (it incorrectly required topic selection), added
  boundary/brace hit testing and highlight state, and wired named EN/DE context
  actions. Summary removal reuses model subtree cleanup, including nested owned
  annotations, while preserving covered branches; the context menu explains scope.
  Topic selection, Escape, rubber selection and sheet reset clear object selection.
- Corrected the canvas zoom cap to 500%, matching the existing preset and persisted
  initial-zoom range; exact 10%/500% interactions are regression-covered.
- Current verification: 183 workspace library tests and 42 non-CLI app e2e tests
  plus the German UI test pass; workspace clippy passes. Full CI encountered
  Windows Application Control error 4551 in the CLI version/help subprocess;
  CLI conversion passed. The earlier full green run remains a prior baseline,
  not a claim that this latest full CI run passed. No policy bypass attempted.
- Final non-CLI workspace run: 246 tests pass, including exact zoom limits,
  structural pointer/context actions and persistence. The two CLI tests were
  explicitly excluded because of the application-control blocker above, not
  marked passed. After the user closed Thinoda, refreshed the standard
  `target\release\thinoda.exe` and `target\packages\thinoda_0.1.0_x64-setup.exe`
  with this slice. Original foundation tasks are marked complete; this does not
  close the remaining R0–R12 parity/release work.

Continue R1 command-availability consistency and remaining R2 preferences/bottom controls,
alongside reference verification. Sidebar family/tab preferences are now delivered
above; Sticker/Illustration/Pitch panes are not implemented. Assets/legend/custom groups/task-marker
synchronization stay with R4/R7. No entire R0/R1/R2 package is complete.

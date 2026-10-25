_Last updated: 2026-10-25 08:04:13 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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

#### Active feature phase plan - user-directed, 2026-10-24

**This is the current execution order.** The user's latest instruction supersedes
the stabilization-first ranking and feature-restart restrictions below: finish
the reported Fill/color-picker regression, then implement and test only native
OS menus, configurable keyboard mappings, task planning/Gantt, Zones/Pitch and
enhanced AI. Resume the existing security/stability backlog only after all five
feature workstreams are implemented, tested and complete. Preserve the unfinished
sprint evidence; deferral is not completion or acceptance of its risks.

Do not start additional security scans, tool-download hardening, old device-audit
campaigns, unrelated UX polish, rich-asset/cloud/updater/collaboration projects or
release preparation during this feature sequence. Existing CI can finish; normal
feature-specific testing and fixing defects introduced by these changes remain
part of delivering each feature, not a restart of the suspended sprint. This
sequencing change does not authorize permission/account changes, external data
transmission or publication with unresolved release prerequisites.

| Phase | Outcome and implementation sequence | Main components | Completion evidence / dependency |
|---|---|---|---|
| F0 - finish current fix | Repair selected-topic Fill ("Füllung") without breaking Border, Text Color or Line Color; retain custom-color selection, keyboard focus and canceled-touch behavior. Publish with this phase-plan update. | `thinoda-ui/src/widgets.rs`; app inspector integration tests | **Implementation and local verification complete:** two-row regression fails before/passes after; English/German, both themes/scales, multi-selection and Undo/Redo covered. Full workspace: 661 passed, 1 pre-existing ignored, 75 suites; native all-target/WASM clippy and 25 documentation contracts pass. Native-device acceptance is not claimed. |
| F1 - native OS menus | Reuse stable command IDs and shared availability/execution; finish the grouped menu model and native event routing. Deliver Windows shell/menu behavior, real macOS global menus and Ubuntu desktop integration, retaining the web hamburger. Implement focused-window routing and lifecycle required by the existing P4 contract. | `thinoda-app` command catalog/native host; `thinoda-platform`; reusable `thinoda-ui` shell | All enabled native entries execute the same command exactly once and target the focused document; text editing, Settings aliases, Close Window versus Quit and unsaved-work decisions work. Follow P4.1-P4.7's Windows ribbon/native-menu distinctions and per-step review. Depends on F0; no dependency on completing deferred SS0-SS8 or unrelated R0-R12 work. |
| F2 - keyboard mappings | Offer built-in Thinoda Standard and XMind-compatible mappings; preserve the current default unless a preset is explicitly chosen. Add named, user-created mappings with capture, assign/unassign, conflict/reserved-key feedback, Apply/Cancel, save/duplicate/rename/delete and reset. Resolve all command dispatch and hints from the active profile. | `thinoda-platform/src/settings.rs`; app command/shortcut catalog, conflict checks and Settings; canvas/Outline dispatch; native menu accelerators | Saved mappings survive restart and work across Map/Outline, menus, palette and help. Conflicting presets show their changed bindings before application; no silent activation of legacy inactive overrides. Keyboard/IME/text-entry/browser ownership tests pass. Depends on F1's command/menu routing; preserves existing bindings throughout migration. |
| F3 - task planning and Gantt | Define versioned task/calendar/dependency data, keeping simple To-do checkboxes compatible. Add task inspector editing and work-calendar Settings, then a separate native Gantt window with synchronized task table/timeline, date/duration/progress edits, dependencies, zoom and export/print. Provide a browser alternate view without claiming an OS window. | `thinoda-core` model/edit/calendar logic; `thinoda-io` persistence; app task/Gantt view and native viewport host; shared controls | Task edits synchronize with Map/Outline/Gantt and Undo/Redo. Working days, exceptions, leap days and dependency propagation are deterministic; invalid/cyclic dependencies are rejected visibly. Save/reopen/recovery roundtrips retain schedules; closing Gantt preserves the map. Depends on F1 window routing and F2 command bindings. |
| F4 - Zones and Pitch | Add persisted, named Zones for floating-topic groups, with create/rename/order/move/resize/delete and Undo. Then add branch/zone slide generation, frame/order editing, windowed/fullscreen Pitch, presenter workflow and explicit navigation/exit. Export the same presentation framing to PDF and ordered slide images. | `thinoda-core` Zone/presentation state; canvas framing/rendering; app Pitch inspector/player; `thinoda-io` exports | Zones stay distinct from boundaries and reject central-tree membership. Save/reopen/recovery retain membership/order/framing; cross-zone relationships render correctly. Keyboard and pointer playback/exit work; exports match frames without mutating maps. Depends on F1/F2 commands; no Gantt dependency beyond the chosen delivery order. |
| F5 - enhanced AI | Deliver bounded SSE streaming with preview/cancel and deliberate Apply; then a persistent chat panel with Quick/Balanced/Deep modes and selected-topic/sheet scope. Add rewrite, translation, prompted idea growth, reorganize/refine, editable saved prompts and model-configured semantic search/clustering. Finish AI Pitch outline suggestions against F4. | `thinoda-ai` transport/parsers/jobs; app preview/chat/prompt/search UI; existing credential/endpoint settings; core undoable batch edits | Local mock streams cover partial/malformed/error/cancel responses; canceled/stale previews never mutate maps. Apply is one undoable change and checks source identity/revision. Prompt profiles persist, embeddings invalidate on edits/model changes, unsupported model capabilities are explicit, and Pitch suggestions remain reviewable. Depends on F2; Pitch assistance also depends on F4. No live endpoint calls without explicit authorization. |
| F6 - integrated feature completion | Finish focused cross-feature testing and docs for F1-F5: native menu/keymap parity, Gantt window ownership, Zones/Pitch persistence/export and AI review/apply. Reconcile each feature's remaining acceptance, not the suspended sprint's entire matrix. | Existing feature tests, relevant native/web hosts and user documentation | All five feature rows have real implementation plus recorded applicable tests/host evidence. Unavailable native hosts remain open limitations, never counted as passes; other feature implementation can continue independently. Only after all five close may SS0-SS8 resume. No merge/release is implied. |

**Starting design contracts (verify against the cited references before schema/UI
implementation):**

- **Menus:** retain the already-approved P4 shared File/Edit/Sheet/Insert/AI/Tools/
  View/Help taxonomy, Settings in Edit with standard native aliases, Windows ribbon
  behavior, macOS global menu plus an Office-style in-app ribbon, and Ubuntu fallback. Native host actions must be real,
  not egui popups relabeled "native". Reuse the existing command IDs and preserve
  focused text editing. Add new feature commands to this model as F3-F5 ship.
- **Keymaps:** use a versioned active-profile identifier and named bindings keyed
  by stable command IDs, with platform/context-aware combinations. Thinoda Standard
  is the safe default; XMind compatibility is opt-in, not a claim that every OS or
  browser-reserved key can be rebound. Show Home/Find-Replace, Fit and other known
  conflicts explicitly. Keep inactive legacy values preserved, not automatically
  interpreted as valid bindings. Every profile operation has an unsaved-draft path;
  changing profiles updates native accelerators and visible hints together.
- **Planning:** use date-only scheduling rather than UTC-midnight timestamps,
  explicit work weekdays/dated exceptions and one consistent duration convention.
  Start with finish-to-start dependencies and visible cycle/invalid-date errors;
  document unsupported scheduling constraints rather than silently approximating.
  Existing `todo: Option<bool>` data must still load and remain a simple checkbox
  unless scheduling is explicitly added. Do not invent reference behavior for
  unlabeled calendar/Gantt toolbar controls.
- **Presentation:** preserve the guide's floating-topic-only Zone membership.
  Pitch entry is a named rebindable command; choose its default through the same
  conflict audit, not an unverified XMind F5 claim. Deliver PDF/slide-image export
  first; PPTX and AI-generated video are not implied by "Pitch".
- **AI:** build on existing local/OpenAI-compatible endpoints and credential
  handling. Response depth is request guidance, not a promised provider reasoning
  API. Semantic features require an explicitly selected embedding-capable model;
  cache by source/model and keep unsupported endpoints actionable. Selected content
  and destination are visible before sending; never send maps to external test
  services. Vision/image/video generation, web-search services, hosted accounts
  and billing are outside this text/embedding-focused phase.

**Execution rules:** finish F0 first, then F1 -> F2 -> F3 -> F4 -> F5 -> F6.
Implement complete vertical slices, not disconnected buttons or placeholders.
Persist/migrate only the data each feature needs; use existing document history,
command availability and error reporting. Test each feature while building it,
update completion status honestly, and do not interleave deferred stability work.
No staffing or calendar promises are inferred from these phases.

**F2 implementation slice (2026-10-25):** versioned persisted profiles and the
Settings > Keyboard Shortcuts > Edit mappings editor are implemented. Standard
preserves existing gestures, with the native macOS close-window correction below;
XMind-compatible is opt-in and covers supported
commands rather than claiming every XMind/system gesture. Named copies support
capture, assignment/unassignment, duplicate/rename/delete/reset and staged
Apply/Cancel. The editor reviews changed aliases in both views before applying.
All active dispatch/hint surfaces, including the AppKit menu model and offline
guide's active-shortcut table, use the profile resolver. Invalid/conflicting or
reserved assignments are reported; inactive legacy override strings stay inert.
Focused text, system clipboard events and IME composition retain ownership.
Local fixtures cover capture, save/reload, cancel, modifier precedence, Map/
Outline dispatch, native accelerator models and shipped-locale/theme/scale
geometry. F1/F2 integration shares one canonical settings/profile library across
document windows. Apply cancels stale Settings gestures, preserves live sidebar
choices, and repaints owner viewports; failed saves retain the draft and unchanged
live settings. AppKit Settings/Quit aliases no longer overwrite custom or empty
accelerators. Native macOS Cmd+W now consistently means Close Window, not both
Close Window and Close Tab; the separate tab command can be assigned explicitly.
Windows/Linux retain Ctrl+W for Close Tab. System Hide/Minimize keys remain
reserved. Actual macOS/Linux/browser interaction acceptance remains open; this
slice is not a claim of F1/F2/F6 host acceptance.

**F3 model/persistence progress (2026-10-25):** the core now has a versioned,
date-only scheduling schema, work-calendar arithmetic, bounded deterministic
finish-to-start propagation, task/dependency validation and undoable document
edit helpers. Native `.thinoda` version 2 persists the data; legacy version 1
and simple To-do checkboxes remain compatible. Copy, duplicate, move and delete
preserve scheduled task data without dangling dependency IDs. App integration
and IO export/print progress follow.

**F3 IO export progress (2026-10-25):** `thinoda-io` now exposes bounded Gantt
SVG, PNG, vector PDF, print-document and UTF-8 CSV rendering from a validated
sheet schedule. Rows retain unscheduled tasks and use stable topic-ID ordering;
chart bounds cover all computed task dates. Raster/PDF and print placement reuse
existing IO helpers. This prerequisite is now connected to the app surfaces below;
full native/browser interaction acceptance remains open.

**F3 app integration (2026-10-25):** Insert > Task Planning and the Style
inspector expose explicit Apply/Cancel task drafts with ISO start/due dates,
inclusive working-day duration, progress, optional priority and finish-to-start
predecessors. Settings and Tools expose the current sheet's Work Calendar,
including weekday rules and dated exceptions. Apply validates captured
document/sheet/revision identity and makes one undoable transaction; invalid
or stale drafts stay open with an error. Removing scheduling leaves To-do intact.

View > Gantt Chart opens an auxiliary native viewport, or an in-app browser
window, over the same document. It includes task/date rows, dependency connectors,
timeline labels/zoom, staged bar move/resize and progress edits, Undo/Redo,
SVG/PNG/PDF/CSV export and the existing print-preview/submission pipeline using
Gantt output rather than map output. Pointer edits cancel on Escape, focus loss,
unsafe touch or revision/zoom changes. Closing Gantt preserves the map; any
already-open print preview returns to the root window. The interactive chart
explicitly limits 1,000 rows and 100,000 logical timeline units; export has its
own documented IO limits. Exports capture a snapshot, include all schedule rows,
and report rendering/write/delivery failures.

Local headless coverage includes atomic task/calendar/dependency edits,
Undo, To-do compatibility, stale/cyclic/invalid drafts, small-window Cancel,
IME candidate Escape, real multi-frame bar movement/cancellation, auxiliary
close targeting, print-source capture and all four output formats. Native
Windows/macOS/Linux visual interaction and browser download/print acceptance
remain F6 host gates, not passes inferred from these tests.
Local verification: 129 app unit tests, 50 related integration tests, 172 core/IO
tests and 25 documentation contracts pass; scoped native all-target and WASM
app-library strict clippy pass. The F3 implementation can proceed into F4
without treating the outstanding host gates as completed.

**F1 implementation progress (2026-10-24):** the compact main menus now use a
shared File/Edit/Sheet/Insert/AI/Tools/View/Help command tree. A first Windows
HMENU adapter is wired to the root eframe window; it queues native selections
for application-frame dispatch, preserves the producing snapshot, rejects
stale document/sheet/history targets and modal execution, and sends standard
editing actions to focused text controls instead of the map. Native menu
failure is reported and leaves the compact menu available. Hidden Win32-window
fixtures cover dispatch, disabled IDs, replacement, invalid input and destruction;
app fixtures cover menu parity, stale/modal ownership and text selection.
Local evidence: 77 app unit tests, four hidden-window native menu fixtures and
36 existing menu/shortcut/context/touch integration tests passed; native and
WASM clippy and 25 documentation contracts passed. The desktop executable was
built and launched at the user's request; its live Windows accessibility tree
exposed all eight native menu groups. This does not establish screen-reader,
ARM64 or other-OS acceptance.
This is a Windows integration slice, **not F1/P4 completion**: ribbon/OS-menu
acceptance, multiple windows, Ubuntu desktop acceptance, command/window
lifecycle and the broader P4 review/host matrix remain pending. F2-F6 remain
pending; SS0-SS8 remains deferred.

**F1 ribbon progress (2026-10-24):** Windows now draws the shared eight category
tabs with semantic command groups, scrollable command overflow and a persisted
collapse preference. Ctrl+F1 toggles collapse; Alt+1 through Alt+8 chooses a tab;
collapsed tabs open temporarily, with command/Escape/outside dismissal.
Short windows use a bounded Ribbon menu. Contextual floating controls and their
existing visibility settings remain unchanged. Text-edit command clicks retain
their producing field and dispatch after pointer release. Focus/scroll testing
found and fixed floating tab-scrollbar interception and text Undo losing focus.
Six ribbon behavior/parity fixtures and a separate 99-case shipped-language/Auto,
theme and scale geometry matrix cover these paths. The latest desktop executable
was built and launched locally; its live Windows accessibility tree exposed the
eight category tabs, collapse control, semantic groups and disabled file actions.
This is an implemented
ribbon slice, not completed P4.3 acceptance: native keyboard/AT, high-contrast
and Windows ARM64 acceptance still need evidence. The other F1 OS/window work
and F2-F6 remain open, and deferred stability work has not resumed.

**Windows single-menu correction:** at the user's request, the Windows desktop
now activates the ribbon directly, without attaching the native HMENU bar or
showing the redundant hamburger. Short windows retain the ribbon's bounded menu
fallback. macOS still supplies its native application menu alongside the ribbon;
web/Linux keep the compact menu. Earlier HMENU attachment evidence above is
historical, not the current Windows presentation.
Ribbon explanations are hover tooltips rather than visible controls and do not
consume layout/overflow slots. The user's subsequent Microsoft Ribbon design guide
supersedes the intermediate uniform-icon layout: primary/labeled/compact controls
now use 32/24/16-point icons, with a large primary action and two-row secondary
columns, labeled footers and 44-point minimum targets. One combined tooltip per command prevents duplicate
label/hint popups; style help is associated with the style commands, not the
following sheet command.

**Ribbon design-guide and map-capsule follow-up (2026-10-24):** Home/Capture task
tabs and saved Quick Access Toolbar customization follow the
[Microsoft Ribbon introduction](https://learn.microsoft.com/en-us/windows/win32/windowsribbon/windowsribbon-introduction);
this remains an egui ribbon, not Windows COM/XML Ribbon Framework integration.
Every command family has an explicit semantic icon mapping, including distinct
Zoom In/Out/Reset/Fit, sheet operations and all five AI actions. Primary and
secondary icons use different light/dark semantic colors; enabled icon contrast
is checked against surfaces and composited selection backgrounds. Submenu arrows
are sized and vertically centered against their captions; the expansion toggle
shares the task-tab centerline. Document capsules now contain their own close
button, with New Tab directly after the final capsule in the same scroll viewport.
Fixed navigation arrows, keyboard focus reveal and shared unsaved-close decisions
remain. Tests cover actual painted sibling-icon shapes, contrast, caption/chevron
alignment, capsule geometry, inactive close, canceled touch and unsaved confirmation.
PR #70 has merged; these refinements are follow-up changes, not edits to that PR.
Real macOS/VoiceOver, multi-window and wider F1 host acceptance remain open.

**Preview follow-up (2026-10-25):** document scroll rows reserve the capsule's
full height and extra vertical space, keeping the lower outline visible. A
translated Pin/Unpin Ribbon control exposes the existing saved expansion mode:
pinned ribbons remain open after commands; unpinned previews dismiss after use.
Regression coverage checks unclipped capsule borders, saved pin state and both
post-command behaviors. This is release preparation, not completion of F1-F6.

**Optional compact ribbon (2026-10-25):** Settings > Editor adds a saved
Compact Ribbon switch, default off with unchanged standard layout. Opting in
reduces the expanded panel from 200 to 160 logical points, uses 72-point primary
buttons, 34-point secondary/tab targets, smaller icons, tighter spacing and
recalculated widths/overflow. It does not remove tabs, commands, tooltips or group
labels, change Outline/QAT density, or alter pinning/command behavior. Coverage
includes all eight tabs, both densities, translated/theme/scale combinations,
rendered fixtures and real settings toggling/save/reload.

**F1 Linux in-window menu slice (2026-10-25):** roomy native Linux windows expose
the shared File/Edit/Sheet/Insert/AI/Tools/View/Help menu categories directly in
the top bar (at least 1400 logical points at the selected UI scale) and route
them through the same command availability and execution path. Narrow windows
keep the compact hamburger; the browser is unchanged.
Headless coverage exercises category visibility, Settings in Edit and the narrow
fallback. This is an app-owned eframe menu row, not a GNOME global-menu exporter
or Ubuntu/Orca host acceptance. P4.2's remaining menu/accessibility audit,
P4.5's DEB/AppImage/Wayland/X11 and Orca acceptance, F1 multi-window lifecycle
and non-Windows native execution remain open.

**F1 window-close intent slice (2026-10-25):** OS window close now stays distinct
from explicit Quit through the shared unsaved-work prompt and Save All continuation.
Cancel retains dirty documents; Save waits for all documents to save before closing;
Discard closes only after the accepted decision. The macOS Window menu now offers
Close Window (Cmd+W) separately from application-menu Quit (Cmd+Q), both using the
same app-owned close checks. Focused lifecycle/Save All tests and the host-independent
menu model pass; the AppKit callback fixture and live macOS behavior still require
a macOS host, and multi-window/recovery ownership remains open.

**F1 document-window ownership slice:** `Move to New Window` now assigns the
active document to an eframe secondary viewport without cloning its live model;
document tabs are shown only in their owning viewport. Closing a child viewport
commits pending inspector edits and returns its documents to the root window,
preserving dirty state. The AppKit menu snapshot now records its viewport; only
the focused viewport synchronizes/polls the global menu, and deferred commands
reselect the captured document before dispatch while retaining the prior root
selection. Headless regressions cover ownership, state preservation, fallback
rendering and document-targeted menu dispatch. This does not establish native
macOS viewport/menu behavior: per-window close/save decisions, root/last-window
lifecycle, recovery ownership, native multi-window execution and VoiceOver/
multi-monitor acceptance remain open. No single-instance IPC or open/reopen
forwarding change is included.

**F1 Close Window command parity follow-up:** Close Window has the stable
`window.close` identity and routes through the existing viewport-close lifecycle,
not Quit. Native macOS defaults Command+W to Close Window without also assigning
it to Close Tab; Windows/Linux retain Control+W for Close Tab. AppKit's Window
menu aliases the shared command and its resolved accelerator rather than adding
a fixed `performClose:` binding. Host-independent regressions cover the menu
model, platform defaults, root unsaved-work intent and child document return.
The AppKit callback fixture also covers a reassigned Close Window accelerator;
executing that fixture and live macOS behavior still require a macOS host.

**Screenshot-directed ribbon refinement (2026-10-24):** the main ribbon now
uses original vector line icons, large primary actions, stacked secondary actions,
bottom group captions and an active-tab underline. Width-driven adaptation removes
secondary captions, then collapses groups; remaining groups use a bounded overflow
menu. The Outline structural toolbar shown in the user's second screenshot shares
these controls, adapting between captioned tiles, labeled rows and compact icons.
Its Insert/Edit/Arrange/Delete groups retain all nine actions, accessible names,
tooltips, keyboard activation and Undo; the Structure/Map sidebar is unchanged.
The latest Windows build was launched locally, with the native menus and new
controls visible in its live accessibility tree. The user is actively editing in
that app; further work must preserve that window and its unsaved map.

The macOS adapter now has a real AppKit global menu alongside the ribbon:
Thinoda/About/Settings/Services/Hide/Quit and a standard Window menu, shared
category commands and typed key equivalents. The existing native bridge owns
tracking, pending events and generations; Close/Quit still use app decisions.
Platform model tests run on Windows; a dedicated macOS-only fixture compiles for
Intel/ARM and exercises real NSMenu callbacks/lifecycle in CI. **Implementation is
not native-host acceptance:** macOS build/fixture results and UI/VoiceOver evidence
must be recorded separately. The earlier initial-ribbon workspace snapshot passed
676 tests (one pre-existing ignored); a later native submenu-state correction had
its own targeted app/backend checks. Do not represent that earlier run as covering
the screenshot refinement or the AppKit adapter.

**Ribbon follow-up evidence:** GPU fixtures render six width/theme combinations
and assert aligned group footers and light/dark ribbon surface colors. The
compact Outline regression checks unclipped vector icons and complete accessible
names, not captions intentionally hidden at that width. Eight ribbon unit tests,
five native-menu tests and five integration tests (including the 99-case layout
matrix) pass on Windows after the final sizing corrections; native all-target
clippy with GPU tests and WASM clippy also pass. The preceding full workspace
run failed only its obsolete compact-caption assertion; its corrected regression
passes separately, not as a claimed fresh full-suite pass.
CI run `40100971593` passed real macOS Intel/ARM SDK bridge checks, NSMenu
callback/lifecycle fixtures and native clippy, then exposed pre-frame shortcut
formatting in four app snapshot tests. macOS snapshots now use typed key
equivalents without initializing font-dependent display hints; a fresh Mac-context
regression passes locally. The corrected macOS CI run and actual UI/VoiceOver
acceptance remain outstanding. These source refinements do not replace or restart
the user's running Windows app.

#### Active priorities by end-user value (2026-10-24)

**Historical ranking, superseded by the active feature phase plan above.**
This ranks remaining work, not whole packages as unfinished. Reuse the delivered
slices below; close each task with observable behavior before moving on.
Data-loss, security, launch and release regressions override the ranking.
The security and stability sprint below now owns the first delivery gate;
ranks 2-8 resume only after its acceptance criteria are met.

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

### Security and stability sprint - draft, 2026-10-24

**Deferred by user instruction on 2026-10-24.** Keep this finite backlog and its
evidence, but do not execute further SS0-SS8 work until F1-F5 and integrated
feature acceptance above are complete. The earlier stabilization-first
instructions below are historical and no longer determine implementation order.

**User-directed sequencing change:** consolidate the remaining safety work into
one stabilization sprint, then resume the value-ranked improvements above.
This section supersedes the open-ended sequence of small touch-control slices:
finish a finite current-product safety checklist before adding more UX features.
It does not mark the wider R0-R13 roadmap complete.

**Timebox:** ten working days from approval, not ten calendar days or a promise
that unavailable hardware/accounts can be replaced by automated tests. The day
ranges below are work budgets, not additional staffing or permission to launch
parallel agents. Confirm native test hosts, security-tool access and signing
capacity on day 1. If capacity is insufficient, change the timebox or supported
release scope explicitly; do not silently remove acceptance cases.

**Scope contract:** close all confirmed security/data-loss/stability defects in
the currently implemented native/web product, including defects found by the
sprint's defined audit. Reverify delivered protections rather than rebuilding
them. Unimplemented password protection, OAuth/cloud APIs, an auto-updater,
collaboration and plugin execution remain R9-R13 features with their own safety
gates; they are not existing vulnerabilities or prerequisites for completing
this sprint. Do not expose those features before those gates pass.

#### Baseline and known gaps

- No open GitHub issues were returned on 2026-10-24. PR
  [#70](https://github.com/AxelWol/Thinoda/pull/70) is open; current checks are
  still running. Earlier run
  [40071626605](https://github.com/AxelWol/Thinoda/actions/runs/40071626605)
  failed only its Ubuntu ARM64 job: `crash_recovery_restores_unsaved_work`
  discovered zero orphan sessions instead of one. Root cause is unconfirmed;
  do not dismiss it as a flaky test or weaken the recovery assertion.
- The in-progress numeric-draft fix binds provisional Style values to document,
  sheet, selection and revision, and cancels on hidden/disabled/modal contexts.
  Its targeted interruption test passes locally; it is not yet merged or
  released. Finish this work before extending the helper.
- Remaining gesture gaps include standard menu/toolbar/dialog commands,
  theme-editor numeric/color drags and other property controls; a per-frame
  draft restore is not rollback of earlier frames in a canceled drag.
- Save/picker/close, real native/browser IME and assistive-technology acceptance,
  browser storage/zoom and native installation acceptance remain partly open.
  Existing headless tests and earlier release packaging do not close these gates.
- GitHub's alert APIs report Dependabot disabled (403), secret scanning disabled
  (404), and no code-scanning analysis (404). Alert coverage is therefore
  **unavailable**, not zero findings. Tool-access messages also request broader
  token scopes; do not change credentials/permissions or enable services without
  authorization. Use existing permitted local/CI scanners as applicable and
  record tool/database versions and coverage limits.

#### Sprint backlog and acceptance

These are audit/remediation work packages, not a list of already-confirmed
vulnerabilities. Record each finding with its affected revision/surface, minimal
reproduction, impact/severity, owner, fix, regression and verification evidence.
Handle exploitable findings privately under [SECURITY.md](../SECURITY.md).

| ID / budget | Work and relevant components | Done evidence |
|---|---|---|
| SS0 / day 1 | Freeze the current-product checklist from this plan, BACKLOG, WEB, MINDMANAGER and open GitHub work. Inventory dependencies, credential/network/file trust boundaries and every state-changing input surface. Establish security-review, native QA and release access; verify private vulnerability reporting and branch protection/required checks, obtaining owner approval for any configuration changes. | Every known gap maps to SS1-SS7, with a named executor and available verification host/tool. Reporting and merge protections are evidenced, or an explicit owner decision remains open. A missing prerequisite is a visible blocker, not a pass. New findings enter this same checklist and can displace lower-impact work. |
| SS1 / days 1-3 | Fix recovery/CI reliability first: reproduce the ARM64 orphan-discovery failure, inspect snapshot persistence, session-lock release, parallel/subprocess interactions and cleanup. Exercise actual crash/restart, two live sessions, restore/discard, failed writes and orphan leases in `thinoda-platform`/`thinoda-app`. | Root cause and failing regression established; latest ARM64 run and the complete required CI matrix pass at the candidate SHA. Repeat the previously failing recovery case at least 50 times on ARM64, both alone and under concurrent test load, without lost snapshots or weakened assertions. |
| SS2 / days 2-4 | Close save/lifecycle data-loss risks: atomic save/backup errors, stale sources and symlink aliases, focused Notes/Hyperlink/title/IME drafts, Save All continuation, picker cancel, Close/Quit during startup or background work, and sheet/document switching. | Fault-injection plus native picker/close tests preserve source bytes, unsaved drafts, other documents and recovery on cancel/error; successful edits have correct one-step Undo/Redo. No silent overwrite, false success or accidental clean-state transition. |
| SS3 / days 3-5 | Finish the input-safety inventory as one bounded pass: canvas/Outline, sheet/document tabs, main/context menus, quick actions, Style/Map/Symbols/custom-theme controls, Settings, dialogs and popups. Share cancellation/ownership logic; bind all staged values to their actual target and lifecycle. | Each state-changing control family has canceled one-contact, two/three-contact, added/removed-finger, delayed-release, hide/modal/focus-loss and stale-target cases where applicable. Rejected gestures leave model/history or staged settings unchanged; clean touch/mouse/pen/keyboard and text composition still work. No unsupported gestures or disabled controls that merely mask a defect. |
| SS4 / days 4-6 | Review current import/export/clipboard/link/asset boundaries in `thinoda-io`, `thinoda-core`, platform and native printing bridges. Verify archive/XML/depth/count/image allocation limits, malformed IDs/references, traversal/symlink handling, external-resource behavior and source-preserving conversion consent. | Boundary and over-limit fixtures fail visibly without panic, unbounded allocation, unexpected fetch/launch or source mutation. Existing fuzz targets run with retained failure artifacts; at least a 30-minute bounded campaign per existing target completes on the candidate, in addition to CI smoke. Review native bridge lifetime/ownership and error/cancel paths without submitting physical print jobs. |
| SS5 / days 4-6 | Review shipped dependency/secret exposure, AI endpoint/request handling, keychain/session-storage policy, legacy migration, logging, response limits, cancellation/timeouts and stale/background result application. Audit network behavior with mocks; no user documents or credentials sent to external test services. | Security review plus dependency/secret-scan evidence at the candidate SHA; confirmed exploitable findings remediated and retested. Locked/unavailable keychains never fall back to plaintext; logs/files/exports exclude secrets; failed/canceled/stale AI results cannot edit a different map or leave partial mutations. List inaccessible scanners explicitly. New streaming/prompt-library features remain R9 work. |
| SS6 / days 5-7 | Verify web persistence/recovery across reload, tab contention, storage quota/write errors and PWA updates/offline use. Verify download/save semantics and browser zoom ownership. Audit CI/release permissions, untrusted PR handling, dependency/tool downloads, credential cleanup, package integrity/signatures and install/update compatibility of the shipped product. | Browser execution, not only metadata tests or WASM compilation; errors are visible and old/new clients do not silently lose maps. Package downloads match checksums; available platform signature/notarization checks pass. No release/secret exposure to untrusted PR code; installs/reinstalls preserve user files/settings. Adding a new updater or signing account is not part of this gate. |
| SS7 / days 7-8 | Run independent security review and focused behavioral QA of recovery/lifecycle, untrusted inputs, native bridges and changed input surfaces. Exercise repeated open/edit/undo/save/recover/close cycles and large-map responsiveness against existing benchmarks; execute the supported native/browser matrix below with temporary profiles/maps. | Findings have reproducible evidence; fixes are independently retested. Zero unresolved confirmed security vulnerabilities, data-loss defects, crashes or required-check failures. No unbounded resource growth, blocking UI regressions or lost edits during the exercised cycles. CPU layout timing is not GPU frame-rate evidence. Tests cannot be skipped or relabeled to make the exit green. |
| SS8 / days 9-10 | Reserve two days for findings/retests, freeze a release candidate, merge only after green checks, build all six native targets plus the website and perform post-release checks. Reconcile PLAN/BACKLOG/WEB/security limitations. | The exact merged/tagged SHA passes the gate below. Publication/download/site checks succeed; runtime/install acceptance is recorded separately. No change after sign-off without rerunning affected checks. Resume feature work only after sign-off. |

**Dependency order:** SS0 precedes the audit baseline. SS1 is the first repair
priority and blocks any release; SS2-SS6 findings can be investigated alongside
it where capacity permits. SS3's draft helper depends on SS2 ownership/commit
contracts. SS7 depends on fixes and evidence from SS1-SS6; SS8 depends on SS7.
Every newly discovered exploitable or data-loss defect interrupts lower-impact
work. Day 6 is the scope checkpoint: escalate unresolved roots/access blockers
then, not after the release is promised.

#### Execution evidence - 2026-10-24

- **SS1 in progress (implementation: current coding session):** reproduced a
  deterministic recovery-lock lifetime defect. Holding a duplicated descriptor,
  then dropping `fs::Lock`, left `try_acquire` returning `WouldBlock`. Native
  locks now explicitly unlock before closing; a forked child's inherited open
  description can no longer prolong the parent's released recovery lock.
  The regression failed before the change and passes afterward; it also checks
  that closing an old duplicate does not unlock a new owner's session. This
  establishes the lock defect, not retrospective proof of every CI failure.
- Added a real child-process exit without Rust destructors after snapshot
  persistence. The parent must load/discard that orphan, skip a live session,
  and honor a cloned orphan-document lease. The existing app restore test now
  exercises 50 crash/restore cycles per invocation, including normal concurrent
  workspace execution. Ubuntu ARM64 CI additionally runs those cycles alone.
- Windows x64 validation: 196 tests across platform/app unit tests and app
  e2e/inspector-save suites passed; all-target platform/app clippy and workflow
  contract tests passed. The exact-candidate ARM64/native/WASM CI results remain
  required before SS1 can close. Local Windows success is not ARM64 evidence.
- **SS1 ownership follow-through:** three further failing regressions established
  that a suspended manager could write a claimed orphan, normal drop could
  delete that orphan, and failed suspension released the live lock despite
  unpersisted snapshots. Suspension now flushes and releases under one state
  lock, retaining ownership on failure. Flush/remove/crash-flush reject suspended
  ownership; explicit cleanup reacquires the lock before deleting anything.
  Queued work survives until successful resume/flush. All three regressions pass;
  the combined targeted native run passes 199 tests and native/WASM clippy pass.
  Complete affected platform/app suites subsequently pass 314 tests with one
  pre-existing ignored test across 56 suites. Browser event/device execution
  remains separate acceptance work.
- A follow-up app regression showed that failed snapshot cleanup advanced the
  document's recovery watermark, suppressing retry after resume. The watermark
  now advances only on successful cleanup, with failures logged; queue failures
  are also logged without dropping the retry. That regression failed before the
  change. The final targeted run passes 200 tests; native/WASM lint remains green.
- **SS2 in progress (save/conflict protection):** reproduced a missing-baseline
  overwrite bypass: clearing an existing document's fingerprint allowed Save to
  replace newer external bytes without confirmation. Metadata-query errors were
  also treated as unchanged. Both cases now log the uncertainty and require
  conflict resolution; autosave cannot write an unverifiable file. Regressions
  cover unavailable metadata, missing baselines, Save/Save All cancel and explicit
  Overwrite, plus autosave after its actual idle threshold. Model/history and
  external/later-document bytes remain unchanged when writing is rejected.
- Reproduced single-save overwrite consent surviving a failed write (batch saves
  already revoked it). Failed saves now revoke that consent; canceling standalone
  MindManager conversion consent revokes it too. A subsequent attempt must ask
  again rather than silently overwrite a newer external version. Regressions
  exercise native and MindManager saves, failed writes, conversion cancellation
  and retry. The affected app unit/save/inspector/title/MindManager run passes
  88 tests; native all-target and WASM lint pass. Native picker/close, source-alias
  and broader fault-injection acceptance remain open; SS2 is not complete.
- **SS2 close/save continuation:** reproduced Close > Save leaving the tab open
  after conflict resolution, and a stale close prompt saving/discarding the tab
  that later occupied the same index. `CloseIntent::Tab` now stores the document
  key, resolving its current index only at the action boundary. A key-scoped
  pending close survives conflict and MindManager consent, closes only after
  successful saving with no remaining dirty work, and clears on cancellation,
  missing targets or write failure. The old conversion-only continuation is
  replaced rather than duplicated.
- Added regressions for native/MindManager conflict-to-close, target reordering
  between dialogs, conflict cancellation, stale Save/Don't Save prompts, and
  conversion cancel/failure followed by a later successful save that must not
  revive the canceled close. The initial two regressions failed before the fix.
  The affected app unit and seven integration suites pass 172 tests; all-target
  native and WASM lint pass. Real native picker execution and exact-head CI
  remain separate gates.
- **SS2 destination identity and backup faults:** reproduced a Save As conflict
  bypass through `nested/../map.thinoda`: literal-path comparison allowed newer
  external bytes to be replaced without conflict resolution. Save destinations
  now use the shared source-preservation identity helper; non-NotFound identity
  errors fail visibly rather than being treated as a different file. Sheet-save
  source checks reuse that helper instead of duplicating it. Regressions cover
  parent aliases, Windows case aliases, Unix file/directory symlinks (CI),
  invalid identity checks, preserved workbook/history and separate-copy success.
- Added a backup-write fault using a non-replaceable backup destination while
  Notes is focused, in Map and Outline. Original/backup blocker bytes survive,
  failed saves remain dirty with an error, recovery contains the notes, successful
  retry preserves the original backup, and one Undo removes the notes. The
  affected app unit/save/lifecycle/interop suites pass 161 tests on Windows x64;
  native/WASM lint pass for the implementation. Unix symlink execution remains
  candidate-CI evidence, not a Windows runtime claim.
- **SS1 CI milestone:** run
  [40074194593](https://github.com/AxelWol/Thinoda/actions/runs/40074194593)
  at `6cce6cb` completed successfully across all native jobs, WASM and fuzz smoke,
  including the additional isolated 50-cycle Ubuntu ARM64 recovery step. This
  verifies the first explicit-unlock patch only; later ownership/lifecycle
  revisions and final-candidate review/acceptance still gate SS1 closure.
- **SS3 custom-theme gesture lifecycle:** reproduced canceled numeric dragging
  leaving Font Size at 38 rather than its pre-drag 30. The editor now keeps a
  pointer-sequence snapshot of the full staged theme/role, restores it across
  all frames of cancellation or multitouch, and latches rejection through
  pointer release. Window-focus loss and skipped editor frames cancel the
  sequence too; repeated egui layout passes share its frame identity. No rejected
  pointer sequence can Apply, save to the library or dismiss the editor through
  a synthetic outside click.
- Actual-app numeric and color-picker channel regressions cover canceled and
  two/three-contact drags, clean touch/mouse dragging, focus loss, retention of
  prior keyboard edits, Save to Library/Apply agreement, unchanged workbook while
  staging and single-step Undo after Apply in both themes. The initial numeric
  rollback test failed before the fix; the focus-loss case caught an additional
  event-frame gap before correction. All 97 affected unit/theme/input/lifecycle
  tests pass, with native all-target and WASM lint green. Free-form color-plane
  gestures, remaining control families and real devices still need SS3 acceptance.
- **SS3 shared confirmation lifecycle:** reproduced a canceled single-contact
  release activating Save and closing a dirty document. Shared confirmation
  dialogs now gate all three button kinds and backdrop dismissal with the
  app-wide touch latch. Actual-app regressions in both themes reject canceled,
  two- and three-contact choices for Save/Don't Save/Cancel and external-file
  Overwrite/Save As Copy/Cancel. They preserve model/history, dirty work, pending
  decisions and disk bytes, without granting overwrite authorization. Close
  tests additionally reject backdrop releases and verify subsequent clean
  keyboard, mouse and touch choices; conflict tests verify deliberate touch
  overwrite still saves the complete local workbook. All 104 affected app
  unit/input/save/theme/lifecycle tests and 21 shared-UI unit tests pass, with
  native all-target and locked WASM lint green. This covers shared confirmation
  dialogs, not arbitrary custom modals or every standard egui control.
- **SS3 library controls:** reproduced a rejected gallery release opening a
  quick-style modal and a canceled Save persisting its draft. Theme-library
  edit/remove launchers, quick-style save/remove launchers and the quick-style
  removal menu action now share the touch latch. The custom quick-style name
  dialog guards Save/Enter, Cancel and backdrop dismissal too. Both-theme
  actual-app tests cover canceled and two/three-contact gallery releases,
  subsequent deliberate opening, unchanged map/history and library counts.
  Name-dialog tests retain earlier keyboard text and unchanged persisted
  settings through rejected Save/Cancel/backdrop releases, then verify mouse,
  Enter and clean-touch saves agree with reloaded settings. All 108 affected
  unit/input/quick-style/theme/lifecycle tests pass; native all-target and locked
  WASM lint pass. Context-menu invocation with raw secondary-pointer input was
  not established by the gallery fixture, so that path has no new automated
  acceptance claim. Other custom modals and real-device acceptance remain open.
- **SS1/SS3 recovery decisions and failure retention:** reproduced a canceled
  Restore consuming its recovery session; Restore/Restore All/Discard All now
  share the touch latch. Both-theme actual-app tests reject canceled and
  two/three-contact actions while verifying the snapshots still load, then
  exercise intentional single-contact restore/discard. Inspection also exposed
  failed loads followed by unconditional session deletion and failed discards
  silently removed from the UI. Failing regressions established both defects.
  Restore now preloads/validates the whole selected session before mutation;
  failure retains all its snapshots and opens no partial maps. Explicit discard
  retains failed documents for retry while successful deletions leave the list.
  Fault fixtures repair the blocked/corrupt path and verify successful retry,
  original workbook contents and dirty restored maps. Errors are logged and
  displayed, including post-restore cleanup errors. All 168 affected app
  unit/e2e/input/save/theme/library tests pass, with native all-target and locked
  WASM lint green. Subsequent handoff coverage is recorded below; final
  ARM64/candidate and native/device execution remain required.
- **SS1 durable restore handoff:** reproduced an immediate second recovery scan
  finding zero snapshots after restoration, before any drawing, housekeeping or
  panic-hook flush. Restored maps now get synchronous replacement snapshots
  before opening/old-session deletion. Write or suspended-ownership failure keeps
  originals claimed in the dialog, opens no partial session, reports the error
  and attempts cleanup of replacement keys. The platform handoff writer does not
  queue abandoned writes; queued unrelated work remains untouched. When the
  recovery service is unavailable, maps can open with a warning but original
  snapshots stay on disk. Regressions verify immediate ownership-release/restart,
  unavailable service, two-map failed destination/suspension retry without extra
  snapshots, and handoff replacement of a stale queued snapshot. All 67 platform
  unit tests and 165 affected app unit/e2e/input/save/inspector tests pass; native
  all-target and locked WASM lint pass. A crash partway through a multi-map
  transfer or failed rollback/old cleanup may leave extra copies rather than
  delete the last copy; this is not a deduplication or final-device acceptance
  claim. No full SS1 or sprint sign-off yet.
- **SS2/SS3 sheet-rename ownership:** reproduced canceled confirmation applying a
  rename and a switched document receiving another map's pending name. Rename
  drafts now capture document key, active sheet index and revision. Changed or
  missing owners dismiss with a warning; rejected OK/Cancel releases leave the
  name draft, workbook and history unchanged. Rename failures are surfaced.
  Both-theme actual-app cases cover canceled/two/three-contact decisions followed
  by intentional touch confirmation/cancel and Undo. Stale-target cases cover
  document switching/removal, revision changes and active-sheet switching.
  All 103 affected unit/input/title/IME/sheet-strip/context-menu/shortcut tests
  pass; native all-target and locked WASM lint pass. Remaining custom dialogs,
  state-changing controls and real-device acceptance are still SS3 work.
- **SS3 print-preview decisions:** reproduced a canceled touch starting print
  output preparation. Print/PDF-save, preview retry, Close and backdrop dismissal
  now share the touch latch. The print harness explicitly uses 60 Hz input steps:
  its previous default cadence failed to reproduce the same canceled click.
  Both-theme actual-app coverage rejects canceled/two/three-contact output and
  dismissal choices, preserves workbook/history and verifies later keyboard
  closing. Existing deliberate mouse preparation and cancel/error retry cases
  still pass without opening an OS dialog or submitting a physical job. All 102
  affected unit/input/shortcut/title tests pass; native all-target and locked
  WASM lint pass.
  Print page-setup coverage follows below; screenshot-editor controls and
  real native/browser dialog execution remain separate SS3/SS6 acceptance work.
- **SS3 print page setup:** page settings now retain a full pointer-sequence
  snapshot, with cancellation latched across multitouch, focus loss and skipped
  frames through release. Both-theme multi-frame margin tests verify earlier
  drag changes roll back on cancel/added finger/focus loss and remain on clean
  release. Rejected orientation/background/dark/grayscale choices leave staged
  settings unchanged; output decisions use the same gesture ownership. All 11
  print tests pass. Numeric keyboard/popup and real-device coverage remain part
  of final acceptance rather than a claim of comprehensive print-device testing.
- **SS4/SS5/SS6 independent security review:** reviewed shipped input/asset,
  credential/AI, native bridge and release boundaries. One high-confidence
  HIGH finding concerns arbitrary `v*` tag builds receiving signing secrets;
  no other confirmed exploitable findings were reported. This is not complete
  scanner or device coverage. Read-only GitHub ruleset enumeration returned only
  the active main-branch ruleset, with no active tag ruleset. No settings changed.
  Owner-controlled release-tag/workflow/signing protections remain an unresolved
  security gate: an in-workflow check alone can be removed by a writer who can
  replace that workflow.
- **SS6 release defense in depth:** a secret-free preflight checks out main,
  resolves an exact tag/main ref, rejects missing/noncommit/unmerged refs and
  emits its immutable commit SHA. All package jobs depend on it and use that SHA
  rather than re-resolving a mutable tag. Publication rechecks the tag against
  the packaged SHA. Manual feature-branch artifact runs are intentionally
  rejected; the preflight script must land on main before the updated workflow
  is used. Local git fixtures cover lightweight/annotated tags, main ancestry,
  moved/unmerged/missing/malformed refs and unavailable main. Workflow contracts
  cover preflight dependency and SHA wiring. This mitigates mistaken/untrusted
  source selection while the workflow is intact; it does not close the owner
  configuration finding or eliminate a later concurrent tag mutation.
- **SS3 screenshot editor:** output/cancel decisions and staged crop/square/frame/
  color settings now retain a pointer-sequence snapshot. Rejected touches,
  focus loss and skipped frames roll back through release; rejected output
  cannot copy an image or invoke PNG save. Tests cover both-theme canceled and
  two/three-contact controls, multi-frame canceled/focus-interrupted crop
  rollback, clean crop/square editing and dismissal. All 105 affected unit/input/
  shortcut/title tests pass; native all-target and locked WASM lint pass.
- **SS4 fuzz host probe:** installed nightly/cargo-fuzz could build importer
  dependencies but Windows linking failed because
  `clang_rt.asan_dynamic_runtime_thunk-x86_64.lib` is missing. A no-sanitizer
  alternative was denied; sanitizer coverage was not weakened. No campaign
  executed. The probe exposed an out-of-date separate fuzz lockfile (workspace
  versions/dependencies); it was synchronized by Cargo. Long sanitized campaigns
  remain an explicit Linux/compatible-host gate, not satisfied by CI smoke.
- **SS4 compatible-host alternative:** added Linux acceptance campaigns for each
  existing importer target with default sanitizers and 1,800-second budgets,
  plus the existing memory/input/time limits. PR fuzz/workflow changes trigger
  the matrix; manual dispatch is available after merge. Logs, corpus and failure
  reproducers upload even after failure and expire after 14 days. The workflow
  has read-only permissions, no signing secrets, and cancels superseded runs.
  Contract tests verify the three-target budget, limits and evidence retention.
  Adding this workflow is not campaign completion; only successful full-duration
  runs at the candidate qualify, and later code changes require reassessment.
- **SS3 label popup:** guarded add/remove/suggestion/sort/apply/cancel actions
  and preserved the popup through rejected outside releases. Both-theme tests
  cover canceled/two/three-contact sequences with retained input, staged sort,
  workbook/history and subsequent deliberate Apply/Undo. Existing IME protection
  remains ahead of popup input handling.
- **SS7 focused independent QA:** at `7d5a96b`, the QA agent executed 31 focused
  Windows headless recovery/handoff/discard/rename/print/screenshot checks without
  reproduced defects. This does not satisfy the cross-platform/device/browser
  matrix or the unresolved security gate. A complete local native workspace run
  at that revision passed 638 tests with 1 pre-existing ignored across 79 suites.
  The separate PWA Node suite passed all 7 contracts.
- **SS6 browser execution blocker:** Playwright and Chrome DevTools MCP navigation
  both failed during catalog verification with request timeouts. No browser
  acceptance was executed and no browser was installed. The published site and
  metadata are not substitutes for candidate runtime/persistence testing.
- **Latest native candidate evidence:** complete locked workspace tests at
  `e233bbf` passed 639 tests with 1 pre-existing ignored test across 79 suites,
  with live AI requests disabled. CI run
  [40080887181](https://github.com/AxelWol/Thinoda/actions/runs/40080887181)
  remains in progress: WASM, fuzz smoke, notices and documentation jobs passed
  at the latest read; native jobs were still running. Earlier candidates
  `fee4171` and `f85951d` completed the full matrix successfully.
- **SS5 audit-tool access blocker:** `cargo audit` is not installed. Installing
  it was denied because specific authorization is required. No alternate
  executable was installed or scanner coverage claimed. This joins disabled
  repository scanners, owner-controlled signing/tag protections, long sanitized
  fuzz execution and browser/device acceptance as unresolved release gates.
- **SS2 export consent ownership:** symbol-color and MindManager warnings now
  bind to the source document key, active sheet and revision after title-edit
  commit. A source switch/removal/revision/sheet change revokes pending export
  and color-loss authorization before a picker can open. Tab reordering that
  preserves the source still allows the two independent consent steps without
  switching to the document now at its old vector index. Actual-app regressions
  cover both warning types and their chained reordered-tab path; existing
  conversion, source-preservation, title and symbol tests remain green.
- **SS1 ownership CI milestone:** run
  [40075054509](https://github.com/AxelWol/Thinoda/actions/runs/40075054509)
  at `093ba19` also completed successfully across all native jobs, WASM and fuzz
  smoke, including Ubuntu ARM64 isolated recovery cycles. This includes the
  suspension/cleanup retry fixes, but does not replace final-candidate acceptance.
- **SS0 prerequisites remain open:** no available native macOS/ARM64/mobile
  device acceptance evidence has been supplied; security scanner coverage and
  any necessary owner authorization remain open. Read-only API verification
  reports private vulnerability reporting disabled. Main's active ruleset
  prevents deletion and non-fast-forward pushes, but its effective rules do not
  require CI checks or pull-request reviews; the classic protection endpoint
  returns 404. This is not an entirely unprotected branch, nor enforcement of
  the sprint's review/CI gate. Repository-owner approval is required before
  changing those settings. Do not mark the sprint or release ready.
- **SS3 Settings and batched screenshot input:** Settings now keeps pointer
  edits outside the live/persisted profile until clean release. Cancel,
  multitouch, focus loss, hidden/disabled windows, skipped frames and intervening
  profile changes reject staged edits; newer profile changes remain intact.
  Rejected tab/shortcut, credential save/clear, connection-test, folder-picker
  and web-install activation cannot trigger those actions. API-key text remains
  independently staged. Existing language/keyboard behavior remains supported;
  settings-folder launch failures are now visible instead of ignored.
  Both-theme actual-app cases exercise rejected toggles/actions and multi-frame
  slider cancel/multitouch/focus/hide/modal/profile-change paths, plus deliberate
  release/persistence. They do not execute native pickers or live AI requests.
  A separate one-frame screenshot press/cancel fixture fails against the original
  rollback and passes with a pre-controls fallback when no gesture snapshot exists.
- **SS5 partial dependency evidence:** GitHub advisory queries for 22 selected
  locked parser/image/TLS/credential/UI package versions returned no matching
  advisories. This is neither a full-lockfile audit nor a secret scan; the
  audit/scanner access gates above remain unresolved.
- **Candidate evidence before Settings follow-through:** the complete native
  workspace run after source-bound export consent and batched screenshot rollback
  passed 644 tests with 1 pre-existing ignored test across 74 suites, with live AI
  disabled. This precedes the Settings changes and is not their full-suite result.
- **Settings candidate validation:** the complete native workspace now passes
  646 tests with 1 pre-existing ignored test across 74 suites, with live AI
  disabled. Native all-target and WASM clippy pass; 25 documentation metadata
  contracts also pass.
- **SS4 sanitized campaign milestone:** run
  [40082010663](https://github.com/AxelWol/Thinoda/actions/runs/40082010663)
  at `75ed5d9` completed all three default-sanitized Linux campaigns successfully.
  Each log records 1,801 seconds: FreeMind 6,482,358 executions, XMind 6,005,049
  and OPML 4,246,033. All three log/corpus/reproducer artifacts uploaded
  successfully with 14-day retention. No crashes were reported in these bounded
  runs; this is not proof of absence of importer defects. The subsequent
  Settings/screenshot changes do not modify importer sources, fuzz targets or
  dependencies; exact final-candidate acceptance remains required at release.
- **CI milestone:** run
  [40082010570](https://github.com/AxelWol/Thinoda/actions/runs/40082010570)
  at `75ed5d9` passed the full configured native matrix, WASM, fuzz smoke, notices
  and Markdown checks. This precedes the Settings/screenshot follow-through.
- **SS3 remaining auxiliary-dialog implementation pass:** shortcut-reference
  filters/Close/backdrop, Guide filters/Close/backdrop, legal-document selection,
  copy/links/Close, About links/legal navigation and masked-copy decisions now
  use the shared cancellation latch. Typed searches and text selection remain
  independent. Actual-app tests cover canceled and two/three-contact actions
  in both themes, retained decisions/model/history and deliberate follow-through.
  Masked-copy consent additionally binds to document/sheet/revision; changed or
  removed sources dismiss it visibly instead of previewing another source.
  This closes those implementation gaps, not the remaining native/device input
  acceptance gate.
- **SS5 AI result ownership:** a local-mock regression failed without the new
  source guard because a delayed response mutated a locally edited document.
  Requests now bind to document key, sheet/root and revision after title commit.
  Changed/closed sources, active title/Notes/Hyperlink drafts and disabled AI
  reject late results visibly without mutation or history entries. An unchanged
  background document still receives its own result with one-step Undo without
  activating another tab. Connection-test jobs and model lists are invalidated
  when endpoint/credential draft changes, but not by model selection.
  Generate rejects canceled/multitouch input and keeps its prompt; a stable
  editor protects IME candidate Escape before global shortcuts, then restores
  deliberate dismissal. New messages are translated in all supported catalogs.
  Tests use local mock endpoints or no-network worker closures, not live providers.
- **Latest local candidate validation:** complete native workspace tests pass
  655 tests with 1 pre-existing ignored test across 74 suites, with live AI
  disabled. Native all-target and WASM clippy pass; the prior metadata run passed
  all 25 documentation contracts. This includes the auxiliary-dialog, masked-copy
  ownership, AI source/draft/disable, connection-list and prompt IME changes.
- **Settings candidate CI/acceptance milestone:** both
  [40084433631](https://github.com/AxelWol/Thinoda/actions/runs/40084433631)
  (full configured CI) and
  [40084433630](https://github.com/AxelWol/Thinoda/actions/runs/40084433630)
  (all three sanitized campaigns) passed at `02153c8`. Each campaign ran
  1,801 seconds: FreeMind 7,582,132 executions, OPML 4,123,832 and XMind
  6,169,806. All three evidence artifacts uploaded successfully. These runs were
  allowed to finish before publishing further application-only changes; they
  do not sign off the newer AI/auxiliary-dialog candidate.
- **Exact AI/auxiliary candidate milestone:** full CI
  [40086807314](https://github.com/AxelWol/Thinoda/actions/runs/40086807314)
  and all three default-sanitized campaigns
  [40086807315](https://github.com/AxelWol/Thinoda/actions/runs/40086807315)
  passed at `8fbad47`. Each campaign ran 1,801 seconds: FreeMind 6,576,733
  executions, XMind 6,891,062 and OPML 4,481,874; all evidence artifacts uploaded.
  An attempted native Chrome fallback could launch but safe address-bar delivery
  returned `no_viable_candidate`; perception then failed closed without a
  verified URL. The CI-built artifact served HTTP 200 locally, but no candidate
  page was navigated or tested. The temporary server was stopped. Browser/PWA
  runtime acceptance remains open; artifact serving is not browser execution.
  Private-reporting enablement was proposed but the user was unavailable; no
  approval or permission change was inferred.
- **SS3 palette and Find/Replace:** regressions failed against unguarded command
  execution and Replace All: canceled input closed/executed the palette and
  replaced topic titles. Shared cancellation guards now retain palette/query,
  search/replacement drafts, options/scope, navigation selection and model/history.
  Both-theme cases cover cancel and two/three-contact releases, backdrop retention,
  deliberate command execution and replacement Undo. Stable palette input protects
  IME candidate Enter/Tab/Escape including the final composition frame.
  Targeted app touch/palette/search suites pass 31 tests; shared-UI units pass 21.
  Native all-target and WASM clippy pass. The complete local native workspace
  subsequently passes 658 tests with 1 pre-existing ignored across 74 suites;
  live AI is disabled. All 25 documentation metadata contracts pass.
- **Exact palette/search candidate CI:** full configured CI
  [40090360397](https://github.com/AxelWol/Thinoda/actions/runs/40090360397)
  passed at `ed07409`. This includes the configured native matrix, isolated
  ARM64 recovery repetition, packaging/printing bridge fixtures, WASM build/lint,
  fuzz smoke and documentation/notices checks. Browser/device acceptance and
  owner-enforced release restrictions remain separate gates.
- **Exact palette/search candidate importer acceptance:** all three
  default-sanitized campaigns in
  [40090360402](https://github.com/AxelWol/Thinoda/actions/runs/40090360402)
  passed at `ed07409`. Each ran 1,801 seconds: XMind 7,256,836 executions,
  FreeMind 6,969,922 and OPML 4,283,971. All three log/corpus/reproducer evidence
  artifacts uploaded successfully with 14-day retention. No crash was reported
  in these bounded campaigns; this is not proof of defect absence or the
  remaining real-host acceptance.
- **SS5 complete locked Cargo advisory lookup:** GitHub Advisory Database checks
  cover all 568 registry package/version pairs in the workspace lockfile at
  `ed07409`, plus all four additional fuzz-lockfile pairs (`arbitrary` 1.5.0,
  `libc` 0.2.191, `libfuzzer-sys` 0.4.13 and `zip` 9.0.5). All 572 distinct pairs
  returned no matching advisories, in 20 successful requests bounded to the
  service's 30-dependency limit. This includes target-specific and development
  dependencies, not only the earlier 22-package selection. Workspace lockfile
  SHA-256: `f0512adba78f429533010a59a94184ddc09efd3a22617655438b69c1bf853185`;
  fuzz lockfile: `11c1e0d7cecde75b16b250b598cb01433f6ae8cae14c24432e17e19ae6cfdbb4`.
  Only public package metadata was submitted; no scanner installation or
  permission change was needed. The service did not report its database revision.
  This is a complete lookup of these locked registry versions against that
  service, not RustSec unsound/unmaintained-package coverage, secret scanning,
  source/provenance verification, GitHub Actions/toolchain auditing or a claim
  that dependencies are defect-free. Those remaining gates are not waived.
- **SS6 CI download integrity:** replaced the web job's download-to-privileged-tar
  pipeline with an installer pinned to Trunk 0.21.14 and the upstream archive's
  SHA-256, independently checked against the downloaded archive bytes. The
  installer verifies before archive parsing, limits download/binary sizes,
  accepts only the single regular `trunk` member and installs in the runner's
  temporary tools directory without `sudo`. Offline fixtures reject bad hashes,
  traversal/absolute paths, symlink/hardlink members, oversized data, malformed
  archives, non-HTTPS final URLs and network failures. Existing binaries survive
  checksum rejection. The real archive verifies and extracts the expected
  30,533,520-byte Linux ELF without executing it on Windows. CI fuzz smoke now
  pins cargo-fuzz 0.13.2, matching the longer acceptance campaign. This does not
  close owner-enforced release authorization, upstream provenance, mutable
  action/toolchain refs or Trunk's additional tool-download review.

#### Bounded SS3 inventory reconciliation

**User-reported priority regression: selected-topic Fill ("Füllung") locked.**
Added to SS3 ahead of further hardening/feature work. A failing shared-widget
fixture established that adjacent palettes reused IDs for matching colors;
later palettes captured the earlier Fill controls. Swatches now combine their
individual layout UI's unique ID with the color, rather than the inherited
scope ID. Fill, Border, Text Color and Line Color remain independent, including
when a custom swatch disappears. Actual-app coverage exercises English/German,
both themes, 1x/1.5x/2x scale, single/multiple selected topics, canceled/two/three-
contact releases, visible resolved fill, unchanged other color fields and
one-step Undo/Redo. The original two-row fixture fails before the fix and passes
after it. German UI checks run in a separate integration-test executable because
the application locale is process-wide; they must not race English-only tests.
The complete touch suite and isolated German suite pass 31 tests together;
the full workspace then passes 661 tests with 1 pre-existing ignored across 75
suites. Native all-target and WASM clippy and 25 documentation contracts pass.
Real-device/browser delivery remains deferred with the existing separate gate.

This is the implementation inventory, not a substitute for the SS7 real-host
acceptance matrix. Newly reproduced defects interrupt this list; broadening it
to new product features is not required to close the current sprint.

| Existing control family | Implemented safety evidence | Remaining acceptance |
|---|---|---|
| Canvas/Outline, sheet/document strips, main/context menus | Shared cancellation/ownership, stale-target and delayed-release regressions recorded above | Device-delivered touch/pen/trackpad and platform/AT matrix |
| Style/Map/Symbols, theme and quick-style libraries | Target-bound edits, full pointer rollback, guarded commands and retained drafts | Free-form color-plane and actual secondary-pointer popup/device execution |
| Settings and credential/connection controls | Provisional profile, stale-profile rollback, rejected side-effect tests in both themes | Native folder/install UI and device input delivery |
| Recovery, save/close/conflict, rename/export consent | Stable ownership, durable handoff, error/retry, canceled decisions and source-preservation fixtures | Actual OS picker/quit/crash runtime and remaining supported hosts |
| Print preview and screenshot editor | Full staged rollback, guarded output/decisions, same-frame cancellation | OS print/picker/clipboard/GPU and physical-device runtime; no print submission |
| Labels, shortcuts, Guide, legal/About and masked preview | Guarded decisions/links/filter state, independent text drafts, source-bound preview | Native browser-launch/clipboard and AT/device behavior |
| Command palette and Find/Replace | Canceled command/replacement regressions, retained query/scope/history, palette IME and clean execution/Undo | Real IME/AT and device-delivered gestures |
| AI Generate and delayed results | Guarded prompt/IME, source revision/drafts, endpoint/key ownership, disable/re-enable and background Undo | Supported browser/native runtime against local mock; no external documents or credentials |

At the latest read, the repository returned no open GitHub issues. This does not
close unfinished work in PLAN/BACKLOG or waive owner/security/runtime gates.

#### Completion gate - no partial-success closure

1. All SS0-SS8 acceptance cases have recorded evidence against the candidate
   revision, and all confirmed security/stability defects are fixed and retested.
   Mitigations or unavailable tests remain open unless the user explicitly
   changes the scope. A clean scan alone is not proof of security.
2. `cargo xtask ci`, importer fuzz smoke/campaigns, native bridge/packaging tests,
   web Node tests, WASM lint/build and all required GitHub checks pass.
   Repeated recovery testing includes the previously failing Ubuntu ARM64 host.
   No broad retries, sleep-only fixes, ignored regressions or assertion removal.
3. Native execution covers Windows x64/ARM64, macOS Intel/Apple Silicon and
   Ubuntu x86_64/ARM64, distinguishing Wayland/X11 input acceptance. Browser
   execution covers desktop Chromium and Safari where applicable, iOS/iPadOS
   Safari/PWA and Android Chrome/PWA for the shipped touch/recovery paths.
   Compile/package checks do not count as device execution. Missing hosts,
   assistive technologies or signing access block the affected claim; arrange
   user/QA evidence or explicitly revise release scope before closure.
4. Shared UI regressions cover both themes, Auto/system behavior, supported
   locale/scale/viewport matrices and keyboard alternatives. Native IME/AT
   evidence is separate; JA/zh-CN/zh-TW remain planned rather than falsely added
   to the shipped-locale claim. User data and external service access require
   explicit authorization; use synthetic fixtures and isolated profiles.
5. Independent review/QA sign-off, recovery/save compatibility, all six target
   artifacts/checksums, relevant signatures/notarization and the live website
   are verified. If any gate is blocked at day 10, the sprint is **not finished**:
   report the exact blocker and obtain a timebox/scope decision. Do not move to
   feature work with hidden safety debt.

#### Feature restart after the sprint

**Superseded:** the active feature phase plan above now runs before completing
this suspended sprint. The following order is retained only as historical context.

Return to the existing value ranking without another indefinite hardening loop.
New defects retain the normal stop-the-line rule; unrelated polish does not
reopen this sprint.

1. **Adaptable commands (R1-R2):** versioned shortcut capture/apply/reset and
   persistence, visible conflict/reserved-key handling and keyboard navigation.
   Resolve the Fit collision before activating overrides; retain existing
   bindings and do not silently migrate legacy inactive values.
2. **Trustworthy outputs and richer maps (R3-R6):** finish output/print fidelity
   and supported sheet exchange, then versioned embedded assets, usable
   image/attachment/equation/audio workflows and searchable asset panels.
   Preserve original/licensed content, undo and roundtrip acceptance.
3. **Presentation and planning (R7-R8):** zones/Pitch and presentation export,
   then deterministic task/calendar scheduling and Gantt.
4. **Optional intelligent/connected work (R9-R10):** streaming AI preview/cancel,
   then reviewed protected-file and cloud conflict contracts before connecting
   accounts or sharing. These new attack surfaces require their own reviews.
5. **Broader reach (R12-R13):** remaining locales, sandboxed extension contracts
   and collaboration after storage/security prerequisites; OS-native menus,
   ribbon and multi-window lifecycle follow the already agreed P4 gates.

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
priority rating. Track it as **R13** in the delivery order. The latest
user-directed feature sequence promotes this work to F1, directly after the
Fill/color-picker fix; the previous requirement to finish R0-R12 first is
superseded. Reuse delivered R1/R2 command/menu work. R13 does not depend on
completing the deferred sprint, CRDTs or plugins.

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
The initial same-process viewport mechanism is now selected and implemented with
egui/eframe `show_viewport_immediate`; this does not replace the P4.1 audit of
single-instance IPC, startup/open/reopen routing and recovery ownership. Those
host and lifecycle contracts remain prerequisites for P4.4 acceptance.

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

Finish and publish the Fill/color-picker fix (F0), then execute the active
feature phase plan: native menus, saved configurable keymaps, planning/Gantt,
Zones/Pitch and enhanced AI. The security/stability sprint and gesture checklist
below are deferred evidence/inventory, not work to resume before those features
are implemented, tested and complete.

**User-directed workstream - native/web touch and trackpad gestures:**
deliver consistent direct manipulation on Windows x64/ARM64, macOS Intel/Apple
Silicon and supported Ubuntu desktops, plus WASM/PWA on desktop and iOS/iPadOS/
Android browsers. Native mobile packages are not existing deployment targets.
The compact sheet-strip implementation below includes touch scrolling through
the shared scroll area rather than competing gesture handlers.

1. Audit egui/eframe/winit and browser input delivery. Regress and fix two-finger
   pan plus simultaneous centroid-anchored pinch zoom, zoom limits, gesture
   ownership and cancellation. Adding/removing fingers must not jump the map,
   reparent topics, select objects or activate controls on release. Gestures
   begun outside the canvas, disabled UI or dialogs must not manipulate the map.
2. Verify tap selection/activation, double-tap editing, one-finger background
   navigation versus intentional topic movement, long-press context actions,
   touch scrolling in sheets/Outline/menus/inspectors/dialogs, and trackpad
   two-finger scroll/pinch. Preserve mouse/pen/keyboard behavior, text selection,
   visible alternatives and undoable edits. Test transitions/cancellation before
   promising inertia, long-press drag, or other platform-specific recognizers.
3. Respect platform/system ownership: Windows press-and-hold secondary actions;
   Apple standard tap/drag/pinch and OS text-edit/multifinger/edge gestures;
   Ubuntu Wayland/X11 backend differences; mobile browser navigation/zoom and
   pointer cancellation. Do not map canvas rotation or three/four-finger system
   gestures to destructive commands. Audit web touch-action/viewport zoom and
   accessibility rather than globally disabling page zoom or browser gestures.
4. Add deterministic raw touch, synthesized pointer and trackpad-event tests
   across scale/theme/layout states: simultaneous pan/pinch, limits, transitions,
   cancellation, overlays, unchanged model/history, and normal inputs afterward.
   Add browser touch smoke coverage and a real-device acceptance matrix for
   Windows touch/precision trackpad, macOS trackpad, Ubuntu Wayland/X11 touch/
   touchpad, iPhone/iPad Safari/PWA and Android Chrome/PWA. Distinguish automated
   event tests from hardware acceptance; document every unavailable host.

Platform research references (2026-10-23):
[Apple gestures](https://developer.apple.com/design/human-interface-guidelines/gestures),
[Windows touch interactions](https://learn.microsoft.com/en-us/windows/apps/develop/input/touch-interactions),
[Android drag/scale](https://developer.android.com/develop/ui/views/touch-and-input/gestures/scale),
[libinput gestures](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html),
and [web touch-action](https://developer.mozilla.org/en-US/docs/Web/CSS/touch-action).
Apple recommends familiar gestures, non-gesture alternatives and avoiding system
conflicts; Android stresses stable pointer identity across finger transitions.
libinput touchpad recognition is not touchscreen recognition. Browser gesture
ownership/cancellation and low-vision page zoom require separate acceptance.

**Delivered first gesture slice:** reproduced wrong first-finger pinch anchoring,
missing simultaneous pan, and canceled touch topic drops mutating the map.
The shared canvas now pans one-finger empty-background drags, pans and pinches
around the two-finger centroid simultaneously, and accepts combined trackpad
scroll/zoom. Multitouch cancels pending topic/relationship drags and blocks
synthetic click/drop handling until all contacts lift. Canceled touches discard
pending drops. Ownership is limited to gestures started over an enabled canvas;
disabled/interrupted or three-finger sequences stay canceled until release.
Raw-touch/synthesized-pointer regressions cover transform math, limits, transitions,
cancel, unchanged model/history and normal clicks afterward. Existing canvas,
title-lifecycle and bottom-navigation regressions and native/WASM lint pass.
**Delivered context-gesture safety slice:** deterministic taps select topics and
double taps start title editing without changing history; single-finger holds
open context actions without a pending topic drag. Reproduced an egui long-touch
secondary click opening the host context menu during a two-finger hold despite
the canvas rejecting that action. `CanvasOutput::context_menu` now applies the
same ownership/cancellation gate as canvas editing. Two/three-finger holds stay
suppressed through contact removal until release; ordinary secondary clicks work
afterward. Shared-canvas and actual app-menu regressions cover unchanged content/
history and release without edits. No custom long-press timer was introduced.
This is not full gesture parity: real browser/device acceptance, all-surface
touch controls and browser viewport zoom remain open.

**Delivered browser-zoom metadata correction:** the mobile viewport no longer
sets `maximum-scale=1` or `user-scalable=no`, so browser-controlled zoom is not
disabled by page metadata. A web regression guards against restoring those
restrictions. **Gesture ownership decision:** a pinch begun on the full-screen
canvas belongs to map pan/zoom; page zoom remains available through browser
controls and keyboard shortcuts, while the app's visible zoom controls remain an
alternative. Eframe still prevents browser defaults for canvas touch events so
the map gesture works. Browser/device acceptance must verify browser zoom
controls/keyboard and map pinch separately without sacrificing low-vision access.

**Delivered Style-inspector touch-release safety slice:** ordinary Style controls
that commit combo-box selections, branch-side changes, reset/marker actions and
relationship endpoint choices now consult the shared canceled/multitouch release
guard. An actual-app regression verifies canceled one-contact and two/three-
contact Shape selections leave workbook/history unchanged, while ordinary mouse
selection still applies the shape. Style width sliders now keep pointer-down
changes provisional and commit them once on release; canceled or multitouch
sequences discard the draft. Tests cover canceled and two/three-contact drags,
clean touch commit/Undo, Font Size numeric drags and ordinary mouse dragging.
Numeric drafts are scoped to document identity, sheet index/root, the complete
selection and document revision. Changing any of these, disabling the control
or skipping inspector frames cancels the draft through release. Regressions
cover identical documents, duplicated sheets with shared topic IDs, selection
changes, intervening model edits and hiding/reopening the inspector, without
creating history entries or applying the draft to a different target. Opening
Settings or another app-blocking modal also cancels provisional numeric values.
The Map inspector also rejects canceled/multitouch theme-switch releases and
does not commit staged theme changes for rejected sequences; ordinary switch
activation remains available. The custom-theme editor now snapshots staged
role/theme values for the whole pointer sequence, restoring earlier drag frames
after cancel, multitouch, focus loss or skipped editor frames. Numeric/color
channel tests verify the restored draft reaches library saves and Apply; clean
touch/mouse editing remains available. Apply, Save and Cancel controls require
a clean release. Free-form color-plane/device gestures, other inspector families
and combo boxes, text-selection behavior,
browser/device gestures and hardware acceptance remain open.

**Delivered Outline touch/release safety slice:** the first one-finger diagonal
swipe scrolls deep Outline content in both axes without selecting, editing or
mutating topics. Reproduced a two-finger release collapsing a branch; Outline
now suppresses title/disclosure/toolbar activation after multitouch or cancel
until raw contacts and the synthesized pointer have released. Per-frame caching
preserves the guard during repeated layout passes. A related canvas regression
showed a delayed pointer release selecting a topic after raw touch ended; canvas
ownership now survives that interval too. The locale/theme/scale/viewport matrix
covers first swipes, canceled and two/three-finger disclosure/toolbar taps, normal
click recovery, rename/Undo, keyboard reveal and document-scoped scroll restoration.
No custom Outline scroll recognizer or mouse-drag override was needed. Real
device/backend delivery, remaining menu/inspector surfaces and browser zoom
ownership remain open.

**Delivered menu/Settings touch safety slice:** first swipes scroll menus and
Settings without activating controls. Repeated export-menu swipes reach the last
format; Escape still dismisses the menu. Reproduced canceled/multitouch releases
changing Settings switches. Extracted the Outline release latch into
`widgets::touch_activation_allowed`, sampled each app frame and cached across
layout passes. Shared switches, segmented controls and color swatches now reject
canceled and two/three-contact activation, including delayed pointer releases.
Actual Settings tests cover unchanged deletion confirmation, theme and accent,
then ordinary click/Space recovery in both themes at 1x/1.5x/2x scale. Existing
Settings viewport/keyboard, Outline and widget regressions remain intact.
This is not a general input filter: standard egui buttons, sliders, combo boxes,
text selection, inspector-specific workflows, browser zoom and hardware delivery
remain part of the remaining audit.

**Delivered Symbols-inspector touch safety slice:** first swipes scroll the
Symbols inspector without applying markers. Reproduced canceled/multitouch
releases inserting symbols; insertion, removal, default-color and contrast
actions now share the activation guard. Symbol color-picker/hex changes use the
same guard before applying color edits, including context-menu instance editors.
Both-theme/1x/1.5x/2x tests cover first swipes, canceled and two/three-finger
releases with unchanged map/history, normal Space insertion, click removal,
Enter contrast improvement and Undo restoration. Existing symbol-color,
inspector focus/draft-save and native/WASM checks remain intact. This does not
complete generic sliders, combo boxes, all inspector property widgets, browser
zoom ownership or real-device acceptance.

**Delivered preview-picker touch safety slice:** reproduced a canceled touch
applying a Quick Style. Shared preview cards now reject canceled/multitouch
pointer activation while retaining explicit Enter/Space activation. Quick Style,
structure, built-in/custom theme and template previews inherit the guard;
template double-click creation also requires guarded activation. Rejected
structure-launcher taps do not toggle its popup. Both-theme/scale actual-app
coverage verifies unchanged history for Quick Style/structure/theme cards,
unchanged template choice/document count after rejected double taps, then normal
Space/Enter/click application and Undo. Existing quick-style/custom-theme,
structure-gallery and template workflow regressions remain intact. Generic
property sliders/combo boxes and other standard buttons remain under audit.

**Delivered status-control touch safety slice:** reproduced a canceled release
switching Map to Outline. Status view/zoom/Fit actions and zoom preset selections
now use the shared activation guard; standalone Zoom/Statistics popups reject
canceled and two/three-contact opening without changing other menu behavior.
Both-theme/1x/1.5x/2x app coverage checks unchanged zoom, camera offset, mode and
map history, including delayed pointer releases and Fit inside the zoom popup.
Normal single-finger zoom, click Fit, keyboard view/preset/zoom actions and
Statistics opening remain available. Standard buttons elsewhere, property
sliders/combo boxes, browser zoom ownership and real-device acceptance remain open.

**Delivered sheet-control touch safety slice:** reproduced canceled releases
creating extra sheets. The shared activation guard now protects New Sheet,
tab selection/double-click rename, scroll arrows and sheet context commands,
including duplicate, reorder and delete. Rejected holds do not open a sheet
context menu; ordinary one-finger holds still do. Both-theme/scale actual-app
coverage verifies unchanged active sheet/content/history after canceled and
two/three-finger taps/double taps, context actions and arrow taps, followed by
ordinary tab clicks, Space creation, Enter scrolling and Undo. Existing clipping/
layout/scrolling and rename input regressions remain intact. This does not yet
cover all status controls, main-menu commands or generic property interactions.

**Delivered user-directed priority-1 follow-up - compact sheet-strip navigation:**
the supplied Excel screenshot informed fixed previous/next scroll arrows outside
the scrolling names. Arrows browse by 80% of the name viewport without changing
the active sheet or map history; they disable at their respective ends, including
both disabled when names fit. New Sheet stays outside the name viewport. Explicit
region clipping prevents sheet-name paint/hit targets from crossing into arrows,
New Sheet or independently scrolling status/zoom controls. Small windows retain
fixed arrow/add slots and truncate names with full tooltips/accessibility names.
Active/focus reveal, context actions, wheel/trackpad scrolling and rename/Undo/
resize/document-switch behavior are preserved. Touch and mouse dragging use the
same scroll area; a first-touch regression exposed a missing pre-registered drag
target, fixed by enabling drag scrolling rather than adding an input recognizer.
Tests cover arrow endpoint traversal with pointer/Enter/Space, touch swipe without
selection or content changes, one-sheet end states, stable height and pinned Add.
The existing 198-case shipped-locale/Auto, theme, scale and 320/640/1000-point
viewport matrix now checks arrow/name/status separation and actual text clipping.
Native/WASM lint passes. Real-device browser/touch and AT acceptance remains in
the gesture workstream; this is not a claim of full small-device acceptance.

**User-directed follow-up - compact document-tab navigation:** the top document
strip now mirrors the Excel-inspired sheet strip with fixed previous/next arrows,
a clipped horizontally scrollable file-name viewport. The later capsule follow-up
places Close Tab inside each capsule and New Tab after the final capsule.
Browsing with arrows, mouse/touch drag or horizontal scrolling does not
switch or modify documents. Selecting, closing, opening a new tab and keyboard
focus/active-tab reveal retain their existing behavior; stable document keys keep
tab widget identity. Narrow windows prioritize document navigation and move the
other quick-action icons into the existing main menu. Automated coverage checks
arrow endpoints, inline controls and document contents/history across browsing,
selection, close/new-tab behavior and narrow layouts. Hardware/browser/AT
acceptance remains part of the wider interaction workstream.

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

**Delivered priority-1 follow-up - Rename Sheet input safety:**
reproduced candidate-confirmation Enter prematurely applying a provisional sheet
name. Rename Sheet now owns composition state and uses the shared IME guard
before other UI handles candidate keys. Candidate Enter/Tab/Escape and Shift
variants retain the draft through final CJK commit events; ordinary Enter then
applies one undoable rename. Focus is requested only on opening, so Tab reaches
OK and Cancel instead of being pulled back to the input every frame. Empty
preedit/commit dismissal restores normal navigation and Escape cancellation.
Map/Outline regressions cover these paths and unchanged documents on cancel;
real platform IME/font/assistive-technology acceptance remains open.

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

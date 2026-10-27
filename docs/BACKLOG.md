_Last updated: 2026-10-27 11:01:53 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Thinoda backlog

Prioritized ideas beyond the shipped phases. P1 = next, P2 = soon, P3 = later.
Execution order, dependencies, acceptance criteria and release gates are defined in
[PLAN.md](PLAN.md#remaining-work-delivery-plan-2026-10-16). Reference inventories below
are not lists of wholly missing features; audit and reuse existing implementations.

## Current open work-item inventory

### P1 — CLI generation providers (user request, 2026-10-26)

🟡 Partial implementation in the current worktree; see the
[CLI provider plan](PLAN.md#cli-generation-provider-requirement-2026-10-26).
Claude/Codex adapters, provider settings, separated API embeddings, localization,
manual login help and fixture coverage are implemented. Copilot generation is
blocked pending a documented non-inference status/safe MCP-and-hook-disabled
invocation; selecting it must show that limitation and never silently use the API.
Keep cross-platform installed-CLI/manual appearance acceptance open. No login,
paid probe inference or installation was performed. The implementation is included
in PR #85; this does not remove the Copilot generation blocker.

**Current inventory (2026-10-27):** [PR #85](https://github.com/AxelWol/Thinoda/pull/85)
is open for the shared dialogs, date inputs, native AI provider and live Pitch
increment; UR4 is committed on the same branch. Its first native CI failures were the
shared save-dialog test's ambiguous title query; the fix uses the Window role
and settled geometry. Merge/release/deployment remain pending, not completed.
PRs #70, #72, #74, #76 and release #77 are merged. F0-F5 implementation
is shipped in [v0.12.1](https://github.com/AxelWol/Thinoda/releases/tag/v0.12.1):
Fill, native menu/window routing, saved keyboard profiles, task/calendar/Gantt,
Zones/Pitch and enhanced AI. Main CI, all six native packaging targets and
publication succeeded; all ten downloads match their SHA-256 manifests.
macOS signing/notarization passed; Windows Authenticode and optional WinGet
publication are not delivered. The web app is deployed and actual fresh-profile
Edge startup/rendering was observed.

**F6 is partial, not closed:** recorded browser/CORS/exports and Windows
Gantt/Pitch evidence does not close real native focus/global-menu, screen-reader,
physical-print or touch-hardware acceptance. macOS/Linux GUI hosts and safe
Windows modified-key delivery remain blockers. SS0-SS8 is **deferred, not
complete**; this backlog refresh does not lift its restart gate or start a new
hardening campaign. The
[current plan](PLAN.md#current-state-and-next-value-plan-2026-10-26) is authoritative
over older dated progress entries below.

Historical baseline (2026-10-22), after incorporating main through `ba05910`:
there are no open issues. [PR #65](https://github.com/AxelWol/Thinoda/pull/65)
merged the R1 shortcut-catalog, conflict-audit and Zen/search improvements at
`679c85a`; its final head CI passed. Release preparation
[PR #63](https://github.com/AxelWol/Thinoda/pull/63) merged version 0.8.0 metadata
at `ba05910`. The release entry exists, but artifact publication was still
running at this inventory check; complete downloadable assets are not claimed.
[PR #64](https://github.com/AxelWol/Thinoda/pull/64)
merged the native CJK preference, nested symbol picker and alignment-guide slices.
Issues [#47](https://github.com/AxelWol/Thinoda/issues/47) (topic size after moving), [#48](https://github.com/AxelWol/Thinoda/issues/48) (hover pointer), and [#50](https://github.com/AxelWol/Thinoda/issues/50) (long text in graphical exports) are closed. Merged [PR #54](https://github.com/AxelWol/Thinoda/pull/54) fixes #47.

The former WIP pull requests are now merged:

| Work item | Current status | Roadmap placement |
|---|---|---|
| [#56 Linux CI recovery persistence failure](https://github.com/AxelWol/Thinoda/pull/56) | Merged at `07fec3c`; included in the current branch. | R0 regression coverage and CI reliability |
| [#57 Windows print-preview test timeout](https://github.com/AxelWol/Thinoda/pull/57) | Merged at `68c988c`; test-only wait adjustment included in the current branch. | R0 regression coverage and CI reliability; related R6 print verification |

The remaining unfinished work is the feature/quality inventory in this file and
the package acceptance plan in [PLAN.md](PLAN.md). R0 covers baseline,
reference/status verification and incomplete review states; R1-R3 cover daily
editing/navigation; R4-R6 cover rich content and file/output workflows; R7-R8
cover presentation and project planning; R9-R10 cover AI and protected/connected
files; R11 covers distribution; R12 covers extensibility, collaboration and
the accepted additional locales. R13 / Phase P4 adds future OS-integrated menus
and window lifecycle under "R12 and above." No issue is omitted from the priority plan.

## Next-value backlog - XMind audit (2026-10-26)

These are scoped work items, not new GitHub issues. **XV1-XV3 and UR6 (Material
Symbols icons) shipped in v0.13.0; UR1/UR2/UR5 shipped in v0.14.0.
UR3/UR7 and UR4 are implemented, release pending; XV4-XV9 remain open.** XV1 adds shared Find
facets and read-only all-notes results with all shipped translations and
light/dark/System coverage; see PLAN for exact tests/browser evidence and host
limits. XV2 adds derived leaf-To-Do counts, parent completion/reopen and matching
Find status without changing scheduled Tasks or the native file schema.
Priority is end-user frequency, value and enabling dependencies, rather
than a count of XMind features. On 2026-10-26 the user authorized starting this
ordered queue with localization/theme/platform coverage and ongoing UX research.

**XV4 is in progress:** sheet-owned bounded resources, native v3 raw-blob
storage, recovery, resource-aware clipboard and atomic copy/paste/merge/undo
are the foundation. The next local image slice now adds bounded PNG/JPEG decode,
contextual insert/replace/remove, proportional width/reset and above/below
placement, map previews and SVG/raster/PDF/print/Pitch rendering. Native bytes,
sizing and undo/clipboard are preserved. Attachments, XMind resource exchange,
drag/drop insertion and handle-based resize remain open. Editable non-native
exports and unsupported visual resources still fail explicitly rather than
silently omit content. See the [active slice](PLAN.md#xv4-active-slice-portable-resource-storage-2026-10-27).
This lifts the earlier feature-start restriction, not the remaining F6 acceptance
gaps; SS0-SS8 remain deferred. Extend the existing R packages; do not create parallel search, task,
resource or presentation systems. The
[research audit](XMIND-RESEARCH.md#12-thinoda-valuegap-audit-2026-10-26) records
code evidence and official sources.

| Order / ID | Priority / package | Gap and value | Acceptance for a complete first slice |
|---|---|---|---|
| 1 / XV1 | P1 / R6 | **Faceted navigation and View All Notes.** Implemented locally: unified marker/label/To-Do/Task/priority filters and consolidated notes review. Find action items and meeting context without editing the map. | Reuse search; OR within a facet and AND between selected facets, active-sheet/all-sheet scope, connected ancestor context and a visible Clear action. Offer a read-only note list with topic/sheet identity and jump-to-topic. Changing criteria/scope and clearing never change document history or saved collapse state; explicit result navigation retains the existing undoable ancestor expansion. Deleting results cannot retarget a different topic. Cover empty results, edits while filtered, keyboard navigation and Map/Outliner consistency. |
| 2 / XV2 | P1 / R8 | **Nested lightweight To-Do progress.** Implemented locally: derived parent counts and explicit subtree completion/reopen improve checklists without requiring dates or Gantt. | Count deepest To-Dos, traversing plain topics and excluding scheduled Tasks/intermediate double-counting. Show completed/total in Map/Outliner/Style and shared graphical output; explicit parent completion/reopen is one undoable subtree edit. Add/remove/reparent/Undo recomputes progress, old files load unchanged, and scheduled Task progress stays unchanged. Find uses derived completion. Preserve the original child checkboxes when adding/removing a parent's checkbox. |
| 3 / XV3 | P1 / R3, R6 | **Automatic structural numbering.** Despite an old Phase 1 checkmark, the current Topic model has no numbering settings/render path. Useful for procedures, agendas and study outlines. | Store numbering separately from title text; implement None/decimal, sibling application, tiered numbers and explicit restart first. Insert/reorder/reparent recomputes labels deterministically. Match Map/Outliner and declared export/print support, keep search/copy title behavior explicit, persist/recover with Undo/Redo, and disclose unsupported XMind/Pitch fields. Do not add literal prefixes to titles as a shortcut. |
| 4 / XV4 | P1 / R4, R6 | **Portable embedded images and attachments.** Bounded storage and the local PNG/JPEG authoring/rendering slice are implemented locally; attachment consent and XMind resource exchange remain open. This unlocks evidence-rich maps and later illustrations/equations. | Preserve bounded decode, stable IDs, sizing and missing/corrupt-file errors. Keep image insert/render/resize/replace/remove and explicit attachment save/open without auto-execution. Native save/recovery, clipboard, sheet copy/merge and supported XMind resources preserve bytes/remap references. Exports either include supported content or show precise losses; existing reference-only files keep loading. |
| 5 / XV5 | P2 / R6 | **DOCX and TextBundle import.** Both names occur in export support, but neither is in `Format::IMPORTABLE`. Reduce onboarding/retyping for researchers and document authors. | DOCX headings/lists and TextBundle Markdown become deterministic hierarchy with an import preview and omission report. Preserve sources; never fetch external references or execute document content. Test malformed/bounded archives, Unicode, lists/headings and cancellation. Text-only DOCX can ship independently; embedded-resource retention depends on XV4. Do not claim general Word layout fidelity or add every competing app format at once. |
| 6 / XV6 | P2 / R7 | **Presentation handoff and framing.** PDF/PNG Pitch exists; `.pptx` and explicit delivery aspect ratios are missing. Make reviewed slides usable outside Thinoda. | Reuse saved slide order/crops in aspect-ratio/fit previews without stretching or source edits. Export a valid image-based PPTX with one rendered slide per saved slide, matching PDF/PNG framing; disclose that shapes/text are not separately editable. Check generated OOXML plus opening in available PowerPoint/LibreOffice hosts; keep unavailable host checks open. Editable PPTX/transitions/video are not implied. |
| 7 / XV7 | P2 / R4 | **Rich notes and rendered equations.** Notes are plain strings; equation insertion/rendering is absent. Improve study, engineering and research workflows. | Migrate plain notes losslessly; stage a bounded formatting model/editor with lists/emphasis, searchable plain-text projection and existing save/draft safety. Add an explicit supported LaTeX subset with edit/preview/error handling and export parity. Do not execute arbitrary TeX, lose unsupported formatting silently or mistake an image path for equation support. Deliver notes and equations as separate vertical slices; reuse XV4 for packaged assets where needed. |
| 8 / XV8 | P2 / R3, R8 | **Task-aware Tree Table columns.** The TreeTable structure exists but currently shares a stacking layout; Gantt has scheduling data, not a task-column Tree Table editor. | Add title/start/due/duration/progress columns using existing validated scheduling edits, with row/topic identity, keyboard navigation and Map/Gantt synchronization. Column changes and sorting do not reorder the underlying topic hierarchy silently. Cover working-day rules, invalid edits, Undo/Redo, saved view choices and output scope. No duplicate scheduling engine. |
| 9 / XV9 | P2 / R9 | **Durable local AI conversations.** The shipped panel labels history session-only; restarting loses research continuity and there are no named per-file threads. | Add opt-in bounded app-local history and new/rename/select/delete threads keyed to stable document identity. Reopen/restart, Save As, duplicate/moved files and recovery must not mix unrelated conversations. Store no API credentials, never resend on reopen, retain explicit Send/scope review, provide retention/delete controls and keep transcripts out of normal document/export output unless explicitly included. Preserve session-only mode and test failed writes, cancellation and stale-source isolation. |

### User-requested usability items (2026-10-26)

Added by the user while XV3 was in progress. Order after XV3: the three quick wins
UR1, UR2 and UR5 first, then UR3 and UR4. UR6 (icons, requested next) is delivered
immediately with XV3, and UR7 (shared dialog frame) follows UR5 because UR1/UR3 dialogs
should adopt it. XV4-XV9 follow, unless the user changes
the order. Each item keeps the standing rules: all shipped locales, Light/Dark/System,
all native targets plus web, UX research and current documentation.

| ID | Priority / package | Request | Acceptance for a complete first slice |
|---|---|---|---|
| UR1 | P1 / R9, R11 — ✅ released (#82/#83) | **Flat AI settings page.** Remove the collapsible sections from AI settings. When AI is selected in Settings, show the full page. | Every AI control is visible on the AI page without accordion state, with no lost settings or keyboard/a11y order regressions. Scrolls on small/web viewports. |
| UR2 | P1 / R9 — ✅ released (#82/#83) | **Embedding model discovery on Test Connection.** Also list the installed text-embedding models (e.g. LM Studio `text-embedding-nomic-embed-text-v1.5`). | The existing explicit Test Connection reads `/v1/models` once and separates chat and embedding candidates by documented name heuristics. An empty or unavailable Embedding Model field is filled with the detected model; an explicit valid choice is never overwritten. Results and a "none found" state are disclosed. No background polling or extra requests. |
| UR3 | P1 / R8, R11 — implemented, release pending | **Date pickers for every date input.** For example, Gantt task start and due dates. | One shared localized calendar picker replaces bare date text entry wherever dates are edited. Typed entry stays available for keyboard users. Non-working days from the sheet work-week definition are visibly disabled for scheduling fields, and invalid ranges are rejected with the existing validation. Light/Dark contrast, touch target size, web and Undo are covered. |
| UR4 | P1 / R7 — implemented, release pending | **Live in-app presentation mode.** Present the defined Pitch slides inside the app, not only as export. | Existing full-window presenter now adds First/Last and Home/End, none/fade/slide/push/zoom transitions, optional topic-by-topic builds and smooth saved-frame Zoom. Motion is off by default with an immediate Reduce motion control on every target; automatic OS preference detection is not claimed. Choices are playback-session-only; themes, localized controls, saved crops, source revision ownership and static exports stay intact. All-language/appearance small-window interactions, transition completion, geometry, popup key ownership and touch cancellation are covered. Speaker notes, timers, persisted per-slide choreography and multi-monitor presenter view are later. |
| UR5 | P1 / R11, R13 — ✅ released (#82/#83) | **Compact ribbon by default.** Always show the Compact ribbon setting, including for the web app and its Outliner ribbon. | New and migrated settings start compact (the old default-off value cannot be told apart from an explicit choice, so it is not carried over); the enlarged ribbon is opt-in. The setting is visible on every target, applies immediately to Map and Outliner, and persists. |
| UR6 | P1 / R11, R13 — ✅ released (#81) | **One Material Symbols icon vocabulary.** Replace emoji/letter/hand-drawn icons in the floating action bar, node action menu, header, footer, ribbons, menus, Outliner and native quick-access Save/Undo/Redo/Print with Google Material Symbols (Apache-2.0). | A bundled, pinned, subset Material Symbols font (`thinoda_ui::icons`) is the only icon source; the same command uses the same icon everywhere (`command_icons.rs`), Fit Map is distinct from the Map/Outline switch, sibling commands stay distinguishable, and icons follow the Microsoft ribbon/Fluent size tiers (16 small, 20 compact/toolbar, 24, 32 large) in Light/Dark on native and web. Notices, docs and glyph-coverage tests are updated. |
| UR7 | P1 / R11, R13 — ✅ implemented | **One dialog frame for every dialog.** About, Help, Settings, Key mapping, Gantt, Print, Labels, Planning, Pitch and other dialogs share one design. | A shared dialog frame has a title bar with maximize/restore, minimize-to-hide and close buttons, a lower-right resize grip and resizable edges on all four sides. Opening sizes fit content width without clipping (bounded by the window); tall content scrolls vertically; very long content is split into tabbed pages with Material icons on the tabs (e.g. Settings: General, Editor, AI Assistant, Cloud, Keyboard Shortcuts, Work Calendar). Escape/close semantics, IME guards, small-window and touch reachability, Light/Dark and all locales are preserved; native and web behave the same. Delivered as `thinoda_ui::Dialog` (title row with icon, Minimize/Expand, Maximize/Restore and Close Window; Fluent 28 px caption buttons with 16 px glyphs; four-edge and corner resize; fit-to-window opening height with vertical scroll; modal backdrop with Esc) plus `tab_bar` icon tabs. Every app dialog uses it; Settings pages are icon tabs. |
| UR8 | P1 / R9 — ✅ implemented | **AI answers in the UI language.** Every AI feature (one-shots, transforms, scoped chat, Pitch AI, brainstorm/generate) currently leaves the reply language to the model. | All AI system prompts state the resolved UI language (Auto resolves to the OS locale) by its English name, ask the model to reply in it, and fall back to English when the model cannot. Translation keeps its explicit target language. User text, prompts and responses stay unchanged otherwise; covered by prompt unit tests for all shipped locales on native and web. |

**Not missing / do not reimplement:** 17 structures including Matrix/TreeTable,
categorized generated template previews, custom themes/quick styles, Map Shot,
masked derived copies, sheet merge, cross-sheet title replacement, named
shortcuts, Zones/Pitch PDF/PNG, Gantt and F5 text/embedding AI. Existing gaps
include richer fidelity and host acceptance, not absence of these foundations.
Keep MindManager reverse-roundtrip, remaining gesture/IME/accessibility coverage,
Windows signing and installer/runtime checks in their existing packages.
Spelling dictionaries/suggestions remain P2 under Settings; background Quick
Entry, web clipping, cloud collaboration, image/video AI, audio and decorative
asset libraries rank later because they add platform/service or resource
dependencies before improving existing offline workflows.

## P1: MindManager native file compatibility (bounded slice)

- User request: support the official Corel/Alludo MindManager format for
  load/save/import/export, not extension renaming or Markdown conversion.
- Implemented `.mmap` ZIP/Application/2003 XML reading and writing, plus `.xmmap`
  import, based on privately inspected native references and schemas. Hierarchy,
  mapped fonts/shapes/opaque colors, plain notes, labels, links, priority/progress
  and selected metadata have original regression coverage. See the explicit
  [feature/read-write matrix](MINDMANAGER.md).
- Open/Import require consent for omitted content and preserve imported sources.
  Single-sheet Save As is guarded; Export discloses current-sheet scope. CLI,
  filters, association metadata, recovery guards and ten-locale safety text are
  covered; archives are bounded and writes atomic.
- Independently XSD-validated original candidates were observed opening/rendering
  in MindManager by the user. This is not certification or full fidelity.
- Residual: native-resaved reverse roundtrip, note-body/link/metadata checks;
  rich notes/resources, advanced map features, alpha, legacy/encrypted variants
  and redistributable public schema/reference validation remain unimplemented
  or unverified. Keep original sources and use `.thinoda` for full workbooks.

## Symbol colors and contrast (implementation shipped; host acceptance open)
- Add per-instance colors to the existing Symbols insertion popover and sidebar,
  including editing existing symbols and a localized, keyboard-accessible
  right-click Remove Symbol action targeting only that instance.
- Preserve legacy appearance, native persistence, clipboard and undo/redo.
  Audit rendering, printing and interchange; document unsupported format fields
  explicitly rather than silently discarding colors.
- Show WCAG contrast against the composited topic background in light/dark mode:
  graphical symbols require 3:1, normal-size month/week text 4.5:1. Offer an
  explicit correction without automatically changing documents. Localize all
  controls and cover persistence, targeting, contrast and output with tests.
- Implementation includes native/clipboard/XMind-payload persistence, canvas and
  visual export/print rendering, per-instance editors and export-loss confirmation.
  All ten supported locales have catalog entries and light/dark UI coverage,
  alongside model/output regressions. Merged localization PR29 is integrated;
  both catalog additions are retained.
- Included in the merged release baseline; no outstanding GitHub review remains.
  Do not schedule duplicate feature implementation. Native assistive-technology
  acceptance and the full 13-locale validation matrix remain separate gates.

## User-directed priority-1 navigation follow-up

- Deliver native/web touchscreen and trackpad gesture parity in tested slices:
  two-finger pan/simultaneous pinch first, then tap/double-tap, background/topic
  drag ownership, long-press context actions and touch scrolling throughout the
  UI. Cover finger transitions/cancel, overlays, zoom limits and unchanged model/
  history during navigation. Respect OS/browser text, accessibility, edge and
  multifinger gestures; preserve keyboard/mouse/pen alternatives. Native targets
  remain desktop; iOS/iPadOS/Android apply to WASM/PWA. See PLAN's vendor research,
  backend/browser audit and real-device acceptance matrix. Synthetic events alone
  do not establish full device support.
## Recently completed
- XV1 implementation/local verification (not yet released): shared native/web
  Find facets for markers/labels/priorities and To-Do/Task completion; all-notes
  previews and sheet/topic navigation; non-destructive Clear and optional
  ancestor-preserving dimming in Map/Outliner. Notes remain read-only, and
  changing filters does not navigate or alter saved folding/history. Source/
  scope changes invalidate results; stale row indices cannot retarget topics.
  Locale/theme/focus/scope/source/cancel-touch regressions and strict native/
  WASM lint pass. Browser startup/entry was observed, not full device acceptance.
- v0.12.1 release closes F0-F5 implementation and release/deployment delivery:
  native menus/windows, named keymaps, calendars/tasks/Gantt, Zones/Pitch PDF/PNG,
  streaming/scoped AI chat, transformations, prompt profiles, semantic search/
  clustering and reviewed AI Pitch outlines. PRs #74/#76/#77 are merged.
  Linux/macOS asynchronous UI timing failures were corrected without bypassing
  assertions; all six native package gates and publish passed. F6 manual host/
  hardware acceptance remains open rather than being inferred from CI.
- User-directed R1 document navigation: top file tabs now use fixed scroll arrows,
  a clipped scrollable name viewport, and pinned Close Tab/New Tab actions.
  Arrow/drag browsing preserves the active document and its history; active/focus
  reveal, keyboard selection, closing and new-tab behavior remain intact.
- R1 status-control touch safety: canceled/two/three-contact releases cannot switch
  view, change zoom/Fit or open Zoom/Statistics. Popup presets and Fit also reject
  these releases. Both-theme/scale tests preserve camera offset and map history,
  then ordinary tap/click/keyboard recovery. Other controls and device acceptance
  remain in the open gesture workstream.
- R1 sheet-control touch safety: canceled/multitouch releases cannot create,
  select, rename, duplicate, reorder or delete sheets, scroll arrows or open a
  sheet context menu. Normal single-finger holds, tab clicks, keyboard creation/
  scrolling and Undo remain covered alongside unchanged map/history checks.
- R1 preview-picker touch safety: canceled/multitouch releases cannot apply Quick
  Styles, structure or theme previews, toggle the structure gallery or select/
  double-tap-create templates. Shared cards keep keyboard activation; actual-app
  both-theme/scale tests preserve history and verify ordinary actions/Undo.
- R1 Symbols-inspector touch safety: first swipes scroll without adding markers.
  Canceled/multitouch releases cannot insert/remove symbols or invoke default-color/
  contrast changes; color application uses the same guard. Both-theme/scale tests
  cover unchanged history, ordinary keyboard/click actions and Undo recovery.
- R1 menu/Settings touch slice: first swipes scroll, export-menu swipes reach the
  last format without executing it, and shared switches/segmented choices/color
  swatches reject canceled or multitouch releases. Extracted the Outline latch
  for consistent per-frame tracking; Settings theme/accent/deletion confirmation
  and ordinary click/keyboard recovery have both-theme/scale coverage.
- R1 Outline touch safety: first swipes scroll both axes without editing. Canceled
  and multitouch sequences cannot activate disclosures, titles or toolbar actions;
  the guard survives delayed synthesized pointer release. The shared canvas now
  also suppresses late click-through. Locale/theme/scale/viewport, ordinary click/
  keyboard/edit/Undo and document scroll-restoration regressions cover the change.
- R1 context gesture safety: shared and full-app tests cover tap selection,
  double-tap title editing and single-finger context holds. Multitouch holds no
  longer leak synthesized secondary clicks into the host context menu; two/three
  contacts stay suppressed until full release without changing map history.
  Ordinary right-click works afterward. Hardware/browser acceptance remains open.
- R1 Excel-inspired sheet browsing: fixed left/right scroll arrows, disabled end
  states and pinned New Sheet surround the clipped names. Arrows/touch/mouse drag
  browse without switching sheets or editing maps. Region clipping and the
  shipped-locale/theme/scale/small-viewport matrix cover name/status separation;
  active/focus reveal, rename/Undo, wheel access and independent status scrolling
  remain covered. Real-device touch/browser/AT acceptance remains open above.
- R1 first gesture slice: one-finger background pan, centroid-anchored simultaneous
  two-finger pan/pinch and combined trackpad scroll/zoom. Pending drags cancel on
  multitouch/canceled touch instead of applying drops. Finger transitions and
  outside/disabled ownership have deterministic tests; full platform/all-surface
  gesture parity remains in the priority-1 workstream above.
- R0/R1 Rename Sheet input safety: IME candidate keys cannot apply provisional
  names or cancel the draft. Final CJK commit, ordinary Enter/Undo, dismissal and
  normal Tab-to-OK/Cancel/Escape are covered in Map/Outline. Opening-only focus
  requests replace per-frame focus stealing; real OS/browser IME acceptance
  remains open.
- R0/R1 Labels IME safety: candidate Enter no longer adds provisional text, and
  candidate Escape no longer discards the popup draft. Labels and titles share
  the composition-key guard with separate state. Map/Outline, Shift variants,
  same-frame final CJK commits, dismissal, normal Tab/Escape and Apply/Undo are
  covered; real native/browser IME acceptance remains open.
- R1 color-swatch keyboard focus: visible focus is separate from color selection,
  disabled state reaches accessibility clients, and preset focus survives removal
  of a custom swatch. Both themes, Tab, Enter/Space and disabled controls have
  regressions; native/browser assistive-technology acceptance remains open.
- R1 Settings control focus: shared segmented tabs and toggle switches show a
  theme-text-colored keyboard-focus outline. Disabled tabs expose their actual
  accessibility state. Both themes, Enter/Space, single activation and disabled
  controls are covered; native screen-reader/browser acceptance remains open.
- R11 user-directed runtime/distribution changes: Node.js 24 is selected by
  `.node-version` for scripts and web CI; JavaScript actions (including nested
  Pages upload) use Node-24 runtimes without forcing older actions onto Node 24.
  Release dependencies enforce Intel macOS first, Windows/Linux x86 next,
  native Ubuntu ARM64 after Linux x86, and Apple Silicon last. Shared packaging
  retains strict signing/notarization and gates publication on all six targets.
  Final DMG validation now uses the existing bounded retry for confirmed
  CloudKit network loss; invalid signatures and retry exhaustion still fail.
  Workflow/Node/artifact/retry regressions are in place. PRs #66/#67 are merged;
  the authorized v0.9.0 website deployment succeeded. Native release packaging
  succeeded for all six targets, including both macOS signing/notarization
  stages. All ten published downloads match their six SHA256 manifests.
  Real Ubuntu ARM64 installation and platform runtime acceptance remain open.
- R1 toolbar keyboard focus: shared icon/captioned toolbar buttons show a visible
  focus outline in both themes and selected states. Tab/Enter/Space, disabled
  controls and keyboard Undo/Redo after focused Notes Save are covered. This is
  not a claim of full screen-reader or native keyboard acceptance.
- R0/R1 topic-title IME key safety: reproduced candidate-confirmation Enter
  prematurely closing inline editing. Map and Outline now share a per-edit
  composition guard for Enter/Tab/Escape and their Shift variants, including
  final commit/dismissal events delivered in the same frame. Outline retains
  keyboard focus during candidate navigation. Composition does not commit the
  provisional title or insert topics; after dismissal, ordinary editing keys
  resume. Regressions cover both views, final CJK text, empty preedit/commit
  cancellation, normal Tab insertion and document Undo. Actual native/browser
  IME, font rendering and assistive-technology acceptance remain open.
- R0/R1 inspector draft save safety: reproduced Ctrl/Cmd+S silently omitting
  focused Notes in both center views. Notes/Hyperlink drafts now belong to the
  document, count as unsaved work and participate in idle tracking. Save,
  Save All, command transitions and native close handling commit them without
  requiring a click elsewhere; recovery captures draft revisions without
  committing history. Separate-sheet output includes the draft without
  retargeting the source. Field buffers are document/sheet/topic-scoped,
  including duplicated sheets with shared IDs. Source conflicts and failed
  saves preserve edits; existing Hyperlink Escape cancellation and one-step
  Undo/Redo remain. Undo also handles an uncommitted draft without stale text
  returning on the next frame or Save. New UI regressions cover focused saves, insertion-key
  isolation, native close/Cancel, repeated recovery and identical documents;
  model tests cover duplicate sheets, stale targets and failed writes.
  This closes the reproduced inspector data-loss case, not the full R1
  native picker/window-manager/IME/accessibility acceptance matrix.
- Web PWA installation (R11, user-directed): web-only Settings > General >
  Install web app opens an accessible browser install panel. Chromium's one-shot
  prompt is invoked only by a real panel-button click; Safari/iOS/iPadOS and
  unsupported browsers receive manual instructions. The scoped manifest and
  generated standard, maskable and Apple touch icons cover supported desktop and
  mobile installation surfaces. An exact-build offline shell includes matching
  JS/WASM/snippets, excludes documents/AI/other sites, and waits for old windows
  to close before updating. Storage limits, backups and possible Safari storage
  isolation remain explicit. Node lifecycle/cache tests, icon safe-zone/build
  generation tests and actual isolated Edge WASM startup/offline reload under a
  deployment subpath and browser installability checks pass. Full workspace CI
  with live AI disabled and WASM lint pass. Actual OS installation/icon/assistive-technology
  acceptance on Windows/Linux/macOS/iOS/iPadOS remains open; no store distribution
  or P4 native-menu/window parity is claimed.
- R1 Outline Redo specificity: Ctrl/Cmd+Shift+Z now invokes Redo before the broad
  Ctrl/Cmd+Z Undo matcher can consume it, matching Map behavior. Ctrl/Cmd+Y and its
  primary menu hint remain; the reference lists both Redo aliases. Actual app
  frames cover Undo/Redo on three simulated host modifier models; Outline
  coverage preserves sidebar focus, ordinary Undo, collapsed-row navigation and
  document contents. Fit's separate Ctrl/Cmd+Shift+H collision remains unremapped.
  The value-ranked remaining phase/task list is in PLAN.md.
- R1 stable shortcut-target identities: explicit nonlocalized IDs are separate
  from unchanged palette row IDs. The registry covers static palette/default
  bindings and reference-only targets, resolving Close Tab to the active document
  and generic Paste to an OS request rather than clipboard contents. Arbitrary
  payloads cannot alias generic commands. Tests pin the namespace across locale,
  view and order changes and preserve stable/legacy/numeric/unknown inactive
  overrides through settings roundtrip. Targeted tests and native/WASM lint pass.
  Direct planning/finished review was performed under autopilot after reviewer
  startup failed. Versioned migration, binding capture/apply/reset, conflicts,
  reserved-key decisions and native/P4 acceptance remain open.
- R1 reachable shortcut hints: menu/palette/reference metadata now excludes
  declared gestures intercepted by another default handler, using the real
  dispatch audit and runtime host/view. Fit Map is correctly unassigned instead
  of advertising Ctrl/Cmd+Shift+H (Find & Replace), while remaining clickable.
  Keys, availability, IDs and raw audit inventories are unchanged; no saved
  overrides are enabled. Targeted three-host metadata/menu/reference/audit and
  Fit execution tests plus native/WASM lint pass. User-approved direct reviews
  followed reviewer startup failure. Keymap conflict decisions, shortcut editing,
  reserved-key validation and native/P4 acceptance remain open.
- User-directed R1/R2 priority: Escape exits Zen with focused Map/Outline topics,
  preserving selection and yielding to editor/search/popup/modal/relationship
  cancellation first. Localized hints advertise Escape rather than relying on
  browser-owned F11. Search Escape is routed before canvas/text handling, so
  closing search does not also exit Zen. Search & Replace now grows from actual
  available height instead of inheriting its old compact Area size; normal
  viewports expose all controls without scrolling, while short/scaled windows
  retain bounded overflow. Direct planning/finished review was user-approved
  after reviewer startup failed. Full workspace CI with live AI disabled and
  WASM lint pass; real browser fullscreen/native input acceptance
  remains separate.
- R1 shortcut-conflict prerequisite: read-only default-dispatch diagnostics
  distinguish exact duplicates from modifier shadowing in actual global ->
  insertion -> Map/Outline order, excluding redundant same-command aliases.
  Three-host modifier/egui-consumption tests and actual app frames reproduce
  Fit's Ctrl/Cmd+Shift+H interception by Find & Replace and Outline's existing
  Ctrl/Cmd+Shift+Z Undo behavior (Map uses Redo). Keys/hints/settings are unchanged.
  Targeted tests and native/WASM lint pass; direct planning/finished review was
  user-approved after reviewer startup failed. Editable overrides, stable
  persisted IDs/migration, conflict decisions and OS/browser-reserved keys remain
  open; this diagnostic is not a claim of native keyboard acceptance.
- R1 view-aware Outline shortcut metadata: the existing eight action bindings
  now supply both Outline dispatch and live menu/palette/reference hints.
  Separate Up/Down descriptors retain visible-row traversal, including collapsed
  branches; they do not invoke geometric canvas navigation. The reference names
  the current view and retains shared insertion/priority keys, but no longer
  advertises Map-only aliases, clipboard events or canvas Escape in Outline.
  Command IDs, keys/order, focus/search/modal guards and toolbar ownership are
  unchanged. Direct planning/finished review was user-approved after reviewer
  startup failed; no rubber-duck review is claimed. Full workspace CI with live
  AI disabled and WASM lint pass. Rebinding, reserved-key
  conflicts, native input/accessibility acceptance and P4 remain open.
- R1 command-aware menu hints: main File/Edit/Insert/AI/Tools/View/Help and canvas
  context buttons derive hints from the same binding lookup as the catalog.
  Nested Select/Arrange entries show existing central-topic, selection and move
  bindings instead of omitting them. egui supplies platform formatting, removing
  duplicated key strings and manual Ctrl/Cmd substitutions. Command order,
  handlers, availability and context accessible names are preserved.
  Cut/Copy/Paste are documented as read-only event metadata, not new palette
  actions or key dispatch. Three-platform simulations check actual hint paint/
  clip bounds, availability and nested menus down to 640x240; existing localized
  geometry matrices remain. Direct planning/finished review was user-approved
  after reviewer startup failed. Outline-specific metadata, shortcut editing,
  reserved-key conflicts and native/P4 acceptance remain open.
- R1 contextual-shortcut inventory: map and insertion dispatch tables now supply
  contextual palette hints and every alias in the searchable reference. Redo,
  Insert/Backspace, central-topic, zoom, boundary and priority aliases are visible;
  Move Up/Down and Select None no longer appear unassigned. Escape remains a
  separate Cancel interaction, not a replacement for its existing cleanup path.
  Keys, ordering, clipboard/text-input/modal guards and command IDs are preserved.
  Tests cover inventory parity, real alias execution/Undo and long hint paint
  within compact localized viewports. Direct planning/finished review was
  user-approved after reviewer startup failed. Outline metadata, rebinding,
  reserved-key conflicts and native acceptance remain open.
- R1 global-shortcut catalog consistency: the palette and searchable reference
  now derive all 24 global bindings from the ordered dispatch table. Close Tab,
  Next Document and Command Palette are included; live Close Tab resolves the
  active document and retains unsaved-work prompts. Platform-aware hints use
  egui formatting for global commands and Cmd/Option names for contextual/menu
  hints on macOS. Existing command IDs and key precedence remain unchanged.
  Custom rebinding, conflicts/reserved keys, unified Outline metadata
  and native keyboard-layout acceptance remain open; this is not P4 delivery.
  The user approved direct planning/finished review after reviewer startup failed.
- R1 Outline keyboard ownership: structural shortcuts now belong to focused
  topic rows/disclosures, not the action toolbar, sidebar or other chrome.
  Stable topic-scoped IDs, explicit pointer focus and row-focus restoration
  preserve intentional topic editing/navigation. Global non-text Undo/Redo
  remains available without jumping focus away from the invoking control.
  The 99-case locale/theme/scale/panel matrix covers toolbar no-edit keys,
  actual button Enter, pointer/arrow focus, row Tab/edit Escape and sidebar Undo.
  Full workspace CI and WASM lint pass; planning/finished rubber-duck reviews
  found no blockers. Native input/accessibility acceptance remains open.
- R2 inspector keyboard reachability: Hyperlink could be focused at y1112 in
  an 800x400 viewport without scrolling. Style/Map/Symbols now reveal newly
  focused descendant controls. Shared scroll-focus handling follows widget
  ancestry, not just background layer, so Outline and sidebar regions cannot
  scroll one another. Unchanged focus preserves manual scrolling. The 66-case
  shipped-locale/Auto, light/dark, viewport/scale matrix covers focus, real Tab,
  clipped paint bounds, Outline isolation, document/history preservation and
  Notes draft/commit/one-step undo. Native assistive-technology acceptance remains
  separate; this does not close R2.
- R2 preferred native CJK fallback: native Settings > Editor lists installed
  faces; Auto resets. The preference follows configured faces and precedes
  automatic discovery, preserving document font names/history. Missing names
  warn once and stay stored. Browser builds omit/ignore this native preference.
  Compact localized picker bounds, Home initialization, settings roundtrips,
  actual missing-glyph availability, same-pass theme-font registration and
  installed Windows collection-font/two-document cache checks are covered.
  Direct planning/finished review was explicitly user-approved after the planning
  reviewer could not start; no rubber-duck review is claimed for this slice.
- R2 native mixed-script fallback: enable eframe 0.37's existing `system_fonts`
  provider, previously excluded by the default-feature override. Missing glyphs
  resolve to installed OS faces on demand, after configured fonts, without
  rewriting document fonts/history. Deterministic tests cover precedence,
  first measurement/cache reuse and font-definition changes; opt-in Windows
  installed-CJK acceptance covers real glyphs, title drafts/undo, documents and
  Outline. No font download/embedding or new locale. Native macOS/Linux CJK,
  IME, browser fonts and vector-export matching remain
  open; Ubuntu users need installed coverage such as `fonts-noto-cjk`.
- R1/R2 Find and Replace reachability: a reproduced x=-27 replacement field is
  now inside a viewport-bounded, height-scrollable search card with adaptive
  fields, wrapped controls and keyboard focus reveal. Narrow windows may
  temporarily cover the inspector to retain usable search controls. Outline
  editing shortcuts yield while search is open, preventing Tab navigation from
  inserting a child. A 66-case locale/theme/size-scale matrix covers both views,
  panel states, focus/Tab, control paint, resize/drafts, replacement/undo and close
  cleanup; existing search/cross-sheet tests remain. Daily-editing feature and
  native acceptance gaps are still open in the plan.
- R11 DMG lifecycle hardening: release packaging
  no longer explicitly mounts/customizes a writable image. Closed staging keeps
  the signed app, Applications link, icon and license prompt before image signing
  and notarization. Read-only verification scopes cleanup to its exact image/
  device, handles partial attaches, diagnoses open handles and retries EBUSY;
  last-resort forced detach is limited to that owned read-only device and must
  leave no attachment. Ticket/Gatekeeper checks share CloudKit-only retry policy.
  Local script suite: 71 tests, 9 native-only skips. Artifact-only run
  [`39598190225`](https://github.com/AxelWol/Thinoda/actions/runs/39598190225)
  at `afdf9b7` passed all five targets, including Intel/ARM native image lifecycle/
  EULA/signature tests and signed/notarized package/install verification.
  Publish/WinGet intentionally skipped. Planning/finished rubber-duck
  reviews found no blockers. Repeated Intel failures span different stages;
  different macOS runner images do not establish a CPU-specific defect.
  v0.6.0 attempt 4 failed installed-copy CloudKit validation; assets remain empty.
- R2 compact Outline actions: long selected titles no longer push actions off
  the page. A truncated summary/full hover and height-bounded, independently
  scrollable actions retain space for topics in short windows; adaptive margins
  reduce wasted space. Per-response focus reveal prevents header/body scroll
  interference. A 66-case locale/theme/viewport-scale matrix checks all nine
  actions' hit/paint bounds, root visibility, focus isolation and Move Down/Undo.
  Full workspace CI with live AI disabled and WASM lint pass; planning/finished
  rubber-duck reviews found no blockers. Native input/accessibility and extremely
  small viewports remain manual acceptance work.
- R2 deep Outline reachability: a 30-level branch reproduced an unreachable
  title even after scrolling. Both-axis scrolling now exposes deep rows, with
  document/sheet-scoped offsets, changed-primary/focus reveal and viewport-bounded
  title editors. Stable selection does not snap manual browsing back. A 66-case
  shipped-locale/Auto, light/dark and measured viewport/scale matrix exercises
  50-level trees, unclipped title paint, pointer selection, arrow navigation,
  typing/commit/undo and cross-document scroll retention. Existing Outline/title/
  e2e suites (70 cases total) and native/WASM lint pass. Planning/finished
  rubber-duck reviews found no blockers. Very short action-toolbar layout, native
  input/assistive technology and broader Outline presentation polish remain open.
- R11 Apple ticket retry hardening: failed Intel release jobs were traced to
  CloudKit connection loss after accepted notarization, not failed compilation
  or missing signing credentials. App packaging and DMG stapling now opt into
  five bounded attempts with 15/30/60/120-second backoff for the confirmed
  CloudKit network-loss signature only. DMG retries do not resubmit notarization;
  exhausted retries and all other errors fail, with mandatory signature/stapling/
  Gatekeeper checks unchanged. Recovery, exhaustion, rejection and real stderr
  capture regressions pass; planning/finished rubber-duck reviews found no
  blockers. The first live run exposed and locally reproduced a separate Settings
  startup-width regression in all five native test jobs. Reserving frame/scrollbar
  space fixes the clipped Editor toggle; direct unscrolled clicks and an expanded
  88-case Settings matrix cover it. Full local workspace CI and WASM lint pass.
  Fixed-branch artifact-only run
  [`39585598204`](https://github.com/AxelWol/Thinoda/actions/runs/39585598204)
  at `df8d1b7` passed all five native test/package jobs, including both macOS
  signature/notarization/stapling/installed-package checks, and uploaded all five
  artifacts. Publishing was intentionally skipped; fixes are not yet on main,
  and no repaired v0.6.0 release assets or full distribution acceptance is claimed.
- R2 shortcut-reference navigation: Settings now opens the existing searchable
  Keyboard Shortcuts reference from its category row. It is an action, not an
  empty preferences tab or a shortcut editor. Settings is disabled beneath the
  modal; Close/Escape returns to the previous category and retains unsaved AI
  key drafts. The 88-case shipped-locale/Auto, theme, Home/Map/Outline and category
  matrix uses actual pointer/search input at logical 320x240/200%; document,
  history, selection and preferences stay unchanged. Existing shortcut and
  Settings viewport regressions plus native/WASM lint pass. Planning/finished
  rubber-duck reviews found no blockers; rebinding/conflicts/reserved keys remain
  open, as does native input/assistive-technology acceptance.
- R2 bounded Settings: the window is constrained to the current viewport with
  both-axis scrolling, keyboard-focus reveal and a locale-independent identity.
  Wrapped API-key controls and stacked default-folder controls remove reproduced
  negative-coordinate overflow; detected cloud-folder rows wrap. All shipped
  languages/Auto, light/dark and three measured viewport/scale cases cover button
  ink/clip bounds, real Tab navigation, all categories, long paths, Cloud Use,
  close, resize/live scale and unsaved key-draft retention without credential,
  network or picker calls. Existing preference regressions and native/WASM lint
  pass; planning/finished rubber-duck reviews found no blockers. Native input/
  renderer acceptance and the broader preferences/shortcut editor remain open.
- R2 compact Map/Outliner switch: bottom controls now show the current view and
  reuse the existing toolbar/menu/palette/shortcut command. Pointer, Enter and
  Space work; localized shortcut hints use the platform modifier. Long current-
  view labels truncate to their scroll region, with full accessible names.
  Focus reveals the control without pinning manual browsing; hidden status, Zen,
  Home and modal/palette behavior follow the existing bottom-bar rules. Pending
  title drafts, selection, workbook and undo state survive view changes. All
  shipped languages/Auto, light/dark and three measured logical size/scale cases
  cover pointer/keyboard operation and actual label paint/clip bounds. Planning/
  finished rubber-duck reviews found no blockers; 69 targeted cases and native/
  WASM lint pass. The floating bar redesign and Gantt/Pitch remain open.
- R2 sidebar preferences: Format/Symbols family and the last Style/Map tab now
  survive disk-backed restart, independently of panel visibility. Symbols/hiding
  keep the remembered Map tab; topic formatting/Notes/Hyperlink intentionally
  select and remember Style. Missing fields retain the previous Format/Style
  defaults; unknown future string choices fall back without discarding unrelated
  settings. Saves occur on actual UI/command events, not idle frames. EN/DE and
  light/dark restart tests cover reopening, Home guards, unchanged map/history/
  selection and idle-write detection; platform settings regressions and native/
  WASM lint pass. Planning/finished rubber-duck reviews found no blockers.
  Future Sticker/Illustration/Pitch panes are not implied by these preferences.
- R2 responsive sheet/bottom navigation: many long sheet names no longer push Zoom
  and Fit outside the window. Explicitly bounded, independent horizontal regions
  retain tab context actions, zoom and statistics; New Sheet has a persistent slot.
  Long tabs truncate with full tooltips/accessibility names. Ordinary mouse wheels,
  horizontal scrolling and new keyboard focus reveal actions without pinning manual
  browsing or moving the other region. Active-sheet visibility is refreshed after
  rename/undo/resize and document switching, including returning to an existing
  document. Hiding status controls still preserves/expands sheet navigation.
  Sequential coverage includes 198 shipped-language/Auto, light/dark, scale and
  measured logical-viewport combinations, paint/clip bounds, navigation/Tab order,
  context menus, unchanged sheet data/history and stable row height. Menu tests
  share semantic popup scoping so accessible background buttons cannot masquerade
  as popup actions. Planning/finished rubber-duck reviews found no blockers;
  full workspace CI with live AI disabled and WASM lint pass.
  Native GPU/touch/assistive-technology acceptance and broader R2 work remain open.
- R0/R1 resumable Save All: one stable-document-key queue pauses at each conflict
  or whole-document MindManager consent and automatically resumes after resolution,
  including pending Quit, as explicitly selected by the user. Home/original-tab
  restoration survives reordering. Cancel/picker cancel/write failure aborts
  without closing or writing later documents; earlier saves are retained and
  unused overwrite consent is revoked. Autosave pauses during the batch; native
  close cancels without replacing the decision or upgrading manual Save All to
  Quit. Missing queued documents abort safely; new dirty documents veto Quit.
  Real file/reopen, conflict/consent cycling, partial success, cancellation,
  actual write failure, autosave timing and Close/CancelClose regressions pass.
  Planning/finished rubber-duck reviews found no blockers. Workspace CI with live
  AI disabled and WASM lint pass. Native picker/Save As Copy interaction and
  actual OS window-close acceptance remain manual; broader lifecycle is not done.
- R0/R1 pending-title lifecycle safety: Save All commits all inline title drafts
  before dirty checks; tab close commits only its target; native window close
  commits all titles before the save prompt. Cancel retains undoable text, while
  unchanged titles create neither edits nor spurious prompts. Map/Outline tests
  cover saved native files, Home/background documents, target-only flushing,
  all close choices, actual native-close cancellation output and one-step undo.
  Planning/finished reviews have no blockers. Idle autosave/recovery of pending
  editors remain open; interrupted/conflicting Save All is handled above.
- R1/R2 command-palette safety: viewport-bounded results keep search visible and
  automatically reveal keyboard selection without pinning manual scrolling.
  Palette input no longer reaches the canvas/Outline, fixing Escape dismissal
  and preserving selection/title drafts. Direct entry rejects other dialogs;
  modal/background-click precedence is covered. Locale/theme/scaling matrices
  verify actual frame/hit/paint/clip bounds down to 320x180 logical points, and
  keyboard tests cover all twelve visible matches, refiltering and exactly-once
  Enter activation. Planning/finished reviews completed; native keyboard/AT
  coverage remains open.
- R1 active-file command availability: File Save/Save As and both Export roots/
  leaves agree with palette filtering and guarded direct execution. Missing
  documents produce an explicit warning without save pickers/export consents.
  Save All remains available for Home's background documents and preserves Home/
  the original active tab. Real native two-file save/reopen and shipped-locale/
  theme/state matrices cover these routes. Planning/finished reviews have no
  remaining blockers in this slice. Draft detection is fixed by the subsequent
  lifecycle slice above; interruption/conflict/Quit continuation is handled above.
- R1/R2 shortcut-reference viewport safety: actual frame margins and available
  height replace fixed chrome/minimum-list estimates. A single reference scroll
  keeps all filters/commands reachable; a measured Close row stays visible.
  Footer reservation preserves natural forward/reverse Tab order. The shipped-
  locale/theme/scaling matrix checks frame/hit/paint/clip bounds, actual search/
  category/unassigned filters, focus reveal, resizing and document preservation
  down to 320x180 logical points. Planning and finished reviews completed; the
  latter's Tab-order finding was reproduced and corrected before commit.
  Shortcut rebinding/conflicts and native keyboard/AT acceptance remain open.
- R1 topic-field command consistency: Insert exposes Notes/Hyperlink using their
  existing shortcuts. Menu/toolbox/context/palette availability and direct/shortcut
  execution now require one valid topic and no blocking dialog/object selection.
  Invalid execution warns without changing the map or editor state. Opening a
  field explicitly commits an in-flight Map/Outline title edit before switching
  views. The locale/theme/scaling matrix covers focus, hidden-inspector reopening,
  buffered Notes/Hyperlink editing, undo/redo and disabled menu parity; guard/draft
  regressions cover invalid contexts. Planning/finished reviews have no blockers.
  General command auditing, rebinding and native keyboard acceptance remain open.
- R2 main-menu overflow follow-up: hamburger categories and all their nested
  submenus now scroll within the available viewport height. Newly focused menu
  actions scroll into view without pinning manual scrolling. The shipped-locale
  Light/Dark/System matrix covers 100/150/200% scaling down to 240 logical pixels
  high, actual popup bounds, template/Arrange nesting, keyboard activation and
  document/selection preservation. Finished-step rubber-duck review has no blocking
  findings. See subsequent context/toolbar-root fixes below. Nested color pickers,
  text-control menus, full keyboard/accessibility audits and real native/browser
  acceptance remain open; this does not complete R2/P4.2.

### R2 screenshot-driven mitigations

- User screenshot (2026-10-20): the German topic context menu is taller than the
  app window. **Delivered:** all application-defined context-menu roots and nested
  Select/Arrange/AI menus use shared viewport-height scrolling/focus behavior,
  retaining direct command access with semantic separators. The regression matrix
  covers short/edge viewports, shipped locales, themes and 100/150/200% scaling;
  existing command/symbol/quick-style tests pass. Planning and finished-step reviews
  found no blockers. Full grouping/text-control/native-accessibility acceptance,
  other color-picker surfaces remain open; the nested symbol-instance follow-up
  is delivered below and toolbar roots are covered by
  the completed follow-up below.
  Scope boundary: this context-menu mitigation does not cover toolbar-triggered
  marker/symbol insertion, Export, New-from-Template categories, Zoom or topic
  statistics. Their separate toolbar/main-menu matrices are recorded below/above;
  nested color-picker interaction and the full P4.2 audit remain distinct gates.
- Nested symbol-instance color picker (user review, 2026-10-22): **Delivered.**
  Opening egui's root color popup replaced the context-menu open state and closed
  both. Use a proper height-bounded submenu for symbol instances, leaving other
  picker surfaces unchanged. The shipped-locale/Auto, light/dark, 100-200% matrix
  proves parent scrolling after shrinking an open menu, checks child bounds and
  anchor/focus stability, edits a numeric color channel, verifies Escape and
  selection preservation, and restores color/contrast changes with single Undo.
  The unavailable reviewer was replaced by explicitly user-approved direct
  planning/finished review for this slice. Other nested pickers, text-control
  menus and native/browser/accessibility acceptance remain open.
- Map inspector maximum-resize corruption (user screenshot, 2026-10-20):
  **Delivered:** pointer-drag reproduction isolated theme-grid cell expansion.
  Reused fixed-size preview cards with responsive columns instead of allowing
  preview/caption layout to push content beyond the panel's clamped boundary.
  Locale/appearance/scaling coverage exercises in-drag/released min/max/repeated
  resize, window shrink, hide/reopen and restored egui panel memory. Paint/clip
  checks include both tabs and every scrolled theme caption; document, selection
  and revision stay unchanged. Keyboard theme activation/undo is covered.
  Planning and finished-step reviews completed, including nonvacuity improvements.
  Native GPU visual acceptance and actual disk-backed host restart remain open.
- Outline/structure toolbar collision (user screenshot, 2026-10-20): the floating
  map insertion toolbox obscures list-modification buttons. **Delivered:** the
  insertion toolbox is Map-only; Outline's own wrapped action rows remain
  unobscured. Locale/theme/100-200% regression coverage verifies visible layers,
  hit/paint/clip bounds, actual editing with undo/redo, mode/selection preservation,
  popup cleanup and hidden-inspector reopening/Notes-shortcut alternatives.
  Planning and finished-step reviews completed without remaining blockers.
  Native GPU/accessibility acceptance remains open.

### R2 toolbar-popup follow-up

- **Delivered:** Export, Marker, More insert actions, Zoom and Statistics share
  viewport-bounded scrolling and keyboard-focus reveal. Marker no longer assumes
  a 560-point-tall screen; Export has a localized accessible name.
  The locale/appearance/100-200% matrix covers short and normal-height windows,
  first/last item reachability, actual popup bounds (one physical pixel of frame
  rounding tolerance), Marker/More widths, marker insertion/undo, keyboard zoom
  activation and unchanged document/selection on dismissal.
  Planning and finished-step rubber-duck reviews found no remaining blockers.
  Nested egui color-picker/text-control menus and native/GPU/accessibility
  acceptance remain open; R2/P4.2 are not marked complete.

### Earlier completed slices

- R1 compact menu-navigation slice: the hamburger now presents eight categories
  (File/Edit/Sheet/Insert/AI/Tools/View/Help). File contains document actions and
  Quit; Settings is in Edit, with matching palette/shortcut-browser categorization.
  Escape dismissal preserves topic selection. The root fits short localized
  windows at 100/200% scaling across shipped locales and appearance modes.
  Nested Arrange/template/print routes retain coverage. Main submenu overflow is
  handled by the R2 follow-up above; context menu bounds, full keyboard traversal
  and native menus remain open. Finished-step rubber-duck review identified the category mismatch,
  which was corrected before completion.
- R0 startup-close follow-up: headless viewport-event regressions verify that
  first-splash close preserves earlier recovery snapshots and unopened sources;
  closing during multi-file import protects dirty maps with the existing save
  prompt, and Cancel preserves all imported documents. Dedicated rubber-duck
  review has no blocking findings; real native window close remains unverified.
- R0 startup/sidebar regression slice: splash appearance and readiness across all
  ten shipped locales plus Auto, light/dark and simulated system appearance,
  narrow windows and 150% app scaling. Persisted sidebar visibility is exercised
  through Home, Map/Outline and Style/Map changes without document/selection loss.
  Outline action rows now wrap instead of intercepting Map-tab clicks at narrow
  widths. This does not close the broader Format Panel report or all localized
  geometry work; Italian toolbar/sidebar accessible bounds still need R2 review.
- Legal documents now render CommonMark/GFM formatting, including headings,
  emphasis, nested lists, links, code blocks and tables, with 1.5x natural text
  line height in both themes. Wide tables scroll; Copy preserves original
  Markdown. Bundled-document links stay offline; remote images and executable
  HTML are not loaded.
- Automatic documentation timestamps: every tracked Markdown file carries
  non-binding UTC metadata from its last source-changing Git commit. The
  `docs_stamp.py` tool ignores metadata-only commits, preserves frontmatter and
  full legal text, and refreshes metadata automatically after main pushes
  (including squash/rebase merges and release automation). Generated notices
  retain their independent content check. See the contribution guide for
  optional local commands, CI permissions and concurrency handling.
- Help > About > Legal documents now embeds the full unchanged licence,
  generated third-party notices and Impressum for offline reading and copying.
  A scrollable, theme-aware modal keeps Close visible at narrow sizes; original
  legal texts retain their original language. EN/DE labels, document completeness,
  keyboard dismissal and map-edit isolation are covered.
- Template-card double-click creates and opens the chosen map once through the
  existing Create command, across the overview, every category and Recently used
  templates. Single-click/keyboard selection and Create/Cancel remain available;
  unsaved source maps stay intact. Pointer-driven tests cover all 35 templates
  in EN/DE and light/dark mode, slow clicks, recents and the overview.
- Clickable links: the About dialog's contact address opens the default mail client
  (`mailto:`) and the project URL opens the web browser; Help ▸ Feedback/Project News
  and topic hyperlinks use the same path. eframe is built without its `links`
  feature, so the shell drains egui `OpenUrl` commands and passes them to the
  hardened `system::open_url` (HTTP(S) and `mailto:` only). Translated hover hints,
  theme-aware link colours; covered for EN/DE × light/dark by `tests/about_links.rs`.
- New Thinoda logo (a "T" grown from mind-map nodes whose stem resolves into a solid shape). `cargo xtask icons` generates standard, small-size and Apple-grid variants: `icon.ico` (10 frames, 16–256 px), `icon.icns` (all Retina slots up to 1024 px), Linux hicolor PNGs (16–512 px) and the macOS Dock runtime icon. The logo is shown in the About dialog, User Guide and README.
- Renamed the app to **Thinoda** ("Give your thoughts shape.") across all crates,
  binaries, file extension (`.thinoda`), bundle identifiers, environment variables,
  installers, workflows, documentation and the GitHub repository. The tagline appears
  in the About dialog (EN/DE), README, CLI help and package metadata.
- Publication readiness: added CHANGELOG.md (Keep a Changelog + release-please
  Conventional-Commit semantic versioning), CODE_OF_CONDUCT.md (contribution
  guide and rules of engagement), LICENSE.md (EUPL-1.2 with informal summary;
  workspace and installer metadata switched from MIT/Apache-2.0), SECURITY.md,
  IMPRESSUM.md (author, originality disclaimer, trademarks) and generated
  THIRD_PARTY_NOTICES.md (runtime dependency licences, CI drift check).
  Releases are published automatically with all installers attached as GitHub
  release assets; the NSIS installer shows the licence page.
- Release CI: macOS platform tests were flaky because the panic-hook test
  installed a process-global hook that captured other tests' panics; it now runs
  in an isolated child process, and single-instance re-election tolerates a
  short lock hand-off window.
- Windows system printer preview: the modern OS dialog receives the actual
  single-page bitmap through WinRT PrintManagerInterop, a native document source
  and DXGI preview surfaces, replacing the missing-preview PrintDlg/GDI path.
  The same Direct2D drawing supplies native output with OS-selected page and
  imageable bounds. Printer choice/advanced options, modal ownership, fixed-page
  aspect ratio and cancellation/submission/error distinctions remain intact.
  Job-free real-SDK/WARP pixel readback, in-memory XPS page output and retained
  callback lifecycle tests cover the bridge; Windows x64/ARM64 CI runs them.
  macOS/Linux backends and existing translations are unchanged, not claimed as
  newly delivered preview parity. See [native printing](NATIVE-PRINTING.md).
  No test print jobs, printer configuration changes, external viewer handoff or
  forced classic dialog. Independent of legal/localization work; WinGet paused.
- Windows executable embeds the Thinoda icon and version info (Explorer/taskbar).
- The Outliner now surfaces controls for adding siblings/children, editing topics,
  moving and reparenting topics, deleting a subtree or keeping children, with
  availability tied to the shared canvas actions. Edits remain undoable. EN/DE
  labels and end-to-end coverage verify topic creation, rename, ordering,
  hierarchy changes, both deletion modes and undo.
- New from Template main-menu submenu now groups all 35 templates under their
  existing eight EN/DE categories. Category submenus retain direct template
  creation; coverage verifies every template appears in its declared category
  and that selection creates the expected workbook.
- Importer mutation smoke: valid native ZIP/raw JSON, XMind, FreeMind, OPML,
  Markdown, plain text and MindManager seeds undergo systematic truncation and
  128 deterministic byte mutations each. Accepted imports must validate and
  round-trip through native storage. Structured XML depth/entity/attribute cases
  and malformed XMind JSON are covered. Oversized declared ZIP entries are
  rejected explicitly before decompression without allocating a large fixture.
  This is bounded regression coverage, not coverage-guided fuzzing or proof that
  arbitrary inputs are safe; longer campaigns remain open.
- Coverage-guided importer fuzzing: separate `cargo-fuzz` targets cover XMind,
  FreeMind and OPML with original generated seeds and dictionaries. Linux CI runs
  each target for 20 seconds with input, timeout, RSS and job-duration limits.
  See [`fuzz/README.md`](../fuzz/README.md) for commands, dependency isolation
  and limits; this smoke does not prove arbitrary inputs safe.
- CPU layout benchmark: `thinoda-core` release example measures 10,000-topic balanced/
  wide maps with JSON p50/p95/max layout, undoable leaf rename and undo latency.
  Optional p95 budgets fail explicitly after preserving the report; CLI input
  bounds and nearest-rank statistics are covered. CI is configured to retain
  per-platform reports; execution outside this Windows x64 host is not claimed.
  Initial local layout p95 was 23.68/28.71 ms; a 50 ms local-only regression
  envelope passed on repeat (see PLAN for hardware and exact methodology).
  ApproxMeasure CPU layout is not GPU rendering, frame latency or a 60-fps result.
- Shortcut reference: F1/Help opens an EN/DE searchable, category-filtered
  command browser, including unassigned commands and navigation aliases.
  Scroll resets on filter changes; the fixed Close control and modal isolation
  work on narrow windows without editing the map/outliner behind the reference.
  The reference is explicitly read-only: rebinding, reserved keys, conflict
  editing and reset-all remain open. Fixed advertised Ctrl+Q dispatch; it now
  requests the normal native close path and retains dirty-document save prompts.
  All 387 workspace cases and all-target clippy pass with live AI disabled.
- Offline Help: Quick Start and searchable User Guide are available from Help
  and the command palette in EN/DE, including on Home. Six sections explain
  supported workflows and explicit cloud/export/recovery/privacy limitations.
  Guide modals isolate canvas/outliner input and retain a visible keyboard-
  accessible Close control at narrow sizes. Send Feedback opens the public
  GitHub issue chooser; Project News opens commit history, with no document,
  settings or log upload. These are not updater/release-announcement services.
  All 384 workspace cases and all-target clippy pass with live AI disabled.
- Original template expansion: 18 new scenario maps bring the gallery to 35.
  Problem Solving, Concept Framework, Software Architecture, Business Plan,
  Workshop, Dissertation Plan, School Timetable, Reading Notes, Language Learning,
  Project Retrospective, Shopping List, Mood Journal, Service Comparison,
  Cause and Effect, Comparison, Social Media Plan, Recipe and Film Production
  contain original editable prompts and scenario-specific branches. EN/DE gallery
  titles, actual preview/Create agreement, keyboard selection, bounded recents,
  native/XMind round trips and responsive layouts cover every registered template.
  Starter document text remains English. School Timetable is a topic hierarchy,
  not a calendar; no native XMind template equivalence is claimed. AR, organization/
  nine-box/BCG matrix semantics and truncated reference titles remain open.
- Template-section navigation: EN/DE category chevrons and counts open each of
  the eight sections; fixed All templates returns to the overview. Each view
  retains its own scroll position, selections clear on navigation, and Create/
  Cancel retain their non-destructive semantics. All sections support keyboard
  activation and narrow/wide layouts. Reference-specific additional template
  content remains open. All 378 workspace cases and all-target clippy pass
  with live AI disabled; Windows EXE/NSIS refreshed.
- Custom-theme authoring: staged EN/DE editor with actual-renderer previews,
  built-in base, 1-16 palette colors, separate light/dark backgrounds, global
  font and central/main/sub/floating/Summary role defaults. Apply embeds the
  theme in one undo step; Save to Library persists a separate profile copy.
  Up to 32 unique names, edit-as-copy, confirmed removal, atomic profile writes,
  visible errors, built-in reset and opt-in recent-custom-theme reuse are
  implemented. Explicit topic/map overrides retain precedence; maps remain
  independent of the local library. Reset Style inherits custom role defaults;
  automatic quick-style width inherits a target role's fixed width.
  Native/private XMind snapshots preserve authoring data. Public JSON/XML exports
  materialize supported light-mode topic appearance, not editable imported XMind
  theme registries or verified native visual parity. Modal footers remain stable
  and notifications no longer intercept their clicks. All 377 workspace cases
  and all-target clippy pass with live AI disabled.
- Reusable quick styles: four original rendered presets plus up to 32 named
  local styles captured from resolved topic appearance, including inherited
  colors, font, branch geometry and automatic/fixed wrapping. Applying across
  roles/documents replaces appearance only in one multi-selection undo step.
  Save is available in Style/Edit/context/palette; applying is available in
  Style/palette. EN/DE controls, keyboard activation, cancellation and confirmed
  removal are implemented. Unique printable names (1-80 characters) are required;
  saving a duplicate is rejected rather than overwriting. Profile writes are
  atomic and failed saves/removals leave the library and map unchanged with a
  visible error. Legacy settings default to an empty library; invalid libraries
  use the existing preserved-file/warning recovery path. Applied styles persist
  in native and unchanged XMind snapshots. Custom-theme authoring is delivered above.
- Map-wide branch stroke width: Map > Branches offers Automatic or an explicit
  0.5-16 width in EN/DE. Topic overrides take precedence; restoring Automatic
  returns the theme's level-specific defaults. Buffered numeric drafts commit
  once, reject malformed/nonfinite/out-of-range values visibly and cancel with
  Escape. Confirming sidebar fields consumes Enter rather than inserting topics.
  Native/unchanged Thinoda XMind snapshots preserve inheritance; public JSON/XML
  exports include effective widths. Custom-theme authoring is delivered above.
- Real font-family support: Map > Theme > Global Font and Style > Text > Font
  Family select local installed faces (including user-installed fonts); Auto
  inherits the map setting. Reusable UI font registration loads only requested
  faces and preserves symbol fallbacks. Canvas measurement, painting, inline
  editing and Map Shot share the face; font-definition changes invalidate layout.
  Missing names warn once without dirtying maps or erasing requests. Native
  persistence and public XMind JSON/XML effective names are covered, as are EN/DE
  controls, undo and actual glyph-width changes. Fonts are not embedded or
  redistributed; exact vector-export metrics, CJK fallback/IME and other native
  hosts remain separate release work.
- Flexible Floating Topics: undoable per-sheet Map > Advanced Layout switch
  (EN/DE). Floating roots move freely over topics without attaching; attached
  topics still reparent/reorder, but blank-space detachment requires switching it
  off. Mixed floating/attached moves report an error without partial changes.
  Defaults off for old maps; native and unchanged Thinoda XMind snapshots preserve
  the setting. Pointer/zoom, undo, sheet isolation, stale-drag cancellation and
  German controls are covered. Separate Topic Overlap/collision behavior and
  Branch Free-Positioning remain open.
  Sources: https://xmind.com/user-guide/advanced-layout-new ,
  https://xmind.com/user-guide/floating-topic
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
  are covered. Section navigation is delivered above; reference-specific content remains open.
- Map structure preview gallery: current-layout thumbnail/name opens an anchored,
  bounded scrollable popup with nine collapsible families and responsive one-to-three
  column cards for all 17 supported structures. Original samples use the real core
  layout and canvas painter, cached separately for light/dark. Selection is highlighted;
  keyboard/accessibility activation, cancellation, family collapsing, narrow/high-scale
  bounds, all-layout rich reopening and atomic undo/redo are covered in EN/DE.
  Popup keys no longer insert/change topics in the underlying map or outliner;
  outliner leaf placeholders are noninteractive and branch controls have names.
- Tools > Merge Map Files appends complete `.thinoda`/`.xmind` sheets atomically,
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
  Thinoda instances; canvas Paste recognizes the payload, while Paste Sheet explicitly
  rejects unrelated/invalid data. Payloads are validated and capped at 16 MiB;
  late responses, document switches and empty clipboard responses are handled.
  Pasting adds a sheet in one undo step. This is Thinoda clipboard interoperability,
  not compatibility with XMind's private clipboard format.
- Save Sheet As writes only the active sheet to `.thinoda` or `.xmind`, retaining
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
- About Thinoda includes Axel Wolters' author credit and contact email in EN/DE.
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
  Audit the SDK's supported file versions and API coverage against Thinoda's existing
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
    with Thinoda import/export; compare supported content and style fidelity.
  - [xmindltd/nevermind](https://github.com/xmindltd/nevermind):
    assess the font's license/redistribution terms, script coverage, readability,
    fallback behavior and cross-platform rendering before considering optional use.
  - [xmindltd/xmindmark](https://github.com/xmindltd/xmindmark):
    assess the markup format and tools for text-based map authoring, import/export
    and interoperability with existing Markdown/outline workflows.
  - [xmindltd/xmind-cli](https://github.com/xmindltd/xmind-cli):
    inventory commands and supported formats; evaluate useful Thinoda CLI parity
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
- P1 macOS native system-preview follow-up: connect the document to AppKit's
  native preview lifecycle. The existing standalone `NSPrintPanel` has no document
  preview; preserving it is not delivery of cross-platform native-preview parity.
  Retain reliable cancellation versus failure reporting when changing that lifecycle.
  Linux GTK preview behavior remains desktop/backend-dependent and unchanged.
- P1 Print (Ctrl+P) with page setup and fit-to-page
  - Reference dialog: left live page preview with page count; right scrollable,
    grouped settings; fixed Cancel/Print footer.
  - Printer selection, copies, content scope (Current map shown), printer-supported
    paper sizes, landscape/portrait and scaling (Fit page shown). Verify other
    scope/scaling options rather than guessing from closed dropdowns.
  - Background-color and monochrome toggles, reflected in the preview and output.
    The displayed paper size is a selection, not a required default.
  - XMind's attribution toggle is product branding, not a requirement to print its
    name or add a watermark to Thinoda. Decide any Thinoda attribution separately.
  - Cover no printer, unavailable printer, driver capability changes, invalid copy
    counts, multi-page navigation, cancellation and spool errors; never send a real
    print job as an automated smoke check without explicit user approval.
- Cross-sheet Find and title replacement are implemented; notes/labels are explicitly
  search-only. Rich-field replacement needs separately specified behavior.
- Delivered Pitch from branches/Zones, presenter/player and PDF/PNG slide export
  in F4; PPTX/aspect-ratio follow-up is XV6. F5 is AI, not a Pitch shortcut claim.
- Delivered Map Shot (F7): visible-canvas region capture to clipboard/PNG (see above).
- Delivered Zone editing in F4; callout polish and host interaction acceptance remain.
- Delivered F3 Gantt for scheduled Tasks, dates/progress/dependencies/export;
  lightweight To-Do roll-up is XV2, not automatic Gantt scheduling.
- P2 Equations: XV7
- P1 Portable images/attachments: XV4
- P2 Password-protected files (AES-GCM, Argon2 key derivation)
- Delivered custom theme editor, quick-style libraries and actual font rendering
  (see Recently completed). Editable imported XMind theme/style registries and
  native reference visual parity remain open.
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
    Global font and map-wide branch width are implemented; theme creation and
    exact reference previews still remain. Reuse existing theme settings and
    verify partially visible controls.
  - Generate previews from original sample maps, not copied XMind thumbnails;
    commercial badges do not establish Thinoda feature restrictions. Test every preset,
    undo/redo, saved-file round trips, large/deep trees and keyboard popup navigation.
- P3 Audio notes (record/play)
- Delivered configurable saved shortcut profiles in F2; real OS/IME acceptance
  remains open. Historical screenshot requirements below are not missing editor code.

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
  commerce actions, not requirements to add accounts or monetization to Thinoda.
  Never trigger updates/restarts, feedback transmission or authentication merely
  by reproducing the menu.
- P1 Command consistency: menus, context menus, toolbars, palette and shortcuts must
  dispatch the same commands with the same selection/document availability rules.
  Cover keyboard submenu traversal, Escape, focus restoration, viewport-edge
  placement/scrolling and EN/DE labels in both appearances.
- P2 Reference shortcut audit: record Windows mappings without silently replacing
  Thinoda bindings. The screenshots show Ctrl+H for Home, Ctrl+F for Find & Replace,
  Ctrl+Shift+P for Settings, Ctrl+R for the central topic, Ctrl+Shift+R for Relationship,
  Ctrl+Shift+B for Boundary, Ctrl+Shift+N for Note, Ctrl+Shift+L for Label,
  Ctrl+Shift+I for Local image and Ctrl+Alt+N for New sheet.
  Layout-dependent keys (# and umlaut keys in the reference) need physical/logical
  key and macOS/Linux verification. Preserve current shortcuts pending an explicit
  compatibility-keymap decision.
- P2 Bottom-left sheet-tab context menu: Rename, Copy, Paste, Duplicate, Delete,
  New sheet and Save sheet as a separate file are implemented, with last-sheet
  protection and undo for document changes. Copy uses a validated, versioned Thinoda
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
- Delivered template-card double-click creation; see Recently completed.
- Implemented generated previews, responsive grids, scrolling, fixed header/category
  filter, grouped sections, Recently used templates and fixed Cancel/Create actions.
  Selection enables Create without immediate creation. Section navigation chevrons,
  counts and fixed All templates return navigation are implemented (see above).
  Additional reference-specific content still needs verified implementation.
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
  The original 17 templates plus the 18 scenario maps above cover the verified
  general-purpose inventory without copying reference content. AR application maps,
  BCG/organization/nine-box matrices and truncated reference titles remain open;
  exact XMind equivalence is not claimed.
- P2 Verify category filtering, recent-template persistence, keyboard selection,
  Cancel without document creation and creation from each template; cover narrow
  windows, display scaling and long localized names.

## Settings parity (user-supplied XMind screenshots, 2026-10-16)
These are reference requirements, not a claim that the settings are implemented.
The screenshots show selected values, not every available option or factory default.
Reuse existing persisted settings where applicable rather than introducing duplicates.

- [x] P1 Expand language support beyond English/German to Spanish, French, Danish,
  Dutch (Netherlands), Portuguese (Portugal and Brazil), Italian and Polish. Translate every user-facing word
  and sentence in its actual UI/workflow context, using consistent terminology
  rather than isolated literal translations. Include help, dialogs, errors and
  installer/package UI for all Windows, macOS and Linux targets/builds; retain
  the existing languages and avoid missing or unlabeled installer controls.
  Replace the Settings > General > Language segmented control with a drop-down
  containing Auto and every supported language. Preserve system-language
  detection, explicit selection and persistence. Cover translation completeness,
  placeholders, switching/restart, installer labels, and long text in light/dark
  mode with tests and contextual review.
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
- P2 Editor: visual alignment-guide visibility is implemented as an opt-in
  Settings preference for free-placement topic dragging: nearest matching group
  edge/center within four screen points, no snapping/history changes. Canvas
  threshold/group/paint/drop/cancel/Undo and localized Settings persistence checks
  cover this slice; equal-spacing/snap/native visual parity remain open.
  Initial canvas zoom and opt-in reuse of
  the last structure/theme for new blank maps are implemented; skeleton-specific
  variants still need the layout-gallery research.
- P2 Editor: browser font provisioning, IME and remaining macOS/Linux font
  acceptance. Native automatic mixed-script fallback and the optional default
  CJK-font preference are implemented; configured font priority, layout/cache
  behavior, open-document switching and installed Windows CJK coverage are verified.
- F3 delivered working-weekday/dated-exception calendar drafts and Gantt
  scheduling integration. Additional base-calendar management, month-navigation/
  shading parity and reference overflow operations still need research; do not
  schedule the delivered calendar arithmetic and Settings integration again.
- P3 Sharing: configurable destination visibility for email, blog, Facebook, X,
  Pinterest and Evernote. Verify current service APIs before committing integrations;
  require explicit user action before connecting accounts or publishing content.
- F2 delivered command search/category filters, assign/unassign/reset, named
  profiles and shared menu/palette/native hints with conflict/reserved-key
  feedback. Preserve platform/IME/host acceptance rather than reimplementing it.
- Historical shortcut command coverage checklist from the reference: home, new blank map, new/template,
  merge windows, new sheet, sheet from topic, and delete a single topic while
  retaining children still need this customizable-shortcuts UI row. Previous/next
  sheet (implemented with fixed Ctrl+PageUp/Ctrl+PageDown) and copy/paste/reset
  style (implemented with fixed Ctrl+Alt+C/V; Reset has no shortcut) now exist as
  commands; use the shipped F2 command registry/editor to audit any residual
  command coverage, not a second customization system.
  The screenshot assigns Ctrl+H to Home, conflicting with Thinoda's existing Find &
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
  Thinoda gestures are retained, with the optional Flexible Floating Topics setting
  now controlling free floating placement and conversion (see Recently completed).
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
  zoom/pan settings, including large curves. Native files and unchanged Thinoda XMind
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
  XMind's "PRO" badges are product-specific and do not establish Thinoda licensing.
  Cover menu accessibility, dismissal/focus, insertion undo and packaged-file round trips.
- P1 Right-sidebar navigation: two adjacent buttons at the top-right of the canvas.
  The smiley selects the Symbol / Sticker / Illustration panel family; the panel
  icon selects the Style ("Stil") / Pitch / Map family. Show the active family
  and active tab distinctly, using one shared sidebar region rather than six
  unrelated panes. Preserve topic selection while switching families/tabs.
  Thinoda retains its existing repeated-click hide/reopen behavior and remembers the
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
  `.thinoda`/`.xmind` round trips. The screenshot does not establish these semantics.
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
- [x] Streaming responses (SSE), bounded preview/cancel and deliberate Apply (F5)
- [x] P1 Store API keys in the OS keychain (Windows Credential Manager, macOS Keychain,
  Secret Service) instead of `settings.toml` (implemented; legacy keys migrate only
  after secure storage succeeds)
- [x] Semantic search/clustering with an explicitly selected embedding model (F5)
- [x] Rewrite topic, translate branch, reviewed AI Pitch outline (F5)
- [x] User-editable saved prompt library (F5)

## AI — user-guide research addendum (2026-10-16)
**Gap inventory reconciled 2026-10-26:** F5 delivered scoped chat and
depth modes, prompted Grow Ideas, Reorganize and Refine with explicit reviewed
Apply. Session-only history can be extended by XV9; do not rebuild chat.
Online-search augmentation, vision, generated images/video,
hosted integrations and companion-device workflows remain undelivered.

Verified against xmind.com/user-guide pages not previously digested in
XMIND-USER-GUIDE.md/XMIND-RESEARCH.md; each item cites its source page. Thinoda's AI
is local/OpenAI-compatible only (LM Studio, Ollama, llama.cpp, OpenAI). Residual
gaps must fit that transport or receive a separate scope decision; they are not
requirements to add XMind's cloud account, billing or hosted AI service.

- Delivered F5: scoped discussion panel with Quick/Balanced/Deep response
  guidance, selected-topic/sheet context, Send/Cancel and session-only per-open-
  document history. XV9 covers optional durable local history/named threads.
  Source: https://xmind.com/user-guide/xmind-ai-chatbot/web
- Delivered F5: "Grow Ideas" with Auto or prompted expansion. The residual
  P2 research item is optional online-search grounding with source links.
  Thinoda has no live web-search-augmented generation;
  treat external web search as an explicit, user-visible, opt-in data egress if added.
  Source: https://xmind.com/user-guide/grow-ideas
- Delivered F5: AI "Reorganize" with optional guidance and reviewed Apply,
  distinct from deterministic Map Style layout actions.
  Source: https://xmind.com/user-guide/reorganize
- Delivered F5: AI "Refine" with content additions, reviewed Apply and Undo;
  do not reclassify the existing feature as absent.
  Source: https://xmind.com/user-guide/map-refine
- P3 Confirmed absent: "Image to Mind Map" generates a structured map from an
  uploaded photo (JPG/PNG, documented 10 MB limit), using OCR for text-bearing
  images and visual-content analysis for images without text, with an output-
  language choice. Thinoda's AI only accepts text prompts today; this needs a
  vision-capable model and explicit scope/privacy decision before implementation.
  Source: https://xmind.com/user-guide/image-to-mind-map
- P3 Confirmed absent: AI-generated topic images ("Make Image": prompt + selectable
  image-generation model + style presets + "Inspire" prompt regeneration, inserted
  as a topic illustration with automatic background removal for preset styles) and
  a standalone "Remove Image Background" action for any already-embedded image.
  Both need an image-generation-capable backend distinct from Thinoda's current
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
  as prior art for Thinoda's own MCP-server backlog item above (XMind SDK/ecosystem
  review), not as a requirement to integrate with a third-party chat product or account.
  Source: https://xmind.com/user-guide/chatgpt-apps
- P3 Platform-specific, future companion-app scope only: "Draw to Map" (iPad + Apple
  Pencil sketch digitized into an editable map, original sketch kept as the central
  topic's image) and "Record to Map" (iOS/iPadOS 26 "Apple Intelligence" voice
  recording with live transcription saved as the central topic's note, then
  converted to a structured map) are undocumented in Thinoda's desktop-first scope.
  Track as future companion-app research only; no current application change.
  Sources: https://xmind.com/user-guide/draw-to-map ,
  https://xmind.com/user-guide/record-to-map

## Platform & collaboration
- User-directed **Phase P4 / R13 (R12 and above)**: enhanced OS-integrated menus.
  Windows gets an original collapsible Office-like ribbon; macOS gets the real
  screen-top menu bar and multiple document windows; Ubuntu follows the supported
  desktop/session's standard menu conventions with an in-window fallback. WASM
  retains a compact hamburger with grouped, height-bounded menus; an optional
  wide-screen menu bar is evaluated without pretending to be an OS-global menu.
  Shared actions include File/Edit/Sheet/Insert/AI/Tools/View/Help, Settings in
  Edit (plus the standard macOS alias), native shortcuts and required OS actions.
  Audit and regroup **all** main/context menus so no item becomes unreachable in
  short viewports or at high scaling. All 13 accepted locales, light/dark/Auto,
  accessibility, updated tests/docs and safe unsaved-work handling are required.
  [P4.1-P4.7 in PLAN.md](PLAN.md#phase-p4---os-integrated-menus-and-windows-r12-and-above)
  define the multiple steps and full planning/per-finished-step rubber-duck gates.
  Full planning rubber-duck review is complete; its ribbon taxonomy,
  Settings-placement, single-instance/viewport and floating-toolbar clarifications
  are incorporated. F1/F2 implementation is now shipped; the detailed P4 native
  host/window/accessibility acceptance gates remain open, not all implementation.
- P3 Quick Entry-equivalent background capture (XMind "Quick Entry", documented
  macOS-only): a menu-bar-resident popup, launchable without opening the main
  window, with a single text field and a "Create Map" action that converts the
  note into a new map, plus an optional "start at system login" preference.
  Thinoda has no background/menu-bar quick-capture surface on any platform; verify
  a cross-platform equivalent (tray icon on Windows/Linux) before committing to
  parity with the macOS-only original. Source: https://xmind.com/user-guide/quick-entry-for-mac-new
- Delivered macOS Developer ID signing/notarization in v0.12.1.
  P1 Windows production Authenticode and updater delivery remain separate work.
- P2 macOS universal binary; Flatpak and `.rpm` packages; winget/Homebrew manifests
- P2 Publish installers to package managers from the release workflow (winget
  manifest PR, Homebrew cask tap, Flathub); GitHub Packages is not used because
  it does not host desktop installers — GitHub Release assets are the canonical
  download location
- P2 Enable GitHub private vulnerability reporting and branch protection with
  required CI checks before making the repository public
- Delivered offline licence, third-party notices and Impressum inside Help > About;
  see Recently completed.
- P1 Startup splash screen (implemented; R0 acceptance verification remains):
  while Thinoda launches, show a centered,
  About-style branded card (logo, name, tagline, version) with a real progress
  indicator — indeterminate while starting, determinate only for counted launch
  files (file i of n) — in the configured light/dark/Auto theme and startup
  language. Launch files are imported off the UI thread so the window stays
  responsive. Ready = every launch file opened or reported as an error toast and
  the main UI drawn; only then does the splash disappear and pending crash
  recovery prompts and settings warnings appear. No artificial minimum wait;
  closing during startup quits without discarding recovery data. No splash or
  window for `--help`, `--version`, `--convert` or a secondary instance that only
  forwards files. Limitation: eframe keeps the window hidden until its first
  painted frame, so native window/GPU initialization before that frame (about
  0.3–0.4 s warm on Windows x64, longer cold) cannot be covered without a second
  native window and renderer.
- P2 Real cloud APIs (OneDrive/Google Drive/Dropbox) beyond synced folders, conflict resolution UI
- P3 Real-time co-editing (CRDT, e.g. `yrs`) — basis for a companion web/mobile app
- P3 Plugin API (WASM) for importers/exporters

## Quality
- P1 Visual regression tests (kittest snapshot images) per theme
- P2 Accessibility audit with screen readers (NVDA, VoiceOver, Orca)
- [x] P2 Fuzzing of the XMind/FreeMind/OPML importers (`cargo fuzz`; bounded CI smoke)
- P2 Performance budget: 10 000-topic map at 60 fps, benchmark in CI
- [x] Expanded UI localization: ES, FR, DA, NL, PT-PT, PT-BR, IT and PL, retaining
  EN/DE and Auto. Settings uses an accessible language dropdown with immediate
  switching and persistence. Catalogs cover UI, application messages and offline
  help; Windows x64/ARM64 installer controls, macOS Intel/ARM bundle language
  declarations and Linux DEB/AppImage launcher descriptions are wired. Native
  OS/driver and package-manager controls keep host translations. Template starter
  content stays English and editable; technical diagnostics retain original detail.
  Regression coverage checks all catalog keys, dynamic labels, interpolation
  contracts, locale variants, preference roundtrips and light/dark switching.
- P1 User-directed localization expansion: add Japanese, Simplified Chinese
  (`zh-CN`) and Traditional Chinese (`zh-TW`) in addition to the ten shipped
  locales. Translate all UI, help, installer and platform strings in context;
  verify font fallback, IME, shortcut handling, layout remeasurement, long-text
  behavior, light/dark/Auto, and all supported native targets plus applicable
  WASM workflows before advertising support.
- P2 Launching Thinoda again without files exits silently when an instance is
  already running; it should bring the existing window to the front instead.
- P3 GPU acceleration under WSL 2: wgpu falls back to the llvmpipe software
  rasterizer; evaluate the GL backend on Mesa's D3D12 driver.

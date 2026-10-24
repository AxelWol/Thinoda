_Last updated: 2026-10-23 06:58:00 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Architecture

```mermaid
graph TD
    app[thinoda-app<br/>shell, menus, dialogs, i18n] --> canvas[thinoda-canvas<br/>map view, input, rendering]
    app --> ui[thinoda-ui<br/>reusable design system]
    app --> io[thinoda-io<br/>import / export]
    app --> platform[thinoda-platform<br/>settings, recovery, cloud, OS]
    app --> ai[thinoda-ai<br/>OpenAI-compatible client]
    web[thinoda-web<br/>browser entry, wasm32] --> app
    canvas --> ui
    canvas --> core[thinoda-core<br/>model, edits, layout]
    io --> core
    platform --> core
```

| Crate | Responsibility | Depends on egui? |
|---|---|---|
| `thinoda-core` | Workbook/sheet/topic model, transactional edits + undo, layout engine for all structures, themes, templates, search, outline conversion | no |
| `thinoda-io` | `.thinoda`, XMind, FreeMind, OPML, Markdown, text; bounded MindManager `.mmap` read/write and `.xmmap` import with conversion reports; SVG/PNG/JPEG/PDF/Word/Excel/CSV export; format detection | no |
| `thinoda-platform` | Paths, settings (TOML), atomic file writes, crash recovery journal + panic hook, cloud folder detection, single instance, system theme; on wasm a `localStorage` virtual file system, downloads and browser printing | no |
| `thinoda-ai` | Blocking HTTP client (async `fetch` client on wasm) for `/v1/models` + `/v1/chat/completions`, prompts, outline parser, background `AiJob`, mock server for tests | no |
| `thinoda-ui` | Design tokens, light/dark visuals, widgets, palette, toasts, dialogs, icons, fonts — **the reusable library for companion apps** | yes |
| `thinoda-canvas` | Interactive mind map widget: hit-testing, selection, drag & drop, inline editing, keyboard actions | yes |
| `thinoda-app` | eframe application, commands (`Cmd`), menus, panels, dialogs, CLI | yes |
| `thinoda-web` | Static web app entry: mounts `ThinodaApp` on a canvas with `eframe::WebRunner`, unsaved-changes and recovery page handlers ([docs/WEB.md](WEB.md)) | yes |
| `xtask` | `cargo xtask ci`, `icons`, `package` | no |

## Key decisions
- **One code base for desktop and web**: the web app is the same `thinoda-app`
  compiled to `wasm32-unknown-unknown`. Platform differences live behind
  `thinoda-platform::fs` (std::fs vs. `localStorage` VFS), `thinoda-app::files`
  (blocking vs. async pickers, downloads) and `cfg(target_arch = "wasm32")`
  branches for printing and AI; see [WEB.md](WEB.md).
- **egui + wgpu**: immediate-mode UI keeps the whole app in Rust, renders through Direct3D 12,
  Metal or Vulkan, and is testable headlessly with `egui_kittest` (AccessKit tree).
- **Commands**: every menu item, shortcut and palette entry maps to one `Cmd`; e2e tests call
  `ThinodaApp::exec` or press real keys.
  `Cmd::shortcut_id()` supplies explicit, nonlocalized built-in identities such as
  `file.save`, independently of numeric palette row IDs. `shortcut_commands()` and
  `shortcut_command(id)` expose the shortcut target registry without adding
  palette actions; Close Tab resolves to the active tab at lookup time.
  Payload-specific paths/templates/styles/clipboard text are not generic targets.
  `edit.paste` resolves to `MenuPaste` (request OS clipboard), not stored content.
  Registry membership is not availability or key ownership: callers must still
  use the command/focus guards. `Settings.shortcuts_overrides` remains inactive
  and preserves stable, legacy, numeric and unknown keys without guessing a
  migration. Activating overrides still requires a versioned binding format,
  conflict/reserved-key policy and explicit editor/reset behavior.
- **Crash safety**: edits are journaled to the recovery directory every few seconds and on
  panic; saves are atomic (temp file + rename). On start the recovery dialog restores unsaved work.
  Native recovery locks explicitly unlock before closing their file descriptor:
  a concurrently forked child's duplicate must not delay orphan discovery after
  the owning session releases its lock. Orphan documents retain a shared lease
  until restoration/discard or the last handle is dropped.
  Suspension flushes while holding ownership and releases only on success.
  Suspended managers may queue in-memory work, but cannot flush/remove snapshots
  before resuming; cleanup must reacquire ownership so it cannot delete another
  process's claimed orphan.
  Restore validates every snapshot in a selected session before opening any of
  its maps or deleting its snapshots. Failed sessions stay claimed and visible
  for retry; other selected sessions can still restore. Explicit discard retains
  only documents whose deletion failed, with visible errors and logging.
  Post-restore cleanup errors are reported rather than silently ignored.
  Before opening restored maps and removing their originals, the app synchronously
  persists replacement snapshots in its current recovery session. Failed handoffs
  retain the original session for retry and attempt cleanup of new snapshot keys.
  The platform handoff writer holds session ownership and never queues a failed
  write for later resurrection. If recovery is unavailable, maps can still open,
  but their original snapshots remain on disk and a warning is shown.
  Focused Notes/Hyperlink drafts belong to `OpenDoc`, not only egui's temporary
  text buffer. They contribute to dirty/close detection and idle tracking.
  Commands, saves and native close handling commit them as one undo step;
  recovery overlays their current value on a copy without committing history.
  Missing file fingerprints or failed metadata checks count as an unverifiable
  external change: interactive saves require conflict resolution and autosave
  skips the write. Failed saves and canceled MindManager save consent revoke
  overwrite authorization so a later attempt asks again.
  Save As compares absolute/canonical destination identity, not just literal
  path spelling; aliases of the current file retain conflict protection.
  Source-preserving sheet saves/conversions use the same identity check and
  propagate identity errors instead of silently classifying them as distinct.
  Tab-close prompts and save continuations target the stable document key,
  never a retained vector index. Conflict/conversion decisions resume that same
  close only after a successful save leaves no unsaved work. Cancellation,
  failure or removal of the target ends the continuation, so subsequent saves
  cannot revive it or close a replacement tab.
  Drafts bind to the document, sheet index/root and topic, since duplicated
  sheets can share topic IDs. Changed or missing source fields fail explicitly
  and retain the draft instead of overwriting newer content. Single-line
  Hyperlink Escape still cancels rather than commits.
  Command-boundary flushes invalidate committed UI buffers, and unchanged field
  buffers follow model updates, so Undo cannot resurrect stale text. Toolbar
  Undo/Redo availability accounts for pending inspector changes.
- **Text input ownership**: Map and Outline use `MapView`'s per-edit IME
  state with the shared `protect_ime_candidate_keys` guard before drawing their
  title text widgets. Candidate
  Enter/Tab/Escape cannot trigger document commit or insertion, including the
  final composition-event frame. Dismissal restores ordinary title shortcuts;
  the Outline editor locks focus while its explicit Tab handling owns insertion.
  Labels keeps independent composition state and runs the same guard before
  menus can dismiss its popup. Its input locks focus only during composition,
  so ordinary Tab navigation resumes after dismissal; candidate keys cannot add
  provisional labels or discard the draft.
- **Toolbar focus**: shared icon/captioned toolbar buttons keep egui's standard
  keyboard activation and draw a two-logical-pixel focus outline in the semantic
  text color, independent of selection or user accent. Disabled parent UIs also
  disable captioned controls' accessibility state and interaction.
- **Touch navigation ownership**: `MapView` latches an owned or rejected
  multitouch sequence until all contacts lift. Owned two-finger input scales
  about the previous centroid, then translates to the current centroid. It
  bypasses the wheel/zoom fallback so synthesized zoom is not applied twice.
  Multitouch or touch cancellation discards pending topic/relationship drags and
  suppresses click/drop actions for that sequence. Single-finger blank-background
  dragging pans; mouse blank-background dragging retains rubber-band selection.
  Three-or-more-finger, popup and disabled states cancel ownership rather than
  acquiring it halfway through a gesture. No gesture creates a navigation undo
  entry. Hosts use `CanvasOutput::context_menu`, not the raw response's menu:
  it applies the same ownership guard to egui's synthesized long-touch secondary
  clicks and closes menus for suppressed sequences. Single-touch holds retain
  the upstream recognizer; no duplicate timer or platform gesture is installed.
  Canvas ownership survives the final raw touch until the synthesized primary
  pointer also releases, preventing late click-through. The shared
  `widgets::touch_activation_allowed` cancellation/multitouch latch is sampled
  each app frame, including while controls are hidden, and cached across layout
  passes. Outline title/disclosure/command activation and shared switches,
  segmented choices and color swatches use it. Symbol insertion/removal, default-
  color/contrast actions and color application share that guard in the inspector
  and context instance editor. Scroll areas and text inputs keep
  upstream scrolling/text-selection behavior. Preview cards gate pointer
  activation with the same latch while retaining explicit keyboard activation;
  template double-click creation requires that guarded activation too. The
  structure launcher's popup toggle is guarded independently. This does not yet
  guard every standard egui button, slider or combo box. Sheet strip selection,
  creation, rename, arrows, context opening and commands share the latch without
  disabling scrolling or replacing egui's single-finger hold recognizer.
  The custom-theme editor retains a full staged-theme/role snapshot across each
  pointer sequence. Cancel, multitouch, focus loss or skipped editor frames restore
  its starting values through release, including changes made on earlier drag
  frames; rejected releases cannot save/apply or dismiss it via outside clicks.
  Settings pointer edits live in a separate staged profile until clean release;
  ordinary app behavior and settings writes continue to see the committed profile.
  Cancel, multitouch, focus loss, a disabled/hidden window, skipped frames or an
  intervening profile change reject the draft without replacing newer preferences.
  Language/title defaults update only after acceptance. Credential save/clear,
  connection tests, folder/install actions and shortcut navigation require accepted
  activation; rejected input retains the independent API-key text draft.
  Shared confirmation dialogs gate every button kind and backdrop dismissal
  with the same touch latch, so rejected releases cannot authorize saving,
  discarding, overwriting or canceling a pending decision. Clean mouse/touch
  and keyboard activation retain the normal confirmation behavior.
  Theme-library edit/remove launchers and quick-style save/remove launchers
  (including the removal menu action) share this latch. The quick-style name
  dialog also gates Save, Cancel and backdrop dismissal, preserving typed
  draft text and persisted library contents across rejected releases.
  Sheet-rename decisions use the same latch and a draft owner composed of the
  document key, active sheet index and model revision. A changed owner dismisses
  the dialog with a warning instead of renaming the current replacement target.
  IME candidate keys retain their existing protection before final confirmation.
  Print-preview output, retry and close decisions also share the latch,
  including backdrop dismissal; canceled input cannot initiate a print/PDF
  handoff. Page-setup gestures retain their starting settings through the primary
  pointer sequence; cancel, multitouch, focus loss and skipped dialog frames
  restore that snapshot through release without disabling page setup.
  Screenshot-editor crop, square/frame/color settings retain the same
  sequence-scoped rollback contract. Rejected releases cannot copy/save output
  or dismiss the editor; a canceled crop clears its provisional drag origin.
  Label-popup add/remove/suggestion/sort/apply/cancel actions use the touch latch;
  rejected outside releases retain the popup and its typed draft.
  Symbol-color and MindManager export consents bind to document key, sheet and
  revision after committing title edits. Switching/removing/changing the source
  revokes both decisions and symbol-loss authorization; tab-vector reordering
  alone does not transfer consent to the tab now occupying its old index.
  Masked-copy preview consent uses the same document/sheet/revision boundary;
  rejected touches cannot create or cancel a preview. Guide/shortcut filters,
  legal-document selection/copy/links and auxiliary dialog decisions share the
  cancellation latch without changing text selection or typed searches.
  Backend/browser and real-device acceptance is tracked in PLAN/WEB.
- **AI off the UI thread**: `AiJob` runs on a worker thread; the app polls it each frame and applies
  the result as one undo step. Works with any OpenAI-compatible server; nothing leaves the
  machine when a local server (LM Studio, Ollama) is used.
  Topic requests capture document key, active sheet/root and revision after title
  commit. Results for closed, changed or draft-editing sources are rejected with
  a visible warning; unchanged background documents retain their original target
  without activating a different tab. Disabling AI drops pending result ownership
  so re-enabling it cannot revive that result; already-running bounded HTTP work
  is not forcibly aborted.
  Connection-test work and model lists belong to their endpoint and credential
  draft; changing either discards them, while choosing a model keeps the list.
  Generate requires accepted activation and retains rejected prompt drafts.
  Its stable text editor protects IME candidate keys before global shortcuts;
  deliberate Escape dismisses the prompt only after composition has ended.
- **Printing**: `thinoda-io::print` fits the complete current-sheet export SVG onto
  physical paper, rendering directly at 150/300 dpi with bounded page allocation
  (no viewport screenshots or full-map intermediate raster clipping). The app
  freezes a workbook snapshot after committing title edits, runs rendering/encoding
  on workers and rejects stale previews after settings changes. Preview, native
  printer page and paper-sized PDF use the same raster. `thinoda-platform::printing`
  validates dimensions and converts RGB to bounded opaque BGRA on a worker.
  The eframe adapter borrows its live window handle and invokes a compact native
  C++/C/Objective-C bridge synchronously on the main/window thread, never a rendering
  worker. Windows uses `IPrintManagerInterop` for the HWND and a WinRT
  `IPrintDocumentSource`/`IPrintDocumentPageSource` with a
  `IPrintPreviewPageCollection`. Pagination reads the selected printer's
  `PrintTaskOptions.GetPageDescription(1)`; Direct2D paints the full source page
  inside that page's imageable rectangle. DXGI surfaces populate the actual
  Windows system preview; `ID2D1PrintControl` writes the same drawing to the OS
  package target only when Windows requests `MakeDocument` after confirmation.
  D3D11 WARP avoids dependence on the application's GPU and supports x64/ARM64.
  macOS uses the AppKit modal printer/driver panel and NSPrintOperation;
  Linux uses GTK 3 PrintOperation, Cairo and desktop printer backends. No generated
  HTML, browser/viewer launcher or temporary print files remain. Native buffers are
  borrowed only for the synchronous call; the Windows COM source owns a pixel
  copy, serializes callbacks and disconnects retained preview targets on exit.
  Late callbacks return `RO_E_CLOSED`. Its STA modal message loop keeps the owner
  disabled until PrintTask completion, unregisters handlers and restores the
  owner's enabled state. Only `PrintTaskCompletion.Submitted` means submission;
  cancelled/abandoned tasks are cancellation, and HRESULT failures are errors.
  Showing the native UI alone is never reported as submission.
  The app preview stays modal during
  preparation, native UI and submission; cancellation retains it for retry.
  Native cancellation, submission acceptance and errors are distinct; no result
  confirms physical printing. OS paper changes fit the entire fixed preview page
  proportionally into the selected printable area without cropping/reflow.
  Driver options may further affect output. Linux respects page-setup and reported
  hardware margins; no hardware information means the selected setup is used.
  Native dialogs follow OS language/theme rather than app overrides.
  GTK cannot use a winit Wayland surface as a GtkWindow parent: the blocked app
  event loop guarantees editor modality, but compositor taskbar grouping may differ.
  Build requirements are the target's C toolchain, macOS AppKit SDK, or Linux GTK 3
  development files (`libgtk-3-dev`, `pkg-config`). These are already installed by CI.
  Linux runtime needs GTK 3 with a CUPS backend for physical printers.
  GTK/Cairo symbols are resolved from the host's `libgtk-3.so.0` (including its
  dependency scope); missing libraries/symbols are explicit errors. This keeps
  the existing AppImage packager from bundling a partial GTK without its CUPS
  modules or mixing bundled GTK and host modules. `.deb` already declares GTK 3;
  AppImage users also need host GTK 3. Build-time pkg-config supplies headers
  only, not a direct GTK link. Windows/macOS use their system APIs/frameworks.
  Unit/UI tests never submit jobs: they cover the same native fit function,
  bounded conversion, outcome mapping, immutable snapshots and modal behavior.
  Windows SDK-native tests additionally read back WARP preview pixels for
  page/imageable-area changes, retain interfaces after disconnect, distinguish
  completion states and serialize/reopen a one-page XPS package entirely in a
  memory stream (no printer/spooler). The Python native harness runs on Windows
  x64/ARM64 CI with strict C++ compilation. See [native printing](NATIVE-PRINTING.md)
  for API references, manual checks and platform limitations.
  Native driver behavior still requires smoke tests on Windows x64/ARM64,
  macOS Intel/ARM64 and Linux X11/Wayland.
- **Localization**: app chrome uses the embedded English/German and
  ES/FR/DA/NL/PT-PT/PT-BR/IT/PL catalogs in `thinoda-app/src/i18n.rs`,
  `locales.txt` and `pt-BR.txt`. English strings are stable keys; the
  pipe-delimited catalog has an explicit translation in each column (the header
  specifies order). Brazilian Portuguese deliberately shares common Portuguese
  text with explicit regional overrides. Completeness tests cover literal calls
  and dynamically selected labels; missing runtime keys are logged before English
  fallback. Add a key to every catalog, preserve extension/shortcut tokens and
  review terminology in its actual UI. Autonyms in the language dropdown are not
  translated. Settings keeps stable snake-case enum values; Auto resolves complete
  locale subtags. Installer translations live in `assets/installer`; pinned
  packager contracts are checked by `.github/scripts/test_installer_languages.py`.

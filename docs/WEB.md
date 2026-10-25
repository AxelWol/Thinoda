_Last updated: 2026-10-24 12:13:23 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Thinoda for the web

Thinoda also runs as a static web app. The same Rust crates as the desktop
app are compiled to WebAssembly (`wasm32-unknown-unknown`): model, layout,
canvas, import/export, AI client, dialogs, translations and themes. The site is
plain files (HTML, JavaScript glue, `.wasm`), so any static host can serve it.
There is no server-side code; your maps never leave the browser unless you
download them or use an AI server.

## Build and run locally
```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked        # or download a release binary
cd crates/thinoda-web
trunk serve                         # http://127.0.0.1:8080, rebuilds on change
trunk build --release               # static site in crates/thinoda-web/dist
```
Trunk downloads `wasm-bindgen` and `wasm-opt` on first use. The build uses
relative URLs (`public_url = "./"` in `Trunk.toml`), so `dist` works from any
sub path. WebGL 2 is required; Thinoda renders through wgpu's WebGL backend.

## Publishing on GitHub Pages
The web app is built and deployed by the `CI` workflow
([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)):

| Job | Runs on | What it does |
|---|---|---|
| `web` | every push and pull request | wasm32 Clippy for all browser crates, `trunk build --release --locked`, uploads `dist` as the `thinoda-web` artifact; on `main` also packages it as the Pages artifact |
| `pages` | pushes to `main` | after `web` and all native `test` jobs pass, deploys the site with `actions/deploy-pages` to the `github-pages` environment |

The site is published at <https://axelwol.github.io/Thinoda/>. One-time setup:
in **Settings > Pages**, set **Build and deployment > Source** to
**GitHub Actions**. Pull requests build the site but never deploy it; download
the `thinoda-web` artifact from the workflow run to try a pull request locally
(`python -m http.server -d <unzipped folder>`).

CI installs Trunk 0.21.14 with
[a checksum-pinned installer](../.github/scripts/install_trunk.py), verifying the
Linux archive before parsing it and accepting only its single regular `trunk`
binary. The expected SHA-256 is committed, not fetched alongside the executable
at build time. Installation uses a temporary runner directory without `sudo`.
Changing Trunk requires reviewing the upstream release and updating the version,
checksum and fixtures together. This verifies the pinned archive bytes; it does
not establish upstream build provenance or cover Trunk's separately downloaded
`wasm-bindgen`/`wasm-opt` tools.

## Feature parity and browser alternatives
All editing features are the same shared code as on the desktop: topics,
structures, themes, styles, symbols, relationships, boundaries, summaries,
notes, labels, links, images, search, outline, templates, multiple sheets and
documents, undo/redo, Map Shot, AI tools, settings, languages and shortcuts.
Browsers cannot access files, printers or OS services directly, so these
features work differently:

| Desktop feature | In the browser |
|---|---|
| Files on disk | Documents are kept in the browser's local storage under `/Documents` (a small virtual file system). **Open Recent** and autosave work with these copies. |
| Open / Import / Merge dialogs | The browser's file picker. The chosen file is copied into `/Documents` (`name (2).ext` if a different file has the same name). |
| Drag & drop onto the window | Supported. Dropped files are copied into `/Documents` and opened. |
| Save As, first save, Save Sheet As | Asks for a file name, stores the document and **downloads** it. A plain Save of an already named document only updates the browser copy; use Save As to download again. |
| Export (PNG, PDF, Word, Excel, XMind, …) | Generated in the browser, then downloaded. Graphical exports use font-measured node text; PDF contains vector graphics and outlined text, not a page-sized bitmap. |
| Map Shot "Save PNG" | Downloaded. Copy to clipboard uses the browser clipboard. |
| Native print dialog | **Continue to print…** renders the same print page and opens the browser's print dialog. Select the same paper size and orientation there and set margins to none/default. **Save print PDF…** downloads a paper-sized PDF for exact output. |
| Crash recovery | The recovery journal is flushed to local storage every half second and when the page is hidden. After a reload, a crash or closing the tab, the next visit offers **Recover Unsaved Work**. |
| Unsaved changes on quit | Closing or reloading the tab with unsaved changes asks for confirmation. **Quit** shows a hint to close the tab instead. |
| OS keychain for the AI API key | The key is kept in session storage for the current tab only and is not written to local storage. |
| AI server | Requests use `fetch`. The server must allow cross-origin requests (CORS) from the site's origin; LM Studio has a CORS option in its server settings. An `https` site can only call `https` servers or `http://localhost`. |
| Open link | Opens a new browser tab. |
| Fonts | System font enumeration is unavailable; the bundled Ubuntu and Hack fonts are used for rendering and export. |
| Show in folder, settings folder, cloud folder picker | Not available in the browser; hidden in the UI. |
| Single instance, file associations, CLI `--convert` | Not applicable. Each tab is independent. |

## Touch and trackpad coverage

The shared canvas supports one-finger dragging on empty background to pan, and
two-finger pan plus simultaneous pinch zoom. The map follows the fingers' center
instead of zooming around the first finger. Zoom stays within 10%-500%.
Mouse background drags still select a rectangle; dragging a topic is an edit.
Adding a second finger cancels a pending edit drag and keeps the rest of that
touch sequence in navigation mode until every finger lifts. Touch cancellation
discards pending topic/relationship drops. Starting outside the canvas or in
disabled UI does not acquire map navigation; disabling it during a gesture
requires lifting all fingers before starting again. Three-or-more-finger
sequences do not acquire map navigation, avoiding app mappings for system
multifinger gestures.

Tap a topic to select it, double-tap to edit, or hold one finger for its context
actions. Two/three-finger holds do not open context menus or mutate maps, including
after returning to one finger until every contact lifts. This uses egui's standard
long-touch recognizer, not a separate platform-specific timer.

In Outline, one-finger swipes scroll deep content horizontally and vertically
without activating rows. Canceled and two/three-finger sequences cannot activate
disclosure buttons, titles or the command toolbar. Both Outline and canvas keep
the rejection guard through a synthesized primary-pointer release delivered after
the raw touch contacts end, preventing late click-through.

Menu and Settings first-swipe scrolling has shared-input coverage in both themes
at 1x/1.5x/2x scale; repeated export-menu swipes reach the last format without
executing it. Settings switches, segmented theme choices and color swatches reject
canceled and two/three-finger taps, including delayed pointer releases; normal
click/keyboard activation is preserved. Other button/slider/combo interactions
and actual browser touch delivery remain separate acceptance work.

Symbols-inspector first swipes also scroll without applying markers. Canceled
and two/three-finger releases cannot insert/remove symbols or invoke default-color
and contrast actions; color application shares the guard. Automated both-theme/
scale coverage preserves ordinary keyboard/click actions and Undo. Other
inspector property controls and actual device delivery remain open.

Open-document tabs follow the same compact browsing model as sheet tabs: fixed
left/right arrows scroll clipped names while Close Tab and New Tab stay pinned.
Touch/mouse browsing does not switch documents; keyboard focus and active-tab
changes reveal the selected file. Narrow layouts prioritize these controls over
quick-action icons that remain in the main menu.

Style, structure, theme and template preview cards reject canceled/multitouch
pointer activation. Template double-tap creation and the structure launcher's
popup toggle use the guard too. Enter/Space activation remains available.
Actual-app both-theme/scale regressions cover unchanged map history and template
document count, ordinary application and Undo; real browser delivery remains open.

Status view/zoom/Fit controls, including zoom-menu presets and Fit, reject canceled
and two/three-contact releases. Zoom/Statistics popup opening uses the same guard.
Both-theme/scale app regressions check unchanged camera zoom/offset, view and map
history, then ordinary tap/click/keyboard recovery. This is shared-input coverage,
not real-browser or hardware acceptance.

Sheet creation/selection/rename, scroll arrows and sheet context commands also
reject canceled/multitouch activation. Single-finger holds still open sheet
context actions; two/three-finger holds do not. Both-theme/scale app regressions
verify unchanged active sheet/content/history and normal click/keyboard/Undo
recovery. Status controls and main-menu commands are not fully audited yet.

Trackpad scroll and zoom events can apply together. OS/browser gesture delivery
varies; these shared input behaviors are not a claim of full hardware support.
Keyboard shortcuts and visible zoom/Fit controls remain alternatives.

| Acceptance surface | Current evidence / remaining work |
|---|---|
| Shared native/WASM canvas | Raw touch and synthesized-pointer tests cover pan/pinch math, zoom limits, finger changes, cancellation, outside/disabled ownership, unchanged maps/history, normal clicks afterward and trackpad scroll plus zoom. Tap selection, double-tap editing and single-touch context holds are covered; canvas and full-app regressions reject two/three-finger context holds without changing map history. |
| Windows touch / precision trackpad | Shared native code checked on Windows; real touchscreen/trackpad, press-and-hold context actions and pen coexistence remain open. |
| macOS trackpad | Native scroll/zoom event path preserved; real-device natural scroll, pinch, momentum and system-gesture acceptance remain open. |
| Ubuntu Wayland / X11 | Separate touch/touchpad/backend runtime acceptance remains open; libinput touchpad support alone does not prove touchscreen behavior. |
| iPhone/iPad Safari and Android Chrome, browser/PWA | Shared WASM code compiles; real browser gesture delivery, pointer cancellation, text input, orientation and installed-PWA behavior remain open. No native iOS/Android package is implied. |
| All UI surfaces and accessibility | Shared canvas tap/double-tap/hold, sheet/menu/Settings/Symbols-inspector swipes, Outline release safety and guarded Settings/Symbols actions have automated coverage. Device-delivered gestures, topic moves, remaining inspector properties, touch targets and AT/OS-reserved gestures still need the full acceptance matrix in PLAN. |
| Browser zoom ownership | The viewport no longer caps or disables browser-controlled zoom. A pinch on the full-screen canvas belongs to map navigation; browser controls/keyboard zoom the page, and visible app controls zoom the map. Eframe prevents browser defaults for canvas touch events. Verify these paths in browser/device testing; map pinch alone is not page zoom or proof of low-vision acceptance. |

See [the gesture workstream](PLAN.md#immediate-next-work) for official platform
references and remaining tasks. Do not disable system accessibility gestures or
add custom three/four-finger shortcuts to work around missing platform delivery.

## Install Thinoda as an app (PWA)

The web build includes a web app manifest, standard 192/512 px icons, an opaque
maskable icon and 152/167/180 px Apple touch icons. Install from **Settings > General >
Install web app**, or from the browser's own menu. The Settings action is
web-only; it opens a keyboard-accessible installation panel, never silently
installs software. HTTPS is required outside localhost/loopback; opening
`index.html` directly from disk is not supported.

| Platform | Installation |
|---|---|
| Windows / Linux | Chrome or Edge: use the Settings panel's install button when offered, or the browser menu/address-bar install action. Other browsers may not offer installation. |
| macOS | Safari on supported macOS versions: Share > Add to Dock (also available from File in some versions). Chrome/Edge can offer their own install prompt. |
| iOS / iPadOS | In Safari, Share > Add to Home Screen. The system owns this flow; websites cannot trigger Apple's installation sheet programmatically. |

The panel always provides manual instructions when no browser install event is
available, including already-installed/standalone use. Dismissing the browser
prompt does not install anything or cause repeated prompts; a new browser event
is needed to offer it again. This is browser installation, not App Store
distribution, native file associations or native multi-window support.

### Offline use and updates

After a successful complete online load, the service worker precaches the built
app shell, including the matching JavaScript, WASM and imported snippets. It
caches **only build assets**, not maps, AI responses or other sites. Documents
and recovery remain in the existing browser virtual file system. Offline
editing works while that shell and site storage remain available; online AI,
external links and remote services still need a connection.

Trunk's post-build hook (`cargo xtask web-pwa`) creates `dist/sw.js` from the
exact staged artifacts and a content-derived revision. URLs, manifest identity,
worker scope and caches are relative to the deployment subpath, including
GitHub Pages. A new worker waits until all existing app windows/tabs close
before taking over; there is no forced reload or `skipWaiting` during editing.
Save/download maps before closing and reopen online to receive updates.
Activation removes only obsolete Thinoda shell caches for the same scope.
Registration/cache failures appear in the browser console and install panel;
installation alone is not proof that offline caching succeeded.

Installation does **not** provide unlimited storage, synchronization or backups.
Safari may isolate an installed app's storage from the original browser tab;
do not assume existing maps transfer. Download maps before switching contexts,
and open the downloaded files in the installed app. Browser eviction, private
browsing, site-data clearing or uninstall options can remove local copies.

References: [MDN installability and browser support](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Guides/Making_PWAs_installable)
and [Apple Safari web apps](https://support.apple.com/guide/safari/use-web-apps-ibrw9e991864/mac).
Actual installation, icon appearance and platform assistive technology still need
Windows/Linux/macOS/iOS/iPadOS acceptance; shared assets do not prove OS acceptance.

## Storage limits and privacy
Local storage is per site origin and typically limited to about 5 MB. Large
maps or many embedded images can exceed it; Thinoda then reports a save error,
and you should download the map (Save As or Export) to keep it. Clearing site
data in the browser deletes stored documents, settings and recovery data.
GitHub Pages serves all project sites of an account from the same
`<owner>.github.io` origin, so they share one storage quota. Thinoda stores its
documents, settings and recovery data under `thinoda.`-prefixed keys to avoid
collisions with other sites on that origin.

## Troubleshooting: "Thinoda could not start"
Thinoda prefers WebGPU and falls back to WebGL 2. Browsers usually still offer
WebGL 2 through software rendering when the GPU is unavailable, so this error
means WebGL is blocked entirely. The start page lists these steps and keeps
the raw error under **Technical details**:
1. Enable **Use graphics acceleration when available** (`edge://settings/system`
   or `chrome://settings/system`) and restart the browser.
2. Check `edge://policy` / `chrome://policy` for `WebGLAllowed` or
   `HardwareAccelerationModeEnabled` set by your organization.
3. Try a private window without extensions (privacy extensions often block WebGL).
4. Update the graphics driver; `edge://gpu` / `chrome://gpu` shows WebGL 2 status.
5. On Linux, Chromium-based browsers (Edge, Chrome) do not enable WebGPU by
   default and often block the GPU (older Mesa or NVIDIA drivers, virtual
   machines). Update the driver, enable 3D acceleration in the VM, or as a
   test enable **Override software rendering list**
   (`edge://flags/#ignore-gpu-blocklist`).
6. Test <https://get.webgl.org/webgl2/>. If it fails too, the cause is the
   browser or system, not Thinoda.

## Architecture notes
- `crates/thinoda-web` is only the entry point: it mounts `ThinodaApp` on the
  `thinoda_canvas` element with `eframe::WebRunner`, installs `beforeunload`
  (unsaved changes) and `pagehide`/`pageshow` handlers
  (`RecoveryManager::suspend`/`resume`).
- `thinoda-platform::vfs` is a key-value file system over `localStorage`;
  `thinoda-platform::fs` routes reads, atomic writes, renames and locks to it
  on wasm and to `std::fs` on desktop. Desktop behavior is unchanged.
- `thinoda-platform::web` builds the print page HTML, sanitizes download names
  and triggers downloads and the print dialog.
- `thinoda-app::files` hides the difference between blocking desktop dialogs
  and asynchronous browser pickers: picked files arrive in an inbox that the
  app drains each frame.
- `thinoda-ai::web` is an async `fetch` client with the same API as the
  blocking desktop client; `AiJob` uses it on wasm.
- Time comes from `web-time` and background work runs inline on wasm because
  browsers do not allow `std::thread` without special headers.
- The web entry point installs pointer feedback for enabled, hovered click-only
  controls, including custom tabs and buttons. Text, resize and drag cursors
  retain precedence; desktop cursor behavior is unchanged.

## Testing
- Use Node.js 24, selected by the repository root `.node-version` file. CI reads
  that same file through the Node-24-based `actions/setup-node` action; the PWA
  tests reject a different Node major. No npm install is needed.
- `node --test crates/thinoda-web/tests/pwa.test.cjs` (Node 24, repository root)
  checks real PNG dimensions/manifest scope, install/dismiss/error/standalone
  states, and offline worker cache/update isolation without installing software.
- `cargo test -p xtask` checks opaque maskable safe zones, complete/versioned
  shell generation and icon generation. `cargo xtask icons` regenerates PWA
  assets together with the existing desktop icons.
- `cargo test -p thinoda-web` exercises browser hover feedback on the shared
  widgets, including disabled controls, theme changes and specialized cursors.
- `cargo test --workspace` covers the shared code, including the virtual file
  system, print page HTML, file name sanitizing, upload naming and recovery
  suspension, all of which are platform independent.
- `cargo clippy --target wasm32-unknown-unknown -p thinoda-web -p thinoda-app
  -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings` checks the
  browser-only code paths.
- Browser smoke test (manual or scripted): load the site, create a map from a
  template, add a topic, save (download), reload and confirm the recovery
  prompt, open the downloaded file, print and save the print PDF.

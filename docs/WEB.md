_Last updated: 2026-10-19 10:21:54 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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
| Export (PNG, PDF, Word, Excel, XMind, …) | Generated in the browser, then downloaded. |
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

## Testing
- `cargo test --workspace` covers the shared code, including the virtual file
  system, print page HTML, file name sanitizing, upload naming and recovery
  suspension, all of which are platform independent.
- `cargo clippy --target wasm32-unknown-unknown -p thinoda-web -p thinoda-app
  -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings` checks the
  browser-only code paths.
- Browser smoke test (manual or scripted): load the site, create a map from a
  template, add a topic, save (download), reload and confirm the recovery
  prompt, open the downloaded file, print and save the print PDF.

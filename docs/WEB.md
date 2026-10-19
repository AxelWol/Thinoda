_Last updated: 2026-10-19 09:22:45 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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

## Publishing on GitLab Pages
[`.gitlab-ci.yml`](../.gitlab-ci.yml) defines the pipeline:

| Job | Stage | What it does |
|---|---|---|
| `web-check` | test | wasm32 Clippy for all browser crates; native tests for the browser helpers |
| `web-build` | build | `trunk build --release --locked`, keeps `dist` as an artifact |
| `pages` | deploy | copies `dist` to `public`, adds `.gz` copies, publishes (default branch only) |

The source repository is on GitHub. To publish:
1. Create a GitLab project and set up **Settings > Repository > Mirroring
   repositories** to pull from `https://github.com/AxelWol/Thinoda`, or push
   the branch to GitLab directly.
2. Run the pipeline on the default branch. The site appears at
   `https://<namespace>.gitlab.io/<project>/` (see **Deploy > Pages**).

GitHub CI also builds the site in the `web` job and uploads it as the
`thinoda-web` artifact, so web build breakage shows up in pull requests.

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
GitLab Pages gives every project its own origin only if
[unique domains](https://docs.gitlab.com/user/project/pages/#unique-domains) are
enabled; otherwise sites in the same namespace share one storage origin.

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

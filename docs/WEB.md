_Last updated: 2026-11-03 18:29:20 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Thinoda for the web

Thinoda also runs as a static web app. The same Rust crates as the desktop
app are compiled to WebAssembly (`wasm32-unknown-unknown`): model, layout,
canvas, import/export, AI client, dialogs, translations and themes. The site is
plain files (HTML, JavaScript glue, `.wasm`), so any static host can serve it.
There is no server-side code; your maps never leave the browser unless you
download them, use an explicitly configured AI server, or explicitly connect
and upload to a cloud storage provider.

The shared Notes inspector includes the bounded offline equation-preview editor
(fractions, groups, superscripts/subscripts), without browser MathML, downloaded
fonts or a TeX engine. Applied source stays in native v4 Markdown notes for
save/reopen/recovery/copy; the scratch editor must be applied before it is durable.
This is not a map/Outliner-row equation. Non-native/visual/Pitch output, printing
and Map Shot refuse equation-bearing content rather than omit it. See
[the equation slice and exact syntax](XMIND-USER-GUIDE.md#equation).

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
The custom Trunk bootstrap pattern uses the generated wasm-bindgen loader and
default preloads, and reports initialization failures before the Rust UI is
available. Preserve `window.wasmBindings` and the `TrunkApplicationStarted`
event when changing it. The bootstrap requires Trunk's generated integrity
metadata; builds with `--no-sri` or `TRUNK_BUILD_NO_SRI=true` fail closed.

Integrated remote cloud accounts are disabled in the default build while
publisher registrations are pending. OPFS/IndexedDB document storage, quota
and persistence controls, local file pickers and downloads remain available.
Only an intentional `trunk build --release --features integrated-cloud-services`
enables account integration, and it still requires publisher registrations;
see [CLOUD-AUTH-SETUP.md](CLOUD-AUTH-SETUP.md). Pages builds do not pass that flag.

## Publishing on GitHub Pages
The web app is built and deployed by the `CI` workflow
([`.github/workflows/ci.yml`](../.github/workflows/ci.yml)):

| Job | Runs on | What it does |
|---|---|---|
| `web` | pushes to `main` and pull requests | wasm32 Clippy, a locked release build and Chromium startup/board regressions; uploads `dist` as the `thinoda-web` artifact and packages it for Pages on `main` |
| `pages` | pushes to `main` | after `web`, native `test` and Office validation pass, deploys the site with `actions/deploy-pages` to the `github-pages` environment |

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

The **Auto** language uses the browser's preferred language at startup or when
switching back to Auto, with English as the fallback for unsupported languages.
An explicit Settings choice takes precedence; reload after changing the
browser's language preference. The static startup and bootstrap-error messages
remain in English.

Browsers cannot access files, printers or OS services directly, so these
features work differently:

| Desktop feature | In the browser |
|---|---|
| Files on disk | Documents under `/Documents` use OPFS with an IndexedDB fallback. **Open Recent** and autosave use these site-private copies; settings and recovery retain the synchronous virtual file system. |
| Open / Import / Merge dialogs | The browser's file picker. Open and Import copy files into `/Documents`; Merge uses a temporary browser input. Name collisions receive a suffix. |
| Drag & drop onto the window | Documents, including explicitly named `.drawio.png` files, are copied into `/Documents` and opened. Ordinary PNG/JPEG drops stay in memory and insert an image on the selected topic or as a board image card. A `.png` with embedded draw.io data can instead be opened as a diagram through File > Open. |
| Save As, first save, Save Sheet As | Asks for a file name, stores the document and **downloads** it. Plain Save updates the browser copy and, for a file explicitly picked with Chromium File System Access in this tab, saves back to the original file. Firefox/Safari retain downloads; use Save As to download again. |
| Export (PNG, PDF, Word, Excel, XMind, …) | Generated in the browser, then downloaded. Graphical exports use font-measured node text; PDF contains vector graphics and outlined text, not a page-sized bitmap. |
| Map Shot "Save PNG" | Downloaded. Copy to clipboard uses the browser clipboard. |
| Copy Sheet | Copies the editable Thinoda sheet text and SVG in one clipboard item when the browser supports SVG clipboard data. Otherwise it warns and copies the sheet text only. Clipboard permissions and a secure context are required; receiving apps choose which representation to use. |
| Native print dialog | **Continue to print…** renders the same print page and opens the browser's print dialog. Select the same paper size and orientation there and set margins to none/default. **Save print PDF…** downloads a paper-sized PDF for exact output. |
| Crash recovery | The app attempts to flush recovery snapshots to local storage every half second and when the page is hidden. Available snapshots are offered under **Recover Unsaved Work** on a later launch. Storage failures or competing tabs can prevent recovery; save or download important work and keep external backups. |
| Unsaved changes on quit | Closing or reloading the tab with unsaved changes asks for confirmation. **Quit** shows a hint to close the tab instead. |
| OS keychain for the AI API key | The key is kept in session storage for the current tab only and is not written to local storage. |
| AI server | Requests use `fetch`. The server must allow cross-origin requests (CORS) from the site's origin; LM Studio has a CORS option in its server settings. An `https` site can only call `https` servers or `http://localhost`. |
| Open link | Opens a new browser tab. |
| Fonts | System font enumeration is unavailable; the bundled Ubuntu and Hack fonts are used for rendering and export. |
| Show in folder, settings folder, cloud folder picker | Native folder actions are hidden. Settings > Cloud offers browser storage usage/persistence controls; API connections are unavailable in default builds during the provider-registration pause. See [CLOUD-STORAGE.md](CLOUD-STORAGE.md). |
| Single instance, file associations, CLI `--convert` | Not applicable. Each tab is independent. |
| Separate native Gantt window | **View > Gantt Chart** opens an in-app window over the same document instead of a separate OS window. |
| Native Pitch window/fullscreen | **View > Pitch** opens an embedded player. **Expand Player** fills more of the tab, not the operating system's fullscreen mode. |

**Export > draw.io SVG (editable)** and **draw.io PNG (editable)** download an
image containing an editable draw.io diagram for the current sheet only. The
preview and the embedded diagram use the same sheet. Embedded content can extend
outside the visible preview, so these images are not redacted. Review the
conversion warnings and keep a `.thinoda` copy; unsupported content can block
export. These are export formats, not Save As or lossless save-back. See the
[draw.io plan and current limitations](DRAWIO.md).

Classic, open and block arrows, diamonds, ovals, half-circle and bar connector
markers, including thin variants where available, have independent start and
end sizes from 0 to 999 in the connector inspector. Classic and block arrows,
diamonds and ovals can be filled or hollow independently. Open arrows,
half-circles and bars remain open; their fill setting is retained when changing
directly to a fillable draw.io marker. Unsupported marker shapes, out-of-range sizes,
unsupported fill values, and fixed-port hints trigger a formatting warning.
Existing Thinoda triangle arrows retain their native appearance; draw.io export
warns that they will be approximated. Keep the original diagram.

Opening editable images also accepts older URI- or base64-encoded SVG diagrams
and older PNG diagram metadata. Ordinary PNG drops still add an image; a
`.drawio.png` drop opens the embedded diagram instead.

An `.html` or `.htm` file with one embedded draw.io viewer, including older
compressed or URI-encoded viewers, can be opened or dropped as an unsaved
converted copy. The page is treated as data: scripts and remote resources are
never loaded. HTML with remote-only viewers or multiple diagrams is not
imported; export each diagram as a separate editable file.

**File > Import draw.io link…** imports self-contained published `#R` links
or fragments locally, without contacting the link's website. The result is an
unsaved converted copy; review any conversion warning and save or download a
`.thinoda` copy. Links containing remote references such as `#U` are not
supported, and ordinary paste does not invoke this import.

Pasting draw.io diagram text inserts the first page as one undoable edit. A
persistent warning counts omitted first-page container frames and additional
pages; children of those frames become free board items. If the first page has
no pasteable items, the paste is rejected and later pages are not pasted. The
warning also reports connector attachments that may have been detached.

## Durable browser documents (C1 platform integration)

The platform provides `browser_storage::BrowserStorageProvider::new(backup)`
for asynchronous `/Documents` storage. **It is not a synchronous VFS
replacement.** The web binary awaits migration before app startup, and
`thinoda-app/src/files.rs` stores opened, imported and dropped documents in it.
Interactive document open/save and startup in `doc.rs` use this provider and
retain the exact read revision for conditional writes. Synchronous document
helpers reject durable paths instead of reading a stale VFS copy. Settings, crash
recovery, exports, images, attachments, merge/schema inputs and other scratch
files intentionally stay on the synchronous VFS.

The adapter prefers **Origin Private File System (OPFS)** blobs and falls back
to **IndexedDB bytes** if OPFS cannot be initialized. IndexedDB is also required
for OPFS's revision index: it atomically commits the document ID, SHA-256 revision,
and pointer to a fully written, closed, reread/verified immutable blob. A stale
conditional save never replaces the index. IndexedDB-only writes commit the
bytes and revision in the same readwrite transaction. A document's recorded
backend remains authoritative across later visits; failed OPFS reads never
silently return an empty file or an older IndexedDB/localStorage copy.

This is site-private local storage, not a cloud upload, sync-folder workspace,
or backup guarantee. It requires a secure context (HTTPS or loopback).
Explicitly clearing site data removes it; private browsing, browser policy,
quota, eviction and device/power loss can affect availability. Strict IndexedDB
transaction durability is requested where supported; older browsers use the
standard atomic transaction with browser-defined power-loss durability.
The IndexedDB database (schema v2) keeps metadata only in `documents` (indexed by
blob), so listing never loads document bytes. IndexedDB-fallback bytes live in a
separate `contents` store. A v1 database is upgraded in its versionchange
transaction, moving inline bytes into `contents`.

Superseded blobs are reclaimed, so repeated saves use bounded storage: the
current document, plus its `.bak` when backups are enabled. A blob is deleted
only when no record (including `.bak`, which shares the previous blob)
references it. Because blob names are never reused, an unreferenced blob cannot
become referenced again. IndexedDB contents are deleted in the commit
transaction. OPFS files are deleted after the commit, under the document's
exclusive Web Lock and the blob's exclusive lock. Readers hold a shared blob lock
while reading, and retry against the newer record if a blob disappeared.
Initialization also sweeps unreferenced OPFS blobs left by interrupted writes,
skipping any blob whose lock is held by an in-flight write in another tab.
Without Web Locks, superseded blobs are still reclaimed (readers retry), but the
crash-orphan sweep is skipped.

### Legacy migration and startup

`initialize_documents().await` reads only `thinoda.vfs:/Documents/...` entries
from localStorage. It validates the complete snapshot's paths, JSON envelope,
timestamp/length integer ranges, canonical base64 and decoded byte lengths before
writing. Malformed entries or blocked legacy access fail initialization, not an
apparently successful empty migration. Settings and recovery keys are untouched.

For each document it writes with `ExpectedRevision::Missing` and rereads the
durable bytes. Equal existing bytes need no write. Different durable bytes win,
regardless of legacy mtime; the legacy content is preserved and verified under
`/Documents/.thinoda-legacy/<path hash>/<content hash>/<original name>`.
The migration report lists original and preserved IDs so an app can offer both
versions. Repeated migration reuses and verifies that same copy. Concurrent
conditional-write conflicts are re-evaluated with bounded retries; continuous
changes fail safely. Changes to the localStorage snapshot during migration
also fail and require retry. **No legacy key is removed, even after verification.**
This retention supports rollback while older tabs or the old synchronous app
can still write VFS documents. It is not continuous synchronization.

Wiring status:

1. Done: `thinoda-web/src/main.rs` calls
   `thinoda_web::start_after_storage_ready(|report| { thinoda_app::files::record_browser_migration(report); web_main::main() })`.
   Migration completes **before** the app creates paths, starts recovery or
   launches the renderer. On failure the loading element shows the error and the
   app does not start. Persistence and filesystem consent are never requested.
2. Done in `files.rs`: Open/Import/non-image drops go to durable storage under
   `/Documents` with deduplication (`name`, `name (2).ext`, …). Each candidate is
   written with `ExpectedRevision::Missing`; on conflict an identical document
   is reused and a different one is never overwritten. Merge and FIM schema
   inputs go to VFS `/UploadInputs` (synchronous readers), dropped images go to
   `/ImageUploads/dropped`, and image/attachment uploads stay where they were.
   Delivering a durable document reads the provider and downloads it; failures
   are queued for `files::take_delivery_errors()`.
3. Done in `doc.rs`/`app.rs`: use `BrowserStorageProvider::new(backup)`
   for `/Documents` (`backup` writes the previous record to `<id>.bak` in the same
   transaction), and `browser_storage::document_revision(id, cancel)` for
   `expected_revision`; VFS fingerprints are not mixed with SHA-256 revisions.
   Migration conflicts and delivery failures enter the translated error flow.
   Successful saves call `files::save_back_picked` for retained picked handles;
   save-back failure leaves the document dirty. Closing releases the handle.

The provider lists immediate children without reading blobs, reports local
availability, supports content revisions, and has no watch/version-history/share
capabilities. Cancellation is cooperative at asynchronous checkpoints and before
the atomic commit; a successful commit is never reported as cancelled afterward.
Dropping a Rust future is not an undo of an in-flight write: explicitly cancel
its token and await its result when the outcome matters.

### Known-document availability (recent files)

Synchronous UI checks such as `Settings::existing_recent` cannot await the
provider. `browser_storage::known_document(path) -> Option<bool>` answers from an
in-memory, **metadata-only** index of durable `/Documents` IDs: `None` outside the
namespace (callers keep using the VFS), `Some(true)` for a known record, and
`Some(false)` before initialization or for unknown IDs. The index stores no bytes
and never creates VFS content. It is filled from the IndexedDB metadata keys
returned by initialization (after migration, before the app starts) and refreshed
by successful writes (including `.bak`), reads, revision probes, listings, `Missing`
conflicts and `notfound` reads. Commits are also announced to other tabs through a
`BroadcastChannel`. `files::document_exists(path)` wraps this for app code.
`Settings::existing_recent` uses the index for durable paths, so durable documents
stay in Open Recent.

Recovered documents retain known durable original paths and asynchronously
refresh their revision from IndexedDB metadata, without a legacy VFS
fingerprint. A delayed result cannot rebind a saved or retargeted document;
missing/failed identity keeps recovery edits dirty and requires conflict
resolution rather than permitting an unchecked overwrite.

Limits: a stale entry, such as data cleared by another context or eviction, only
fails safely when opened (the provider read returns a real error). There is no
delete event. Without `BroadcastChannel`, a document another tab created stays
unknown here until a list/read/revision call reconciles it. Native builds return
`None`.

### Persistence consent and quota

`storage_estimate().await` returns optional origin-wide usage/quota and persisted
status. It does not prompt. `request_persistence().await` calls
`navigator.storage.persist()` **only when invoked explicitly**, returns `false`
when denied, and skips the request if storage is already persistent. Initialize
never calls it. Settings > Cloud can request protection and show estimates;
do not claim that permission, a successful save, or an estimate means an
off-device backup exists. Unsupported APIs return `StorageError::Unsupported`.

### Explicitly picked files and download fallback

`file_system_access_supported()` detects the secure-context open-picker API.
Poll `pick_file_handles(multiple)` immediately from an explicit user gesture,
before any other awaited work. Read and stage each selected handle separately
with `read_picked_file`, retaining its opaque **tab-lifetime session** and file
revision. Each opened handle gets a distinct durable path, even when two files
have the same name and bytes. Handles are never inferred from paths or persisted
as permission grants. Use `write_picked_file` with the revision from that
handle, not the browser-copy revision, and release the session with
`forget_picked_file` on close. After reload, pick the file again.

Known bounded input formats are size-checked before reading: explicit XDF2/3
files use the 32 MiB FIM XML limit; MindManager, TextBundle, Word and unknown
or XML filenames use the 64 MiB archive/format-detection limit. Other supported
formats retain their existing limits. The picker rejects oversized files
without calling `arrayBuffer`; unexpected growth is also checked after reading.
For save-back, rereads are bounded by the import limit or the last verified
saved size, so saving a larger valid output does not prevent a later smaller
save. IndexedDB must still clone a corrupt record's bytes before it can check
their actual length when its size metadata is inconsistent; `localStorage`
similarly materializes its stored string before checking it.

A Save can request read/write permission; denial/cancellation/errors are real
failures, never fake successful downloads. Saving uses `createWritable`, checks
for changes before writing and immediately before close, then rereads to verify.
Web Locks serialize cooperating Thinoda picked-file saves on the same origin;
without Web Locks save-back returns Unsupported. **File System Access cannot
provide atomic compare-and-swap against external OS applications**; an external
edit in the final check/close interval can still race. This limitation is not
the origin-private provider's atomic IndexedDB revision contract.

Firefox/Safari (or policy-blocked browsers) retain the existing file-import and
`thinoda_platform::web::download` flow. Download/export remains unchanged; no
fallback download is silently triggered after a picked-handle save failure.
Folder workspace selection and persisted handle permissions are not implemented
in this bounded slice.

### C1 verification

Native unit tests in `browser_storage.rs` cover strict binary migration decoding,
malformed entries, namespace isolation, missing/equal/conflicting content and
repeatable conflict-copy IDs. `files.rs` tests cover upload name deduplication and the migration report hook.
Known-index tests cover namespace mapping, readiness, records and child reconciliation.
Run:

```sh
cargo test --offline -p thinoda-platform --lib browser_storage
cargo clippy --offline --target wasm32-unknown-unknown \
  -p thinoda-web -p thinoda-app -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings
```

Isolated Chromium checks against the actual bundled JS module exercised real
OPFS/IndexedDB initialization, binary read/write/revision checks, two competing
`Missing` writes (one winner), stale revision rejection, cancellation, verified
migration retaining legacy data, stable preserved conflict copies, malformed
migration failure, shallow listing, and a 6 MiB document. These are platform API
checks, **not** independent QA approval or cross-browser acceptance.
Forced OPFS initialization failure also exercised real IndexedDB byte fallback,
mixed-backend reads, successful/stale conditional saves, concurrent `Missing`
writes, path/directory collision rejection and fallback migration. Initialization
made no persistence request; a synthetic denied persistence request remained
`false`. Synthetic picked-file handles (with real Web Locks) covered read/save,
permission denial, external-change conflicts, staged-write cancellation, competing
saves and session release; no real filesystem picker prompt or OS file was used.
With the integrated workspace, the platform and `files.rs` native tests pass, and
wasm `cargo check`/Clippy of `thinoda-web`, `thinoda-app` and `thinoda-platform` are
clean for these files. Chromium checks of the module confirmed `.bak` backups,
`documentRevision`, stale `Match` and duplicate `Missing` rejection, estimates
without a persistence prompt, and File System Access detection. GC checks covered:
- the v1 to v2 upgrade (inline bytes moved, a pre-existing orphan swept);
- 40 autosaves with backup plus 10 without, staying at 2 OPFS files;
- 20 reads during saves without failures, and 10 parallel `Any` writes;
- a held reader lock delaying deletion until release;
- an IndexedDB fallback with 31 saves keeping 3 content rows;
- metadata-only listing, legacy migration conflicts and malformed-entry failure.

A Trunk-built app initialized in isolated Chromium without runtime errors and
saved a synthetic native document through Ctrl+N / Ctrl+S. Its IndexedDB metadata
record referenced a real OPFS blob and SHA-256 revision; the document survived
reload. Settings > Cloud showed actual origin usage/quota and a non-persistent
status after the explicit usage action, without a persistence prompt. After the
known-document index was added, the same built app listed the saved document under
File > Open Recent after reload and opened it from there. Before the fix, the legacy
VFS filter hid it. A Chromium module check confirmed the initialization snapshot
(including migrated documents, excluding settings), the `backup` commit flag,
cross-instance commit broadcasts and `notfound` reads.
Firefox/Safari, real picker permission prompts, external-application races,
browser eviction/power loss, deployed callbacks/CORS and broader integrated
settings/recovery/app flows still require browser acceptance.

## Reviewing AI results

AI commands use the endpoint configured in **Settings > AI**. Completed output
opens **Review AI Result** without changing the map. Inspect or copy the response,
then choose **Apply AI Result** to make one undoable edit to the existing map, or **Discard AI Result**
to leave the map unchanged. Generated maps also wait for Apply before opening a
new document.

While working, a nonmodal window shows incoming text. **Cancel AI Request** or
closing that preview stops the request and discards partial output; disabling AI
does the same. A broken/interrupted stream reports an error, never a partly
applied map. The configured server must support OpenAI-compatible SSE streaming,
including the final `[DONE]` marker, as well as the browser CORS requirements above.

The review is nonmodal. You can continue working, but changing the source sheet
or keeping a topic/inspector draft open disables Apply with an explanation.
Changing the endpoint, model, committed API key or temperature also invalidates
that result. Discard it and run the command again with the current settings.
The result retains its original topic/document even if you switch tabs.

### Discussing a map

Choose **AI > AI Chat** to open the discussion sidebar. Opening it sends nothing.
Choose **Quick**, **Balanced** or **Deep** for response detail, and **Selected topics**
(including their descendants) or **Current sheet** for context. Inspect **Context
to send** and the server address before choosing **Send**. Titles and hierarchy,
your question and completed chat history are sent; notes and attachments are not.
The modes guide the answer, not provider-specific reasoning APIs.

Chat streams answers without changing the map. **Cancel AI Request** discards the
partial answer and keeps your draft; disabling AI does the same. Errors and
oversized requests/replies are shown rather than saved as completed answers.
Only one map/chat request runs at a time; finish or discard a map review before
sending a chat question.

Each open document has its own session-only history. Hiding the panel or switching
tabs preserves it; closing the document, reloading the browser or restarting the
app does not. **Clear conversation** requires confirmation and preserves your
unsent draft. Closing chat restores the format sidebar without changing its
preferences. The sidebar is scrollable in short windows. F6 exercised split
streaming responses and cancellation in real Edge against a separate loopback
CORS fixture; no live AI service was contacted. Other providers, browser engines
and real-device behavior still require their own acceptance.

Expand **Saved prompts** in AI Chat to create reusable prompts. **New prompt** and
**Edit prompt** open a draft editor; **Save prompt** persists it, while **Cancel**
leaves the library unchanged. Deleting requires **Confirm Delete**. **Use prompt**
fills the composer without sending; replacing an existing unsent draft requires
confirmation. Review the text and context before choosing **Send**.

Prompts are local preferences in `settings.toml` (browser-local settings storage on
the web), not encrypted secrets or part of the map. Do not save passwords or API
keys in prompts. Names must be unique and at most 80 UTF-8 bytes; prompt text is
limited to 8192 bytes, with up to 32 prompts and 128 KiB in the library. Saving
reads fresh settings and changes only the library, without credential-store access.
If another window changed the library, Cancel and **Reload prompts** before editing
again; failed saves leave the editor and existing library intact.

### Semantic search and clustering

Set **Embedding model** in **Settings > AI** to an explicit embedding-capable
model, then expand **Semantic tools** in AI Chat. Opening these controls is
offline. **Build semantic index** sends current-sheet titles to the displayed
server in sequential batches of at most 128; it excludes notes and attachments.
The chat model is never used as a fallback. Unsupported endpoints/models report
an error. The sheet is limited to 500 topics and 64 KiB of titles, with nonblank
titles of at most 16 KiB each.

**Search meaning** explicitly sends your query (at most 8192 UTF-8 bytes) to the
same embedding model. It lists the best 20 cosine-similarity matches; clicking
one selects and reveals it without changing content. Editing the query discards
pending results. **Preview clusters** runs local, deterministic spherical k-means,
chunked across frames, with up to eight groups. Identical directions can produce
fewer groups. **Apply clusters to new sheet** creates an outline-only sheet in one
Undo step, preserving the source, not moving or replacing its topics.

The index, queries, results and previews are session-only. Source edits, unfinished
title/inspector drafts, sheet changes, source close, AI disable or changed AI
configuration/credentials invalidate work; rebuild before continuing. Cancel stops
pending indexing/search or local clustering without editing the map. Real Edge
acceptance against a local CORS fixture covered explicit index/query requests,
ranked results, cluster preview, Apply to a new sheet and Undo. This does not
establish every embedding provider's model or CORS compatibility.

### Guided map transformations

The **AI** menu and topic context menu offer **Rewrite Topic**, **Translate Branch**,
**Grow Ideas**, **Reorganize Branch** and **Refine Branch**. Choose topics, enter
instructions (or the target language), inspect the context and server, then **Send**.
Opening the dialog is offline. These commands include selected subtrees' complete
titles/hierarchy, not notes or attachments, with the primary topic identified for
rewrite/growth. Context exceeding 500 topics or 64 KiB is rejected rather than
truncated; instructions are limited to 8192 UTF-8 bytes.

Review the streamed result before **Apply AI Result**. Rewrite changes only the
primary topic's title; Grow adds children there. Translation, reorganization and
refinement create a **new outline-only sheet** in the same document, leaving every
source topic, note, style, relationship and attachment unchanged. The new sheet
does not copy that metadata. Each Apply is one undoable operation. Changed source
documents, unfinished title/inspector edits or changed AI settings invalidate
Apply. Cancel/Discard never edits the map. A transformed title must be one line
and at most 1024 bytes; transformed outlines are bounded to 64 KiB/500 lines.

## Zones

Select one or more floating root topics and choose **Insert > Create Zone** or
**Zones > Create Zone** in the context menu. Central-tree topics cannot become Zone
members. The frame initially includes the selected topics and their visible
descendants. **Apply** saves the name and frame as one undoable change;
**Cancel** discards the draft.

Click a Zone border/title to select it. Drag the border to move the frame and
its topics together, or drag the lower-right handle to resize the frame (minimum
100 by 100). Changes commit only when released inside the map; Escape, focus
loss or an intervening document change cancels the gesture. Use **Edit Zone**
in the context menu or Style inspector for precise position and size.
**Tools > Zones** lists Zones in order with rename/edit, Move Up/Down and Remove
controls. Removing a Zone preserves its topics. Overlapping frames are allowed;
the last Zone in the order is on top.

### AI Pitch outline suggestions

In **Edit Presentation**, generate or add slides, then expand **AI Pitch outline**.
Enter the audience, purpose or desired flow and choose **Suggest Pitch outline**.
Opening the editor/panel is offline. Sending transmits only the draft's slide
titles, numbered candidates and your instructions to the displayed AI server;
topic notes, attachments and frame coordinates are excluded. The chat model and
streaming/CORS requirements above apply.

Suggestions can select, reorder and retitle real candidates, never invent branch/
Zone sources or change their frames. Review the streamed result, then choose
**Use suggested outline** to replace only the editor's draft. **Apply** separately
saves that draft as one undoable presentation edit; source topics and metadata stay
unchanged. Discard, request cancellation or editor Cancel never changes the saved
presentation. Source/draft/instruction/configuration changes invalidate suggestions.
Limits are 64 candidate slides, 64 KiB of candidate titles, 8192-byte instructions
and 64 KiB of output; each suggested title is at most 256 UTF-8 bytes.

## Pitch and slide exports

Choose **View > Edit Presentation**. **Generate Slides** replaces only the
current draft with central-topic child branches in map order, then Zones in Zone
order. Alternatively, select a branch or Zone before opening the editor and use
**Add Selected Branch or Zone**. The vector slide workspace supports title,
order, visibility and framing edits. New slides isolate their source branch or
Zone; legacy slides retain contextual framing until changed. Automatic framing
tracks current visible content; manual framing preserves the saved viewport.
Hiding a subtree never deletes it from the map. The preview and exports share
the same semantic visibility rules. **Apply** saves all changes as
one Undo entry; **Cancel** leaves the saved presentation unchanged.

Choose **View > Pitch** to play the saved presentation, or **Presenter View** to
show the slide alongside its map and highlighted crop. If no slides exist, Pitch
opens the editor first; apply the slides and choose Pitch again. Use the
**Previous Slide** / **Next Slide** buttons, Left/Right, Page Up/Down or Space
(next), and **Exit Pitch** or Escape to exit. Native fullscreen is scoped to the
player window (also F11); the browser's **Expand Player** remains inside its tab.
Closing playback never closes the map. Playback is read-only and refreshes
after map edits, retaining the current slide by ID where possible. Source
deletion removes dependent slides; an empty deck or changed source identity
ends playback with an explanation instead of showing unrelated content.
Pitch/editor/presenter entry commands can be rebound in keyboard settings;
playback arrows are local, not global map shortcuts.

**Export PDF** produces a multipage vector presentation. **Export Slide Images**
produces a ZIP with ordered `slides/slide-001.png` files. The editor exports its
current draft without applying it; the player exports its current saved
presentation. Neither changes the map. The browser downloads these outputs;
native builds use a save dialog. Oversized frames/outputs fail visibly rather
than being truncated. **File > Export > PowerPoint** offers 16:9/4:3 and native
editable text, vector shapes and connecting paths; original photos remain
images. Its preview and exported pages follow the resolved light or dark app
appearance, including the browser's system theme. Playback transitions are not
exported animations. Presentation video is not provided. See [the presentation
contract and acceptance notes](PRESENTATIONS.md).

## Task planning and Gantt

The shared inspector orders **Style**, **Data**, **Map** in both Map and Outliner.
**Data** groups Local Image, Attachment, Hyperlink, Labels, Notes and notes-equation
controls; Notes/Hyperlink commands open it directly. Each tab remembers its own
scroll position. Style retains appearance, To-Do and Task Planning controls.
Each data category has its own titled group and divider spacing, without repeated
topic headings. Markdown preview stays in Notes; Equation is a separate group.

Notes and notes equations offer explicit **Enlarge** actions using the same
resizable nonmodal dialog as desktop (translated header, minimize/maximize,
restore and close). Enlarging an equation transfers its scratch source.
**Apply notes** / **Apply notes equation** commits one undoable change; Cancel
or Close discards scratch. Unapplied dialog edits are not included in save or
recovery. Switching tabs/selection/document or editing the source revision
invalidates scratch rather than applying it elsewhere. The map stays interactive.

Select a topic and choose **Insert > Task Planning**, or use **Task Planning**
in the Style inspector. Start/due dates use `YYYY-MM-DD`; duration counts
inclusive working days. An empty start or duration allows an unscheduled draft.
Entering a due date derives duration; leaving due blank keeps the entered
duration. Progress is 0-100%, and priority is an optional numeric value (0 in
the editor means none). Choose existing tasks as finish-to-start predecessors.
Successors start on the next working day after their predecessors finish.
**Apply** saves one undoable edit; **Cancel** discards the draft. Invalid dates,
cycles or edits made from a stale document revision show an error without
partially changing the map. Removing task planning does not remove its To-do
checkbox.

**Settings > Work Calendar** or **Tools > Work Calendar** edits the current
sheet's working weekdays and dated working/non-working exceptions. These are
document data, not global preferences, and are saved with the map.

**View > Gantt Chart** shows the same tasks, dates, progress and dependencies.
Select a task name to edit its details. Drag a dated bar to move it, drag its
right edge to change duration, or use its progress slider; release commits one
Undo step. Escape, focus loss or a concurrent document edit cancels a pending
bar gesture. The date fields provide a keyboard alternative. Closing this view
does not close the map.

The chart's **Export** menu produces complete SVG, PNG, vector PDF or task CSV
from a captured sheet snapshot. **Print** uses the Gantt-specific page preview,
including browser printing and **Save print PDF…**, not a map screenshot.
The interactive view limits 1,000 rows and 100,000 logical timeline units.
Graphical exports allow up to 5,000 rows and 6,000 timeline days; CSV is not
subject to the graphical timeline limit. Large PNG output has separate pixel
limits and fails explicitly rather than truncating tasks. Reduce zoom for a
wide interactive view, or use CSV for a long schedule.

## Keyboard mappings

Open **Settings > Keyboard Shortcuts > Edit mappings** to review or customize
shortcuts. **Thinoda Standard** preserves the familiar defaults;
**XMind-compatible** is an opt-in preset for supported commands, not a promise
that every XMind or operating-system shortcut is available.

To edit a preset, enter a profile name and choose **Duplicate**. Use the search
field to find a command, choose its **Assign** action, then press the desired
keys. The editor reports conflicting or reserved combinations and shows changes
for both Map and Outline before **Apply**. **Unassign** removes a command's
explicit bindings; **Reset All** restores the profile's base preset.
**Cancel** discards the draft, including profile selection and management changes.
A failed save leaves the draft open and the active mapping unchanged.

Applied profiles are saved with the rest of the browser's settings and survive
reloads, subject to the storage limitations below. All open document views use
the same active profile. Menus, the command palette, shortcut reference and the
offline guide's active-shortcut tables update together. The guide's historical
prose examples are labeled as Thinoda Standard.

Browser navigation/reload/tab controls and ordinary focused-text/clipboard
operations retain their ownership. Native-only commands such as Close Window
are not browser-window controls and are not offered as browser features.
Clearing site data also removes saved profiles; no profile import/export is
currently provided.

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

The service worker precaches the built app shell, including the matching
JavaScript, WASM and imported snippets. On a first load it registers after the
WASM bootstrap succeeds or reports a failure, so its downloads do not compete
with the initial WASM download. Bootstrap completion can precede a usable app.
Closing the tab before caching finishes can leave the first offline installation
incomplete. The worker caches **only build assets**, not maps, AI responses or
other sites. Documents remain in OPFS/IndexedDB; recovery stays in the
synchronous VFS. Offline editing works once that shell has been cached and while
site storage remains available; online AI, external links and remote services
still need a connection.

Trunk's post-build hook (`cargo xtask web-pwa`) creates `dist/sw.js` from the
exact staged artifacts and a content-derived revision. URLs, manifest identity,
worker scope and caches are relative to the deployment subpath, including
GitHub Pages. The hook also embeds the package version and content revision
into the **loaded HTML**, so the running shell's identity never comes from a
mutable service worker.

While running, the app requests `registration.update()` at startup and hourly.
Focus, online, page-show and visibility/resume events share that throttle and
one-in-flight guard. Errors retry with exponential backoff (two minutes up to
one hour), are logged locally, and never produce success notices. Timers may
be suspended or clamped; **closed/suspended apps cannot guarantee polling**.

The browser automatically downloads a complete new shell into its waiting
worker's cache. The shared translated light/dark **Update ready** notice shows
the new version/build, a release/changelog link and close/reopen instructions.
That is not a successful activation: the new worker waits until **all** existing
app windows/tabs close before taking over. There is no forced reload,
`skipWaiting` or `clients.claim`; open and unsaved documents remain untouched.
Save/download maps before closing. A complete waiting shell can activate on a
later launch, including offline while the browser retains its cache.

After an observed waiting update actually boots as a different shell, the app
shows **Update installed successfully** with the running version/build and
release/changelog link. A scope-specific local-storage baseline and pending
identity prevent first-install, unchanged-boot and stale-tab false positives.
Per-build seen/pending markers cannot be rolled back by an older tab.
Simultaneously launched tabs may each show the same legitimate applied notice.
Storage failure, clearing site data, or never observing the waiting build can
suppress the success notice;
it must never infer success from a downloaded worker or failed storage write.
Two tiny per-build markers are retained for observed updates/boots; clearing
site data removes this history together with other browser-local app data.
Repeated builds of the same package version are distinguished by content revision;
unreleased development builds may have no corresponding published release notes.
Dismissal is per version/build within the app session.

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
OPFS/IndexedDB document storage uses the browser's origin-wide quota rather than
localStorage's roughly 5 MB limit. Settings, recovery and scratch VFS data still
use localStorage and can exceed its separate limit, including recovery snapshots
of large maps. Storage errors are reported; download maps and keep external
backups rather than assuming that a browser save guarantees recovery. Clearing site
data in the browser deletes stored documents, settings and recovery data.

Browser recovery uses cooperative localStorage leases, not transactional
cross-tab locks. It checks the current claim before recovery operations and
retains queued snapshots when a write fails; missing, corrupt or competing
claims stop the former owner from intentionally resuming its writes. An
unavailable journal, a failed snapshot or an incomplete recovery scan produces
a warning. A previous complete snapshot is retained when a pending draft cannot
be prepared; resolve or copy the draft and save or download the document. An
active availability warning does not clear just because an empty flush succeeds
or a retry is queued: outstanding snapshots must first be written or canceled.
A browser can still interleave a claim change between checking it and writing or
removing a value, so this is not a guarantee against simultaneous-tab races.
When discovery is incomplete, the recovery dialog offers **Keep recovery
data** instead of **Discard All**; this closes the dialog without deleting the
listed snapshots. They can be inspected again after a later restart.
Failed cleanup is logged, preserves recovery data and displays a separate
warning. Thinoda retries while the saved document remains open and, in memory,
after it closes. It never removes an open document's newer recovery snapshot as
a closed-document retry. Persistent failures, lost ownership, a full retry
queue, or shutting down before a retry succeeds can still leave a snapshot to
be offered again on a later launch.
Do not treat crash recovery as a substitute for downloaded backups.

GitHub Pages serves all project sites of an account from the same
`<owner>.github.io` origin, so they share storage quotas. Thinoda uses its named
OPFS directory, `thinoda.documents.v1` IndexedDB database and `thinoda.`-prefixed
VFS keys to avoid accidental collisions; these names are not an isolation
boundary between applications served on the same origin.

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
- Opening and saving documents shows the same busy card with the file name as on
  the desktop. Without worker threads the import or write still runs on the
  page's main thread: it starts only after a frame with the indicator was
  presented, so the indicator stays visible, but the page does not respond (and
  the spinner does not animate) until that file is done.
- The web entry point installs pointer feedback for enabled, hovered click-only
  controls, including custom tabs and buttons. Text, resize and drag cursors
  retain precedence; desktop cursor behavior is unchanged.

## Testing
- Use Node.js 24, selected by the repository root `.node-version` file. CI reads
  that same file through the Node-24-based `actions/setup-node` action; the PWA
  tests reject a different Node major. The Node-only PWA tests need no npm
  install; the browser suite uses the locked dependencies below.
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
- From `crates/thinoda-web`, run `trunk build --release --locked`, `npm ci`,
  `npx playwright install chromium`, and `npm run test:browser`. The suite
  serves `dist` on loopback with Python, runs one isolated Chromium context per
  test, and covers the production loader, deferred offline installation,
  rootless board editing, synthetic-event clipboard flows with a virtual
  clipboard, mini-map navigation and pointer capture, SVG/PNG/JPEG/PDF and
  PowerPoint exports, embedded image cards (including PDF) and Pitch viewport.
  Set `THINODA_BROWSER_DIST` to serve another built directory, `THINODA_WEB_URL` to
  test an existing server instead, or `THINODA_BROWSER_EXECUTABLE` to use an
  installed Chromium-compatible browser. Concurrent Playwright runs must use
  separate `--output` directories: each run clears its output directory on
  startup and can otherwise delete another run's downloads. Use an existing
  server through `THINODA_WEB_URL` or separate servers for concurrent runs.
  The PDF-content check uses Playwright's
  full Chromium build independently; set `THINODA_PDF_BROWSER_EXECUTABLE` only
  if a different browser with an inline PDF viewer is required. These desktop
  checks do not replace real system-clipboard, physical iPad or screen-reader
  testing.
- Browser smoke test (manual or scripted): load the site, create a map from a
  template, add a topic, save (download), reload and confirm the recovery
  prompt, open the downloaded file, print and save the print PDF.

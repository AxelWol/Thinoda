# Cloud storage: current state, gap analysis and plan

Status: **implementation in progress** (user-directed, 2026-11-01).
Sections 1–3 retain the pre-implementation inventory. Delivery and verification
are recorded below; mock tests are not evidence of live provider acceptance.
Extends [PLAN.md R10](PLAN.md#r10---protected-files-cloud-synchronization-and-sharing)
and the Platform & collaboration items in [BACKLOG.md](BACKLOG.md#platform--collaboration).
Freeform boards are planned separately in [FREEFORM.md](FREEFORM.md); nothing here
is on their critical path.

Providers in scope: **OneDrive, iCloud Drive, Dropbox, Box, Nextcloud** (plus
Google Drive and ownCloud, which Thinoda already detects). Platforms: Windows,
macOS, Linux native builds and the web (wasm) build.

## 1. What Thinoda does today

| Area | Current behaviour | Source |
|---|---|---|
| Cloud folder discovery | Best-effort detection of local sync folders for OneDrive, iCloud, Dropbox, Google Drive, Box, Nextcloud, ownCloud on Windows/macOS/Linux. No network, no provider SDKs | `thinoda-platform/src/cloud.rs` |
| Settings | Settings has a Cloud tab with the detected folders; hidden on web | `thinoda-app`, [WEB.md](WEB.md) |
| Saving | Atomic write: temporary file in the same folder, `fsync`, optional `.bak`, rename over the target, directory `fsync` on Unix | `thinoda-platform/src/fs.rs` |
| External change check | `FileFingerprint` (length + modification time) compared at save; dialog **File changed on disk**: Overwrite / Save As Copy / Cancel | `fs.rs`, `thinoda-app/src/ui.rs` |
| Crash recovery | Recovery journal; autosave only for documents that have a path | `thinoda-app` |
| Credentials | OS keychain (native), session storage (web) | `thinoda-platform` |
| Web storage | Virtual file system in `localStorage` under `/Documents`; open = copy in from the file picker; Save As = download | `thinoda-platform/src/vfs.rs`, [WEB.md](WEB.md) |

Not present: online-only/placeholder awareness, file watching, conflict-copy
detection, revision identity, any provider API, File System Access API, OPFS
or IndexedDB on web.

## 2. Platform facts that matter

- **Windows (OneDrive, Dropbox, Box, iCloud for Windows, Nextcloud VFS)** use the
  Cloud Files API. Online-only files are placeholders with
  `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (and sometimes `OFFLINE`); reading
  them downloads ("hydrates") them, which can be slow, fail offline, or be
  blocked by policy. Pinned/unpinned state is visible through attributes.
- **macOS** providers use the File Provider framework. Online-only files are
  "dataless"; reads trigger downloads. iCloud Drive expects coordinated access
  (`NSFileCoordinator`/`NSFilePresenter`) and exposes download status
  (`ubiquitousItemDownloadingStatus`). Sandboxed (App Store) builds need
  security-scoped bookmarks for recent files.
- **Linux**: Nextcloud and ownCloud desktop clients, Dropbox's client and
  third-party clients (rclone, Insync, GNOME Online Accounts with gvfs) are
  regular folders or FUSE mounts; no standard placeholder API. iCloud and Box
  have no official Linux client.
- **Conflict copies**: providers keep both versions by creating a renamed copy,
  for example Dropbox `name (Computer's conflicted copy 2026-10-28).ext`,
  OneDrive appends the computer name, Nextcloud `name (conflicted copy …)`.
  Formats differ per provider and language and change over time.
- **Temporary-file-plus-rename** is atomic on the local disk, but a sync client
  may upload the temporary file, see the rename as delete + create (losing the
  provider's version history or sharing links), or fail to replace a
  placeholder. Behaviour must be tested per provider.
- **Web**: File System Access file/folder pickers with write access exist only in
  Chromium browsers (Chrome, Edge, Opera); Firefox and Safari don't offer them.
  The Origin Private File System (OPFS) and IndexedDB are available in all
  major browsers but are private to the site. `localStorage` is small (about
  5 MB) and synchronous.
- **Provider APIs**: OneDrive (Microsoft Graph), Dropbox and Box offer REST APIs
  with OAuth 2.0 + PKCE and revision IDs (eTag/`rev`) for conditional uploads.
  Nextcloud and ownCloud use WebDAV (eTags, `If-Match`); browser access needs
  CORS configured on the server. **iCloud Drive has no general web API for
  third-party apps** (CloudKit only covers an app's own container), so iCloud
  stays native-only via the local folder.

## 3. Gap analysis

Legend: ✅ works today · 🟡 partly · ❌ missing · ➖ not applicable / not possible

### 3.1 Native builds (local sync folders)

| Capability | Windows | macOS | Linux | Gap / value |
|---|---|---|---|---|
| Detect provider folders | ✅ | ✅ | 🟡 (no iCloud/Box clients) | Low |
| Open/save files in sync folders | ✅ | ✅ | ✅ | — |
| Recognise online-only files before opening; show "downloading…" with cancel | ❌ | ❌ | ➖ | **High**: no download state, progress or cancel during hydration today |
| Never hydrate files just to show recents, thumbnails or previews | ❌ | ❌ | ➖ | **High** |
| Coordinated access (iCloud `NSFileCoordinator`) | ➖ | ❌ | ➖ | Medium |
| Detect external changes while open (file watching) | ❌ | ❌ | ❌ | **High**: only checked at save today |
| Reliable change identity (file ID + content hash, not only length + mtime) | 🟡 | 🟡 | 🟡 | Medium: same-length edits within the mtime resolution are missed |
| Detect conflict copies next to the open file and offer compare/merge | ❌ | ❌ | ❌ | **High** |
| Provider-tested save strategy (rename vs. in-place write; `.bak` placement) | 🟡 | 🟡 | 🟡 | **High**: `.bak` files are synced too and create noise |
| Sync status badge (synced, pending, offline, error) | ❌ | ❌ | ❌ | Medium; needs Cloud Files / File Provider status queries |
| Lock / "in use by" hints | ❌ | ❌ | ❌ | Low: advisory locks don't coordinate across machines |
| Version history (provider) | ❌ | ❌ | ❌ | Medium; via provider API (3.3) |
| Share link | ❌ | ❌ | ❌ | Medium; via provider API or OS share sheet |

### 3.2 Web build

| Capability | Chromium | Firefox | Safari | Gap / value |
|---|---|---|---|---|
| Storage that survives larger maps (images) | 🟡 localStorage | 🟡 | 🟡 | **High**: move to OPFS/IndexedDB |
| Open a local file and save back to it (no download) | ❌ | ➖ | ➖ | **High** on Chromium via File System Access |
| Open a local (synced) folder as a workspace | ❌ | ➖ | ➖ | Medium |
| Request persistent storage (`navigator.storage.persist`) and show quota | ❌ | ❌ | ❌ | Medium |
| Cloud folder picker | ➖ | ➖ | ➖ | Replaced by provider connections (3.3) |
| Nextcloud/ownCloud via WebDAV | ❌ | ❌ | ❌ | Medium; server must allow CORS |
| OneDrive / Dropbox / Box via REST | ❌ | ❌ | ❌ | Medium; OAuth PKCE redirect, token handling |
| iCloud Drive | ➖ | ➖ | ➖ | Not possible; user downloads/uploads manually |

### 3.3 Provider APIs (both builds)

| Provider | API | Native | Web | Notes |
|---|---|---|---|---|
| OneDrive / SharePoint | Microsoft Graph | ❌ | ❌ | eTag/cTag, upload sessions for large files, sharing links, versions |
| Dropbox | Dropbox API v2 | ❌ | ❌ | `rev` for conditional upload (`update` mode) |
| Box | Box API | ❌ | ❌ | `If-Match` eTags, versions; enterprise admin approval may be needed |
| Nextcloud / ownCloud | WebDAV (+ OCS for shares) | ❌ | ❌ | App passwords or Login Flow v2; self-hosted CORS |
| Google Drive | Drive API v3 | ❌ | ❌ | Already in R10; restricted scopes need Google verification |
| iCloud Drive | none for third parties | ➖ (local folder only) | ➖ | — |

## 4. Recommended architecture

One **storage contract** in `thinoda-platform`, used by native, web and provider
adapters instead of scattered path code:

```text
trait StorageProvider (async)
  list(location) -> entries with: name, stable id, size, revision, availability
                    (local | online-only | downloading | error)
  read(id, cancel) -> bytes + revision
  write(id, bytes, expected_revision) -> new revision | Conflict(remote revision)
  watch(location) -> debounced change events (own saves suppressed)
  capabilities -> {placeholders, revisions, versions, share_links, watch}
```

Rules:

- **Revisions, not timestamps**: every write states the revision it replaces
  (eTag/`rev`/file ID + content hash for local files). A mismatch never
  overwrites: both versions are kept and the existing conflict dialog gets a
  **Compare** option (a diff of sheets/topics, later a three-way merge).
- **Never hydrate silently**: online-only files show their state; opening asks
  or shows progress with Cancel, off the UI thread.
- **Debounced watching** with save-origin suppression, so Thinoda's own saves
  and intermediate sync-client steps don't trigger reload prompts.
- **Conflict copies**: watch the document's folder for siblings matching the
  provider patterns and offer "Open both / Compare"; never auto-delete.
- **Save strategy per provider**, chosen from tests: rename-replace (default),
  in-place write with verification (for providers that break history on
  rename), `.bak` optionally moved to a local backup folder outside the synced
  tree.
- **Security and privacy**: tokens only in the OS keychain (native); on web
  in memory/session storage with short-lived access tokens, strict CSP and
  per-tab handling; minimal OAuth scopes (app folder or selected files where
  the provider supports it); explicit user action for connecting accounts,
  uploading and sharing; nothing about document content in logs or telemetry.
  A dedicated security review is required (as for R10).
- The synchronous `localStorage` VFS stays as a fallback; the app moves to the
  async contract first, so OPFS and provider adapters fit behind it.

## 5. Phased plan

| Phase | Content | Acceptance |
|---|---|---|
| **C0 Async storage contract** | `StorageProvider` trait, async open/save paths in the app with progress and cancel, existing local and VFS storage behind it | No behaviour change; all file tests pass on native and web |
| **C1 Web storage** | OPFS (fallback IndexedDB) replaces localStorage with migration; persistent-storage request and quota display; Chromium File System Access "save back to the original file" and folder workspace | Migration keeps all existing browser documents; Firefox/Safari keep the download flow; docs in [WEB.md](WEB.md) updated |
| **C2 Sync-folder hardening (native)** | Placeholder detection (Windows Cloud Files attributes, macOS dataless/File Provider status), no silent hydration, file watching, file ID + hash change identity, conflict-copy detection, provider-tested save strategy, sync badge | Test matrix per provider and OS: online-only open, offline save, concurrent edit on two machines, rename/history preserved; no lost edits |
| **C3 WebDAV (Nextcloud/ownCloud)** | Adapter with eTags, Login Flow v2/app password, CORS guidance for self-hosting | Conditional writes reject stale saves; works in native and web (with CORS) |
| **C4 First REST provider (OneDrive/Graph)** | OAuth PKCE (system browser + loopback on native, redirect on web), file picker, conditional upload, versions, share link | Token revocation, offline, conflict and large-file tests; security review passed |
| **C5 Dropbox, Box, Google Drive** | Same contract, one provider at a time | Same acceptance as C4 per provider |
| **C6 (optional) Real-time collaboration** | Only after C0–C4, via PLAN R12 (CRDT) | Out of scope here |

iCloud Drive is covered by C2 (native local folder) only.

### Implementation checkpoints

| Phase | Implementation | Verification / remaining acceptance |
|---|---|---|
| C0 | Revision-aware async contract, cancellable local/VFS providers and conditional writes. Native app and startup loaders retain the exact read revision; background saves recheck it. | 25 platform tests, 5 app file-job tests and 6 progress workflows pass. Native and combined wasm Clippy pass. Review found a chat-binding/revision coupling in the legacy loader; corrected by keeping chat optional and carrying read identity through startup. App wiring is committed after its browser dependencies. |
| C1 | OPFS with IndexedDB fallback, schema migration, metadata-only listing, conflict preservation, persistence/quota helpers and session FSA handles. | Isolated Chromium checks include a 6 MiB document, migration, conditional writes and bounded storage after repeated saves. Review-driven blob reclamation and metadata separation are implemented. Full app/browser acceptance remains pending. |
| C2 | Windows recall/offline and macOS dataless detection; bounded identity hashes; debounced poll-watcher primitives; external-change reload/keep/compare and sibling conflicts. | Native primitives and comparison/reload tests pass; review found no remaining blocking C2 defects. Real sync-client/OS history, hydration, offline and two-machine cases are not verified. Cancellation is cooperative and cannot abort a blocked OS hydration syscall. Atomic rename remains the default, not an unverified in-place strategy. |
| C3 | Conditional WebDAV adapter and Nextcloud Login Flow v2 primitives. | Local HTTP mocks exercise request/ETag/conflict handling. Connected app workflow and live server/CORS checks remain pending. |
| C4 | Graph adapter, upload-session primitives, explicit versions/share methods and PKCE credentials. | OAuth tests and HTTP mocks are offline. App workflow, security review and live registered-client acceptance remain pending. |
| C5 | Dropbox revision updates, Box conditional writes and Google Drive v3 adapter primitives. | Local HTTP mocks cover conditional requests. Per-provider live account, registration and browser redirect-download acceptance remain pending. |

The local revision is an identity plus a bounded 192-KiB content sample, not a
full-file integrity digest. Conditional local saves serialize Thinoda writers
and recheck before replacement; external programs do not participate in that
lock, so filesystem replacement is not a provider-grade compare-and-swap.

## 6. Risks and open questions

- Provider behaviour changes without notice; the C2 test matrix must be rerun
  per release.
- Box and Google Drive may require app verification or enterprise approval
  before public use.
- OAuth redirect handling for a static web deployment needs a registered
  redirect URL per deployment; self-hosters must register their own app IDs.
- Mac App Store sandboxing changes file access (security-scoped bookmarks);
  needed only if Thinoda ships through the store.
- Real merge of concurrent edits needs a stable topic identity across
  sessions; Thinoda's topic IDs are per document and suitable, but merge UI is
  a separate design.

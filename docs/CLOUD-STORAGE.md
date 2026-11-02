_Last updated: 2026-11-01 11:36:42 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Cloud storage: current state, gap analysis and plan

Status: **C0-C5 implementation; live acceptance remains open** (user-directed,
2026-11-01). C6 is not included.
Integrated remote accounts are **disabled by default** at the user's request
while publisher registrations are pending. The opt-in
`integrated-cloud-services` Cargo feature is not enabled in normal native
packages or Pages builds. Local sync-folder hardening and durable browser
storage remain active; see [feature/build setup](CLOUD-AUTH-SETUP.md).
The disabled build was checked in isolated Chromium: account controls were
absent, browser-storage controls remained visible, and a stale OAuth callback
was scrubbed without PKCE state or external requests. Disabled/enabled cloud
tests and native/wasm lint both pass; CI retains opt-in test coverage without
enabling the feature in release artifacts.
Sections 1–3 retain the pre-implementation inventory. Delivery and verification
are recorded below; mock tests are not evidence of live provider acceptance.
Extends [PLAN.md R10](PLAN.md#r10---protected-files-cloud-synchronization-and-sharing)
and the Platform & collaboration items in [BACKLOG.md](BACKLOG.md#platform--collaboration).
Freeform boards are planned separately in [FREEFORM.md](FREEFORM.md); nothing here
is on their critical path.

Providers in scope: **OneDrive, iCloud Drive, Dropbox, Box, Nextcloud** (plus
Google Drive and ownCloud, which Thinoda already detects). Platforms: Windows,
macOS, Linux native builds and the web (wasm) build.

## 1. Pre-implementation inventory

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
- **Provider APIs**: OneDrive (Microsoft Graph) and Dropbox support public
  OAuth 2.0 + PKCE; Box's documented code exchange requires a client secret
  and is not enabled in this native/static public-client implementation.
  Their REST APIs provide revision IDs (eTag/`rev`) for conditional uploads.
  Nextcloud and ownCloud use WebDAV (eTags, `If-Match`); browser access needs
  CORS configured on the server. **iCloud Drive has no general web API for
  third-party apps** (CloudKit only covers an app's own container), so iCloud
  stays native-only via the local folder.

## 3. Pre-implementation gap analysis

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
- The synchronous `localStorage` VFS remains for settings, recovery and scratch
  inputs. Durable document errors never silently fall back to old VFS bytes;
  OPFS's document fallback is IndexedDB behind the async contract.

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
| C0 | Revision-aware async contract, cancellable local/VFS providers and conditional writes. App and startup loaders retain exact read identity; worker saves recheck it and preserve newer edits. | Platform, file-job, progress and document tests cover cancellation, conflicts, startup identity and failed-save drafts. Review fixed chat-binding/revision coupling; full-suite regression checks also restored undoable inspector commits before a failed save. |
| C1 | OPFS with IndexedDB fallback, lossless migration, metadata-only listing, conflict preservation, persistence/quota controls and tab-lifetime FSA handles; app open/save uses the async provider. | Isolated Chromium module checks cover 6 MiB documents, migrations, contention, backups and bounded repeated-save storage. A Trunk-built app initialized and saved a real OPFS document; Settings displayed actual usage/quota without requesting persistence. Review fixed orphan reclamation and metadata separation. Real Firefox/Safari and picker permission flows remain unverified. |
| C2 | Windows recall/offline and macOS dataless detection; bounded identity hashes; settling/debounced polling, own-save suppression, external-change reload/keep/compare and sibling conflicts; optional external backup directory. | Native primitives and comparison/debounce/whole-workbook reload tests pass. Review found no remaining blocking C2 defects. Real sync-client/OS history, hydration, offline and two-machine cases are not verified. Cancellation cannot abort a blocked OS hydration syscall. Atomic rename remains the default; in-place/coordinated save and a provider-confirmed synced badge are not claimed. |
| C3 | WebDAV listing/read/conditional write/create, app-password and Nextcloud Login Flow v2; explicit app connection, browse/open/upload/save and disconnect. | Offline authorization tests and local HTTP mocks cover ETag/preconditions, malformed replies, cancellation and ambiguous successful writes without a revision. Live server/login/CORS acceptance remains open. |
| C4 | Graph PKCE, system browser/loopback and web callbacks, small/large conditional uploads and creation, explicit versions/share links, secure credential store and connected app workflow. Consumer sign-in uses publisher-owned configuration and discovers its app folder instead of asking ordinary users for registration IDs. Current consent supports personal Microsoft accounts, not organizational OneDrive/SharePoint. | Offline OAuth and protocol mocks plus app workflow tests cover implementation. Dedicated security review found no vulnerabilities. No Thinoda-owned OAuth registrations exist yet: publisher registration/build preparation is required before enabling live sign-in. OS keyrings, revocation, real large uploads and Graph final-commit races remain unverified. |
| C5 | Dropbox revision updates/creation, Box conditional transport/creation and Drive v3 existing-file conditional updates plus explicit distinct-ID creation; same consent/cancellation/app workflow. Supported public sign-in targets are Dropbox native/web and Google Drive Desktop. Box sign-in on either target and Google Drive web code exchange are explicitly unavailable without a separately reviewed architecture. | Offline protocol and app tests cover provider requests, conflicts, stable returned identities and upload binding/dirty-state guards. The user approved Drive creation that can duplicate names, with an explicit warning; no name-atomicity guarantee is claimed. Live account/registration, signed-download/CORS and enterprise/browser acceptance remain open; tested Box transport does not establish an available consumer connection. |

Each phase received a rubber-duck review; meaningful findings were fixed before
integration. The native workspace gate is `cargo xtask ci`; browser compilation
is checked with the combined wasm Clippy command in [CONTRIBUTING.md](../CONTRIBUTING.md).
Automated protocol tests use local mock servers, never live provider accounts.
Runtime storage checks used an isolated Chromium profile and synthetic documents.
Neither those checks nor a successful Trunk build establish deployed OAuth,
cross-browser picker, power-loss, OS keyring or sync-client acceptance.

User-directed consumer sign-in correction: the normal OAuth screen is
**Connect -> provider sign-in -> provider authorization**, not client-ID,
drive-ID or callback-URI entry. Public application registrations belong to
the publisher/build deployment; users authorize that registered Thinoda app.
The user confirmed that registrations do not yet exist and requested
[publisher registration/build preparation](CLOUD-AUTH-SETUP.md). Unconfigured
builds must explain that sign-in is unavailable rather than launch a fabricated
authorization request. WebDAV remains different: users must identify their own
server; Nextcloud's browser login can then authorize access.

Native CI and reusable release packaging now map the supported public publisher
Variables at compilation. SPA Variables are mapped only for the `main` push
that builds the canonical Pages site; pull-request web previews remain
unconfigured. These workflow mappings do not create external registrations.
Follow-up reviews found no blocking consumer-authorization or security defects,
including with offline tests compiled against fixture publisher registrations.
A final production Trunk build was opened in isolated Chromium: OneDrive's
normal settings had no registration/account/redirect inputs and explained the
unconfigured-build limitation. Selecting it and pressing Connect kept the page
local, issued no external fetch and created no OAuth session state. This
verifies the unavailable-build guard, not a live authorization.

Creation is an explicit action returning a new stable ID and revision, distinct
from `write(existing_id, ExpectedRevision::Match(...))`. Graph, WebDAV, Dropbox
and Box reject conflicting names rather than silently overwrite or rename.
Google Drive permits duplicate names: its explicit creation always creates a
distinct file and never updates an existing one. Current creation
bounds are 256 MiB for WebDAV/Graph, 150,000,000 bytes for Dropbox and 50,000,000
bytes for Box and 5,000,000 bytes for Drive; Graph switches to upload sessions
for large content.

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

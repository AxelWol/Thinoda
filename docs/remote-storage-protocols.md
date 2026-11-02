_Last updated: 2026-11-01 10:51:35 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Remote storage adapter primitives (C3–C5)

These are platform-only protocol primitives, not an accepted cloud-storage
feature. There is no UI, OAuth flow, account discovery, background synchronization,
watcher, retry daemon, or implicit sharing. Constructing a provider validates its
explicit configuration and creates a client, but never contacts the network.
Only calling and polling an operation starts a request.

Source: `crates/thinoda-platform/src/remote_storage.rs` and its provider modules.
Native mock tests: `crates/thinoda-platform/tests/remote_storage.rs`.
This implements bounded parts of [CLOUD-STORAGE.md](CLOUD-STORAGE.md), not its
live-service, OAuth, browser, or independent security-review acceptance criteria.

For publisher-owned public OAuth registrations, build variables, exact native/
SPA redirects and unsupported Box/Google-browser protocol boundaries, follow
[Cloud sign-in publisher setup](CLOUD-AUTH-SETUP.md). Ordinary users do not
register provider apps or enter client IDs; unconfigured builds fail before
provider browser launch.

## Integration requirements (owned by the manifest/module integrator)

No Cargo manifest, lockfile, app code, or `lib.rs` is changed by this adapter work.
Required additions:

```toml
# crates/thinoda-platform/Cargo.toml [dependencies]
reqwest.workspace = true
quick-xml.workspace = true

# [target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokio.workspace = true

# [target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen-futures.workspace = true
```

Existing workspace reqwest disables default features and enables `json`, `stream`,
and `rustls-tls`. Existing tokio `rt`/`time` features suffice; no macros, multithreaded
runtime, or extra cancellation dependency is needed. Native callers must poll
operations on a Tokio runtime off the UI thread. Native tests also require Tokio,
which the native dependency above supplies.

The existing wasm `web-sys` dependency needs these additional features:
`Request`, `RequestInit`, `RequestMode`, `RequestCredentials`, `RequestRedirect`,
`Response`, `Headers`, `AbortController`, `AbortSignal`, `ReadableStream`,
`ReadableStreamDefaultReader`. Existing `Window`, `js-sys`, `wasm-bindgen`,
`web-time`, `serde_json`, `base64`, and `uuid` are reused.

For a complete direct declaration, the browser transport uses precisely these
`web-sys` features (do not rely on the app enabling them transitively):

```toml
features = [
    "Window", "Request", "RequestInit", "RequestMode",
    "RequestCredentials", "RequestRedirect", "Response", "Headers",
    "ReadableStream", "ReadableStreamDefaultReader",
    "AbortController", "AbortSignal",
]
```

Merge these with the platform's other browser features rather than replacing them.

Export `pub mod remote_storage;` in the platform library. The existing
`crate::storage` contract must export `StorageError::Remote(String)`.
The implemented signatures match the foundation's owned `CancelToken` and
`ExpectedRevision` arguments, mandatory `ReadResult.revision: Revision`, and
native Send / wasm non-Send `StorageFuture`.

The integration tests import `thinoda_platform::remote_storage::*` and use the
real platform storage contract and native HTTP client, not a fake transport.

After integration, run:

```sh
rtk cargo test -p thinoda-platform --test remote_storage
rtk cargo clippy -p thinoda-platform --all-targets -- -D warnings
rtk cargo clippy --target wasm32-unknown-unknown -p thinoda-platform -- -D warnings
```

The integrator also owns dependency notices/lockfile updates and full workspace CI.

## Shared behavior and security boundaries

- `Authentication` holds an explicitly supplied bearer token or DAV basic
  username/app password. It, configs, clients, and `ShareLink` deliberately have
  no `Debug` or serialization implementation. Authorization headers are marked
  sensitive. These objects do not obtain, refresh, persist, or log secrets.
- HTTPS is mandatory, except HTTP to **literal loopback IP addresses** for local
  mocks. URL userinfo and fragments are rejected. Base URLs cannot contain
  queries. REST API hosts are restricted to each supported global provider
  endpoint; WebDAV can use an explicitly configured self-hosted HTTPS collection.
- Native reqwest uses rustls through the workspace feature configuration,
  disables redirects, disables ambient proxy discovery, and has 15-second
  connection / 120-second request timeouts. Account headers are attached only
  to configured API roots. DAV IDs and DAV listing hrefs must remain within
  the configured origin and collection.
- Graph/Box native content downloads can take **one** validated HTTPS redirect
  to approved provider domains, without bearer/basic credentials. Signed session
  URLs are similarly validated before sending file bytes. Loopback mock links
  must retain the original mock origin. Provider-domain tests reject suffix
  lookalikes, userinfo, non-HTTPS, nonstandard ports, and unrelated hosts.
- Graph upload fragments and session cleanup never receive an Authorization
  header; the session URL itself is a capability. Never log or display that URL.
- Network/parser errors return fixed sanitized messages. HTTP errors expose
  only status and an optional response ETag for conflicts; error bodies are
  bounded to **zero bytes** and are never logged, parsed, or returned.
  Metadata response bodies are capped at 8 MiB; content downloads at 256 MiB.
  Paged listings fail rather than silently truncate after 100 pages.
- Native cancellation selects the network/body future against a 25 ms token
  heartbeat. Dropping the losing reqwest future interrupts a pending request or
  body read. Browser cancellation uses an AbortController with the same heartbeat,
  plus a 120-second timeout. Cancellation cannot undo a write already committed
  by a server: reread before deciding whether to retry.
- `ExpectedRevision::Any` is an explicit overwrite, never the normal-save
  default. HTTP 409/412 become `Conflict { actual }` without retrying a write,
  creating a renamed conflict copy, or silently switching to an unconditional save.
- Reads return an ETag/rev from the **download response**. They never query
  metadata after downloading and attach a potentially newer revision to old bytes.
  If the mandatory foundation revision cannot be obtained, the read fails closed.
  A successful write without its response revision is reported as an ambiguous
  success requiring reread, not retried.

## Provider wire contracts

### Normal connection discovery and starting folders

No user-entered drive ID or starting-folder ID is needed for a normal OneDrive
connection. After browser consent has produced an explicit delegated token,
the connection layer calls:

```rust
GraphProvider::discover(
    GraphDiscoveryConfig {
        api_url,       // global Graph root or explicit literal-loopback mock
        access_token,  // supplied by the account/auth layer; never Debug/logged
        location: GraphStartLocation::AppFolder,
    },
    cancel,
) -> StorageFuture<'static, GraphDiscovery>
```

`GraphDiscovery` has public `provider: GraphProvider`, `drive_id: String`,
and `root: StorageId` fields. Use the ready provider and returned root directly;
do not ask users to copy IDs. The result and config intentionally have no Debug.
The future starts no requests until polled; invoke it only as part of explicit
Connect after authentication, not on startup or before consent.

For **AppFolder**, only GET `/me/drive/special/approot` is requested, selecting
`id,folder,specialFolder,parentReference`. The result must be a folder with
`specialFolder.name == "approot"` and a nonempty `parentReference.driveId`.
The returned parent drive ID configures the provider; no broader `/me/drive`
metadata call is needed under the narrow scope. Missing/invalid metadata,
403/404, CORS or redirects fail rather than falling back to the full drive.

For an explicitly broader-consented **DriveRoot** connection, discovery GETs
`/me/drive?$select=id`, then `/drives/{id}/root` selecting only folder/identity
metadata, checks that the root's parent drive matches, and returns its actual ID.
No children lists, file content, previews, owner names, or document uploads are
requested in either discovery mode. The same endpoint trust, error redaction,
bounded metadata and cancellation rules apply on native and wasm.

Official documentation scope/account considerations:

- The app-folder guide documents delegated `Files.ReadWrite.AppFolder` for
  personal and work/school OneDrive, with access confined by the service to the
  registered app's folder. The specific special-folder API permission table
  lists AppFolder for personal accounts but broader Files.Read for work/school;
  tenant/service differences must be live-tested. Never broaden consent or
  silently retry a failed app-folder request against the root.
- `/me/drive` metadata lists delegated Files.Read as least privilege;
  a writable ordinary-drive experience needs explicit `Files.ReadWrite`
  consent rather than assuming an app-folder token grants full-drive access.
  The adapter does not inspect tokens or mint/refresh/request OAuth scopes.
- `/me` requires delegated user authentication, not app-only tokens. App-only
  access, SharePoint library selection, national clouds and enterprise
  registration/approval are outside normal automatic discovery here.
- Official docs say first app-folder access can create `Apps/<Entra app name>`,
  and `/me/drive` may provision a licensed user's OneDrive. These server-side
  setup effects are part of explicit Connect, **not** document transfers.
- The app-folder guide contains one shorthand `/me/special/approot` example;
  this implementation uses the documented API route
  **`/me/drive/special/approot`**.

Other adapters expose `default_root(&self) -> StorageId`, without I/O:
DAV uses its explicitly configured collection, Dropbox uses the empty namespace
root (already app-folder restricted for app-folder tokens), Box uses `0`,
and Drive uses the supported `root` alias. OAuth scope and account policies
still determine visible content; Drive's `drive.file` scope does not imply
access to every existing document. Self-hosted DAV still needs an explicitly
configured collection/server and credentials, not a provider-specific folder ID.

### Explicit new-file API

All five concrete providers expose the same inherent API without changing the
shared storage trait:

```rust
pub fn create_file<'a>(
    &'a self,
    parent: &'a StorageId,
    name: &'a str,
    bytes: &'a [u8],
    cancel: CancelToken,
) -> StorageFuture<'a, Entry>
```

Call this for new uploads, not `write` with a manufactured REST item ID.
Successful creation returns the actual stable `Entry.id`, `revision: Some(...)`,
name and byte size. The caller must use that returned identity/revision for later
saves; the input parent/name is not a file ID. Creation does not mutate any
existing document binding, retry, rename automatically, fetch a post-write
revision, or fall back to overwrite. Missing/invalid revisions and unexpectedly
renamed server responses fail explicitly instead of reporting a save.

Names must be nonblank single components: dot/dot-dot, slash/backslash and control
characters are rejected before any request. Provider-specific filename restrictions
are also enforced by the service. Cancellation and payload limits apply before
network I/O. HTTP 409/412 at creation or final commit returns Conflict.

- **DAV:** parent is a scoped DAV collection ID; PUT child with
  `If-None-Match: *`, using its response ETag.
- **Dropbox:** parent is a folder path (empty for root) or `id:...` folder ID.
  POST upload with add/strict_conflict/no autorename, returning the response
  stable ID and rev.
- **Graph:** parent is an item ID in the configured drive. Small files use named
  child PUT with `If-None-Match: *` and conflictBehavior=fail. Larger files use a
  named-child upload session with the same Missing/fail policy, then aligned
  sequential anonymous fragments. Creation requires final HTTP 201 and the
  created item's response ID/eTag. Session-final name conflicts are surfaced and
  native session cleanup is attempted, without overwrite/replay.
- **Box:** parent is a folder ID (`0` for root). POST the new-file endpoint,
  not the file-version endpoint, with attributes `{name, parent: {id}}` before
  the content part. Box's atomic new-upload name check returns 409 for occupied
  siblings. No list/preflight-then-overwrite sequence. HTTP 201 and one returned
  file entry with ID/etag are required.
- **Drive:** user-approved policy is **distinct new file IDs with a duplicate-name
  warning**, not Missing-name. The user selected “Allow distinct new files with
  a duplicate-name warning (Recommended).” POST creates a new resource using
  multipart/related metadata `{name, parents: [parent], mimeType}` followed by
  binary content. It never addresses or updates an existing file ID, performs
  a name preflight, or claims If-None-Match imposes sibling-name uniqueness.
  Same-name creations are allowed to return separate stable IDs. The caller must
  warn before this action and use the returned ID/response ETag for later saves.
  HTTP 200/201, a nonempty returned ID, matching name, and response ETag are
  required; absent ETag is ambiguous success requiring reread, never retry.

Payload bounds remain DAV/Graph 256 MiB, Dropbox 150,000,000 bytes and Box
50,000,000 bytes; Drive multipart creation is capped at 5,000,000 bytes.
No background creation/network activity occurs at construction.

### Nextcloud / ownCloud WebDAV

`WebDavConfig` takes `collection_url` and explicit `Authentication`.
`WebDavProvider::id(relative_path)` builds a scoped DAV URL without I/O.

- List: `PROPFIND`, `Depth: 1`, explicit `DAV:` XML property request for
  `displayname`, `getcontentlength`, `getetag`, and `resourcetype`.
- Parsing is namespace-aware and accepts only successful propstats. It handles
  escaped/numeric references and CDATA, excludes the requested collection itself,
  rejects DTDs/malformed XML/non-child or untrusted hrefs, and sorts/deduplicates IDs.
- Read: `GET`, full HTTP 200 body, response ETag.
- Write: binary `PUT`; Match uses `If-Match`, Missing uses `If-None-Match: *`,
  Any has neither. Only completed 200/201/204 writes return the response ETag.
  Payloads above 256 MiB are unsupported in this bounded implementation.
- Login Flow v2, OCS shares, locks, and provider version history are not implemented.

### Dropbox API v2

`DropboxConfig` requires both API and content roots plus an explicit access token.

- List: POST `files/list_folder`, nonrecursive, no deleted items; subsequent
  POST `files/list_folder/continue` with the opaque cursor.
- Read: POST `files/download`, JSON `Dropbox-API-Arg`; revision comes from the
  download's `Dropbox-API-Result` header.
- Write: POST `files/upload`, octet-stream and JSON argument. Match sends
  `{".tag":"update","update":rev}`; Missing sends `add`; Any sends `overwrite`.
  All use `autorename: false` and `strict_conflict: true`.
- Header JSON escapes non-ASCII text. Files above 150,000,000 bytes are unsupported;
  upload sessions, versions, and share links are not implemented.

### Box

`BoxConfig` supplies explicit API/upload roots and access token.

- List: GET `folders/{id}/items` with marker pagination and selected
  `id,name,size,etag,type` fields.
- Read: GET `files/{id}/content`; native validated one-hop downloads never forward
  the token. Browser reads instead GET `files/{id}?fields=id,download_url`,
  validate the returned file ID and top-level preauthorized URL, then download
  without account credentials or redirects. No share link is created, and
  `shared_link.download_url` / `authenticated_download_url` are not fallbacks.
  A byte-bound download response ETag is required in either build.
- Write: POST `files/{id}/content`, `If-Match` for Match, multipart body with
  `attributes` **before** `file`. Random boundaries are checked against the
  file bytes. This replaces an existing stable file ID; `write(..., Missing)`
  is Unsupported. Use the explicit new-file endpoint described above for creation.
  Files above 50,000,000 bytes need a separate chunk-upload implementation.
- `list_versions` requests `files/{id}/versions`, currently bounded to one
  1000-item page; an advertised additional page returns an explicit error.
- `create_share_link` is a separate PUT operation requiring an explicit
  Collaborators/Company/Open choice. List/read/write never create shares.

### Google Drive v3

`GoogleDriveConfig` supplies explicit API/media-upload roots and access token.

- List: GET `files`, escaped parent query with `trashed = false`, selected
  `id,name,size,mimeType`, and page-token pagination. Size is parsed from v3's
  decimal string. `Entry.revision` is **None**: v3 list resources do not contain
  per-file HTTP ETags. The numeric `version` field is not an If-Match token.
- Read: GET `files/{id}?alt=media`, with the media response ETag required.
- Write: PATCH `files/{id}?uploadType=media`, binary body and exact If-Match
  token for Match. Only existing IDs and simple updates up to 5,000,000 bytes.
  `write(..., Missing)` remains Unsupported: it must never transform a stable-ID
  save into creation. Distinct-file `create_file` separately POSTs
  `files?uploadType=multipart&fields=id,name` with multipart/related metadata and
  bytes, without If-Match/If-None-Match headers on the collection.
  Name-unique creation, Workspace document export, resumable uploads, versions,
  and sharing are not implemented.
- A live-service test must establish that the media ETag is usable for
  conditional updates. No substitute metadata-version token or post-read
  metadata lookup is used.

### Microsoft Graph (global OneDrive / SharePoint)

`GraphConfig` supplies the API root, a drive ID, and access token. Item IDs are
stable **within that configured drive**, not globally portable.

- List: GET `drives/{drive}/items/{id}/children`, selected
  `id,name,size,eTag,folder,file`. Next-page URLs must stay within the configured
  API origin/path.
- Read: GET item `content`; a native preauthenticated download hop is validated
  and receives no bearer token. Browser reads GET item metadata with
  `$select=id,@microsoft.graph.downloadUrl`, validate the file ID and returned
  URL, then fetch directly without account credentials or redirects.
  The final download ETag is required; metadata ETags are never attached to bytes.
- Small write: PUT item `content`, octet-stream and If-Match.
  `write(..., Missing)` is Unsupported for an existing item ID; use
  `create_file` to upload a named child and obtain its assigned ID.
- Larger writes (over 4 MiB), or explicit `write_large`, create a session with
  POST item `createUploadSession`, If-Match, and
  `item.@microsoft.graph.conflictBehavior = "fail"`.
- Session fragments are sequential 3,276,800-byte requests (10 × 320 KiB,
  below 60 MiB); only the last may be shorter. Content-Length and Content-Range
  have a consistent total. Intermediate HTTP 202 acknowledgements must identify
  the exact next offset; final HTTP 200/201 must return the committed item ETag.
  No bearer is sent with fragments. Unexpected offsets/416/network failures
  abort rather than guess a resumable offset or silently overwrite.
- Native errors/cancellation attempt unauthenticated DELETE cleanup, bounded
  to two seconds. Browser-abandoned sessions expire server-side.
  Explicit resumability/retries, sovereign-cloud endpoints, and payloads above
  256 MiB are outside this implementation.
- Graph checks If-Match at **session creation**. The mocks establish request
  semantics, not a service-level CAS guarantee against edits made after the
  session was created. Concurrent-edit-at-final-commit behavior still requires
  live acceptance tests; never claim that a local mock proves it.
- `list_versions` is a separate paginated GET; `create_share_link` is a
  separate POST `createLink` requiring Organization/Anonymous scope, with
  read-only `type: view`. Sharing is never called by ordinary storage operations.

## Browser and live-service limitations

Reqwest 0.12's wasm backend does not expose a redirect policy. A target-gated
browser Fetch transport therefore uses `redirect: error`, `credentials: omit`,
CORS mode, bounded streaming, and AbortController cancellation. It requires a
Window context. No token-bearing automatic redirect is allowed.

DAV servers must allow CORS methods PROPFIND/GET/PUT, Depth/Authorization/
If-Match/If-None-Match headers, and expose ETag. Dropbox must expose
Dropbox-API-Result. Other providers must expose their needed response headers.
Graph and Box browser reads use their documented authenticated metadata download
URL fields instead of `/content`, which would redirect after an authenticated
CORS preflight. The URL is fetched immediately and anonymously after provider-host
validation; subsequent redirects are rejected, even to trusted domains.
Both providers also expose
`read_via_download_url(&self, id: &StorageId, cancel: CancelToken)
-> StorageFuture<ReadResult>` natively, allowing the same metadata/download logic
to be tested against local servers without changing the established native
`StorageProvider::read` route.

Missing direct URLs return Unsupported. Expired URLs, CORS failures, absent/
unexposed download ETags, policies preventing downloads, or a CDN requiring
another redirect fail explicitly. This work does not introduce a proxy, create
shares to bypass a policy, pass bearer tokens to a CDN, or relax redirect rules.

These providers may return download ETags different from API mutation ETags, or
omit them entirely. The adapter does not guess, relabel a content hash as a provider
revision, or fetch newer metadata to make a read appear saveable. Missing ETags
produce explicit errors; matching media/API tokens still need live tests,
particularly for Graph, Box, and Drive.

## Verification recorded for this work

Windows x64 native mocks compile/run against the real foundation, reqwest,
rustls-capable workspace dependencies, and local TCP servers. Tests verify
conditional headers and bodies, byte-bound read revisions, stale DAV/Dropbox
writes preserving mock-server bytes, Graph fragment constraints and abort paths,
explicit shares/versions, sanitized errors, DAV XML escaping/DTD rejection,
URL trust boundaries, pre-cancellation/no constructor traffic, and cancellation
while waiting for headers or streaming content.
Metadata-download mocks additionally verify Graph/Box URL-field requests,
correct file ID checks, no bearer/cookies on signed downloads, refusal of
untrusted URLs and shared-link fallback, rejected redirects, and download ETags
instead of metadata ETags.
The successful DAV PUT-without-ETag mock commits the submitted bytes and returns
204, then verifies an explicit ambiguous-success error instead of a saved revision,
unchanged caller bytes, and no subsequent request (retry or metadata fallback).
Creation mocks verify Graph/Box returned server IDs/revisions, same-name rejection
preserving existing bytes, DAV/Dropbox's common API, Graph large-file ranges and
final-commit name race cleanup, untrusted upload-session rejection, and ambiguous
creation responses that omit a revision or silently rename. Drive multipart mocks
verify separate POSTs for duplicate names return distinct IDs without modifying
existing bytes, metadata/content ordering and escaping, response ETag identity,
subsequent conditional stable-ID PATCH, sanitized errors, redirect rejection,
missing-ETag ambiguous success and cancellation while awaiting a response.
Invalid names, parent/payload guards and pre-cancellation make no requests.
Discovery mocks verify the minimal-scope app-folder-only metadata path, explicit
broader drive/root path, returned provider/IDs, folder/drive validation,
no fallback/escalation on errors, rejected redirects/untrusted endpoints,
pending-request cancellation, no unpolled/pre-cancel network, and valid network-free
default roots for other providers.

After the owner supplied module/dependency hooks, the integrated Windows x64
`cargo test -p thinoda-platform --test remote_storage` passed all 38 mock tests
with the actual workspace quick-xml 0.42. Platform `cargo clippy --all-targets
-- -D warnings` also passed. The repository's combined wasm clippy command
(`-p thinoda-web -p thinoda-app -p thinoda-platform -p thinoda-ai -p thinoda-io
-- -D warnings`) passed after the browser streaming let-chain lint fix.
After the owner declared the complete direct browser features, standalone
`cargo clippy --target wasm32-unknown-unknown -p thinoda-platform --lib
-- -D warnings` passed without CLI feature overrides or app feature unification.
Scoped native test clippy and formatting also passed.
Platform wasm `--all-targets` additionally
encounters an unrelated native-file-time test type mismatch outside these adapters.

These focused checks are **not** a substitute for full workspace CI, browser
execution, other native operating systems, live provider accounts, enterprise
policies, token revocation, or independent security review. An earlier pre-hook
rustc/artifact check was superseded by the integrated Cargo tests.

## Protocol references

- [Graph drive metadata](https://learn.microsoft.com/en-us/graph/api/drive-get?view=graph-rest-1.0)
- [Graph special folders](https://learn.microsoft.com/en-us/graph/api/drive-get-specialfolder?view=graph-rest-1.0)
- [App-folder access and scope/account guidance](https://learn.microsoft.com/en-us/graph/onedrive-sharepoint-appfolder)
- [Graph upload sessions](https://learn.microsoft.com/en-us/graph/api/driveitem-createuploadsession?view=graph-rest-1.0)
- [Graph browser downloads](https://learn.microsoft.com/en-us/graph/api/driveitem-get-content?view=graph-rest-1.0)
- [Dropbox HTTP API](https://www.dropbox.com/developers/documentation/http/documentation)
- [Box upload file version](https://developer.box.com/reference/post-files-id-content/)
- [Box file resource download URL](https://developer.box.com/reference/files-resources.md#file-full)
- [Drive media uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
- [WebDAV protocol (RFC 4918)](https://www.rfc-editor.org/rfc/rfc4918)

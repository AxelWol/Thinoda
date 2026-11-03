_Last updated: 2026-11-03 07:19:33 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Web first-start performance: concept and critique

Status: **investigation and implementation in progress**. A user reports that
a clean-cache launch on iPad Safari or Edge can take about two minutes. This is
a report from a real device, not yet a reproducible measurement in this
workspace. The goal is to make the app usable sooner, not merely to show a
spinner sooner.

## Scope and measurement

Keep all current editing, import/export, presentation, accessibility and
offline/update capabilities. Cloud-provider work remains paused; this work
does not enable providers or provision registrations.

Measure a release build from a clean, isolated browser profile. Separate HTML
and resource discovery, transfer bytes and time, Wasm compilation/instantiation,
application initialization, first paint and the first successfully handled app
interaction. Compare cold and warm runs under the same network and CPU
conditions; capture failures and offline behavior as well. A browser trace helps
locate blocking work, but debugger instrumentation may itself slow Wasm, so
measure time to interaction separately without the debugger enabled. Record
the artifact and toolchain used, sample counts and device/browser limitations.
Desktop emulation or WebKit automation is not an actual iPad measurement.

On 2026-11-02 the then-deployed Pages build exposed a hashed Wasm preload.
For a gzip-capable request, its Wasm response reported `application/wasm`,
`Content-Encoding: gzip`, and a compressed length of 8,907,096 bytes. This is
an observation of the previous deployed build, not a post-merge benchmark or
evidence of iPad download or compilation time. Cache, network and host behavior
still need measurement against the new release artifact.

## Proposed approach

The existing site is a static Trunk/Wasm app with a versioned, complete
service-worker shell; see [WEB.md](WEB.md#offline-use-and-updates). Audit the
actual release boot chain before choosing the changes:

1. Start the essential app download and compilation as early as the existing
   deployment and browser support allow. Avoid duplicate downloads and
   unnecessary buffering; use streaming only where the response and browser
   support it. Any compressed or alternate artifact must have a working,
   tested fallback on browsers and static hosts that lack that capability.
2. Defer or parallelize genuinely optional initialization or assets only when
   their dependencies, ordering and failure behavior are understood. Keep the
   initial usable app, document recovery and graphics error handling intact.
   The current Rust/Wasm bundle is not assumed to be dynamically splittable;
   distinguish deferred initialization from actual code splitting.
3. Keep a lightweight loading surface visible until the app is ready. Report
   real phases and, only when byte counts are reliable, real download progress;
   otherwise show an indeterminate state. Respect reduced motion, accessibility,
   appearance and the existing startup-failure troubleshooting page.
4. Preserve version-matched offline shells and safe updates. Validate that
   optional work eventually completes or reports a recoverable failure, and
   that a user can invoke an optional feature immediately after startup.

## Rubber-duck review

| Challenge | Decision and verification |
|---|---|
| A fast spinner is not a fast app. | Compare cold time to a successful interaction, not only HTML paint or a loading animation. Attribute download, compilation and initialization separately. |
| The two-minute report may have multiple causes. | Establish a baseline before changing code; do not infer iPad behavior from a desktop trace or claim a device fix without device evidence. |
| Wasm may already compile while downloading. | Inspect the generated loader and network waterfall before adding a second fetch or replacing a working streaming path. |
| Streaming is not universal. | `instantiateStreaming` needs an `application/wasm` response. Test a real non-streaming/MIME fallback and surface genuine network, compilation and startup failures rather than masking them. |
| Compression can exclude older Safari or double memory use. | Feature-detect the actual format/API, preserve the uncompressed path, and measure transfer, compile time and memory. Do not assume the static host adds `Content-Encoding`. |
| A percentage can be false. | Fetch streams may expose decoded bytes while `Content-Length` describes encoded bytes. Show percentages only for matching, known units; otherwise use phase-based indeterminate progress. |
| Background work can delay the foreground on a small iPad. | Avoid saturating the main thread, memory or network before the app is usable; benchmark parallel and on-demand scheduling rather than assuming more concurrency is faster. |
| A deferred module may be needed immediately. | Define dependencies explicitly, make first-use loading safe, preserve pending user actions and report failures. Test representative features before and after the background work finishes. |
| Rust crates are not automatically separate browser modules. | Measure the actual Wasm and assets. Do not claim code splitting when only initialization was postponed, or remove optional features to shrink the bundle. |
| The service worker can fetch the shell again and compete with startup. | Measure the cold network waterfall, while retaining a complete, content-versioned offline shell, waiting-update safety and scoped cache cleanup. Test first install, reload, update and offline recovery. |
| A loader can hide the existing graphics error. | Preserve the WebGPU/WebGL failure page and technical details. Test failed fetch, invalid Wasm, unsupported graphics and retry without leaving a permanent overlay. |
| First-run UI can regress accessibility or localization. | Use semantic status/error announcements, keyboard access, reduced-motion handling, available translations and light/dark contrast. Keep the original application controls reachable. |
| Cross-tab and crash recovery may be affected by deferred storage. | Do not delay or discard recovery decisions, change document ownership or silently fall back to volatile storage. Exercise recovery and multiple tabs independently. |
| Profiling can distort results. | Compare trace and debugger-off measurements, use isolated profiles, record the same artifact/conditions before and after, and disclose unavailable real-device or GPU measurements. |

### Recovery integration blocker

The separate, unintegrated browser-recovery draft specifies one nominal
30-second asynchronous discovery budget. Scheduling, parsing and browser
throttling mean this is not a hard wall-clock limit or a measurement of startup.
The draft itself still lacks an integrated rescue path for oversized or
unreadable sources; those cases can leave editing gated. The currently shipped
synchronous path can download listed unreadable records as described below,
but that does not make the draft safe to integrate. Browser document migration
also runs before recovery and has not been measured against this budget. Before
integration, bound synchronous work and provide explicit pending, failure,
retry and rescue behavior without allowing unresolved snapshots to be silently
overwritten or discarded. Verify the deadline and ownership behavior before
using that build as the startup baseline. A proposed resumable scan also
missed a snapshot written by an uncoordinated same-origin writer after a
checkpoint; it was removed rather than treating an incomplete scan as empty.
Retained deletion tombstones can therefore still exhaust the draft's
discovery limits and gate editing across retries and reloads. The draft's
discard/verification work remains unintegrated, and a bounded protocol that
accounts for uncoordinated writers is still needed.

The unintegrated recovery sources and standalone test harness are preserved
under [work in progress](work-in-progress/freeform/README.txt), outside the
application and its CI test selection. They are not a production recovery
implementation. The same directory contains a historical toolbar-placement
candidate; an adapted toolbar fix now lives in the application, with its
separate critique in [FREEFORM.md](FREEFORM.md).

Early editing is acceptable only for a newly fenced, distinct document while
discovery remains visibly pending. Late recovery results must not interrupt
active edits, replace an edited document or silently overwrite the same open
file. Recovery decisions must remain user-driven, with source snapshots
preserved until a safe handoff; restore into a separate unsaved document or
otherwise prove equivalent isolation. If that cannot be guaranteed, editing
must remain gated. Measure the first safe interaction and completed recovery
discovery separately.

### Current loader and offline-shell checkpoint

The static loader now exposes the storage-preparation and app-start phases as
an accessible status, announces the existing storage, startup and graphics
error paths as alerts, and offers a reload link. A bootstrap guard also reports
application-module and Wasm loading failures before Rust can render an error.
Trunk's custom bootstrap pattern dynamically imports its generated bindings and
preserves the normal Wasm fetch, preloads and success event while catching
initialization failures. Unrelated script errors are not classified as
bootstrap failures. The brand mark gently pulses while pending; reduced-motion
users get no animation.

Some browsers can retain a failed module response across a normal reload.
The error link therefore adds a fresh, temporary retry query, also remembered
in session storage when available for an ordinary browser reload. On both the
initial load and a retry, an explicit module script applies the generated
preload's integrity metadata before the bindings are imported. This avoids
relying on modulepreload enforcement alone: in a desktop WebKit probe,
modulepreload did not prevent a modified initial module from executing.
The normal Trunk build supplies that metadata. A custom build with
`--no-sri` or `TRUNK_BUILD_NO_SRI=true` fails closed at startup rather than
loading an unverified app module.
A successful bootstrap removes the temporary retry markers.
If session storage is unavailable, the error link still supplies its own
marker. The service worker maps only that retry query on a whitelisted app
module to its cached, version-matched URL; it does not cache arbitrary
queries, documents or other resources. An isolated desktop WebKit check against
an actual HTTP 503 confirms both the retry link and a browser reload recover
without losing unrelated URL parameters, fragments or existing local data.
Actual tampered HTTP responses for the initial module were also rejected in
desktop Chromium, Firefox and WebKit; a subsequent retry loaded the valid
module without losing the same URL or local data.

The loader now waits for both the browser renderer and the app's initial
launch state before it disappears. Previously a shortcut sent immediately
after loader removal could arrive while the splash still blocked commands.
The loader does not assert that every optional background task has finished.

On a cold online load, registering the service worker before the Wasm bootstrap
completed caused its complete-shell precache to request the same Wasm file
while the foreground request was still downloading. Registration now waits
until that bootstrap succeeds or reports a scoped failure. Existing cached
HTML without the new bootstrap signal still registers normally. Bootstrap
completion is **not** the same as the fully interactive app: storage and UI
initialization can still be running when precaching starts.

On 2026-11-02, six matched cold-start pairs compared the previous and new
production shells in headless Edge 156 on Windows, with fresh browser contexts
and a synthetic shared 4 MiB/s bandwidth limit for uncompressed Wasm responses.
Both shells used the identical 27,318,703-byte Wasm file. Median loader removal
changed from 13.20 s to 7.36 s; median time to a handled Open shortcut changed
from 13.42 s to 7.65 s. The shortcut reached a test file-picker shim that
immediately cancelled; it did not test a real operating-system picker.
Pairwise interaction improvements ranged from 42.5% to 44.3%.

The change postpones the second full download rather than eliminating it; total
transfer and Wasm size are essentially unchanged. An early tab close can leave
the first offline installation incomplete. The complete versioned shell and
waiting-worker update policy are unchanged, and the worker is still registered
after a bootstrap failure. Tests cover deferred installation, offline reload,
Wasm and module failures, and an update waiting while an old tab remains open.
These synthetic desktop results show a reproducible contention improvement,
not the magnitude of an improvement on a real iPad, a compressed production
connection, a warm launch, or a busy document. Actual iPad Safari and Edge
acceptance remains open.

Six additional interleaved pairs compared the original and optimized shells
over an unthrottled loopback connection, with a fresh browser profile for each
cold run and a service-worker-controlled reload for its warm run. All
interactions succeeded. Median cold interaction was 0.65 s before and 0.64 s
after; median warm interaction was 0.25 s for both. These small, noisy samples
do not establish a warm-performance bound, and the OS file cache was not cleared.

After adding the initial-module integrity guard, four interleaved pairs compared
the preceding and guarded production shells with identical Wasm in headless
Edge on Windows over an unthrottled loopback connection. All cold and
service-worker-controlled warm interactions succeeded. Median cold interaction
was 1.02 s before and 0.83 s after; median warm interaction was 0.33 s and
0.34 s, respectively. These few noisy samples do not establish an overhead
bound. Neither comparison is a physical-iPad or representative-network result.

As a size-only probe, applying `wasm-opt -Oz` to the already optimized final
Wasm reduced it from 27,318,620 to 25,889,852 bytes; gzip level 6 reduced the
same artifacts from 9,738,066 to 9,649,777 bytes (about 0.9%). This is not a
fresh size-optimized production build and neither startup time nor behavior was
measured for it. The probe was not adopted; transfer size alone does not
establish a useful iPad improvement.

### On-demand template previews: critique

The Home gallery previously constructed and laid out every template the first
time any thumbnail was painted. It now resolves only visible thumbnails and
reuses each sample by template, language and appearance. Offscreen controls
still receive their normal layout, focus and activation metadata; scrolling or
filtering generates newly visible previews synchronously in that frame. Recent
templates, selection, creation and document state are unchanged. This moves
some work to first use, rather than loading a separate Wasm module or reducing
download size.

The cache now retains at most twice the current number of templates (98
entries for 49 templates), using least-recently-used eviction across language
and appearance combinations. This accommodates all templates in two
combinations without eviction; switching among more combinations can rebuild
older previews synchronously on first use. An `Arc` already returned to a
caller remains valid after eviction. Tests cycle through every template, all
ten languages and both appearances, verify the entry bound and eviction order,
and compare rebuilt samples with the originals. The entry limit is not a
byte budget or a measurement of actual memory savings, and it does not limit
references retained elsewhere. Cache hits scan a recency queue of at most 98
keys; the limit trades bounded retention for lookup work and synchronous
reconstruction after eviction.

A focused 480 × 360 headless egui fixture requested four of 46 templates on the
first all-categories frame; the old cache built all 46. With the current
catalogue it requests four of 49. Tests cover subsequent scrolling and recent
items, every template in all languages and both
appearances, and gallery selection and creation. The earlier bounded-cache
candidate passed 18 startup, offline-update and language checks plus Meeting
Board and Swimlane gallery creation in each of desktop Chromium, Firefox and
WebKit. The later Venn build passed the same startup/offline/language checks
and Meeting Board, Swimlane and Venn creation in those three engines. After
Empathy Map was added, all four gallery workflows passed in those engines.
Board thumbnails now use one normal scene pass for objects, fixing duplicate
painting of translucent shapes; frame and grid previews remain simplified.
Browser tests do not assert thumbnail pixels or exercise eviction across all
language/theme combinations. First-scroll latency, eviction overhead and actual
memory impact have not been benchmarked.

Six interleaved pairs compared the previous and new production builds in
headless Edge on Windows over unthrottled loopback. All cold and controlled warm
interactions succeeded. Median cold time to a handled Open shortcut was
1.036 s before and 1.036 s after; warm medians were 0.373 s and 0.395 s.
These few noisy samples do not demonstrate a startup improvement or an
overhead bound. The shortcut reached a cancelling file-picker shim, not a real
picker. The [measurement record](benchmarks/gallery-startup-2026-11-03.json)
includes the artifact identities, paired timings and limitations. The OS file
cache was not cleared, and no physical iPad or representative network was
measured.

### Browser accessibility checkpoint

A desktop Lighthouse navigation audit of the production shell identified the
transparent, unlabeled text input created for egui's browser IME. The browser
accessibility tree exposed the canvas and that input, but not the application's
board controls or their egui accessibility names. The current eframe 0.37 web
backend explicitly discards `accesskit_update`; enabling egui accessibility
metadata alone therefore does not expose it to the browser. Eframe's optional
web speech synthesis is not an accessibility-tree bridge. The loader's semantic
status and reduced-motion behavior are useful, but they do not establish
browser screen-reader access to the editor or toolbox. Do not mask the input or
add a generic label merely to improve an audit score without preserving real
editing and IME semantics. Investigate an accessible browser bridge and verify
real screen readers, keyboard focus, editing and localized labels before closing
the F1-F3 accessibility gates.

One possible integration point is egui's `Plugin::output_hook`, which receives
the completed `FullOutput` before eframe handles and discards it. A browser
bridge could enable AccessKit and observe its tree there without forking
eframe. This is a direction to investigate, not an implemented bridge: it also
needs a synchronized semantic DOM, action requests, focus and IME ownership,
localized names, modal behavior, document privacy and performance checks. Board
content painted directly on the canvas may require additional semantic nodes.
An output observer or generic canvas label alone would not meet these gates.
Browser recovery warnings additionally have localized live-alert regions, but
these do not expose the editor controls and do not replace real screen-reader
testing.

Browser **Auto** language previously always resolved to English because the
native locale provider did not read the browser's language. It now uses the
browser preference while preserving explicit Settings choices. The document
language follows the active catalog, the English static loader identifies its
own language, and each recovery announcement retains the language it was
written in if the setting changes. This improves the language metadata of
existing DOM content; it is not an editor accessibility bridge. On Windows,
German and both Portuguese variants, an unsupported-language fallback and
explicit preferences were checked in Chromium, WebKit and Firefox; Chromium
also exercised a settings change and reload. Auto does not react immediately to
a changed browser preference, and the static bootstrap remains English. No real
screen reader was tested.

AccessKit's [published adapter list](https://lib.rs/crates/accesskit) does not
include a web adapter. Its [experimental web adapter discussion](https://github.com/AccessKit/accesskit/discussions/514)
also reports unreliable screen-reader focus in an early prototype. That work is
useful prior art, not evidence that a browser bridge is ready to ship.

### Recovery safety critique

The synchronous recovery path now checks a per-claim owner before operations,
retains queued bytes after failed writes or removals, and reports incomplete
discovery without bulk-discarding the data it could not inspect. A suspended
session must resume before cleanup. A listing that visibly changes length or
loses an indexed key fails instead of treating a partial scan as complete.
An incomplete scan or a later failed recovery load also blocks the app's
bulk-discard action and offers a non-destructive way to leave the recovery
dialog. For listed records, **Download raw record** rechecks the cooperative
lease and copies the original browser storage value without modifying it.
The downloaded file contains the exact UTF-16LE code units without a BOM,
including unpaired surrogates; it is not a `.thinoda` document, and it does not
repair or re-import the record. A download request may be blocked or cancelled
silently by the browser, so the user must verify the downloaded file. Records
that could not be listed, ambiguous keys, unavailable storage and allocations
too large for memory remain outside this rescue path.

Focused desktop Edge, Firefox and WebKit checks cover unreadable records and
byte-for-byte preservation of all 65,536 JavaScript code units, including lone
surrogates and an empty value. Failure, retry and lost-lease checks preserve the
originals; these do not establish device-level acceptance. This narrows observed
stale-owner hazards; it cannot make localStorage's
separate read, check and write operations atomic. Same-length key changes or
competing tabs can still race between them, large snapshots remain
subject to localStorage quota and main-thread costs, and the asynchronous draft
above is not integrated. Cleanup errors are logged rather than silently ignored.
An open document retries removing its saved snapshot on a later recovery cycle;
persistent failures or closing before a successful retry can still leave a saved
snapshot to be offered again.
In desktop Chromium, Firefox and WebKit, a real quota failure while adding a
synthetic JPEG larger than 4 MiB retained the previous snapshot, announced
the failure, and still allowed a native download containing the exact image
bytes. These checks do not recover the oversized unsaved board or establish
storage limits on other devices or browser configurations. Device-level
reload, quota, concurrent-tab and large-board acceptance therefore remain open.

The image-card and quota tests now share a bounded JPEG fixture: browser
encoders can produce different sizes for the same quality, so it lowers
quality as needed and still verifies that the result exceeds 4 MiB without
exceeding the 8 MiB upload limit. The earlier Firefox image-card run exposed
this fixture assumption before exercising the app. A successful bounded
fixture is not evidence that every user image or quota size is supported.

The production bundle at source checkpoint `03cbac1` passed 56 headless
Chromium regressions,
including startup and offline-shell failures, browser-language selection,
mixed-board editing, mini-map input, SVG/PNG/JPEG/PDF/PowerPoint export,
image-card export, recovery lease loss, quota retry and the real-quota case.
Seventeen board and image-card scenarios, eleven startup/offline checks and
three raw recovery checks passed on that bundle in headless WebKit and Firefox
on Windows. PDF rendering checks use a separate Chromium viewer. An earlier
build also passed language checks in those engines; desktop and
390-pixel screenshots showed the warning and dismiss control within the
viewport. An earlier run had one failure when its loopback
server refused a static-resource connection; that case passed three isolated
repeats and the later full run passed on a dedicated server. These checks do not
replace physical-device, screen-reader or cross-platform CI acceptance.

## Acceptance

- Demonstrate a measured improvement in cold time to a usable app against the
  same baseline, along with transfer and startup-phase measurements.
- Verify first use of deferred features, all existing feature paths affected by
  the changes, startup failure/retry, accessible status, reduced motion, warm
  reload, offline shell and safe update behavior.
- Run the relevant native and Wasm checks and production build. Document
  remaining limitations, including whether actual iPad Safari and Edge were
  available for acceptance.

References: [MDN: streaming WebAssembly](https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/instantiateStreaming_static),
[MDN: resource timing](https://developer.mozilla.org/en-US/docs/Web/API/Performance_API/Resource_timing),
and [MDN browser compatibility data for DecompressionStream](https://raw.githubusercontent.com/mdn/browser-compat-data/main/api/DecompressionStream.json).

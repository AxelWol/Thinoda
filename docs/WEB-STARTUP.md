_Last updated: 2026-11-02 20:10:56 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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
The draft still lacks a non-destructive in-app rescue path for oversized or
unreadable sources; those cases can leave editing gated. Browser document
migration also runs before recovery and has not been measured against this
budget. Before integration, bound synchronous work and provide explicit pending,
failure, retry and rescue behavior without allowing unresolved snapshots to be
silently overwritten or discarded. Verify the deadline and ownership behavior
before using that build as the startup baseline.

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
Trunk's custom bootstrap pattern preserves its normal Wasm fetch, preloads,
bindings and success event while catching initialization failures; a scoped
listener handles failure to import the application module itself. Unrelated
script errors are not classified as bootstrap failures. The brand mark gently
pulses while pending; reduced-motion users get no animation.

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

As a size-only probe, applying `wasm-opt -Oz` to the already optimized final
Wasm reduced it from 27,318,620 to 25,889,852 bytes; gzip level 6 reduced the
same artifacts from 9,738,066 to 9,649,777 bytes (about 0.9%). This is not a
fresh size-optimized production build and neither startup time nor behavior was
measured for it. The probe was not adopted; transfer size alone does not
establish a useful iPad improvement.

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

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

The separate, unintegrated browser-recovery draft currently uses independent
timeouts that can accumulate to roughly 138 seconds under adverse conditions;
scheduling and parsing mean that is not a hard wall-clock limit. This is a
design finding, **not** a measurement of the existing app. Before integration,
recovery discovery needs one overall asynchronous deadline, bounded synchronous
work and explicit pending, failure and retry states, without allowing
unresolved snapshots to be silently overwritten or discarded. Permission
requests may overlap where safe. Browser throttling can delay callbacks, so the
deadline is not a guaranteed wall-clock bound. Verify the deadline and ownership
behavior before using that build as the startup baseline.

Early editing is acceptable only for a newly fenced, distinct document while
discovery remains visibly pending. Late recovery results must not interrupt
active edits, replace an edited document or silently overwrite the same open
file. Recovery decisions must remain user-driven, with source snapshots
preserved until a safe handoff; restore into a separate unsaved document or
otherwise prove equivalent isolation. If that cannot be guaranteed, editing
must remain gated. Measure the first safe interaction and completed recovery
discovery separately.

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

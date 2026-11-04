Experimental browser recovery: discovery checkpoint

This directory remains an unintegrated draft. It does not change the
production localStorage recovery path and is not part of the production
Playwright selection. Do not enable it as a large-board recovery solution.

Discovery now streams retained tombstones and empty session records instead
of charging them against the live-preview limits. Session keys are visited in
pages of 64; pagination is not a durable checkpoint or proof of completeness.
A preliminary audit and a final verification each read the sessions and
documents stores in one IndexedDB transaction. The final transaction checks
manifests, ownership tokens and exact claimed bytes, including records inserted
behind the session cursor. Changed claims are released rather than returned as
current. Unknown session, manifest-entry or document fields are treated as
uninspectable rather than discarded. Opaque sessions and bounded malformed
manifests are not claimed; a claim becomes invalid if its session becomes
opaque before the final audit. Oversized manifests can produce bounded
previews, but orphan discard actions reject them. Writers and discard
operations also reject opaque session or document records. An incomplete,
malformed, missing, changed, timed-out or interrupted scan remains gated. Even
a fully inspected snapshot returns a coordination warning, so the draft's
existing application adapter cannot enable editing until the post-snapshot
write-barrier blocker below is resolved.

Discovery retains document records, tombstones, discard receipts and session
data. It may change a session's fencing token, under its Web Lock and in the
same transaction that inspects that session. An empty session is fenced only
after a successful preliminary audit and a complete per-session inspection.
Existing byte mutations still check the owner token and revision in their own
transaction; a retry at the same revision is acknowledged only when its bytes
and deletion state match. The normal preview limits remain 256 nonempty
sessions, 512 live snapshots, 128 snapshots per session, 32 MiB per snapshot
and 64 MiB total. An IndexedDB implementation can materialize one oversized
record before its size can be checked; byte comparisons also cannot be
preempted.

Remaining correctness and liveness blockers:

* The final audit is a consistent snapshot, not an origin-wide write barrier.
  A same-origin writer that ignores Web Locks and fencing tokens can commit
  after that snapshot, including before its result is used. It can therefore
  make a previously complete result stale; token/revision checks on later
  mutations do not discover every newly inserted orphan. Production editing
  must not rely on this draft until a verified writer-coordination and
  revalidation design handles that boundary and old/uncoordinated clients.
* Tombstones and empty sessions still require full inspection. Work is bounded
  by the shared 30-second startup deadline and the 15-second transaction
  timeout; an oversized retained history can time out on every retry. A saved
  cursor cannot safely resume across reload or independent writes. Solve this
  without silently omitting records, deleting receipts or recoverable data,
  or relaxing transaction ownership/revision checks.
* On Windows Playwright WebKit, the standalone 1,300-tombstone/300-empty-session
  regression and a 1,000-tombstone retry scenario timed out in IndexedDB
  transactions. A separate malformed-record rescue check fails before rescue
  begins: IndexedDB rejects the synthetic record containing a Blob with
  "Error preparing Blob/File data to be stored in object store". The harness
  now reports that fixture error instead of waiting indefinitely for an abort.
  Chromium and Firefox completed all 73 checks. These are not iPad/Safari
  device results or performance acceptance thresholds.
* The Rust adapter and application draft have not been integrated, compiled,
  or exercised end to end. Production integration also requires migration,
  failure/quota/eviction and rescue review, real multi-tab and BFCache testing,
  accessibility/localization checks, physical-device startup measurements,
  and independent safety review.

The standalone browser harness is
crates\thinoda-web\tests\recovery-db.browser.html under this directory. Serve
the repository on an isolated local origin; it creates and removes synthetic
storage. It imports the draft's JavaScript directly and is not an application
integration test.

The existing Playwright dependency can also run the harness from the repository
root, with an optional case-insensitive test-name filter:

  node docs\work-in-progress\freeform\recovery\run-browser-tests.cjs chromium
  node docs\work-in-progress\freeform\recovery\run-browser-tests.cjs firefox
  node docs\work-in-progress\freeform\recovery\run-browser-tests.cjs webkit "uncoordinated"

Each invocation uses a fresh browser context and a random loopback port, serves
only the draft harness and the production browser-storage module it exercises,
and blocks non-local browser requests.
Individual checks have a 90-second deadline, and the runner also has an
overall deadline. WebKit failures are reported, not treated as passes; use
test-name filters to investigate the separate fixture and stress failures.

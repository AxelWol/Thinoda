Thinoda Freeform: preserved work in progress

These files are draft artifacts, not application modules or CI tests. They are
retained for future investigation and are not enabled in released builds.

recovery/crates/ preserves the unintegrated browser-recovery sources and their
standalone harness. Retained tombstones can exhaust discovery limits, and an
uncoordinated writer can invalidate a resumable scan. Rust compilation and
application-level recovery tests have not been completed. Do not integrate the
draft until these safety blockers and the documented startup and rescue limits
have been resolved and independently verified.
The relocated standalone harness passed 57 Chromium storage checks; that does
not verify application integration or resolve the blockers.

toolbar-overlap.patch.gz preserves an unvalidated candidate for avoiding other
board cards when positioning the selected-topic toolbar. It has not been
applied, compiled, or checked in the native application. Decompress it and
normalize CRLF to LF before checking whether it applies; review and test it
before use.

The recovery harness retains its draft-relative imports inside recovery/crates/
and uses the repository's live browser_storage.js for document-migration checks.
It is not part of the production Playwright test selection.

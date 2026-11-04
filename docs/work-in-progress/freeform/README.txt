Thinoda Freeform: preserved work in progress

These files are draft artifacts, not application modules or CI tests. They are
retained for future investigation and are not enabled in released builds.

recovery/crates/ preserves the unintegrated browser-recovery sources and their
standalone harness. Retained tombstones can exhaust discovery limits, and an
uncoordinated writer can invalidate a resumable scan. Rust compilation and
application-level recovery tests have not been completed. Do not integrate the
draft until these safety blockers and the documented startup and rescue limits
have been resolved and independently verified.
The standalone harness passes 73 storage checks in each of Chromium and
Firefox. WebKit has separate fixture and stress failures documented in
recovery/README.txt. These results do not verify application integration or
resolve the blockers.

rich-text/ explores explicit Unicode-scalar span operations in a standalone
prototype. It is not an application feature and has no model, persistence,
editor, clipboard or export integration; see rich-text/README.txt.

toolbar-overlap.patch.gz preserves the historical placement candidate. An
adapted and tested implementation now lives in the application; the archive
is retained for context, not as a patch to apply again. The current scope,
verification and remaining limits are recorded in docs/FREEFORM.md.

The recovery harness retains its draft-relative imports inside recovery/crates/
and uses the repository's live browser_storage.js for document-migration checks.
It is not part of the production Playwright test selection.

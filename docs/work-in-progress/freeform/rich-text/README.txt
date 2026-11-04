Experimental selected-range text: span-editing prototype

This standalone experiment is not compiled into Thinoda and does not add
rich-text editing, native-format fields, clipboard support or rendering.
Do not treat it as an implementation or enable it in the application.

It explores bounded, canonical non-overlapping style runs indexed by Unicode
scalar positions. Explicit insert/delete and formatting operations preserve
unaffected runs, distinguish inherited values from explicit false, and reject
invalid ranges and over-budget results without changing the source.

A separate draft-history experiment records before/after text, style, directional
selection and typing-style snapshots. It tests undo, redo, branch invalidation,
entry-count and estimated-byte eviction, and caret-only formatting without a
document undo entry. Snapshot estimates include inline entry storage, string
and span-vector capacities and style-reported owned allocations such as font
names. Budget reduction retains the nearest undo and redo entries even when
they exceed the estimate; outcomes report both eviction and over-budget
retention. Undo/redo transfers do not evict additional entries on their own.
Explicit composition begin/update/commit/cancel groups multiple preedits into
one document edit, uses the supplied final commit text rather than assuming
the last preview is final, and restores the original state and redo on cancel.
Surrounding-text deletion is modeled both inside and outside composition; the
prototype groups it with an active composition and rolls it back on cancel.
Failed edits and failed composition updates leave the draft and history intact.
An accounting overflow also rejects a history commit without losing a redo
branch or an active composition preview. Two deterministic 2,500-operation
suites compare span and history behaviors against independent per-character
models; 10,000 further operations exercise changing budgets while editing,
undoing and composing.

This has not been wired to egui's TextBuffer or its event and undo handling.
The commit API deliberately distinguishes an empty committed replacement from
cancel; mapping platform IME events, including empty preedit/commit, focus loss,
UTF-16 conversion and candidate-range semantics, remains unresolved. Caret
style inheritance and undo eviction are experimental policies, not product
decisions. Navigation and caret-only style changes keep the redo branch, whose
stored selection and typing style are restored on redo.

Local egui 0.37 source review shows that empty preedit or commit clears the
preedit, while the initial preedit has already replaced any selected text.
That differs from this prototype's explicit cancel-restores-original policy.
Egui also accepts DeleteSurrounding outside a composition and saturates its
cursor arithmetic; an oversized after-count can still reach TextBuffer as an
out-of-bounds range. This prototype rejects out-of-bounds requests instead.
The app's text_buffer_characterization integration tests exercise these actual
egui operations, including plain-string undo/redo. They simulate egui events
and do not establish which events physical IMEs send. The correct transaction
and cancellation behavior for real platform input, including whether surrounding
deletions survive a canceled composition, must be established before adapting
these APIs.

The history clones complete snapshots. Its budget is an estimate, not a hard
process-memory limit: it excludes the current draft, active composition, stack
container capacity, allocator overhead and temporary copies, and depends on
accurate style accounting and immutable/deep-copied style payloads. Shared
allocations can be double-counted. The nearest entries may exceed the budget;
their retention policy and the choice not to trim on undo/redo differ from the
current app document history and are not product decisions. Font attributes are
not validated, and text-byte/span limits are examples rather than product
limits. Formatting copies text and sorts run boundaries; no large-document
performance or peak-memory bound has been established. Paragraph and list
semantics, grapheme-safe navigation, bidirectional text, persistence,
clipboard/version migration, layout and export are not implemented. Those
gates and real IME/device acceptance remain open.

On Windows, run the prototype from the repository root with the installed Rust
toolchain:

  rustc --edition=2024 -D warnings --test docs\work-in-progress\freeform\rich-text\span_edit.rs -o target\rich-text-span-prototype-tests.exe
  target\rich-text-span-prototype-tests.exe

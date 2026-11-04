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
entry-count eviction, and caret-only formatting without a document undo entry.
Explicit composition begin/update/commit/cancel groups multiple preedits into
one document edit, uses the supplied final commit text rather than assuming
the last preview is final, and restores the original state and redo on cancel.
Failed edits and failed composition updates leave the draft and history intact.
Two deterministic 2,500-operation suites compare the span and history behaviors
against independent per-character models.

This has not been wired to egui's TextBuffer or its event and undo handling.
The commit API deliberately distinguishes an empty committed replacement from
cancel; mapping platform IME events, including empty preedit/commit, focus loss,
UTF-16 conversion and candidate-range semantics, remains unresolved. Caret
style inheritance and undo eviction are experimental policies, not product
decisions. Navigation and caret-only style changes keep the redo branch, whose
stored selection and typing style are restored on redo.

The history clones complete snapshots. Entry count is bounded and eviction is
reported, but the memory occupied by styles and snapshots is not bounded; font
attributes are not validated, and text-byte/span limits are examples rather
than product limits. Formatting copies text and sorts run boundaries; no
large-document performance or memory bound has been established. Paragraph
and list semantics, grapheme-safe navigation, bidirectional text, persistence,
clipboard/version migration, layout and export are not implemented. Those
gates and real IME/device acceptance remain open.

On Windows, run the prototype from the repository root with the installed Rust
toolchain:

  rustc --edition=2024 --test docs\work-in-progress\freeform\rich-text\span_edit.rs -o target\rich-text-span-prototype-tests.exe
  target\rich-text-span-prototype-tests.exe

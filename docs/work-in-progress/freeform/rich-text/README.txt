Experimental selected-range text: span-editing prototype

This standalone experiment is not compiled into Thinoda and does not add
rich-text editing, native-format fields, clipboard support or rendering.
Do not treat it as an implementation or enable it in the application.

It explores bounded, canonical non-overlapping style runs indexed by Unicode
scalar positions. Explicit insert/delete and formatting operations preserve
unaffected runs, distinguish inherited values from explicit false, and reject
invalid ranges and over-budget results without changing the source. Its tests
also compare operation sequences against a per-character oracle.

This does not validate font attributes or the memory occupied by styles, and
its text-byte and span limits are examples rather than product limits. It does
copy the text and sort run boundaries during formatting; no large-document
performance or memory bound has been established. It does not implement rich
undo/redo, caret formatting, paragraph/list semantics,
grapheme-safe navigation, bidirectional text, IME composition transactions,
clipboard/version migration, layout or export. Those gates remain open.

On Windows, run the prototype from the repository root with the installed Rust
toolchain:

  rustc --edition=2024 --test docs\work-in-progress\freeform\rich-text\span_edit.rs -o target\rich-text-span-prototype-tests.exe
  target\rich-text-span-prototype-tests.exe

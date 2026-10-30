_Last updated: 2026-10-30 08:51:46 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Task-aware Tree Table

Choose **Map > Structure > Tree Table**. The existing Tree Table structure now
opens a hierarchical task-column editor instead of only displaying the stacking
map layout. **Map** previews that layout; **Tree Table** returns. Outliner and
Gantt retain the same topic IDs and selection. There is no second task model.

## Editing

- Title, Start date, Due date, Duration and Progress are editable cells.
- Click a cell, or focus with Tab/arrows and press Enter/F2. Enter or Apply
  commits one document transaction. Tab/Shift+Tab applies and advances to an
  adjacent visible cell. Escape/Cancel discards. Moving to another cell discards
  an unapplied draft rather than silently saving it.
- Dates use `YYYY-MM-DD`; duration is an inclusive **working-day** count,
  1–3650, with empty/0 removing duration. Progress is an integer 0–100.
- Blank date/duration cells on untouched topics do not create tasks. Opening,
  selecting, collapsing, sorting and choosing columns never schedules topics.
  Independent To-Do checkboxes and completion counts remain unchanged.
- Start displays the engine's effective date; its editor changes the explicit
  start constraint, which may be blank on a dependency-scheduled task. Due is
  computed from start/duration. Editing Due derives duration using the effective
  start and sheet calendar; clearing Due clears duration, not the task or links.
- Weekdays, exceptions and finish-to-start dependencies use the existing
  planning engine. A successor can be pushed later by predecessors. **Task
  Planning** opens the same full editor as Data; **Work Calendar** edits the
  sheet calendar. A dependency cannot be made uncomputable by a table edit:
  the error stays visible and the previous document remains intact.
- Invalid dates, non-working dates, reversed ranges, invalid duration/progress,
  cycles and date overflows never partially apply. Errors remain beside
  Apply/Cancel, with localized field guidance and the scheduling diagnostic.
- Applied changes support Undo/Redo and native `.thinoda` save/reopen.
  Unapplied drafts are not saved/recovered. Changing selection, document,
  sheet, view, viewport ownership or revision discards them. IME candidate
  Enter/Tab/Escape do not complete a cell; touch scrolling does not activate it.

## View choices and accessibility

Title always remains available. The other four columns can be hidden separately.
Column choices and ascending sibling sort persist in local application settings
on desktop/web. **Document order** restores hierarchy order. Sorts compare title
by Unicode code points and other columns by computed dates/numeric values;
missing values follow present values, ties retain document order. Parents and
children stay grouped. None of these operations reorder or dirty the workbook.
Table collapse is temporary, does not alter map collapse, and resets when the
table owner changes. A selection received from another view reveals its ancestors.

The table has compact, theme-derived controls, localized column/row accessible
names, keyboard focus/selection synchronization, focus scrolling and horizontal
scrolling for narrow windows. Titles are truncated visually, with full hover
text and accessible names. English, German, Spanish, French, Danish, Dutch,
Portuguese (Portugal/Brazil), Italian and Polish use the existing catalogs.
OS screen-reader announcements and physical touch devices still need
platform-specific manual validation; headless accessibility/input checks are
not a claim of independent QA approval.

## Output boundary

Map images, PDF and main Print render the underlying **map**, not a screenshot
or tabular export of this view. The table help states that boundary.
Use **Gantt Chart > Export** for scheduled-task SVG/PNG/PDF/CSV output. There
is no dedicated Tree Table print/export in this slice. Native storage retains
task authoring data, not the local table presentation settings.

## Verification and self-review

Regression coverage includes real cell editing, invalid/no-op values, one-step
undo/redo, derived dependency dates, working-day exceptions, preserved To-Do,
native round-trip, stale revision/document/sheet/selection/view rejection,
row identity through reorder, fresh editor state after cancellation, keyboard
navigation, view-only columns/sort, touch swipes and IME candidate keys. The
360-point locale matrix covers all ten languages in light/dark.

Self-review found and corrected five interaction risks: primary-selection
changes could steal cell focus; Escape was consumed before cancellation; and a
global TextEdit ID could retain cursor/text-undo state across canceled drafts or
document owners. Enter on a focused Cancel button also needed to remain Cancel,
not the cell-completion shortcut. Draft editors now have owner/topic/revision/session-specific
IDs. Due-date conversion uses computed starts without storing them as explicit
constraints, including newly selected predecessors in the shared planning dialog.
Background sibling viewport rendering cannot consume, display or discard
another viewport's draft; Apply still rejects a different viewport owner.

Local Windows verification (2026-10-30), all invoked through `rtk proxy`:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo clippy --target wasm32-unknown-unknown -p thinoda-web -p thinoda-app -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings`: passed.
- App library: 264 passed, including 16 new table regressions and the new
  shared-planning dependency/due-date test. Core 138, canvas 104, IO 40, AI 31,
  and UI 44 library tests passed.
- `cargo test -p thinoda-app --test tree_table_locales --test planning_dates --test outliner_toolbar --test outliner_image_previews --test e2e`:
  all 80 passed, including the ten-language light/dark table matrix.
- `cargo test -p thinoda-platform --test tree_table_settings`: passed.
- A completely green `cargo test --workspace --lib` is **not claimed**:
  untouched platform test
  `tests::cloud_prefix_boundaries_case_and_nested_provider` fails at
  `crates/thinoda-platform/src/tests.rs:1117` and also fails in isolation.
  The remaining 79 platform library tests pass with that test filtered.
  One repeat also hit the untouched updates HTTP fixture's Windows
  `WouldBlock` socket race; its isolated rerun passed. Neither unrelated path
  was modified. Live AI was disabled for the library runs.

Limits: sort is ascending only, columns have fixed compact widths, disclosure
state is session-local, and the table is not virtualized for very large maps.
There is no dedicated table export or native screen-reader/device certification.

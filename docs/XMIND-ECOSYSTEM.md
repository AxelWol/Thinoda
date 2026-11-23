_Last updated: 2026-11-23 01:13:13 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# XMind ecosystem research, gap analysis and phase plan

This document records a source review of five public XMind-related projects and
compares their evidence with Thinoda's XMind import/export (`crates/thinoda-io/src/xmind.rs`).
It ends with a phase plan (X1-X7) that closes the gaps and reports delivery status.
[XMIND-RESEARCH.md](XMIND-RESEARCH.md) covers the product feature inventory.
[XMIND-USER-GUIDE.md](XMIND-USER-GUIDE.md) is the user-facing verification checklist.

**Evidence rules.** The facts below come from pinned upstream source, the two
desktop sample files in `crates/thinoda-io/tests/fixtures/` (Xmind "Vana"
26.03.04126, `dataStructureVersion` 3), and Thinoda tests. They do not claim that
the installed XMind app opens Thinoda files unless a row says that this was verified.

## 1. Reviewed projects

| Project | Pinned revision | Licence | What it is | Use for Thinoda |
|---|---|---|---|---|
| [xmindltd/xmind-generator](https://github.com/xmindltd/xmind-generator) | `af8e66ab57f480854671a45bccaa029151977534` | MIT | TypeScript builder that writes `.xmind` packages "in the same manner as Xmind UI applications". | Minimal-writer evidence: package layout, marker IDs, summaries, relationships, resource naming. Reader tolerance targets. |
| [xmindltd/xmindmark](https://github.com/xmindltd/xmindmark) | `78d5b6ba5813d46ab870aa91cdde19672e95184d` (v0.3.2, last change 2022) | MIT | Indented-text markup for maps, plus a converter to `.xmind`. | Grammar for a new `.xmindmark` import/export format (X6). The CLI's SVG rendering downloads a proprietary renderer and is **not** used. |
| [xmindltd/xmind-viewer](https://github.com/xmindltd/xmind-viewer) | `272b324bd82ea8567739c6617154d8d2556f6b50` (v1.1.2, deprecated) | MIT | Browser parser/SVG renderer for `content.json` and legacy `content.xml` + `styles.xml`. | Structure, shape, line-class and style-key vocabulary; legacy XML style resolution; reader tolerance rules. |
| [atkins126/mind-viewer](https://github.com/atkins126/mind-viewer) | `bee7fd57` | MIT text with XMind copyright removed | A rebrand of xmind-viewer with no new format knowledge. | None beyond xmind-viewer. Cite upstream, not the fork. |
| [xmindltd/mapify-mcp-server](https://github.com/xmindltd/mapify-mcp-server) | `753b8170` | MIT | One MCP tool that sends text, URLs or videos to the hosted mapify.so API (API key required) and returns image and edit links. | **No integration.** It contains no file-format data, needs a network account and would send user content to a third party. That conflicts with Thinoda's local-first and privacy defaults. Thinoda's AI features already use user-configured providers. |

### 1.1 xmind-generator (writer evidence)

- The package is `content.json` (sheet array), `metadata.json`
  `{"creator":{"name":"xmind-generator"},"dataStructureVersion":"2"}` and
  `manifest.json` (`file-entries` objects with `{}` values; the manifest does not
  list itself). Entries are STORE-compressed.
- Images are `resources/<sha256>.<ext>` and are referenced as `xap:resources/...`.
- Notes are written as `notes.plain.content` with a trailing newline.
- Markers are written as `{ "markerId": "..." }`. The exported catalog includes
  `smiley-embarrass`, `task-pause`, `flag|star|people-dark-blue`, the `people-*`
  group and the `arrow-*` group (`left`, `right`, `up`, `down`, `left-right`,
  `up-down`, `refresh`).
- Defects that readers must tolerate:
  - `week-web` is a typo for `week-wed`.
  - `month-aug` is missing.
  - Summary ranges come from lexicographic sorting, so `"(10,2)"` can be written.
- Optional fields are omitted freely: sheet titles, relationship titles, `class`.

### 1.2 xmindmark (text format)

- First non-empty line: central topic. Topic lines start with `-` or `*` plus
  whitespace and nest by indentation (a tab counts as four spaces).
- In-line markers:
  - `[n]` marks a relationship target; `[^n](title)` marks a relationship source.
  - `[Bn]` and `[Sn]` (case-insensitive) mark boundary or summary membership.
    Adjacent siblings with the same name merge into one inclusive range.
- Title lines `[B]: Title` / `[S]: Title` give the boundary or summary its title;
  the colon is optional. A summary title line may be followed by indented summary
  children. The default summary title is "Summary".
- `\[` and `\]` escape brackets.
- The upstream reverse converter is lossy and does not round-trip. Thinoda defines
  its own deterministic writer (X5).

### 1.3 xmind-viewer (reader evidence)

- Reads `content.json`. Without it, reads `content.xml` + `styles.xml`
  (requires `META-INF/manifest.xml`).
- Structures:
  - `org.xmind.ui.map[.clockwise|.anticlockwise|.unbalanced]`, logic, org-chart,
    tree, timeline, fishbone (left/right-headed) and matrix variants.
  - A plain `org.xmind.ui.map` means clockwise.
  - Unbalanced maps store their split in an extension:
    `{"provider":"org.xmind.ui.map.unbalanced","content":[{"name":"right-number","content":"3"}]}`.
- Line classes: `org.xmind.branchConnection.{curve,bight,roundedElbow,elbow,straight,fold,roundedfold}`.
- Topic shapes:
  - `rect`, `roundedRect`, `underline`, `doubleunderline`, `ellipse`,
    `ellipserect`, `ellipticrectangle`, `diamond`, `circle`, `parallelogram`,
    `hexagon`, `roundedhexagon`, `singlebreakangle`, `stack`, `fishHead*`.
  - Unknown shapes fall back to a rounded rectangle.
- Legacy XML styles:
  - `<topic style-id>` resolves through `styles.xml`.
  - Theme defaults come from `<master-styles>` / `<default-style style-family>`.
  - `svg:width` on a title is the custom topic width.
- Tolerance:
  - Trim style values (real samples contain `" line-through"`).
  - Accept numbers or numeric strings, with or without `pt`.
  - Treat `inherited` as "not set".

### 1.4 Desktop sample evidence (#136 fixtures)

- Packages contain `Thumbnails/thumbnail.png`, which is listed in `manifest.json`.
  The current `content.xml` is a warning stub ("This file can not be opened
  normally") and must never be imported when `content.json` exists.
- Colours are `#RRGGBBAA`.
- Topics may have `attributedTitle` runs (for example a bold segment) next to the
  plain `title`.
- The sheet `theme` has role entries:
  - topic roles: `centralTopic`, `mainTopic`, `subTopic`, `floatingTopic`,
    `summaryTopic`, `calloutTopic`, `importantTopic`, `minorTopic`, `expiredTopic`
  - other entries: `boundary`, `summary`, `relationship`, `zone`
  - `map` with `svg:fill`, `multi-line-colors` and `line-tapered`
- Values may be `inherited`.
- Sheet keys `topicOverlapping`, `arrangeableLayerOrder`, `zones` and an
  `org.xmind.ui.skeleton.structure.style` extension appear.

## 2. Gap analysis

Legend: ✅ supported · ◐ partial · ❌ missing / fails · ➖ deliberately not supported.
The *Before* column is the state at commit `5b94b80` (after the #136 fix).

### 2.1 Package and robustness

| Capability | Before | Gap | Target (phase) |
|---|---|---|---|
| Ancillary manifest entries (thumbnails) | ✅ | — | Keep (#136). |
| Missing `manifest.json` with images | ❌ hard error | Third-party writers omit it. | Use the archive inventory (X1). |
| Resources present but unreferenced | ❌ hard error | Note images and attachments leave bytes that the map does not reference. | Ignore them (still path- and size-checked) and record a warning (X1). |
| Topic image in an undecodable format (GIF, SVG, WebP...) | ❌ hard error | Full SVG and animated GIF/WebP remain unsupported. | Static GIF, WebP and a bounded SVG subset in modern and legacy packages are converted to PNG with a Resources warning; unsupported pictures are skipped and warned. |
| Attachment topics (`href: xap:resources/...`) | ❌ verbatim inert link | — | Import as an opaque Thinoda attachment (X2). |
| Other `attachment(s)`, `resourceRefs`, note images | ❌ hard error | The whole map fails. | Drop the payload and warn under Resources (X1). |
| Import warnings and consent | ❌ none for XMind | Lossy conversions are silent or fatal. | `xmind::import_with_report` plus the existing localized consent dialog (X1). |

### 2.2 Topic content (`content.json`)

| Capability | Before | Gap | Target |
|---|---|---|---|
| Title, children, folding, labels, href | ✅ | — | Keep. |
| `attributedTitle` rich runs | ◐ plain title kept | Run formatting is lost silently. | Keep the plain title and warn under Formatting (X1). |
| Notes `plain` | ✅ | — | Keep. |
| Notes `realHTML` / `ops` only | ❌ dropped silently | Text is lost. | Convert to plain text and warn under Notes (X1). |
| Markers in the catalog | ✅ | — | Keep. |
| Markers missing from the catalog | ◐ shown as "•" | `people-*`, `arrow-*`, `*-dark-blue`, `smiley-embarrass`, `task-pause` | Add catalog entries with localized names (X3). |
| `week-web` typo | ◐ unknown | — | Normalize to `week-wed` (X3). |
| Unknown marker IDs | ◐ silent | — | Keep the ID and warn under Markers (X1). |
| `taskInfo` (progress, dates, assignee) | ❌ silent | — | Warn under Markers (X1). Thinoda tasks use a different model. |
| Numbering (`numbering.numberFormat`, `prependingNumbers`) | ❌ silent | Not read or written. | Map to `child_numbering` both ways (X2). |
| Custom width (`customWidth`) | ❌ | — | Map to `style.width` both ways (X2). |
| Callouts (`children.callout`) | ❌ dropped silently | Text is lost. | Floating topic plus a relationship to its owner; warn under Other (X1). |
| Detached topics under non-root topics | ❌ dropped silently | — | Floating topic at the given position; warn under Other (X1). |
| Unknown or malformed child collections, malformed labels | ◐ silent, image → error | — | Warn under Other when non-empty or partially unrepresentable. |

### 2.3 Structure, layout and style

| Capability | Before | Gap | Target |
|---|---|---|---|
| Structure classes | ✅ 17 kinds | Unknown classes and variant-specific behavior are not represented. | Keep recognized mappings; plain `org.xmind.ui.map` is clockwise. Warn under Formatting for unrecognized classes and non-canonical variants approximated by a supported structure. |
| Unbalanced `right-number` extension | ❌ | The left/right split is lost. | Set `branch_side` on import; write the extension on export (X2). |
| Role theme and inline style properties | ✅ | Values are not trimmed; numeric `fo:font-weight` only partly parsed. | Trim values, treat `inherited` as unset, weights ≥ 600 are bold (X2). |
| Line classes `bight`, `fold`, `roundedfold` | ❌ unset | Desktop default theme uses `roundedfold`. | Map to the nearest Thinoda line (X2). |
| Shapes `circle`, `ellipticrectangle`, `doubleunderline`, others | ◐ unset | — | Map to the nearest shape; warn under Formatting when approximated (X2). |
| Sheet `theme.map` background, `line-tapered` | ❌ | — | Background (non-white) and tapered lines (X2). |
| Boundaries | ✅ | `master` handled. | Keep. |
| Summaries | ◐ | Reversed or out-of-range ranges are kept as-is; unresolved summaries vanish silently. | Normalize/clamp, warn under Other (X1/X2). |
| Relationships | ◐ | `controlPoints` ignored on import and export; unresolved endpoints vanish silently. | Import/export control points; warn under Other on drops (X1/X2). |
| Zones, comments, `topicOverlapping`, `arrangeableLayerOrder`, sheet extensions | ❌ silent | — | Warn under Other when non-empty; boolean `topicOverlapping` values, including `false`, are reported because their semantics are not preserved. |

### 2.4 Legacy XMind 8 (`content.xml`)

| Capability | Before | Gap | Target (X4) |
|---|---|---|---|
| Sheets | ❌ first sheet only | Extra sheets become floating topics. | One Thinoda sheet per `<sheet>`. |
| Attached / detached / summary topic collections | ❌ all attached | — | Respect `<topics type>`, positions. |
| Boundaries, summaries, relationships | ❌ | — | Import. |
| `styles.xml` `style-id` and theme | ❌ | — | Resolve topic styles. |
| HTML notes | ❌ | — | Convert to plain text (Notes warning). |
| Images (`xhtml:img`, `xap:attachments/`) | ❌ hard error | — | Embed decodable PNG/JPEG pictures and attach other files. Warn under Resources for anything skipped. |
| Numbering, `svg:width` | ❌ | — | Map like content.json. |

### 2.5 Export

| Capability | Before | Gap | Target (X2/X5) |
|---|---|---|---|
| `metadata.json` creator and `dataStructureVersion` | ✅ | — | Keep. |
| Relationship control points | ❌ | — | Write `controlPoints`. |
| Numbering, width, unbalanced split | ❌ | Only the private snapshot keeps them. | Write public fields. |
| Attachment topics | ❌ export refused | — | Write `resources/` and `href: xap:resources/...`. |

### 2.6 New format

| Capability | Before | Target |
|---|---|---|
| XMindMark `.xmindmark` | ✅ | Bounded UTF-8 import and deterministic export registered in format detection and the open/export dialogs. Supports indented topics, escaped brackets, numbered relationships, boundaries, summaries, and summary descendants. |

## 3. Phase plan

Every phase:

- adds focused tests (unit, fixture and fuzz seeds where relevant);
- keeps existing limits (1024 archive entries, 128 images, 8/32/64 MiB resource limits, 2000 nesting levels);
- passes `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and the targeted test suites;
- ends with a written critique and a commit with explicit pathspecs.

| Phase | Scope | Acceptance |
|---|---|---|
| **X1 Import report and source protection** | Add `xmind::import_with_report` and route `Format::XMind` through it. Classify every package/content failure as **fatal** or **recoverable** (§3.3). Recoverable content becomes a categorized warning, never a silent drop. Recover note text from HTML/ops. Turn callouts and nested detached topics into floating topics placed deterministically and linked to their owner. Report rich title runs, unknown markers, task info, zones, comments, internal `xmind:#` links and unresolved relationships or summaries. Detect password-protected packages (`encryption-data` in the manifest) with a dedicated error. **A native XMind file that imported with warnings is opened like a foreign import:** no writable path, Save asks for a new file, and the source bytes are never overwritten. | Maps that used to fail open through the consent dialog with exact warning categories. Strict `import_bytes` still refuses them with `ImportNeedsConfirmation`. The desktop samples report Formatting (bold title runs) and Other (unpreserved sheet settings). App tests prove that accepting a lossy import cannot save over the source. Adversarial tests keep every fatal case fatal. |
| **X2 Content fidelity** | Trim style values and handle `inherited`. Add line and shape aliases. Normalize summary ranges. Import and export relationship control points, numbering, custom width, the unbalanced split and attachments (archive-only resolver, §3.4). Read the sheet background and tapered lines. **Version the public projection** (`thinodaPublicProjection` in `metadata.json`). Older exports without the key are validated against projection 1, so their private snapshots stay trusted. | Round-trip tests for each field, including exports with the Thinoda metadata stripped. Checked-in pre-X2 Thinoda export fixture still imports through its private snapshot. Generator-style minimal fixtures import. |
| **X3 Markers** | Extend the catalog (`people` and `arrow` groups, dark-blue flag/star, `smiley-embarrass`, `task-pause`). Add the `week-web` alias. Localize every new name in all ten UI languages. | The marker picker shows the new groups; the catalog test covers ID uniqueness, and translation tests cover the new names. |
| **X4 XMind 8** | Tree-based legacy parser: multiple sheets, attached/detached/summary collections, positions, boundaries, summaries, relationships, `styles.xml` style resolution, HTML notes, numbering, width, pictures and attachments from `attachments/`, all with warnings instead of silent loss. | Fixture-based tests; legacy files import with only the expected warnings. |
| **X5 Export review** | Check the public JSON against the generator's minimal writer and the desktop samples. Document what remains private-snapshot only. | Tests that the export contains no unexpected keys and that every new public field round-trips with the private snapshot removed. |
| **X6 XMindMark (follow-up format)** | New `Format::XMindMark` (`.xmindmark`). UTF-8 input limited to 32 MiB and 200,000 topic nodes; line-numbered parser errors; deterministic writer; open/export dialog and localized preflight integration. | Tests cover tabs as four-space indentation, escaped brackets, numbered relationship endpoints with and without titles, boundaries, summaries and descendants, malformed-line locations, deterministic output, public format routing, and round trips. |
| **X7 Documentation** | Update this file's status, the user guide and the fixtures README. | Docs match the implementation. No claim of installed-app verification without evidence. |

### 3.2 Plan review (rubber duck) and resolutions

| Finding | Resolution |
|---|---|
| Accepting a lossy XMind import kept the writable path, so Save could overwrite richer source data. | X1: warnings make the document an import (`import_source`), and Save asks for a new file. |
| Publishing new fields changes the regenerated public JSON, invalidating older private snapshots. | X2: versioned public projection; projection 1 reproduces the pre-X2 output exactly. |
| A broad "error → warning" change could downgrade safety failures. | X1: explicit fatal/recoverable contract (§3.3) with adversarial tests. |
| Attachment `href` resolution needed safety rules. | X2: archive-only resolver (§3.4). |
| "No warnings for desktop samples" contradicted the rich-title warning. | Acceptance names the expected Formatting warning and, following the sheet-setting audit, Other. |
| Tasks and zones *do* have Thinoda models. | Neither the reviewed sources nor the samples document XMind's task or zone JSON, so mapping would be guesswork. They are reported and listed as deferred interoperability, pending real samples. |
| Callout flattening needs traversal before creation. | X1: one shared traversal covers every child collection for resources, IDs and relationships; deterministic placement. |
| Thinoda-to-Thinoda round trips can hide broken public JSON. | X2/X5 tests strip Thinoda metadata before re-import. Installed-app verification stays an explicit manual release check in the user guide. |
| Internal links need remapping across sheets. | Thinoda has no topic-link target type (only draw.io page links). `xmind:#…` links are kept as text and reported under Links; real topic-link navigation is a deferred feature. |
| XMindMark is not a prerequisite. | Moved to X6, after the `.xmind` phases. |

### 3.3 Fatal versus recoverable

Fatal (the whole import fails):

- unsafe or duplicate archive paths;
- too many entries, oversized entries, entries declared in the manifest but missing;
- ZIP or JSON syntax errors in `content.json` or `manifest.json` (an invalid
  `metadata.json` is only Thinoda's sidecar: it is ignored and the public content is read);
- image or resource byte/count budgets exceeded, counting unused resources too
  (unused resources are size-checked but never decoded);
- a protected (versioned) private snapshot that fails validation;
- a workbook that fails `Workbook::validate`;
- password-protected packages (dedicated message).

Recoverable (content is kept where possible, a warning is recorded):

- topic pictures that cannot be decoded or use unsupported formats or alignment (Resources);
- unused resources, note images, `resourceRefs`, and attachment payloads that cannot be resolved (Resources);
- rich title runs and approximated shapes or lines (Formatting);
- HTML or delta-only notes (Notes);
- unknown markers and task info (Markers);
- internal or unresolvable links (Links);
- callouts, nested detached topics, unresolved relationships or summaries, zones, comments and unknown child collections (Other).

### 3.4 Attachment resolver

- Only `xap:resources/<name>` (and, for XMind 8, `xap:attachments/<name>`) is
  resolved, and only against entries already validated by the archive inventory.
  The name must be a single path component; percent-encoded separators, `..`
  and absolute paths are rejected, and the link is reported under Links.
- The bytes stay opaque (`application/octet-stream`). They are never decoded,
  executed or opened automatically.
- The display name comes from the topic title and is sanitized with
  `attachments::safe_name`.
- If a resource is both a topic picture and an attachment, both uses share the
  same bytes.
- A missing target keeps the topic and reports Resources.

### 3.1 Deliberate non-goals

- **mapify-mcp-server**: hosted service with no format content (see §1).
- **xmindmark SVG CLI**: depends on a proprietary downloaded renderer.
- **Rich title runs and rich notes**: Thinoda topic titles are plain outside board
  cards, and notes are Markdown-source. The text is kept and the formatting loss is
  reported.
- **Task info and zones (deferred)**: Thinoda has scheduling tasks and zones,
  but no reviewed source documents XMind's JSON for them. They are reported until
  real samples are available.
- **Comments and audio notes**: no Thinoda equivalent; reported.
- **Internal `xmind:#` topic links (deferred)**: Thinoda has no topic-link target; kept as text and reported.
- **Password-protected (encrypted) XMind files**: the encryption scheme is
  proprietary. These files fail with a dedicated, actionable error.

## 4. Delivery status

This section is updated as phases land. See the commit history on the XMind
branch for per-phase critiques.

| Phase | Status |
|---|---|
| X1 | Done. `xmind::import_with_report` reports categorized warnings; the manifest is optional and the archive is the inventory; encrypted packages fail with a dedicated message; callouts and nested detached topics become floating topics linked by a relationship (callouts stacked right of their owner, detached positions kept relative to the owner); notes fall back to visible HTML/delta text; lossy XMind opens never keep a writable path. Lossy XMind files cannot be merged (merge uses the strict import). |
| X2 | Done. Public projection 2 imports/exports style aliases and approximations, summary ranges, control points, numbering, custom widths, unbalanced branch sides, map background/tapering, and opaque topic attachments. Projection 1 still validates pre-X2 private snapshots; round-trip tests also remove Thinoda metadata to exercise public content. Attachments share bytes with image resources when deduplicated and are never decoded or opened. |
| X3 | Done. Added generator marker groups and variants with localized names in all ten languages; `week-web` imports as `week-wed`; task pause has a distinct glyph in vector/SVG rendering. |
| X4 | Done. The bounded XML importer reads multiple sheets, attached/detached/summary collections, positions, relationships, boundaries, numbering, topic widths, styles/theme defaults, notes and safe local image/attachment members. Recoverable losses are reported. |
| X5 | Done. The public projection is checked against an explicit XMind key allowlist; projection-2 fields round-trip with the Thinoda snapshot removed, while projection-1 snapshots remain covered by a checked-in fixture. |
| X6 | Done. `.xmindmark` is a bounded, localized import/export format; supported hierarchy, boundaries, summaries and relationships round-trip deterministically, while export preflight identifies omitted topic details, annotation styling/routing and other sheets. |
| X7 | Done. User guidance and this status/critique reflect the implemented behavior and its loss boundaries. |

**Attributed-title safety follow-up:** When a JSON topic has no plain title,
valid attributed runs now supply its visible text. A mismatch between plain
and attributed text, malformed runs, or unsupported run formatting produces a
Formatting warning; imports with that warning stay protected from in-place
overwrite. Duck check: text recovery does not preserve run formatting or
establish installed-XMind round-trip parity, so those gaps remain open.

**Sheet-setting safety follow-up:** Modern XMind sheet overlap, layer-order
settings and extensions are reported as Other when present with content rather than silently
discarded. Duck check: even `false` may carry meaningful source intent, so it
must trigger the same source protection as `true`; absent, null and empty
collections do not. This is a conservative warning, not preservation or
verified interpretation of these settings. The checked-in desktop samples
contain these settings and consequently now report Other alongside Formatting.

**Malformed and unknown topic content follow-up:** Unknown or approximated
structure classes, non-array marker data, malformed labels and malformed child collections now
produce loss warnings. Duck check: valid entries are still imported where
possible, while a warning protects the source from in-place overwrite; the
warning does not reconstruct missing content or guarantee exact layout.

### 4.1 X2 review critique

- **Compatibility:** Adding public fields changes the serialized content, so
  snapshot validation is projection-versioned. A checked-in pre-X2 export proves
  that older packages remain trusted; unknown projection versions fall back to
  public parsing rather than restoring private state.
- **Loss boundaries:** Exact supported style values are mapped directly.
  Unsupported shapes use the documented nearest Thinoda shape and emit a
  Formatting warning. Widths outside Thinoda's supported range, non-default
  numbering options and unsupported relationship control-point shapes are
  reported rather than silently coerced.
- **Archive safety:** Attachments resolve only through validated archive entries
  and safe single-component resource paths. Bytes remain opaque and are exported
  with the existing resource budgets. A resource referenced as both an image
  and attachment is stored once.
- **Evidence limit:** The new public round trips, legacy projection fixture,
  generator-style ranges, and the two supplied desktop fixtures are covered by
  tests. Installed-XMind interoperability has not been claimed or verified.

### 4.2 X3 review critique

- **Marker scope:** Catalog entries preserve the reviewed generator IDs and
  have local glyph/color representations. XMind's custom marker groups and
  `taskInfo` remain unsupported; unknown IDs still produce a Markers warning.

### 4.3 X4 review critique

- **XML tree shape:** Thinoda's legacy compatibility writer emits child
  `<topic>` elements directly, while XMind 8 commonly groups them inside
  `<children><topics type="attached">`. The importer accepts both layouts and
  the round-trip test protects the direct-child form.
- **Position semantics:** Detached topic coordinates are absolute in the XML.
  The shared deterministic placement pass also creates an owner relationship;
  its derived position is therefore replaced with the source coordinates after
  placement so the imported map does not drift.
- **Style and data loss:** `styles.xml` definitions and theme defaults resolve
  before topic overrides. HTML notes are deliberately flattened and warned;
  unsupported relationship control points and detached-topic conversions are
  also reported rather than silently discarded.
- **Package safety:** XML depth, archive entry sizes, resource count/size,
  relative resource paths and image decoding remain bounded. External links are
  never fetched, and non-image attachment bytes remain opaque.
- **Evidence limit:** The generated multi-sheet legacy package fixture covers
  sheets, style/theme resolution, numbering, width, notes, images, attachments,
  boundaries, summaries, relationships and detached positions. XMind desktop
  itself has not been used to verify re-exported legacy packages.

### 4.4 X5 review critique

- **Snapshot trust:** A Thinoda snapshot is restored only when its saved public
  projection and content digest match the package. Projection 1 remains
  reproducible for pre-X2 files; a future projection must be versioned rather
  than changing the old serializer in place.
- **Interoperability boundary:** Tests now enforce the reviewed public key set
  and reopen projection-2 output with the private snapshot removed. This catches
  both accidental Thinoda-only public fields and regressions hidden by exact
  same-app restoration.
- **Deliberate privacy/loss:** Private-only model data remains in Thinoda's
  snapshot and is not represented as invented XMind fields. Other applications
  receive only the documented XMind projection; third-party edits invalidate
  the private snapshot and are read from their public content.
- **Evidence limit:** The checked-in projection-1 file, generated projection-2
  maps, and supplied desktop packages are exercised locally. No claim is made
  that a particular installed XMind release accepts every generated field.

### 4.5 X6 plan and outcome review

- **Grammar boundary:** The parser follows the pinned XMindMark specification:
  the first nonempty line is the root, `- ` and `* ` introduce indented topics,
  tabs count as four spaces, and escaped square brackets remain literal.
  Relationship endpoints are paired by their numeric marker, and group IDs
  scope to one parent so repeated numbers in separate branches do not collide.
- **Model fidelity:** Boundaries, summary ranges, summary descendants, and
  relationship titles round-trip. Export is deterministic and selects one
  sheet; preflight calls out omitted sheets, topic details and annotation
  appearance/routing. Embedded resources fail explicitly rather than being
  silently discarded.
- **Resource limits:** UTF-8 input is capped at 32 MiB and 200,000 topic nodes.
  Syntax and model errors identify the source line; external URLs are not
  fetched. Escapes, nested summaries, malformed pairing, and repeated
  deterministic exports have focused coverage.
- **Evidence limit:** These tests validate Thinoda's implementation against
  the reviewed grammar; they do not claim installed XMindMark CLI or XMind
  Desktop interoperability.

### 4.6 Final integration rubber-duck review

| Question | Failure considered | Resolution / remaining boundary |
|---|---|---|
| Does an incidental desktop package member replace the authoritative map or block opening? | Thumbnail/metadata entries or legacy XML conflict with current content. | `content.json` remains authoritative; ancillary entries are validated as safe archive members and ignored. The supplied with/without-thumbnail fixtures must import to the same workbook. |
| Can the XMind 8 fallback shadow a modern package? | A package contains both generations of content. | The importer selects `content.json` first; legacy `content.xml` is used only when modern content is absent. The legacy parser is bounded and keeps unsupported-content warnings explicit. |
| Can a changed public projection silently trust stale Thinoda-private data? | New fields change the JSON the snapshot authenticates. | Projection versions separate the pre-X2 and current public shapes; tests cover both the checked-in older file and metadata-stripped public round trips. |
| Does “XMind support” imply every feature round-trips? | Tasks, zones, topic links, rich runs, comments, and visual/resource details are not fully represented. | Import reports recoverable losses, export preflight identifies known losses, and docs distinguish unsupported/deferred features. Installed XMind interoperability remains unverified. |
| Does the new `.xmindmark` format silently flatten model data or load remote content? | One-sheet grammar cannot encode board details, resources, or annotations' style/routing. | Export selects one sheet, refuses embedded-resource output, and surfaces omitted sheets/details/unsupported master boundaries in preflight. No URLs or external renderer are fetched. |
| Are UI paths and localization part of the feature, not just library code? | File dialogs, export commands, or locales omit the new format or warnings. | The format is in import/export catalogs, extension routing, export menu and command IDs; app tests require every supported language catalog key. |

The resulting support is intentionally evidence-bounded: Thinoda robustly imports
the reviewed current and legacy XMind structures and supports a documented
XMindMark subset, but does not claim exhaustive coverage of every future or
undocumented XMind payload.

### 4.7 Static WebP picture recovery: review and remaining work

- [x] Recover bounded static WebP pictures in modern `content.json` packages as
  PNG, preserving decoded pixels and applying orientation. Report Resources
  loss and require conversion consent because original format and metadata are
  not preserved.
- [x] Limit input and output to 8 MiB, dimensions to 4096 per side and
  4 megapixels, and decoder allocations. Count all source package resources
  against the archive limit and reserve per-sheet referenced bytes and resource
  counts so recovered images do not displace existing resources.
- [x] When a WebP resource is also an attachment, retain its original opaque
  bytes and use a separate PNG for the picture. If the extra resource would
  exceed a budget, retain the attachment and report the skipped picture.
- [x] Recover static WebP pictures in legacy `content.xml`, retaining any
  original attachment and caching conversions across sheets.
- [x] Recover static GIF pictures in modern and legacy packages, retaining
  transparency and any shared attachment under the same resource budgets.
- [x] Recover a bounded static SVG subset in modern and legacy packages,
  rasterizing to PNG with a resource-loss warning and preserving shared
  attachments.
- [x] Recover gzip-compressed SVG/SVGZ pictures within the same XML size and
  feature limits, preserving the original compressed attachment.
- [x] Exercise mutable SVG and SVGZ picture bytes through the XMind fuzz
  target, including shared attachments and deterministic seed checks.
- [x] Recover SVG pictures containing embedded static PNG/JPEG/WebP/GIF data URLs, with
  per-picture limits on image count and total decoded pixels.
- [x] Recover static SVG stylesheets and inline styles with comments at rule or
  declaration boundaries, while rejecting comments that could obscure tokens.
- [ ] Preserve GIF/WebP/SVG metadata and animation; expand SVG picture support
  beyond the bounded static subset; validate interoperability in installed XMind.

**Rubber-duck critique:** Converting a picture is not a lossless package
round-trip even when its pixels survive, so warning and source protection are
mandatory. Animated or malformed images are skipped, not flattened. Modern
packages reserve the source paths each sheet references and charge each
conversion to the sheets using it, avoiding the previous assumption that every
sheet used every file. Unsupported and duplicate source paths still count
conservatively, so some pictures near a limit may be skipped. Shared
picture/attachment paths use separate visual and opaque resources and charge
both where a sheet uses both; the attachment is never replaced.

The legacy parser reserves the original package plus each converted PNG for
every sheet, even when a picture is not also an attachment. This is intentionally
conservative and can skip pictures near a limit; cached successes and failures
avoid repeated transcoding of the same source. Legacy and modern conversion
both report Resources loss and never silently flatten animated WebP.

**Static GIF critique:** A GIF may contain animation even when its first frame
looks like a complete picture. Recovery decodes at most two bounded frames and
rejects multi-frame or malformed streams rather than silently flattening them;
the Resources warning protects the source because GIF metadata and encoding
are not preserved. Transparency is retained as decoded, and shared attachments
remain byte-for-byte unchanged. Full SVG and animation support remain open.

**Static SVG critique:** SVG is not a native editable image resource, so
rasterization loses vector content and requires the same consent and source
protection as GIF/WebP conversion. A shared bounded renderer preserves intrinsic
size up to 1024 pixels on the longest side and never fetches external images.
The XMind subset permits simple static CSS and local fragment references
(for example gradients), including presentation attributes such as `fill` and
`filter`. Rule- and declaration-boundary CSS comments are accepted; comments
inside tokens or functions are rejected. Validation examines the uncommented
CSS so external URLs and animation cannot hide behind comments, and rejects CSS
escapes. Embedded image
elements are limited to 16 base64 PNG/JPEG/WebP/GIF images and four megapixels
in total; APNG, animated WebP and animated GIF,
invalid images and external references are rejected. Image links must use
unqualified `href` or the XLink namespace, including its alternate prefixes.
Images with orientation metadata requiring transformation are rejected because
the SVG renderer may not apply it.
SVGs combining embedded
images with `use` are rejected to avoid unbounded image replication. It also
rejects external CSS URLs, escapes, imports, variables and animation,
scripts, foreign objects, animation elements, external links,
malformed XML and excessive XML events rather than silently dropping
their content. More SVG features, original vector
preservation and animation remain open; the package and attachment budgets
remain unchanged.

**Compressed SVG critique:** SVGZ expands through an 8 MiB limit before the
same static-SVG validator runs. Invalid, truncated, concatenated or oversized
gzip streams are rejected rather than partially decoded. The original bytes
remain attached when shared, and rasterization still requires loss consent;
compression support does not make SVG vectors editable.

**Picture fuzzing critique:** Structured picture envelopes mutate resource
bytes without breaking the ZIP container and exercise source preservation.
Raw SVG mutations can reach XML parsing, while most gzip mutations fail their
checksum before reaching it. These seeds do not replace bounded-decoder tests,
legacy-package coverage, or the sanitizer acceptance campaign.

**Embedded WebP and GIF critique:** Static WebP uses the same bounded decoder as
standalone pictures, and rejects animation and orientation transforms before
the SVG renderer sees it. The original SVG remains available as a shared
attachment, and conversion still requires consent. The SVG renderer normally
ignores GIF frame offsets; XMind's validated static GIFs are composited onto
their logical screen and replaced with PNG before rendering. Animated or
malformed GIFs are rejected rather than flattened. This normalization applies
only to the XMind path and does not change draw.io rendering.

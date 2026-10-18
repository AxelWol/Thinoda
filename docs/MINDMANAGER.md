_Last updated: 2026-10-18 11:42:49 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# MindManager compatibility

Thinoda supports a **bounded conversion subset**, not certified or lossless
MindManager compatibility. Keep original files. Use `.thinoda` for complete
Thinoda workbooks.

## File and save policy

| Operation | Policy |
|---|---|
| Open / Import `.mmap` | Read the ZIP's exact `Document.xml`. Unsupported content is listed by category before a converted copy is opened. Cancel leaves the current document unchanged. |
| Open / Import `.xmmap` | Read standalone UTF-8 MindManager XML. Generic `.xml`/extensionless files are recognized only by the namespaced `Map` root, not by extension alone. |
| Save / Save As `.mmap` | Write a real ZIP containing Application/2003 `Document.xml`, never renamed JSON/Markdown. Only a single-sheet workbook whose fields are supported can be saved this way, with a scope confirmation. Unsupported workbook fields cause an error; use `.thinoda` or explicit Export conversion instead. Subsequent Save uses the same guarded path. |
| Export `.mmap` | Export the current sheet (or an explicitly selected sheet through the library), with a warning that unsupported content and theme appearance can be omitted. The open workbook is not retargeted or marked saved. |
| Save / Export `.xmmap`, legacy `.mmp` | Not implemented. `.xmmap` is import-only; there is no claim of generic XML, all historical versions, encrypted maps, or template/theme compatibility. |

Opening a MindManager file creates an unsaved converted document, **not a writable
handle to that source**. Save asks for a separate destination. Save, sheet-copy
and export operations reject the imported source path and resolved symlink
aliases, even after saving a separate copy. Recovery snapshots retain this
source-protection information. CLI conversion also requires a separate path.
Writes use temporary files and atomic replacement; failed encoding does not
replace the target.

MindManager's map schema contains one `OneTopic`. Thinoda does not invent a
multi-sheet `.mmap` package. The reader preserves additional top-level roots in
older permissive input as separate Thinoda sheets instead of silently keeping
only the first; this is an import accommodation, not a claimed native feature.
Saving a multi-sheet workbook as `.mmap` is refused. Export explicitly selects
one sheet; `.thinoda` retains the complete workbook.

## Supported-feature matrix

| Feature | Read | Write |
|---|---|---|
| Central topic, ordered attached hierarchy, Unicode titles | Yes | Yes |
| Sheet name | Map `Name`, otherwise central title | Map `Name` |
| Creator and description | Document `Creator.UserName`, `Description.Comments` | Same |
| Created/modified dates, other document/custom metadata | Warn if not represented | Require conversion consent; omitted |
| Font face, positive finite size, bold, italic, underline, strikeout, alignment | Explicit topic values | Explicit topic values |
| Topic shape | Rectangle, rounded rectangle, oval, diamond, capsule, line, none | Same seven mappings |
| Opaque topic fill and font color | Eight-digit `FFRRGGBB` | Same; font and fill are independent |
| Alpha/transparency, line/border styles, widths, theme defaults | Warn if not represented | Require consent for explicit unsupported fields; no alpha/channel guess |
| Collapsed topics | Yes; other view indices warn | View index 0 |
| Plain notes | `NotesXhtmlData.PreviewPlainText` | Escaped preview and XHTML `pre`, preserving line breaks |
| Rich notes / note resources | Warn; preview retained when available, XHTML formatting not retained | No rich-text reconstruction |
| Hyperlinks | First HTTP(S) or `mailto:` link | One HTTP(S) or `mailto:` link |
| Extra/internal/file/archive links | Warn; not activated or fetched | Require consent; omitted |
| Text labels | `TextLabelName` | Same |
| Priority / progress | Priority 1-9; exact 0/25/50/75/100 percent | Same; no rounding of other percentages |
| Other markers, task schedules/costs/resources | Warn | Require consent; omitted |
| Images, binary attachments, external resources | Warn, original source preserved; never extracted or fetched | Not packaged; require consent |
| Floating topics, relationships, boundaries, background objects, conditional rules, formulas, advanced layout | Warn before conversion | Require consent; omitted |
| Internal object IDs/generation flags, revision history, camera/selection, computed statistics | Safely ignored; not treated as lost editable content | Optional IDs omitted; no dangling ID references emitted |

Theme definitions, including resolved theme colors, are **not** reconstructed.
Explicit supported topic styles are retained; unspecified styles use
MindManager defaults and may look different from Thinoda. Export's warning
covers this difference even for a workbook using a built-in Thinoda theme.
Font availability in the receiving application can affect glyph rendering;
UTF-8 preservation does not guarantee that every emoji has a glyph.

## CLI and library

```text
thinoda --convert input.mmap output.thinoda
thinoda --convert input.xmmap output.thinoda --allow-lossy
thinoda --convert input.thinoda output.mmap --allow-lossy
```

`--allow-lossy` explicitly accepts omitted source/destination content. Without
it, warnings block conversion before any output is replaced. For multi-sheet
CLI input, the workbook's active sheet is exported only with that consent.
Unsafe/malformed documents are rejected regardless of consent.

Custom per-symbol colors are not mapped to MindManager markers. CLI exports of
these workbooks additionally require `--allow-symbol-color-loss` (independent
of `--allow-lossy`); either flag alone must not bypass the other guard.
The UI likewise retains the symbol-color disclosure before MindManager's
sheet/conversion disclosure. Launch-file loading through the startup splash
uses the same import reports and defers lossy conversion to the consent dialog.

`import_bytes` / `import_file` are strict: a lossy MindManager conversion returns
`IoError::ImportNeedsConfirmation`. Interactive integrations should use
`import_bytes_with_report` / `import_file_with_report`, display the warning
categories and obtain consent before using the workbook. `export_bytes` is
strict about unrepresented model fields; `export_bytes_with_consent` is the
explicit conversion path. Callers must preserve source files themselves.

## Evidence and limits

Public vendor help establishes the distinction between `.mmap` archives,
standalone `.xmmap` and legacy `.mmp`:

- [MindManager: what is an MMAP file?](https://www.mindmanager.com/en/pages/mmap-file/)
- [MindManager 2012: save maps, templates and themes](https://onlinehelp.mindjet.com/help/MindManager/2012/ENU/save_maps_templates_and_themes.htm)
- [MindManager 2012: save options, archive/XML encoding](https://onlinehelp.mindjet.com/help/MindManager/2012/ENU/options_save.htm)

Those pages are not a complete public package specification. Two user-supplied
native references contained Application/Core/Primitive schemas. They were
inspected privately with permission, not uploaded, copied into source, or
redistributed. An original synthetic exporter candidate was independently
validated against those full schemas using a local, network-disabled XSD
resolver. A negative control with an invalid fixed-length object ID failed
validation. The exporter omits optional IDs instead of inventing an ID scheme.

The user confirmed that the original core candidate opens in MindManager.
Screenshots show ordered hierarchy, the seven shapes, font styling, labels,
priority/progress and the collapsed indicator. A separately named corrected
color probe showed opaque red/blue fills with legible white labels, supporting
the opaque color mapping. MindManager 26 displayed a nonfatal classic
branch-information-style notice. This is **narrow observed interoperability**,
not certification, proof of all map versions, or a fidelity guarantee.

The note body, link action, metadata UI and native-save/reimport reverse
roundtrip have not been independently verified in MindManager. The user could
provide screenshots but not native-resaved files. These remain explicit
verification limits, not reasons to label self-roundtrips as native validation.
The repository contains only original synthetic fixtures and a reproducible
candidate generator:

```text
cargo run -p thinoda-io --example mindmanager_candidate -- NEW-CANDIDATE.mmap
```

Committed tests cover structural output assertions independently of the import
adapter, deterministic bytes, supported-field roundtrips, multiple roots,
warning/consent behavior, CLI, localized UI, source preservation and corruption.
The private XSD check cannot be reproduced in public CI without a legitimately
redistributable schema or equivalent licensed verifier.

## Input bounds

MindManager input is limited to 64 MiB, XML to 32 MiB, 4,096 ZIP entries,
256 MiB declared aggregate size, 100,000 topics and 256 XML nesting levels.
The writer's topic-depth bound ensures its own output stays within the XML
reader's limit. Duplicate archive names (case-insensitive), traversal,
absolute/drive/backslash paths, symlinks, oversized entries, DTDs and malformed
XML are rejected. Only `Document.xml` is decompressed. Embedded resources are
not extracted; external entities, links and schema URLs are not followed.

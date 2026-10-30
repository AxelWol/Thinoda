# FIM authoring workspace: concept and delivery contract

Status: lossless authoring engine and native/browser application integration
implemented, with independent final critique and regression fixes. The delivery
ledger below distinguishes implemented workflows from remaining product scope.
This document is not a claim that full FIM support has shipped or that a
document has been certified.

## Outcome

Thinoda should be a trustworthy, enjoyable editor for FIM XDatenfelder schema
documents, using a tree-chart mindmap and a synchronized structure view.
Import is the entry point, not the product boundary. Users must be able to
understand, inspect, edit, reuse, validate, save, reopen and export a complete
schema without losing information or silently changing its meaning.

"Best editing experience" is a product objective, not a verifiable comparative
claim. Its acceptance criteria are concrete: preservation, correct operations,
understandable diagnostics, discoverability, accessibility, responsiveness and
interoperability.

## Research discipline

Authority order:

1. Version-specific XDatenfelder specification and exchange-message XSDs.
2. Official transformation specification and code lists.
3. FITKO documentation and quality-check catalogue.
4. Live FIM-Portal / schema-repository contracts and public example documents.
5. Local KERN documentation as experience and inspiration, not normative rules.

A generated XSD for **citizen submission data** is not the XSD for an **XDF
schema exchange message**. Passing one does not establish conformance to the
other. REST API normalized names are not automatically XML element names.
Search-engine summaries and validator vendor conformance percentages need
verification; neither is acceptance evidence.

Primary discovery sources:

- <https://docs.fitko.de/fim/>
- <https://www.xrepository.de/>
- <https://fimportal.de/openapi.json>
- <https://fimportal.de/docs>
- <https://docs.fitko.de/fim/docs/schema-repository/schemarepository-for-group-4/>
- <https://docs.fitko.de/fim/docs/datenfelder/QS_Bericht/>
- <https://schema.fim.fitko.net/docs>

The user-authorized KERN documentation directories are read-only references:
`C:\git\kern\kern-editor\docs\fim-import` and
`C:\git\kern\kern-editor\docs\fim`. Thinoda implementation remains original;
third-party schemas/examples require provenance and redistribution review.

### Confirmed starting facts

- XDF2 and XDF3 are distinct dialects, not interchangeable namespace labels.
- Containment cardinality is recorded on `struktur/anzahl`, not globally on a
  field or group definition. Unbounded upper bounds must remain unbounded.
- Schema (S), group (G), field (F), and rule (R) have different semantics.
  The user confirmed **C means Codelisten; support choice groups separately**.
  C is therefore an editor object type for code lists, not an assertion that
  the exchange grammar defines a `C<digits>` identity scheme. Inline values,
  external lists and group selection semantics remain distinct.
- Rule source is document content, never trusted executable application code.
- The supplied S00000000367 v1.0.0 example has 28 containment edges, seven group
  occurrences, 21 field occurrences and one rule. Address group
  G60000000086 v2.1.0 is present in three contexts. It includes legal references,
  rich descriptive text, code-list metadata, inline choices, field constraints
  and both optional and unbounded multiplicities.
- The attachment is 61,360 bytes, SHA-256
  `58dc8af3c28dee1b6d943f3bbd5f2f48637571669b3c0bcf09f85985602dde43`.
  Its 29 F/G/R occurrences resolve to 17 distinct type/ID/version keys.
  Namespace-aware inspection finds 752 qualified and 129 unqualified elements;
  qualified inline-value codes and unqualified metadata codes coexist. Codes
  `001` and `002` must not become numbers.

### Research gates still requiring evidence

- Exact schema/archive hashes and dependency closure are established below.
  Official bundles will not be redistributed: the user selected an explicitly
  consented first-use download with subsequent offline validation. This avoids
  relying on an unverified upstream redistribution grant.
- Exact code-list representations, selection-group semantics, rule
  scope/parameters/targets, derivation and representation restrictions.
- Transformation PDF: complete mapping, defaults, unsupported features and
  semantic changes. Regex dialects must be verified from primary text.
- Public fixture provenance and complete expected counts.
The initial validator feasibility gate now has native/WebAssembly evidence;
it is not a claim of exhaustive XSD-engine conformance or finished semantic
validation. See the implementation evidence below.

Unknown versions must not be accepted by a permissive namespace regex and
presented as supported. Unknown extensions may be preserved without being
declared valid under a schema that does not allow them.

## Product model and information architecture

### Three synchronized perspectives

1. **Map:** imported documents use the existing Tree Chart (right), German
   "Baumdiagramm (rechts)". It shows containment, not invented rule containment.
   Type, required/optional/repeating state and validation indicators are visible
   without corrupting editable titles. Shared-use indicators distinguish
   reuse from accidental duplicates.
2. **Structure:** keyboard-first outline/table over the same occurrences,
   with name, type, ID/version, cardinality and validation state. Filtering by
   type, requiredness, reused definition or problems does not alter the model.
   Rules and code lists remain discoverable as associated objects, with
   navigation to owners/usages rather than fake required data children.
3. **FIM inspector:** fourth right-hand tab after Style, Data, Map. Always
   discoverable; an ordinary map explains how FIM authoring starts instead of
   pretending it is exportable. Selection in any perspective identifies the
   same occurrence, definition and owner.

### FIM inspector

Progressive disclosure, search across properties, descriptive labels and
technical-name hints should provide access to **all** stored FIM content.
Common tasks use typed controls, not a wall of XML or JSON.

| Section | Contents |
|---|---|
| Overview | Dialect/profile, element type, identity/version, lifecycle, validation |
| Usage | Parent/path, cardinality, contextual references and usage-specific data |
| Definition | Names, labels, descriptions, definitions, help and keywords |
| Field | Field kind, data type, constraints, default/prefill semantics |
| Values / C | Inline values, codes/labels, external list identity/version/binding |
| Rules / R | Source, type, parameters, targets, errors, associations and dependencies |
| Governance | Publication/status, authorship, legal references, relations, derivation |
| Advanced | Header/envelope, namespaces, attributes and otherwise-unmapped content |

Empty, absent, inherited, invalid and unknown values are distinct states.
Repeated properties are ordered collections, not a single flattened string.
Links remain inert until explicitly opened. External code-list resolution is
an explicit action, with source/version/cache/failure state; opening a map
must not fetch arbitrary URLs.

Edit definition vs edit usage is explicit. Editing a shared definition previews
all affected usages. "Create independent variant" is a separate action with an
identity/version workflow, not an accidental side effect of typing.

### Interaction contract: extend Thinoda, do not redesign it

This is an Operate surface for schema authors and reviewers, including people
who understand administrative forms but do not know XML. It inherits Thinoda's
existing egui controls, fonts, blue selection accent, semantic theme tokens,
standard dialogs and keyboard patterns. No new visual identity, display font,
decorative animation, parallel widget library or unrelated settings redesign.

The defining interaction is **inspect a usage, understand its shared definition,
preview a change's impact, commit safely**. The first imported viewport should
make these distinctions apparent without requiring an introductory wizard.

```text
 Existing ribbon / document tabs
 +---------------------------------------+-----------------------------+
 | Map / Structure                       | Style  Data  Map  FIM       |
 |                                       |                             |
 | S  Schema                             | G  Anschrift Inland         |
 | +-- G Company                         | G60000000086  v2.1.0        |
 | |   +-- G Address [1:1] [3 uses]       | Usage / Definition          |
 | |       +-- F Street [1:1]            |                             |
 | |       +-- F House number [0:1]      | Used in: Company > Address  |
 | +-- G Person                          | Occurs: [1] to [1]          |
 |     +-- G Address [1:1] [3 uses]       | Shared definition: 3 uses   |
 |                                       | [Show usages]               |
 | Associated objects: Rules / Code lists| Properties, grouped/search  |
 +---------------------------------------+-----------------------------+
 | Problems: 2 errors / 3 warnings [Open] | Draft / validation revision |
 +---------------------------------------+-----------------------------+
```

This is a conceptual arrangement, not a pixel specification or a claim that the
displayed example has errors. Counts and validation states come only from the
actual document. Rules and code-list entries may be opened as associated-object
rows, but are never counted as data-instance repetitions.

**Import:** Open/Import/drop/CLI all use the same namespace-aware detector.
Show dialect, schema identity, object/usage counts and diagnostic summary.
Valid input opens directly; invalid or unsupported input preserves source in a
clearly labeled repair/inspection state. "Unknown" is not "valid".

**Change shared Street:** selecting any occurrence opens its usage. Switching
to Definition shows the three affected addresses before Apply. Name/help/
constraint edits update the intended definition occurrences in one transaction;
contextual cardinalities and legal references remain independent. Apply errors
stay beside the field with the draft intact.

**Change repetition:** two labeled controls, minimum and maximum, with a distinct
unbounded option. Show the unchanged source notation alongside the editor.
`0:1`, `1:1`, `0:*` and `1:*` shortcuts are conveniences, not the only permitted
ranges. Cardinality describes future submitted values, not the number of schema
definition rows a user is allowed to create or remove.

**Add/reuse:** Tab/Enter retain their navigational familiarity but use a compact
typed insertion chooser when metadata/identity is required. Remember the last
valid type within the document, not a global preference that surprises other
maps. Invalid parent/type combinations are absent or disabled with an
accessible reason. Escape leaves no half-created topic or empty undo step.

**Delete:** an inline impact preview identifies the exact usages/associations
removed. A definition with remaining usages is retained. Rule text mentioning a
removed ID produces a review requirement even when no machine-readable target
is present; the editor never claims free-text dependency analysis is complete.

**Review/export:** Problems reveals its target and field. Export shows the
selected dialect/profile, validation revision and any conversion losses.
Failed export leaves the destination untouched and focuses the actionable
problem. Warnings are not hidden behind a transient toast.

### Density, keyboard and screen-reader contract

- At ordinary desktop widths the inspector is a stable side panel; at narrow
  widths it uses the existing panel behavior rather than shrinking labels into
  unreadable abbreviations. Long IDs and paths are selectable and expandable.
- Structure columns have a stable tree/name column. Secondary columns can
  scroll or collapse into row details; hierarchy/selection stay visible.
- Four inspector tabs must fit the current minimum supported viewport and
  translated captions, with no overlap or offscreen keyboard focus.
- Navigation and editing have distinct focus states. Arrow keys navigate;
  Enter/F2 edits; Escape cancels the draft before affecting map selection.
  Keyboard alternatives exist for every DnD, reorder and code-list-row action.
- Type letters are accompanied by accessible names. Requiredness, reuse,
  warnings and errors use text/icons in addition to color.
- Focus follows inserted/restored objects and revealed diagnostics. Applying a
  change does not reset scroll, collapse the inspector or jump to the root.
- Multiline text and large rules open the existing resizable nonmodal editor,
  with source ownership and explicit Apply/Cancel. Advanced source is an
  escape hatch with preview/validation, never the main editing interface.
- Search distinguishes document search (names, IDs, metadata, rules/values)
  from filtering property labels in the inspector. A filtered-out selected
  occurrence has an explicit reveal action rather than disappearing silently.

### Creation and editing

- The schema root is a first-class authoring object with header and schema
  metadata, not merely a generic map title.
- Typed insertion for field, group, supported choice/container, rule and list
  association; create new or reuse existing.
- Valid defaults are version/profile-specific. Do not invent official allocated
  identifiers. Local authoring identities must be visibly distinguished from
  registry identities, with publication blocked until required identity fields
  are resolved.
- Reorder/move changes containment only; cardinality travels with its usage.
  DnD previews the valid destination and explains rejections.
- Duplicate offers reuse vs independent variant; paste/cross-sheet copy makes
  reference closure and identity collisions explicit.
- Delete previews usages, rule/list dependencies and collateral changes.
  Referenced definitions are not silently removed; free-text mentions cannot
  be automatically interpreted as a complete dependency graph.
- Multi-selection edits show mixed values and apply atomically.
- All mutations, including keyboard shortcuts, context menus, batch replace,
  import/merge, paste, AI and direct command execution, share domain policy.
- Cosmetic styling, collapse and camera changes cannot mutate XDF content.
- FIM annotations and ordinary map notes are separate namespaces of meaning.
  Editing a note must not silently overwrite a schema description or rule.
- Editing a published import marks the working document as a local modification;
  the UI cannot imply the changed document still has the authority of the
  imported release. Source lifecycle metadata is retained as provenance.
  Adoption of a new identity/version/status is an explicit authoring operation,
  not an automatic claim of official approval or registry allocation.
- Every document mutation is one undo step. Failed mutations retain the
  document, selection, drafts and undo/redo history.

## Integrity architecture

### Canonical document, definitions, usages and projections

Do not make generic topic labels/notes the sole FIM data model. Use a
versioned FIM document associated with a sheet:

- Immutable imported source bytes and explicit encoding/provenance.
- Lossless ordered XML representation retaining namespaces, qualified names,
  attributes, text/CDATA, comments, processing instructions and extensions.
- Typed version-specific semantic indices over the XML.
- Definition keys including type, ID and version; occurrence keys independent
  of topic IDs and stable across reorders.
- Ordered containment edges owning cardinalities and contextual metadata.
- Rules, code lists and associations represented according to their real XML
  grammar, not forced into the generic topic tree.
- A projection mapping FIM occurrences to map/outline nodes. Display labels and
  badges are derived without polluting original names.

Preserve repeated copies exactly on import, even when equal IDs/versions have
conflicting content. Report conflicts; do not merge-and-discard evidence.
Shared-definition editing is available only after identity and equivalence
are established, or after an explicit reconciliation transaction.

An untouched export should return original bytes. An edited export preserves
the XML information set and all untouched content; it must not claim byte
identity after normalization. Prefixes and whitespace in mixed content or
script bodies are not expendable formatting.

### Transactions and validation

`Document::edit_sheet` and `edit_book` are the shared mutation boundaries.
FIM semantic reconciliation/validation belongs there, with rollback on
failure, not solely in inspector handlers or export.

Validation layers must be named separately:

1. Safe XML decoding/well-formedness and supported namespace/message.
2. Version-specific official exchange-XSD conformance.
3. FIM semantic/reference/identity/cardinality integrity.
4. Methodological quality/derivation constraints, with catalogue provenance.
5. Optional project policy and external checks.

"Valid" never means merely "XML parsed" or "we recognized the root".
Generated submission-schema validation is an optional additional tool,
not a replacement for layer 2.

Schema imports resolve only through a pinned local allowlist. Document-supplied
`schemaLocation`, DTDs, external entities, XInclude and rule scripts cannot
access files, the network or execution. Bound bytes, events, depth, text,
attributes, occurrences, reference expansion and validation work. Cycles
cannot recurse indefinitely.

**Schema installation decision (user confirmed).** On first use, request
explicit consent to download the version-pinned official bundles. Explain the
publisher, exact artifacts, hashes, approximate download size and local storage.
Only schema/example archives are downloaded; document contents never leave the
device. Retain the verified archive bytes in local application storage, with
independent native and browser persistence adapters, and validate offline
afterward. Opening a document cannot silently consent. Cancellation, missing
cache, offline failure, proxy/CORS failure or changed upstream hashes produce
`Unavailable`, never a valid-looking fallback. A manual local installation
path remains available for air-gapped/proxy-restricted environments. Cache
refresh/removal is explicit, and compiled results are invalidated by bundle
and policy identity changes.

Imported invalid documents need a **repair workspace** that retains their
source and identifies problems. They must not be exported under a valid-XDF
label. During repairs, allow atomic changes that do not introduce structural
corruption; distinguish "draft has unresolved errors" from an operation that
would violate an established invariant. A validated document cannot silently
be downgraded by ordinary editing.

An invalid document's original bytes can still be saved/downloaded explicitly
as **original source**, without a validation claim. Native draft save/recovery
must remain possible while unresolved authoring problems exist; otherwise
validation would become a data-loss hazard. Validation status must distinguish
not checked, checking, passed, failed, unavailable and stale.

Heavy validation runs off the native UI thread and cooperatively on web.
Results are revision-tagged; stale results cannot approve a newer document.
Cheap deterministic mutation invariants run synchronously. Final export
validates the exact serialized bytes for that revision.

### Diagnostics UX

Persistent, navigable Problems view: severity, source/code, readable explanation,
element/path/property and actionable remedy. Clicking reveals the occurrence
and inspector field. Preserve FITKO quality severities separately from parser
or XSD error severity. Do not call all checks "certification".

A rejected action explains what would break and provides safe alternatives.
Do not silently coerce multiplicities, replace missing metadata with fabricated
values, clear invalid fields or discard failed drafts.

### Architecture decisions after the first rubber-duck critique

**Canonical storage and scope.** One FIM exchange document belongs to exactly
one `Sheet`, in a versioned `Sheet.fim: Option<FimDocument>` envelope. It is
included in `Sheet` equality, snapshots, native serialization and recovery.
Different sheets contain independent documents, even when their registry IDs
match. They do not share live mutable definitions. Copy/merge between sheets
imports a closed document or explicitly reconciled subtree; it never creates
an invisible cross-sheet reference. Multiple visual perspectives of a document
use its existing sheet, not separate sheets.

The canonical authored content is the lossless XML tree **inside** that
envelope. Semantic indices and topic projections are derived. An ordered arena
of XML records retains opaque local record IDs and source spans; a registry
identity index maps a type/ID/version tuple to **a list** of records. It is not
a map that overwrites conflicting definitions. Missing identity remains missing,
with an opaque local record key for inspection/repair. Whether an anonymous
record is schema-valid is decided by the actual message XSD, never by assuming
that nested XML makes identification optional.

`FimDocument` is message-polymorphic, not schema-rooted: its envelope records
the exact dialect and message kind, and its payload index distinguishes schema,
standalone group, standalone field, document profile, code list, translation
and collection messages. A standalone field is projected as a field root;
it never acquires an invented S definition or mandatory containment edge.
Unsupported message workflows remain source-inspectable without being labeled
editable/exportable. The registry index has kind-specific natural keys:
F/G/S/R (and XDF2's identified list records) use element identity, while
Genericode uses its complete canonical identity/version/version-URI tuple.
Missing optional components and contradictory identities are retained; they
are not normalized into fabricated IDs.

The original bytes and immutable baseline use cheap shared ownership
(`Arc<[u8]>` and an `Arc`-owned immutable XML snapshot). Authored XML uses
copy-on-write snapshot ownership; derived indices are shared/read-only until
recomputed. Cosmetic edits therefore do not copy source/arena/index payloads.
The first semantic edit may clone an arena; measure that bounded cost before
considering finer-grained persistent nodes. Serde adapters serialize the actual
content, not process-local pointers. Persisted sizes and history memory need
separate budgets and regression measurements.

**Write direction.** FIM-aware user commands mutate the canonical FIM tree,
validate cheap invariants, rebuild affected semantic indices/projections and
commit the entire `Sheet` transaction. No generic topic-diff-to-XML inference.
An atomic transaction includes source records, index changes and projection
updates. Semantic indices and rendered labels are rebuildable caches and are
not authoritative persisted data. In contrast, the topic-to-occurrence binding
is authoritative native presentation state: it records stable TopicIds and
their association with opaque source-record IDs across edits, so styles,
selection history and annotations survive reorder/reopen. Persist and validate
that binding separately; reconstructing it is an explicit repair, never a
quiet allocation of new topic IDs on every parse.

One shared dependency-closure service operates over containment, structured
rule parameters/targets, relations and code-list bindings. Delete, copy/paste,
merge, extraction and associated-object edits all consume its typed results:
internal resolved dependencies, explicit external references, ambiguous
identities and unresolved references. It does not recursively fetch external
objects or pretend every external relation must be embedded. Free-text ID
mentions are review hints, not proven dependencies. Each command declares
which result categories it can preserve, reconcile or must block.

Map and Structure rename commands route to the same typed definition-name
command as the inspector. A shared definition presents its affected usages
before the first mutation; the choice of shared edit vs independent variant is
explicit. A provisional text draft cannot update a random occurrence's label
and then reverse-project it later. Ordinary notes/style never write definition
properties. Guardrails at `edit_sheet` and `edit_book` reject a projection that
does not match its FIM content, including unconverted generic/AI mutations.

**Two kinds of change.** Native document dirtiness continues to cover FIM and
map annotations. Separately, a FIM content identity covers the canonical XML
tree/envelope only. It excludes topic cosmetics, collapse, selection, layout,
camera and derived caches. Source passthrough is allowed only when canonical
content equals the imported baseline; do not use `Document::is_dirty()` or a
monotonic revision alone. Undo back to the baseline restores byte passthrough.
Persist the baseline and original bytes together; reopening a native map after
style changes must still preserve the original XDF bytes.

Validation results are ephemeral and keyed to the exact FIM content identity,
schema-bundle hash and validation-policy version. A job that finishes after
undo/redo may be reused only for that same identity; an increasing native
state ID alone is insufficient. Pending/error/unavailable results cannot
authorize export. Cache absence after reopen means "not checked", not "valid".

**Projection cardinality.** One containment edge yields one visible schema
occurrence, regardless of `min:max`. `0:*` is one node with a repetition badge,
never zero nodes, infinitely many nodes or cloned example instances. Inner
edges of an equal reused group are shared definition content, while the
incoming edge's cardinality is usage-specific. Rules, inline value sets and
external lists have associated-object rows, not fabricated data containment.

**Redaction.** The initial masked-copy behavior for a FIM sheet produces a
clearly non-FIM projection. It drops the entire FIM envelope, original bytes,
provenance, indices and hidden metadata before the output enters save/export.
It cannot take a source-passthrough path. A future valid-FIM redaction workflow
would be a separate feature with semantic revalidation.

### Validity and command state machine

There are two independent axes: an authoring state and a validation result.
Authoring state is `ImportedUnchanged`, `LocalDraft` or `RepairDraft`; validation
is `NotChecked`, `Checking`, `Passed`, `Failed`, `Unavailable` or `Stale`,
always scoped to a content identity. Source validity is never inherited merely
because a user imported a document or its status says "freigegeben".

- Import retains source and enters ImportedUnchanged; validation determines
  whether guided authoring or repair restrictions apply.
- A complete successful typed edit enters LocalDraft and invalidates the
  previous validation result. Cheap invariants run before commit.
- Metadata/rule text under composition stays in an owned field draft until
  Apply. Invalid field drafts are retained, not committed as fabricated values.
- A locally authored incomplete schema can be saved natively as LocalDraft.
  It cannot export as valid XDF until validation passes for the serialized bytes.
- Invalid imported content enters RepairDraft; repair commands retain all
  untouched offending source and must not introduce new hard corruption.
- Revalidating a repaired document does not erase its provenance or grant
  official lifecycle approval. It only changes the named validation result.

The integrity promise is structural and semantic consistency after every
**committed command**, not that every keystroke or incomplete authoring session
already satisfies all publication metadata requirements. Live validation
reports incomplete authoring explicitly. A complete valid-XDF export is a
separate, strict gate.

| Mutation | Guided valid document | Local/repair draft | Shared entry point |
|---|---|---|---|
| Select, pan, zoom, filter, collapse | Allow; no XDF mutation | Allow | Existing view commands |
| Style/theme/notes/layout | Allow as map annotations | Allow | Existing commands with FIM projection check |
| Name/help/metadata/constraints | Typed Apply; shared-impact preview | Typed Apply or explicit repair | FIM property command |
| Cardinality | Parse/check range; preserve edge ownership | Permit corrective valid range | FIM usage command |
| Insert child/sibling/parent | Typed chooser; no partial generic topic | Typed chooser with incomplete metadata visible | FIM creation command |
| Reorder | Allow valid edge order; preserve full edge | Same, preserving unedited source | FIM reorder command |
| Indent/outdent/drop | Validate new parent, cycle, derivation and shared impact | Same hard checks; report unresolved soft constraints | FIM move command |
| Delete/delete-only/cut | Impact preview; reject dangling structured dependencies | Explicit repair plan; no silent cascade | FIM removal command |
| Duplicate/paste | Explicit reuse vs independent variant; reference closure | Same; expose identity conflicts | FIM copy/import command |
| Rule/list edits | Typed association and identity checks; scripts inert | Preserve invalid source for repair | FIM associated-object command |
| Search/replace/batch/AI | Proposal with affected definitions; atomic Apply | Same; never generic Topic writes | FIM batch command |
| Sheet duplicate/merge | Independent complete FIM envelope | Same; retain diagnostic/provenance state | Workbook transaction |
| Branch-to-sheet | Closed FIM extraction workflow or explicit non-FIM projection | Same; no orphan references | FIM extraction command |
| Undo/redo | Restore complete canonical/projection snapshot | Same | Existing history |
| Raw advanced edit | Staged validated preview, explicit draft/repair transition | Preserve unaccepted source until Apply | FIM source command |
| XDF export | Strict exact-byte validation before write | Block until exact output passes | FIM export preflight |

Hard checks (tree integrity, cycle/reference closure, cardinality grammar,
stable object ownership, source/projection coherence and processing limits)
always apply. Schema completeness and methodological review remain visible
draft diagnostics where authoring requires intermediate incomplete states.
No generic action is permitted to bypass a required FIM chooser/impact preview
by invoking the command directly; the domain layer returns a structured
"decision required" result.

### Message coverage and standards details

The XDF3 specification distinguishes exchange messages 0101 (document profile),
0102 (schema), 0103 (group), 0104 (field), 0105 (Genericode list), 0106
(translation), and catalogue/bulk messages 0201--0203. The schema-mindmap path
starts with 0102, but "full F/G/C/R support" also requires first-class library
objects and explicit handling of standalone messages. Phase 0 records each
message's support and scope; unsupported wrappers cannot be misdetected as a
schema and partially imported.

Code lists use Genericode identity (`canonicalIdentification`, `version`,
`canonicalVersionUri`) and bindings (`codeKey`, `nameKey`, `helpKey`), distinct
from F/G/S/R identity. Inline `werte` is a separate ordered value set. Full
Genericode XML and column metadata survive even when a particular column
cannot be presented in a convenient typed table.

The researched XDF3 group-kind list identifies `X` as Auswahlgruppe.
The exact runtime selection policy is not inferred as "exactly one" without
the methodological specification. Derivation settings have separate structure
(`0` immutable, `1` restriction only, `2` extension only, `3` modifiable) and
representation (`0` immutable, `1` modifiable) code sets. These govern derivation,
not whether the imported author's source may ever be edited locally. An
explicit derive-from workflow requires an immutable parent baseline and a
property-level permitted-change check; do not turn these fields into blanket
locks on all native map annotations.

XDF3 specification II.3.1.13 (p. 31) explicitly defines `praezisierung/@pattern`
by the HTML input `pattern` attribute, not the exchange XSD's own regex dialect.
Preserve the authored expression and its version context; browser regex
evolution must not silently reinterpret an old document during conversion.

Official XRepository also publishes **2023-09-04 file-based transformation
rules in both directions**. These supersede the earlier research assumption
that only a 2022 draft was available. Both current documents were acquired and
read; their own goal is a "möglichst äquivalente" result, not a lossless inverse.
They describe XSLT implementations but are not evidence that Thinoda has
implemented those mappings or that arbitrary rule scripts are equivalent.

Concrete conversion hazards from the 2023 documents:

- XDF2 -> XDF3 adds an ID sub-number range, a micro-version component and new
  lifecycle/default metadata; legal references may be split. Empty groups and
  schemas are legal in XDF2 but XDF3 requires at least one structure.
  The published migration workaround inserts a BOB date field; Thinoda must
  **not silently insert unrelated business data**. Show the empty-container
  incompatibility and require an explicit meaningful authoring decision.
- XDF3 -> XDF2 removes the sub-number range and maps nonzero micro versions to
  an expanded minor component. Check collisions before accepting any mapping;
  the 2023 assumption that sub-number ranges are unused is not a current fact.
- `locked`/`hidden` -> `label`, `text_latin`/`time`/`datetime` -> `text`,
  selection-group kind, prefill, attachment constraints, relations, keywords,
  rule type/parameters/targets/errors and changed lifecycle values can lose
  semantics. These require property-level blocking/loss entries.
- XDF2 code-list export may require **separate Genericode files**, not just one
  XML download. Unavailable external list content blocks a complete package;
  never select "latest" over the network without an explicit resolution step.
  The published C-prefixed migration naming convention is not authority to
  allocate registry identifiers or overwrite conflicting lists.
- Upcast conversion of internal lists to inline values assumes the first three
  columns mean code/name/help and drops further columns. Thinoda must request
  explicit column mapping and retain/report all unmappable data instead.
- XDF2 `script` and XDF3 `skript` are not interchangeable. The downcast document
  proposes a default empty script; that is a semantic loss, not an equivalent
  executable rule. Neither direction silently executes or translates source.

Comments containing discarded original values can preserve evidence but do
not preserve executable semantics. Conversion previews distinguish retained
native provenance from representable target content and validated equivalence.

### Persistence and other formats

- New native archive and whole-sheet clipboard versions must reject older
  readers rather than permit silent FIM loss.
- Native save, autosave, recovery, undo/redo, sheet duplicate/merge and full
  clipboard retain the complete FIM model and source provenance.
- Subtree copy/paste retains required definitions and dependencies or refuses
  with an explanation. Plain-text clipboard is explicitly not lossless FIM.
- XMind/private metadata must be audited before claiming preservation.
  Editable exports that cannot retain FIM require an explicit loss warning;
  presentation/image exports are clearly projections, not FIM backups.
- Masked copies must not retain original FIM source or hidden sensitive metadata.
  Either produce a clearly non-FIM projection or a separately validated,
  fully redacted document. Do not label visible-only redaction safe.

## XDF export and conversion contract

Native save is the default working format; import never makes the original XDF
a writable save target. Export selects the active FIM document explicitly.

- Same-dialect export: exact source if untouched, validated serialization if
  changed. Ordinary maps cannot masquerade as FIM schemas.
- Cross-dialect export: a separate conversion transaction/preview with a
  property-level mapping and loss report. No namespace substitution shortcut.
- Unsupported semantics are blocking by default. Consent to conversion loss
  is explicit and never authorizes malformed or falsely certified output.
- Keep original dialect/source and conversion provenance. Export does not
  mutate the working document unless the user chooses to adopt the conversion.
- Preserve envelope/header and update only fields that the specification
  requires for a newly authored message, with explicit policy and provenance.
- Never infer rule equivalence or translate executable source silently.
- Import/export summaries distinguish fields, group usages, distinct
  definitions, rules, inline value sets and external code-list bindings.
  A count of all XML nodes is not a useful substitute.
- Multi-file XDF2/list output is prepared and validated as one package, with
  deterministic safe filenames, collision checks and a manifest. ZIP export
  provides an atomic single-file destination; extracted-directory export uses
  staging and explicit failure recovery, not partially written success.

## Completeness additions beyond the initial request

The initial request needs these coupled capabilities to be genuinely complete:
definition reuse and conflicts; occurrence-scoped metadata; code-list identity
and bindings; dependency-aware rule management; invalid-input repair;
derivation constraints; lifecycle/version workflows; loss reports; source
provenance; safe native/recovery/clipboard storage; accessible diagnostics;
bounded offline processing; stable undo; and interoperability evidence.

Optional later integrations are explicitly not hidden prerequisites: portal
search/download, authorized publication, remote quality services, generated
submission forms/schemas, and assistive rule translation. Network use and
publication need separate user intent. AI suggestions are never a source of
normative validation or automatic rule execution.

## Delivery plan and measurable gates

| Phase | Deliverable | Gate |
|---|---|---|
| 0 | Evidence ledger, profile/message matrix, schema provenance, fixture inventory | Resolve the research gates; record unsupported versions explicitly |
| 1 | Original concept, interaction contracts, independent rubber-duck critique | Resolve architectural blockers before schema/model commitment |
| 1.5 | Canonical store, sheet scope, change identity, command/state contracts | B1--B6/H1--H4 plus R1--R5: message polymorphism, natural keys, shared dependency closure, persistent bindings and cheap snapshots pinned down before model commitment |
| 2 | Validator feasibility spike and lossless XML/semantic model | Official exchange schemas compile offline on native/web; independent oracle agrees on positive and negative fixtures |
| 3 | XDF2/3 imports, native persistence, recovery, Tree Chart projection | Supplied sample and public corpus retain every property; original-byte no-op export; native reopen equivalence |
| 4 | Transaction policy and typed mutations | Add/change/remove/move/reuse/duplicate/paste/rules undo atomically; rejected changes preserve state and drafts |
| 5 | FIM tab and synchronized structure/map workflow | All data discoverable; typed common edits; complete advanced access; keyboard, themes, locales, narrow/scaled windows |
| 6 | Same-dialect validated export and conversion preview | Exact output passes official exchange XSD plus semantic gates; property-by-property round-trip/conversion matrix |
| 7 | Hardening, performance, documentation, final independent critique | Fix findings; malformed/cycle/size corpus, native/web checks, human workflow acceptance |

Additional user-directed phase 5 acceptance: while the right-hand FIM tab is
open, selecting a map node reveals its corresponding FIM entry with a rectangle
in the configured accent color. The rectangle must work in light and dark
themes, identify the exact occurrence rather than every reused identity, and
leave canonical content and pending form drafts unchanged.

No phase can be marked complete because its API exists. Its gate needs retained
evidence. Do not ship a read-only XML text box as "full FIM editing", a blanket
ban on structural changes as "integrity", or hand-picked tag checks as "XSD
validation".

### Acceptance corpus

- Both dialects: minimal, real-world, deep/wide, reused definitions, conflicting
  definitions, optional/mandatory/repeating/unbounded occurrences.
- All requested node surfaces, rule types/associations, inline and external lists,
  constraints, lifecycle, legal references, relations, Unicode and multiline text.
- Prefix/default-namespace variants, unqualified code elements, extensions,
  CDATA/comments/PI, whitespace-sensitive and entity-escaped content.
- Invalid cardinality, dangling/cyclic references, bad IDs/versions, duplicate
  identity, unsupported message/version, invalid mandatory metadata and XSD order.
- Malformed XML, DTD/entity attempts, oversized/deep input, invalid encoding,
  large repeated-reference expansion, resource-budget exhaustion.
- Every operation through map, outline, inspector, keyboard, clipboard, batch
  replace and AI, plus undo/redo/save/recovery/reopen.
- Export failure before destination write; external source never overwritten.
- Light/dark, all ten locales, narrow viewports, keyboard-only navigation,
  focus/draft ownership, screen-reader labels and non-color state cues.

## Critique ledger

Independent concept critique completed against the first draft. Verdict:
**REVISE before model implementation**. It identified six blocking architecture
gaps: canonical/projection write direction, cosmetic-vs-FIM dirtiness, conflicting
identity storage, local/missing identity handling, cross-sheet scope, and the
validity/command state machine. High findings covered generic command bypass,
projection mappings, masked-source leakage and cardinality rendering.

The architecture decisions, state machine and mutation matrix above address
these findings. The review's suggestion that anonymous inline definitions are
necessarily legal is **not accepted as a normative fact**: retain and inspect
missing IDs without fabrication, but decide validity from the official XSD.
The C terminology and cardinality UI sections were already revised while that
review was running; those stale-copy findings do not reopen user decisions.

Additional required regressions:

- Shared definition rename through Map/Structure/inspector yields identical
  canonical state and one undo step; all three reused labels update.
- Cosmetic-only changes plus native reopen preserve source-byte export.
- Undo to imported content restores byte passthrough.
- Conflicting equal registry identities survive import/native save/original
  source output with both source locations preserved.
- A missing ID is inspectable/repairable but never granted fabricated validity.
- Sheet duplication makes independent documents; edits cannot cross sheets.
- Masked copies contain no FIM original-source bytes, including in archives.
- A stale validator result cannot approve a changed document or write a file.
- Each Action/command route obeys the matrix, including direct programmatic
  invocation, keyboard, batch replace and AI.

Independent final implementation critique is recorded in the authoring delivery
ledger below. The earlier validation/domain review remains historical evidence;
it was not used as a substitute for reviewing the integrated authoring feature.

The second independent review confirmed resolution of B1--B6, H3 and H4.
It requested five further readiness decisions (R1--R5), now specified above:
message-polymorphic envelopes, kind-specific identity, one dependency-closure
service, authoritative persisted presentation bindings distinct from caches,
and shared immutable/copy-on-write ownership under clone-based history.
These are architectural resolutions, not claims that their tests already pass.

## Authoring delivery ledger

The canonical store retains the original UTF-8 bytes and an ordered,
namespace-aware XML tree, with stable source-record/topic bindings. Untouched
documents export their original bytes; edits retain unknown metadata,
attributes, comments, processing instructions, CDATA and sibling order.
Native v5 persists source, current XML and bindings; whole-sheet clipboard v4
preserves the envelope. Older readers cannot silently discard it. Masked copies
explicitly remove the entire FIM envelope.

Implemented application workflows:

- Root-aware XDF2/XDF3 XML import, including conventional `.xdf2.xml` and
  `.xdf3.xml` files, opens Baumdiagramm and the fourth FIM sidebar.
- Occurrence-aware S/F/G/choice projection; identities, lexical cardinalities,
  shared-use badges and associated rules/code lists are available in Structure.
- Inspector access to all ordered source elements and attributes, with search,
  pagination, explicit insertion position, namespace-aware insertion/removal,
  retained Apply/Cancel drafts and one-occurrence conflict repair.
- Map selection reveals an accent-outlined FIM summary entry and outlines its
  corresponding document-metadata row. Both themes use the configured accent;
  associated-record browsing is not mistaken for selection of an unrelated
  containment node.
- Definition rename, shared reuse, duplicate, independent identity variants,
  cardinality edits and guarded structural operations. Referenced objects and
  unresolved/ambiguous/external dependencies have navigable source locations.
  Published identities are never fabricated.
- Pending title/FIM/legacy inspector edits participate in dirty state, save,
  separate-sheet output and recovery. Rejected edits remain available for
  correction. Further typing cannot bind an old draft to a new source revision.
- Native/schema installation is explicitly consented, pinned, bounded and
  cached for offline use. Browser Fetch rejects redirects before following,
  omits credentials and has streamed size/time bounds. Local ZIP installation
  is an alternative; documents are never uploaded.
- Same-dialect export validates the actual serialized bytes against the
  exchange XSD and semantic/projection checks before atomic output. Results
  are owner/revision-scoped; stale work cannot write. The imported source path
  is protected. Validation is not methodological certification.
- Generic structural edits are reconciled transactionally or rolled back.
  Unsupported subtree paste/extraction and foreign editable exports explicitly
  refuse silent FIM loss; image/presentation projections are not FIM backups.
- Opening and saving large models stays responsive on the desktop: launch files,
  later opens/imports and saves run on worker threads behind a busy card naming
  the file (multi-file opens count finished files; no per-file percentage).
  Saves write a committed snapshot after drafts were applied and mark the
  document saved only if it still holds exactly that content, so edits or
  drafts made during the write stay unsaved, including for Save All and
  close-tab saves. The browser has no worker: the card paints first, then the
  page is unresponsive until that file is done.

### Final rubber-duck critique and fixes

A fresh independent review traced domain transactions, shared-definition
semantics, persistence, command routing and the exact-byte export lifecycle.
Its initial paste concern was correctly downgraded after verifying transactional
rollback; nevertheless, a direct FIM-targeted subtree-paste guard was added so
foreign same-shape topics cannot be heuristically interpreted as shared reuse.
A matching-foreign-subtree regression verifies failure without mutation.

The review also identified cancelled-draft feedback and clean recovery-revision
bookkeeping gaps. Cancel now clears obsolete error/report feedback, and successful
clean-document recovery removal advances the FIM draft revision. Open/Import
filters now include XML, matching the conventional export filename.
Cancellation also advances the draft revision: a snapshotted, subsequently
cancelled value cannot survive as stale recovery content. A regression snapshots
a valid cardinality draft, cancels it, and verifies that the recovery copy is
removed.

Additional parent verification found and fixed an important rendering bug:
unbounded full-path collapsing headers expanded the sidebar beyond the window,
allowing the canvas to cover it. Compact source headers, wrapped paths/name
buttons and a width-constrained FIM scroll viewport fix the cause. GPU
regressions measure canvas/sidebar non-overlap and render the supplied document
at 1280 and 720 pixels in light/dark Map and Structure views. Merely finding an
accessible FIM tab did not establish that it was visibly usable.

Selecting a library rule/code list also now selects that source record rather
than displaying or mutating the previously selected containment topic. Structure
provides associated-object entry points without fabricating containment edges.
A narrow follow-up review independently identified the same source/topic
mismatch for dependency navigation. The inspector now derives metadata,
shared-use feedback and independent/structural command targets from the browsed
source record, not from an unrelated canvas selection.

The full workspace run exposed two application regressions beyond the focused
FIM tests: adding a guide page left the shortcut groups one entry short, and
command preflight committed a title merely when opening a modal or changing
views. Shortcut groups are now compile-time-sized to the page count. Preflight
validates a cloned draft without committing; operation-specific commits retain
their integrity checks. All failing application targets were rerun successfully.

The selection-highlight follow-up uses exact occurrence bindings, not identity
equality: only the selected reuse receives a rectangle. Its summary remains
available while a pending scalar draft retains its original target. The
rectangle uses the theme palette's live configured accent, with no hard-coded
light/dark colors. Revealing an entry scrolls only as needed rather than forcing
the FIM heading offscreen. Regressions cover actual map clicks, changing accents,
both themes, hidden/reopened tabs, reused definitions and pending drafts; GPU
renders exercise the supplied source at wide and narrow widths.

### Verification evidence

- The engine's 16 read-only corpus cases preserve exact original source through
  native save/reopen; root-name edits retain source closure and undo-to-original
  behavior. The corpus runner is `thinoda-io/examples/fim_roundtrip.rs`.
- The supplied 61,360-byte XDF3 document imports as 29 containment topics:
  schema root plus 28 occurrences. Rules and code lists are associated library
  data, not extra cardinality-bearing containment edges.
- A separately invoked reference regression installs the real pinned archives,
  reloads the cache offline, validates untouched and renamed source, checks native
  reopen and undo, writes conformant export, and verifies invalid output does
  not create a destination. It requires local `THINODA_FIM_REFERENCE_DIR` and
  `THINODA_FIM_SAMPLE` paths and is ignored in ordinary CI.
- Headless application regressions exercise fourth-tab access, invalid/corrected
  cardinality drafts, source revision ownership, title/native/separate-sheet
  save and associated-rule navigation. GPU tests can retain images with
  `THINODA_FIM_VISUAL_DIR`; no upstream samples are committed.
- The native desktop app was opened with the supplied document; Structure
  exposed F/G badges, lexical cardinalities and three shared address usages.
  The review map was saved as a native session artifact before closing for
  rebuild, leaving the original XDF untouched.
- Final application checks pass: 245 unit tests (one reference test normally
  ignored), 32 focused integration tests across eight targets, and all three
  additional application targets that failed in the earlier full run. Native
  workspace and shared-browser strict Clippy checks, formatting and third-party
  notice checks pass.
- The full workspace run is not entirely green:
  `thinoda-platform::tests::cloud_prefix_boundaries_case_and_nested_provider`
  fails at the nested new-file path lookup. It also fails in isolation; its
  test and cloud-path implementation are unchanged by this feature. This
  unrelated platform issue is recorded rather than hidden or changed here.
- A current Windows build is available under
  `target\x86_64-pc-windows-msvc\debug\thinoda.exe`. Final integration checks used
  this separate native target because a reopened review app held the default
  executable locked; the running review session was not interrupted.

### Remaining scope, not hidden success

This is an integrated loss-preserving authoring release, not completion of every
experience proposed in this concept and not evidence for a comparative
"best in the world" claim.

- Cross-dialect migration is explicitly unsupported; each dialect can be edited
  and exported in its own version. No namespace-swap conversion is offered.
- External list fetching and XDF2 multi-file package generation are not
  implemented. Existing references and list payloads remain intact.
- Source authoring supports incomplete repair drafts; creating a node does not
  invent official identity, lifecycle/publication metadata or XSD conformance.
  Many less-common metadata properties use the structured source browser rather
  than bespoke typed controls.
- Methodological/derivation-quality review is not exhaustive. Rule text remains
  inert; free-text mentions are review hints, not proven executable references.
- Subtree clipboard/extraction requires a future closed-document identity and
  dependency workflow; complete-sheet copy/duplicate is available.
- Input is UTF-8 only, with explicit processing limits. Namespace rebinding is
  rejected rather than silently reinterpreted.
- Browser schema compilation/validation is queued locally but is not yet a
  dedicated Web Worker. Native work is background-threaded; large-browser-input
  responsiveness still needs dedicated performance evidence.
- Invalid, unapplied scalar drafts remain in the editor for correction, and block
  saving/exporting stale source. Recovery projects applicable drafts, but does
  not yet restore rejected scalar form input separately from the canonical
  document after a crash.

## Evidence appendix

The exact public download route was verified from XRepository's own download
component: `/api/xrepository/{file-kennung}`. Downloads succeeded anonymously;
the SPA frontend is not evidence that the files require authentication.

| Artifact | Authoritative file identifier suffix | SHA-256 |
|---|---|---|
| XDF2 exchange XSD archive | `xdatenfelder_2.0:xmlschema:datei:XDatenfelder_2.0_XML-Schema.zip` | `6687900227b8b151bb1a4a6ce9b61c8e3f21c3dada130296efa74a985936645f` |
| XDF3 exchange XSD archive | `xdatenfelder_3.0.0:xmlschema:datei:XDatenfelder_3.0.0_XML-Schema.zip` | `3111627bfe4ee923a22e9ca1248b0478dfbfe11afa3cd4730db6a6e63b148a38` |

Identifiers above follow `urn:xoev-de:fim:standard:`. Source metadata:

- <https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_2.0/metadaten>
- <https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_3.0.0/metadaten>
- <https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_3.0.0:dokument:Transformationsregeln_XDatenfelder_2_zu_3:datei:20230904_Transformationsregeln_XDF2_-XDF3_auf_Dateibasis.pdf>
- <https://www.xrepository.de/api/xrepository/urn:xoev-de:fim:standard:xdatenfelder_3.0.0:dokument:Transformationsregeln_XDatenfelder_3_zu_2:datei:20230904_Transformationsregeln_XDF3_-_XDF2_auf_Dateibasis.pdf>

The XDF3 archive contains six XSDs: exchange messages, collection messages,
building blocks, data types, Latin characters and XÖV code. Its dependency
closure needs an important qualification: the strict Genericode wildcard
requires the OASIS schema, available in the official **examples** archive,
even though there is no direct XSD import in the exchange archive. XDF2
contains those six plus Genericode, search and synchronization schemas (nine
total). Genericode additionally imports `xml.xsd`, absent from both archives;
the validator dependency supplies the unmodified, licensed W3C schema.
The installed authoring validator loads the six exchange/common schemas,
Genericode and W3C XML, not XDF2's operational search/synchronization messages.
Do not combine both dialects into an accidentally permissive schema set.
Official metadata says XDF2 is certified
and XDF3 is **not certified**; application validation must not misrepresent
either certification state as a claim about a user's edited document.

Both metadata responses have `lizenz: null`, and neither archive contains a
LICENSE/NOTICE. A public Apache-2.0 mirror (`escalon/glykon`, revision
`784489c92cbbb05614a5ce5878edc7169cbeecba`) helped discover the grammar but
does not independently establish the upstream schemas' redistribution grant.
Keep official bundles in research artifacts until provenance/permission is
resolved; never silently relicense them under Thinoda's EUPL. The selected
first-use-download design avoids bundling those archives. The W3C documents
embedded by the validator dependency do have explicit redistribution terms;
their complete notice is included in Thinoda's generated user-visible notices.

The supplementary local reference review inventoried both authorized KERN
directories and reviewed the implementation/test/critique, AI/cross-page,
OpenAPI/FIT-Connect and builder-SDK material. Its only XSD is a generated or
per-schema **submission** contract, not the exchange grammar. Relevant lessons
are explicit source provenance, guarded re-import, deduplicated diagnostics,
stable identity, inert rule source and reviewable AI proposals. FIT-Connect,
runtime forms and proposed KERN SDKs are separate systems, not requirements
that should expand Thinoda's offline schema-authoring implementation.

## Initial implementation and retained evidence

Historical first-phase infrastructure (subsequently connected above):

- `thinoda-core::fim::Cardinality` validates exact decimal/unbounded range
  syntax, minimum <= maximum and serde input. Decimal strings avoid integer
  overflow/truncation and preserve leading zeros in the source notation.
  The explicit processing budget is 1024 bytes per range.
- `thinoda-io::fim` has exact dialect/message allowlists, bounded inspection,
  pinned archive-source descriptors and archive-hash verification, closed
  offline schema compilation, and exact-byte exchange-XSD reports.
- The Rust validator is pinned to `xsd-schema = 0.3.0` under its permissive
  license. Its `xsd11` build feature enables the full XSD regex engine;
  `SchemaSet::new()` retains **XSD 1.0 runtime semantics**.
- Reports carry input SHA-256, full compiled-bundle SHA-256, policy version,
  namespace-aware diagnostic paths, severity and explicit truncation counts.
  An exchange-conformant report is not a semantic/identity/lifecycle approval.
- Input is currently UTF-8 only, with an explicit unsupported-encoding error.
  Other encodings require a deliberate lossless decode/source-retention design
  before the importer can claim support; they are not silently misdecoded.
- Validation bounds: 32 MiB input, depth 256, one million element nodes,
  1024-byte names, 256 attributes/element, 200 retained diagnostics, 1 MiB
  schema archive and 512 KiB schema entry. DTDs are rejected. Instance
  schema-location hints never trigger loading.
- `cargo run -p thinoda-io --example fim_validate -- xdf3 SCHEMA_ZIP EXAMPLES_ZIP DOCUMENT_XML...`
  is a developer evidence runner, not the application importer. XDF2 uses
  `xdf2 SCHEMA_ZIP - DOCUMENT_XML...`. Nonconformant input produces a failing
  process exit and explicit JSON evidence.

Research artifacts retain original official archives, 2023 PDFs/text extracts,
an isolated Rust feasibility harness, an independent .NET XML Schema oracle,
mutation fixtures and JSON reports. No upstream schema/example payload or
user attachment is committed to this repository.

The production service agrees with the independent oracle on **37 cases**:
20 XDF2 and 17 XDF3; 14 are accepted and 23 rejected. Cases include official
schema/group/list/translation/collection examples, the supplied document,
prefix variants, missing fields, wrong order, invalid type codes/cardinality,
qualified-vs-unqualified codes, unknown attributes, multiple roots and DTDs.
An initial DTD policy difference was fixed by explicit host preflight.

Two important counterexamples remain part of the acceptance contract:

1. Both XSD engines accept a lexically correct but reversed `2:1` range.
   The separate domain range check rejects it. XSD alone cannot guarantee
   cardinality semantics.
2. The local KERN XDF2 reference `S05000633V1.4.xdf2.xml` is rejected by both
   independent engines against the official exchange schema. A filename or
   familiar reference directory is not validity evidence. Preserve it for
   repair; do not silently weaken the schema or insert missing business data.

The updated repository regression run passes 186 core/I/O library tests,
native all-targets core/I/O lint and the complete shared WebAssembly app lint
gate. New license-notice coverage also passes.
These results do not cover the planned Map/Structure/FIM UI or their end-to-end
workflows, which have not been implemented.

### Interim foundation critique and resolution

The independent rubber-duck reviewer found no blocking foundation defects,
verified the runtime XSD-version and validity/PSVI flag assumptions directly
against the dependency implementation, and confirmed the range-comparison,
archive bounds, schema closure and deterministic evidence-hash behavior.

The review identified one real inspection gap: CDATA outside the root was
ignored. It is now rejected, including empty/whitespace CDATA; XML permits only
ordinary whitespace/comments/PIs outside its root, not a CDATA section.
Adjacent cases are covered too: character references outside the root,
misplaced/duplicate XML declarations and the reserved case-insensitive `xml`
PI target. A positive BOM/declaration/comment/PI/inner-CDATA case prevents the
fix from rejecting legitimate document prologues.

The shared ZIP cap was reviewed against all three pinned artifacts
(19,679 / 16,552 / 32,140 bytes), and the code now documents that changing an
artifact pin requires rechecking its cap and closure. Increasing the cap
speculatively would not make an unrecognized archive trusted.

The schema-location regression was renamed to describe its actual assertion:
location hints do not change the result. The no-download property comes from
the architectural absence of an HTTP/file resolver in this API, not from a
test that merely uses an unreachable-looking URL. Network consent/cache
integration remains a separate, unimplemented acceptance gate.

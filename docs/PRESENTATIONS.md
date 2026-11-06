_Last updated: 2026-11-03 18:29:20 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Vector presentations

## Outcome and implementation contract

Replace screenshot-based branch and Zone slide authoring and PowerPoint handoff
with live vector scenes and editable PowerPoint objects. A slide is a view of
current map content, not a detached copy of its text. Source topics remain the
single source of truth. Board viewport slides are an explicit exception in
PowerPoint: their visual scenes are flattened to images.

The existing player already paints vectors; the editor preview and PowerPoint
export are the bitmap paths being replaced. An SVG picture inside PowerPoint is
not sufficient: topic text, topic geometry and connecting paths must be native
DrawingML objects. Original photographs remain raster image objects.

### Prioritized phases

1. **Shared scene contract and compatibility.** Persist source isolation,
   automatic versus manual framing, hidden subtrees and optional depth. Resolve
   visibility once and reuse it across editor, player and export backends.
   Existing files retain contextual crops and their saved frames. Newly created
   slides focus on their source. A branch includes its owned descendants; a Zone
   includes all its member roots and descendants.
2. **Editable handoff.** Replace slide-sized PNGs with native PowerPoint text,
   shapes and paths. Keep 16:9/4:3 aspect fitting, original order, theme and
   resource/export safeguards. Validate package semantics and Microsoft Open XML
   schemas; do not infer installed Office acceptance from XML parsing alone.
3. **Visual workspace.** Add a responsive slide strip and directly painted vector
   stage, focus/framing/visibility controls, and reversible topic hiding.
   Keep Add, Generate, reorder, duplicate, delete, title editing and reviewed AI
   suggestions discoverable. Preserve Apply/Cancel, IME/touch safeguards and a
   single undo transaction. The common resizable dialog remains modal because
   its transactional draft intentionally owns a frozen map revision.
4. **Live player and polish.** Refresh a running presentation after map changes,
   retain its active slide by stable ID where possible, and handle an empty or
   deleted source safely. Preserve presenter context, keyboard navigation,
   fullscreen, builds, reduced-motion default and transitions.
5. **Acceptance and critique.** Cover persistence/undo, visibility and framing,
   hidden-content leakage, dynamic edits, editable PowerPoint objects, resources,
   all ten locales and both appearances. Run a second independent rubber-duck
   critique, fix concrete findings, and update user/developer documentation.

Phases 2 and 3 may proceed independently after the scene contract is agreed.
No release or branch merge is implied by this implementation task.

### Dynamic-content rules

- Slide IDs and source topic IDs survive ordinary map edits and slide reordering.
  Renaming a topic changes its displayed content; a custom slide title remains
  independent. New descendants appear when permitted by scope, hidden-subtree
  and depth settings. Automatically framed slides recompute visible bounds.
- Manually framed slides retain their saved viewport. Branch and Zone visibility
  is semantic, not just spatial: an unrelated sibling inside that viewport must
  still be excluded. A View slide instead represents the chosen spatial area.
- Connectors and relationships require visible endpoints. Group annotations
  must not disclose hidden text or aggregate hidden members. Zones must not leak
  unrelated roots into branch-focused slides. A depth limit that removes part
  of a Zone also removes its enclosing frame rather than representing partial
  ownership as a complete group.
- Existing core deletion cleanup is authoritative: removing a source removes
  its dependent slides rather than weakening document validation or silently
  substituting the whole map. The player must reconcile the selected ID after
  cleanup and explain that current content has changed.
- Hidden IDs no longer present in the sheet must not block unrelated map edits.
  Hiding changes presentation metadata only, never deletes map topics.
- Generate explicitly replaces the draft deck. It is not a merge/regeneration
  operation; ordinary map edits do not require Generate.

### Deliberate boundaries

Keep the current explicit refusal of unsupported resources and unsupported note
formats. A bounded notes-equation appendix is included only for topics resolved
as visible on each slide, using native editable text and fraction-rule paths;
topic labels use font-measured, wrapped lines above their equations, with the
same bounded appendix geometry in SVG, PDF and PowerPoint. Malformed source is
rejected. Do not silently rasterize unsupported vector/text content. Viewport
slides are intentionally flattened in PowerPoint, with that loss reported by
export preflight.
Pixel-identical font rendering across Office installations cannot be
guaranteed; preserve Unicode, editable text and geometry with declared font
behavior.

This is not a general-purpose free-layout slide design application. Video,
persisted choreography, automatic narration, multi-monitor presenter routing,
timers and a new rich speaker-note format remain separate work. Existing
transitions are playback-session choices, not exported PowerPoint animations.

## Concept rubber-duck review

The pre-implementation review identified three important risks:

- **A crop is not isolation.** The current renderer draws the complete sheet.
  Resolution must filter topics and dependent decorations before any backend
  paints; fitting a smaller rectangle alone is incorrect.
- **Broken references cannot bypass validation.** Preserve the core's existing
  source-deletion cleanup and strong document invariants. Add explicit player
  reconciliation and stale-hidden-ID handling rather than a permissive loader.
- **Regeneration is not live update.** Preserve stable identities on ordinary
  edits and reordering; document Generate as replacement instead of promising
  a complex, ambiguous merge of duplicated slides by source.

The review also required separate branch/Zone scope semantics and treating
DrawingML as a real rendering backend with its own acceptance tests, not as a
different picture encoding. These decisions are incorporated above.

## Official product research

The [XMind Pitch Mode guide](https://xmind.com/user-guide/pitch-mode-new)
documents map-derived slides, topic visibility controls, hierarchical navigation
(including return to a parent topic), aspect ratios, appearance settings and a
presenter view showing the map alongside a slide. Its
[export documentation](https://xmind.com/user-guide/working-with-files) lists
PowerPoint among the output formats. These are useful precedents for reversible
visibility, consistent framing and retaining map context while presenting.

MindManager's official
[Windows 21 knowledge-base entry](https://kb.mindmanager.com/en/125360)
records removal of direct PowerPoint export in that version and points users
toward Slides. The [support page](https://www.mindmanager.com/en/support/) and
[Microsoft integrations page](https://www.mindmanager.com/en/integrations/microsoft/)
provide version context, not proof of a particular PPTX object model.

Neither vendor's reviewed official documentation establishes that PowerPoint
output consists of individually editable vectors/text, nor specifies deleted
topic handling or synchronization guarantees after arbitrary map changes.
Do not advertise those as verified parity features. Thinoda defines its own
explicit contract for those requirements. Speaker-note, ordering and automatic
sibling-isolation details were not verified in the official MindManager sources;
they are not used as claimed competitor capabilities.

Research-driven additions to the original request are explicit 16:9/4:3 fitting,
reversible hidden-subtree controls, parent/depth-aware focus, presenter map
context, stable slide ordering/duplication, reduced-motion preservation and
documented deletion/backward-compatibility behavior. Existing playback features
should be retained rather than reimplemented as new scope.

## Authoring and playback

Open **View > Edit Presentation** and generate a draft or add the branch/Zone
selected in the map. The slide strip becomes a vertical rail in a wide workspace
and a horizontal strip in a narrow one. Select a thumbnail to edit that slide;
use **Duplicate Slide**, **Move Up**, **Move Down** and **Remove Slide** to shape
the narrative without changing map topics.

To add the current board view, position and zoom the canvas before opening the
editor, then choose **Capture** next to **View** in the editor. This adds a slide
with a fixed viewport containing board objects and map content; its
X/Y/Width/Height can be adjusted in the slide settings. The viewport is captured
when the editor opens, not continuously while the editor is open.

**Slide source** selects the live branch, Zone or View. For branch and Zone
slides, **Source subtree** excludes unrelated topics and **Map context** retains
the surrounding map. **Auto-fit content** follows visible content after map
edits; disable it for a fixed crop and edit X/Y/Width/Height. Auto-framing is
conservative for curves and labels and may include extra space.
**Limit topic depth** counts the source root as depth zero. Click a topic on the
vector stage to hide its subtree, or use the
keyboard-accessible **Topic visibility** controls. **Hidden topics** offers
individual restoration and **Restore all topics**.

For source-only branch and Zone slides, board objects explicitly assigned to
another Zone are excluded. Objects with no Zone assignment remain visible if
they fall within the resulting frame. They do not enlarge an automatically
fitted source frame; disable auto-fit and adjust the frame to include them.
Connectors are hidden if a topic or object endpoint is hidden, including
through chains of connectors. Context and View slides retain all Zone objects,
subject to topic visibility and the slide frame. The editor, player and export
paths use the same object-visibility rules.

**Apply** saves the draft as one undoable edit; **Cancel** discards it.
**Apply and export PowerPoint** deliberately saves the draft before opening the
ratio/export flow. PDF and slide-image actions export the draft without applying
it. AI suggestions remain reviewable and do not bypass Apply.

GUI PowerPoint export uses the current resolved light or dark app appearance,
including the system setting, as shown in the slide preview. This changes
dark-mode PowerPoint exports that previously used a light page; programmatic
and CLI callers continue to choose their existing export options. Print has
its own page-appearance controls.

Pitch and Presenter View use the same slide scene resolution. The presenter's
**Map** orientation panel deliberately shows the complete map with the current
slide viewport highlighted. It is not exported, but is visible whenever Presenter
View is on screen; share the slide-only view when hidden map content must stay
private. Current map revisions refresh playback; reordering retains the active
slide by ID. Playback is bound
to the captured document/sheet index/root, not to an arbitrary other sheet that
happens to reuse topic IDs. If that source is replaced or all slides are removed,
the player ends safely. A deleted source's slide can be restored by undoing the
map deletion.

For branch and Zone slides, PowerPoint text is emitted as editable text boxes
with measured line placement and the topic's paragraph alignment and text
styles. Connecting curves are editable freeform vector
paths, not a promise of PowerPoint auto-routing/reconnection; photographs remain
separate pictures. Viewport slides instead become non-editable images, as
reported before export. Some frame clipping on editable branch and Zone slides
cannot be preserved and is also reported. PDF retains vectors using outlined
text from the existing export font resolver; editable text is a PPTX guarantee
for supported branch and Zone content, not a PDF text-search guarantee.
Editable card shapes, text, annotations, pictures and paths carry their card's
rotation, and shape labels rotate around the parent shape. They are separate
PowerPoint objects, not a single linked object. The interactive canvas rotates
card text, images and annotations around the same card center; its title
editor remains upright at the rotated title position.
Branch, relationship and free-connector anchors use the same rotated-card
geometry as the shared layout; rotating a card does not reflow its children.
PowerPoint edits do not synchronize back into Thinoda. Clipping is visual and
must not be relied on as secure redaction.

## Implementation rubber-duck review

The second independent review traced scene isolation, deletion and undo
ownership, slide identity/reordering, native DrawingML, multipage PDF resource
renumbering and bounds, localization and the documentation contract. It found
no blocking correctness defect. Two polish findings were corrected:
the presenter's Map panel should retain full-map orientation rather than repeat
the isolated stage, and filmstrip work should be bounded by visible thumbnails
rather than resolve every slide on every frame.

The presenter regression verifies full-map context beside an isolated stage in
both appearances. Offscreen filmstrip thumbnails now skip scene resolution and
map cloning. A 512-slide, 436-topic fixture resolves four vertical or two
horizontal thumbnails per frame, with a regression cap of eight independent of
deck length. Twenty debug redraws took 39-44 ms per layout/appearance combination;
this is focused fixture evidence, not a universal device performance guarantee.
Local review also exposed a zero-height rail allocation that clipped the first
thumbnail and its title. The wide workspace now reserves the scroll viewport
and heading height; a paint-clip regression covers the complete first card in
both appearances.

Separate integration acceptance uncovered two export issues:

- Multipage slide PDF still rasterized whole pages. It now composes vector Form
  XObjects; regressions require no image objects for a text-only deck and allow
  raster objects only for original photographs.
- Installed PowerPoint required repair for custom geometry inside connector
  elements even though Microsoft's Open XML SDK reported no schema errors.
  Emitting those same curves as native editable freeform shapes preserves their
  geometry and opens cleanly. A semantic regression prevents reintroducing
  custom connector geometry.

### Scene object-isolation follow-up

A later review found that source-only editor and Pitch scenes retained
foreign Zone objects even though exports filtered them. Scene object
visibility is now shared with the export path, and connector exclusions
propagate to dependent connectors. Auto-fit also excludes source-owned
connectors whose endpoints are hidden; otherwise an invisible connection to a
distant object could enlarge the slide. Retained clipping frames now suppress
their independent background tint as well as their title and outline.
Synthetic regressions cover source, context and View slides, unassigned objects,
ink, connector chains, hidden and unrevealed topics, clipping frames, object
order, SVG, PNG, PDF and PowerPoint output. A 2,000-connector reversed chain
checks iterative exclusion propagation rather than general performance.
Production-browser Pitch and PowerPoint checks passed in Chromium, Firefox and
WebKit for viewport slides; they do not exercise source-only isolation.

This preserves the existing crop behavior for unassigned objects rather than
turning every unrelated free object into an auto-fit target. The presenter's
orientation panel still shows the whole map, and slide clipping is not secure
redaction. The follow-up has not been accepted on every browser, device or
Office version; the earlier Office evidence below predates this change.

## Evidence and status

All eight regenerated Office fixtures pass Microsoft Open XML SDK validation.
Fresh, unmodified dark 4:3 and light 16:9 decks open in installed Windows
PowerPoint without repair; each exposes four slides with individual native
topic shapes, text, paths and original photos. Those generated files are
retained as session acceptance artifacts. Schema validity alone is not treated
as successful PowerPoint acceptance.

Final verification passes: 307 core/IO tests, 246 app library tests, the editor
and playback integration executables, formatting, strict Clippy and the
production web build. The editor matrix covers all ten languages, both
appearances and narrow/wide layouts. No new tests are ignored.

Implementation and both critiques are complete for branch and Zone slides.
Native opening acceptance was performed in Windows PowerPoint, not in every
Office version or LibreOffice. Font substitution and consumer-specific text
metrics remain portability limits; editable text, vector geometry and separate
photographs remain the contract for supported branch and Zone content. The
newer flattened viewport-slide path is covered separately and was not part of
those Office fixtures. This work does not create a release or merge the feature
branch.

The later editable-rotation fixture passes the Open XML SDK alongside the other
nine Office fixtures. Windows PowerPoint opened it without repair and exported
a PDF showing its rotated sticky, photo and diamond, branch, relationship and
free connector. In a separate copy opened in Windows PowerPoint, the rotated
sticky's text was edited and a native custom-geometry path was nudged. Both
changes persisted in the saved PPTX; the original fixture was unchanged. This
is evidence for those two editing operations, not a systematic appearance or
editing matrix. Other Office versions and hosts remain unverified.

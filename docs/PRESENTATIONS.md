# Vector presentations

## Outcome and implementation contract

Replace screenshot-based slide authoring and PowerPoint handoff with live vector
scenes and editable PowerPoint objects. A slide is a view of current map content,
not a detached copy of its text. Source topics remain the single source of truth.

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
- Manually framed slides retain their saved viewport. Visibility is semantic,
  not just spatial: a sibling inside that viewport must still be excluded.
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
rejected. Do not silently rasterize unsupported vector/text content.
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

**Slide source** selects the live branch or Zone. **Source subtree** excludes
unrelated topics; **Map context** retains the surrounding map.
**Auto-fit content** follows visible content after map edits. Disable it for a
fixed crop and edit X/Y/Width/Height. **Limit topic depth** counts the source root
as depth zero. Click a topic on the vector stage to hide its subtree, or use the
keyboard-accessible **Topic visibility** controls. **Hidden topics** offers
individual restoration and **Restore all topics**.

**Apply** saves the draft as one undoable edit; **Cancel** discards it.
**Apply and export PowerPoint** deliberately saves the draft before opening the
ratio/export flow. PDF and slide-image actions export the draft without applying
it. AI suggestions remain reviewable and do not bypass Apply.

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

PowerPoint text is emitted as editable text boxes, with measured line placement;
it is not a screenshot or outlined glyph image. Connecting curves are editable
freeform vector paths, not a promise of PowerPoint auto-routing/reconnection.
Photographs remain separate pictures. PDF retains vectors using outlined text
from the existing export font resolver; editable text is a PPTX guarantee, not a
PDF text-search guarantee. PowerPoint edits do not synchronize back into Thinoda.

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

Implementation and both critiques are complete. Native opening acceptance was
performed in Windows PowerPoint, not in every Office version or LibreOffice.
Font substitution and consumer-specific text metrics remain portability limits;
editable text, vector geometry and separate photographs remain the contract.
This work does not create a release or merge the feature branch.

_Last updated: 2026-11-20 15:11:55 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Nearby selection menu

The existing nearby mind-map node menu is the selection menu for the canvas.
It shares one overlay identity with card formatting, board object formatting,
and shape/connector label editing. Selecting an object changes the available
sections, not the location of formatting in the application.

## Sections and scope

- **Actions** retains node actions such as adding a subtopic, connecting topics,
  notes, links and to-dos. Other objects show their applicable commands.
  **More actions** reuses the existing target-specific context menu and filters
  out unavailable commands.
- **Text** contains font, size, decorations, alignment and text colour.
  While editing a card or object label, **Selected text** means the range and
  **New text** means subsequent typing at the caret. Outside editing, controls
  apply to the selected object(s). Changing a whole-object text property clears
  only that property's run overrides; unrelated formatting and lists remain.
- **Style** contains applicable appearance controls: topic shape/fill/border,
  card appearance and geometry, shape fill/outline/width, or connector
  routing/colour/width/arrows. Shape effects are under **More style options**;
  detailed label layout is under **More text options**. Card-only multi-selection
  exposes common fill, preserving each card's kind and geometry in one undo step.
  Single-card geometry is not offered as a misleading multi-card operation.
- **Arrange** contains the available stacking commands, including front/back
  and above/below the map. For one ungrouped, unlocked canvas object it also
  shows exact Left, Top, Width, Height and (except containers) Rotation, like
  draw.io's Arrange geometry: values are the unrotated frame, each completed
  field edit is one undo step, and a rejected value stays as a draft with
  Apply/Cancel. The section is omitted when none apply.

All draw.io stencil instances use the shape capabilities; there is no
per-library formatting menu. Image cards do not show text controls. Locked
objects retain applicable actions such as unlocking, not editable style
controls. Ink, annotations, frames, groups and mixed selections retain their
applicable actions and arrangement commands; unsupported properties are not
invented or applied to unrelated kinds.

The previous `+`/chevron was **Insert**, not a text setting. It is no longer
part of the selection-formatting bar. Creation remains in the toolbox and
normal Insert commands; selected-node creation actions remain under Actions.
The former upper floating insert toolbar is removed, including when nothing is
selected. Marker and find commands remain accessible from selection Actions.
The former simultaneous "inline" and "whole-card" controls are replaced by
the explicit editing scope above.

## Interaction

The canvas does not move when the menu appears. Controls wrap inside a compact
popup near the selection; the main formatting surface has no horizontal
scrollbar. Long font lists, colour pickers and detailed option menus are
separate popovers. Placement accounts for viewport edges, camera transforms,
rotated objects and the creation toolbox. The closed menu also avoids the
actual hit regions of visible quick-connect, add-topic, resize, rotate,
relationship and image-resize handles.

If the full surface cannot clear those handles, a compact **Format** button
opens its controls on demand. The button itself stays clear of the handles.
The explicitly opened popover may temporarily cover canvas content or the
toolbox on a constrained viewport; dismiss it to use what is underneath.
If there is not even room for the button without covering a handle, the nearby
menu is hidden rather than blocking that handle.

Cards default to Actions, with a compact Actions dropdown when the full action
row cannot fit. Explicit formatting opens Text; editing opens Text automatically.
Typography uses compact rows without reducing the 28-point icon targets. The
creation toolbox yields when it would overlap the selection menu, and returns
after selection/formatting closes. Selected-object formatting takes priority
over ink-tool creation options, which return when formatting is dismissed.
Placement prefers keeping the selected title/editor clear. On constrained
canvases where no clear rectangle exists, explicitly requested formatting may
overlap the object body rather than becoming unreachable; Escape dismisses it
without discarding the draft. Default Actions prefers avoiding other cards and
frame titles while preserving handle access.
Sections always appear in the same order: Actions, Text, Style, Arrange.
Active shape/connector label editing uses the same placement algorithm, anchored
to the object's visible bounds and avoiding the editor where space permits.
Its Text section is flat like the other producers: decorations and lists, then
font family and size, then text colour swatches with a custom-colour entry.
Family, exact size and custom colour open inline value pages inside the same
menu so the label edit is not ended by a popup. On compact canvases the swatch
row collapses into a single **Text Color** entry that opens the swatches.
Placement penalises any position covering the selection centre. When a very
short window leaves no other position the node menu may still cover the
selected topic; right-clicking the menu then opens the selection's full
context menu, exactly as right-clicking the object would. Selecting an object
by right-click does not close the context menu it opens, and the full context
menu always stacks above the nearby menu, even when both appear in the same
frame: like the former toolbars, the nearby menu is a Middle-order tool surface,
so every Foreground popup and auxiliary window stays above it. While a mind-map title is being edited the menu
stays visible and opens Text when editing starts; leaving the editor returns
the topic to its default Actions section.
Below the native 720 x 480 minimum, the expanded popover may cover a small
shape; the passive nearby menu still avoids its active handle hit regions.

Card and shape/connector editors retain their own validated draft, IME and
undo pipelines. Switching away from an active text editor commits through
that editor, after its input pass. A failed commit retains the draft and
reports the error rather than displaying a successful section transition.
Composition disables formatting transitions. Existing formatting shortcuts,
card Alt+F10 and label F6 remain editor-specific focus entry points.
Plain cards start rich draft history before their first keystroke, so enabling
inline formatting after typing preserves the same undo history. Pointer events
in the companion menu cannot terminate an active IME preedit; cancelled or
multi-contact touches neither apply formatting nor commit the title.

## Design critique and acceptance

The design review rejected a second floating toolbar beside the existing node
menu: identical actions in different places would preserve the original
problem. It also rejected one giant wrapping inspector, ambiguous inherited
versus inline colours, and replacing the existing text-editing state machines.

The highest-risk boundaries are same-frame text plus section navigation,
numeric/picker draft ownership, IME, failed commits, grouped/locked selections,
and menu placement on small canvases. Regression checks must exercise those
boundaries, not just assert that a menu exists. Geometry checks must inspect
actual rendered controls, not only a requested popup size.

### Final rubber-duck critique

| Challenge | Resolution / remaining boundary |
|---|---|
| Sharing an Area ID does not prevent two producers painting during editor handover. | A per-pass producer claim prevents competing card, label and selection surfaces; discarded passes can render again. |
| Estimated popup size is not proof of fit. | Measured section/owner/width sizing and rendered-control checks cover nodes, stencils, cards and active card editing at 320 x 240 and 640 x 360, in both themes. Font selectors participate in their parent's wrapping layout. |
| Inline and whole-object controls can silently disagree. | One explicit editing scope is shown; whole-object changes clear only matching run overrides. Rich drafts keep typing, composition and formatting in one history. |
| Blanket mixed-selection toggles can leave some objects unchanged. | Common values and all-selected toggle state are computed across targets; mixed shape typography and multi-card fill have one-step undo regressions. |
| Removing a strip can lose its commands. | Node actions, markers, links, notes, to-dos and find remain in Actions/More actions; native command availability filters unsupported operations. |
| Right-clicking a new object could paint the nearby menu over the freshly opened context menu (egui keeps stale relative order for areas that appear together). | The nearby menu uses Middle order (as the former toolbars did), below every Foreground popup and auxiliary window; regression tests assert paint order after right-click selection and that feature windows are not intercepted. |
| A short window can leave the nearby menu over the selection, swallowing right-clicks. | A secondary click inside the visible selection menu opens the selection's full context menu; any retained marker target is cleared first; tested at 640 x 240 logical. |
| A popup can physically cover a section tab. | Normal popup dismissal/keyboard focus is retained; no pointer click-through bypass is used. Section/owner changes close stale popups. |
| Re-measuring after a topic's own buttons change (to-do, link) hid the menu for one frame, so it flickered after every such action. | Content-only changes keep the menu visible at its previous size; the existing same-frame size check discards and repaints if the size changed. Layout-affecting changes (window size, fonts, language, scale) still re-measure invisibly. |
| Every kind does not support every property. | Stencils reuse native shape capabilities; generic/annotation selections expose supported actions and layering only. This does not invent new editable properties, card stacking, or multi-card geometry. |
| Moving menus invalidate pixel comparisons beneath their shadows. | Outline acceptance excludes both actual popup shadows, requires at least two unchanged corners at normal size and actual visible stroke pixels at 640 x 360 and the native 720 x 480 minimum, across both themes. |
| A visually clear outline can still hide a larger interactive handle. | Placement uses the actual handle hit rectangles, including rotated/zoomed shapes, topic additions, card transforms, relationships and images. A bounded sweep searches free space without evaluating every obstacle at every candidate. |
| A full surface may not fit without covering a handle. | A compact button opens a temporary popover; if the button itself cannot fit, it is hidden. The expanded popover can cover handles until dismissed, and dense or extremely small canvases cannot guarantee a visible nearby button. |
| A nested font or colour popup can replace egui's root popup and strand its parent. | The compact parent owns its open state separately, keeps nested controls usable, closes stale state when the selection disappears, and returns focus on Escape without clearing the selection. |

The independent source review accepted the producer and generic capability
gates and identified the missing multi-card fill path, now covered above.
Its final pass did not re-review IME or inspect visuals; those boundaries are
covered by the focused editor regressions and GPU-rendered acceptance, not by
an overstated blanket review claim. Browser screen-reader behavior and
platform-specific IME integration still require platform acceptance.

This is the contextual-formatting increment, not completion of draw.io phases
D9-D15.

Research: draw.io's [editor overview](https://www.drawio.com/docs/manual/editor/)
and [getting-started guide](https://www.drawio.com/docs/getting-started/)
distinguish object selection from editing label text. Thinoda retains that
scope distinction without adopting draw.io's permanent format-panel layout.

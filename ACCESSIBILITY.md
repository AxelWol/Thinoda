# Accessibility

Thinoda is a mind mapping app for giving your thoughts shape. Thinking,
planning and organizing ideas are things everyone does, so Thinoda should work
for everyone: people who use only a keyboard, people who use a screen reader or
magnifier, people who need large text or strong contrast, and people who read
the interface in a language other than English. The same is true for
contributing: anyone should be able to report a problem, translate, test or
write code.

This document explains what we focus on, what we expect from contributions,
how to report a barrier, who looks after accessibility, where Thinoda is
expected to work, and which problems we already know about.

If you came here because something in Thinoda did not work for you: we are
sorry, and we want to hear about it. Go straight to
[Reporting accessibility issues](#reporting-accessibility-issues).

## Priorities

Thinoda is maintained by volunteers. We have not had Thinoda evaluated by an
independent accessibility expert, and we do not claim that it meets any
accessibility standard. We use the Web Content Accessibility Guidelines
(WCAG) 2.2 at level AA as an **aspirational goal** to guide our decisions, as
far as they apply to a desktop app.

These are the outcomes we work towards, in order of priority:

1. **You can create, read and edit a map without a mouse.** Topics can be
   added, edited, selected and rearranged with XMind-style shortcuts (for
   example Tab for a child topic, Enter for a sibling, arrow keys to move
   between topics). Every command can be found and run from the command
   palette (Ctrl+Shift+P, Command+Shift+P on macOS). F1 opens a searchable list
   of all shortcuts. Dialogs can be used and closed with the keyboard.
2. **Screen reader users can find and understand the controls.** On desktop,
   Thinoda tells the operating system about its buttons, menus, switches, text
   fields and dialogs, including what each one is called and whether it is on
   or off. Buttons that only show an icon have a spoken name. The **Outliner**
   shows the map as a text tree you can read and edit, as an alternative to the
   visual map.
3. **You can see what you are doing.** You can choose a light or dark
   appearance or follow your system, and enlarge the whole interface from 75%
   to 200% (Settings > General > UI Scale) without changing the map zoom. The
   app's own colours are checked automatically for readable contrast. Topic
   text picks a readable colour for its background, and the Symbols panel warns
   when a symbol is hard to see and can fix it for you. You can use any font
   installed on your computer. Zen mode hides panels to reduce clutter.
4. **You can use Thinoda in your language.** The interface, messages and the
   offline, searchable user guide are available in English, German, Spanish,
   French, Danish, Dutch, Portuguese (Portugal and Brazil), Italian and Polish.
   You can switch language without restarting.

## Contributor expectations

Every change that people can see or use should keep Thinoda at least as
accessible as it was. In practice:

- **Give every control a name.** Each button, toggle, field and menu entry
  needs a translated name that a screen reader can announce. This matters most
  for buttons that only show an icon. In code, use `accessible_name`,
  `labelled_by` or `WidgetInfo` from egui.
- **Make it work with the keyboard.** New actions must be reachable through the
  keyboard, normally as a command (`Cmd`) that appears in the command palette
  and, where it makes sense, has a shortcut. If something can be done by
  dragging, offer another way to do it. Focus should stay visible and return to
  a sensible place when a dialog closes.
- **Check light, dark and large.** Look at your change in Light and Dark
  appearance and at a UI Scale of 200%, and in a narrow window.
- **Translate it.** New text must be added to every language catalog (see
  [CONTRIBUTING.md](CONTRIBUTING.md#translations)).
- **Test it.** Add end-to-end tests in `crates/thinoda-app/tests` that find the
  control by its name with `egui_kittest` and check its role and state (for
  example whether a toggle is on). These tests read the same accessibility tree
  that screen readers use and run in CI on Windows, macOS and Linux as part of
  `cargo test --workspace`. CI also checks the UI colour palettes for
  sufficient contrast.
- **Show your evidence.** In the pull request, describe how you checked
  keyboard use and appearance. Screenshots in light and dark appearance help. If
  you tested with a screen reader, say which one and which version. If you
  could not test something, say so; that is fine.
- **Document it.** If your change affects how people use Thinoda, update the
  README, the offline help and, where relevant, the
  [Known limitations](#known-limitations) below.

Currently there is no automated check that catches a missing name or keyboard
path on its own, so reviewers check these by hand.

## Reporting accessibility issues

If something in Thinoda is hard or impossible for you to use, please tell us.
You do not need any technical knowledge, and you never need to tell us about a
disability.

You can report a problem in either way:

- **On GitHub:** open an issue in the
  [issue tracker](https://github.com/AxelWol/Thinoda/issues/new/choose). In the
  app, **Help > Send Feedback** opens the same page. Please start the title with
  `Accessibility:`.
- **By e-mail:** if you cannot or would rather not use GitHub, write to
  <axelwolters@hotmail.com> with the subject `Thinoda accessibility: <short
  description>`. E-mail is also the right choice if you prefer to keep your
  report private.

Tell us as much as you can of the following. It is all optional:

- what you were trying to do (for example "add a note to a topic");
- what happened, and what you expected to happen;
- the Thinoda version (**Help > About**) and whether you use the desktop app or
  the web app (and in that case, the web address and browser);
- your operating system and version;
- any assistive technology or settings you use, with versions if you know them
  (for example a screen reader, magnifier, voice control, switch access, or a
  large display scaling);
- whether you found a way around the problem.

Screenshots, recordings or sample maps are welcome but never required. Please
do not include confidential or personal information in them. Security problems
are reported privately as described in [SECURITY.md](SECURITY.md).

### Severity

You do not need to choose a severity. The maintainer assigns or confirms it
when looking at your report.

| Severity | Meaning | Examples |
|---|---|---|
| **Blocker** | You cannot complete a basic task at all, and there is no workaround. | A dialog cannot be closed without a mouse; the app cannot be started or a map cannot be opened or saved with a screen reader. |
| **Major** | You can complete the task only with great effort or help, or only through a workaround. | A frequently used button has no spoken name; a feature can only be reached by dragging; text is unreadable in dark appearance. |
| **Minor** | The task is possible but harder, slower or confusing. | A control is announced with an unclear name; focus jumps to an unexpected place after closing a dialog. |

### How we respond

- **Acknowledgement:** we aim to reply within 14 days to confirm that we have
  read your report.
- **Workaround:** if there is a way to get the task done today, we will tell
  you.
- **Plan:** we will tell you whether we can reproduce the problem, which
  severity we assigned, and how we plan to address it. Blockers come first.
- **Updates:** progress is recorded in the GitHub issue. For e-mail reports we
  open an issue (without your personal details) if you agree, or reply to you
  directly.
- **Verification:** when a fix is ready, we will ask whether you would like to
  try it before or after it is released.

Because Thinoda is maintained by volunteers, we cannot promise a fix by a
specific date. Fixes are published with the next regular release.

## Ownership and maintenance

Accessibility is looked after by the project maintainer,
[Axel Wolters](https://github.com/AxelWol). The maintainer:

- reads and triages accessibility reports and assigns severity;
- reviews pull requests against the [contributor
  expectations](#contributor-expectations);
- makes sure accessibility regressions are fixed before the affected version is
  released;
- keeps this document, the planned screen reader checks in
  [docs/PLAN.md](docs/PLAN.md) and the backlog in
  [docs/BACKLOG.md](docs/BACKLOG.md) up to date.

This document is reviewed at least with every minor release (for example
0.6.0) and whenever a change affects the supported environments or known
limitations. If responsibility for accessibility moves to someone else, this
section and the contact address above are updated in the same change.

## Supported environments

"Supported" here means that Thinoda passes information to the system's
accessibility features and that we fix problems reported there. It does **not**
mean that the combination has been fully tested with a screen reader.

| Environment | Status |
|---|---|
| Windows 10 and 11 (x64 and ARM64) | Desktop app supported. Works with the Windows accessibility interface used by Narrator and NVDA. Not yet tested systematically with a screen reader. |
| macOS 11 or newer (Apple silicon and Intel) | Desktop app supported. Works with the macOS accessibility interface used by VoiceOver. Not yet tested systematically with a screen reader. |
| Ubuntu 22.04 LTS or newer (x64) | Desktop app supported. Works with the Linux accessibility interface used by Orca. Not yet tested systematically with a screen reader. |
| Keyboard | Supported on all desktop platforms and in the web app. |
| Mouse, touchpad, pen | Supported on all desktop platforms and in the web app. |
| Display scaling and high-resolution screens | Supported; can be combined with UI Scale. |
| Light/dark system preference | Followed when Appearance is set to **System**. |
| Web app in current desktop browsers | Partial: keyboard and pointer work, screen readers do not (see below). |
| Touch-only devices (phones, tablets) | Not evaluated; no desktop app is offered for them. |

Other assistive technology, such as JAWS, screen magnifiers, voice control or
switch access, may work but has not been evaluated. We would like to hear about
your experience.

## Known limitations

These are the barriers we know about. They are not yet tracked as individual
GitHub issues; links will be added here when they are. If you run into
something that is not on this list, please
[report it](#reporting-accessibility-issues).

- **Screen readers cannot read the visual map.** The map area is announced as
  a single "Mind map canvas"; individual topics, branches and connections are
  not announced. *Workaround:* use the **Outliner**, which shows the same
  content as a text tree and lets you add, edit, move and delete topics.
- **Screen reader testing is incomplete.** Thinoda has not yet been checked
  systematically with NVDA, VoiceOver or Orca, so some panels or dialogs may be
  announced incompletely or in an unexpected order.
- **The web app does not work with screen readers.** It draws its interface as
  one picture, so screen readers, browser zoom and browser high-contrast or
  reading extensions have little or no effect. *Workaround:* use the desktop
  app.
- **System high-contrast themes are not applied.** For example, Windows
  Contrast themes do not change Thinoda's own interface. *Workaround:* use Dark
  or Light appearance, increase UI Scale, and choose a high-contrast map theme.
- **The system's "reduce motion" setting is not read.** Thinoda only uses a few
  short, small animations, such as switches sliding on and off.
- **Keyboard shortcuts cannot be changed.** The shortcut list (F1) can be
  searched but not edited.
- **Some tasks are easier with a mouse.** Choosing a region for Map Shot,
  placing floating topics freely and some drag-and-drop actions have keyboard
  alternatives only in part.
- **Image and print output cannot be read by screen readers.** PNG, JPEG and
  the print PDF are pictures, and text in outlined PDF exports cannot be
  selected. *Workaround:* export to Markdown, OPML, plain text, Word or Excel to
  get readable text.
- **System dialogs follow your operating system.** File pickers, printer
  dialogs and installers belong to the operating system; their language and
  accessibility depend on your system settings, not on Thinoda.
- **Starter templates are in English.** The sample text in templates is not
  translated, but you can edit all of it.
- **Japanese and Chinese are not supported yet.** These need text input
  features that Thinoda does not have yet.

## Feedback and improvements

This document and the way we work on accessibility can always get better. If
something here is unclear, out of date or missing, or you have ideas for how we
could do better, open an issue or a pull request that changes this file, or
write to <axelwolters@hotmail.com>. Help with screen reader testing is
especially welcome.

If you are facing a barrier in Thinoda itself, please use
[Reporting accessibility issues](#reporting-accessibility-issues) so it can be
triaged and tracked.

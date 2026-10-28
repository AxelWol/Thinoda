_Last updated: 2026-10-28 06:50:11 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# XMind product research (basis for the Thinoda clone)

Sources: xmind.com (structure, features, user guide – 931 indexed pages), the locally
installed **Xmind 26.05.01106** (Electron app, `resources/app.asar` inspected for
identifiers only), and UI screenshots of the desktop app (German locale).

**Thinoda status refresh, 2026-10-26:** sections 1-11 are the reference product
inventory, not a list of current Thinoda gaps. The
[value/gap audit](#12-thinoda-valuegap-audit-2026-10-26) below compares current
source and v0.12.1 delivery with that inventory. Implementation priorities and
acceptance are maintained in [PLAN](PLAN.md#current-state-and-next-value-plan-2026-10-26)
and [BACKLOG](BACKLOG.md#next-value-backlog---xmind-audit-2026-10-26).

## 1. Desktop UI anatomy (Xmind 26)

**Notes/equations refresh (2026-10-28):** the official
[Note](https://xmind.com/user-guide/note-new) page confirms ordered/unordered
lists and bold/italic/underline emphasis. The
[Equation](https://xmind.com/user-guide/equation-new) page describes separately
inserted, editable LaTeX with resizing. Thinoda's first XV7 slice deliberately
adds source-first Markdown tools and an opt-in offline preview without
reinterpreting legacy notes or claiming full rich-text/LaTeX parity. Exact
source survives native saves/recovery; search and other views/exports may
still show markup. Structured notes, plain-text projection and equations
remain distinct follow-up acceptance items.

| Area | Content |
|------|---------|
| Top-left | Hamburger menu, Home button, document title |
| Top-center floating toolbar | Relationship, Summary, Boundary, Zone, Label, Task/To-Do, Insert (+ ▾) |
| Top-right | Marker/Sticker panel toggle (☺), Format panel toggle |
| Right format panel | Tabs **Style** (topic) / **Pitch** / **Map** |
| Bottom-left | Sheet tabs + "+" (new sheet) |
| Bottom-right status bar | Topic count ("Nodes: 1 / 5"), zoom % ▾, View all notes, Outliner, Zen/fullscreen, Pitch |
| Canvas | Central topic (no fill, 30 px ExtraBold), colored main topics (rounded rectangles), curved colored branches, "+" add-topic button next to selected topic |

**Hamburger menu:** Undo (Ctrl+Z), Redo (Ctrl+Y), New blank map (Ctrl+N), Open home (Ctrl+H),
Open local file (Ctrl+O), Save (Ctrl+S), Save as (Ctrl+Shift+S), Export ▸, Print (Ctrl+P),
Settings (Ctrl+Shift+P), File ▸, Edit ▸, Insert ▸, Tools ▸, View ▸, Help ▸.

**Export submenu (in this order):** PNG, JPEG, SVG, PDF, Markdown, Word, Excel,
PowerPoint (Pitch), OPML, TextBundle, Excel (Tasks), Calendar (Tasks). Several are marked
PRO/PREMIUM in Xmind – all are free in Thinoda.

**Format panel – Map tab:** structure picker (preview), color theme (palette + 6 variants: multicolor
and 5 monochrome), custom theme "+", background color, global font, branch line width,
colored branch toggle + color, Map style (auto balance, compact map), topic display
(justify sibling alignment, show all notes).

**Format panel – Style tab (topic):** shape, fill, border (shape/color/width), length (fixed width),
text (font, size, weight, color, B/I/S/U, case, alignment), structure (e.g. clockwise),
branch (line shape, line end, width, colored branch), reset style.

## 2. Structures (`structureClass` identifiers, needed for .xmind compatibility)

- Mind map: `org.xmind.ui.map`, `.map.clockwise`, `.map.anticlockwise`, `.map.unbalanced`, `.map.floating*`
- Logic chart: `org.xmind.ui.logic.right`, `.logic.left`
- Brace map: `org.xmind.ui.brace.right`, `.brace.left`
- Org chart: `org.xmind.ui.org-chart.down`, `.org-chart.up`
- Tree chart: `org.xmind.ui.tree.right`, `.tree.left` (+ `.btt`, branch-aligned variants)
- Timeline: `org.xmind.ui.timeline.horizontal` (+ up/down/rtl), `.timeline.vertical` (+ btt, through/symmetric)
- Fishbone: `org.xmind.ui.fishbone.leftHeaded`, `.fishbone.rightHeaded`
- Matrix: `org.xmind.ui.spreadsheet` (+ row/column)
- Tree table: `org.xmind.ui.treetable`
- Mixed structures: any topic may override the structure of its subtree.

## 3. Topic elements & enrichment

Central/main/sub/floating/summary topics, relationships (curved, labeled, styled ends),
boundaries (wrap a range of sibling topics), summaries (bracket + summary topic that can grow
its own branch), callouts, zones (new: canvas areas grouping content), markers, stickers,
illustrations, labels, notes (rich text), hyperlinks (web / topic link / local file / folder),
attachments, local images, audio notes, LaTeX equations, comments, numbering, To-Do and Task
(dates, progress, assignee, dependencies, Gantt view), custom markers.

**Marker ids** (as stored in `.xmind`): `priority-1..9`, `task-start|oct|quarter|3oct|half|5oct|3quar|7oct|done`,
`flag-<color>`, `star-<color>`, `half-star-<color>`, `people-<color>`, `smiley-<name>`,
`arrow-<dir>`, `symbol-<name>`, `c_simbol-*`, `month-<mmm>`, `week-<ddd>`, `tag-<color>`.

## 4. Styling

Skeletons (structure + shapes), smart color themes (6 variations each), custom themes,
quick styles, copy/paste/reset style, update style to all main/subtopics, background color,
global font, branch line width, colored branches, hand-drawn style, auto-balance,
compact map, justify topic alignment, branch free-positioning, flexible floating topics,
topic overlap, CJK font.

## 5. Views & presentation

Mind map / Outliner (indent/outdent, drag), Zen mode, Pitch mode (slides from topics,
presenter view, aspect ratios, themes, export as PDF / video), Gantt chart, Show branch only,
fold/unfold (all) sub-branches, zoom (Ctrl +/−/0), fit map, Map Shot (snapshot of a region),
view all notes, topic count, topic filtering (marker, label, task, text, note), find & replace,
smart guidelines, floating toolbar, add-topic button.

## 6. Files

- Formats: `.xmind` (zip: `content.json`, `metadata.json`, `manifest.json`, resources; legacy
  XMind 8: `content.xml`).
- Import: EdrawMind (.emmx), MindManager (.mmap), FreeMind (.mm), MindNode, Word (.docx),
  Markdown, OPML, TextBundle, Lighten.
- Export: PNG/JPEG (1x–3x, transparent, split by main branch, frames), SVG, PDF (map, outliner,
  pitch), Excel, Word, PowerPoint, OPML, TextBundle, Markdown, tasks as CSV/ICS.
- File cache (auto backup, retrieve after crash/power loss), save/save as, merge files
  (into sheets or current map), password encryption, print, multiple sheets (tabs),
  new from template / library.

## 7. Platform & app features

Appearance follows system (light/dark), language, spell check, auto updates, usage
statistics opt-in, default zoom, last used skeleton/theme, CJK font, floating toolbar,
add-topic button, customizable shortcuts (search, reset), Quick Entry (macOS),
cloud: Xmind Cloud, external storage integrations, sharing (link, email, embed),
real-time collaboration, comments, teams/spaces, SSO, AI (generate, refine, explain,
work breakdown, chat), MCP server / CLI.

## 8. Default keyboard shortcuts (Windows; ⌘ on macOS)

| Action | Shortcut |
|---|---|
| Subtopic | Tab |
| Sibling after / before | Enter / Shift+Enter |
| Parent topic | Ctrl+Enter |
| Edit topic | F2 / Space / typing |
| Delete topic / single topic | Delete / Ctrl+Delete |
| Fold / unfold | Ctrl+/ |
| Undo / Redo | Ctrl+Z / Ctrl+Y (⌘⇧Z) |
| Copy / Cut / Paste / Duplicate | Ctrl+C / X / V / D |
| Hyperlink | Ctrl+K |
| Summary | Ctrl+] |
| Bold / Italic / Underline | Ctrl+B / I / U |
| Find & replace | Ctrl+F |
| Zoom in / out / reset | Ctrl+= / Ctrl+- / Ctrl+0 |
| Priority markers 1–7 | Ctrl+1 … Ctrl+7 |
| New / Open / Save / Save as | Ctrl+N / O / S / Shift+S |
| Print | Ctrl+P |
| Settings | Ctrl+Shift+P (⌘,) |
| Next / previous sheet | Ctrl+Tab / Ctrl+Shift+Tab |
| Navigate | Arrow keys |

## 9. Website feature pages (xmind.com screenshots, Oct 2026)

### Structure page ("Built for thinkers, not just drawers")
* Structures showcased: **Zone** (focused areas for exploration, mixes structures), Mind Map, **Matrix** (rows × columns),
  Logic Chart, **Timeline** (horizontal/vertical), **Tree Table**, Tree Chart, Org Chart.
* **Skeleton**: pick logic + look up-front (structure presets with visual skeleton previews).
* **Smart Color Theme**: palette tabs (Favorite/My Colors/Colorful/Classic), auto-contrast between text and background.
* **Custom Theme** (user-defined theme: structure, shape, colors) and **Quick Style** (one-click polished presets).
* Toolbox: **To-Do (Task)** with progress, **Topic Link** (jump to another topic/sheet), Summary, Boundary,
  Relationship, **Marker and Sticker**, Label, Note.

### Project management page
* **AI Work Breakdown** (split a topic into actionable tasks), **Task** panel (assignee, start/due date, progress,
  priority, duration), **Project working days / non-working days** calendar, **Gantt chart** view synced with the map,
  Pitch mode for status updates, Spaces to organize projects.

### Integration page
* AI tools: ChatGPT plugin, **Xmind MCP server** (create/read/update maps from AI agents), **Skills & CLI**.
* Import: Markdown, OPML, TextBundle, other mind-map apps (drag & drop a file to import).
* **Web clipper** (capture web pages/Google Docs into maps).
* Collaboration integrations: Slack, Zoom, Notion embed (share links).
* **Cloud storage**: Google Drive, Dropbox, OneDrive, Box... via an "External Storage" dialog.
* Export & sharing: Word, PDF, PowerPoint, images; share to email, Facebook, Twitter, Pinterest, blogs.

### Collaboration page
* Real-time co-editing with live cursors, cross-device sync (desktop, web, iOS, Android), share links with
  view/edit permissions and optional password, **comments** on topics (threads, resolve), team workspaces & spaces.

### Security page
* AES-128 file encryption (password-protected files), ISO 27001 / SOC 2 Type II (cloud service), SAML SSO for teams.

**Thinoda implications:** Zone, Matrix, Tree Table, Gantt, Task panel, comments, topic links, stickers, password
encryption, an MCP server/CLI and cloud-provider folders are tracked in `docs/PLAN.md` / `docs/BACKLOG.md`.
Server-side features (real-time collaboration, SSO, compliance certifications) are out of scope for the desktop
MVP and live in the backlog.

## 10. Solutions pages (use cases)

* **Time management**: Eisenhower matrix (urgent/important quadrants), priority markers, labels and colors to group
  tasks, "Generate as to-dos" (AI converts branches into tasks), templates (Personal SWOT, Eisenhower, Ansoff).
* **Presentations**: Pitch Mode turns the map into slides automatically (one branch per slide), with pitch themes
  and aspect ratios, transitions, and export to PowerPoint, PDF or video. The structure picker drives the slide layout.
* **Education/learning**: curriculum maps, study plans as to-dos, notes alongside structured topics, and shared
  maps for research groups.
* **Brainstorming**: AI-assisted idea expansion, "Brainstorm" boards (cards grouped in columns) organized into a
  mind map in one click, and shared co-ideation.
* **Collaboration**: team workspaces, share links, comments, and co-editing.
* **Strategic planning**: goal → strategy → action breakdown, to-dos, and fishbone/SWOT templates.
* Common flow on every page: start blank or from a template (or AI prompt) → structure → style → share → export
  (PDF/PNG/PowerPoint).

**Thinoda implications:** a template gallery with Eisenhower, SWOT, Ansoff, fishbone and study-plan templates;
Pitch mode with slide export (PowerPoint/PDF); a brainstorm card board; to-do generation (rule-based now, optional
AI provider later). All of these are tracked in PLAN/BACKLOG.

## 11. Role pages (xmind.com "Xmind for …")

| Role | Pain points named by XMind | Features highlighted | Templates offered |
|---|---|---|---|
| Marketers | cross-functional campaigns, fast strategy changes, scattered assets | Matrix/tables for campaign plans, marker panel (Color, Priority, Progress, Flag, Star, People), strategic-planning maps, AI map generation, comments | Customer journey map, Competitive analysis, Marketing calendar |
| Software engineers | large-system complexity, SDLC orchestration, cross-team alignment | Architecture diagrams (org/tree structures + boundaries), dark themes, monospace "code" look, SDLC timelines, async comments | Flowchart for debugging, Architecture diagram, Code review checklist |
| Consultants | scope creep, communicating value, risk management | Benchmark matrix with colored shape cells (High/Medium/Low), generate as to-dos, live sharing, attachments/links to external resources | Six Sigma (DMAIC) process map, SWOT analysis, BCG matrix |
| Students | organizing study planning, group work, heavy workloads | Structure picker (Mind Map, Brace, Logic, Tree, Org, Timeline, Fishbone, Tree Table, Matrix, Grid), PowerPoint export, AI study plans | Annual learning plan, Lecture note-taking, Thesis outline |
| Teachers | different learning styles, online engagement, lesson-prep workload | Daily planner (tables), comments, "Following" (presenter follow mode), AI lesson outlines | Lesson planner, Teacher planner, Curriculum flow |
| Project managers | unclear scope, fragmented communication, remote teams | Tree Table with Start/End date and Duration columns, Task info (start/due, progress), Gantt, stage timelines, context menu "Mark as Completed" | Project kickoff map, Agile project workflow, Project manager resume |

**Thinoda implications:**
* Role-oriented **template gallery categories** (Business, Engineering, Education, Project Management, Personal).
* **Marker panel tabs**: Marker / Sticker / Illustration.
* **Tree Table columns** for task fields.
* **"Mark as Completed"** in the context menu.
* A **presenter "follow" mode** (backlog: needs collaboration).
* The dark "engineering" theme (`neon`/monochrome) already exists.

## 12. Thinoda value/gap audit (2026-10-26)

Method: read this research and the complete user-guide digest, reconcile current
PLAN/BACKLOG and GitHub work, inspect the actual model/search/UI/format/render
surfaces, and re-fetch the official guides cited below. This is a source-based
capability audit, not a fresh runtime pass of every feature or every XMind guide.
No XMind binary/assets or third-party map contents were copied.

### Already delivered, not new feature requests

F0-F5 implementation is included in v0.12.1: Fill repair, native menu/window
routing, named/configurable keyboard profiles, date-only tasks/calendars/Gantt,
Zones/Pitch with PDF/PNG output, bounded cancellable AI streaming, scoped chat and
depth modes, rewrite/translate/grow/reorganize/refine, saved prompts, explicit-model
embeddings search/clustering and reviewed Pitch suggestions. Existing templates,
custom themes/quick styles, 17 structures, Map Shot, masked derived copies,
sheet merge and cross-sheet text search must also be reused, not reinvented.
F6 native OS/device/accessibility acceptance is still partial; an implemented
command or enum is not proof of full reference interaction parity.

### Confirmed gaps and partially used foundations

The table records the audit baseline. **XV1-XV2 have since been implemented and
locally verified in the current worktree**, extending those foundations; this is
not yet part of v0.12.1. Current execution/test status is in PLAN/BACKLOG.
XV3-XV9 remain open. XV2 reuses `Topic.todo` without a schema change: a linear
bottom-up aggregation counts deepest checkboxes across plain grouping topics.
Explicit subtree completion/reopen changes existing checkboxes in one Undo and
leaves scheduled Tasks independent. Map/Outliner/Style and Find share the derived
state; graphical output includes numeric progress. The
[WAI-ARIA checkbox pattern](https://www.w3.org/WAI/ARIA/apg/patterns/checkbox/)
informed the mixed checkbox state and group activation. Unlike XMind's ring,
Thinoda retains its checkbox/count language and provides translated keyboard
buttons in Style. Actual native mixed-state accessibility remains a host gap,
not a claim inferred from appearance or harness checks.

| Work item | Evidence in current Thinoda source | Reference behavior and end-user value |
|---|---|---|
| XV1 faceted navigation + notes overview | `thinoda-core/src/search.rs` has text search and `filter_topics(markers, labels)` with ancestor context. `thinoda-app/src/ui.rs::search_bar` exposes text/scope/notes/labels/dimming, not a complete facet picker or an all-notes overview. | [Topic Filtering](https://xmind.com/user-guide/topic-filtering-new) exposes marker/label/task/priority and text/note navigation; [View All Notes](https://xmind.com/user-guide/view-note-new) consolidates notes and jumps to topics. High-frequency retrieval improvement; extend the foundation rather than claim filtering is wholly absent. |
| XV2 To-Do roll-up | `Topic.todo` is `Option<bool>`; `edit.rs::set_todo` sets an individual topic. Scheduled Tasks live separately in `schedule.rs`. No derived nested checklist completion is implemented. | [To-Do vs Task](https://xmind.com/user-guide/to-do-vs-task) explicitly distinguishes sub-To-Do aggregate progress from dated/dependent Gantt Tasks. Useful lightweight planning without forcing every checkbox into the scheduler. |
| XV3 numbering | `model.rs::Topic` has no numbering field and the current source has no structural numbering implementation. The old PLAN Phase 1 "done" claim was inaccurate and is corrected. | [Numbering](https://xmind.com/user-guide/numbering-new) is separate from title text, updates with topic position, supports siblings/tiered numbers/restart, and is not supported by XMind Pitch. Useful for outlines/procedures; do not counterfeit it with title prefixes. |
| XV4 embedded resources | Partial: bounded resources/native v3/recovery/clipboard support local PNG/JPEG insert/replace/remove, selection-based drop insertion, proportional inspector width/reset/above-below, cached map and Outliner previews, direct proportional handles with staged preview/cancellation/keyboard/one-step undo, shared visual/print/Pitch output, and a verified bounded XMind topic-image subset. XMind attachment package schema and installed-app interoperability remain unverified; unsupported content fails explicitly. | The pinned [`xmind-sdk-js` 2.2.33 source at `d35e820`](https://github.com/xmindltd/xmind-sdk-js/tree/d35e820a2d09995c69bbb452d4edc42b09a13946) verifies `src/core/topic.ts::Topic.image` uses `xap:resources/{componentID}` and returns `resources/{componentID}`; `src/utils/zipper.ts::updateManifestMetadata` writes bytes to that key and adds it to `manifest.json` `file-entries`. Official `xmind-model` 1.1.12 makes only `src` mandatory; absent dimensions/alignment are accepted by the model. Thinoda supports bounded PNG/JPEG only, strict safe single-component refs, declared archive resources, proportional defaults for missing dimensions, top/bottom alignment, and existing resource limits; its private metadata preserves exact bytes, display size and top/bottom state. Two fuzz corpus seeds and deterministic tests cover absent and partial metadata; the Linux campaign remains pending. The [Picture guide](https://xmind.com/user-guide/pic-new), rechecked 2026-10-27, confirms embedded images, top/bottom placement, resize and Reset Size. The [Attachment guide](https://xmind.com/user-guide/attachment-new) motivates explicit save/open consent, but does not verify the package schema. No installed XMind app was used. |
| XV5 document ingestion | `thinoda-io/src/lib.rs::Format::IMPORTABLE` excludes Word and TextBundle, although both are exportable and recognized by extension. | [Import](https://xmind.com/user-guide/import-new) explicitly includes DOCX and TextBundle. Prioritize common document-to-outline workflows over implementing every competitor format. Resource-bearing imports depend on XV4. |
| XV6 presentation handoff | `model.rs::Presentation` stores ordered source frames; `thinoda-io/src/presentation.rs` renders those frames to PDF/PNG. `Format` has no PowerPoint variant; no explicit presentation aspect-ratio setting exists. | [Export](https://xmind.com/user-guide/export-new) includes PowerPoint; [Pitch](https://xmind.com/user-guide/pitch-mode-new) describes aspect ratios and delivery options. Start with honest image-based PPTX and fit preview, not promises of editable Office objects or video. |
| XV7 rich notes / equations | Notes are `Option<String>` edited with a buffered text field in `ui.rs`; current model/Insert surfaces lack an equation content type/render path. | [Note](https://xmind.com/user-guide/note-new) supports rich lists/emphasis; [Equation](https://xmind.com/user-guide/equation-new) supplies editable mathematical/chemical notation. Preserve draft/recovery/search behavior when introducing formatting. |
| XV8 task Tree Table | `Structure::TreeTable` exists, but `layout.rs` routes it through the same stack family as LogicRight/Matrix. F3 scheduling data exists; task-column Tree Table editing does not. | The earlier project-manager role research calls for start/end/duration columns. This is a useful Thinoda extension grounded in that research, not a newly verified exact XMind column/editing contract. Reuse Gantt's validated schedule engine. |
| XV9 durable local AI conversations | `thinoda-app/src/ai/chat.rs` stores bounded conversations in `ChatState` and explicitly labels history "session-only". The chat surface/depth modes exist, but reopen-after-restart and multiple named threads per file do not. | [Xmind Agent](https://xmind.com/user-guide/xmind-ai-chatbot/web), "Manage your conversations", documents separate per-file chats and reopening history. Preserve research continuity with optional app-local history, not a hosted account or automatic inclusion in shared documents. |

### XMind image-package evidence

The official [XMind JavaScript SDK](https://github.com/xmindltd/xmind-sdk-js)
provides a concrete image-resource example beyond the user-guide behavior. Its
published package metadata identifies version 2.2.33. At pinned source revision
[`d35e820`](https://github.com/xmindltd/xmind-sdk-js/tree/d35e820a2d09995c69bbb452d4edc42b09a13946),
[`Topic.image`](https://github.com/xmindltd/xmind-sdk-js/blob/d35e820a2d09995c69bbb452d4edc42b09a13946/src/core/topic.ts)
sets the image `src` to `xap:resources/{componentID}` and returns
`resources/{componentID}` as the archive key;
[`Zipper.updateManifestMetadata`](https://github.com/xmindltd/xmind-sdk-js/blob/d35e820a2d09995c69bbb452d4edc42b09a13946/src/utils/zipper.ts)
stores the supplied bytes at that key and adds it to `manifest.json` under
`file-entries`. The
[official image example](https://github.com/xmindltd/xmind-sdk-js/blob/d35e820a2d09995c69bbb452d4edc42b09a13946/example/example.image.js)
uses a PNG fixture. This is evidence for an SDK-compatible image-package
contract, not proof that every current XMind version uses it or that attachment
topics share the same schema; the SDK model documents topic images but no attachment representation. The
initial fuzz corpus did not contain resources. Two bounded PNG package seeds
have since been added for missing and partial image metadata, with deterministic
import and round-trip regression tests.

Follow-up verification of the official [`xmind-model` 1.1.12
schema](https://github.com/xmindltd/xmind-model/blob/c448b0fcfc8bee51f68124aae87a57e3947c6b44/schemas/image.json)
and its [topic-image tests](https://github.com/xmindltd/xmind-model/blob/c448b0fcfc8bee51f68124aae87a57e3947c6b44/test/models/topic-image.test.js)
found that only `src` is required: width and height may be absent, and the
model accepts arbitrary alignment values (including `left`). This confirms why
the importer must not assume those optional properties are present. Thinoda's
verified subset derives missing display dimensions proportionally from bounded
decoded image dimensions, defaults absent alignment to top, and supports only
explicit top/bottom alignment. Other explicit alignments are rejected rather
than silently remapped. URL/path references remain rejected and are never
resolved or fetched. These model tests do not establish which alignments or
source forms are used by current XMind application exports.

Thinoda's current implementation is limited to this verified image-package
contract, with bounded decode and manifest/path checks. Attachment/unknown
resource forms remain unsupported rather than silently discarded or
interpreted. Generated package fixtures test the supported contract; no
installed XMind application interoperability or successful local fuzz campaign
is claimed.

Re-fetched on 2026-10-26: To-Do vs Task, Numbering, Topic Filtering, View All Notes,
Export, Import and Xmind Agent (including conversation history). The other linked guides remain prior digest references;
their detailed behavior must be rechecked before implementation. Defaults,
unseen controls and undocumented limits are not inferred from screenshots.

### Value ranking and deliberate non-parity

Recommend XV1 -> XV2 -> XV3 first: each helps existing offline maps daily without
new hosted services or a large binary-content subsystem. XV4 then removes a
portability blocker and enables richer imports/assets. XV5-XV9 follow as bounded
workflow improvements, with the precise order/dependencies in PLAN. Remaining
F6 acceptance remains open. The user's subsequent instruction authorizes XV
execution while preserving the deferred stability inventory and host limits.

Do not chase cloud feature count: real-time collaboration, hosted permissions,
web clipping/online research, Quick Entry background lifecycle and image/video AI
introduce service, privacy or platform commitments. Keep them later and opt-in.
Prefer original useful templates to copied catalogs. Retain Thinoda's deliberate
AI preview/review/Apply/Undo model instead of XMind's automatic incremental edits.
Local-first content, transparent fidelity limits and dependable recovery provide
more immediate user value than branding, commercial badges or superficial parity.

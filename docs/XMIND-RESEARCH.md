_Last updated: 2026-10-17 09:37:32 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# XMind product research (basis for the Thinoda clone)

Sources: xmind.com (structure, features, user guide – 931 indexed pages), the locally
installed **Xmind 26.05.01106** (Electron app, `resources/app.asar` inspected for
identifiers only), and UI screenshots of the desktop app (German locale).

## 1. Desktop UI anatomy (Xmind 26)

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

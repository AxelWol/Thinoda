# XMind product research (basis for the yMind clone)

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
PRO/PREMIUM in Xmind – all are free in yMind.

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

_Last updated: 2026-10-27 12:09:42 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# XMind user guide digest (verification checklist)

This checklist condenses the desktop-facing behaviors documented in the official XMind user guide at [xmind.com/user-guide](https://xmind.com/user-guide). Verify each item against the cited page; where the guide gives no exact values, defaults, or shortcut, that omission is stated rather than guessed. Keyboard equivalents below are Windows first, followed by the documented Mac equivalent.

## Thinoda reconciliation (2026-10-26)

The sections below describe **XMind**, not assertions that Thinoda implements or
lacks every item. Current code and the released v0.12.1 baseline were compared
with this checklist; see the
[research audit](XMIND-RESEARCH.md#12-thinoda-valuegap-audit-2026-10-26) for exact
source evidence and [current plan](PLAN.md#current-state-and-next-value-plan-2026-10-26)
for delivery/acceptance status.

| Guide area | Thinoda state | Follow-up |
|---|---|---|
| Preferences: Shortcuts, Task/calendar/Gantt, Zone/Pitch, text/embedding AI | Implementation shipped; native/browser evidence and remaining host gaps are recorded in PLAN. | Do not create duplicate F1-F5 implementation tasks. PPTX and more delivery settings are not implied by Pitch PDF/PNG. |
| Topic Filtering / View All Notes | Text search and core marker/label matching exist; unified faceted navigation and a consolidated notes UI are missing. | XV1 |
| To-Do vs Task | Independent checkboxes and scheduled Tasks exist; hierarchical checklist progress is missing. | XV2 |
| Numbering | No structural numbering model/rendering in current Thinoda. | XV3; old Phase 1 checkmark corrected. |
| Picture / Attachment | Bounded local PNG/JPEG insert/replace/remove, selection-based drag/drop, inspector width/reset/above-below, map and visual/Pitch output build on native v3/recovery/clipboard. Direct proportional resize handles (staged preview, cancellation, keyboard arrows, one-step undo) are implemented locally. Import/export supports the pinned SDK's verified embedded topic-image subset; XMind attachment package exchange and installed-app interoperability remain unverified. Local attachment topics and a separate explicit Save/Download action are implemented; no external-app launch. Outliner previews are implemented. | XV4 partial |
| Import | TextBundle Markdown and bounded DOCX text/hierarchy import are implemented locally with conversion warnings; Word page layout and embedded resources are not preserved. External relationships are never fetched. | XV5; other unsupported importers stay in the broader inventory. |
| Export / Pitch Mode | Saved framing and PDF/PNG slides exist. A local XV6 slice exports saved Pitch slides to image-based `.pptx`, with explicit 16:9/4:3 choices, a first-slide fit preview and a flattened-slide disclosure. PowerPoint/LibreOffice interoperability remains unverified. | XV6 partial |
| Note / Equation | Plain notes exist; structured rich notes and rendered equations are missing. | XV7 |
| Structure / project-management role research | TreeTable enum/layout exists; editable task columns are missing. | XV8; exact reference interaction still needs verification. |
| Xmind Agent conversation history | Scoped/depth-guided chat exists; Thinoda explicitly keeps history session-only. | XV9 optional local persistence and named threads, not another chat implementation. |
| File Cache / Map Shot / Custom Themes / Quick Style / Merge / Mask Data | Existing implementations; not wholly missing. | Retain recovery, export-fidelity and real-host acceptance requirements. Masked copies are not a guarantee of universal anonymization. |
| Set Password / Audio / cloud collaboration / vision and video AI | Not delivered in this feature release. | Existing R4/R10/R12 and later-service backlog; do not silently expand F5. |

**Subsequent XV1-XV2 implementation:** faceted Find, the all-notes overview and
nested To-Do progress are now implemented and locally verified, not yet released.
Thinoda counts deepest To-Dos through plain grouping topics, provides one-Undo
subtree completion/reopen and keeps scheduled Tasks independent. Its shared
checkbox/count UI deliberately differs from XMind's circular indicator.
The rows above preserve
the research baseline; PLAN/BACKLOG record the current evidence and remaining
host checks. XV3-XV9 remain open.

To-Do vs Task, Numbering, Topic Filtering, View All Notes, Export, Import and
the [Xmind Agent guide](https://xmind.com/user-guide/xmind-ai-chatbot/web) were
re-fetched from the official guide on 2026-10-26. Other entries retain their
earlier source verification and need a reference refresh before exact-parity
implementation. XMind commercial availability does not impose Thinoda paywalls.

## Add Topic Button
- With one topic selected, verify one or two adjacent `+` buttons appear; use the child button to add a subtopic and the sibling button to add a same-level topic.
- The guide also identifies `Tab` for subtopic and `Enter` for sibling insertion.
- Source: https://xmind.com/user-guide/add-topic-button

## Advanced Layout
- In Format > Advanced Layout, verify toggles for **Branch Free-Positioning**, **Flexible Floating Topics**, and **Topic Overlap**.
- Branch positioning allows branch movement; flexible floating topics are positioned independently; overlap permits topic overlap. Undo with `Ctrl+Z` or uncheck the option.
- Source: https://xmind.com/user-guide/advanced-layout-new

## Applying Global Format
- Verify the map-level format panel can apply a skeleton, color theme, and background color to the sheet/map.
- Source: https://xmind.com/user-guide/applying-global-format

## Attachment
- Insert > Attachment (toolbar or menu) adds a file as a new topic; clicking it prompts to save locally and opens it with the system default application.
- Delete the attachment topic with Delete or right-click > Delete.
- Source: https://xmind.com/user-guide/attachment-new

**Thinoda continuation (source rechecked 2026-10-27):** follows selected-topic
Insert > Attachment and Delete, but intentionally separates selection from
delivery. One opaque local file (1 byte–8 MiB) becomes a selected child topic
with a paperclip title. Style > Attachment shows sanitized filename, byte size
and uninspected status. **Save/Download attachment** is a second explicit action:
choose a native destination or request a browser download via the VFS. Exact
bytes are preserved; no external app is launched, no path/URL is fetched and
documents/macros/scripts are never interpreted or executed. This is safe saving,
not XMind's click-to-open/system-default-app behavior or attachment preview parity.
All ten locales and Light/Dark/System share the same commands and inspector.
Native v3/recovery/clipboard/undo retain attachments; other exports explicitly
refuse them. Thinoda rejects XMind attachment topics because their package
schema has not been verified; image-only XMind support does not imply
attachment interoperability or installed-app compatibility (XV4 partial).

## Audio Note
- Desktop: select a topic > Insert > Audio Note; recording requires an enabled microphone. Select the audio-note child to play it; right-click > Save As exports it.
- The guide says a topic containing an Audio Note does not support inserting links.
- Source: https://xmind.com/user-guide/audio-note-new

## Background Color
- Select blank sheet area > Format > Map > Background Color; choose a background hue. Undo with `Ctrl+Z` / Edit > Undo.
- XMind automatically adjusts text color according to background depth; exact palette values are not specified in the article.
- Source: https://xmind.com/user-guide/background-color-new

## Boundary
- Select one or more topics, then use toolbar **Boundary** or Insert > Boundary. Topics from different branches receive separate boundaries; same-branch selections group under one boundary.
- Shortcut: Windows `Ctrl+B`; Mac `Shift+Command+B`.
- Source: https://xmind.com/user-guide/boundary-new

## Branch Line Width
- Select blank sheet area > Format > Map > Branch Line Width; choose a thickness. Exact width values are not listed in the article.
- Undo using `Ctrl+Z` or Edit > Undo.
- Source: https://xmind.com/user-guide/branch-line-width-new

## Branch
- Verify branch coloring can be applied using a palette; the Topic article describes nine colored-branch schemes in XMind Desktop.
- Branch styling includes line shape, thickness, color, and tapering; exact line-width value lists are not enumerated in the branch article.
- Source: https://xmind.com/user-guide/branch-new

## Browse and View
- Verify map navigation features include folding/unfolding, find and replace, and topic filtering; the page is an overview linking to those feature guides rather than a complete shortcut reference.
- Source: https://xmind.com/user-guide/browse-and-view

## Callout
- Select a topic > Insert > Callout; edit the callout’s shape, colors, text, and other styling in Format > Style.
- The shortcut is customizable in Preferences > Shortcuts; the article does not provide a fixed default combination.
- Source: https://xmind.com/user-guide/callout-new

## Color Theme
- In Format > Map, choose a preset color theme or create a custom one. The article does not enumerate preset names or a default theme.
- Source: https://xmind.com/user-guide/color-theme-new

## Comment
- Click the map/topic, choose the Comment toolbar icon, type a comment, then Send or press Enter to publish.
- Open a comment’s `…` menu to Edit or Delete; drag it to reposition. Availability depends on the collaboration context.
- Source: https://xmind.com/user-guide/comment

## Copy/Paste Style
- Right-click a styled topic > Copy Style; right-click another topic > Paste Style.
- Right-click a topic > Reset Style restores its default style.
- Source: https://xmind.com/user-guide/copy-paste-style-new

## Create New
- File offers **New Blank Map**, **New From Template**, and **New From Library**; templates can also be selected from the Templates page.
- New-map shortcut: Windows `Ctrl+N`; Mac `Command+N`.
- Source: https://xmind.com/user-guide/create-new

## Custom Themes
- Create a theme from Tools > Create/Customize Theme or Format > Custom Themes > Create Theme; customize and Save.
- Saved themes can be edited, renamed, duplicated, or deleted from Custom Themes.
- Source: https://xmind.com/user-guide/custom-themes-new

## Equation
- Select a topic > Insert > Equation (toolbar or menu) to add a LaTeX mathematical or chemical equation.
- Double-click to edit; drag its frame to resize.
- Source: https://xmind.com/user-guide/equation-new

## Export
- File > Export supports PNG, SVG, PDF, Excel, Word, OPML, TextBundle, PowerPoint, Markdown, and other formats; exact availability can vary by app/version.
- PDF options include Mind Map, Outliner, or Pitch Mode and current map or current file; background color and watermark can be retained. Landscape PDF can be produced via File > Print and Save as PDF.
- PNG/JPEG allow 2×/3× scaling, Split by Main Branch, transparent background, and the **Presented with Xmind** watermark toggle. Image framing offers six frame styles, aspect ratio, background color, and padding.
- SVG exports Current Map or Current File; installed fonts are needed for correct text rendering. Other formats are selected from Export and saved to a chosen location.
- Source: https://xmind.com/user-guide/export-new

## Export Task
- Export > Excel (Task) writes a `.csv`; the CSV’s Task ID is the Topic ID, and dependencies are represented as Topic ID plus dependency type (example: `abc123FS`).
- The guide also describes export to Calendar (`.ics`) for tasks with start dates; exact field/format details beyond the article are not specified here.
- Source: https://xmind.com/user-guide/export-task

## File Cache
- File > File Cache > select a file > Retrieve restores an edited-file backup; Clear Cache removes the selected temporary cache data.
- The guide says cache auto-backs up edited XMind files locally and is supported from version 9.1.0. It gives no cache cadence, retention count, or auto-save interval; it advises periodic manual saves.
- Source: https://xmind.com/user-guide/file-cache-new

## File Location
- The guide says XMind saves locally by default. For iOS and Android, it documents storage-folder selection; it does not specify a desktop default path or desktop folder-setting control.
- Source: https://xmind.com/user-guide/file-location

## Find and Replace
- View > Navigation Panel opens the search panel; Windows `Ctrl+F`, Mac `Shift+F` per this article.
- Search settings include **Find & Replace**, **Whole Words**, **Match Case**, and scope selection. Do not conflate this page’s Mac shortcut with Zen Mode’s documented `Command+F` content-search shortcut.
- Source: https://xmind.com/user-guide/find-and-replace-new

## Floating Toolbar
- When a topic is selected, the floating toolbar can appear beside it. Settings > **When topic is selected** offers **Always**, **Click Again**, or **Never**.
- Source: https://xmind.com/user-guide/floating-toolbar

## Floating Topic
- Create by double-clicking blank sheet space, Insert > Floating Topic (then click its position), or right-click blank space > Insert Floating Topic.
- Select a floating topic and press Space or double-click to edit; it can have subtopics. It is independent of the central topic/branches.
- Source: https://xmind.com/user-guide/floating-topic

## Fold and Unfold Subtopics
- Fold/unfold one selected branch using Edit or the topic’s context menu; a hover button folds, and the child-count indicator unfolds.
- Single-branch shortcut: Windows `Ctrl+/`; Mac `Command+/` to fold and `Shift+/` to unfold. Fold/unfold all sub-branches: Windows `Ctrl+Alt+/`; Mac `Shift+Command+/` (documented identically for both actions).
- Source: https://xmind.com/user-guide/fold-unfold-subtopic-new

## Gantt Chart
- Open from the Gantt toolbar icon or View > Gantt Chart. Drag the time bar to change duration; dependent tasks update automatically.
- Drag a task’s progress bar to update progress; select a task to edit details in the left panel, which sync to the map.
- Source: https://xmind.com/user-guide/gantt-chart

## Global Font
- Select blank sheet area > Format > Map > Global Font; select a font to apply globally. Undo with `Ctrl+Z` or Edit > Undo.
- Source: https://xmind.com/user-guide/global-font-new

## Import
- Open an existing file with File > Open; recently opened files are available under Files > Recently Opened. A `.xmind` file can also be opened from the OS.
- File > Import supports formats including EdrawMind, MindManager, FreeMind, MindNode, DOCX, Markdown, OPML, and TextBundle. The guide does not give an exhaustive format/version matrix.
- Source: https://xmind.com/user-guide/import-new

## Label
- Insert > Label adds a label to a topic; multiple labels are comma-separated. Existing labels can be selected from the quick-label suggestions.
- Select a label and delete it from the label popup or right-click > Delete.
- Source: https://xmind.com/user-guide/label-new

## Link
- Links can target webpages, topics (including another sheet), local files, and local folders.
- Webpage link shortcut: Windows `Ctrl+K`; Mac `Command+K`. Use Insert > Link to choose the link type.
- Source: https://xmind.com/user-guide/link-new

## Map Shot
- Open Tools > Map Shot; shortcut: Windows `F7`, Mac `Command+Shift+P`.
- Capture settings include Square (1:1), frame thickness/color, **Presented with Xmind** label, and Copy & Save. The capture region can be resized.
- Source: https://xmind.com/user-guide/map-shot-new

## Map Style
- Select blank map area and use Format > Map > Map Style to apply **Auto Balance Map**, **Compact Map**, or **Justify Topic Alignment**.
- These actions rebalance, compact, or align same-level topic lengths; undo with `Ctrl+Z` or uncheck the option.
- Source: https://xmind.com/user-guide/map-style-new

## Markdown to Mind Map
- File > Import > Markdown (`.md`) converts a Markdown file; the first line becomes the central topic (filename is used if the first line is non-text).
- Valid Markdown text pasted/imported is converted to structured topics and preserves corresponding Markdown formatting; lists become topic hierarchy.
- Source: https://xmind.com/user-guide/markdown-to-mind-map

## Marker
- Select topics and use toolbar Marker or Insert > Marker, then select a marker; click an existing marker to switch it.
- Built-in groups include priority, task progress, flags, stars, arrows, and symbols. Desktop supports custom marker groups/icons; exact marker catalog is not fixed by the article.
- Source: https://xmind.com/user-guide/marker-new

## Mask Data
- Select a map with sensitive text, choose the toolbar sparkle icon > **Data Masking**, then **View Now** to inspect and adjust the masked map.
- Masking replaces parts of sensitive text with asterisks; review results before sharing.
- Source: https://xmind.com/user-guide/mask-data

## Merge XMind Files
- Tools > Merge Xmind Files lets users select multiple `.xmind` files and merge them as **Multiple Sheets**; **Generate a Map of Content** turns other maps’ central topics into branches and creates links.
- Merge can be undone: Windows `Ctrl+Z`; Mac `Command+Z`.
- Source: https://xmind.com/user-guide/merge-xmind-files-new

## Note
- Insert > Notes adds a rich-text note to a topic; notes support ordered/unordered lists and bold, italic, and underline.
- Shortcut: Windows `Ctrl+Shift+N`; Mac `Shift+Command+N`.
- Source: https://xmind.com/user-guide/note-new

## Numbering
- Select one or more topics > Format > Style > Numbering; choose a style. The text listing does not enumerate the numbering-format names.
- **Apply to Siblings** copies the style to same-level topics. **Tiered Numbers** inherits the parent’s numbering and defaults off; siblings also enable it. **Start From Begin** restarts at the smallest number and defaults off. Reordering topics automatically renumbers them; choose **None** to remove numbering.
- Numbering precedes markers and topic text, matches text color, and is not editable as topic text. It is unsupported in Pitch Mode. In Outliner, typing a recognized first number at the start of text auto-generates numbering, but its style cannot be edited there.
- Source: https://xmind.com/user-guide/numbering-new

## Outliner
- Switch via View > Outliner or the lower-right Outliner control. Desktop Outliner currently supports local files only.
- Enter adds a topic; Tab indents; Shift+Tab outdents. Drag topics to reorganize; Insert offers Note, To-Do, Task, Hyperlink, Label, Image, and Equation.
- Valid embedded local PNG/JPEG images appear as small, labeled previews beside their topic in the Outliner. Legacy paths/URLs, attachments, and unsupported image data are not previewed.
- Outliner zoom: Windows `Ctrl++` / `Ctrl+-`; Mac `Command++` / `Command+-`. Show Branch Only: Windows `Ctrl+;`; Mac `Command+;`.
- File > Export supports PDF, Markdown, Excel, Word, OPML, and TextBundle, with Mind Map/Outliner and sheet/content choices.
- Source: https://xmind.com/user-guide/outliner-new

## Picture
- Insert a local image from Insert > Local Image, or drag a local image onto a topic. Reposition it above/below a topic and resize by dragging.
- Right-click the image > Reset Size restores its original size.
- Source: https://xmind.com/user-guide/pic-new

Thinoda implementation, rechecked 2026-10-27: one selected topic owns the local
PNG/JPEG picker; Style > Local Image provides proportional width, above/below,
Reset Size (160 × 100 fit, no upscaling), replace and remove. Original bytes
survive native storage/undo and appear in graphical outputs. Editable
non-native formats other than the limited XMind image-package subset reject
resource-bearing documents rather than discard content. No external reference
is fetched or executed.

**Verified package subset (current worktree):** against
[`xmind-sdk-js` 2.2.33 at commit `d35e820`](https://github.com/xmindltd/xmind-sdk-js/tree/d35e820a2d09995c69bbb452d4edc42b09a13946),
`Topic.image` uses `image.src = "xap:resources/{componentID}"`; the returned
`resources/{componentID}` key is written as bytes and declared in manifest
`file-entries`. The official `xmind-model` 1.1.12 schema requires only `src`;
its tests show width/height may be omitted and arbitrary alignment can be set.
This slice accepts only bounded embedded PNG/JPEG and safe single path
components. Missing dimensions are derived proportionally from decoded bytes,
missing alignment defaults to top, and explicit alignment is limited to
`top`/`bottom`. Thinoda preserves exact bytes, size and placement in its private
round trip. This does not verify XMind
attachment package data or every XMind version. No installed XMind app was
used, so installed-app interoperability is not claimed.

## Pitch Mode
- Enter with the Pitch Mode toolbar button; full-screen, windowed, and Presenter View modes are documented. Presenter View shows the presentation alongside the map.
- Full-screen settings include themes (e.g. Dark/Light), aspect ratio, topic visibility, three delivery modes, layouts, and image position. Press `?` in full screen to display the shortcut list; no fixed entry shortcut is specified in the article.
- Source: https://xmind.com/user-guide/pitch-mode-new

## Preferences
- Open Settings from Xmind > Settings (Mac), upper-left menu > Settings (Windows), or the Home sidebar. Shortcut: Mac `Command+,`; Windows `Ctrl+,`.
- General options: Appearance (system/light/dark), Toolbar buttons effect, Language, Week starts on Monday, Check spelling, Start Quick Entry at system login, Automatically install updates, Automatically send usage statistics.
- Editor options: Default zoom level, Use last used skeleton & color theme, Always display Smart Guideline, Default CJK font, Show Floating Toolbar, Show Add Topic button.
- Calendar options: Basic Calendar, Working Days, Exceptions. Share options: Email and available Evernote connection. Shortcut settings are searchable/customizable and resettable; changes take effect immediately, with no Apply step noted on this page.
- Source: https://xmind.com/user-guide/preferences-new

## Preferences: Shortcuts
- Open Settings/Preferences > Shortcuts; filter by **All** or category and search by action name.
- Select an action’s key field to assign a combination; remove an existing assignment with its cancel button first. A key type can appear only once in a combination. **Apply** saves changes; **Reset All** restores defaults. Desktop customization is supported.
- Source: https://xmind.com/user-guide/preferences-shortcuts-new

## Print
- File > Print; shortcut Windows `Ctrl+P`, Mac `Command+P`.
- Print settings include Content, Page Size, Layout, Scale, and Background Color. Right-click a branch for **Export Branch To…** or **Print Branch**. **Actual Size** can cut a large map across pages.
- Source: https://xmind.com/user-guide/print-new

## Quick Style
- Apply named Quick Styles to topic(s), including **Very Important**, **Important**, **Cross Out**, and **Default**; they change emphasis/style rather than topic content.
- Source: https://xmind.com/user-guide/quick-style-new

## Relationship
- Select a topic/boundary, choose Relationship in the toolbar or Insert menu, then click a second topic; alternatively invoke Relationship and select the two endpoints in sequence.
- A relationship is a customizable connector with line, color, arrows, and text description.
- Source: https://xmind.com/user-guide/relationship-new

## Reorganize
- Select a map/topic, right-click blank area > Reorganize, then choose **Auto** or **On Demand**. On Demand accepts user directions/requirements.
- Source: https://xmind.com/user-guide/reorganize

## Retrieve Your XMind Files
- Desktop (macOS/Windows): File > File Cache > select file > Retrieve; supported from version 9.1.0.
- If retrieval fails, the guide recommends contacting Xmind support with the corrupted file. iOS retrieval is under Settings > File Cache.
- Source: https://xmind.com/user-guide/retrieve-your-xmind-files

## Save File
- XMind’s guide states maps are saved manually and the app does not support auto-saving (as described by that guide).
- File > Save prompts for a local path/name on first save; File > Save As creates a new file. Shortcut: Windows `Ctrl+S`; Mac `Command+S`.
- Source: https://xmind.com/user-guide/save-file-new

## Set Password
- File > Set Password lets the user set a file password and Password Hint.
- The guide states a forgotten password cannot be retrieved or cracked; verify password protection and recovery behavior without assuming a reset path.
- Source: https://xmind.com/user-guide/set-password-new

## Sheet
- A single `.xmind` file can contain multiple sheets. Create one with the `+` near the sheet name or Insert > New Sheet; create one from a topic with right-click > New Sheet From Topic.
- Source: https://xmind.com/user-guide/sheet-new

## Skeleton
- A skeleton defines map-wide base layout and topic/line styles; choosing another skeleton changes the whole map’s layout/style combination.
- Source: https://xmind.com/user-guide/skeleton-new

## Skip Non-Working Days
- Enable **Skip Non-Working Days** in the Task panel; first use prompts for a project working-days calendar.
- Configure working weekdays and date exceptions; preview updates when changed. These project changes do not alter the original calendar template.
- Source: https://xmind.com/user-guide/skip-non-working-days

## Smart Guideline
- Settings/Preferences > Smart Guideline > **Always Display** enables alignment guides while editing. Windows shortcut documented: `Ctrl+Shift+P`; Mac `Command+,` opens Preferences.
- Source: https://xmind.com/user-guide/smart-guideline

## Sticker and Illustration
- Select a topic > Insert > Sticker/Illustration; alternatively drag an item from the picker onto a topic or open it from the Marker toolbar.
- Move it to the topic’s top/bottom/left/right; resize by dragging; right-click > Reset Size restores its original size.
- Source: https://xmind.com/user-guide/sticker-illustration-new

## Structure
- Format > Structure changes map/topic structure; documented structures include Mind Map, Logic Chart, Brace Map, Tree Chart, Org Charts, Timeline, Fishbone, Matrix, and Tree Table.
- The Topic guide states desktop maps can mix structures; orientation can also be adjusted. The separate Structure article’s exact full list/options should be checked in-app.
- Source: https://xmind.com/user-guide/structure-new

## Style
- Select a topic and open Format > Style. Up to 31 topic shapes are offered; shape, fill, border, text font/size/color/format/alignment, structure, and branch appearance can be adjusted.
- Rich text formatting can apply to a selected portion of topic text; no default shape/font sizes are specified by the article.
- Source: https://xmind.com/user-guide/style-new

## Summary
- Select one or more topics > toolbar Summary or Insert > Summary. Same-branch selections group in one summary; different branches receive separate summaries.
- Central topics and multiple floating topics cannot receive summaries. The shortcut is customizable; no fixed default is listed.
- Source: https://xmind.com/user-guide/summary-new

## Task
- Add a Task from the top `+` toolbar or a topic’s right-click menu. Task details include priority, start date, due date, and progress; options can show task details/creator on the topic or keep them hidden.
- Remove a task using **Remove** in the task panel.
- Source: https://xmind.com/user-guide/task

## Text
- Double-click a topic or select it and press Space to edit. Format > Style > Text controls font, size, color, formatting, and alignment for one or multiple topics.
- The guide’s text-editing article does not specify F2 as an editing shortcut; verify configurable shortcuts separately rather than assuming it.
- Source: https://xmind.com/user-guide/text-new

## To-Do
- Add To-Do from a topic’s right-click menu or Insert > To-Do; deleting is available from the To-Do icon’s right-click menu.
- A main topic can carry a To-Do for progress tracking. Highlight Completed/Uncompleted Task opens the Navigation Panel for filtering/highlighting.
- Source: https://xmind.com/user-guide/to-do

## To-Do vs Task
- The 2026 guide distinguishes **To-Do** (lightweight checkbox tracking, including sub-To-Dos with an automatically updated circular progress indicator) from **Task** (priority, dates, and project-progress tracking).
- To-Do is identified as a Pro feature. The article describes the redesigned systems as replacing the previous Task and Planned Task split.
- Source: https://xmind.com/user-guide/to-do-vs-task

## Topic Count
- Topic count is shown at the map’s lower-right; clicking it opens topic, word, and character statistics.
- Source: https://xmind.com/user-guide/topic-count-new

## Topic Editing
- Topic types: Central, Main, Subtopic, Floating, Summary. Select a topic and press Enter for a main/sibling topic; Tab for child; Ctrl+Enter for parent (Mac: Command+Return for parent). Floating topics can be made by double-clicking blank canvas or Insert/context menu.
- Delete removes the selected topic; right-click > Delete Single Topic or Windows `Ctrl+Delete` (Mac `Command+Backspace`, shown as `⌘+⌫`) is the separate single-topic action. Space/double-click edits; new/default text is replaced when typing, while existing text is continued.
- `Shift+Enter` inserts a line break in topic text; `Alt+Up/Down` reorders topics. Mac equivalents: `Shift+Return`, `Option+Up/Down`. Width can be set in Format; **Fit** restores default width.
- Edit > Go to Central Topic: Windows `Ctrl+R`, Mac `Command+R`. Undo `Ctrl+Z` / Mac `Command+Z`; redo Windows `Ctrl+Shift+Z` / Mac `Shift+Command+Z`.
- Source: https://xmind.com/user-guide/topic-editing-new

## Topic Filtering
- Filter by topic text, note, marker, label, task, or priority; nonmatching map content is hidden while filtering.
- View > Navigation Panel > Marker & Label opens filtering; `Ctrl+F` (Mac `Command+F`) is listed for filtering. Right-click marker/label > **Highlight Relevant Topics** or task > **Highlight Completed/Uncompleted Task**.
- Source: https://xmind.com/user-guide/topic-filtering-new

## Update Style
- Format > Style > Update applies a changed main-topic style to peers at the same level.
- For subtopics, **Update to All Level x Topics** or **Update to All Subtopics** propagates the selected style; `x` denotes the selected subtopic’s hierarchy level.
- Source: https://xmind.com/user-guide/update-style-new

## View All Notes
- Right-click a note > View All Notes, or View > Navigation Panel > Note, to open the consolidated note list.
- Selecting a note overview entry navigates to its topic; the panel can search the collected notes.
- Source: https://xmind.com/user-guide/view-note-new

## XMind CLI
- The guide describes a CLI and agent skill for creating, reading, editing, and checking native local `.xmind` files; this page is a product overview, not a command/flag reference.
- Source: https://xmind.com/user-guide/xmind-cli

## XMind File Transfer
- The guide covers importing `.xmind` files into Android from local storage, USB/Bluetooth, OneDrive, Google Drive, or Wi-Fi Transfer between devices.
- Source: https://xmind.com/user-guide/xmind-file-transfer

## XMind Glossary
- Terms include `.xmind` file, sheet, topic/branch, relationship, boundary, summary, label, marker, and other topic content; use the glossary to verify product terminology rather than behavior.
- Source: https://xmind.com/user-guide/xmind-glossary-new

## Zen Mode
- Enter Zen Mode from its toolbar icon. It provides Light, Dark, Ultra Light, and Ultra Dark appearances; markers, stickers, illustrations, and the format panel remain available.
- **Today in ZEN** shows creation duration; `Ctrl+F` (Mac `Command+F`) opens content search. The article gives no Zen-entry keyboard shortcut.
- Source: https://xmind.com/user-guide/zen-mode-new

## Zone
- Create from right-click > Create Zone or toolbar Insert > Zone, then select content. Zones can wrap floating topics and their subtopics only, not the central topic or its subtopics.
- Shortcut: Windows `Ctrl+Alt+Z`; Mac `Command+Option+Z`. A zone’s content can be edited, rearranged, exported, or printed independently.
- Source: https://xmind.com/user-guide/zone

## Zoom In/Out
- Bottom-right zoom percentage controls map scale; Windows `Ctrl+mouse wheel up/down`, Mac `Command+mouse wheel up/down` zooms in/out.
- Source: https://xmind.com/user-guide/zoom-in-out-new

## Questions You May Ask
- The FAQ landing page links to file retrieval and file-transfer guidance; it does not define a distinct editor behavior or keyboard shortcut.
- Source: https://xmind.com/user-guide/questions-you-may-ask

## AI Explain
- Cloud/AI feature: the AI Explain guide is outside core offline editor behavior; verify separately against the current XMind AI service.
- Source: https://xmind.com/user-guide/ai-explain

## AI-Generated To-Dos
- Cloud/AI feature: the guide covers AI-generated To-Do suggestions; no offline editor shortcut/default is specified here.
- Source: https://xmind.com/user-guide/ai-generated-to-dos

## AI Work Breakdown
- Cloud/AI feature: AI-assisted work breakdown is service-dependent and is not part of core local-map behavior.
- Source: https://xmind.com/user-guide/ai-work-breakdown

## Create with AI
- Cloud/AI feature: map generation with AI is service-dependent; verify account, prompt, and network requirements separately.
- Source: https://xmind.com/user-guide/create-with-ai

## XMind AI
- Cloud/AI product overview; no offline editor behavior or keyboard shortcut is specified in this checklist.
- Source: https://xmind.com/user-guide/xmind-ai

## XMind MCP
- AI/cloud integration page; MCP availability and behavior depend on the connected service and setup.
- Source: https://xmind.com/user-guide/xmind-mcp

## Web Clipper
- Companion/cloud feature for capturing web content; it is not a core desktop-editor action in the cited guide.
- Source: https://xmind.com/user-guide/web-clipper

## Collaboration
- Collaboration and real-time shared editing are service/account-dependent; verify access and conflict behavior separately from offline editing.
- Source: https://xmind.com/user-guide/collaboration

## Sharing and Collaborating
- Account/cloud feature group; sharing destinations and permissions are service-dependent and not fixed by local editor behavior.
- Source: https://xmind.com/user-guide/sharing-and-collaborating

## Space
- Cloud workspace/account feature; not applicable to an offline-only editor unless a compatible service is implemented.
- Source: https://xmind.com/user-guide/space

## Team
- Team management/account feature; team membership and permissions depend on XMind’s service.
- Source: https://xmind.com/user-guide/team

## SSO
- Enterprise account/identity feature; authentication depends on organization configuration and XMind service.
- Source: https://xmind.com/user-guide/sso

## Shortcut table

| Action | Windows | Mac | Notes |
|---|---|---|---|
| Add sibling/main topic | `Enter` | `Return` | Selected topic |
| Add subtopic | `Tab` | `Tab` | Selected topic |
| Add parent topic | `Ctrl+Enter` | `Command+Return` | Selected topic |
| Delete Single Topic | `Ctrl+Delete` | `Command+Backspace` (`⌘+⌫`) | Separate from ordinary Delete |
| Edit selected topic | `Space` | `Space` | Double-click also edits |
| Insert topic line break | `Shift+Enter` | `Shift+Return` | In topic text |
| Reorder topic up/down | `Alt+Up/Down` | `Option+Up/Down` | Sibling ordering |
| Go to Central Topic | `Ctrl+R` | `Command+R` | Edit menu |
| Undo / Redo | `Ctrl+Z` / `Ctrl+Shift+Z` | `Command+Z` / `Shift+Command+Z` |  |
| Fold / unfold branch | `Ctrl+/` | `Command+/` / `Shift+/` | Single selected topic |
| Fold/unfold all branches | `Ctrl+Alt+/` | `Shift+Command+/` | As printed in the guide |
| Zoom map | `Ctrl+mouse wheel` | `Command+mouse wheel` | Bottom-right percentage is also available |
| Zoom Outliner | `Ctrl++` / `Ctrl+-` | `Command++` / `Command+-` |  |
| Outliner: Show Branch Only | `Ctrl+;` | `Command+;` |  |
| Search / find-replace | `Ctrl+F` | `Shift+F` | Find & Replace article; see its Mac-shortcut caveat above |
| Topic filtering | `Ctrl+F` | `Command+F` | Navigation Panel filter |
| Zen Mode content search | `Ctrl+F` | `Command+F` | Zen article; Mac shortcut differs from Find & Replace page |
| New map | `Ctrl+N` | `Command+N` |  |
| Save / Save As context | `Ctrl+S` | `Command+S` | Save page describes Save |
| Print | `Ctrl+P` | `Command+P` |  |
| Add boundary | `Ctrl+B` | `Shift+Command+B` |  |
| Add webpage link | `Ctrl+K` | `Command+K` |  |
| Add note | `Ctrl+Shift+N` | `Shift+Command+N` |  |
| Map Shot | `F7` | `Command+Shift+P` |  |
| Create Zone | `Ctrl+Alt+Z` | `Command+Option+Z` |  |
| Undo merge files | `Ctrl+Z` | `Command+Z` |  |
| Open Settings | `Ctrl+,` | `Command+,` | Preferences page |
| Smart Guideline Preferences | `Ctrl+Shift+P` | `Command+,` | Windows shortcut given by Smart Guideline article |
| Pitch Mode shortcut list | `?` | `?` | In full-screen Pitch Mode, reveals the list; not an entry shortcut |
| Summary, Callout, Zen, Pitch entry | Customizable / toolbar | Customizable / toolbar | No fixed shortcut listed by the cited pages |

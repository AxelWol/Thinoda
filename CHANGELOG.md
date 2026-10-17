# Changelog

All notable changes to yMind are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are generated automatically by
[release-please](https://github.com/googleapis/release-please) from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) merged to
`main` (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major;
while the version is `0.x`, breaking changes bump the minor version). See
[Releasing](README.md#releasing) for details.

## [0.1.0](https://github.com/AxelWol/yMind/releases/tag/v0.1.0) (2026-10-17)

First public version.

### Features

* Mind map editor with the mind map, logic chart, brace map, org chart, tree,
  timeline and fishbone structures, plus Matrix and tree-table views.
* Topics with notes, labels, markers, hyperlinks, images, relationships,
  boundaries, summaries and floating topics; multiple sheets per workbook.
* Outline view with add, edit, reorder, indent/outdent and delete controls.
* Themes, quick styles, custom themes and per-topic formatting.
* 35 templates in eight categories (New from Template).
* Import and export: XMind, FreeMind/Freeplane, OPML, Markdown, plain text and
  MindManager import; PNG, SVG, PDF and other export formats.
* Crash-safe autosave with full session recovery after a crash.
* GPU-accelerated rendering (wgpu), light/dark mode following the system,
  clipboard support, personal settings and cloud-folder awareness.
* English and German user interface.
* Optional local AI assistant (LM Studio).
* Installers for Windows x64/ARM64 (NSIS), macOS Apple silicon/Intel
  (`.app`, `.dmg`) and Ubuntu LTS (`.deb`, AppImage).

### Bug Fixes

* Windows: embed the application icon and version information in `ymind.exe`
  so Explorer and the taskbar show the yMind icon.

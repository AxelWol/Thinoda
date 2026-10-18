# Changelog

All notable changes to Thinoda are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries are generated automatically by
[release-please](https://github.com/googleapis/release-please) from
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) merged to
`main` (`feat:` → minor, `fix:` → patch, `feat!:`/`BREAKING CHANGE:` → major;
while the version is `0.x`, breaking changes bump the minor version). See
[Releasing](README.md#releasing) for details.

## [0.2.1](https://github.com/AxelWol/Thinoda/compare/v0.2.0...v0.2.1) (2026-10-18)


### Bug Fixes

* correct macOS package verification and pause WinGet publishing ([fa3a5c4](https://github.com/AxelWol/Thinoda/commit/fa3a5c48e2fdb7b3eb85be7667e986319dff8095))

## [0.2.0](https://github.com/AxelWol/Thinoda/compare/v0.1.3...v0.2.0) (2026-10-18)


### Features

* print the current map with preview and page settings ([dd972ba](https://github.com/AxelWol/Thinoda/commit/dd972bad93623b2d4d0db891e6cf3f0dc23b2047))

## [0.1.3](https://github.com/AxelWol/Thinoda/compare/v0.1.2...v0.1.3) (2026-10-17)


### Bug Fixes

* sign macOS bundles and verify release installers ([9a0b0ea](https://github.com/AxelWol/Thinoda/commit/9a0b0ea060874ad49737144cf53134de8f7cae24))
* sign macOS bundles and verify release installers ([74783fb](https://github.com/AxelWol/Thinoda/commit/74783fb17335c6c59f2a919cf5f8338a4ce1d733))

## [0.1.2](https://github.com/AxelWol/Thinoda/compare/v0.1.1...v0.1.2) (2026-10-17)


### Bug Fixes

* **about:** open contact email and project links ([c3fd177](https://github.com/AxelWol/Thinoda/commit/c3fd177c440c2822744a38c663e79809ca44e4a5))
* **about:** open contact email and project links from the About dialog ([10392be](https://github.com/AxelWol/Thinoda/commit/10392beacacaa7c32794783f1fbc523dfda110fe))

## [0.1.1](https://github.com/AxelWol/Thinoda/compare/v0.1.0...v0.1.1) (2026-10-17)


### Bug Fixes

* **linux:** declare all runtime libraries in the .deb package ([25ceea1](https://github.com/AxelWol/Thinoda/commit/25ceea1b006790a530bb2cf3e9c4155739e87513))
* **linux:** declare all runtime libraries in the .deb package ([87fb0db](https://github.com/AxelWol/Thinoda/commit/87fb0dbbcff446b34e97dc83ee41814bdbcbdb89))


### Documentation

* **readme:** Ubuntu installation instructions ([ecbc7a8](https://github.com/AxelWol/Thinoda/commit/ecbc7a88ebaccdf95971415c871fa2827b394fe1))

## [0.1.0](https://github.com/AxelWol/Thinoda/releases/tag/v0.1.0) (2026-10-17)

First public version of Thinoda — *Give your thoughts shape.*

### Features

* Thinoda logo for every platform: Windows `.ico` (16–256 px), macOS `.icns` (Apple icon grid, 16 pt–512 pt @2x), Linux hicolor PNGs, and in-app in the About dialog and User Guide.
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

* Windows: embed the application icon and version information in `thinoda.exe`
  so Explorer and the taskbar show the Thinoda icon.

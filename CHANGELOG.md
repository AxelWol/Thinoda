_Last updated: 2026-10-20 10:22:30 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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

## [0.6.0](https://github.com/AxelWol/Thinoda/compare/v0.5.4...v0.6.0) (2026-10-21)


### Features

* **ui:** remember sidebar family and formatting tab across restarts ([43f89a0](https://github.com/AxelWol/Thinoda/commit/43f89a062795160a58ed77427ea4c9807e29239f))


### Bug Fixes

* **app:** commit pending titles before save and close checks ([18266ca](https://github.com/AxelWol/Thinoda/commit/18266ca710df34524ac213e7f45aaf2acd2cdfa0))
* **app:** resume Save All after conflict and consent decisions ([a4d1e06](https://github.com/AxelWol/Thinoda/commit/a4d1e067cecfc2152cb6d097cca16721a8bb1661))
* **ui:** align file actions with the active document context ([3446ec5](https://github.com/AxelWol/Thinoda/commit/3446ec5e8d31aad912588a5a6cf43074101d2e07))
* **ui:** bound command palette and isolate its input ([87133a8](https://github.com/AxelWol/Thinoda/commit/87133a8a1d4317d1484f8ad7a32e0eba0d757123))
* **ui:** bound main-menu popups and reveal keyboard focus ([4b5da84](https://github.com/AxelWol/Thinoda/commit/4b5da84c2ef632a23ce3e95f99201d8da0f25921))
* **ui:** bound toolbar popups to the available viewport ([d4d532e](https://github.com/AxelWol/Thinoda/commit/d4d532ec447f852f9f35f9fa07befe7c33054ca7))
* **ui:** constrain context menus to the available viewport ([2716dc5](https://github.com/AxelWol/Thinoda/commit/2716dc5816ccb3af7ae7e54f7e29393c1d261d9a))
* **ui:** group main menu actions and preserve selection on dismissal ([8892499](https://github.com/AxelWol/Thinoda/commit/8892499df33d48f3fdb70693993f8af2c44fb54d))
* **ui:** keep Map inspector content aligned during resizing ([47238a6](https://github.com/AxelWol/Thinoda/commit/47238a612a4977207f80c8e7db7a75b2f1ab432a))
* **ui:** keep sheet navigation and bottom controls within the window ([1380111](https://github.com/AxelWol/Thinoda/commit/1380111a4fc04aacb0efcd4014ffeca36a6f3893))
* **ui:** keep shortcut reference reachable in short windows ([c349568](https://github.com/AxelWol/Thinoda/commit/c349568c45ed26974742e78e5b5631257041ff8c))
* **ui:** keep the Map insertion toolbox out of Outline view ([a549eb6](https://github.com/AxelWol/Thinoda/commit/a549eb62eba56ae381e88ee09a0f5f95dee11af3))
* **ui:** unify Notes and Hyperlink command availability ([4055773](https://github.com/AxelWol/Thinoda/commit/4055773131339d78a37e36dca32622b8dd663293))
* **ui:** verify startup safety and responsive sidebar lifecycle ([aecc6e3](https://github.com/AxelWol/Thinoda/commit/aecc6e3df6b58e8a1e8e8faafe0fbeb72beb8f51))


### Documentation

* record prioritized cross-platform UX roadmap ([f239ec7](https://github.com/AxelWol/Thinoda/commit/f239ec78a7c8cf9de9687e303cde099c6a0a5002))

## [0.5.4](https://github.com/AxelWol/Thinoda/compare/v0.5.3...v0.5.4) (2026-10-20)


### Bug Fixes

* contain exported node text and preserve PDF vectors ([#50](https://github.com/AxelWol/Thinoda/issues/50)) ([#52](https://github.com/AxelWol/Thinoda/issues/52)) ([24234ea](https://github.com/AxelWol/Thinoda/commit/24234eac89984fb99eced27bf3f1d4bb874f4efd))
* preserve topic size when dragging across hierarchy levels ([#54](https://github.com/AxelWol/Thinoda/issues/54)) ([3ca57a5](https://github.com/AxelWol/Thinoda/commit/3ca57a56b3290b4ec87c748978aef0504d83bcec)), closes [#47](https://github.com/AxelWol/Thinoda/issues/47)
* resolve export fonts on partial-font systems ([#50](https://github.com/AxelWol/Thinoda/issues/50)) ([#55](https://github.com/AxelWol/Thinoda/issues/55)) ([c91fc18](https://github.com/AxelWol/Thinoda/commit/c91fc184f1fb72a3d71a83609c3b572f3ffadab1))
* **web:** show hover pointers on clickable controls ([#48](https://github.com/AxelWol/Thinoda/issues/48)) ([b702b3f](https://github.com/AxelWol/Thinoda/commit/b702b3fb35bf1f9b5284863a9d92d465a62ece1a))
* **web:** show hover pointers on clickable controls ([#48](https://github.com/AxelWol/Thinoda/issues/48)) ([#51](https://github.com/AxelWol/Thinoda/issues/51)) ([5d3740e](https://github.com/AxelWol/Thinoda/commit/5d3740ed8f7b9cc4d2b6e4815c3ed4cba1c91570))

## [0.5.3](https://github.com/AxelWol/Thinoda/compare/v0.5.2...v0.5.3) (2026-10-19)


### Documentation

* add code signing policy ([cb5b5f0](https://github.com/AxelWol/Thinoda/commit/cb5b5f0ea6d8d86f6074f037348d861826487307))

## [0.5.2](https://github.com/AxelWol/Thinoda/compare/v0.5.1...v0.5.2) (2026-10-19)


### Documentation

* add privacy policy ([a0889c8](https://github.com/AxelWol/Thinoda/commit/a0889c8ec9348482a9172b0a518feb33643283a7))

## [0.5.1](https://github.com/AxelWol/Thinoda/compare/v0.5.0...v0.5.1) (2026-10-19)


### Bug Fixes

* **web:** add Linux GPU blocklist tip to startup troubleshooting ([e6919f3](https://github.com/AxelWol/Thinoda/commit/e6919f3fd6ab59232736383d5de6517bdac2505c))
* **web:** show WebGL troubleshooting steps when the renderer cannot start ([9740a98](https://github.com/AxelWol/Thinoda/commit/9740a9882005d564f236d972e2ec1ea7ebdb2b93))
* **web:** show WebGL troubleshooting steps when the renderer cannot start ([fdc3778](https://github.com/AxelWol/Thinoda/commit/fdc3778720871b1bd09ed4f96b908c3162a67a07))

## [0.5.0](https://github.com/AxelWol/Thinoda/compare/v0.4.0...v0.5.0) (2026-10-19)


### Features

* **web:** static WebAssembly web app on GitHub Pages ([6039784](https://github.com/AxelWol/Thinoda/commit/60397842e3c5584ccd154505a511b2e325960775))
* **web:** static WebAssembly web app published on GitLab Pages ([e806fc3](https://github.com/AxelWol/Thinoda/commit/e806fc31b010a7294b0470da14fda7be7872b1c6))


### Documentation

* refresh Markdown timestamps ([9c175f4](https://github.com/AxelWol/Thinoda/commit/9c175f424e640794d933ba67b7cbe909990e2bca))
* refresh Markdown timestamps ([bd57bed](https://github.com/AxelWol/Thinoda/commit/bd57bedf48aaa07f59c69df51c72efa948965cc6))

## [0.4.0](https://github.com/AxelWol/Thinoda/compare/v0.3.1...v0.4.0) (2026-10-18)


### Features

* add per-symbol colors and contrast guidance ([b6523a1](https://github.com/AxelWol/Thinoda/commit/b6523a117112a0d498aa3ec6185bb72a79d653bc))
* add symbol colors, targeted removal and contrast guidance ([7d89aa1](https://github.com/AxelWol/Thinoda/commit/7d89aa1bb0ff24b360ed347ffe346ac1228f9197))
* **app:** splash translations, ready timing log and single-file spinner ([7b076f8](https://github.com/AxelWol/Thinoda/commit/7b076f8761915c494b352d47eeb69dc2d1da1def))
* **app:** startup splash screen while launch state loads ([35d9682](https://github.com/AxelWol/Thinoda/commit/35d9682ec9798fccb37bc7a9d3bd565a6e6c74d7))
* **app:** startup splash screen while launch state loads ([56a0a47](https://github.com/AxelWol/Thinoda/commit/56a0a476a209e6b60726586f0aad7c5770a47f6e))
* create maps by double-clicking template cards ([ec98d5c](https://github.com/AxelWol/Thinoda/commit/ec98d5ca616b30eda6eebfe6f695d1f82fec5ff7))
* create maps by double-clicking template cards ([7fd023e](https://github.com/AxelWol/Thinoda/commit/7fd023eb74e03daa821c4aa9201f222b6c4fdcc4))
* **docs:** automate source-history Markdown timestamps ([babcc0e](https://github.com/AxelWol/Thinoda/commit/babcc0ec2541c20ea3bf4f9bde42d1042064071c))
* **docs:** maintain automatic source-history Markdown timestamps ([370143a](https://github.com/AxelWol/Thinoda/commit/370143a548383d343ba01a0a1e930a39f48c5920))
* embed offline legal documents in About ([03ba24c](https://github.com/AxelWol/Thinoda/commit/03ba24c60ae9daf019d383dfeccd834d1e3f18f0))
* expand application and installer localization ([7551b7d](https://github.com/AxelWol/Thinoda/commit/7551b7d45e7b5382c246495fc8f08b315716208a))
* expand UI and installer localization to ten locales ([4b68d3b](https://github.com/AxelWol/Thinoda/commit/4b68d3be5e98bbaad6ff7fa7daa411b6d4e003e3))
* format offline legal Markdown with 1.5x line height ([972516e](https://github.com/AxelWol/Thinoda/commit/972516e7eb24a225460e0da27606749c27d5f0b5))
* **io:** add guarded native MindManager map conversion ([41b300f](https://github.com/AxelWol/Thinoda/commit/41b300fdf64231d7cb6cc135fb1f8403bc4f32a2))
* **print:** render pages in Windows system print preview ([256a26a](https://github.com/AxelWol/Thinoda/commit/256a26a429144f6c076eb2c4feec37db81846413))
* **print:** support Windows native system preview ([b55821e](https://github.com/AxelWol/Thinoda/commit/b55821ee6e7122ad414d81f3a28bebd466413fd8))
* render legal Markdown with readable line spacing ([02ba1fc](https://github.com/AxelWol/Thinoda/commit/02ba1fcc93200cfb2a117885223dcf44712d012e))
* show complete legal documents offline in About ([e6b0df7](https://github.com/AxelWol/Thinoda/commit/e6b0df7e6d42de71d4724546d747dacfd004266d))
* store AI API keys securely ([42bfe50](https://github.com/AxelWol/Thinoda/commit/42bfe50a0916e71a4be6c00c0a643360a9d00231))


### Bug Fixes

* detect Brazilian locales with explicit script subtags ([ebb95ec](https://github.com/AxelWol/Thinoda/commit/ebb95ecdcd44c348bbdfe52fc0b5e9aafd637f80))
* expose formatted legal links to assistive technology ([a90b74e](https://github.com/AxelWol/Thinoda/commit/a90b74e7ddc8464f049003e969943344ec84037f))
* **print:** use standard C++20 coroutine support ([cafa6ec](https://github.com/AxelWol/Thinoda/commit/cafa6ec6ce1f7211124aeb42ab87399c09a990de))
* report unsupported legal document links ([83089e2](https://github.com/AxelWol/Thinoda/commit/83089e21c6156688ff5f108c9ddc6eb1d506207a))


### Documentation

* include Polish in localization backlog ([a110bb6](https://github.com/AxelWol/Thinoda/commit/a110bb6c7e4d5bb39bfe0db1c1d74574b2236872))
* **print:** group remaining native preview work with printing ([e8938be](https://github.com/AxelWol/Thinoda/commit/e8938be1fe08a5e8393be76454fae852e92e2f7a))
* **print:** record production system preview checks ([d324356](https://github.com/AxelWol/Thinoda/commit/d3243568171d18c4ea798378b3463718745d063b))
* **print:** separate completed entry from concurrent backlog additions ([eae180c](https://github.com/AxelWol/Thinoda/commit/eae180c0fa8c52a56ba25d16accfc997c412936a))
* refresh legal viewer source timestamps ([6398b52](https://github.com/AxelWol/Thinoda/commit/6398b5210dd503499c8c5d9c5e6a36e74a717900))
* refresh merged backlog source timestamp ([59fb8bb](https://github.com/AxelWol/Thinoda/commit/59fb8bb3f665ac8896b9f8be267781cd235705de))
* stamp BACKLOG.md ([844531d](https://github.com/AxelWol/Thinoda/commit/844531de32411fc038c785d7c8500f48eedff666))
* stamp BACKLOG.md after merging main ([926f39c](https://github.com/AxelWol/Thinoda/commit/926f39c2a728e57260f02e9e591a7474ce4c3e32))

## [0.3.1](https://github.com/AxelWol/Thinoda/compare/v0.3.0...v0.3.1) (2026-10-18)


### Bug Fixes

* label German installer shortcut and maintenance controls ([557db7f](https://github.com/AxelWol/Thinoda/commit/557db7fd7c8d28ce2ec99689b11997eea7181117))

## [0.3.0](https://github.com/AxelWol/Thinoda/compare/v0.2.1...v0.3.0) (2026-10-18)


### Features

* use native printer dialogs and driver settings ([47a3e2b](https://github.com/AxelWol/Thinoda/commit/47a3e2b172f41841677fd73d691bd9444c900d4a))

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

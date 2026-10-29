_Last updated: 2026-10-28 13:12:41 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

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

## Unreleased

### Added

- Background web-shell update checks with distinct ready and next-launch
  installed notices, immutable build identity and no forced document reload.
- Background native stable-release checks with current/new versions, release
  notes/changelog and download links (no automatic native installation).
- Dismissible update notices in all ten languages and light/dark mode,
  bounded networking/backoff, lifecycle tests and updated privacy documentation.

## [0.17.0](https://github.com/AxelWol/Thinoda/compare/v0.16.0...v0.17.0) (2026-10-28)


### Features

* **notes:** add safe Markdown authoring and opt-in preview ([bfd1ee5](https://github.com/AxelWol/Thinoda/commit/bfd1ee5d764dac8340f366703481dba0fc071335))
* **notes:** copy bounded Markdown projections as plain text ([160bade](https://github.com/AxelWol/Thinoda/commit/160bade7abb27cf38d015f67cadeab99883e6ad2))
* **notes:** copy bounded Markdown projections as plain text ([#94](https://github.com/AxelWol/Thinoda/issues/94)) ([52e842a](https://github.com/AxelWol/Thinoda/commit/52e842abe0c9c47b1ac8e69b412706a7ffdddec4))
* **notes:** make list formatting reversible and selection-aware ([c124681](https://github.com/AxelWol/Thinoda/commit/c124681070c1cd215b87c53b3843e7ebf19bfd98))
* **notes:** make list formatting reversible and selection-aware ([#93](https://github.com/AxelWol/Thinoda/issues/93)) ([0b3ed1b](https://github.com/AxelWol/Thinoda/commit/0b3ed1b0164747b0f7d9ed9b021c7a1f24d3c061))
* **search:** index visible Markdown note text ([#95](https://github.com/AxelWol/Thinoda/issues/95)) ([b855e3c](https://github.com/AxelWol/Thinoda/commit/b855e3ce9c6692276463a64d9f540772853e88d3))


### Bug Fixes

* **ci:** update release manifests and lockfiles atomically ([0eab22f](https://github.com/AxelWol/Thinoda/commit/0eab22fcc513adbbd70c9d286c265126943cb048))
* **notes:** distinguish formatting actions and make emphasis reversible ([37860f7](https://github.com/AxelWol/Thinoda/commit/37860f7616159eea41abe4cd0d2ed3f19fa14939))
* restore locked releases and add safe Markdown notes tools ([#91](https://github.com/AxelWol/Thinoda/issues/91)) ([e340ee3](https://github.com/AxelWol/Thinoda/commit/e340ee3c17bf538518e93f19ec0efc40ef169dd8))

## [0.16.0](https://github.com/AxelWol/Thinoda/compare/v0.15.0...v0.16.0) (2026-10-27)


### Features

* add portable local image authoring and rendering ([#87](https://github.com/AxelWol/Thinoda/issues/87)) ([ffd2217](https://github.com/AxelWol/Thinoda/commit/ffd2217757bfd587cbf35ca3b975c560c7a7969c))
* extend portable resources and presentation exports ([#90](https://github.com/AxelWol/Thinoda/issues/90)) ([e909385](https://github.com/AxelWol/Thinoda/commit/e909385b83c616e8a236907eb802eb1c64346407))
* portable attachments and image drag/drop ([#88](https://github.com/AxelWol/Thinoda/issues/88)) ([1fb939e](https://github.com/AxelWol/Thinoda/commit/1fb939e12ecaa417a6e6506a3ac030ec4076269e))

## [0.15.0](https://github.com/AxelWol/Thinoda/compare/v0.14.0...v0.15.0) (2026-10-27)


### Features

* add live Pitch transitions and topic builds ([0e392ca](https://github.com/AxelWol/Thinoda/commit/0e392ca1654a1275207b187821037e4ede31af0e))
* **core:** add bounded portable resource storage foundation ([aa4acde](https://github.com/AxelWol/Thinoda/commit/aa4acde0c61d76ebcd2d77117e0ef33f39535fbb))
* shared dialogs, calendar inputs, live Pitch and native AI providers ([#85](https://github.com/AxelWol/Thinoda/issues/85)) ([cbbcf6f](https://github.com/AxelWol/Thinoda/commit/cbbcf6f4a981a4415e43fafd569d1c8b4ce1afeb))
* unify dialogs, add date pickers and native AI provider selection ([09fe25d](https://github.com/AxelWol/Thinoda/commit/09fe25d2c1abac120cccd1af076e95b9399ce0bc))


### Documentation

* record verified v0.14.0 release and deployment ([a19c3d1](https://github.com/AxelWol/Thinoda/commit/a19c3d128947ca7634d25641f403bd0d1e50ddf5))

## [0.14.0](https://github.com/AxelWol/Thinoda/compare/v0.13.0...v0.14.0) (2026-10-26)


### Features

* flat AI settings, embedding discovery, compact ribbon by default (UR1, UR2, UR5) ([#82](https://github.com/AxelWol/Thinoda/issues/82)) ([c84e4d9](https://github.com/AxelWol/Thinoda/commit/c84e4d9a1bb924fe35c3597eb72c6023a1a9f6eb))
* topic numbering, Material Symbols icons and AI reply language ([#81](https://github.com/AxelWol/Thinoda/issues/81)) ([fa9b48b](https://github.com/AxelWol/Thinoda/commit/fa9b48b53965cc32e817bf6c8ce8f445187a037c))

## [0.13.0](https://github.com/AxelWol/Thinoda/compare/v0.12.1...v0.13.0) (2026-10-26)


### Features

* add faceted topic navigation and all-notes review ([2b93169](https://github.com/AxelWol/Thinoda/commit/2b93169a60ecee31d37af88c271dd90acbec512c))
* derive nested checklist progress and subtree completion ([8dc76be](https://github.com/AxelWol/Thinoda/commit/8dc76bed57db444e57342702616f5c6af1c2b125))
* improve topic navigation and nested checklist progress ([bc1cc4d](https://github.com/AxelWol/Thinoda/commit/bc1cc4da344c42d9fed9b858d2527ed00c90c077))

## [0.12.1](https://github.com/AxelWol/Thinoda/compare/v0.12.0...v0.12.1) (2026-10-25)


### Bug Fixes

* make native release Pitch acceptance keyboard-driven ([538dc0c](https://github.com/AxelWol/Thinoda/commit/538dc0c7a8b7f4ddea05382176868343cf9d580b))
* stabilize native release Pitch acceptance ([#76](https://github.com/AxelWol/Thinoda/issues/76)) ([c5033f5](https://github.com/AxelWol/Thinoda/commit/c5033f526b9b519c9cde3e4c1606e8bb7ca8874c))

## [0.12.0](https://github.com/AxelWol/Thinoda/compare/v0.11.0...v0.12.0) (2026-10-25)


### Features

* **ai:** add bounded cancellable embeddings transport ([f086366](https://github.com/AxelWol/Thinoda/commit/f0863664ae1628214883b61d733e2d2f335e719d))
* **ai:** add editable saved prompt library ([9640144](https://github.com/AxelWol/Thinoda/commit/964014472ac672036cd5a07477187664f5f3e57f))
* **ai:** add explicit semantic search and cluster previews ([6135a41](https://github.com/AxelWol/Thinoda/commit/6135a41142144e4b71f73a659c55ff35721985f0))
* **ai:** add reviewable guided map transformations ([86b01ba](https://github.com/AxelWol/Thinoda/commit/86b01ba9095bbd8998183e48e9a26060638a81da))
* **ai:** add reviewable Pitch outline suggestions ([7570171](https://github.com/AxelWol/Thinoda/commit/757017177a2515825428c33751b529bb70185fbe))
* **ai:** add scoped session chat and response modes ([20fb3ac](https://github.com/AxelWol/Thinoda/commit/20fb3ac5a4df47b5cc2f479425d2e7fafb5cc29c))
* **ai:** preview and cancel streaming map requests ([dd6b2ce](https://github.com/AxelWol/Thinoda/commit/dd6b2ced84e529e8780adb14333b3d1d1854aa9a))
* **ai:** require review before applying generated results ([ce041ce](https://github.com/AxelWol/Thinoda/commit/ce041ce5b0abff1d3db93c82ed759e4d2deb29ba))
* **app:** add staged Pitch editing and read-only presentation playback ([4328032](https://github.com/AxelWol/Thinoda/commit/4328032e248ff64e48f746cd283365d15f051bfc))
* **app:** add undoable Zone editor and canvas gestures ([e5ba0fd](https://github.com/AxelWol/Thinoda/commit/e5ba0fd67ffafabb0bffff41bf4f1ab85a4282fe))
* **canvas:** expose visible topic-group framing for Zones ([a049a6c](https://github.com/AxelWol/Thinoda/commit/a049a6ca7f593677e5812303630bdcbdfe273bb4))
* **core:** add persisted Zones and presentation model ([0e68707](https://github.com/AxelWol/Thinoda/commit/0e687075f5bdec0afd8bcf0789b880259a62fa5d))
* **core:** integrate Zones and presentation state ([491f5f0](https://github.com/AxelWol/Thinoda/commit/491f5f0d6be5107a125c2ccfb076845f6d93c919))
* **io:** render saved presentation slides and exports ([c35ce81](https://github.com/AxelWol/Thinoda/commit/c35ce81007c1bfdc3f640843b150fde5368cc4ba))


### Bug Fixes

* **ai:** adapt embeddings cleanup to named wasm timeout ([792a5a9](https://github.com/AxelWol/Thinoda/commit/792a5a970534308e6a15a8bdfab1d62e1f5897e7))
* **app:** retain planning drafts when document windows return home ([432c789](https://github.com/AxelWol/Thinoda/commit/432c7894829984bb7cb338c4603ac97d5b91ce37))
* refresh notices for streaming AI dependencies ([feadef2](https://github.com/AxelWol/Thinoda/commit/feadef2047b6ebf32da7a64db1a78e31fa5c0428))
* **ui:** keep floating map tools below feature windows ([a808f7a](https://github.com/AxelWol/Thinoda/commit/a808f7aeaffd02e3b54344067a357f5f1bdaa09e))


### Documentation

* record real browser feature acceptance ([3680427](https://github.com/AxelWol/Thinoda/commit/36804273058dd31c10f8fcbea53116702c3f5e89))
* record release integration authorization and remaining gates ([33ab6a0](https://github.com/AxelWol/Thinoda/commit/33ab6a07a2621970d839d1ae99308cb4fe83de44))

## [0.11.0](https://github.com/AxelWol/Thinoda/compare/v0.10.1...v0.11.0) (2026-10-25)


### Features

* add optional compact ribbon density ([421fb79](https://github.com/AxelWol/Thinoda/commit/421fb79566390ea76c78c045b0d8e14d947b89b9))
* add persistent ribbon pinning and unclipped document capsules ([0c9c1e2](https://github.com/AxelWol/Thinoda/commit/0c9c1e2d15ba1382e9aee4775ee7dbc75cde8b68))
* release ribbon redesign, compact mode and integrated map tabs ([148dfcd](https://github.com/AxelWol/Thinoda/commit/148dfcd4e7eace2de5954308a5f04d23e423e007))


### Bug Fixes

* keep sheet tabs clear of scroll controls on narrow windows ([78541a0](https://github.com/AxelWol/Thinoda/commit/78541a0b22041f177ceaffffe1f27136adfe1b3c))

## [0.10.1](https://github.com/AxelWol/Thinoda/compare/v0.10.0...v0.10.1) (2026-10-24)


### Bug Fixes

* bind close-save continuation to document identity ([2433751](https://github.com/AxelWol/Thinoda/commit/24337517c67370fa567351e7a9f544e84c6f3fa8))
* bind lossy export consent to its source document ([aa3b927](https://github.com/AxelWol/Thinoda/commit/aa3b927e2c6cd081236d3362731c13840bbc33a4))
* bind sheet rename decisions to their draft owner ([6074dca](https://github.com/AxelWol/Thinoda/commit/6074dcad53809089f7e4b5ebe03494eb116ac3b8))
* discard stale inspector numeric drafts ([6b9f13e](https://github.com/AxelWol/Thinoda/commit/6b9f13e309d8b4b2b5e46bcdfcba107add560ef0))
* explicitly release native recovery locks ([6cce6cb](https://github.com/AxelWol/Thinoda/commit/6cce6cb2ad3e102d08ca5777118cbc8057e2fff9))
* guard theme and quick style library touch actions ([738ef13](https://github.com/AxelWol/Thinoda/commit/738ef13c398d6746e45fc6eaff7e32039cadd7f5))
* persist recovery handoffs before deleting original snapshots ([496df0f](https://github.com/AxelWol/Thinoda/commit/496df0f5b413902fc56c7f7699a55113e7281879))
* preserve conflict checks through save path aliases ([586112c](https://github.com/AxelWol/Thinoda/commit/586112ca6812198568fffe73e082dabb8a728284))
* preserve label drafts through rejected touch actions ([e233bbf](https://github.com/AxelWol/Thinoda/commit/e233bbf918ef232546569e09bba9affc358b979d))
* preserve recovery ownership across suspension ([093ba19](https://github.com/AxelWol/Thinoda/commit/093ba191e595e7027c4aa949589aa9dadc9ca10a))
* preserve recovery sessions after rejected or failed actions ([c958c57](https://github.com/AxelWol/Thinoda/commit/c958c579e16dd2ac6e72a83567739c720fa265cf))
* preserve screenshot drafts across interrupted gestures ([7d5a96b](https://github.com/AxelWol/Thinoda/commit/7d5a96be7c18e52bd967433d277e9220e35c5715))
* reject canceled touch confirmation choices ([5dd542a](https://github.com/AxelWol/Thinoda/commit/5dd542a276f51352921e955896a0cd9bd2aecd76))
* reject canceled touch print preview decisions ([186634d](https://github.com/AxelWol/Thinoda/commit/186634dc1511146fa4510d6232c9f2d2b84cdcf5))
* require verified identity or fresh overwrite consent ([2c24824](https://github.com/AxelWol/Thinoda/commit/2c248243f5f428802b71960c8ffd2ffcae820828))
* restore theme drafts across canceled pointer gestures ([43a243e](https://github.com/AxelWol/Thinoda/commit/43a243e303bd844f7a1379a26de049ee803dc9d4))
* roll back interrupted print page setup gestures ([fee4171](https://github.com/AxelWol/Thinoda/commit/fee4171dc369bfb76d7862b3cb410f5bd1763a67))
* **touch:** defer numeric inspector drags ([a936b8a](https://github.com/AxelWol/Thinoda/commit/a936b8a8e1a719a2e519143b83c5e725d8d9d8b9))
* **touch:** discard canceled slider drags ([37a2236](https://github.com/AxelWol/Thinoda/commit/37a2236e8dccd428f4966887756ddfd6a78862d4))
* **touch:** guard custom theme editor actions ([35acdeb](https://github.com/AxelWol/Thinoda/commit/35acdeb200816bc42d31fb3071180ab380c15787))
* **touch:** guard map inspector theme commits ([0116202](https://github.com/AxelWol/Thinoda/commit/01162027f9959e50da9e5e6c30636bc474da12b7))
* **touch:** guard style inspector releases ([5ec673e](https://github.com/AxelWol/Thinoda/commit/5ec673e08f6578c634277629e77e6672391258b7))
* verify release ancestry and pin packaged source commits ([f85951d](https://github.com/AxelWol/Thinoda/commit/f85951dd77f55fae0dcbc6f769dae6bbd11ae270))
* **web:** preserve browser zoom accessibility ([0e1d403](https://github.com/AxelWol/Thinoda/commit/0e1d4032b5de4590cd260d1c6a7c43806b0cdbd2))


### Documentation

* consolidate security and stability sprint ([fdeb1a6](https://github.com/AxelWol/Thinoda/commit/fdeb1a6d8683f91d55d6dff1b58cd50deacbe4f0))
* record candidate acceptance evidence and access blockers ([a228e05](https://github.com/AxelWol/Thinoda/commit/a228e0534bcfa79d9d82ff44c4dc08df8c0c3f51))

## [0.10.0](https://github.com/AxelWol/Thinoda/compare/v0.9.0...v0.10.0) (2026-10-23)


### Features

* **ui:** add scrollable document tabs ([db428ae](https://github.com/AxelWol/Thinoda/commit/db428ae85c7b598e12c944cd3cce19da4b628616))


### Bug Fixes

* **a11y:** preserve keyboard focus on color swatches ([13a529f](https://github.com/AxelWol/Thinoda/commit/13a529ff1c732dbd534b315d979c4701cdf40fa8))
* **a11y:** show focus on Settings tabs and switches ([b8c97d4](https://github.com/AxelWol/Thinoda/commit/b8c97d4c3e88d7cd5f29a3de2cf5af7d0f1cbfce))
* **canvas:** support safe touch pan and simultaneous pinch ([d6e7b21](https://github.com/AxelWol/Thinoda/commit/d6e7b21c492dae81c52bf17d37c4c14679a82d49))
* **editing:** protect label drafts from IME candidate keys ([ce31e4b](https://github.com/AxelWol/Thinoda/commit/ce31e4b38cce5f5fe97fc5dace47469ef1be3101))
* **editing:** protect sheet rename input and keyboard navigation ([97e280e](https://github.com/AxelWol/Thinoda/commit/97e280edf92dbf41b70840eacd3743a5f8b18cf4))
* **gestures:** guard preview pickers against rejected touch activation ([e81af6f](https://github.com/AxelWol/Thinoda/commit/e81af6f966995fe34a3af412ff9c1b67acd38d21))
* **gestures:** guard Symbols inspector edits on rejected releases ([2112961](https://github.com/AxelWol/Thinoda/commit/211296110f4c325fb3b6431e1dd14d2bce23cae0))
* **gestures:** prevent Outline and delayed release click-through ([afa8517](https://github.com/AxelWol/Thinoda/commit/afa85170590456a6875c933db4288c529cbcd313))
* **gestures:** protect Settings choices from rejected touch releases ([f16cd91](https://github.com/AxelWol/Thinoda/commit/f16cd91b97c1d765dadb8afeb7f78d2af29c8afa))
* **gestures:** protect sheet controls from rejected touch releases ([5c6d252](https://github.com/AxelWol/Thinoda/commit/5c6d25234cf1b7dfb5ba67fa136573a5906cb5e5))
* **gestures:** suppress context menus during multitouch holds ([7c1cfc2](https://github.com/AxelWol/Thinoda/commit/7c1cfc21538755f329e53e78ae12ee585b25fb48))
* **navigation:** add clipped sheet scrolling controls ([7c0ee62](https://github.com/AxelWol/Thinoda/commit/7c0ee6249e04bf59807266d637ad68003f486f0d))


### Documentation

* record verified six-target 0.9.0 publication ([4ec6618](https://github.com/AxelWol/Thinoda/commit/4ec661842310f74628c0f6822e7a31133dc5bb9d))

## [0.9.0](https://github.com/AxelWol/Thinoda/compare/v0.8.0...v0.9.0) (2026-10-23)


### Features

* reliable editing, installable PWA and six-target releases ([ac3442b](https://github.com/AxelWol/Thinoda/commit/ac3442b9b2fa4d8f36718064a1260ceab8e10e65))

## [0.8.0](https://github.com/AxelWol/Thinoda/compare/v0.7.0...v0.8.0) (2026-10-22)


### Features

* **canvas:** add opt-in visual alignment guides ([b4eb525](https://github.com/AxelWol/Thinoda/commit/b4eb52525a00873e5d6704fbd370b922801d0be6))
* **editor:** add preferred native CJK fallback font ([a8bcbff](https://github.com/AxelWol/Thinoda/commit/a8bcbff406f858dc583186665240eadb3091f830))
* **editor:** CJK fallback, symbol menu safety and alignment guides ([#64](https://github.com/AxelWol/Thinoda/issues/64)) ([cfb3b03](https://github.com/AxelWol/Thinoda/commit/cfb3b03e3897da51148e69b732d5a7b39965f30e))


### Bug Fixes

* **menus:** derive shortcut hints from shared command bindings ([4b792f7](https://github.com/AxelWol/Thinoda/commit/4b792f700e543897fdd52acd63883fc5b24b990e))
* **shortcuts:** derive contextual hints and aliases from dispatch ([8c2dfd2](https://github.com/AxelWol/Thinoda/commit/8c2dfd2e4c2f1a8da7b5cda3f3f781dd8816019d))
* **shortcuts:** omit intercepted bindings from command hints ([66785fb](https://github.com/AxelWol/Thinoda/commit/66785fb9054526f5b53687bc09af1d0c4e10b67c))
* **shortcuts:** scope live hints to Map and Outline ([813d51f](https://github.com/AxelWol/Thinoda/commit/813d51f5871496964e1c7d3c53906b0c2aca37c0))
* **shortcuts:** share global bindings with command catalogs ([d207987](https://github.com/AxelWol/Thinoda/commit/d20798700aaa4287a3812fa11dc059c92fd19419))
* **ui:** enable native mixed-script font fallback ([7d45d41](https://github.com/AxelWol/Thinoda/commit/7d45d41b98346f5cb9a842640fd0e4d4fb436788))
* **ui:** improve search, Outline and inspector keyboard safety ([#62](https://github.com/AxelWol/Thinoda/issues/62)) ([f909c21](https://github.com/AxelWol/Thinoda/commit/f909c2191f7e03007372401d70708b7c1c9c8cdb))
* **ui:** keep nested symbol color menus usable ([df8971f](https://github.com/AxelWol/Thinoda/commit/df8971f7998e9d7c6df0b8c37e3ba0cb7e3553a6))
* **ui:** make Zen Escape reliable and expand search controls ([196e86d](https://github.com/AxelWol/Thinoda/commit/196e86dfcf3b7ecb2300f23f453df06a860605e3))
* **ui:** reveal inspector keyboard focus within its scroll region ([f5cdde0](https://github.com/AxelWol/Thinoda/commit/f5cdde0ef77bca9699fda8798e14e2e606361345))
* **ui:** scope Outline keyboard edits to topic focus ([e1bcc27](https://github.com/AxelWol/Thinoda/commit/e1bcc271e5122d166923eb8d1c79d68cf331385d))
* **ui:** unify shortcuts and improve Zen and search ([#65](https://github.com/AxelWol/Thinoda/issues/65)) ([679c85a](https://github.com/AxelWol/Thinoda/commit/679c85ad902de22431f8c0f7cd5beee6ca0919f3))


### Documentation

* confirm 0.7.0 assets and download checksums ([a215a44](https://github.com/AxelWol/Thinoda/commit/a215a44b07d435090cf63e70d6354607a97838f5))
* track 0.7.0 publication and post-release UX work ([f2bf5f6](https://github.com/AxelWol/Thinoda/commit/f2bf5f63b1e94206838fdc52201a4c153b4f323d))

## [0.7.0](https://github.com/AxelWol/Thinoda/compare/v0.6.0...v0.7.0) (2026-10-21)


### Features

* improve responsive editing and harden macOS release packaging ([6e2fcfa](https://github.com/AxelWol/Thinoda/commit/6e2fcfa9746e4a5155ad49f7230c579c77f7fe51))

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

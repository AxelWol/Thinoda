_Last updated: 2026-10-28 06:49:28 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Contributing to Thinoda

Thank you for your interest in Thinoda! Bug reports, ideas, code, translations,
templates and documentation are all welcome. This guide explains how to set up a
development environment, what we expect from a change, and how it gets from a
pull request into a release.

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Contents

- [Before you start](#before-you-start)
- [Reporting bugs and requesting features](#reporting-bugs-and-requesting-features)
- [Development setup](#development-setup)
- [Project layout](#project-layout)
- [Building and running](#building-and-running)
- [Checks and tests](#checks-and-tests)
- [Translations](#translations)
- [Documentation](#documentation)
- [Dependencies and licensing](#dependencies-and-licensing)
- [Commits and pull requests](#commits-and-pull-requests)
- [Releases](#releases)
- [Licensing of contributions](#licensing-of-contributions)

## Before you start

- **Security issues are never reported publicly.** Follow [SECURITY.md](SECURITY.md)
  instead of opening an issue or pull request.
- **Original work only.** Do not submit code, icons, images, templates, themes
  or other assets copied or reverse-engineered from other mind-mapping products
  (commercial or open source). Thinoda is compatible with formats such as XMind
  and MindManager, but its implementation and content are its own.
- **Privacy matters.** Never attach maps, screenshots or logs that contain
  confidential or personal data, and do not add code that sends user data
  anywhere the user did not explicitly configure (see [PRIVACY.md](PRIVACY.md)).
- **Discuss larger changes first.** For new features, new file formats, new
  dependencies or architectural changes, open an issue before investing a lot of
  time. Check [docs/PLAN.md](docs/PLAN.md) and [docs/BACKLOG.md](docs/BACKLOG.md)
  to see what is already planned.

## Reporting bugs and requesting features

1. Search the [existing issues](https://github.com/AxelWol/Thinoda/issues) first.
   In the app, **Help > Send Feedback** opens the same issue chooser.
2. Open a new issue with a clear title and include:
   - the Thinoda version (**Help > About**);
   - your operating system, its version and architecture (for example
     Windows 11 ARM64, macOS 15 Apple silicon, Ubuntu 24.04 x64), or the browser
     for the web app;
   - steps to reproduce, the expected result and the actual result;
   - screenshots or a minimal sample map, if possible (no confidential data).
3. For import/export problems, say which application and version created the
   file and attach the smallest file that shows the problem.

## Development setup

| Requirement | Notes |
|---|---|
| Rust stable, at least the `rust-version` in [Cargo.toml](Cargo.toml) (currently 1.89) | Edition 2024. Install via [rustup](https://rustup.rs) with the `rustfmt` and `clippy` components. |
| C/C++ compiler | Needed for the native printing bridge: MSVC on Windows, Xcode/Clang with the AppKit SDK on macOS, GCC or Clang on Linux. |
| Python 3 | Used by the helper scripts in `.github/scripts` (CI uses 3.13). |
| Linux system libraries | `pkg-config libgtk-3-dev libxkbcommon-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libwayland-dev` (CI also installs `libssl-dev` and `xvfb`). |
| Web app (optional) | `rustup target add wasm32-unknown-unknown` and [Trunk](https://trunkrs.dev). |
| Fuzzing (optional) | A nightly toolchain and `cargo install cargo-fuzz --locked`. |

Then fork the repository, clone your fork, and create a feature branch from
`main`:

```sh
git clone https://github.com/<you>/Thinoda.git
cd Thinoda
git switch -c feat/my-change
```

## Project layout

Thinoda is a Cargo workspace. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
for the full picture and the key design decisions.

| Path | Responsibility |
|---|---|
| `crates/thinoda-core` | Document model, transactional edits and undo, layout for all structures, themes, templates, search (no egui) |
| `crates/thinoda-io` | Native format, XMind, FreeMind, OPML, Markdown, MindManager import/export, image/PDF/Office export (no egui) |
| `crates/thinoda-platform` | Settings, atomic writes, crash recovery, cloud folders, OS integration, native printing bridge, browser VFS on wasm |
| `crates/thinoda-ai` | OpenAI-compatible client for the optional AI assistant, with a mock server for tests |
| `crates/thinoda-ui` | Reusable design system: tokens, widgets, dialogs, icons, fonts |
| `crates/thinoda-canvas` | Interactive mind map widget: hit testing, selection, drag and drop, inline editing |
| `crates/thinoda-app` | The eframe application: commands (`Cmd`), menus, panels, dialogs, i18n, offline help, CLI |
| `crates/thinoda-web` | WebAssembly entry point for the browser build ([docs/WEB.md](docs/WEB.md)) |
| `xtask` | `cargo xtask ci`, `icons` and `package` |
| `fuzz` | Separate workspace with `cargo-fuzz` targets for the importers ([fuzz/README.md](fuzz/README.md)) |
| `.github/scripts` | Packaging, release, notices and documentation helpers, with their own Python tests |

A few conventions worth knowing before you write code:

- Keep `thinoda-core`, `thinoda-io`, `thinoda-platform` and `thinoda-ai` free
  of UI dependencies.
- Every menu item, shortcut and command-palette entry maps to one `Cmd`, and
  end-to-end tests drive the app through `ThinodaApp::exec` or real key presses.
- A user action that changes a document should be **one undo step**.
- Desktop and web share one code base; platform differences live behind
  `thinoda-platform::fs`, `thinoda-app::files` and `cfg(target_arch = "wasm32")`.
  Changes must keep the web build compiling.
- Long-running work (AI requests, print rendering) runs off the UI thread.

## Building and running

```sh
cargo run -p thinoda-app                 # start the desktop app
thinoda --convert map.xmind map.md       # CLI conversion (after building)
cd crates/thinoda-web && trunk serve     # web app at a local URL
cargo xtask package                      # installers in target/packages
```

Packaging commands for each platform are listed in the
[README](README.md#packages). You normally do not need them for a contribution.

## Checks and tests

Run the same checks as CI before opening a pull request:

```sh
cargo xtask ci        # fmt --check, clippy -D warnings, workspace tests, benchmark example test
python .github/scripts/third_party_notices.py --check
```

Depending on what you touched, also run:

| If you changed… | Run |
|---|---|
| Anything used by the web app | `cargo clippy --target wasm32-unknown-unknown -p thinoda-web -p thinoda-app -p thinoda-platform -p thinoda-ai -p thinoda-io -- -D warnings` |
| `.github/scripts` or packaging | `python -m unittest discover -s .github/scripts -p "test_*.py"` |
| Importers in `thinoda-io` | `cargo test -p thinoda-io --test import_mutations` and the fuzz smoke runs from [fuzz/README.md](fuzz/README.md) |
| The fuzz harness | `cargo fmt --manifest-path fuzz/Cargo.toml --all -- --check` and `cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings` |
| Layout performance | `cargo run --release -p thinoda-core --example layout_benchmark -- --topics 10000 --samples 30` |
| Map Shot / GPU rendering | `cargo test -p thinoda-app --features gpu-tests --lib map_shot_gpu_readback` (needs a GPU adapter) |
| Native printing | See [docs/NATIVE-PRINTING.md](docs/NATIVE-PRINTING.md); tests never submit real print jobs |

Test expectations:

- **Every behaviour change needs tests.** Put model and format logic under unit
  or integration tests in the relevant crate, and user-facing behaviour under
  `crates/thinoda-app/tests` (headless `egui_kittest` end-to-end tests).
- Bug fixes should come with a test that fails without the fix.
- Importers must reject invalid input gracefully. Every accepted import must
  validate and round-trip through native storage.
- The live AI test is skipped when no local server answers. Set
  `THINODA_AI_SKIP_LIVE=1` to skip it explicitly, or `THINODA_AI_TEST_URL` to
  point it at your own OpenAI-compatible server.
- CI runs on Windows x64, Windows ARM64, macOS and Ubuntu. If you cannot test a
  platform-specific change locally, say so in the pull request.

## Translations

The UI is available in English, German, Spanish, French, Danish, Dutch,
Portuguese (Portugal and Brazil), Italian and Polish. English strings in the
code are the translation keys, looked up with `i18n::t("…")`.

When you add or change a user-visible string, add it to **every** catalog in
`crates/thinoda-app/src`:

| Catalog | Format |
|---|---|
| `i18n.rs` (`DE`) | `("English", "Deutsch")` pairs |
| `locales.txt` | One line per key: `English|Español|Français|Dansk|Nederlands|Português (Portugal)|Italiano|Polski` |
| `pt-BR.txt` | Only explicit Brazilian Portuguese overrides: `English|Português (Brasil)`; other keys reuse the Portugal text |

The catalogs are validated when they load and by tests: rows must have every
column, values must not be empty or contain `|`, and keys must be unique. Lines
starting with `#` are comments. Keep product and format names (XMind, Map Shot,
`.thinoda`, …) unchanged, and follow the terminology notes at the top of
`locales.txt`. Corrections by native speakers are very welcome; small
translation fixes can be sent without an issue.

Installer translations live in `assets/installer/*.nsh`; see
`.github/scripts/test_installer_languages.py` for what is checked.

## Documentation

- Update the [README](README.md), the offline help (`crates/thinoda-app/src/help.rs`,
  including its translations) and the relevant file in [docs/](docs) whenever
  user-visible behaviour changes.
- Describe what Thinoda actually does. Avoid claims the code or tests do not
  back up (for example about OS dialogs, printers or third-party applications).
- Do not edit the `_Last updated: …_ <!-- docs-stamp -->` line at the top of
  Markdown files by hand. CI adds and refreshes it on `main` after your change
  is merged. To preview it locally, commit first and then run
  `python .github/scripts/docs_stamp.py` (it needs full Git history; use
  `git fetch --unshallow` in a shallow clone). Details are in
  [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md#documentation-timestamps).
- Do not change the bundled legal documents (`LICENSE.md`,
  `THIRD_PARTY_NOTICES.md`, `IMPRESSUM.md`, `PRIVACY.md`) as part of an
  unrelated change. They are shown verbatim in the app.

## Dependencies and licensing

- Prefer the existing workspace dependencies in [Cargo.toml](Cargo.toml) and
  keep new ones to a minimum. Explain in the pull request why a new crate is
  needed.
- Every runtime dependency must be under a licence compatible with the
  EUPL-1.2. Where a crate offers a choice, Thinoda uses the permissive option.
- After adding, removing or updating dependencies, regenerate the notices and
  commit the result:

  ```sh
  python .github/scripts/third_party_notices.py
  ```

  CI fails when `THIRD_PARTY_NOTICES.md` does not match `Cargo.lock`.
- Development- and fuzz-only tools are not redistributed and do not belong in
  the notices.

## Commits and pull requests

Commit messages and **pull request titles** follow
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). They drive
automatic versioning and the [CHANGELOG](CHANGELOG.md) via release-please, so
pick the type carefully:

| Type | Use for | In the changelog |
|---|---|---|
| `feat` | New user-visible functionality | Features (minor bump) |
| `fix` | Bug fixes | Bug Fixes (patch bump) |
| `perf` | Performance improvements | Performance Improvements |
| `docs` | Documentation only | Documentation |
| `deps` | Dependency updates | Dependencies |
| `revert` | Reverting a previous commit | Reverts |
| `refactor`, `test`, `build`, `ci`, `chore`, `style` | Internal changes | Hidden |

Use an optional scope for the affected area, for example
`fix(io): keep notes on XMind export` or `feat(canvas): drag topics between sheets`.
Mark breaking changes with `!` (`feat!: …`) or a `BREAKING CHANGE:` footer. While
Thinoda is `0.x`, breaking changes bump the minor version.

When you open a pull request against `main`:

- Keep it focused on one change, and rebase on the latest `main` if needed.
- Describe **what** changed, **why**, and **how you tested it** (including
  platforms). Link the related issue (`Fixes #123`).
- Add screenshots or a short recording for visible UI changes, ideally in light
  and dark appearance.
- Check the list below. CI must pass on all platforms before a pull request is
  merged.

**Pull request checklist**

- [ ] `cargo xtask ci` passes locally
- [ ] New or changed behaviour is covered by tests
- [ ] New user-visible strings are in all translation catalogs
- [ ] README, offline help and `docs/` are updated where behaviour changed
- [ ] `THIRD_PARTY_NOTICES.md` is regenerated if dependencies changed
- [ ] The web build still compiles if shared code changed
- [ ] The title follows Conventional Commits

## Releases

Releases are made by the maintainer. release-please keeps a release pull request
up to date from the Conventional Commits merged to `main`; merging it tags
`vX.Y.Z`, and the release workflow builds, tests, signs and publishes the
installers for all platforms. You do not need to bump versions or edit
`CHANGELOG.md` in your pull request. Details are in the
[README](README.md#releasing).

The release PR must update workspace versions in `Cargo.toml` and `Cargo.lock`
atomically. CI uses `--locked`; do not rely on a follow-up lockfile push after
the PR can already be merged. Release Please invokes packaging directly because
its token-created tag does not trigger a separate push workflow. Inspect the
packaging jobs in the release-creating run, not only a later green no-op rerun.
Never move a published tag or silently build another commit under its version.
If tagged source cannot build with its committed lockfile, prefer a fresh
corrected release after the repair is merged.

Packaging resolves the release tag to an immutable commit reachable from fetched
`origin/main` before any signing job starts; publication checks that the tag still
selects that commit. Manual artifact-only runs must also target `main`. This
preflight must be merged to `main` before using the updated workflow.
These checks do not replace owner-managed tag/branch protections or protected
signing environments: a writer who can replace workflow code can remove its
checks. Maintainers must restrict release-tag creation/update and privileged
workflow changes before treating signing credentials as isolated from writers.

## Licensing of contributions

Thinoda is licensed under the [European Union Public Licence v1.2 (EUPL-1.2)](LICENSE.md).
By submitting a contribution, you confirm that you wrote it yourself or otherwise
have the right to submit it, and you agree that it is licensed under the EUPL-1.2.

Questions? Open an issue. Thank you for helping to give thoughts shape!

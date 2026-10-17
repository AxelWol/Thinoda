# Code of Conduct and Contribution Guide

Thinoda is an open project. Everyone is welcome to use it, report problems,
suggest ideas and contribute code, translations or documentation. This document
explains how to contribute and how we expect everyone to behave.

## Our pledge

We want participation in Thinoda to be a harassment-free experience for everyone,
regardless of age, body size, visible or invisible disability, ethnicity, sex
characteristics, gender identity and expression, level of experience,
education, socio-economic status, nationality, personal appearance, race,
religion, or sexual identity and orientation.

## Rules of engagement

**Do:**

- Be kind, patient and respectful; assume good intent.
- Give and accept constructive feedback gracefully. Criticise ideas and code,
  never people.
- Stay on topic and keep discussions focused on improving Thinoda.
- Help newcomers; there are no stupid questions.
- Credit the work of others and respect licences and intellectual property.

**Do not:**

- Use insults, harassment, trolling, or personal or political attacks.
- Use sexualised language or imagery, or make unwelcome advances.
- Publish other people's private information (addresses, e-mail, etc.)
  without their explicit permission.
- Submit code, images, templates or other assets copied or reverse-engineered
  from other (commercial or open-source) mind-mapping products. Thinoda contains
  only original work and properly licensed third-party components.
- Engage in any other conduct that could reasonably be considered
  inappropriate in a professional setting.

## Scope

This code applies in all project spaces — issues, pull requests, discussions,
reviews and commit messages — and whenever someone represents the project in
public.

## Enforcement

Report unacceptable behaviour privately to the maintainer, Axel Wolters, at
<axelwolters@hotmail.com>. All reports are reviewed promptly and confidentially.
Depending on the severity, the maintainer may respond with a private reminder, a
public warning, removal of content, or a temporary or permanent ban from the
project. Security vulnerabilities are reported as described in
[SECURITY.md](SECURITY.md), not in public issues.

## How to contribute

### Reporting bugs and requesting features

1. Search the [existing issues](https://github.com/AxelWol/Thinoda/issues) first.
2. Open a new issue with a clear title, the Thinoda version (Help > About), your
   operating system and architecture, steps to reproduce, the expected and the
   actual result, and screenshots or a minimal sample map if possible.
3. Never attach maps that contain confidential or personal data.

### Contributing code

1. Fork the repository and create a feature branch from `main`.
2. Install a current stable Rust toolchain (see `rust-version` in `Cargo.toml`).
3. Make focused changes that follow the existing code style. Add or update unit
   and end-to-end tests for every behaviour change, and update the documentation
   (README, user guide, translations) when user-visible behaviour changes.
4. Run the checks locally before opening a pull request:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   python .github/scripts/third_party_notices.py --check
   ```

   If you add, remove or update dependencies, regenerate the notices with
   `python .github/scripts/third_party_notices.py` and make sure every new
   licence is compatible with the EUPL-1.2.
5. Write commit messages and pull request titles as
   [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), for
   example `feat: add outline drag and drop`, `fix(io): keep notes on XMind
   export`, `docs: clarify recovery`. They drive the automatic versioning and
   the [CHANGELOG](CHANGELOG.md). Mark breaking changes with `!` (for example
   `feat!: ...`) or a `BREAKING CHANGE:` footer.
6. Open a pull request against `main` describing what changed, why, and how you
   tested it. CI must pass on all platforms before merging.

### Licensing of contributions

By submitting a contribution you confirm that you wrote it yourself (or have the
right to submit it) and agree that it is licensed under the
[EUPL-1.2](LICENSE.md), the licence of the project.

## Attribution

Parts of this code of conduct are inspired by the
[Contributor Covenant](https://www.contributor-covenant.org/), version 2.1.

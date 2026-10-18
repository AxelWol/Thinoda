_Last updated: 2026-10-17 09:37:32 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Security Policy

## Supported versions

Security fixes are provided for the latest released version of Thinoda. Please
update to the [latest release](https://github.com/AxelWol/Thinoda/releases/latest)
before reporting a problem.

| Version | Supported |
|---|---|
| Latest release | Yes |
| Older releases | No |

## Reporting a vulnerability

**Please do not report security vulnerabilities in public issues, pull
requests or discussions.**

Report them privately using one of these channels:

1. **GitHub private vulnerability reporting** (preferred, if enabled): open the
   repository's **Security** tab and choose **Report a vulnerability**.
2. **E-mail:** <axelwolters@hotmail.com> with the subject
   `Thinoda security: <short summary>`.

Please include:

- the affected Thinoda version and operating system/architecture;
- a description of the vulnerability and its impact;
- step-by-step reproduction instructions and, if applicable, a minimal
  proof-of-concept file (for example a crafted `.xmind`, `.mm` or `.opml` file);
- any suggested fix or mitigation.

## What to expect

- Acknowledgement of your report within **7 days**.
- An initial assessment and a planned course of action within **30 days**.
- A fix released as soon as reasonably possible, depending on severity and
  complexity. You will be kept informed of progress.
- Credit in the release notes if you wish, once the fix has been published.

Please give us reasonable time to fix the issue before disclosing it publicly
(coordinated disclosure).

## Scope

Examples of relevant issues:

- Code execution, memory-safety problems or crashes triggered by opening
  crafted map files (imports of XMind, FreeMind/Freeplane, OPML, Markdown,
  MindManager and native files).
- Path traversal or writes outside the intended directories (for example when
  extracting attachments, autosave or crash-recovery data).
- Opening unsafe URLs or local files without the user's consent.
- Leaks of map content to the network (for example through the optional AI
  assistant beyond the configured local server).
- Vulnerabilities in the installers or the release pipeline.

Vulnerabilities in third-party dependencies should also be reported upstream;
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the components used.

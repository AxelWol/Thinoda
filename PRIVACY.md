_Last updated: 2026-10-30 09:27:31 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Privacy Policy

Effective date: October 29, 2026

## No data collection by Thinoda

**Thinoda's maintainers do not collect, receive, store or process your personal
data or mind map content through your use of the application.** Thinoda has no
developer-operated service that receives your documents or usage data.

Thinoda does not include advertising, analytics, tracking or automatic crash
reporting. It does not require an account, upload your maps to its maintainers,
sell your data or use your content to train AI models.

This does not mean that no processing happens at all: the application must
process content on your own device to display, edit, save, import, export and
print it. Optional services you choose to use may also process data, as
explained below.

## Your data stays on your device

On desktop, Thinoda processes your maps locally and saves documents, settings,
recent-file information and crash-recovery data on your device. These are not
sent to the maintainers. Diagnostic messages remain local unless you choose
to share them.

In the web app, map editing and file conversion run in your browser, not on a
Thinoda application server. Documents, settings and recovery data are stored
in the browser's local storage. The web app uses this storage for application
functionality, not tracking.

You control your saved files. To remove local application data, delete the
relevant documents, settings and recovery files. Clearing the web app's site
data removes its browser-stored documents, settings and recovery data; it does
not remove files you previously downloaded.

## Automatic update checks

On Windows, macOS and Linux, the running app checks the public GitHub API
(`api.github.com/repos/AxelWol/Thinoda/releases/latest`) at startup and roughly
every six hours, with bounded retries after errors. GitHub receives the request's
network metadata (including IP address) and a Thinoda/version User-Agent.
No documents, filenames, settings, credentials or account tokens are sent.
The response is used only to offer newer stable release/download links;
native installers/binaries are not downloaded or executed automatically.
Following those links opens GitHub in your browser and is subject to
[GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).

The web app periodically checks its own hosting origin for an updated service
worker and downloads public app-shell assets in the background. The host sees
ordinary web requests, not map content. Browser local storage keeps a
deployment-scope-specific build baseline and pending update identity solely to
distinguish an update ready to apply from one actually running on a later launch.
These records are not tracking identifiers and are not transmitted. Clearing
site data removes them and suppresses historical update-success notices.
No checks run after the native app exits; browser suspension/closure can also
prevent web checks. There is no separate updater service or analytics endpoint.

## Optional AI connections

AI features connect to the server you configure, such as a local LM Studio or
Ollama server, or a remote OpenAI-compatible provider. When you request AI
assistance, Thinoda sends the prompt and the map content needed for that request
to that server. Model discovery and connection tests also contact the configured
server. An API key, if configured, is sent to that server for authentication.
These requests do not pass through a service operated by Thinoda's maintainers.

With a local AI server, processing happens on the machine running that server.
If you select a remote provider, that provider may collect, retain or process
the submitted data under its own privacy policy. Review that policy before
sending sensitive content. Thinoda does not control the provider's retention,
training or logging practices. Do not use optional AI features if you do not
want content sent to an AI server.

On desktop, API keys saved through Settings are kept in the operating system's
credential store rather than the settings file. In the web app, the key is kept
in session storage for the current tab, not in persistent local storage.

### Optional local conversation history

AI Chat is session-only unless you explicitly enable local history for a saved
file. Named threads store authored prompts and completed assistant replies,
names/timestamps and bounded file identity metadata in application-local storage.
Prompts and replies can themselves contain sensitive data. This history is
**not encrypted** and is never included in normal maps, recovery or exports.
No credentials, system/context snapshots, drafts or partial streams are stored,
and reopening does not send anything to a provider. Confirmed age-based pruning,
per-file forgetting and global deletion are available in AI Chat; failed writes
leave transcripts in memory. Deletion is not secure erasure of backups or
provider-side copies. See [local history behavior](docs/AI-HISTORY.md).

## Cloud folders, printing and external links

Saving a document in a folder synchronized by OneDrive, iCloud Drive, Dropbox
or another service allows that service to synchronize and process the file.
Thinoda accesses the folder as local storage; the service's own privacy policy
applies to synchronization.

Printing passes the document to your operating system or browser's printing
system and your chosen printer or print service.

Opening an external link, Project News or Send Feedback takes you to an
external website. That website may receive ordinary connection information,
such as your IP address, and is governed by its own privacy policy. Send
Feedback does not automatically upload your maps or logs. Anything you
deliberately submit to a public GitHub issue, including attachments, may be
publicly visible. Do not include personal or confidential information.

## Web hosting and downloads

The public web app, repository and release downloads are hosted by GitHub.
GitHub may process connection information when you visit the site or download
files, independently of Thinoda. This policy's no-collection statement describes
the Thinoda application and its maintainers; it is not a promise that hosting
providers or other third parties collect no data.

See the [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)
for GitHub's practices.

## Policy scope and contact

This policy covers the desktop and web versions of Thinoda in this repository.
Third-party AI providers, cloud synchronization services, websites and print
services have their own privacy policies.

Changes to this policy will be published in this file with an updated effective
date. For privacy questions, use the
[project's GitHub issue tracker](https://github.com/AxelWol/Thinoda/issues)
without including sensitive information.

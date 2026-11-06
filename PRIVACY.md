_Last updated: 2026-11-01 11:36:42 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Privacy Policy

Effective date: November 1, 2026

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
in browser-managed storage belonging to the web app's origin. The web app uses
this storage for application functionality, not tracking.

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

## Optional context-menu usage counts

On desktop, Settings > Editor > Menu usage can count how often you use
context-menu commands. It is off by default. Counts contain only the menu kind
(topic, board object, blank canvas, Zone or annotation) and a command
identifier, are stored in `menu_usage.json` in the
local configuration directory, are never uploaded and never reorder menus.
Turn the option off to stop counting, or use Clear counts to delete them. The
web app does not record these counts.

## Optional cloud accounts

Cloud accounts are optional and distinct from local synchronized folders.
Integrated remote accounts are currently disabled by default behind the opt-in
`integrated-cloud-services` build feature while publisher registrations are
pending. Disabled native/web artifacts expose no account connection controls
and do not consume OAuth callbacks or initiate remote cloud jobs, regardless
of saved provider settings. Local sync-folder access and browser document
storage remain available. Populating publisher registration Variables alone
does not enable the feature.
Opening Settings, selecting a provider, or opening the cloud browser does not
look up stored account credentials or contact a provider. Connecting requires
explicit consent and can contact the configured WebDAV server or OAuth provider
and open its sign-in page. Native OAuth uses the system browser and a temporary
loopback callback; web OAuth redirects the current tab to the provider and
returns to a publisher-registered same-origin URI. A returned web callback is
scrubbed from the address bar. Only a valid, unexpired callback bound to the
one-time PKCE transaction initiated by your Connect consent automatically
continues that sign-in and account-folder discovery. Unsolicited, expired or
mismatched callbacks never trigger token exchange. Startup does not otherwise
look up credentials or reconnect.
Nextcloud Login Flow v2 contacts the configured server and polls during the
requested sign-in only.

Provider selection, public WebDAV server/collection configuration and the
optional local backup directory are saved as preferences. OAuth registration
IDs and redirect configuration belong to the publisher/self-hosted build, not
ordinary settings. Legacy saved OAuth client, redirect, drive and folder values
are ignored in the consumer workflow; no saved configuration is used as a
fallback registration. There are currently no registered Thinoda publisher
OAuth apps. Missing or unsupported build registration is reported before
browser launch, credential lookup or account requests. Box sign-in and Google
Drive web sign-in are disabled by the current public-client protocol boundaries.
See the [publisher build setup guide](docs/CLOUD-AUTH-SETUP.md); preparation
does not register applications or establish live provider acceptance.
Account usernames, app passwords, access/refresh tokens and PKCE transactions
are not saved in settings. Account credentials use the operating system's
credential store on desktop, and session storage for the current browser tab
on web. Active operations also hold credentials in memory. Stored credentials
are only read when you explicitly request their use; an expiring OAuth token
may be refreshed as part of that Connect action. There is no automatic cloud
connection, credential refresh loop or cloud autosave.

After requested authorization, the connection locates its starting folder
automatically. OneDrive looks up only the app folder and derives the drive
identifier from that metadata; it never retries with broader full-drive access.
The provider may create its app folder during lookup. No document content is
transferred by discovery, but it is not a promise of no remote metadata changes.
Other OAuth providers use their default root in the granted account namespace.
WebDAV defaults to Nextcloud browser login after you supply the server address;
app-password inputs are available only when explicitly selected.

An explicit folder listing sends folder identifiers to the chosen provider.
Open downloads the selected document and imports it into a new local editing
session without a local file path. Explicit Save or Upload sends the document
snapshot to that provider, not to Thinoda's maintainers. Saves use the revision
being replaced; conflicts do not cause an automatic overwrite. Upload uses
create-only operations on WebDAV, Dropbox, OneDrive and Box, without a preflight
listing or automatic rename, overwrite or retry. The returned file ID and
revision remain in the app session for later explicit conditional Saves.
Google Drive Upload creates a distinct new file and may duplicate an existing
name. The upload confirmation shows an explicit duplicate-name warning with
the filename and destination before transmission. It does not check that the
name is missing or update a sibling; later Save conditionally targets only the
returned file ID.
Cancellation cannot undo a request already accepted by a provider.

Version listing, where implemented, downloads metadata only. Creating a sharing
link requires separate consent and changes sharing on the selected remote file:
OneDrive requests organization-only view access, Box existing-collaborator access.
No anonymous/public link is requested, and links are not automatically opened
or copied. Provider permissions may reject these requests. Other adapters do
not offer version/sharing actions they have not implemented.

Disconnect removes the active connection and requests deletion of its locally
stored credentials without changing open content or unsaved edits. It does not
delete remote files or revoke provider-side grants; remove those in the provider's
account settings. A failed credential-store deletion is reported rather than
claimed successful. Reconnecting does not silently bind already-open documents
to a potentially different account. Live provider sign-in, OS credential stores
and deployed browser redirects have not been verified end to end for this build.
Your provider/server may process content and ordinary request metadata, including
IP address, under its own policy. Review that policy before connecting or
transferring sensitive documents.

The browser storage usage/quota and persistence controls are explicit local
browser API requests, not cloud-account calls. Persistence may be denied and
does not prevent deletion when you clear site data.

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
Third-party AI providers, cloud account providers, cloud synchronization services,
websites and print services have their own privacy policies.

Changes to this policy will be published in this file with an updated effective
date. For privacy questions, use the
[project's GitHub issue tracker](https://github.com/AxelWol/Thinoda/issues)
without including sensitive information.

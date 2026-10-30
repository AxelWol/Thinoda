# Optional local AI conversations (XV9)

AI Chat is **session-only by default**. Opening chat or a document does not send
a request, replay a conversation, or create a history file. Send still requires
the current selected-topic/current-sheet context review. Existing messages in
the selected conversation accompany an explicit Send.

## Enable and manage

Save a native document first, open **AI Chat → Local chat history**, and choose
**Enable local history**. The common confirmation dialog explains the privacy
trade-off. Existing session messages and drafts are **not copied**. Create a
named thread for future completed exchanges; select, rename and delete threads
in the same section. **Session-only conversation** returns to the original
in-memory conversation without persisting it. Thread switching and destructive
history controls are unavailable while any chat request is in flight.

Names must be nonempty, at most 80 UTF-8 bytes, without control characters and
case-insensitively unique within this file. Names are trimmed on entry.
There are at most 8 threads/file, 32 saved file bindings, 32 messages/thread,
256 KiB of text/thread and a 4 MiB serialized store. The existing per-prompt
8 KiB and per-reply 64 KiB limits still apply. Full stores refuse writes instead
of evicting an unrelated conversation. A request reserves room for its entire
bounded reply before sending.

The **7 / 30 / 90 days** controls are **explicit prune-now retention**, not a
background expiration promise. Confirmation deletes threads whose last completed
exchange/creation/clear predates that cutoff, across **all** stored files. The cutoff
is inclusive. Empty file bindings remain opted in until forgotten.
**Disable and forget this file** removes its durable binding and all its saved
threads; **Delete all local chat history** clears the complete store. Completed
visible messages and other session conversations remain available in memory
after disable/pruning/global deletion. Reopen the document before opting in
again; this conservative boundary avoids silently repersisting retained session
messages. Explicit thread deletion/clear discards that thread's selected content.

## Identity and isolation

No workbook schema or recovery schema was changed to store history. A binding is
the platform-canonical, losslessly representable file path **plus SHA-256 of
the saved bytes** (identity reads are limited to 64 MiB). These bounded bytes
are re-imported and compared with the loaded workbook before automatic
association, so a stale asynchronous import cannot bind its old content to a
file replaced in the meantime. This adds a bounded verification read/parse on
opening a native file.

* Reopen/restart restores only an exact binding; names, titles and topic IDs are
  never used as identity. No network request is made by restoration.
* Successful same-path saves in an already-associated OpenDoc advance that
  session's binding with a revision-checked history transaction. If that write
  fails, messages remain in memory; no successful association is claimed.
* Save As detaches persistence. The old file's stored history stays with the old
  binding. The visible conversation stays session-only; the new file does not
  inherit it.
* A moved or copied file has a different canonical path and requires fresh,
  explicit opt-in. There is deliberately **no automatic history migration or
  merge**, including for byte-identical copies. An alias resolving to the same
  canonical path and bytes is the same binding.
* Externally changed bytes at the same path do not inherit an older binding.
  Byte-identical replacement at the identical canonical path is indistinguishable
  from the same document. Content hashes are not proof of authorship.
* Unsaved/import-only documents and recovery-created OpenDocs have no automatic
  binding. Save first, then explicitly enable. The recovery key is not a history
  key. A second simultaneously open tab for the same binding does not become a
  second writer automatically.
* Unsupported/remote paths, identity failures and oversized files remain usable
  with session chat. No remote filesystem assumptions or lossy path keys.

## Privacy and storage

Authored prompts and completed assistant replies can themselves contain
**sensitive data**, even without automatic map snapshots. The local store is
**not encrypted**. Native storage is `data/ai-conversations.json` under the
configured Thinoda application home. Browser storage uses an origin-local
`thinoda.chat:` key scoped by the normalized app data directory. It is not
included in maps, recovery snapshots, normal exports or provider settings.

The allowlisted versioned envelope contains only binding path/digest, random
revision/thread IDs, names, last-update timestamps, retention choice and
completed user/assistant pairs. It contains no provider configuration,
credentials, system messages, context snapshots, drafts, cancelled/partial
streams or error diagnostics. Context is rebuilt only for explicit Send.
Deletion is logical, not secure erasure: filesystem/browser backups and synced
application-home folders may retain copies. Provider-side retention is separate.

Native writes use the existing same-directory atomic replacement helper under
an exclusive OS lock, with exact prior-store/revision checks. Browser writes
require an origin-wide **Web Lock** held until tab shutdown and a successful
`localStorage` operation; they never use the VFS's volatile fallback for history.
Unsupported/insecure browsers, denied storage, a competing tab, quota failures,
corrupt/unknown/oversized stores and stale revisions refuse persistence. Browser
lock acquisition is asynchronous: use **Retry** after preparation or after
closing the other history-owning tab. Browser storage remains subject to eviction
and user clearing site data.

All failed writes/deletes/prunes leave the pre-operation store and in-memory
transcripts intact, with a visible error; no success toast is shown. A completed
reply whose write fails remains in memory for Retry. Concurrent revisions are
not automatically merged; copy important unsaved text before restarting to
resolve a stale-client conflict. A crash between saving a map and updating its
binding can leave its old history unassociated, rather than attach it to the
wrong file. There is no durability claim for an unsuccessful transaction.

## Verification and remaining manual acceptance

Local verification (2026-10-30):

* Broad workspace run excluding `thinoda-platform`: **1066 passed, 2 ignored**
  in 112 suites. Final focused/library checks were rerun after self-review fixes.
* Final app library: **279 passed**, including **34 chat regressions**.
* Platform: **102 passed**, with only the pre-existing Windows
  `cloud_prefix_boundaries_case_and_nested_provider` alias test skipped, not fixed.
* Translation checks: **8 passed**; strict workspace/all-target clippy and strict
  WASM `thinoda-web` clippy passed, as did formatting, diff whitespace and
  third-party notice checks. SHA-256 reuses the already-locked `sha2` package.

Storage tests cover explicit/default-off writes, round trip, stale/ABA clients,
malformed/unknown/oversized storage, names/roles/counts/byte limits, retention
boundaries, canonical aliases/copies/moves and failed transactions.
App regressions cover restart/threads, same-path save versus Save As/recovery,
provider/draft exclusion, clean map output, failed clear/delete/disable, completed
streams, cancellation/stale thread results, document/viewport ownership,
session-only streams, default-off network/storage and failed completion writes.

The shared dialog and theme-native wrapped/scrollable controls have translations
for all ten shipped languages. Native/WASM compilation is not evidence of live
browser persistence or macOS/Linux integration: browser quota/Web Lock/reload
behavior and platform IME/touch/screen-reader/visual acceptance remain manual
gates. No independent QA approval is claimed.

### Self-review

The final review tightened several meaningful boundaries: a failed file-binding
advance cannot be hidden by a later successful transcript write; a modal
confirmation cannot act on a different document/thread/viewport; thread actions
also refuse in-flight mutation below the UI layer; session-only streams capture
their non-durable ownership at Send; disabling does not discard parked drafts;
and restart selection uses creation order rather than random UUID ordering.
New tests exercise these cases alongside the existing chat IME/touch safeguards.
Prune-now retention and the close/reopen re-enable boundary are conservative
limitations, not automatic TTL/migration features.

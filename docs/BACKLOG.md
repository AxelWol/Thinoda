# yMind backlog

Prioritized ideas beyond the shipped phases. P1 = next, P2 = soon, P3 = later.

## XMind parity (from the user guide)
- P1 Print (Ctrl+P) with page setup and fit-to-page
- P1 Find & Replace across sheets (Ctrl+F / Ctrl+H)
- P1 Pitch mode (presentation from branches, F5) and slide export
- P1 Map Shot (F7): region capture to clipboard/PNG
- P2 Zone (Ctrl+Alt+Z) and callouts editing UI polish
- P2 Gantt view for to-dos with start/end dates, progress and export
- P2 Equations (LaTeX → rendered topic content)
- P2 Attachments and pictures embedded in the `.ymind`/`.xmind` package
- P2 Password-protected files (AES-GCM, Argon2 key derivation)
- P2 Quick styles, copy/paste style, global font, custom theme editor
- P2 Mask data (anonymize topic text before sharing)
- P2 Merge XMind files, skeleton/structure picker gallery
- P3 Audio notes (record/play)
- P3 Customizable shortcuts (Preferences › Shortcuts)

## AI
- P1 Streaming responses (SSE) with live preview and cancel button
- P1 Store API keys in the OS keychain (Windows Credential Manager, macOS Keychain,
  Secret Service) instead of `settings.toml`
- P2 Semantic search/clustering with an embedding model (e.g. `nomic-embed-text`)
- P2 "Rewrite topic", translate branch, AI-assisted Pitch outline
- P3 Prompt library editable by the user

## Platform & collaboration
- P1 Code signing + notarization; auto-update via `cargo-packager-updater`
- P2 macOS universal binary; Flatpak and `.rpm` packages; winget/Homebrew manifests
- P2 Real cloud APIs (OneDrive/Google Drive/Dropbox) beyond synced folders, conflict resolution UI
- P3 Real-time co-editing (CRDT, e.g. `yrs`) — basis for a companion web/mobile app
- P3 Plugin API (WASM) for importers/exporters

## Quality
- P1 Visual regression tests (kittest snapshot images) per theme
- P2 Accessibility audit with screen readers (NVDA, VoiceOver, Orca)
- P2 Fuzzing of the XMind/FreeMind/OPML importers (`cargo fuzz`)
- P2 Performance budget: 10 000-topic map at 60 fps, benchmark in CI
- P3 More UI languages (FR, ES, JA, ZH)

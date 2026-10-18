# Importer fuzzing

The fuzz harness is a separate Cargo workspace: its `libfuzzer-sys` and ZIP
dependencies are used only by these developer/CI tools and are not dependencies
of the shipped workspace or included in `THIRD_PARTY_NOTICES.md`.

## Run locally

Install a nightly Rust toolchain and `cargo-fuzz` once:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz --locked
```

From the repository root, run an individual target (omit `--` arguments for an
unbounded local campaign):

```sh
cargo +nightly fuzz run --release xmind
cargo +nightly fuzz run --release freemind
cargo +nightly fuzz run --release opml
```

To reproduce the bounded CI smoke locally, run each command with its matching
dictionary and libFuzzer limits:

```sh
cargo +nightly fuzz run --release xmind -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/xmind.dict
cargo +nightly fuzz run --release freemind -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/freemind.dict
cargo +nightly fuzz run --release opml -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/opml.dict
```

Inputs over 1 MiB are ignored by the targets. XMind accepts raw input as well as
`JSON\n` and `CONTENT_XML\n` seed envelopes; the latter two are wrapped in a
small generated ZIP package to exercise each XMind entry parser. The corpus also
contains tiny generated ZIP package seeds so mutation reaches the archive
structure, and the raw XMind path includes legacy XML. FreeMind and OPML consume
XML directly. On successful imports each target checks validation and an exact
round-trip through Thinoda's native format. Fuzzer crashes and assertion
failures are intentionally not caught; preserve any reproducer under that
target's corpus and investigate it.

CI runs on Ubuntu with nightly, caps each campaign at 20 seconds, each input at
five seconds, maximum input at 1 MiB, RSS at 2 GiB, and the whole job at 30
minutes including installation and compilation. Corpus files are small original
examples and packages generated from them; no third-party map data is included.

This smoke is a short regression signal, not exhaustive exploration, a proof of
parser safety, or a substitute for the deterministic importer tests. In
particular, it does not probe decompression ratios or allocate oversized
archives; existing tests continue to verify that declared ZIP entries above
200 MiB are rejected before decompression. Keep those production protections
intact. Longer campaigns and broader resource-limit analysis remain future work.

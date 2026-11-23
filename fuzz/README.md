_Last updated: 2026-11-21 14:09:40 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Parser fuzzing

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
cargo +nightly fuzz run --release drawio_import
cargo +nightly fuzz run --release drawio_bundle
```

To reproduce the bounded CI smoke locally, run each command with its matching
dictionary and libFuzzer limits:

```sh
cargo +nightly fuzz run --release xmind -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/xmind.dict
cargo +nightly fuzz run --release freemind -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/freemind.dict
cargo +nightly fuzz run --release opml -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/opml.dict
cargo +nightly fuzz run --release drawio_import -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/drawio_import.dict
cargo +nightly fuzz run --release drawio_bundle -- -max_total_time=20 -rss_limit_mb=2048 -max_len=1048576 -timeout=5 -dict=fuzz/dictionaries/drawio_bundle.dict
```

Inputs over 1 MiB are ignored by the targets. XMind accepts raw input as well as
`JSON\n` and `CONTENT_XML\n` seed envelopes; those are wrapped in a
small generated ZIP package to exercise each XMind entry parser. `IMAGE_SVG\n`
and `IMAGE_SVGZ\n` wrap the following mutable bytes as a shared picture and
attachment, exercising SVG validation and gzip decoding without having to
preserve a ZIP structure during mutation. Raw SVG mutations reach deeper XML
paths; arbitrary SVGZ mutations usually invalidate the gzip checksum, so they
primarily exercise decoder rejection. The corpus also
contains tiny ZIP package seeds, including PNG image resources with omitted and
partial dimensions, so mutation reaches image/manifest validation and the
archive structure; the raw XMind path includes legacy XML. FreeMind and OPML
consume XML directly. The draw.io target exercises XML, compressed pages,
editable SVG/PNG, embedded HTML, self-contained `#R` links and plain or
URI-encoded clipboard text without fetching their URLs. Successful clipboard
decodes must produce a valid board fragment and report known omissions. Its
connector-detachment seed checks that frame attachments are reported rather
than silently converted to free endpoints. A malformed-point seed checks that
an unusable connector endpoint is reported instead of becoming a fabricated
point. A malformed-geometry seed checks that an invalid vertex and its subtree
are omitted with a warning while an unrelated shape survives. An out-of-range
point seed checks that usable connector and ink points survive alongside a
warning. An appearance-colors seed exercises adaptive and inherited colors and
an out-of-range stroke width through document and clipboard imports. A
CSS-colors seed exercises RGB/RGBA, named and four-digit hex colors, nested
adaptive colors, and nonfinite hue through the same paths. A page-background
seed covers a fixed adaptive pair, a finite page, a gradient
and an image, and checks that page-only warnings do not leak into clipboard
fragments. A folded-notes seed checks editable card conversion, connector
attachment, and document and clipboard warnings. A multi-page warning seed
checks that file warnings cover all pages while clipboard appearance warnings
refer to the pasted first page. A double-ellipse seed checks editable shape
identity and the custom inner-outline inset on document and clipboard import.
A flowchart-annotation seed exercises the transparent,
open-sided bracket conversion. A flowchart-process seed covers both the
rounded stencil warning and Thinoda's square-rectangle interchange marker.
A fixed-ports seed covers attached points, offsets, perimeter flags and
malformed constraints through file and clipboard import.
A label-sizes seed exercises named-style font and alignment changes and folded
connector labels through document and clipboard imports.
An HTML-label seed exercises quoted attributes, named entities, hidden script
content and block boundaries through the same imports.
An HTML-whitespace seed covers collapse warnings across adjacent text nodes.
The target's PNG seed is the original editor-exported fixture already used by
the importer tests.
The bundle target splits input at the first zero byte into catalog JSON and a
deflated package blob; without a separator it exercises catalog parsing alone.
It uses the same catalog index and bounded package decoder as the shipped
bundle, including mutated offsets, lengths and compressed bytes. Indexing
preflights the aggregate UTF-8 bytes of namespace-plus-name keys against a
4 MiB limit before allocating them; repeated keys count toward the limit. This
limits key amplification, not all parser or allocator memory. Its small
valid seed contains `drawio.electrical.plc_ladder` copied from the licensed
bundled stencils, with a one-package catalog and an offset adjusted to zero; see
`crates/thinoda-io/stencils/drawio/NOTICE`. The `fuzzing` Cargo feature exposes
only the internal hooks needed by the separate fuzz workspace.
On successful imports, including reports with conversion
warnings, each importer target checks validation and an exact round-trip through
Thinoda's native format. This in-memory check does not bypass the application's
conversion-consent flow. Fuzzer crashes and assertion failures are intentionally
not caught; preserve any reproducer under that target's corpus and investigate it.

The deterministic seed checks verify that each draw.io container, link and
clipboard seed actually reaches its intended importer:

```sh
cargo test --locked -p thinoda-io --test drawio_fuzz_seeds
cargo test --locked -p thinoda-io --test xmind_fuzz_seeds
cargo test --locked -p thinoda-io --lib drawio_bundle::tests -- --test-threads=1
```

CI runs on Ubuntu with nightly, caps each campaign at 20 seconds, each input at
five seconds, maximum input at 1 MiB, RSS at 2 GiB, and the whole job at 30
minutes including installation and compilation. Importer corpus files are small
original examples and packages generated from them; the bundle seed is the
licensed shipped package described above.

The separate **Importer acceptance campaigns** workflow runs all five targets
on Linux with the default sanitizer for 30 minutes each, keeping the same
per-input time, memory and length limits. It runs for pull requests changing the
fuzz harness or its workflow, and supports manual dispatch after merging.
Logs, generated corpus and failure reproducers are retained for 14 days even
when a campaign fails. Concurrent updates to the same pull request cancel the
older campaign; canceled runs do not satisfy acceptance. Record the exact
successful candidate SHA and run before claiming the sprint campaign gate.
Compilation time is outside the 30-minute fuzz budget but inside each job's
60-minute timeout. Local Windows hosts need the compatible ASan linker runtime;
do not disable sanitizers to claim equivalent acceptance.

This smoke is a short regression signal, not exhaustive exploration, a proof of
parser safety, or a substitute for the deterministic importer tests. In
particular, it does not establish decompression-ratio or peak-memory guarantees
or allocate oversized archives; existing tests continue to verify that declared
ZIP entries above 200 MiB are rejected before decompression. Keep those
production protections intact. Longer campaigns do not replace broader
resource-limit analysis.

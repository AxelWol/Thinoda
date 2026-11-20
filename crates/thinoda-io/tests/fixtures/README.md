_Last updated: 2026-11-19 12:53:59 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# draw.io fixtures

`published-rectangle.drawio.png` was exported from the published
[diagrams.net editor](https://app.diagrams.net/) with a labelled green rectangle
and `Include a copy of my diagram` enabled. It is intentionally kept as an
unaltered PNG so tests exercise the editor's actual metadata encoding.

# XMind fixtures

`sample-map-no-thumbnail.xmind` and `sample-map-with-thumbnail.xmind` are the
unaltered sample archives supplied by the reporter of
[Thinoda #136](https://github.com/AxelWol/Thinoda/issues/136#issuecomment-6992737512).
The issue reports XMind desktop 26.03.04126. Both archives contain identical
`content.json` and `metadata.json`. The desktop package additionally contains
`Thumbnails/`, a manifest-declared `Thumbnails/thumbnail.png`, and an unlisted
legacy `content.xml`. The cleaned package omits those entries and removes the
thumbnail declaration. Tests must import both into the same workbook.

# XMindMark fixtures

The bounded XMindMark grammar and deterministic writer are tested with small
in-source examples in `crates/thinoda-io/src/formats.rs`; no external renderer
or generated `.xmindmark` artifact is required.

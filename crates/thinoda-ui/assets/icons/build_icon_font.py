"""Build Thinoda's subset of Google Material Symbols Outlined.

Usage:
    python build_icon_font.py [path/to/MaterialSymbolsOutlined[FILL,GRAD,opsz,wght].ttf]

Without an argument the pinned upstream variable font is downloaded. Requires
`fonttools` (pip install fonttools).

The variable font is instanced at FILL=0, GRAD=0, wght=400, opsz=20 (the optical
size closest to the 16-24 px ribbon/toolbar tiers) and reduced to the icon names
listed in ``material-symbols.txt``. Every Material codepoint U+XXXX is remapped
to U+F0000+XXXX (Supplementary Private Use Area-A) so that it can never collide
with private-use glyphs of egui's bundled fonts. ``thinoda_ui::icons`` uses
those remapped codepoints and a unit test checks them against this manifest.
"""

from __future__ import annotations

import io
import pathlib
import sys
import urllib.request

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
from fontTools.varLib import instancer

HERE = pathlib.Path(__file__).resolve().parent
UPSTREAM = (
    "https://raw.githubusercontent.com/google/material-design-icons/"
    "10ecc14790612ca8fd164e3f4ffe9c16fc50d5b7/variablefont/"
    "MaterialSymbolsOutlined%5BFILL,GRAD,opsz,wght%5D"
)
OFFSET = 0xF0000
FAMILY = "Thinoda Material Symbols"


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=120) as response:
        return response.read()


def main() -> int:
    names = [
        line.strip()
        for line in (HERE / "material-symbols.txt").read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    ]
    if len(set(names)) != len(names):
        raise SystemExit("duplicate icon names in material-symbols.txt")
    if len(sys.argv) > 1:
        source = pathlib.Path(sys.argv[1])
        font_bytes = source.read_bytes()
        codepoints_text = source.with_suffix(".codepoints").read_text(encoding="utf-8")
    else:
        font_bytes = fetch(UPSTREAM + ".ttf")
        codepoints_text = fetch(UPSTREAM + ".codepoints").decode("utf-8")
    codepoints: dict[str, int] = {}
    for line in codepoints_text.splitlines():
        name, _, value = line.partition(" ")
        codepoints.setdefault(name, int(value, 16))
    missing = [name for name in names if name not in codepoints]
    if missing:
        raise SystemExit(f"unknown Material Symbols: {missing}")

    font = TTFont(io.BytesIO(font_bytes))
    font = instancer.instantiateVariableFont(
        font, {"FILL": 0, "GRAD": 0, "wght": 400, "opsz": 20}
    )
    options = subset.Options()
    options.layout_features = []
    options.name_IDs = []
    options.notdef_outline = True
    options.drop_tables += ["GSUB", "GPOS", "GDEF", "STAT"]
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=[codepoints[name] for name in names])
    subsetter.subset(font)

    mapping = {}
    for table in font["cmap"].tables:
        for code, glyph in table.cmap.items():
            if code < 0x10000:
                mapping[OFFSET + code] = glyph
    table = CmapSubtable.newSubtable(12)
    table.platformID, table.platEncID, table.language = 3, 10, 0
    table.cmap = mapping
    font["cmap"].tables = [table]

    name = font["name"]
    name.names = []
    for name_id, value in [
        (0, "Copyright 2022 Google LLC. Subset by Thinoda."),
        (1, FAMILY),
        (2, "Regular"),
        (4, FAMILY),
        (6, "ThinodaMaterialSymbols-Regular"),
        (13, "Licensed under the Apache License, Version 2.0"),
        (14, "https://www.apache.org/licenses/LICENSE-2.0"),
    ]:
        name.setName(value, name_id, 3, 1, 0x409)

    font.save(HERE / "ThinodaMaterialSymbols.ttf")
    with (HERE / "material-symbols.codepoints").open("w", encoding="utf-8", newline="\n") as out:
        for icon in sorted(names):
            out.write(f"{icon} {OFFSET + codepoints[icon]:x}\n")
    print(f"{len(names)} icons -> {HERE / 'ThinodaMaterialSymbols.ttf'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

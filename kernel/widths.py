#!/usr/bin/env python3
"""Regenerate src/ambiguous.tsv: the East Asian Ambiguous ranges of UAX #11.

Vim draws these two cells wide when 'ambiwidth' is double; the kernel needs
the same table to lay out grids Vim will draw. unicode-width's CJK mode is no
substitute: it keeps Greek letters, among others, one cell wide.
"""
from pathlib import Path
import sys
import unicodedata

ranges = []
for code in range(0xA0, sys.maxunicode + 1):
    if unicodedata.east_asian_width(chr(code)) != "A" or unicodedata.category(chr(code)) in ("Mn", "Me", "Cn"):
        continue
    if ranges and ranges[-1][1] == code - 1:
        ranges[-1][1] = code
    else:
        ranges.append([code, code])
(Path(__file__).parent / "src/ambiguous.tsv").write_text(
    f"# first\tlast; East Asian Ambiguous (UAX #11), Unicode {unicodedata.unidata_version}, by widths.py\n"
    + "".join(f"{a:04X}\t{b:04X}\n" for a, b in ranges))
print(len(ranges), "ranges")

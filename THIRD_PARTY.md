# Third-party data and notices

This inventory records the sources of bundled symbol data, width data and test
cases. The files in `LICENSES/` are verbatim copies of upstream notices.

Prebuilt kernel archives also include the Rust dependencies' original
`LICENSE*`, `COPYING*` and `COPYRIGHT*` notices under
`LICENSES/rust/<crate>-<version>/`. Packaging collects these from the locked
Cargo dependency metadata and preserves both notices for dual-licensed crates.

**unicode-math — symbol table.** `kernel/src/symbols.tsv` contains a generated
extract of `unicode-math-table.tex` from unicode-math **0.8r (2023-08-13)**,
tag `v0.8r`, commit `936812ed7407cd2b2f06bf4c97f12275a6b2e781`.
The complete upstream source is available in the
[pinned source tree](https://github.com/latex3/unicode-math/tree/936812ed7407cd2b2f06bf4c97f12275a6b2e781).
Its `COPYRIGHT` and `LICENSE` are preserved in
[unicode-math-COPYRIGHT.txt](LICENSES/unicode-math-COPYRIGHT.txt) and
[LPPL-1.3c.txt](LICENSES/LPPL-1.3c.txt), respectively; upstream specifies
LPPL 1.3c or later.

The installed table resolved with `kpsewhich unicode-math-table.tex` is
byte-identical to that revision's table (SHA-256
`3976ff8f39c84b61055b1cb6851a1b2cfdefa918a2a642c954e6ceafdca7277e`).
`kernel/symbols.py` extracts command names, Unicode characters and math classes,
omitting the table's TeX syntax and descriptions. It adds classic upright Greek
aliases, classic LaTeX command names with single-character `utftex` fallbacks,
and UniTeX aliases without replacing existing entries. It NFC-normalizes text,
removes the `math` prefix from class labels and sorts the result into TSV.
This generated extract is for TeXinsitu's renderer, not a drop-in substitute
for unicode-math or its TeX table. The upstream authors do not maintain this
generated extract; changes and issues belong to this project.

**UniTeX — aliases and baseline cases.** Revision
`6e6294bdfb6101eb6e3dc440e1b9cb5a1a1efdc0` supplies
[`src/impl/unicode/fixed.ts`](https://github.com/kokic/UniTeX/blob/6e6294bdfb6101eb6e3dc440e1b9cb5a1a1efdc0/src/impl/unicode/fixed.ts)
for `vendor/unitex-symbols.json`, merged into `kernel/src/symbols.tsv`.
The same revision's `test-runner.js` and `README.md` supply the baseline cases
in `tests/support/unitex_upstream.json`, which also records project results.
The complete upstream MIT notice is in
[UniTeX-MIT.txt](LICENSES/UniTeX-MIT.txt) and the two JSON source fixtures.

**KaTeX — screenshotter cases.** `tests/support/katex_screenshotter.json`
contains selected input cases from
[`test/screenshotter/ss_data.yaml`](https://github.com/KaTeX/KaTeX/blob/9b2280dee4acb4da7d866dbaccad120b00949c27/test/screenshotter/ss_data.yaml)
at revision `9b2280dee4acb4da7d866dbaccad120b00949c27`.
The complete upstream MIT notice is in
[KaTeX-MIT.txt](LICENSES/KaTeX-MIT.txt) and the fixture's `license` field.

**Unicode — East Asian Ambiguous widths.** `kernel/src/ambiguous.tsv` records
Unicode **17.0.0** data exposed by Python's `unicodedata`. The versioned
[Unicode Character Database](https://www.unicode.org/Public/17.0.0/ucd/ReadMe.txt)
and [EastAsianWidth.txt](https://www.unicode.org/Public/17.0.0/ucd/EastAsianWidth.txt)
identify the source release; its data notice is © 2025 Unicode®, Inc.
`kernel/widths.py` selects East Asian Width `A` from U+00A0 onward, excludes
general categories `Mn`, `Me` and `Cn`, and coalesces adjacent code points.
The official [Unicode License V3](https://www.unicode.org/license.txt)
(SPDX `Unicode-3.0`) is preserved in
[Unicode-3.0.txt](LICENSES/Unicode-3.0.txt), retrieved on 2026-11-18 with SHA-256
`e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`.

Additional generation inputs: `kernel/symbols.py` reads command names from
installed `fontmath.ltx`, `latex.ltx`, `amssymb.sty`, `amsmath.sty` and the project
prelude. Its missing-character lookup invokes the external `utftex` executable
from [libtexprintf](https://github.com/bartp5/libtexprintf).

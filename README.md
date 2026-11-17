# TeXinsitu

TeX math rendered in place for Vim, in TeX, Markdown and Cogitatum buffers.
The buffer stays source: move the cursor into a formula and its TeX appears
for editing; move out and the rendering returns. Yanks, saves, undo and
searches always see source. Nothing is written into the file, and there is no
popup or per-formula command.

```
source    The loss $L_{\mathrm{avg}}=\frac{1}{N_b}\sum_i e_i$ and $x^2+y_{10}$.
rendered  The loss Lavg=1/Nb∑iei and x²+y₁₀.
```

## Installation

- Vim 9.0.0121 or newer with `+textprop`, `+conceal` and `+job`
- Rust (`cargo`), to build the kernel once

vim-plug:

```vim
Plug 'BaksiLi/texinsitu.vim', {'do': 'cargo build --release --manifest-path kernel/Cargo.toml'}
```

Native Vim package:

```sh
git clone https://github.com/BaksiLi/texinsitu.vim.git ~/.vim/pack/plugins/start/texinsitu.vim && cargo build --release --manifest-path ~/.vim/pack/plugins/start/texinsitu.vim/kernel/Cargo.toml
```

Rebuild the kernel after updating. Older Vim versions keep source editing
unchanged; the virtual text API requires Vim 9.0.0121. Neovim is not supported.

## Use

Rendering is automatic. `:TexInsituToggle` switches the current buffer between
rendered formulas and raw source. There are no default key mappings.
See `:help texinsitu` for reference.

| Option | Default | Effect |
| --- | --- | --- |
| `g:texinsitu_filetypes` | `['tex', 'markdown', 'cogitatum']` | Filetypes with rendered formulas |
| `g:texinsitu_enabled`, `b:texinsitu_enabled` | `1` | Off globally or per buffer |

The highlight group `TexInsitu` colours formulas (linked to `Special`);
`TexInsituSub` and `TexInsituSup` follow its colour by default. All three
respect your own highlight definitions.

Formulas are hidden with match conceal, which needs `'conceallevel'` 2. That
would also turn on syntax conceal inside the formula being edited, so the
plugin loads the active buffer's syntax with its conceal defaults off.
Explicit settings for `g:tex_conceal`, `g:vimtex_syntax_conceal_disable` and
`g:markdown_syntax_conceal` take precedence, and their global values stay
unchanged. Toggling off restores the window's conceal options and the
buffer's normal syntax.

## What it renders

Inline math stays on its source row, inside the prose. Unicode supplies the
characters; numeric scripts use Unicode script digits (`x²`, `y₁₀`), other
scripts are styled text, and fractions become `a/(b+c)`. `$K_{\mathrm g}$`,
`$G^*$` and `$\Delta g_\pi$` read as they would on paper.
Letter scripts are italic and underlined for subscripts, italic for
superscripts. Terminals that draw no italic still show superscripts bold.

Standalone display equations (`\[ \]`, `$$`, `equation`, `align`, `cases`,
matrices…) become a two-dimensional block centred on the source lines they
replace, with the `\label` beside the last row. Only fractions, roots, limits
of large operators, tall delimiters and environment rows add height; scripts
stay on the row.

```
source    \begin{equation}
           E=\frac{1}{N}\sum_{i=1}^{N}\left|y_i-\hat y_i\right|^2,
           \qquad \sigma=\sqrt{E}.
           \label{eq:error}
          \end{equation}

rendered    1  N                  ─
          E=─  ∑  |yi−ŷi|²,    σ=√E.
            N i=1                       eq:error
```

In TeX, formulas use the document's `\newcommand` and `\DeclareMathOperator`
definitions, including those in the main file of a VimTeX project when you
edit a child file. `siunitx` values and units (`\SI{110}{nm}`) render too.
TeXinsitu never runs TeX or loads packages. Comments, verbatim environments,
URLs and definitions stay literal, as does fenced or backquoted code in
Markdown. An unsupported or unfinished formula stays source as a whole, never
half rendered.

## Specimens and fonts

Open the [specimen sheet](examples/specimens.md) in Vim to try scripts,
alphabets, accents, SI units, CJK prose and a display fraction with your own
font. The renderer tests use the same sheet.

| Specimen | Rendering choice | Compatibility |
| --- | --- | --- |
| `x^2+y_{10}` | `x²+y₁₀` | Unicode script digits |
| `P_{\mathrm{avg}}^{\mathrm{tot}}` | Letter scripts stay on the baseline | Subscripts use italic and underline; superscripts use italic, plus bold in terminals |
| `\mathbb{R}`, `\mathcal{F}`, `\mathscr{F}` | `ℝ`, `ℱ`, `𝓕` | Distinct mathematical alphabets need font coverage |
| `\alpha-\beta`, `\partial` | `α−β`, `∂` | Plain Greek and derivative symbols avoid less widely covered mathematical italic characters |
| `\text{\'a \"o \H{e}}` | `á ö e̋` | NFC where possible; other accents remain combining marks |
| `64^\circ`, `\SI{125}{nm}` | `64°`, `125 nm` | No space before a plane angle; a no-break space before a unit |

Unicode characters and font styling do different jobs. Mathematical alphabets
carry meaning; ordinary variables remain plain letters, and Vim attributes
distinguish letter scripts. No font is bundled or selected for you. A missing
glyph or an unsupported italic attribute depends on your font and terminal;
there are no terminal-specific rendering paths.

Screen tests cover narrow windows, soft wrapping, `'ambiwidth'` in both
settings and `setcellwidths()` overrides. They check text-cell placement, not
every font's glyph shape. This is an editing view; a typeset PDF remains the
reference for exact mathematical typography.

## Conventions

TeXinsitu follows published conventions wherever a choice has to be made, so
its output reads like typeset math and lines up where Vim draws it.

| Convention | What TeXinsitu does |
| --- | --- |
| [Unicode TR #25](https://www.unicode.org/reports/tr25/), mathematics in Unicode | Characters come from unicode-math's table, normalised to NFC; the minus sign is `−` (U+2212), primes are `′ ″ ‴`. |
| [UAX #11](https://www.unicode.org/reports/tr11/), East Asian Width | With `'ambiwidth'` set to `double`, Ambiguous characters such as Greek letters and box drawing count two cells, as Vim draws them; `setcellwidths()` overrides count too. |
| [UAX #14](https://www.unicode.org/reports/tr14/), line breaking | Closing punctuation wraps with the formula before it. |
| [SI Brochure](https://www.bipm.org/en/publications/si-brochure) | A no-break space joins a number and its unit (`110 nm`); a plane-angle sign takes none (`64°`). |
| ISO 80000-2, mathematical signs and symbols | One deliberate departure: ISO sets variables in italic, but TeXinsitu keeps letters plain, because terminal fonts often lack the mathematical italic alphabet (`𝑥`). Highlighting carries the distinction, and a GUI draws scripts in italic. |

## How it works

Each Vim runs one resident kernel, written in Rust. It reads the document as
TeX does: comments, verbatim text and definition bodies hold no formulas, and
math never crosses a blank line, so an unclosed `$` affects only its own
paragraph. Layouts are cached by formula source, mode and the definitions in
scope, so an edit lays out only the formulas it changed; a change in Vim's
cell widths lays everything out again. The cache holds at most 8,192 layouts,
sharing unchanged command definitions across edits. Parsing waits until
typing pauses; while you type, formulas on the edited line move with the text,
and a touched formula shows source until the next parse.

Cursor movement repaints only the entered and left formulas and their inline
neighbours. Same-line edits repaint that row; resizing replaces virtual text
in one pass.

Vim reserves a wrapped line's height from its source, concealed or not. Prose
therefore wraps where its source would, and a display source line wider than
the window keeps its own continuation row beside the block.

The Rust library has no Vim dependency. `scan` finds source ranges and
definitions, `math` writes inline rows, and `layout` composes display blocks.
`engine` caches typed layouts; the small `main` executable carries them over
JSON lines. Repeated formulas share their layout while keeping their own
source positions.

A consumer can use `Kernel::document` for a whole document, or `Kernel::layout`
for a formula its own parser has already found. Both take the consumer's cell
widths and command definitions, and share the same cache. Vim Script owns
presentation: conceal, source reveal, wrapping, cursor movement and the
resident process's lifetime. Cogitatum currently uses the markup scanner,
not a parser of its argument structure.

The library ships inside this plugin; it is not a separately released core.
Its Rust API and JSON protocol are experimental. A second consumer can use
the library or the process without adopting Vim's presentation choices.

## Performance

Median of 21 runs on an Apple M2 Pro:

| Document | Formulas | TeXinsitu, open | TeXinsitu, after an edit | UniTeX, first pass | UniTeX, repeat |
| --- | ---: | ---: | ---: | ---: | ---: |
| amsmath user's guide (102 KB) | 139 | 5.2 ms | 1.4 ms | 7.3 ms | 2.4 ms |
| AMS short math guide (83 KB) | 96 | 5.3 ms | 1.4 ms | 11.0 ms | 2.9 ms |
| A physics paper (119 KB) | 710 | 8.9 ms | 2.9 ms | 17.6 ms | 8.5 ms |

TeXinsitu times cover the whole round trip: starting the kernel (on open),
scanning the document, collecting definitions, two-dimensional layout and the
reply to Vim. UniTeX times cover only translating the same formula strings
inside an already running Deno, one-row output, excluding the runtime's
startup of about 80 ms.

A generated workload of 600 unique inline formulas and 100 display equations
also measures the Vim side. On the same machine, seven runs gave these medians,
including redraw in an isolated 45-row terminal Vim:

| Action | Time |
| --- | ---: |
| Open and paint | 83.3 ms |
| Enter or leave a formula | 5.9 ms |
| Resize between 40, 75 and 100 columns | 50.0 ms |
| Edit prose on a formula's line, before reparsing | 6.0 ms |

The kernel round trip for this workload was 12.3 ms on open and
2.3 ms after one formula edit; those times exclude Vim painting. With 200
unchanged command definitions, resident RSS after 2,000 formula edits was
about 3.5 MiB with shared definitions. RSS depends on the OS and
allocator; it is a process sample, not a cache-size guarantee.

## Development

```sh
sh tests/run.sh
git diff --check
```

runs the kernel unit tests, the renderer tests and real screen tests in a
terminal Vim. The renderer tests check the KaTeX screenshotter corpus and keep
the test cases of kokic's [UniTeX](https://github.com/kokic/UniTeX) as a
baseline; each deliberate difference states its reason.
The Rust tests also exercise the public library directly, independently of
the executable.
Use `TEXINSITU_VIM=/path/to/vim sh tests/run.sh` to test another Vim binary.

To measure the current kernel with generated source rather than a local
manuscript:

```sh
cargo build --release --manifest-path kernel/Cargo.toml
python3 tests/benchmark.py --runs 7 --vim vim --memory
```

This prints kernel cold-open and warm-edit medians, screen timings in an
isolated Vim, and resident RSS samples on macOS/Linux. It uses only generated
source. Omit `--vim` and `--memory` for kernel timings alone; `--kernel` can
point to another kernel build for comparison.

`python3 symbols.py`, run in `kernel/` with TeX Live and utftex installed,
regenerates `kernel/src/symbols.tsv`.

## License

The code is MIT-licensed. Symbol data comes from
[unicode-math](https://github.com/latex3/unicode-math) (LPPL 1.3c) and
UniTeX (MIT), and the early layout
followed Bart Pieters'
[libtexprintf](https://github.com/bartp5/libtexprintf). Generated tables and
test cases retain their upstream notices; see [THIRD_PARTY.md](THIRD_PARTY.md).

## Related

TeXinsitu is one of a small family of Vim plugins for writing: [MdToc.vim](https://github.com/BaksiLi/mdtoc.vim), [WebPreview.vim](https://github.com/BaksiLi/webpreview.vim) and [TeXinsitu.vim](https://github.com/BaksiLi/texinsitu.vim). Each works on its own. They were developed together in [Onesimus Vim](https://github.com/BaksiLi/Onesimus), a Vim distribution for scientific writing, which shows them combined.

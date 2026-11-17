# TeXinsitu specimens

Open this file in Vim to compare your font and terminal. Move into a formula
to see its source. The rendered characters come from Unicode; letter scripts
also use Vim highlight attributes.

## Characters and scripts

Numeric scripts: $x^2+y_{10}$. Letter scripts: $P_{\mathrm{avg}}^{\mathrm{tot}}$.

Greek, minus and a derivative: $\alpha-\beta+\partial_x u$.

Distinct alphabets: $\mathbb{R}\supset\mathcal{F}\ne\mathscr{F}$.

Composed and combining accents: $\text{\'a \"o \H{e}}$.

## Prose, units and wrapping

A $64^\circ$ YX LiNbO$_3$ substrate has $\SI{125}{nm}$ electrodes.

Mixed writing: 中文 $z^2$ tail $\nu$。 Closing punctuation should stay with
the formula when the window becomes narrow.

## Display

\[
 E=\frac{a^2+b^2}{N}, \qquad
 \sigma=\sqrt{E}.
\]

Display rows use the equation's source lines. A narrow window can leave a
formula as source rather than draw a partial grid. Its original source still
determines the height of wrapped lines.

#!/usr/bin/env python3
"""Text-grid layout, argument scope and literal source boundaries."""
import json
import os
from pathlib import Path
import re
import subprocess
import unicodedata
import unittest

ROOT = Path(__file__).resolve().parents[1]
KERNEL = os.environ.get("TEXINSITU_KERNEL", str(ROOT / "kernel/target/release/texinsitu"))


class Kernel:
    """A resident kernel, spoken to as Vim does."""

    def __init__(self):
        self.process = subprocess.Popen([KERNEL],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.serial = 0

    def __call__(self, lines, filetype, context="", cells=None):
        self.serial += 1
        request = {"id": self.serial, "lines": lines, "filetype": filetype, "context": context}
        if cells is not None:
            request["cells"] = cells
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        reply = json.loads(self.process.stdout.readline())
        assert reply["id"] == self.serial
        return reply["formulas"]

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.process.stdout.close()


formulas = Kernel()
unittest.addModuleCleanup(formulas.close)

# Structurally equivalent regressions with manuscript names/data removed.
DISPLAY = [
    r"\newcommand{\statev}{\bm{\phi}}",
    r"\begin{equation}",
    r" \widetilde r_{ab}=\frac{\sum_{t=1}^{S}|\widetilde H(\nu_t;\widetilde\statev_{ab})-H_a^*(\nu_t)|^2}",
    r" {\sum_{t=1}^{S}|H_a^*(\nu_t)|^2}.",
    r" \label{eq:ratio}",
    r"\end{equation}",
    r"\begin{samepage}",
    "Text between equations.",
    r"\begin{equation}",
    r" J_{\mathrm{mix}}=\frac{1}{B_c}\sum_{a=1}^{B_c}\min_{1\leq b\leq M_{\mathrm h}}\widetilde r_{ab},",
    r" \label{eq:aggregate}",
    r"\end{equation}",
    r"\end{samepage}",
]


def pieces(row):
    """Pieces as text: subscripts as _[..], superscripts as ^[..]."""
    return "".join({"": "{}", "sub": "_[{}]", "sup": "^[{}]"}[kind].format(text)
                   for text, kind in row)


def line(value):
    return pieces(value["segments"])


def grid(value):
    return [pieces(row) for row in value["rows"]]


class PersistentProtocolTest(unittest.TestCase):
    def setUp(self):
        self.kernel = Kernel()
        self.addCleanup(self.kernel.close)

    def test_invalid_requests_do_not_interrupt_valid_replies(self):
        before = {"id": "before", "lines": [r"$x^2$"], "filetype": "tex"}
        after = {"id": {"buffer": 2, "tick": 7}, "lines": [r"$y^2$"], "filetype": "tex"}
        requests = [
            json.dumps(before),
            "{",
            json.dumps({"id": "invalid", "lines": r"$z^2$", "filetype": "tex"}),
            json.dumps(after),
        ]
        result = subprocess.run([KERNEL],
                                input="\n".join(requests) + "\n", capture_output=True,
                                text=True, timeout=5, check=True)
        replies = [json.loads(reply) for reply in result.stdout.splitlines()]
        self.assertEqual([reply["id"] for reply in replies], [before["id"], after["id"]])
        self.assertEqual([[line(formula) for formula in reply["formulas"]] for reply in replies],
                         [["x²"], ["y²"]])

    def test_buffer_filetype_and_context_changes_are_document_local(self):
        source = [r"% $\bufferword$", r"\newcommand{\bufferword}{b}", r"$\bufferword$"]
        context = r"\newcommand{\bufferword}{a}"
        original = self.kernel(source, "tex", context)
        self.assertEqual([(formula["first"][0], line(formula)) for formula in original],
                         [(3, "b")])
        for filetype in ["markdown", "cogitatum"]:
            with self.subTest(filetype=filetype):
                result = self.kernel(source, filetype, context)
                self.assertEqual([(formula["first"][0], line(formula)) for formula in result],
                                 [(1, "bufferword"), (3, "bufferword")])
        result = self.kernel([r"$\bufferword$"], "tex", r"\newcommand{\bufferword}{c}")
        self.assertEqual([line(formula) for formula in result], ["c"])
        self.assertEqual(self.kernel([], "tex"), [])
        result = self.kernel([r"$\bufferword$"], "tex")
        self.assertEqual([line(formula) for formula in result], ["bufferword"])
        self.assertEqual(self.kernel(source, "tex", context), original)

    def test_failed_formula_is_retried_after_macro_changes(self):
        source = [r"$\fragment{q}$"]
        for context, expected in [
            (r"\newcommand{\fragment}[1]{#2}", []),
            (r"\newcommand{\fragment}[1]{#1^2}", ["q²"]),
            (r"\newcommand{\fragment}[1]{#2}", []),
            (r"\newcommand{\fragment}[1]{\frac{#1}{2}}", ["q/2"]),
            (r"\newcommand{\fragment}[1]{#1^2}", ["q²"]),
        ]:
            with self.subTest(context=context):
                self.assertEqual([line(formula) for formula in self.kernel(source, "tex", context)],
                                 expected)

    def test_macro_defaults_and_argument_counts_are_part_of_the_scope(self):
        source = [r"$\alias$"]
        alias = r"\newcommand{\alias}{\selection}"
        for definition, expected in [
            (r"\newcommand{\selection}[1][a]{#1^2}", ["a²"]),
            (r"\newcommand{\selection}[1][b]{#1^2}", ["b²"]),
            (r"\newcommand{\selection}[2][a]{#1^2}", []),
            (r"\newcommand{\selection}[1][a]{#1^2}", ["a²"]),
            (r"\newcommand{\selection}[2][a]{#1^2}", []),
            (r"\newcommand{\selection}[1][b]{#1^2}", ["b²"]),
        ]:
            with self.subTest(definition=definition):
                result = self.kernel(source, "tex", alias + definition)
                self.assertEqual([line(formula) for formula in result], expected)

    def test_display_mode_separates_successful_and_failed_cached_layouts(self):
        for raw, inline, displayed in [
            (r"\[\frac{a}{b}\]", "a/b", ["a", "─", "b"]),
            (r"\[\begin{matrix}a&b\\c&d\end{matrix}\]", None, ["a b", "c d"]),
        ]:
            for standalone in [False, True, False, True]:
                with self.subTest(raw=raw, standalone=standalone):
                    source = [r"$x^2$", raw if standalone else "Text " + raw, r"$z^2$"]
                    result = self.kernel(source, "tex")
                    rendered = standalone or inline is not None
                    self.assertEqual(len(result), 3 if rendered else 2)
                    self.assertEqual([line(result[0]), line(result[-1])], ["x²", "z²"])
                    if rendered:
                        self.assertEqual(result[1]["display"], standalone)
                        if standalone:
                            self.assertEqual(grid(result[1]), displayed)
                        else:
                            self.assertEqual(line(result[1]), inline)

    def test_unclosed_source_does_not_continue_into_another_request(self):
        complete = [r"$x^2$", r"\[\frac{a}{b}\]"]
        for filetype, unfinished in [
            ("tex", [r"$\frac{a}{b}"]),
            ("tex", [r"\begin{equation}", r"\frac{a}{b}"]),
            ("tex", [r"\begin{verbatim}", r"$hidden^2$"]),
            ("tex", [r"\newcommand{\pending}[1]{#1"]),
            ("markdown", ["```tex", r"$hidden^2$"]),
            ("cogitatum", ["```tex", r"$hidden^2$"]),
        ]:
            with self.subTest(filetype=filetype, unfinished=unfinished):
                self.assertEqual(self.kernel(unfinished, filetype), [])
                result = self.kernel(complete, filetype)
                self.assertEqual(len(result), 2)
                self.assertEqual(line(result[0]), "x²")
                self.assertEqual(grid(result[1]), ["a", "─", "b"])

    def test_cell_settings_relayout_cached_formulas_and_reset_when_omitted(self):
        source = [r"$\alpha$", r"\[\frac{\alpha}{b}\]"]
        original = self.kernel(source, "tex")
        self.assertEqual([formula["width"] for formula in original], [1, 1])
        for cells, width in [
            ({"wide": True}, 2),
            ({"wide": True, "overrides": [[0x03B1, 0x03B1, 1]]}, 1),
            ({"wide": True, "overrides": [[0x03B1, 0x03B1, 1], [0x2500, 0x257F, 1]]}, 1),
            ({"overrides": [[0x03B1, 0x03B1, 2]]}, 2),
        ]:
            with self.subTest(cells=cells):
                result = self.kernel(source, "tex", cells=cells)
                self.assertEqual([formula["display"] for formula in result], [False, True])
                self.assertEqual([formula["width"] for formula in result], [width, width])
                self.assertEqual(line(result[0]), "α")
                self.assertEqual(grid(result[1])[0], "α")
                self.assertTrue(grid(result[1])[1].strip(), "a fraction must keep its bar")
        self.assertEqual(self.kernel(source, "tex"), original)

    def test_cell_overrides_change_cached_rows_even_when_width_is_unchanged(self):
        source = [r"\[\frac{x}{y}+z\]"]
        for stroke_width, bar in [(1, "─"), (2, "−"), (1, "─"), (2, "−")]:
            with self.subTest(stroke_width=stroke_width):
                result = self.kernel(source, "tex",
                                     cells={"overrides": [[0x2500, 0x2500, stroke_width]]})
                self.assertEqual(len(result), 1)
                self.assertEqual(result[0]["width"], 3)
                self.assertEqual(grid(result[0]), ["x", bar + "+z", "y"])
        result = self.kernel(source, "tex")
        self.assertEqual(grid(result[0]), ["x", "─+z", "y"])

    def test_duplicate_layouts_keep_positions_when_moved_between_requests(self):
        source = [r"中文 $z_a$ and $z_a$"]
        original = self.kernel(source, "tex")
        for lines, expected in [
            (source, [([1, 8], [1, 13], [[1, 8, 5]]), ([1, 18], [1, 23], [[1, 18, 5]])]),
            (["Heading", r"$z_a$ $z_a$"],
             [([2, 1], [2, 6], [[2, 1, 5]]), ([2, 7], [2, 12], [[2, 7, 5]])]),
        ]:
            with self.subTest(lines=lines):
                result = self.kernel(lines, "tex")
                self.assertEqual([line(formula) for formula in result], ["z_[a]", "z_[a]"])
                self.assertEqual([(formula["first"], formula["last"], formula["spans"])
                                  for formula in result], expected)
        self.assertEqual(self.kernel(source, "tex"), original)


class MathRendererTest(unittest.TestCase):
    def test_public_specimen_sheet(self):
        source = (ROOT / "examples/specimens.md").read_text().splitlines()
        expected = ["x²+y₁₀", "P_[avg]^[tot]", "α−β+∂_[x]u", "ℝ⊃ℱ≠𝓕",
                    "á ö e̋", "64°", "₃", "125\u00a0nm", "z²", "ν"]
        for filetype in ("tex", "markdown", "cogitatum"):
            with self.subTest(filetype=filetype):
                values = formulas(source, filetype)
                self.assertEqual([line(value) for value in values if not value["display"]],
                                 expected)
                displays = [grid(value) for value in values if value["display"]]
                self.assertEqual(displays,
                                 [["  a²+b²        ─", "E=─────,    σ=√E.", "    N"]])

    def one(self, source, context=""):
        result = formulas([source], "tex", context)
        self.assertEqual(len(result), 1, source)
        return result[0]

    def test_inline_math_is_one_row_with_marked_scripts(self):
        for source, expected in [
            (r"$Q^*$", "Q^[*]"), (r"$Q^\*$", "Q^[*]"), (r"$Q^{\*}$", "Q^[*]"),
            (r"$A_{\mathrm h}=12$", "A_[h]=12"),
            (r"$\Delta u_\sigma$", "Δu_[σ]"),
            (r"$z_{ab}^{r+1}$", "z_[ab]^[r+1]"),
            (r"$v^{AB}+u_{CD}$", "v^[AB]+u_[CD]"),
            (r"$x^2+y_{10}$", "x²+y₁₀"),
            (r"$e^{-x^2/2}$", "e^[−x²/2]"),
        ]:
            with self.subTest(source=source):
                value = self.one(source)
                self.assertFalse(value["display"])
                self.assertEqual(line(value), expected)

    def test_unbraced_accents_and_textual_indices(self):
        value = self.one(
            r"$\widetilde P_{\mathrm{SRC}}+3\widetilde P_{\mathrm{aux}}\leq900$")
        self.assertEqual(line(value), "P̃_[SRC]+3P̃_[aux]≤900")
        self.assertEqual(value["width"], len("P̃SRC+3P̃aux≤900") - 2)

    def test_macro_arguments_defaults_and_main_source(self):
        context = (r"\newcommand{\statev}{\bm{\phi}}"
                   r"\newcommand{\pair}[2][q]{#1+#2}")
        value = self.one(r"$\widetilde\statev_a\in\Phi_{\mathrm{trial}}$", context)
        self.assertTrue(line(value).endswith("_[a]∈Φ_[trial]"))
        self.assertNotIn("\\", line(value))
        self.assertEqual(line(self.one(r"$\pair{r}$", context)), "q+r")
        self.assertEqual(line(self.one(r"$\pair[s]{r}$", context)), "s+r")
        self.assertEqual(formulas(
            [r"\newcommand{\bad}[1]{#2}", r"$a+\bad{r}$"], "tex"), [])
        other = self.one(r"$\statev$", r"\newcommand{\statev}{q}")
        self.assertEqual(line(other), "q", "macro meanings are document-local")

    def test_marker_text_is_literal(self):
        value = self.one(r"$\text{TEXINSITUBASELINE}+z_a$")
        self.assertTrue(line(value).endswith("TEXINSITUBASELINE+z_[a]"))

    def test_operators_fractions_and_roots_stay_linear_inline(self):
        context = r"\DeclareMathOperator*{\pick}{pick}"
        self.assertEqual(line(self.one(r"$\pick_a x$", context)), "pick_[a] x")
        for source, expected in [
            (r"$\frac{a+b}{c+d}$", "(a+b)/(c+d)"),
            (r"$\frac{1}{N_b}$", "1/N_[b]"),
            (r"$\frac{a}{b}^2$", "(a/b)²"),
            (r"$\sqrt[3]{z^2+w^2}$", "∛(z²+w²)"),
            (r"$\sum_{i=1}^{N}x_i$", "∑_[i=1]^[N]x_[i]"),
        ]:
            with self.subTest(source=source):
                self.assertEqual(line(self.one(source)), expected)

    def test_display_math_keeps_the_text_grid(self):
        value = formulas([r"\[", r"\frac{a+b}{c+d}^2", r"\]"], "tex")[0]
        self.assertTrue(value["display"])
        self.assertIn("───", "\n".join(grid(value)))
        self.assertIn("a+b", "\n".join(grid(value)))
        value = formulas([r"\begin{equation}", r"Q_{\mathrm h}^*", r"\end{equation}"], "tex")[0]
        self.assertEqual(grid(value), ["Q_[h]^[*]"])

    def test_display_block_scripts_ignore_spaces_newlines_and_comments(self):
        for base, expected, width in [
            (r"\frac{a}{b}", ["a^[n]", "─ +z", "b_[i]"], 4),
            (r"\sqrt{x}", [" ─^[n]", "√x_[i]+z"], 5),
            (r"\left(\frac{a}{b}\right)", ["⎛a⎞^[n]", "⎜─⎟ +z", "⎝b⎠_[i]"], 6),
            (r"\begin{pmatrix}a\\b\end{pmatrix}", ["⎛a⎞^[n]", "⎝b⎠_[i]+z"], 6),
            (r"{\frac{a}{b}+c}", ["a  ^[n]", "─+c +z", "b  _[i]"], 6),
        ]:
            for gap in ["", " ", "\n", " % comment\n "]:
                for script in ["_i^n", "^n_i"]:
                    with self.subTest(base=base, gap=gap, script=script):
                        source = r"\[" + base + gap + script + r"+z\]"
                        result = formulas(source.splitlines(), "tex")
                        self.assertEqual(len(result), 1)
                        self.assertEqual(grid(result[0]), expected)
                        self.assertEqual(result[0]["width"], width)
        self.assertEqual(grid(self.one(r"\[x _i ^n+y^2\]")), ["x_[i]^[n]+y²"])

    def test_display_style_declarations_after_scripts_keep_their_scope(self):
        for source, expected in [
            (r"\textstyle\sum\limits_i\displaystyle\frac{a}{b}+\sqrt{x}",
             [" a  ─", "∑─+√x", "ib"]),
            (r"\textstyle{\displaystyle\frac{a}{b}}_i\displaystyle\frac{c}{d}",
             ["a c", "─ ─", "b_[i]d"]),
            (r"\textstyle{\sum\limits_i\displaystyle\frac{a}{b}}+\frac{c}{d}",
             [" a", "∑─+c/d", "ib"]),
        ]:
            with self.subTest(source=source):
                self.assertEqual(grid(self.one(r"\[" + source + r"\]")), expected)

    def test_display_grid_grows_only_for_fractions_and_limits(self):
        value = formulas([r"\begin{equation}",
                          r" Z(\nu;\phi)=\frac{A(\nu)}{B(\nu)}=R_{12}^{\mathrm{all}}(\nu;\phi),",
                          r" \qquad \nu=2\pi f.", r"\end{equation}"], "tex")[0]
        rows = grid(value)
        self.assertEqual(len(rows), 3, rows)
        self.assertIn("=R₁₂^[all](ν;ϕ),", rows[1])
        self.assertEqual(rows[0].strip(), "A(ν)")
        value = formulas([r"\[", r"J=\frac{1}{B_c}\sum_{a=1}^{B_c}r_{ab}", r"\]"], "tex")[0]
        rows = grid(value)
        # Limits stay stacked over and under the operator.
        top, middle, bottom = ["".join(text for text, _ in row) for row in value["rows"]]
        column = middle.index("∑")
        self.assertEqual(top[column - 1:column + 2].strip(), "Bc")
        self.assertEqual(bottom[column - 1:column + 2], "a=1")
        self.assertTrue(any("r_[ab]" in row for row in rows))
        # Script marks take no width, so the fraction stays centred on its bar.
        plain = ["".join(text for text, _ in row) for row in value["rows"]]
        bar = next(row for row in plain if "─" in row)
        start, end = bar.index("─"), bar.rindex("─")
        numerator = plain[plain.index(bar) - 1]
        self.assertTrue(start <= numerator.index("1") <= end, plain)

    def test_set_builder_with_delimiters_keeps_scripts_on_baseline(self):
        context = r"\newcommand{\pv}{\bm{\phi}}\newcommand{\Na}{N_{\mathrm{src}}}"
        value = formulas([r"\begin{equation}",
                          r" \Omega_{\mathrm{ok}}=\left\{\pv\in\Omega_{\mathrm{all}}:",
                          r" \begin{aligned}&(A,B_h)\in\mathbb{Z}^2,\\&\Na\leq50\end{aligned}",
                          r" \right\}.", r"\end{equation}"], "tex", context)[0]
        rows = grid(value)
        # The brace is exactly as tall as the two aligned rows.
        self.assertEqual(len(rows), 2, rows)
        self.assertIn("Ω_[ok]=⎱", rows[1])
        self.assertIn("Ω_[all] : N_[src]≤50", rows[1])
        self.assertIn("⎰", rows[0])
        self.assertIn("B_[h]", rows[0])
        self.assertTrue(rows[0].endswith("⎱") and rows[1].endswith("⎰."), rows)
        self.assertNotIn("left", "".join(rows))
        for source, expected in [
            (r"$\{x_i\}+1$", "{x_[i]}+1"), (r"$\{x_i\}_{i=1}^N$", "{x_[i]}_[i=1]^[N]"),
            (r"$\left\{a_i\right\}$", "{a_[i]}"), (r"$\big|x\big|$", "|x|"),
        ]:
            self.assertEqual(line(self.one(source)), expected, source)

    def test_unknown_incomplete_or_recursive_formula_stays_whole_source(self):
        for source in [
            r"$\frac{x}{$", r"$\input{private}$",
            r"\newcommand{\cycle}{\cycle}" + "\n" + r"$a+\cycle$",
        ]:
            self.assertEqual(formulas(source.splitlines(), "tex"), [], source)

    def test_unknown_macro_renders_as_name_not_silence(self):
        # A custom command is a symbol the author defined elsewhere; show its
        # name so the formula stays readable instead of disappearing.
        value = formulas([r"$\pvec_k$"], "tex")[0]
        self.assertEqual(value["segments"][0], ["pvec", ""])
        # A call-shaped unknown keeps its argument visible: custom(y²).
        value = formulas([r"$x+\custom{y^2}$"], "tex")[0]
        self.assertEqual(value["segments"], [["x+custom(y²)", ""]])

    def test_siunitx_commands_render_as_value_and_unit(self):
        # \SI/\si/\num/\ang typeset a number plus unit; render that directly
        # in prose and inside math, without needing siunitx loaded.
        # Number and unit are joined by a no-break space (SI Brochure), so a
        # wrapped line never leaves a unit alone; a plane angle takes none.
        cases = {
            r"$\SI{2.5}{\micro\metre}$": "2.5\u00a0μm",
            r"\SI{250}{nm}": "250\u00a0nm",
            r"$\SI{1000}{\kilogram\per\cubic\metre}$": "1000\u00a0kg/m³",
            r"$\SI{433.92}{MHz}$": "433.92\u00a0MHz",
            r"\SI{64}{\degree}": "64°",
        }
        for source, expected in cases.items():
            value = formulas([source], "tex")[0]
            self.assertEqual("".join(t for t, _ in value["segments"]), expected)

    def test_tex_spacing_not_typed_spacing(self):
        # TeX ignores spaces typed in math: the same formula renders the same
        # however the author spaced it.
        for source, expected in [
            (r"$a + b = c$", "a+b=c"), (r"$a+b=c$", "a+b=c"),
            (r"$\alpha x$", "αx"), (r"$\sin x+\sin(x)$", "sin x+sin(x)"),
            (r"$\max_a x$", "max_[a] x"), (r"$\operatorname{Re}Y$", "Re Y"),
            (r"$x\,\mathrm{d}x$", "x dx"), (r"$a\!b$", "ab"), (r"$a~b$", "a b"),
            (r"$\text{if } b$", "if b"), (r"$\{x:x>0\}$", "{x : x>0}"),
            (r"$x_{a:b}$", "x_[a:b]"), (r"$\_x$", "_x"),
            (r"$\frac{a}b \cdot \frac{b}a = 1$", "a/b⋅b/a=1"), (r"$\arg\bar c$", "arg c̄"), (r"$\alpha\frac12$", "α1/2"),
        ]:
            with self.subTest(source=source):
                self.assertEqual(line(self.one(source)), expected)

    def test_environments(self):
        def rows(*lines):
            return grid(formulas([r"\[", *lines, r"\]"], "tex")[0])
        self.assertEqual(rows(r"f=\begin{cases} a+1 & b\text{ odd}\\",
                              r"a & b=0\\ a-1 & \text{else}\end{cases}"),
                         ["  ⎧a+1   b odd", "f=⎨a     b=0", "  ⎩a−1   else"])
        self.assertEqual(rows(r"\begin{rcases} a & b\\ c & d\end{rcases}\Rightarrow x"),
                         ["a   b⎱", "c   d⎰⇒x"])
        self.assertEqual(rows(r"\begin{gathered} x=1\\ y=22 \end{gathered}"),
                         [" x=1", "y=22"])
        # Ragged rows, a final \\, \nonumber and \tag all as TeX allows.
        self.assertEqual(rows(r"\begin{align} a &= 1 & b &= 2 \\",
                              r"\nonumber a &\neq b \tag{3}\\ \end{align}"),
                         ["a=1   b=2", "a≠b"])
        for source in [r"\begin{split} a&=b\\&=c\end{split}",
                       r"\begin{alignedat}{2} a&=b & c&=d\end{alignedat}",
                       r"\begin{array}{|rl:c||}1&2&3\end{array}"]:
            self.assertTrue(formulas([r"\[", source, r"\]"], "tex"), source)

    def test_latex_commands_without_a_symbol(self):
        for source, expected in [
            (r"$\mathit{Ax}+\textcolor{red}{y}+\boxed{z}$", "Ax+y+z"),
            (r"$a\overset{!}{=}b$", "a=^[!]b"),
            (r"$\dfrac{\dfrac{a}{b}}{c}$", "(a/b)/c"),
            (r"$p\land q\lor r$", "p∧q∨r"),
            (r"$a\overset{\Delta}{=}b\stackrel{?}{=}c\overset{\mathrm{def}}{=}d$", "a≜b≟c≝d"),
            (r"$\text{Gal}(\bar a/a)\rarr\N\R$", "Gal(ā/a)→ℕℝ"),
            (r"$x\coloneqq\varGamma\plusmn\cfrac{1}{2}$", "x≔Γ±1/2"),
        ]:
            with self.subTest(source=source):
                self.assertEqual(re.sub(r"\s", "", line(self.one(source))),
                                 re.sub(r"\s", "", expected))

    def test_unitex_upstream_baseline(self):
        # Every difference from UniTeX is deliberate and says why in "policy".
        corpus = json.loads((Path(__file__).parent / "support/unitex_upstream.json").read_text())
        for case in corpus["cases"]:
            with self.subTest(source=case["source"]):
                tex = case["tex"] if case["tex"].startswith("$") else "$" + case["tex"] + "$"
                value = self.one(tex)
                ours = grid(value) if value["display"] else line(value)
                self.assertEqual(ours, case["ours"])
                if "policy" not in case:
                    self.assertEqual(re.sub(r"\s", "", ours), re.sub(r"\s", "", case["upstream"]))

    def test_katex_screenshotter_corpus(self):
        corpus = json.loads((Path(__file__).parent / "support/katex_screenshotter.json").read_text())
        rendered = 0
        for case in corpus["cases"]:
            value = formulas([r"\[", *case["tex"].splitlines(), r"\]"], "tex")
            if value:
                rendered += 1
                text = "\n".join(grid(value[0]))
                self.assertIsNone(re.search(r"\\[A-Za-z]|\b(?:left|right|begin|mathrm)\b", text),
                                  case["name"] + ": " + text)
        # Ratchet: raise this when more cases render; the rest are KaTeX
        # extensions (commutative diagrams, \reflectbox, \cancel, \llap, logos,
        # \includegraphics) or layouts without a text-cell equivalent.
        self.assertGreaterEqual(rendered, 117)

    def test_text_accents_render_and_normalize_in_both_layouts(self):
        source = r'''\text{\'\i \^\j \"o \H{u} \c{c}}'''
        inline = self.one("$" + source + "$")
        self.assertEqual(line(inline), "í ĵ ö ű ç")
        display = formulas([r"\[", source, r"\]"], "tex")[0]
        self.assertEqual(grid(display), ["í ĵ ö ű ç"])
        self.assertEqual(line(inline), unicodedata.normalize("NFC", line(inline)))

    def test_wide_accents_spread_over_word_groups(self):
        # \widetilde over a multi-letter group applies the combining mark to
        # every letter instead of leaving a floating tilde above.
        value = formulas([r"$\widetilde\pvec_k\in\Omega_{\mathrm{free}}$"], "tex")[0]
        text = "".join(t for t, _ in value["segments"])
        self.assertEqual(text, unicodedata.normalize("NFC", "p\u0303v\u0303e\u0303c\u0303k∈Ωfree"))
        # Over a symbol the narrow accent stays: the tilde is not lost.
        value = formulas([r"$\widetilde\statev_{ab}+\bar a+\bar{xy}$"], "tex",
                         r"\newcommand{\statev}{\bm{\phi}}")[0]
        self.assertEqual(line(value), "𝛟\u0303_[ab]+ā+x̄ȳ")

        value = formulas(
            [r"$\widetilde N_{\mathrm{in}}+2\widetilde N_{\mathrm{out}}\leq500$"],
            "tex")[0]
        text = "".join(t for t, _ in value["segments"])
        self.assertEqual(text, unicodedata.normalize("NFC", "N\u0303in+2N\u0303out≤500"))

    def test_degree_sign(self):
        self.assertEqual(line(self.one(r"$128^\circ$")), "128°")
        self.assertEqual(line(self.one(r"$x^{\circ}$")), "x°")

    def test_complete_equation_blocks_and_labels(self):
        result = formulas(DISPLAY, "tex")
        self.assertEqual([v["first"][0] for v in result], [2, 9])
        self.assertEqual([v["last"][0] for v in result], [6, 12])
        self.assertEqual([v["label"] for v in result], ["eq:ratio", "eq:aggregate"])
        for value in result:
            self.assertTrue(value["display"])
            self.assertGreaterEqual(len(value["rows"]), 3)
            self.assertIn("──", "\n".join(grid(value)))
            self.assertNotIn("\\", "".join(grid(value)))
            first, last = value["first"][0], value["last"][0]
            self.assertEqual([s[0] for s in value["spans"]], list(range(first, last + 1)))

    def test_aligned_environment_is_one_formula(self):
        result = formulas(
            [r"\begin{align}", r"a&=\alpha\\", r"b&=a^2", r"\end{align}"], "tex")
        self.assertEqual(len(result), 1)
        self.assertTrue(result[0]["display"])
        self.assertIn("a=α", grid(result[0]))
        self.assertIn("b=a²", grid(result[0]))

    def test_tex_comments_definitions_and_literal_environments(self):
        lines = [
            r"% $comment^2$",
            r"\newcommand{\name}{$definition^2$}",
            r"\def\name#1{$definition^2$}",
            r"\begin{verbatim}$code^2$\end{verbatim}",
            r"\begin{lstlisting}$code^2$\end{lstlisting}",
            r"\url{https://host/$code$}",
            r"\textbf{Text $x^2$} and \(y_i\)",
            r"$unfinished",
        ]
        self.assertEqual([f["first"][0] for f in formulas(lines, "tex")], [7, 7])

    def test_markup_code_and_currency(self):
        lines = [
            r"50% text $z^2$ and \$5.00.",
            r"`$inline$` ``$also code$``",
            "~~~latex", *DISPLAY, "~~~",
            "$unfinished", "", "new paragraph $q_j$",
        ]
        for kind in ["markdown", "cogitatum"]:
            result = formulas(lines, kind)
            self.assertEqual([f["first"][0] for f in result], [1, len(lines)])
            self.assertEqual(line(result[0]), "z²")

    def test_utf8_columns_cover_exact_source(self):
        line = "中文 $z^2$ tail $\\nu$。"
        result = formulas([line], "markdown")
        for value, raw in zip(result, ["$z^2$", r"$\nu$"]):
            first, last = value["first"], value["last"]
            self.assertEqual(line.encode()[first[1] - 1:last[1] - 1].decode(), raw)


if __name__ == "__main__":
    unittest.main()

//! Find formulas and command definitions without executing any TeX.
//!
//! TeX sources are read the way TeX reads them: comments, verbatim text and
//! definition bodies hold no formulas, and math never crosses a blank line,
//! so a `$` typed but not yet closed affects only its own paragraph.

use serde::Serialize;
use std::collections::BTreeMap;

/// A command definition: argument spec (`[` optional, `{` mandatory),
/// default for the optional argument, and body.
pub type Definition = (String, Option<String>, String);
pub type Definitions = BTreeMap<String, Definition>;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Formula {
    #[serde(skip)]
    pub raw: String,
    /// [line, byte column], both from 1, of the first and one past the last byte.
    pub first: [usize; 2],
    pub last: [usize; 2],
    /// [line, byte column, byte length] of each non-empty source line.
    pub spans: Vec<[usize; 3]>,
    pub display: bool,
    pub label: String,
}

const MATH_ENVS: &[&str] = &[
    "math", "displaymath", "equation", "align", "alignat", "flalign", "gather", "multline",
    "eqnarray",
];
const LITERAL_ENVS: &[&str] = &["verbatim", "Verbatim", "lstlisting", "minted", "comment"];

pub fn formulas(source: &str, filetype: &str, context: &str) -> (Definitions, Vec<Formula>) {
    let (defs, ranges) = if filetype == "tex" {
        let mut defs = definitions(context);
        let scan = Tex::scan(source);
        defs.extend(scan.defs);
        (defs, scan.formulas)
    } else {
        (Definitions::new(), markup(source))
    };
    (defs, place(source, ranges))
}

pub fn definitions(source: &str) -> Definitions {
    Tex::scan(source).defs
}

/// Turn byte ranges into editor positions; keep inline math on one line.
fn place(source: &str, ranges: Vec<(usize, usize)>) -> Vec<Formula> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line = |offset: usize| starts.partition_point(|&s| s <= offset) - 1;
    let mut found = Vec::new();
    for (start, end) in ranges {
        let raw = &source[start..end];
        let (top, bottom) = (line(start), line(end));
        let line_end = source[end..].find('\n').map_or(source.len(), |i| end + i);
        let standalone =
            source[starts[top]..start].trim().is_empty() && source[end..line_end].trim().is_empty();
        let display = standalone
            && (top != bottom || ["$$", "\\[", "\\begin"].iter().any(|p| raw.starts_with(p)));
        if top != bottom && !display {
            continue;
        }
        let spans = raw
            .split('\n')
            .enumerate()
            .filter(|(_, text)| !text.is_empty())
            .map(|(i, text)| {
                [top + i + 1, if i == 0 { start - starts[top] + 1 } else { 1 }, text.len()]
            })
            .collect();
        found.push(Formula {
            raw: raw.to_string(),
            first: [top + 1, start - starts[top] + 1],
            last: [bottom + 1, end - starts[bottom] + 1],
            spans,
            display,
            label: labels(raw),
        });
    }
    found
}

fn labels(raw: &str) -> String {
    let mut names = Vec::new();
    let mut rest = raw;
    while let Some(at) = rest.find("\\label") {
        rest = rest[at + 6..].trim_start();
        if let Some(body) = rest.strip_prefix('{') {
            if let Some(close) = body.find(['{', '}']).filter(|&i| body.as_bytes()[i] == b'}') {
                names.push(&body[..close]);
            }
        }
    }
    names.join(", ")
}

struct Tex<'a> {
    s: &'a str,
    b: &'a [u8],
    defs: Definitions,
    formulas: Vec<(usize, usize)>,
}

impl<'a> Tex<'a> {
    fn scan(s: &'a str) -> Self {
        let mut tex = Tex { s, b: s.as_bytes(), defs: Definitions::new(), formulas: Vec::new() };
        tex.prose();
        tex
    }

    fn prose(&mut self) {
        let mut i = 0;
        while i < self.b.len() {
            i = match self.b[i] {
                b'%' => self.comment(i),
                b'$' => {
                    let display = self.b.get(i + 1) == Some(&b'$');
                    let open = i + 1 + display as usize;
                    let close: &[u8] = if display { b"$$" } else { b"$" };
                    self.math(i, open, close)
                }
                b'\\' => self.command(i),
                _ => i + 1,
            }
        }
    }

    /// Record the formula from `start` if it closes, else read on after its opener.
    fn math(&mut self, start: usize, open: usize, close: &[u8]) -> usize {
        match self.math_end(open, close) {
            Some(end) => {
                self.formulas.push((start, end));
                end
            }
            None => open,
        }
    }

    fn command(&mut self, i: usize) -> usize {
        let end = self.control(i);
        let name = &self.s[i + 1..end];
        match name {
            "(" => self.math(i, end, b"\\)"),
            "[" => self.math(i, end, b"\\]"),
            "begin" => {
                let Some((env, after)) = self.environment_name(end) else { return end };
                let base = env.trim_end_matches('*');
                if MATH_ENVS.contains(&base) {
                    match self.environment_end(after, env) {
                        Some(close) => {
                            self.formulas.push((i, close));
                            close
                        }
                        None => after,
                    }
                } else if LITERAL_ENVS.contains(&base) {
                    let close = format!("\\end{{{env}}}");
                    self.s[after..].find(&close).map_or(self.b.len(), |at| after + at + close.len())
                } else {
                    after
                }
            }
            "verb" => {
                let open = end + (self.b.get(end) == Some(&b'*')) as usize;
                match self.s[open..].chars().next() {
                    Some(delimiter) if !delimiter.is_alphabetic() && delimiter != '\n' => {
                        let body = open + delimiter.len_utf8();
                        self.s[body..]
                            .find([delimiter, '\n'])
                            .map_or(self.b.len(), |at| body + at + 1)
                    }
                    _ => end,
                }
            }
            "url" | "path" | "href" => self.literal_group(end),
            "def" | "gdef" | "edef" | "xdef" => self.plain_definition(end),
            "newcommand" | "renewcommand" | "providecommand" => self.new_command(end),
            "DeclareMathOperator" => self.math_operator(end),
            "newenvironment" | "renewenvironment" => self.arguments(end, "*{[[{{").1,
            "SI" | "qty" => self.siunitx(i, end, "[{{"),
            "ang" => self.siunitx(i, end, "[{"),
            "si" | "unit" | "num" => self.siunitx(i, end, "{"),
            _ => end,
        }
    }

    /// End of the control sequence at `i`: a run of letters or one character.
    fn control(&self, i: usize) -> usize {
        let letters = self.b[i + 1..].iter().take_while(|c| c.is_ascii_alphabetic()).count();
        if letters > 0 {
            return i + 1 + letters;
        }
        i + 1 + self.s[i + 1..].chars().next().map_or(0, char::len_utf8)
    }

    fn comment(&self, i: usize) -> usize {
        self.s[i..].find('\n').map_or(self.b.len(), |at| i + at + 1)
    }

    /// A URL-like group, where `%` and `\` are ordinary characters.
    fn literal_group(&self, i: usize) -> usize {
        let j = self.blank(i);
        if self.b.get(j) != Some(&b'{') {
            return i;
        }
        let mut depth = 0usize;
        for (k, &c) in self.b[j..].iter().enumerate() {
            depth = match c {
                b'{' => depth + 1,
                b'}' if depth == 1 => return j + k + 1,
                b'}' => depth - 1,
                _ => depth,
            };
        }
        self.b.len()
    }

    fn blank(&self, i: usize) -> usize {
        let mut j = i;
        while j < self.b.len() && self.b[j].is_ascii_whitespace() {
            j += 1;
        }
        j
    }

    /// Index after the group opened at `i`, which holds `{`.
    fn group_end(&self, i: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut j = i;
        while j < self.b.len() {
            match self.b[j] {
                b'\\' => j = self.control(j),
                b'%' => j = self.comment(j),
                c => {
                    if c == b'{' {
                        depth += 1;
                    } else if c == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            return Some(j + 1);
                        }
                    }
                    j += 1;
                }
            }
        }
        None
    }

    /// A mandatory argument at `i`: a group's contents or one token.
    fn argument(&self, i: usize) -> Option<(&'a str, usize)> {
        let j = self.blank(i);
        match self.b.get(j)? {
            b'{' => self.group_end(j).map(|end| (&self.s[j + 1..end - 1], end)),
            b'}' | b'%' => None,
            b'\\' => {
                let end = self.control(j);
                Some((&self.s[j..end], end))
            }
            _ => {
                let end = j + self.s[j..].chars().next()?.len_utf8();
                Some((&self.s[j..end], end))
            }
        }
    }

    /// An optional `[...]` argument at `i`, if there is one.
    fn optional(&self, i: usize) -> Option<(&'a str, usize)> {
        let j = self.blank(i);
        if self.b.get(j) != Some(&b'[') {
            return None;
        }
        let mut k = j + 1;
        while k < self.b.len() {
            match self.b[k] {
                b']' => return Some((&self.s[j + 1..k], k + 1)),
                b'{' => k = self.group_end(k)?,
                b'\\' => k = self.control(k),
                _ => k += 1,
            }
        }
        None
    }

    /// Read arguments by spec (`*` star, `[` optional, `{` mandatory); stops
    /// at the first missing mandatory argument.
    fn arguments(&self, mut i: usize, spec: &str) -> (Vec<Option<&'a str>>, usize) {
        let mut values = Vec::new();
        for kind in spec.chars() {
            let value = match kind {
                '*' => {
                    let j = self.blank(i);
                    (self.b.get(j) == Some(&b'*')).then(|| (&self.s[j..j + 1], j + 1))
                }
                '[' => self.optional(i),
                _ => match self.argument(i) {
                    Some(found) => Some(found),
                    None => return (values, i),
                },
            };
            values.push(value.map(|(text, end)| {
                i = end;
                text
            }));
        }
        (values, i)
    }

    fn siunitx(&mut self, start: usize, end: usize, spec: &str) -> usize {
        let (values, after) = self.arguments(end, spec);
        if values.len() == spec.len() && !self.s[start..after].contains("\n\n") {
            self.formulas.push((start, after));
        }
        after
    }

    fn new_command(&mut self, end: usize) -> usize {
        let (values, after) = self.arguments(end, "*{[[{");
        if let [_, Some(name), count, default, Some(body)] = values[..] {
            let count = count.and_then(|c| c.trim().parse::<usize>().ok()).unwrap_or(0);
            let spec = if default.is_some() { "[" } else { "" }.to_string()
                + &"{".repeat(count.saturating_sub(default.is_some() as usize));
            self.define(name, spec, default.map(str::to_string), body.to_string());
        }
        after
    }

    fn math_operator(&mut self, end: usize) -> usize {
        let (values, after) = self.arguments(end, "*{{");
        if let [_, Some(name), Some(body)] = values[..] {
            self.define(name, String::new(), None, format!("\\operatorname{{{body}}}"));
        }
        after
    }

    fn define(&mut self, name: &str, spec: String, default: Option<String>, body: String) {
        let name = name.trim().trim_start_matches('\\');
        if !name.is_empty() {
            self.defs.insert(name.to_string(), (spec, default, body));
        }
    }

    /// `\def\name<parameters>{body}`: everything up to and including the body.
    fn plain_definition(&self, mut i: usize) -> usize {
        while i < self.b.len() {
            match self.b[i] {
                b'{' => return self.group_end(i).unwrap_or(self.b.len()),
                b'\\' => i = self.control(i),
                b'%' => i = self.comment(i),
                _ => i += 1,
            }
        }
        i
    }

    fn environment_name(&self, i: usize) -> Option<(&'a str, usize)> {
        let j = self.blank(i);
        if self.b.get(j) != Some(&b'{') {
            return None;
        }
        let end = self.group_end(j)?;
        Some((self.s[j + 1..end - 1].trim(), end))
    }

    /// Where math opened before `i` closes with `close`. Groups are passed
    /// whole, so `\text{$x$}` stays inside; math cannot close a group it did
    /// not open or continue past a blank line.
    fn math_end(&self, i: usize, close: &[u8]) -> Option<usize> {
        let mut j = i;
        while j < self.b.len() {
            if self.b[j..].starts_with(close) {
                return Some(j + close.len());
            }
            j = match self.b[j] {
                b'{' => self.group_end(j)?,
                b'}' | b'$' => return None,
                b'\\' => self.control(j),
                b'%' => self.comment(j),
                b'\n' if self.paragraph(j) => return None,
                _ => j + 1,
            };
        }
        None
    }

    fn environment_end(&self, i: usize, env: &str) -> Option<usize> {
        let mut j = i;
        while j < self.b.len() {
            j = match self.b[j] {
                b'{' => self.group_end(j)?,
                b'}' => return None,
                b'\\' => {
                    let end = self.control(j);
                    if &self.s[j + 1..end] == "end" {
                        if let Some((name, after)) = self.environment_name(end) {
                            if name == env {
                                return Some(after);
                            }
                        }
                    }
                    end
                }
                b'%' => self.comment(j),
                b'\n' if self.paragraph(j) => return None,
                _ => j + 1,
            };
        }
        None
    }

    /// Whether the newline at `i` ends a paragraph.
    fn paragraph(&self, i: usize) -> bool {
        let rest = &self.b[i + 1..];
        let line = rest.iter().position(|&c| c == b'\n').unwrap_or(rest.len());
        line < rest.len() && rest[..line].iter().all(|c| *c == b' ' || *c == b'\t')
    }
}

/// Markdown and other markup: `$..$`, `$$..$$`, `\(..\)`, `\[..\]` outside
/// code, with `%` as ordinary text.
pub fn markup(source: &str) -> Vec<(usize, usize)> {
    let mut masked = source.as_bytes().to_vec();
    let mut fence: Option<(u8, usize)> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let b = line.as_bytes();
        let indent = b.iter().take(3).take_while(|&&c| c == b' ').count();
        let marker = b.get(indent).filter(|c| **c == b'`' || **c == b'~').map(|&c| {
            (c, b[indent..].iter().take_while(|&&d| d == c).count())
        });
        let marker = marker.filter(|(_, n)| *n >= 3);
        if fence.is_some() || marker.is_some() {
            masked[offset..offset + b.len()].fill(b' ');
            if let Some((c, n)) = marker {
                match fence {
                    Some((f, m)) if f == c && n >= m => fence = None,
                    None => fence = Some((c, n)),
                    _ => {}
                }
            }
        }
        offset += b.len();
    }
    mask_code_spans(&mut masked);

    let m = &masked;
    let mut found = Vec::new();
    let mut opening: Option<(&[u8], usize, usize)> = None;
    let mut i = 0;
    while i < m.len() {
        let token: &[u8] = match m[i] {
            b'$' if m.get(i + 1) == Some(&b'$') => b"$$",
            b'$' => b"$",
            b'\\' if matches!(m.get(i + 1), Some(b'(' | b')' | b'[' | b']')) => &m[i..i + 2],
            _ => {
                i += 1;
                continue;
            }
        };
        let (start, end) = (i, i + token.len());
        i = end;
        if m[..start].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1 {
            continue;
        }
        if let Some((value, _, open_end)) = opening {
            if value == b"$" && has_blank_line(&m[open_end..start]) {
                opening = None;
            }
        }
        match opening {
            None => {
                if !matches!(token, b"$" | b"$$" | b"\\(" | b"\\[") {
                    continue;
                }
                if token == b"$" && (end == m.len() || m[end].is_ascii_whitespace()) {
                    continue;
                }
                opening = Some((token, start, end));
            }
            Some((value, open_start, _)) => {
                let close: &[u8] = match value {
                    b"\\(" => b"\\)",
                    b"\\[" => b"\\]",
                    other => other,
                };
                if token != close {
                    continue;
                }
                if token == b"$"
                    && (m[start - 1].is_ascii_whitespace()
                        || m.get(end).is_some_and(u8::is_ascii_digit))
                {
                    continue;
                }
                found.push((open_start, end));
                opening = None;
            }
        }
    }
    found
}

fn has_blank_line(text: &[u8]) -> bool {
    text.split(|&c| c == b'\n')
        .skip(1)
        .take(text.iter().filter(|&&c| c == b'\n').count().saturating_sub(1))
        .any(|line| line.iter().all(|&c| c == b' ' || c == b'\t'))
}

/// Blank inline code: a run of backticks up to the next run of equal length.
fn mask_code_spans(m: &mut [u8]) {
    let run = |m: &[u8], i: usize| m[i..].iter().take_while(|&&c| c == b'`').count();
    let mut i = 0;
    while i < m.len() {
        if m[i] != b'`' {
            i += 1;
            continue;
        }
        let n = run(m, i);
        let mut j = i + n;
        let mut close = None;
        while j < m.len() {
            if m[j] == b'`' {
                let k = run(m, j);
                if k == n {
                    close = Some(j + k);
                    break;
                }
                j += k;
            } else {
                j += 1;
            }
        }
        match close {
            Some(end) => {
                m[i..end].fill(b' ');
                i = end;
            }
            None => i += n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raws(source: &str, filetype: &str) -> Vec<String> {
        formulas(source, filetype, "").1.into_iter().map(|f| f.raw).collect()
    }

    #[test]
    fn tex_skips_comments_verbatim_and_definitions() {
        let source = "a $x$ % $no$\n\\verb|$no$| \\newcommand{\\f}[1]{$no$}\n\
                      \\def\\g#1{$no$} \\begin{verbatim}$no$\\end{verbatim} \\(y\\) \\$ \\SI{3}{nm}";
        assert_eq!(raws(source, "tex"), ["$x$", "\\(y\\)", "\\SI{3}{nm}"]);
    }

    #[test]
    fn unclosed_math_stays_in_its_paragraph() {
        let source = "typing $a+ here\n\nnext $b$ and $\\text{$c$}$";
        assert_eq!(raws(source, "tex"), ["$b$", "$\\text{$c$}$"]);
    }

    #[test]
    fn environments_and_displays() {
        let source = "\\begin{equation}\n x \\begin{aligned}a\\end{aligned}\n\\end{equation}\n\
                      \\begin{figure}\\caption{$c$}\\end{figure}\n\\\\[2pt] $$d$$";
        let found = formulas(source, "tex", "").1;
        assert_eq!(found.len(), 3);
        assert!(found[0].display && !found[1].display);
        assert_eq!(found[0].spans, [[1, 1, 16], [2, 1, 32], [3, 1, 14]]);
        assert_eq!(found[2].raw, "$$d$$");
    }

    #[test]
    fn definitions_with_defaults() {
        let defs = definitions("\\newcommand{\\a}[2][x]{#1#2}\\newcommand\\b{B}\\DeclareMathOperator*{\\c}{c}");
        assert_eq!(defs["a"], ("[{".into(), Some("x".into()), "#1#2".into()));
        assert_eq!(defs["b"], ("".into(), None, "B".into()));
        assert_eq!(defs["c"].2, "\\operatorname{c}");
    }

    #[test]
    fn markdown_ignores_code_and_prices() {
        let source = "`$no$` $x$ costs $5 and $6\n\n```\n$no$\n```\n\\[y\\] \\$ $$z$$";
        assert_eq!(raws(source, "markdown"), ["$x$", "\\[y\\]", "$$z$$"]);
    }
}

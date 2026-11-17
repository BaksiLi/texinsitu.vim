//! Inline math as one row of text: plain pieces, subscripts and superscripts.
//!
//! A formula is expanded with the document's definitions, then written out
//! left to right. Anything this does not know how to write gives `None`, and
//! the source stays visible in the editor.

use crate::scan::Definitions;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Ch(char),
    Cmd(String),
    Group(Vec<Item>),
    Param(usize),
}
use Item::*;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub enum Kind {
    #[serde(rename = "")]
    Text,
    #[serde(rename = "sub")]
    Sub,
    #[serde(rename = "sup")]
    Sup,
}

pub type Row = Vec<(String, Kind)>;

#[derive(Debug, Serialize)]
pub struct Inline {
    pub segments: Row,
    pub width: usize,
}

struct Symbols {
    text: HashMap<String, String>,
    /// Classic name of each upright Greek letter, for font lookups.
    greek: HashMap<char, String>,
}

fn symbols() -> &'static Symbols {
    static TABLE: OnceLock<Symbols> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut text = HashMap::new();
        let mut greek = HashMap::new();
        for line in include_str!("symbols.tsv").lines().filter(|l| !l.starts_with('#')) {
            let mut fields = line.split('\t');
            let (Some(name), Some(value)) = (fields.next(), fields.next()) else { continue };
            text.insert(name.to_string(), value.to_string());
        }
        for (name, value) in &text {
            if let Some(classic) = name.strip_prefix("mup") {
                let mut chars = value.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    greek.entry(c).or_insert_with(|| classic.to_string());
                }
            }
        }
        Symbols { text, greek }
    })
}

pub fn parse(source: &str) -> Option<Vec<Item>> {
    let chars: Vec<char> = source.chars().collect();
    let mut stack = vec![Vec::new()];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\\' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i == start && i < chars.len() {
                    i += 1;
                }
                stack.last_mut()?.push(Cmd(chars[start..i].iter().collect()));
            }
            '{' => stack.push(Vec::new()),
            '}' => {
                let group = stack.pop()?;
                stack.last_mut()?.push(Group(group));
            }
            '#' if chars.get(i).is_some_and(|d| d.is_ascii_digit()) => {
                stack.last_mut()?.push(Param(chars[i].to_digit(10)? as usize));
                i += 1;
            }
            '%' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            _ => stack.last_mut()?.push(Ch(c)),
        }
    }
    (stack.len() == 1).then(|| stack.pop().unwrap())
}

pub fn blank(item: &Item) -> bool {
    matches!(item, Ch(c) if c.is_whitespace())
}

pub fn next(items: &[Item], i: &mut usize) -> Option<Item> {
    while items.get(*i).is_some_and(blank) {
        *i += 1;
    }
    let item = items.get(*i)?.clone();
    *i += 1;
    Some(item)
}

pub fn peek(items: &[Item], i: usize) -> Option<&Item> {
    items[i.min(items.len())..].iter().find(|item| !blank(item))
}

/// A mandatory argument: a group's contents or a single item. A font, text
/// or accent command brings its own argument, as in `x_\mathrm{eff}`.
pub fn argument(items: &[Item], i: &mut usize) -> Option<Vec<Item>> {
    Some(match next(items, i)? {
        Group(group) => group,
        Ch('}') => return None,
        Cmd(name)
            if font(&name).is_some()
                || accent(&name).is_some()
                || name.starts_with("text")
                || ["mbox", "operatorname", "overline", "underline"].contains(&name.as_str()) =>
        {
            let mut j = *i;
            match next(items, &mut j) {
                Some(group @ Group(_)) => {
                    *i = j;
                    vec![Cmd(name), group]
                }
                _ => vec![Cmd(name)],
            }
        }
        item => vec![item],
    })
}

/// An optional `[...]` argument, if one follows.
pub fn optional(items: &[Item], i: &mut usize) -> Option<Vec<Item>> {
    let mut j = *i;
    if next(items, &mut j)? != Ch('[') {
        return None;
    }
    let close = j + items[j..].iter().position(|item| *item == Ch(']'))?;
    *i = close + 1;
    Some(items[j..close].to_vec())
}

/// Annotated equals signs Unicode has as one relation (after UniTeX).
fn relation(over: &[Item], base: &[Item]) -> Option<char> {
    let over: Vec<&Item> = over.iter().filter(|item| !blank(item)).collect();
    if base.iter().filter(|item| !blank(item)).collect::<Vec<_>>() != [&Ch('=')] {
        return None;
    }
    let word = |items: &[&Item]| -> Option<String> {
        items.iter().map(|item| if let Ch(c) = item { Some(*c) } else { None }).collect()
    };
    match (over.as_slice(), word(&over).as_deref()) {
        ([Cmd(name)], _) if name == "Delta" => Some('≜'),
        ([Cmd(name)], _) if name == "star" => Some('≛'),
        ([Cmd(name), Group(g)], _)
            if name == "mathrm" && word(&g.iter().collect::<Vec<_>>()).as_deref() == Some("def") =>
        {
            Some('≝')
        }
        (_, Some("def")) => Some('≝'),
        (_, Some("?")) => Some('≟'),
        (_, Some("m")) => Some('≞'),
        _ => None,
    }
}

/// Replace defined commands by their bodies, arguments in place of `#n`.
pub fn expand(items: &[Item], defs: &Definitions, depth: usize) -> Option<Vec<Item>> {
    if depth > 32 {
        return None; // A definition that never ends.
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        i += 1;
        match item {
            Group(group) => out.push(Group(expand(group, defs, depth)?)),
            Cmd(name) if name == "overset" || name == "stackrel" => {
                let mut j = i;
                let found = argument(items, &mut j)
                    .zip(argument(items, &mut j))
                    .and_then(|(over, base)| relation(&over, &base));
                match found {
                    Some(symbol) => {
                        out.push(Ch(symbol));
                        i = j;
                    }
                    None => i = call(&mut out, name, items, i, defs, depth)?,
                }
            }
            Cmd(name) if defs.contains_key(name) => i = call(&mut out, name, items, i, defs, depth)?,
            other => out.push(other.clone()),
        }
    }
    Some(out)
}

fn call(
    out: &mut Vec<Item>,
    name: &str,
    items: &[Item],
    mut i: usize,
    defs: &Definitions,
    depth: usize,
) -> Option<usize> {
    let (spec, default, body) = defs.get(name)?;
    let mut values = Vec::new();
    for kind in spec.chars() {
        values.push(if kind == '[' {
            optional(items, &mut i).or_else(|| parse(default.as_deref()?))
        } else {
            Some(argument(items, &mut i)?)
        });
    }
    let body = substitute(&parse(body)?, &values)?;
    out.push(Group(expand(&body, defs, depth + 1)?));
    Some(i)
}

fn substitute(body: &[Item], values: &[Option<Vec<Item>>]) -> Option<Vec<Item>> {
    body.iter()
        .map(|item| {
            Some(match item {
                Param(n) => Group(values.get(n.checked_sub(1)?)?.clone()?),
                Group(group) => Group(substitute(group, values)?),
                other => other.clone(),
            })
        })
        .collect()
}

const SUP: [(char, char); 16] = [
    ('0', '⁰'), ('1', '¹'), ('2', '²'), ('3', '³'), ('4', '⁴'), ('5', '⁵'), ('6', '⁶'),
    ('7', '⁷'), ('8', '⁸'), ('9', '⁹'), ('+', '⁺'), ('-', '⁻'), ('−', '⁻'), ('=', '⁼'), ('(', '⁽'), (')', '⁾'),
];
const SUB: [(char, char); 16] = [
    ('0', '₀'), ('1', '₁'), ('2', '₂'), ('3', '₃'), ('4', '₄'), ('5', '₅'), ('6', '₆'),
    ('7', '₇'), ('8', '₈'), ('9', '₉'), ('+', '₊'), ('-', '₋'), ('−', '₋'), ('=', '₌'), ('(', '₍'), (')', '₎'),
];
pub const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh",
    "tanh", "coth", "log", "ln", "lg", "exp", "det", "dim", "ker", "deg", "arg", "gcd", "hom",
    "lim", "liminf", "limsup", "max", "min", "sup", "inf", "Pr",
];
const IGNORED: &[&str] = &[
    "big", "Big", "bigg", "Bigg", "bigl", "bigr", "Bigl", "Bigr", "bigm", "biggl", "biggr",
    "Biggl", "Biggr", "displaystyle", "textstyle", "scriptstyle", "scriptscriptstyle", "limits",
    "nolimits", "nonumber", "notag", "rm", "it", "bf", "sf", "tt", "cal", "em", "sl", "sc",
    "tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge",
    "Huge", "itshape", "bfseries", "upshape", "mdseries", "normalfont", "rmfamily", "sffamily",
    "ttfamily",
];

/// TeX that reads files or defines things is not a formula, whatever it shows.
const PROGRAM: &[&str] = &[
    "input", "include", "includegraphics", "write", "immediate", "openout", "openin", "read",
    "def", "gdef", "edef", "xdef", "let", "csname", "endcsname", "expandafter", "catcode",
    "usepackage", "documentclass", "begin", "end", "newcommand", "renewcommand",
];

pub fn large(name: &str) -> Option<&'static str> {
    Some(match name {
        "sum" => "∑", "prod" => "∏", "coprod" => "∐", "int" => "∫", "iint" => "∬",
        "oint" => "∮", "bigcup" => "⋃", "bigcap" => "⋂", "bigoplus" => "⨁",
        "bigotimes" => "⨂", "bigvee" => "⋁", "bigwedge" => "⋀",
        _ => return None,
    })
}

/// unicode-math's name prefix for a font command; "" keeps letters upright.
fn font(name: &str) -> Option<&'static str> {
    Some(match name {
        "mathbf" | "boldsymbol" | "textbf" | "pmb" => "mbf",
        "mathcal" => "mscr",
        "mathscr" => "mbfscr",
        "mathbb" => "Bbb",
        "mathfrak" => "mfrak",
        "mathsf" => "msans",
        "mathtt" => "mtt",
        "mathrm" | "mathup" => "",
        _ => return None,
    })
}

/// Combining mark of an accent over one symbol, and whether over a word of
/// letters it spreads to each letter.
fn accent(name: &str) -> Option<(char, Option<char>)> {
    Some(match name {
        "widetilde" => ('\u{303}', Some('\u{303}')),
        "widehat" => ('\u{302}', Some('\u{302}')),
        "bar" => ('\u{305}', Some('\u{304}')),
        "tilde" => ('\u{303}', None),
        "hat" => ('\u{302}', None),
        "vec" => ('\u{20d7}', None),
        "dot" => ('\u{307}', None),
        "ddot" => ('\u{308}', None),
        "breve" => ('\u{306}', None),
        "check" => ('\u{30c}', None),
        "acute" => ('\u{301}', None),
        "grave" => ('\u{300}', None),
        _ => return None,
    })
}

#[derive(Default)]
pub struct Line(pub Row);

impl Line {
    pub fn push(&mut self, text: &str, kind: Kind) {
        if text.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some((last, Kind::Text)) if kind == Kind::Text => last.push_str(text),
            _ => self.0.push((text.to_string(), kind)),
        }
    }

    pub fn extend(&mut self, row: Row) {
        for (text, kind) in row {
            self.push(&text, kind);
        }
    }
}

pub fn plain(row: &Row) -> String {
    row.iter().map(|(text, _)| text.as_str()).collect()
}

/// Cell-width policy supplied by the consumer, including editor overrides.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct Cells {
    /// 'ambiwidth' is double: East Asian Ambiguous characters (UAX #11),
    /// Greek letters and box drawing among them, take two cells.
    #[serde(default)]
    pub wide: bool,
    /// Inclusive codepoint ranges [first, last, width], which override the
    /// rest. Vim supplies these from getcellwidths().
    #[serde(default)]
    pub overrides: Vec<(u32, u32, usize)>,
}

thread_local! {
    static CELLS: std::cell::RefCell<Cells> = std::cell::RefCell::default();
}

pub fn set_cells(cells: Cells) {
    CELLS.with(|c| *c.borrow_mut() = cells);
}

fn ambiguous(c: char) -> bool {
    // Our own table, not unicode-width's CJK mode, which keeps Greek narrow
    // where Vim does not.
    static RANGES: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        include_str!("ambiguous.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let (a, b) = l.split_once('\t').unwrap();
                (u32::from_str_radix(a, 16).unwrap(), u32::from_str_radix(b, 16).unwrap())
            })
            .collect()
    });
    let c = c as u32;
    use std::cmp::Ordering::*;
    ranges.binary_search_by(|&(a, b)| if b < c { Less } else if a > c { Greater } else { Equal }).is_ok()
}

pub fn width(text: &str) -> usize {
    CELLS.with(|cells| {
        let cells = cells.borrow();
        text.chars()
            .map(|c| {
                let code = c as u32;
                let set = cells.overrides.iter().find(|&&(a, b, _)| a <= code && code <= b);
                if let Some(&(_, _, w)) = set {
                    return w;
                }
                let base = c.width().unwrap_or(0);
                if cells.wide && base == 1 && ambiguous(c) { 2 } else { base }
            })
            .sum()
    })
}

/// The letter, digit or Greek letter `c` in a unicode-math font.
fn styled(c: char, prefix: &str) -> String {
    if prefix.is_empty() {
        return c.to_string();
    }
    let table = symbols();
    const DIGITS: [&str; 10] =
        ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
    let name = match c {
        'A'..='Z' | 'a'..='z' => c.to_string(),
        '0'..='9' => DIGITS[c as usize - '0' as usize].to_string(),
        _ => match table.greek.get(&c) {
            Some(name) => name.clone(),
            None => return c.to_string(),
        },
    };
    table.text.get(&format!("{prefix}{name}")).cloned().unwrap_or_else(|| c.to_string())
}

/// Write a formula's math as one row.
pub fn row(items: &[Item], prefix: &str, script: bool) -> Option<Row> {
    Some(words(items, prefix, script)?.0)
}

/// A row, and whether it ends with an operator name: a defined operator
/// expands to a group, and the letter after the group is still set apart.
fn words(items: &[Item], prefix: &str, script: bool) -> Option<(Row, bool)> {
    let mut out = Line::default();
    let mut operator = false;
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        i += 1;
        if blank(item) {
            continue;
        }
        if let Ch(c @ ('^' | '_')) = item {
            let (text, kind) = scripted(*c, &argument(items, &mut i)?, prefix)?;
            out.push(&text, kind);
            continue; // An operator name keeps its spacing past its scripts.
        }
        let word = match item {
            Ch(c) => c.is_alphanumeric(),
            Cmd(name) => {
                name.chars().all(|c| c.is_ascii_alphabetic())
                    && !["left", "right", "big", "Big"].contains(&name.as_str())
            }
            _ => false,
        };
        if operator && word {
            out.push(" ", Kind::Text);
        }
        operator = false;
        match item {
            Ch('\'') => {
                let mut count = 1;
                while items.get(i) == Some(&Ch('\'')) {
                    count += 1;
                    i += 1;
                }
                out.push(["′", "″", "‴"].get(count - 1)?, Kind::Text);
            }
            Ch(':') if !script => {
                if peek(items, i) == Some(&Ch('=')) {
                    next(items, &mut i);
                    out.push(":=", Kind::Text);
                } else {
                    out.push(" : ", Kind::Text); // A relation, set apart.
                }
            }
            Ch('~') => out.push(" ", Kind::Text),
            Ch('-') => out.push("−", Kind::Text), // The minus sign, as UTR #25 asks.
            Ch('&') | Ch('#') | Ch('$') | Param(_) => return None,
            Ch(c) => out.push(&styled(*c, prefix), Kind::Text),
            Group(group) => {
                let (value, ends) = words(group, prefix, script)?;
                out.extend(value);
                operator = ends;
            }
            Cmd(name) => operator = command(&mut out, name, items, &mut i, prefix, script)?,
        }
    }
    Some((out.0, operator))
}

/// A script as one segment: digits and signs as Unicode script characters,
/// anything else as styled text.
pub fn scripted(c: char, argument: &[Item], prefix: &str) -> Option<(String, Kind)> {
    let value = plain(&row(argument, prefix, true)?);
    let table = if c == '^' { &SUP } else { &SUB };
    let mapped: Option<String> = value
        .chars()
        .map(|v| table.iter().find(|(from, _)| *from == v).map(|(_, to)| *to))
        .collect();
    Some(if value.is_empty() {
        return None;
    } else if c == '^' && value == "∘" {
        ("°".into(), Kind::Text) // ^\circ is how TeX writes a degree.
    } else if value.chars().all(|v| "′″‴".contains(v)) {
        (value, Kind::Text)
    } else if let Some(mapped) = mapped {
        (mapped, Kind::Text)
    } else {
        (value, if c == '^' { Kind::Sup } else { Kind::Sub })
    })
}

/// Write one command; true when it is an operator name, which a following
/// letter is set apart from.
fn command(
    out: &mut Line,
    name: &str,
    items: &[Item],
    i: &mut usize,
    prefix: &str,
    script: bool,
) -> Option<bool> {
    let text = |out: &mut Line, value: &str| out.push(value, Kind::Text);
    match name {
        "frac" | "dfrac" | "tfrac" | "cfrac" => {
            // Compound operands are parenthesized: a/(b+c), never a/(N_b).
            let operand = |items: Vec<Item>| -> Option<Row> {
                let value = row(&items, prefix, script)?;
                let compound = plain(&value).chars().any(|c| {
                    !(c.is_alphanumeric() || c == '_' || ('\u{300}'..='\u{36f}').contains(&c))
                });
                Some(if compound {
                    [vec![("(".into(), Kind::Text)], value, vec![(")".into(), Kind::Text)]].concat()
                } else {
                    value
                })
            };
            let numerator = operand(argument(items, i)?)?;
            let denominator = operand(argument(items, i)?)?;
            let scripted = matches!(peek(items, *i), Some(Ch('^' | '_')));
            if scripted {
                text(out, "(");
            }
            out.extend(numerator);
            text(out, "/");
            out.extend(denominator);
            if scripted {
                text(out, ")");
            }
        }
        "sqrt" => {
            let degree = optional(items, i);
            let radicand = row(&argument(items, i)?, prefix, script)?;
            let degree = match degree {
                Some(degree) => plain(&row(&degree, prefix, script)?),
                None => String::new(),
            };
            match degree.as_str() {
                "" | "2" => text(out, "√"),
                "3" => text(out, "∛"),
                "4" => text(out, "∜"),
                other => text(out, &format!("[{other}]√")),
            }
            text(out, "(");
            out.extend(radicand);
            text(out, ")");
        }
        "text" | "mbox" | "textrm" | "hbox" | "intertext" | "shortintertext" | "textsf"
        | "textit" | "texttt" | "textup" | "textnormal" | "textsl" | "emph" => out.extend(literal(&argument(items, i)?)?),
        "kern" | "mkern" | "hskip" | "mskip" => {
            // A length such as -2em: sign, number, unit.
            while matches!(items.get(*i), Some(Ch('-' | '+' | '.' | '0'..='9' | ' '))) {
                *i += 1;
            }
            for _ in 0..2 {
                if matches!(items.get(*i), Some(Ch(c)) if c.is_ascii_alphabetic()) {
                    *i += 1;
                }
            }
        }
        "hspace" | "vspace" => {
            if peek(items, *i) == Some(&Ch('*')) {
                next(items, i);
            }
            argument(items, i)?;
            if name == "hspace" {
                text(out, " ");
            }
        }
        "overline" | "underline" | "underbar" => {
            let mark = if name == "overline" { '\u{305}' } else { '\u{332}' };
            out.extend(marked(row(&argument(items, i)?, prefix, script)?, mark));
        }
        "mathop" => {
            out.extend(row(&argument(items, i)?, prefix, script)?);
            return Some(true);
        }
        // Drawn only in display math, and a line break only between rows.
        "overbrace" | "underbrace" | "newline" => return None,
        "operatorname" => {
            if peek(items, *i) == Some(&Ch('*')) {
                next(items, i);
            }
            out.extend(row(&argument(items, i)?, "", script)?);
            return Some(true);
        }
        _ if FUNCTIONS.contains(&name) => {
            text(out, name);
            return Some(true);
        }
        "bmod" | "mod" => text(out, " mod "),
        "pod" => {
            text(out, " (");
            out.extend(row(&argument(items, i)?, prefix, script)?);
            text(out, ")");
        }
        "hdotsfor" | "dotsfor" => {
            optional(items, i);
            argument(items, i)?;
            text(out, "…");
        }
        "pmod" => {
            text(out, "(mod ");
            out.extend(row(&argument(items, i)?, prefix, script)?);
            text(out, ")");
        }
        "left" | "right" | "middle" => match next(items, i)? {
            Ch('.') => {}
            Ch(c) => text(out, &c.to_string()),
            Cmd(delimiter) => {
                command(out, &delimiter, items, i, prefix, script)?;
            }
            _ => return None,
        },
        "phantom" | "vphantom" | "hphantom" => {
            argument(items, i)?;
        }
        "label" | "tag" => {
            if peek(items, *i) == Some(&Ch('*')) {
                next(items, i);
            }
            argument(items, i)?;
        }
        _ if IGNORED.contains(&name) => {}
        "," | ":" | ">" | ";" | " " => text(out, " "),
        "!" => {}
        "quad" => text(out, "   "),
        "qquad" => text(out, "    "),
        "{" | "}" | "_" | "%" | "#" | "&" | "$" | "*" => text(out, name),
        "|" => text(out, "‖"),
        "not" => match next(items, i)? {
            Ch('=') => text(out, "≠"),
            other => {
                out.extend(row(&[other], prefix, script)?);
                text(out, "\u{338}");
            }
        },
        "SI" | "qty" => {
            optional(items, i);
            let number = row(&argument(items, i)?, prefix, script)?;
            let unit = units(&argument(items, i)?)?;
            out.extend(number);
            // The SI Brochure: a space between number and unit, which must not
            // break a line, except for the plane-angle signs: 64°, 30′.
            if !unit.is_empty() {
                let space = if unit.starts_with(['°', '′', '″']) && unit != "°C" { "" } else { "\u{a0}" };
                text(out, &format!("{space}{unit}"));
            }
        }
        "si" | "unit" => text(out, &units(&argument(items, i)?)?),
        "ang" => {
            optional(items, i);
            out.extend(row(&argument(items, i)?, prefix, script)?);
            text(out, "°");
        }
        "num" => out.extend(row(&argument(items, i)?, prefix, script)?),
        _ => {
            if let Some(symbol) = large(name) {
                text(out, symbol);
            } else if let Some(style) = font(name) {
                out.extend(row(&argument(items, i)?, style, script)?);
            } else if let Some((mark, spread)) = accent(name) {
                let base = argument(items, i)?;
                let value = row(&base, prefix, script)?;
                let symbol = plain(&value);
                let visible = symbol.chars().filter(|c| c.width() != Some(0)).count();
                let word = symbol.chars().all(|c| c.is_alphabetic() || c.width() == Some(0));
                match spread {
                    // Over letters, the mark goes on every letter: Ñ, ā, x̄ȳ.
                    Some(spread) if visible > 0 && word => out.extend(marked(value, spread)),
                    _ if visible == 1 => out.extend(marked(value, mark)),
                    _ => return None,
                }
            } else if let Some(symbol) = symbols().text.get(name) {
                let value: String = symbol.chars().map(|c| styled(c, prefix)).collect();
                text(out, &value);
            } else if name.chars().all(|c| c.is_ascii_alphabetic())
                && !name.is_empty()
                && !PROGRAM.contains(&name)
            {
                // Unknown command: show the call, name(a,b), rather than
                // drop the formula.
                text(out, name);
                let mut separator = "(";
                while let Some(Group(group)) = items.get(*i) {
                    *i += 1;
                    text(out, separator);
                    out.extend(row(group, prefix, script)?);
                    separator = ",";
                }
                if separator == "," {
                    text(out, ")");
                }
            } else {
                return None;
            }
        }
    }
    Some(false)
}

/// A row with `mark` over or under every visible character.
fn marked(row: Row, mark: char) -> Row {
    row.into_iter()
        .map(|(text, kind)| {
            let text: String = text
                .chars()
                .flat_map(|c| if c.width().unwrap_or(0) > 0 { vec![c, mark] } else { vec![c] })
                .collect();
            (text.nfc().collect(), kind)
        })
        .collect()
}

fn text_accent(name: &str) -> Option<char> {
    Some(match name {
        "'" => '\u{301}',
        "`" => '\u{300}',
        "^" => '\u{302}',
        "~" => '\u{303}',
        "=" => '\u{304}',
        "u" => '\u{306}',
        "." => '\u{307}',
        "\"" => '\u{308}',
        "r" => '\u{30a}',
        "H" => '\u{30b}',
        "v" => '\u{30c}',
        "d" => '\u{323}',
        "c" => '\u{327}',
        "k" => '\u{328}',
        "b" => '\u{331}',
        _ => return None,
    })
}

fn accented_text(items: &[Item], mark: char) -> Option<Row> {
    let mut base = literal(items)?;
    let text = plain(&base);
    if text.chars().filter(|letter| letter.width().unwrap_or(0) > 0).count() != 1 {
        return None;
    }
    if mark == '\u{307}' && (text == "ı" || text == "ȷ") {
        return Some(vec![(if text == "ı" { "i" } else { "j" }.into(), Kind::Text)]);
    }
    // TeX authors remove i/j's dot explicitly before adding an accent.
    // Unicode encodes í/ĵ with ordinary i/j plus that accent.
    if ('\u{300}'..='\u{30c}').contains(&mark) {
        for (text, _) in &mut base {
            *text = text.replace('ı', "i").replace('ȷ', "j");
        }
    }
    Some(marked(base, mark))
}

/// Text-mode contents: words with their own spacing, a run of spaces as one,
/// and math between dollars written as math.
fn literal(items: &[Item]) -> Option<Row> {
    let mut out = Line::default();
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        i += 1;
        let spaced = |out: &Line| plain(&out.0).ends_with(' ');
        match item {
            Ch(c) if c.is_whitespace() => {
                if !spaced(&out) {
                    out.push(" ", Kind::Text);
                }
            }
            Ch('$') => {
                let close = i + items[i..].iter().position(|item| *item == Ch('$'))?;
                out.extend(row(&items[i..close], "", false)?);
                i = close + 1;
            }
            Ch('&' | '#' | '^' | '_') => return None,
            Ch('~') => out.push(" ", Kind::Text),
            Ch(c) => out.push(&c.to_string(), Kind::Text),
            Group(group) => out.extend(literal(group)?),
            Cmd(name) if ["_", "%", "&", "#", "$", "{", "}"].contains(&name.as_str()) => {
                out.push(name, Kind::Text)
            }
            Cmd(name) if name == " " || name == "," => out.push(" ", Kind::Text),
            Cmd(name) if text_accent(name).is_some() => {
                out.extend(accented_text(&argument(items, &mut i)?, text_accent(name)?)?);
            }
            Cmd(name) if name == "i" || name == "j" => {
                out.push(if name == "i" { "ı" } else { "ȷ" }, Kind::Text);
            }
            Cmd(name)
                if ["text", "textrm", "textbf", "textit", "textsf", "texttt", "textup",
                    "textnormal", "textsl", "textsc", "emph", "mbox"]
                    .contains(&name.as_str()) =>
            {
                out.extend(literal(&argument(items, &mut i)?)?)
            }
            Cmd(name) if IGNORED.contains(&name.as_str()) => {}
            Cmd(name) if name == "textdollar" => out.push("$", Kind::Text),
            Cmd(name) if symbols().text.contains_key(name) => out.push(&symbols().text[name], Kind::Text),
            _ => return None,
        }
    }
    Some(out.0)
}

/// siunitx units as written: \micro\metre is μm, \cubic\metre is m³.
fn units(items: &[Item]) -> Option<String> {
    fn unit(name: &str) -> &str {
        match name {
            "metre" | "meter" => "m", "hertz" => "Hz", "pascal" => "Pa", "kilogram" => "kg",
            "gram" => "g", "second" => "s", "minute" => "min", "ampere" => "A", "volt" => "V",
            "ohm" => "Ω", "watt" => "W", "joule" => "J", "newton" => "N", "celsius" => "°C",
            "percent" => "%", "milli" => "m", "nano" => "n", "micro" => "μ", "mega" => "M",
            "giga" => "G", "kilo" => "k", "centi" => "c", "pico" => "p", "per" => "/",
            "cubic" => "³", "squared" | "square" => "²", "um" => "μm",
            "degree" => "°", "arcminute" => "′", "arcsecond" => "″",
            other => other,
        }
    }
    let mut parts: Vec<String> = Vec::new();
    let mut word = String::new();
    for item in items {
        match item {
            Ch(c) if !c.is_whitespace() => word.push(*c),
            Ch(_) => {}
            Cmd(name) => {
                if !word.is_empty() {
                    parts.push(std::mem::take(&mut word));
                }
                parts.push(unit(name).to_string());
            }
            _ => return None,
        }
    }
    if !word.is_empty() {
        parts.push(word);
    }
    const PREFIXES: &[&str] = &["μ", "n", "m", "k", "M", "G", "c", "p"];
    const BASES: &[&str] = &[
        "m", "Hz", "Pa", "kg", "g", "s", "min", "A", "V", "Ω", "W", "J", "N", "°C", "%", "nm",
        "mm", "cm", "km", "um", "ms", "ns", "MHz", "GHz", "kHz", "GPa", "MPa", "kPa",
    ];
    let mut tokens = Vec::new();
    let mut k = 0;
    while k < parts.len() {
        if PREFIXES.contains(&parts[k].as_str())
            && parts.get(k + 1).is_some_and(|base| BASES.contains(&base.as_str()))
        {
            tokens.push(format!("{}{}", parts[k], parts[k + 1]));
            k += 2;
        } else {
            tokens.push(parts[k].clone());
            k += 1;
        }
    }
    let mut result: Vec<String> = Vec::new();
    let mut exponent = String::new();
    for token in tokens {
        if token == "²" || token == "³" {
            exponent.push_str(&token);
        } else {
            result.push(token + &std::mem::take(&mut exponent));
        }
    }
    if let Some(last) = result.last_mut() {
        last.push_str(&exponent);
    }
    Some(result.concat())
}

/// An inline row, or None to keep the source visible.
pub fn inline(raw: &str, defs: &Definitions) -> Option<Inline> {
    let body = if let Some(rest) = raw.strip_prefix("$$") {
        rest.strip_suffix("$$")?
    } else if let Some(rest) = raw.strip_prefix('$') {
        rest.strip_suffix('$')?
    } else if let Some(rest) = raw.strip_prefix("\\(").or_else(|| raw.strip_prefix("\\[")) {
        &rest[..rest.len().checked_sub(2)?]
    } else if raw.starts_with("\\begin") {
        return None;
    } else {
        raw // A siunitx command in prose.
    };
    let items = expand(&parse(body)?, defs, 0)?;
    let pieces = row(&items, "", false)?;
    let total = width(&plain(&pieces));
    if total == 0 {
        return None;
    }
    Some(Inline { segments: pieces, width: total })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(source: &str, defs: &str) -> String {
        let mut scope = crate::scan::definitions(include_str!("prelude.tex"));
        scope.extend(crate::scan::definitions(defs));
        let layout = inline(source, &scope).expect(source);
        layout.segments
            .iter()
            .map(|(text, kind)| match kind {
                Kind::Sub => format!("_[{text}]"),
                Kind::Sup => format!("^[{text}]"),
                Kind::Text => text.clone(),
            })
            .collect()
    }

    #[test]
    fn scripts_fonts_and_symbols() {
        assert_eq!(line(r"$Q_{22}^{\mathrm{all}}(\omega;\pvec)$", r"\newcommand{\pvec}{\bm{\phi}}"),
                   "Q₂₂^[all](ω;𝛟)");
        assert_eq!(line(r"$e^{j\omega t}+x^{-1}+128^\circ+f''$", ""), "e^[jωt]+x⁻¹+128°+f″");
        assert_eq!(line(r"$\mathcal{L}+\mathbb{R}^n+\mathscr{S}+\varphi$", ""), "ℒ+ℝ^[n]+𝓢+φ");
        assert_eq!(line(r"$\widetilde N_{\mathrm{in}}\leq\bar{xy}$", ""), "Ñ_[in]≤x̄ȳ");
    }

    #[test]
    fn fractions_roots_and_operators() {
        assert_eq!(line(r"$\frac{a+b}{c}+\frac{1}{2}^n+\sqrt{-1}$", ""), "(a+b)/c+(1/2)^[n]+√(−1)");
        assert_eq!(line(r"$\sin x+\argmin_k f+A\op B$", r"\DeclareMathOperator{\op}{op}"),
                   "sin x+arg min_[k] f+Aop B");
        assert_eq!(line(r"$f:A\to B,\ a:=b,\ a\bmod p$", ""), "f : A→B, a:=b, a mod p");
    }

    #[test]
    fn ambiguous_width_follows_vim() {
        // UAX #11 as Vim reads it: α and ─ are Ambiguous, − is not.
        assert_eq!(width("α−─x"), 4);
        set_cells(Cells { wide: true, overrides: vec![] });
        assert_eq!(width("α−─x"), 6);
        // setcellwidths() wins, as many set box drawing back to one cell.
        set_cells(Cells { wide: true, overrides: vec![(0x2500, 0x257f, 1)] });
        assert_eq!(width("α−─x"), 5);
        set_cells(Cells::default());
    }

    #[test]
    fn delimiters_render_as_written() {
        // The previous layout engine drew \left| as ⎢ and the first \| as |.
        assert_eq!(line(r"$\left\lvert x\right\rvert+\|y\|_2+\left.z\right|_0$", ""), "|x|+‖y‖₂+z|₀");
    }

    #[test]
    fn units_and_unknown_commands() {
        // SI Brochure: a non-breaking space before a unit, none before an angle.
        assert_eq!(line(r"\SI{3.6}{\micro\metre}", ""), "3.6\u{a0}μm");
        assert_eq!(line(r"\SI{64}{\degree}", ""), "64°");
        assert_eq!(line(r"\SI{20}{\celsius}", ""), "20\u{a0}°C");
        // UTR #25: the minus sign, and no canonical singleton such as U+212B.
        assert_eq!(line(r"$a-b^{-1}+\Angstrom$", ""), "a−b⁻¹+\u{c5}");
        assert_eq!(line(r"$\binom{n}{k}+\custom{y}$", ""), "binom(n,k)+custom(y)");
        let scope = crate::scan::definitions(include_str!("prelude.tex"));
        assert!(inline(r"$\input{secret}$", &scope).is_none(), "file commands stay source");
        assert!(inline(r"$\vphantom{x}$", &scope).is_none(), "nothing visible stays source");
    }

    #[test]
    fn text_accents_compose_and_dotless_letters_keep_their_meaning() {
        assert_eq!(line(r#"$\text{\'a \"o \H{u} \c{c} \v{s}}$"#, ""), "á ö ű ç š");
        assert_eq!(line(r"$\text{\'\i \^\j \i \j}$", ""), "í ĵ ı ȷ");
        assert_eq!(line(r"$\text{\r A \k{a} \d{a}}$", ""), "Å ą ạ");
        assert_eq!(line(r"$\text{\'{\i} and \`{e}}$", ""), "í and è");
        assert_eq!(line(r"$\text{\H e e̋}$", ""), "e̋ e̋");
        assert_eq!(line(r"$\text{\.\i \.\j}$", ""), "i j");
        let scope = crate::scan::definitions(include_str!("prelude.tex"));
        assert!(inline(r"$\text{\'{ab}}$", &scope).is_none());
    }
}

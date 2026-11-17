//! Display math as a block of rows.
//!
//! A formula is cut into runs that fit on one row, written by the inline
//! writer with its scripts and fonts, and blocks for what is truly two
//! dimensional: fractions, roots, limits of large operators, tall delimiters
//! and environments. Blocks line up on a baseline, as TeX boxes do. Anything
//! this cannot lay out gives `None` and stays source in the editor.

use crate::math::{self, Item, Item::*, Kind, Line, Row};
use crate::scan::Definitions;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Display {
    pub rows: Vec<Row>,
    pub width: usize,
}

/// TeX's math styles, as far as layout needs them: display style stacks
/// fractions, roots and limits; text style (inside a fraction, a matrix or
/// cases) writes them on the row.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Display,
    Text,
}
use Style::*;

/// Box drawing can take two cells while a numerator takes one. A narrow
/// minus fills the odd cell, so even a one-cell fraction keeps its bar.
fn rule(cells: usize) -> String {
    let stroke = math::width("─");
    let narrow = if math::width("−") == 1 { "−" } else { "-" };
    format!("{}{}", "─".repeat(cells / stroke), narrow.repeat(cells % stroke))
}

#[derive(Clone, Debug)]
struct Block {
    rows: Vec<Row>,
    base: usize,
    width: usize,
}

fn measure(row: &Row) -> usize {
    math::width(&math::plain(row))
}

/// Extend `row` with spaces to `column`, then with `pieces`.
fn put(row: &mut Row, column: usize, pieces: &Row) {
    let mut line = Line(std::mem::take(row));
    let gap = column.saturating_sub(measure(&line.0));
    line.push(&" ".repeat(gap), Kind::Text);
    line.extend(pieces.clone());
    *row = line.0;
}

impl Block {
    fn from_rows(rows: Vec<Row>, base: usize) -> Block {
        Block { width: rows.iter().map(measure).max().unwrap_or(0), rows, base }
    }

    fn line(row: Row) -> Block {
        Block::from_rows(vec![row], 0)
    }

    fn text(text: &str) -> Block {
        Block::line(vec![(text.to_string(), Kind::Text)])
    }

    fn below(&self) -> usize {
        self.rows.len() - 1 - self.base
    }
}

/// Blocks side by side on a common baseline.
fn beside(blocks: Vec<Block>) -> Block {
    let above = blocks.iter().map(|b| b.base).max().unwrap_or(0);
    let below = blocks.iter().map(Block::below).max().unwrap_or(0);
    let mut rows = vec![Row::new(); above + below + 1];
    let mut column = 0;
    for block in blocks {
        for (k, row) in block.rows.iter().enumerate() {
            put(&mut rows[above - block.base + k], column, row);
        }
        column += block.width;
    }
    Block { rows, base: above, width: column }
}

/// Blocks one above another, centered; the baseline is row `base`.
fn stack(blocks: Vec<Block>, base: usize) -> Block {
    let width = blocks.iter().map(|b| b.width).max().unwrap_or(0);
    let rows = blocks
        .into_iter()
        .flat_map(|block| {
            let indent = (width - block.width + 1) / 2;
            block.rows.into_iter().map(move |row| {
                let mut out = Row::new();
                put(&mut out, indent, &row);
                out
            })
        })
        .collect();
    Block { rows, base, width }
}

fn fraction(numerator: Block, denominator: Block) -> Block {
    let width = numerator.width.max(denominator.width);
    let base = numerator.rows.len();
    stack(vec![numerator, Block::text(&rule(width)), denominator], base)
}

/// The canonical character of a delimiter token, "" for `.`.
fn delimiter_of(item: &Item) -> Option<&'static str> {
    Some(match item {
        Ch('(') => "(",
        Ch(')') => ")",
        Ch('[') => "[",
        Ch(']') => "]",
        Ch('|') => "|",
        Ch('/') => "/",
        Ch('.') => "",
        Cmd(name) => match name.as_str() {
            "{" | "lbrace" => "{",
            "}" | "rbrace" => "}",
            "langle" => "⟨",
            "rangle" => "⟩",
            "vert" | "lvert" | "rvert" => "|",
            "|" | "Vert" | "lVert" | "rVert" => "‖",
            "lceil" => "⌈",
            "rceil" => "⌉",
            "lfloor" => "⌊",
            "rfloor" => "⌋",
            _ => return None,
        },
        _ => return None,
    })
}

/// A delimiter `height` rows tall; a brace points at row `base`.
fn delimiter(symbol: &str, height: usize, base: usize) -> Block {
    if height == 1 || symbol.is_empty() {
        let mut block = Block::text(symbol);
        block.rows.resize(height, Row::new());
        block.base = base;
        return block;
    }
    let last = height - 1;
    let tip = if 0 < base && base < last { base } else { last / 2 };
    let rows = (0..height)
        .map(|r| {
            let (top, middle, bottom) = match symbol {
                "(" => ("⎛", "⎜", "⎝"),
                ")" => ("⎞", "⎟", "⎠"),
                "[" => ("⎡", "⎢", "⎣"),
                "]" => ("⎤", "⎥", "⎦"),
                "⌈" => ("⎡", "⎢", "⎢"),
                "⌉" => ("⎤", "⎥", "⎥"),
                "⌊" => ("⎢", "⎢", "⎣"),
                "⌋" => ("⎥", "⎥", "⎦"),
                "|" => ("│", "│", "│"),
                "‖" => ("‖", "‖", "‖"),
                "{" if height == 2 => ("⎰", "", "⎱"),
                "}" if height == 2 => ("⎱", "", "⎰"),
                "{" => ("⎧", if r == tip { "⎨" } else { "⎪" }, "⎩"),
                "}" => ("⎫", if r == tip { "⎬" } else { "⎪" }, "⎭"),
                "⟨" => ("╱", if 2 * r < last { "╱" } else if 2 * r > last { "╲" } else { "⟨" }, "╲"),
                "⟩" => ("╲", if 2 * r < last { "╲" } else if 2 * r > last { "╱" } else { "⟩" }, "╱"),
                other => ("", if r == base { other } else { " " }, ""),
            };
            let piece = if r == 0 && !top.is_empty() {
                top
            } else if r == last && !bottom.is_empty() {
                bottom
            } else {
                middle
            };
            vec![(piece.to_string(), Kind::Text)]
        })
        .collect();
    Block::from_rows(rows, base)
}

/// Content between delimiters, the delimiters as tall as the content.
fn fenced(symbols: &[&str], parts: Vec<Block>) -> Block {
    let joined = beside(parts.clone());
    let (height, base) = (joined.rows.len(), joined.base);
    let mut blocks = vec![delimiter(symbols[0], height, base)];
    for (part, symbol) in parts.into_iter().zip(&symbols[1..]) {
        blocks.push(part);
        blocks.push(delimiter(symbol, height, base));
    }
    beside(blocks)
}

fn root(radicand: Block, degree: &str) -> Block {
    let index = if degree == "2" { "" } else { degree };
    let width = radicand.width;
    if radicand.rows.len() == 1 {
        let symbol = match index {
            "" => "√".to_string(),
            "3" => "∛".to_string(),
            "4" => "∜".to_string(),
            other => format!("{other}√"),
        };
        let indent = math::width(&symbol);
        let line = beside(vec![Block::text(&symbol), radicand]);
        let mut rows = Block::text(&format!("{}{}", " ".repeat(indent), rule(width))).rows;
        rows.extend(line.rows);
        return Block { rows, base: 1, width: line.width };
    }
    // A tall radical: a rule over the radicand and a stroke down its side,
    // the index before the stroke's foot.
    let indent = math::width(index);
    let last = radicand.rows.len() - 1;
    let side = (0..=last)
        .map(|r| {
            let stroke = if r == last { format!("{index}╲│") } else { format!("{} │", " ".repeat(indent)) };
            vec![(stroke, Kind::Text)]
        })
        .collect();
    let body = beside(vec![Block::from_rows(side, radicand.base), radicand]);
    let mut rows = Block::text(&format!("{} ┌{}", " ".repeat(indent), rule(width))).rows;
    rows.extend(body.rows);
    Block { rows, base: body.base + 1, width: body.width }
}

/// Operators whose scripts are limits above and below in display math.
fn has_limits(name: &str) -> bool {
    matches!(
        name,
        "sum" | "prod" | "coprod" | "bigcup" | "bigcap" | "bigoplus" | "bigotimes" | "bigvee"
            | "bigwedge" | "bigsqcup" | "lim" | "liminf" | "limsup" | "max" | "min" | "sup"
            | "inf" | "det" | "gcd" | "Pr" | "argmax" | "argmin"
    )
}

/// Whether items make a block rather than a row in `style`.
fn tall(items: &[Item], style: Style) -> bool {
    items.iter().any(|item| match item {
        Cmd(name) => {
            matches!(
                name.as_str(),
                "dfrac" | "cfrac" | "binom" | "dbinom" | "left" | "begin" | "substack"
                    | "overbrace" | "underbrace" | "displaystyle" | "limits"
            ) || style == Display
                && (matches!(name.as_str(), "frac" | "sqrt" | "operatorname" | "mathop")
                    || has_limits(name))
        }
        Group(group) => tall(group, style),
        _ => false,
    })
}

/// The scripts that follow position `i`: (subscript, superscript).
fn scripts(items: &[Item], i: &mut usize) -> Option<(Option<Vec<Item>>, Option<Vec<Item>>)> {
    let (mut sub, mut sup) = (None, None);
    loop {
        match math::peek(items, *i) {
            Some(Cmd(name)) if name == "limits" => {
                math::next(items, i);
            }
            Some(Ch(c @ ('_' | '^'))) => {
                let slot = if *c == '_' { &mut sub } else { &mut sup };
                math::next(items, i);
                if slot.replace(math::argument(items, i)?).is_some() {
                    return None; // A double script is an error in TeX.
                }
            }
            _ => return Some((sub, sup)),
        }
    }
}

/// An operator with its limits stacked over and under it; `spaced` sets it
/// apart from what follows.
fn limits(
    operator: Block,
    sub: Option<Vec<Item>>,
    sup: Option<Vec<Item>>,
    spaced: bool,
) -> Option<Block> {
    let mut blocks = Vec::new();
    if let Some(sup) = sup {
        blocks.push(block(&sup, Text)?);
    }
    let base = blocks.first().map_or(0, |b| b.rows.len());
    let narrow = operator.width;
    blocks.push(operator);
    if let Some(sub) = sub {
        blocks.push(block(&sub, Text)?);
    }
    let stacked = stack(blocks, base);
    // Limits wider than the operator keep a space from their neighbours.
    Some(if stacked.width > narrow {
        beside(vec![Block::text(" "), stacked, Block::text(" ")])
    } else if spaced {
        beside(vec![stacked, Block::text(" ")])
    } else {
        stacked
    })
}

enum OperatorLayout<'a> {
    Inline(&'a [Item]),
    Stacked(Block),
}

fn operator<'a>(
    name: &str,
    items: &'a [Item],
    start: usize,
    cursor: &mut usize,
    style: Style,
) -> Option<OperatorLayout<'a>> {
    let mut end = *cursor;
    let starred = name != "operatorname" || {
        let star = math::peek(items, end) == Some(&Ch('*'));
        if star {
            math::next(items, &mut end);
        }
        star
    };
    let row = if name == "operatorname" || name == "mathop" {
        math::row(&math::argument(items, &mut end)?, "", false)?
    } else {
        math::row(&items[start..*cursor], "", false)?
    };
    let stacked = match math::peek(items, end) {
        Some(Cmd(word)) if word == "nolimits" => false,
        Some(Cmd(word)) if word == "limits" => true,
        // Integrals keep side scripts; starred operator names take limits.
        _ => starred && style == Display && (has_limits(name) || math::large(name).is_none()),
    };
    let mut after = end;
    let (sub, sup) = scripts(items, &mut after)?;
    if !stacked || sub.is_none() && sup.is_none() {
        *cursor = end;
        return Some(OperatorLayout::Inline(&items[start..end]));
    }
    *cursor = after;
    let word = matches!(math::peek(items, after), Some(Ch(letter)) if letter.is_alphanumeric())
        || matches!(math::peek(items, after), Some(Cmd(word)) if word.chars().all(char::is_alphabetic));
    let spaced = word
        && math::plain(&row)
            .chars()
            .all(|letter| letter.is_alphabetic() || ('\u{300}'..='\u{36f}').contains(&letter));
    Some(OperatorLayout::Stacked(limits(Block::line(row), sub, sup, spaced)?))
}

/// Scripts after a block: on its row if it has one, else the superscript at
/// its top right and the subscript at its bottom right.
fn attach(mut base: Block, sub: Option<Vec<Item>>, sup: Option<Vec<Item>>) -> Option<Block> {
    let mut width = base.width;
    let last = base.rows.len() - 1;
    for (c, script) in [('_', sub), ('^', sup)] {
        let Some(script) = script else { continue };
        let piece = vec![math::scripted(c, &script, "")?];
        let r = if last == 0 { 0 } else if c == '^' { 0 } else { last };
        let column = if last == 0 { measure(&base.rows[0]).max(base.width) } else { base.width };
        put(&mut base.rows[r], column, &piece);
        width = width.max(measure(&base.rows[r]));
    }
    base.width = width;
    Some(base)
}

/// The text of an environment name argument.
fn name_of(items: &[Item]) -> Option<String> {
    items.iter().map(|item| if let Ch(c) = item { Some(*c) } else { None }).collect()
}

/// Split an environment body into rows of cells at `\\` and `&`.
fn cells(body: &[Item]) -> Option<Vec<Vec<Vec<Item>>>> {
    let mut rows = vec![vec![Vec::new()]];
    let mut depth = 0;
    let mut i = 0;
    while i < body.len() {
        let item = &body[i];
        i += 1;
        // A nested environment keeps its own rows and columns.
        match item {
            Cmd(name) if name == "begin" => depth += 1,
            Cmd(name) if name == "end" => depth -= 1,
            _ => {}
        }
        if depth > 0 || matches!(item, Cmd(name) if name == "end") {
            rows.last_mut()?.last_mut()?.push(item.clone());
            continue;
        }
        match item {
            Ch('&') => rows.last_mut()?.push(Vec::new()),
            Cmd(name) if name == "\\" || name == "cr" || name == "newline" => {
                if math::peek(body, i) == Some(&Ch('*')) {
                    math::next(body, &mut i);
                }
                math::optional(body, &mut i);
                rows.push(vec![Vec::new()]);
            }
            Cmd(name) if ["hline", "midrule", "toprule", "bottomrule"].contains(&name.as_str()) => {}
            Cmd(name) if name == "intertext" || name == "shortintertext" => {
                // Text between rows: a row of its own, outside the columns.
                let text = math::argument(body, &mut i)?;
                rows.last_mut()?.last_mut()?.extend([Cmd(name.clone()), Group(text)]);
                rows.push(vec![Vec::new()]);
            }
            Cmd(name) if name == "noalign" => return None,
            other => rows.last_mut()?.last_mut()?.push(other.clone()),
        }
    }
    let empty = |row: &Vec<Vec<Item>>| row.iter().all(|cell| cell.iter().all(math::blank));
    if rows.len() > 1 && empty(rows.last()?) {
        rows.pop(); // A final \\ ends the last row; it adds none.
    }
    Some(rows)
}

/// Cells laid out in columns. `align` gives each column's alignment
/// ('l', 'c' or 'r') and `gap` the space before it.
fn grid(
    rows: &[Vec<Vec<Item>>],
    style: Style,
    align: impl Fn(usize) -> char,
    gap: impl Fn(usize) -> usize,
) -> Option<Block> {
    let note = |row: &Vec<Vec<Item>>| {
        row.iter().flatten().find(|item| !math::blank(item)) == Some(&Cmd("intertext".into())) || row.iter().flatten().find(|item| !math::blank(item)) == Some(&Cmd("shortintertext".into()))
    };
    let blocks: Vec<Vec<Block>> =
        rows.iter().map(|row| row.iter().map(|cell| block(cell, style)).collect()).collect::<Option<_>>()?;
    let columns = blocks.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            let cells = blocks.iter().zip(rows).filter(|(_, source)| !note(source));
            cells.filter_map(|(row, _)| row.get(c)).map(|b| b.width).max().unwrap_or(0)
        })
        .collect();
    let mut lines = Vec::new();
    let mut bases = Vec::new();
    for (row, source) in blocks.into_iter().zip(rows) {
        if note(source) {
            lines.extend(row.into_iter().next()?.rows);
            continue;
        }
        let mut parts = Vec::new();
        let (mut start, mut cursor) = (0, 0);
        for (c, cell) in row.into_iter().enumerate() {
            if c > 0 {
                start += widths[c - 1] + gap(c);
            }
            let spare = widths[c] - cell.width;
            let column = start + match align(c) {
                'r' => spare,
                'c' => (spare + 1) / 2,
                _ => 0,
            };
            parts.push(Block::text(&" ".repeat(column - cursor)));
            cursor = column + cell.width;
            parts.push(cell);
        }
        let line = beside(parts);
        bases.push(lines.len() + line.base);
        lines.extend(line.rows);
    }
    let base = bases[bases.len() / 2];
    Some(Block::from_rows(lines, base))
}

/// `\begin{name}` at `i`, through its `\end`.
fn environment(items: &[Item], i: &mut usize) -> Option<Block> {
    let name = name_of(&math::argument(items, i)?)?;
    let start = *i;
    let mut depth = 0;
    let end = loop {
        match items.get(*i)? {
            Cmd(word) if word == "begin" => depth += 1,
            Cmd(word) if word == "end" && depth == 0 => break *i,
            Cmd(word) if word == "end" => depth -= 1,
            _ => {}
        }
        *i += 1;
    };
    *i = end + 1;
    if name_of(&math::argument(items, i)?)? != name {
        return None;
    }
    let mut body = &items[start..end];
    let base = name.trim_end_matches('*');
    // Arguments: a position [t], a column count {2} or a column spec {lcr}.
    let mut j = 0;
    let mut spec = Vec::new();
    if ["aligned", "alignedat", "gathered", "array", "subarray", "matrix", "pmatrix", "bmatrix",
        "Bmatrix", "vmatrix", "Vmatrix", "smallmatrix", "multlined"]
        .contains(&base)
    {
        math::optional(body, &mut j);
    }
    if ["alignat", "alignedat", "array", "subarray"].contains(&base) {
        spec = math::argument(body, &mut j)?;
    }
    body = &body[j..];
    let rows = cells(body)?;
    let matrix = |open: &str, close: &str| -> Option<Block> {
        let block = grid(&rows, Text, |_| 'c', |_| 1)?;
        Some(fenced(&[open, close], vec![block]))
    };
    match base {
        "equation" | "displaymath" | "math" => block(body, Display),
        "align" | "aligned" | "flalign" | "alignat" | "alignedat" | "split" => {
            grid(&rows, Display, |c| if c % 2 == 0 { 'r' } else { 'l' }, |c| if c % 2 == 0 { 3 } else { 0 })
        }
        "eqnarray" => grid(&rows, Display, |c| ['r', 'c', 'l'][c.min(2)], |_| 1),
        "gather" | "gathered" | "multline" | "multlined" | "subarray" => {
            grid(&rows, Display, |_| 'c', |_| 0)
        }
        "cases" => Some(fenced(&["{", ""], vec![grid(&rows, Text, |_| 'l', |_| 3)?])),
        "dcases" => Some(fenced(&["{", ""], vec![grid(&rows, Display, |_| 'l', |_| 3)?])),
        "rcases" => Some(fenced(&["", "}"], vec![grid(&rows, Text, |_| 'l', |_| 3)?])),
        "matrix" | "smallmatrix" => matrix("", ""),
        "pmatrix" => matrix("(", ")"),
        "bmatrix" => matrix("[", "]"),
        "Bmatrix" => matrix("{", "}"),
        "vmatrix" => matrix("|", "|"),
        "Vmatrix" => matrix("‖", "‖"),
        "array" => {
            let columns: Vec<char> = name_of(&spec)
                .unwrap_or_default()
                .chars()
                .filter(|c| "lcr".contains(*c))
                .collect();
            grid(&rows, Text, |c| columns.get(c).copied().unwrap_or('c'), |_| 2)
        }
        _ => None,
    }
}

/// `\left` at `i`: the parts between its delimiters and the delimiters.
fn fence(items: &[Item], i: &mut usize, style: Style) -> Option<Block> {
    let mut symbols = vec![delimiter_of(&math::next(items, i)?)?];
    let mut parts = Vec::new();
    let mut start = *i;
    let mut depth = 0;
    loop {
        let item = items.get(*i)?;
        *i += 1;
        match item {
            Cmd(name) if name == "left" => depth += 1,
            Cmd(name) if name == "right" && depth > 0 => depth -= 1,
            Cmd(name) if (name == "right" || name == "middle") && depth == 0 => {
                parts.push(block(&items[start..*i - 1], style)?);
                symbols.push(delimiter_of(&math::next(items, i)?)?);
                start = *i;
                if name == "right" {
                    return Some(fenced(&symbols, parts));
                }
            }
            _ => {}
        }
    }
}

/// A brace under or over a block, its tip toward the annotation.
fn braced(content: Block, over: bool, note: Option<Block>) -> Block {
    let (left, middle, right) = if over { ("┌", "┴", "┐") } else { ("└", "┬", "┘") };
    let corners = math::width(left) + math::width(right);
    let spare = content.width.max(corners) - corners;
    let tip = math::width(middle);
    let stroke = if spare >= tip {
        let arm = (spare - tip) / 2;
        format!("{left}{}{middle}{}{right}", rule(arm), rule(spare - tip - arm))
    } else {
        format!("{left}{}{right}", rule(spare))
    };
    let mut blocks = vec![content, Block::text(&stroke)];
    if over {
        blocks.reverse();
    }
    if let Some(note) = note {
        if over { blocks.insert(0, note) } else { blocks.push(note) }
    }
    let at = if over { blocks.len() - 1 } else { 0 };
    let base = blocks[..at].iter().map(|b| b.rows.len()).sum::<usize>() + blocks[at].base;
    stack(blocks, base)
}

/// Lay out math in `style`.
fn block(items: &[Item], mut style: Style) -> Option<Block> {
    let mut parts: Vec<Block> = Vec::new();
    let mut pending: Vec<Item> = Vec::new();
    fn flush(parts: &mut Vec<Block>, pending: &mut Vec<Item>) -> Option<()> {
        if !pending.is_empty() {
            parts.push(Block::line(math::row(pending, "", false)?));
            pending.clear();
        }
        Some(())
    }
    let mut i = 0;
    while i < items.len() {
        let item = &items[i];
        let start = i;
        i += 1;
        let name = if let Cmd(name) = item { name.as_str() } else { "" };
        match item {
            Cmd(_) if name == "displaystyle" || name == "textstyle" => {
                style = if name == "displaystyle" { Display } else { Text };
            }
            // In text style a fraction or root is written on the row.
            Cmd(_) if name == "tfrac" || style == Text && matches!(name, "frac" | "sqrt") => {
                if name == "sqrt" {
                    math::optional(items, &mut i);
                }
                for _ in 0..if name == "sqrt" { 1 } else { 2 } {
                    math::argument(items, &mut i)?;
                }
                if tall(&items[start + 1..i], style) {
                    return None;
                }
                pending.extend_from_slice(&items[start..i]);
            }
            Cmd(_) if matches!(name, "frac" | "dfrac" | "cfrac" | "binom" | "dbinom") => {
                flush(&mut parts, &mut pending)?;
                // A fraction's parts are in text style; a continued fraction
                // goes on in display style.
                let inner = if name == "cfrac" { Display } else { Text };
                let top = block(&math::argument(items, &mut i)?, inner)?;
                let bottom = block(&math::argument(items, &mut i)?, inner)?;
                parts.push(if name.ends_with("binom") {
                    let base = top.rows.len();
                    fenced(&["(", ")"], vec![stack(vec![top, bottom], base.saturating_sub(1))])
                } else {
                    fraction(top, bottom)
                });
            }
            Cmd(_) if name == "sqrt" => {
                flush(&mut parts, &mut pending)?;
                let degree = match math::optional(items, &mut i) {
                    Some(degree) => math::plain(&math::row(&degree, "", true)?),
                    None => String::new(),
                };
                parts.push(root(block(&math::argument(items, &mut i)?, style)?, &degree));
            }
            Cmd(_) if name == "left" => {
                flush(&mut parts, &mut pending)?;
                parts.push(fence(items, &mut i, style)?);
            }
            Cmd(_) if name == "begin" => {
                flush(&mut parts, &mut pending)?;
                parts.push(environment(items, &mut i)?);
            }
            Cmd(_) if name == "substack" => {
                flush(&mut parts, &mut pending)?;
                parts.push(grid(&cells(&math::argument(items, &mut i)?)?, Text, |_| 'c', |_| 0)?);
            }
            Cmd(_) if name == "overbrace" || name == "underbrace" => {
                flush(&mut parts, &mut pending)?;
                let over = name == "overbrace";
                let content = block(&math::argument(items, &mut i)?, style)?;
                let mut j = i;
                let (sub, sup) = scripts(items, &mut j)?;
                let note = if over { sup.filter(|_| sub.is_none()) } else { sub.filter(|_| sup.is_none()) };
                let note = match note {
                    Some(note) => {
                        i = j;
                        Some(block(&note, Text)?)
                    }
                    None => None,
                };
                parts.push(braced(content, over, note));
            }
            Cmd(_)
                if has_limits(name)
                    || math::large(name).is_some()
                    || name == "operatorname"
                    || name == "mathop" =>
            {
                match operator(name, items, start, &mut i, style)? {
                    OperatorLayout::Inline(source) => pending.extend_from_slice(source),
                    OperatorLayout::Stacked(block) => {
                        flush(&mut parts, &mut pending)?;
                        parts.push(block);
                    }
                }
            }
            Ch('^' | '_') if pending.iter().all(math::blank) && !parts.is_empty() => {
                pending.clear();
                i -= 1;
                let (sub, sup) = scripts(items, &mut i)?;
                let base = parts.pop()?;
                parts.push(attach(base, sub, sup)?);
            }
            Ch(c @ ('^' | '_')) => {
                let script = math::argument(items, &mut i)?;
                pending.push(Ch(*c));
                pending.push(Group(script));
            }
            // A command's argument stays with it: \hphantom{\frac12}.
            Group(group) if tall(group, style) && !matches!(pending.last(), Some(Cmd(c)) if c.chars().all(|c| c.is_ascii_alphabetic())) => {
                flush(&mut parts, &mut pending)?;
                parts.push(block(group, style)?);
            }
            other => pending.push(other.clone()),
        }
    }
    flush(&mut parts, &mut pending)?;
    Some(if parts.is_empty() { Block::line(Row::new()) } else { beside(parts) })
}

/// A display grid, or None to keep the source visible.
pub fn display(raw: &str, defs: &Definitions) -> Option<Display> {
    let body = if let Some(rest) = raw.strip_prefix("$$") {
        rest.strip_suffix("$$")?
    } else if let Some(rest) = raw.strip_prefix("\\[") {
        rest.strip_suffix("\\]")?
    } else if let Some(rest) = raw.strip_prefix('$') {
        rest.strip_suffix('$')? // Inline math standing on lines of its own.
    } else {
        raw
    };
    let items = math::expand(&math::parse(body)?, defs, 0)?;
    // Rows broken at the top level, as KaTeX allows, are centered.
    let mut depth = 0i32;
    let broken = items.iter().any(|item| match item {
        Cmd(c) if c == "begin" => {
            depth += 1;
            false
        }
        Cmd(c) if c == "end" => {
            depth -= 1;
            false
        }
        Cmd(c) => depth == 0 && (c == "\\" || c == "newline"),
        _ => false,
    });
    let laid = if broken { grid(&cells(&items)?, Display, |_| 'c', |_| 0)? } else { block(&items, Display)? };
    let mut rows: Vec<Row> = laid.rows;
    for row in &mut rows {
        while let Some((text, _)) = row.last_mut() {
            let kept = text.trim_end().len();
            text.truncate(kept);
            if !text.is_empty() {
                break;
            }
            row.pop();
        }
    }
    while rows.first().is_some_and(Vec::is_empty) {
        rows.remove(0);
    }
    while rows.last().is_some_and(Vec::is_empty) {
        rows.pop();
    }
    // Drop the indentation every row shares.
    let lead = |row: &Row| {
        row.first().map_or(usize::MAX, |(text, _)| text.len() - text.trim_start_matches(' ').len())
    };
    let indent = rows.iter().filter(|row| !row.is_empty()).map(lead).min().unwrap_or(0);
    for row in rows.iter_mut().filter(|row| !row.is_empty()) {
        row[0].0.drain(..indent);
        if row[0].0.is_empty() {
            row.remove(0);
        }
    }
    let width = rows.iter().map(measure).max().unwrap_or(0);
    if width == 0 {
        return None;
    }
    Some(Display { rows, width })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(source: &str, defs: &str) -> Vec<String> {
        let mut scope = crate::scan::definitions(include_str!("prelude.tex"));
        scope.extend(crate::scan::definitions(defs));
        let layout = display(source, &scope).expect(source);
        layout.rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|(text, kind)| match kind {
                        Kind::Sub => format!("_[{text}]"),
                        Kind::Sup => format!("^[{text}]"),
                        Kind::Text => text.clone(),
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fractions_center_on_their_bar() {
        assert_eq!(
            rows(r"\[ Y(\omega)=\frac{I(\omega)}{V(\omega)}=P_{33}^{\mathrm{tot}},\qquad \omega=2\pi f. \]", ""),
            ["     I(ω)", "Y(ω)=────=P₃₃^[tot],    ω=2πf.", "     V(ω)"]
        );
    }

    #[test]
    fn scripts_after_blocks_ignore_math_whitespace() {
        for (base, expected) in [
            (r"\frac{a}{b}", vec!["a^[n]", "─ +z", "b_[i]"]),
            (r"\sqrt{x}", vec![" ─^[n]", "√x_[i]+z"]),
            (r"\left(\frac{a}{b}\right)", vec!["⎛a⎞^[n]", "⎜─⎟ +z", "⎝b⎠_[i]"]),
            (r"\begin{pmatrix}a\\b\end{pmatrix}", vec!["⎛a⎞^[n]", "⎝b⎠_[i]+z"]),
            (r"{\frac{a}{b}+c}", vec!["a  ^[n]", "─+c +z", "b  _[i]"]),
        ] {
            for gap in ["", " ", "\n", " % comment\n "] {
                for script in ["_i^n", "^n_i"] {
                    let source = format!(r"\[{base}{gap}{script}+z\]");
                    assert_eq!(rows(&source, ""), expected, "{source}");
                }
            }
        }
        assert_eq!(rows(r"\[x _i ^n+y^2\]", ""), ["x_[i]^[n]+y²"]);
    }

    #[test]
    fn style_declarations_after_scripts_keep_their_scope() {
        assert_eq!(
            rows(r"\[\textstyle\sum\limits_i\displaystyle\frac{a}{b}+\sqrt{x}\]", ""),
            [" a  ─", "∑─+√x", "ib"]
        );
        assert_eq!(
            rows(r"\[\textstyle{\displaystyle\frac{a}{b}}_i\displaystyle\frac{c}{d}\]", ""),
            ["a c", "─ ─", "b_[i]d"]
        );
        assert_eq!(
            rows(r"\[\textstyle{\sum\limits_i\displaystyle\frac{a}{b}}+\frac{c}{d}\]", ""),
            [" a", "∑─+c/d", "ib"]
        );
    }

    #[test]
    fn an_odd_cell_keeps_a_visible_fraction_rule() {
        math::set_cells(math::Cells { wide: true, overrides: vec![(0x03b1, 0x03b1, 1)] });
        assert_eq!(rows(r"\[ \frac{\alpha}{b} \]", ""), ["α", "−", "b"]);
        assert_eq!(rows(r"\[ \frac{abc}{d} \]", ""), ["abc", "─−", " d"]);
        math::set_cells(math::Cells { wide: true, overrides: vec![(0x2212, 0x2212, 2)] });
        assert_eq!(rows(r"\[ \frac{a}{b} \]", ""), ["a", "-", "b"]);
        math::set_cells(math::Cells::default());
    }

    #[test]
    fn wide_delimiters_and_braces_report_their_drawn_width() {
        math::set_cells(math::Cells { wide: true, overrides: vec![] });
        let scope = crate::scan::definitions(include_str!("prelude.tex"));
        for source in [
            r"\left\langle\frac{a}{b}\right\rangle",
            r"\sqrt{\left|\frac{a}{b}\right|}",
            r"\overbrace{abcdef}^{n}=\underbrace{abc}_{m}",
        ] {
            let items = math::expand(&math::parse(source).unwrap(), &scope, 0).unwrap();
            let laid = block(&items, Display).unwrap();
            assert_eq!(laid.width, laid.rows.iter().map(measure).max().unwrap(), "{source}");
        }
        let brace = braced(Block::text("abcdef"), true, None);
        assert_eq!(brace.width, 6);
        assert_eq!(math::plain(&brace.rows[0]), "┌┴┐");
        math::set_cells(math::Cells::default());
    }

    #[test]
    fn limits_stack_and_integrals_keep_scripts() {
        assert_eq!(
            rows(r"\[ L=\frac{1}{N}\sum_{i=1}^{N}\min_{k}e_{ik}+\int_0^1 f \]", ""),
            ["  1  N", "L=─  ∑  min e_[ik]+∫₀¹f", "  N i=1  k"]
        );
    }

    #[test]
    fn environments_align_and_fence() {
        assert_eq!(
            rows(r"\begin{align} a&=b+c,\label{x}\\ dd&=\frac{1}{2}. \end{align}", ""),
            [" a=b+c,", "   1", "dd=─.", "   2"]
        );
        assert_eq!(
            rows(r"\[ |x|=\begin{cases} x & x\geq0,\\ -x & x<0. \end{cases} \]", ""),
            ["    ⎰x    x≥0,", "|x|=⎱−x   x<0."]
        );
        assert_eq!(
            rows(r"\[ \left\{ x\in X : \begin{aligned} &a,\\ &b \end{aligned} \right\} \]", ""),
            ["⎰      a,⎱", "⎱x∈X : b ⎰"]
        );
    }

    #[test]
    fn roots_binomials_and_intertext() {
        assert_eq!(rows(r"\[ s=\sqrt{a^2+b}+\binom{n}{k} \]", ""), ["   ────", "s=√a²+b+⎛n⎞", "        ⎝k⎠"]);
        assert_eq!(
            rows(r"\begin{align} a&=1,\\ \intertext{and} bb&=2. \end{align}", ""),
            [" a=1,", "and", "bb=2."]
        );
        assert_eq!(rows("$\\displaystyle\n\\sum_{\\substack{i<m\\\\ j<n}} P$", ""), [" ∑  P", "i<m", "j<n"]);
    }

    #[test]
    fn fraction_parts_and_matrix_cells_are_text_style() {
        // As in TeX: inside a display fraction, fractions, roots and limits
        // are written on the row.
        assert_eq!(
            rows(r"\[ e=\frac{\sum_{m=1}^{F}|y_m|^2}{\frac{a}{b}+\sqrt{2}} \]", ""),
            ["  ∑_[m=1]^[F]|y_[m]|²", "e=──────────", "   a/b+√(2)"]
        );
        assert_eq!(rows(r"\[ \begin{pmatrix}\frac12&0\\0&1\end{pmatrix} \]", ""), ["⎛1/2 0⎞", "⎝ 0  1⎠"]);
    }

    #[test]
    fn nested_environments_and_top_level_rows() {
        assert_eq!(
            rows(r"\begin{align} x&=\begin{pmatrix}1&2\\3&4\end{pmatrix}\\ y&=0 \end{align}", ""),
            ["  ⎛1 2⎞", "x=⎝3 4⎠", "y=0"]
        );
        assert_eq!(rows(r"\[ a+b \\ c \]", ""), ["a+b", " c"]);
    }

    #[test]
    fn braces_carry_their_notes() {
        assert_eq!(
            rows(r"\[ \overbrace{a+b+c}^{n}=\underbrace{xyz}_{\text{one}} \]", ""),
            ["  n", "┌─┴─┐", "a+b+c=xyz", "      └┬┘", "      one"]
        );
    }

    #[test]
    fn text_math_and_kerns() {
        assert_eq!(
            rows(r"\[ \partial_s f = g\quad\text{for $x=x_0$.}\kern-2em \]", ""),
            ["∂_[s]f=g   for x=x₀."]
        );
    }
}

//! Document rendering and the resident formula cache.

use crate::{layout, math, scan};
use serde::Serialize;
use std::collections::HashMap;
use std::rc::Rc;

/// An inline row or a display grid, with widths measured in text cells.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Layout {
    Inline(math::Inline),
    Display(layout::Display),
}

/// A layout and its independent source position in a document.
#[derive(Debug, Serialize)]
pub struct Rendered {
    #[serde(flatten)]
    pub source: scan::Formula,
    #[serde(flatten)]
    pub layout: Rc<Layout>,
}

/// Inline formulas stay on one row; display formulas may stack rows.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Mode {
    Inline,
    Display,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    scope: Rc<str>,
    raw: String,
    mode: Mode,
}

struct Scope {
    definitions: scan::Definitions,
    key: Rc<str>,
}

struct Cached {
    layout: Option<Rc<Layout>>,
    used: u64,
}

/// One consumer's resident renderer and bounded formula cache.
///
/// Source positions are never cached. Duplicate or moved formulas share
/// layouts, including across document and individual-formula requests.
pub struct Kernel {
    prelude: scan::Definitions,
    cache: HashMap<Key, Cached>,
    tick: u64,
    limit: usize,
    cells: math::Cells,
    scope: Option<Rc<Scope>>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self {
            prelude: scan::definitions(include_str!("prelude.tex")),
            cache: HashMap::new(),
            tick: 0,
            limit: 8192,
            cells: math::Cells::default(),
            scope: None,
        }
    }
}

impl Kernel {
    /// Render a document without changing its source.
    ///
    /// `filetype` is `"tex"` for TeX scanning, or `"markdown"` / `"cogitatum"`
    /// for markup scanning. `context` supplies TeX definitions, such as a main
    /// file's preamble; local definitions take precedence. Unsupported formulas
    /// are omitted, so the consumer can leave their source visible.
    pub fn document(
        &mut self,
        source: &str,
        filetype: &str,
        context: &str,
        cells: math::Cells,
    ) -> Vec<Rendered> {
        let (defs, found) = scan::formulas(source, filetype, context);
        let scope = self.prepare(defs, cells);
        let mut result = Vec::new();
        for formula in found {
            let mode = if formula.display { Mode::Display } else { Mode::Inline };
            if let Some(layout) = self.cached(&formula.raw, mode, &scope) {
                result.push(Rendered { source: formula, layout });
            }
        }
        self.trim();
        result
    }

    /// Lay out a formula body directly, without scanning a document.
    ///
    /// Delimiters (`$…$`, `\[…\]`, etc.) are optional. The context supplies
    /// command definitions. `None` means the source should stay visible.
    pub fn layout(
        &mut self,
        source: &str,
        mode: Mode,
        context: &str,
        cells: math::Cells,
    ) -> Option<Rc<Layout>> {
        let scope = self.prepare(scan::definitions(context), cells);
        let result = self.cached(source, mode, &scope);
        self.trim();
        result
    }

    fn prepare(&mut self, defs: scan::Definitions, cells: math::Cells) -> Rc<Scope> {
        if cells != self.cells {
            self.cells = cells.clone();
            self.cache.clear();
        }
        // Every entry point installs its own width policy. Alternating
        // independent kernels on the same thread must not inherit it.
        math::set_cells(cells);
        let mut definitions = self.prelude.clone();
        definitions.extend(defs);
        self.tick += 1;
        // Edits usually leave definitions unchanged. Reuse their allocation:
        // otherwise every new formula retains another copy of the preamble.
        if let Some(scope) = &self.scope {
            if scope.definitions == definitions {
                return scope.clone();
            }
        }
        let key = serde_json::to_string(&definitions).unwrap().into();
        let scope = Rc::new(Scope { definitions, key });
        self.scope = Some(scope.clone());
        scope
    }

    fn cached(&mut self, source: &str, mode: Mode, scope: &Scope) -> Option<Rc<Layout>> {
        let key = Key { scope: scope.key.clone(), raw: source.to_string(), mode };
        let cached = self.cache.entry(key).or_insert_with_key(|key| Cached {
            layout: match key.mode {
                Mode::Inline => math::inline(&key.raw, &scope.definitions).map(Layout::Inline),
                Mode::Display => layout::display(&key.raw, &scope.definitions).map(Layout::Display),
            }
            .map(Rc::new),
            used: 0,
        });
        cached.used = self.tick;
        cached.layout.clone()
    }

    fn trim(&mut self) {
        if self.cache.len() <= self.limit {
            return;
        }
        let mut oldest: Vec<_> =
            self.cache.iter().map(|(key, value)| (key.clone(), value.used)).collect();
        let excess = self.cache.len() - self.limit;
        oldest.select_nth_unstable_by_key(excess - 1, |(_, used)| *used);
        // Entries from one document share an age. Evict only the excess,
        // rather than dropping that whole age (possibly the entire document).
        for (key, _) in oldest.into_iter().take(excess) {
            self.cache.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(kernel: &mut Kernel, source: &str) -> Vec<String> {
        kernel
            .document(source, "tex", "", math::Cells::default())
            .iter()
            .map(|formula| match &*formula.layout {
                Layout::Inline(inline) => math::plain(&inline.segments),
                Layout::Display(_) => panic!("expected inline math"),
            })
            .collect()
    }

    #[test]
    fn layouts_follow_their_definitions_and_the_cache_stays_bounded() {
        let mut kernel = Kernel { limit: 4, ..Kernel::default() };
        assert_eq!(texts(&mut kernel, "\\newcommand{\\x}{y}\n$\\x$ $\\frac{a}{$"), ["y"]);
        assert_eq!(texts(&mut kernel, "\\newcommand{\\x}{z}\n$\\x$"), ["z"]);
        for number in 0..10 {
            texts(&mut kernel, &format!("${number}$"));
        }
        assert_eq!(kernel.cache.len(), 4);
    }

    #[test]
    fn a_large_document_keeps_results_and_a_full_cache() {
        let mut kernel = Kernel { limit: 4, ..Kernel::default() };
        assert_eq!(texts(&mut kernel, "$0$ $1$ $2$ $3$ $4$ $5$"), ["0", "1", "2", "3", "4", "5"]);
        assert_eq!(kernel.cache.len(), 4);
    }

    #[test]
    fn duplicate_and_moved_formulas_share_a_layout_but_not_a_position() {
        let mut kernel = Kernel::default();
        let first = kernel.document("$x_a$ $x_a$", "tex", "", math::Cells::default());
        assert!(Rc::ptr_eq(&first[0].layout, &first[1].layout));
        assert_ne!(first[0].source.first, first[1].source.first);
        let moved = kernel.document("Prose.\n$x_a$", "tex", "", math::Cells::default());
        assert!(Rc::ptr_eq(&first[0].layout, &moved[0].layout));
        assert_eq!(moved[0].source.first, [2, 1]);
    }

    #[test]
    fn formula_edits_share_the_unchanged_definition_scope() {
        let mut kernel = Kernel::default();
        texts(&mut kernel, "\\newcommand{\\state}{x}\n$\\state_a$");
        let original = kernel.scope.as_ref().unwrap().key.clone();
        for index in 0..20 {
            texts(&mut kernel, &format!("\\newcommand{{\\state}}{{x}}\n$\\state_{{{index}}}$"));
        }
        assert!(kernel.cache.keys().all(|key| Rc::ptr_eq(&key.scope, &original)));
        assert_eq!(texts(&mut kernel, "\\newcommand{\\state}{y}\n$\\state_a$"), ["ya"]);
        assert!(!Rc::ptr_eq(&kernel.scope.as_ref().unwrap().key, &original));
        // Returning to the old definitions still finds their cached layouts.
        let old = kernel.cache.len();
        assert_eq!(texts(&mut kernel, "\\newcommand{\\state}{x}\n$\\state_a$"), ["xa"]);
        assert_eq!(kernel.cache.len(), old);
    }

    #[test]
    fn a_recent_formula_survives_eviction() {
        let mut kernel = Kernel { limit: 2, ..Kernel::default() };
        kernel.document("$x$", "tex", "", math::Cells::default());
        let recent = kernel.document("$y$", "tex", "", math::Cells::default());
        kernel.document("$z$", "tex", "", math::Cells::default());
        let repeated = kernel.document("$y$", "tex", "", math::Cells::default());
        assert!(Rc::ptr_eq(&recent[0].layout, &repeated[0].layout));
        assert_eq!(kernel.cache.len(), 2);
    }

    #[test]
    fn ambiwidth_relays_out_cached_formulas() {
        let mut kernel = Kernel::default();
        let mut layout = |wide| {
            serde_json::to_value(kernel.document(
                r"\[ \frac{\alpha}{b} \]",
                "tex",
                "",
                math::Cells { wide, overrides: vec![] },
            ))
            .unwrap()
        };
        let narrow = layout(false);
        assert_ne!(narrow, layout(true), "a wide α needs a wider rule");
        assert_eq!(layout(false), narrow);
    }
}

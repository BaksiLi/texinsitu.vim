//! Exercise only the public library API, without the JSON-lines executable.

use std::rc::Rc;
use texinsitu::{Cells, Kernel, Kind, Layout, Mode};

fn text(layout: &Layout) -> String {
    match layout {
        Layout::Inline(line) => line.segments.iter().map(|(text, _)| text.as_str()).collect(),
        Layout::Display(grid) => grid.rows.iter()
            .map(|row| row.iter().map(|(text, _)| text.as_str()).collect::<String>())
            .collect::<Vec<_>>().join("\n"),
    }
}

#[test]
fn a_consumer_can_render_a_formula_without_a_document_or_json() {
    let mut kernel = Kernel::default();
    let layout = kernel.layout(
        r"\state_a^2", Mode::Inline, r"\newcommand{\state}{\theta}", Cells::default(),
    ).unwrap();
    let Layout::Inline(line) = &*layout else { panic!("expected one row") };
    assert_eq!(line.segments, [("θ".into(), Kind::Text), ("a".into(), Kind::Sub),
                              ("²".into(), Kind::Text)]);
    assert_eq!(line.width, 3);

    let grid = kernel.layout(r"\frac{a}{b}", Mode::Display, "", Cells::default()).unwrap();
    assert_eq!(text(&grid), "a\n─\nb");
    let inline = kernel.layout(r"\frac{a}{b}", Mode::Inline, "", Cells::default()).unwrap();
    assert_eq!(text(&inline), "a/b", "mode is part of the cache key");
}

#[test]
fn direct_and_document_consumers_share_layouts_but_keep_source_positions() {
    let mut kernel = Kernel::default();
    let source = "$x_a$";
    let direct = kernel.layout(source, Mode::Inline, "", Cells::default()).unwrap();
    let document = kernel.document(&format!("中文 {source} {source}"), "markdown", "", Cells::default());
    assert_eq!(document.len(), 2);
    for formula in &document {
        assert!(Rc::ptr_eq(&direct, &formula.layout));
        assert_eq!(formula.source.raw, source);
    }
    assert_eq!(document[0].source.first, [1, 8]);
    assert_eq!(document[1].source.first, [1, 14]);
    assert_eq!(document[0].source.last, [1, 13]);
}

#[test]
fn independent_consumers_restore_their_width_policy_on_each_call() {
    let mut narrow = Kernel::default();
    let mut wide = Kernel::default();
    let render = |kernel: &mut Kernel, wide| {
        kernel.layout(r"\frac{\alpha}{b}", Mode::Display, "", Cells { wide, overrides: vec![] }).unwrap()
    };
    let first = render(&mut narrow, false);
    assert_eq!(text(&first), "α\n─\nb");
    assert_eq!(text(&render(&mut wide, true)), "α\n─\n b");
    // Use a different formula too: a cache hit alone cannot verify that the
    // narrower consumer restored the width policy before laying out new work.
    let next = narrow.layout(r"\frac{\alpha\beta}{b}", Mode::Display, "", Cells::default()).unwrap();
    assert_eq!(text(&next), "αβ\n──\n b");
    assert!(Rc::ptr_eq(&first, &render(&mut narrow, false)));
    let Layout::Display(grid) = &*render(&mut wide, true) else { panic!("expected a grid") };
    assert_eq!(grid.width, 2);
}

#[test]
fn unsupported_results_stay_source_and_definitions_can_repair_them() {
    let mut kernel = Kernel::default();
    let source = r"\fragment{q}";
    assert!(kernel.layout(source, Mode::Inline, r"\newcommand{\fragment}[1]{#2}", Cells::default()).is_none());
    let repaired = kernel.layout(source, Mode::Inline, r"\newcommand{\fragment}[1]{#1^2}", Cells::default()).unwrap();
    assert_eq!(text(&repaired), "q²");
    assert!(kernel.layout(r"\input{file}", Mode::Display, "", Cells::default()).is_none());
}

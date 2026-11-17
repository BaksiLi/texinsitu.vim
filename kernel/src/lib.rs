//! TeX formulas as text cells, independent of an editor or transport.
//!
//! [`Kernel::document`] finds formulas in a source document and returns their
//! positions and shared layouts. [`Kernel::layout`] takes a formula directly,
//! for a consumer that already knows its math ranges.
//!
//! ```rust
//! use texinsitu::{Cells, Kernel, Layout, Mode};
//!
//! let mut kernel = Kernel::default();
//! let rendered = kernel.layout(r"x^2+\alpha", Mode::Inline, "", Cells::default()).unwrap();
//! let Layout::Inline(line) = &*rendered else { unreachable!() };
//! assert_eq!(line.segments[0].0, "x²+α");
//! ```
//!
//! This library ships with the Vim plugin. Its Rust API and JSON protocol
//! are experimental; it is not a separately versioned core package yet.

mod engine;
mod layout;
mod math;
mod scan;

pub use engine::{Kernel, Layout, Mode, Rendered};
pub use layout::Display;
pub use math::{Cells, Inline, Kind, Row};
pub use scan::Formula;

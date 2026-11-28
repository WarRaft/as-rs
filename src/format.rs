//! Formatting AngelScript: each line moved left or right to its level, never a
//! line added, removed or joined (see [`parser_rs::format`]). Blocks are
//! braces, so the shared [`braces`] reads the levels from the tokens,
//! with `case` labels at their `switch`'s level and their code deeper.

use parser_rs::format::{braces, edits, BraceStyle, Indent};
use parser_rs::Edit;

use crate::lexer::SPEC;

const STYLE: BraceStyle = BraceStyle { case_labels: true };

/// The level of each line of `src`.
pub fn indents(src: &str) -> Vec<Indent> {
    indents_at(src, 1, 0)
}

/// [`indents`] of a text starting on line `first_line` of its file,
/// `base` levels deep.
pub fn indents_at(src: &str, first_line: usize, base: usize) -> Vec<Indent> {
    braces(src, &SPEC, first_line, base, STYLE)
}

/// The edits that indent `src`'s lines `lines` (1-based, inclusive),
/// each level `unit`.
pub fn format(src: &str, unit: &str, lines: std::ops::RangeInclusive<usize>) -> Vec<Edit> {
    edits(src, &indents(src), unit, lines)
}

#[cfg(test)]
#[path = "format.test.rs"]
mod tests;

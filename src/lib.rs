//! A parser, linter and semantic highlighter for AngelScript. Structured
//! like the other language crates; everything not AngelScript-specific
//! comes from `parser-rs` (see its README for the shared design).

pub mod ast;
pub mod format;
pub mod html;
pub mod lexer;
pub mod lint;
pub mod parser;
pub mod printer;
pub mod query;
pub mod semantic;
pub mod token;

pub use ast::{ColumnEncoding, Script, Span};
pub use html::render_html;
pub use lint::{lint, Diagnostic, Level};
pub use parser::{ParseError, Parser};
pub use printer::{render_dot, render_tree};
pub use query::{find_node_at, find_node_in, AstRef};
pub use semantic::{encoded_semantic_tokens, semantic_tokens, semantic_tokens_and_symbols, symbols};

/// Parses AngelScript source into an AST.
pub fn parse(src: &str) -> Result<Script, ParseError> {
    Parser::parse_str(src)
}

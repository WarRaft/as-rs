use crate::token::{AS_KEYWORDS, Token};
use parser_rs::{LexSpec, NumberSpec, ParseError, Punct};

/// AngelScript's lexical rules for the shared [`parser_rs::lexer`]: C-style
/// comments, `"..."`/`'...'`/`"""heredoc"""` strings, `0b`/`0o`/`0d`/`0x`
/// integers, exponents and `f` suffixes on reals, and C++-like operators
/// plus `**`, `>>>`, `^^`, `@`.
pub const SPEC: LexSpec = LexSpec {
    keywords: AS_KEYWORDS,
    block_comment: Some(("/*", "*/")),
    single_quote_strings: true,
    heredocs: true,
    numbers: NumberSpec {
        exponent: true,
        leading_dot: true,
        radix_prefixes: true,
        float_suffix: true,
        trailing_dot: false,
        leading_zero_octal: false,
        dollar_hex: false,
    },
    puncts: &[
        Punct::Plus,
        Punct::Minus,
        Punct::Star,
        Punct::Slash,
        Punct::Percent,
        Punct::StarStar,
        Punct::PlusPlus,
        Punct::MinusMinus,
        Punct::Amp,
        Punct::Pipe,
        Punct::Caret,
        Punct::Tilde,
        Punct::Shl,
        Punct::Shr,
        Punct::UShr,
        Punct::Bang,
        Punct::AndAnd,
        Punct::OrOr,
        Punct::CaretCaret,
        Punct::EqEq,
        Punct::NotEq,
        Punct::Lt,
        Punct::Gt,
        Punct::LtEq,
        Punct::GtEq,
        Punct::Assign,
        Punct::PlusAssign,
        Punct::MinusAssign,
        Punct::StarAssign,
        Punct::SlashAssign,
        Punct::PercentAssign,
        Punct::StarStarAssign,
        Punct::AmpAssign,
        Punct::PipeAssign,
        Punct::CaretAssign,
        Punct::ShlAssign,
        Punct::ShrAssign,
        Punct::UShrAssign,
        Punct::Question,
        Punct::Colon,
        Punct::ColonColon,
        Punct::Dot,
        Punct::Comma,
        Punct::Semicolon,
        Punct::At,
        Punct::LParen,
        Punct::RParen,
        Punct::LBracket,
        Punct::RBracket,
        Punct::LBrace,
        Punct::RBrace,
    ],
    ..LexSpec::BASE
};

pub struct Lexer;

impl Lexer {
    pub fn tokenize(src: &str) -> Result<Vec<Token>, ParseError> {
        parser_rs::tokenize(src, &SPEC)
    }
}

#[cfg(test)]
#[path = "lexer.test.rs"]
mod tests;

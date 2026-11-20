//! AngelScript's token vocabulary: which of the shared [`Kw`]s are active,
//! on top of the shared [`parser_rs::TokenKind`] (identifiers, literals,
//! [`Punct`]).
//!
//! Only AngelScript's *reserved* words are keywords. Its contextual words
//! stay identifiers and the parser recognizes them where they apply, so
//! they remain usable as names elsewhere: `abstract`, `explicit`,
//! `external`, `final`, `from`, `function`, `get`, `override`, `property`,
//! `set`, `shared`, `super`, `this`.

pub use parser_rs::{Kw, KwSet, Punct};

/// AngelScript's reserved keywords.
pub const AS_KEYWORDS: KwSet = KwSet::of(&[
    Kw::And,
    Kw::Auto,
    Kw::Bool,
    Kw::Break,
    Kw::Case,
    Kw::Cast,
    Kw::Catch,
    Kw::Class,
    Kw::Const,
    Kw::Continue,
    Kw::Default,
    Kw::Do,
    Kw::Double,
    Kw::Else,
    Kw::Enum,
    Kw::False,
    Kw::Float,
    Kw::For,
    Kw::Funcdef,
    Kw::If,
    Kw::Import,
    Kw::In,
    Kw::InOut,
    Kw::Int,
    Kw::Int8,
    Kw::Int16,
    Kw::Int32,
    Kw::Int64,
    Kw::Interface,
    Kw::Is,
    Kw::Mixin,
    Kw::Namespace,
    Kw::Not,
    Kw::Null,
    Kw::Or,
    Kw::Out,
    Kw::Private,
    Kw::Protected,
    Kw::Return,
    Kw::Switch,
    Kw::True,
    Kw::Try,
    Kw::Typedef,
    Kw::UInt,
    Kw::UInt8,
    Kw::UInt16,
    Kw::UInt32,
    Kw::UInt64,
    Kw::Void,
    Kw::While,
    Kw::Xor,
]);

/// The built-in primitive type keywords.
pub const PRIMITIVES: KwSet = KwSet::of(&[
    Kw::Void,
    Kw::Bool,
    Kw::Int,
    Kw::Int8,
    Kw::Int16,
    Kw::Int32,
    Kw::Int64,
    Kw::UInt,
    Kw::UInt8,
    Kw::UInt16,
    Kw::UInt32,
    Kw::UInt64,
    Kw::Float,
    Kw::Double,
    Kw::Auto,
]);

/// An AngelScript token kind (the shared one).
pub type TokenKind = parser_rs::TokenKind;

/// An AngelScript token.
pub type Token = parser_rs::Token<TokenKind>;

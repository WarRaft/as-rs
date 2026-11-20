use super::*;
use crate::token::{Kw, TokenKind};

fn kinds(src: &str) -> Vec<TokenKind> {
    Lexer::tokenize(src)
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

#[test]
fn reserved_words_are_keywords_contextual_ones_are_not() {
    assert_eq!(
        kinds("class int const this get"),
        vec![
            TokenKind::Keyword(Kw::Class),
            TokenKind::Keyword(Kw::Int),
            TokenKind::Keyword(Kw::Const),
            TokenKind::Ident("this".into()),
            TokenKind::Ident("get".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn tokenizes_literals() {
    assert_eq!(
        kinds("42 0xFF 0b101 0o17 1.5f 2e3 .5 'c' \"s\\n\" \"\"\"raw\n\"\"\""),
        vec![
            TokenKind::Int(42),
            TokenKind::Int(255),
            TokenKind::Int(5),
            TokenKind::Int(15),
            TokenKind::Real(1.5),
            TokenKind::Real(2000.0),
            TokenKind::Real(0.5),
            TokenKind::Str("c".into()),
            TokenKind::Str("s\n".into()),
            TokenKind::Str("raw\n".into()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn tokenizes_angelscript_operators() {
    assert_eq!(
        kinds("**= ** >>>= >>> ^^ :: @ ++ !"),
        [
            Punct::StarStarAssign,
            Punct::StarStar,
            Punct::UShrAssign,
            Punct::UShr,
            Punct::CaretCaret,
            Punct::ColonColon,
            Punct::At,
            Punct::PlusPlus,
            Punct::Bang,
        ]
        .into_iter()
        .map(TokenKind::Punct)
        .chain([TokenKind::Eof])
        .collect::<Vec<_>>()
    );
}

#[test]
fn skips_comments() {
    assert_eq!(
        kinds("a // x\n/* y\nz */ b"),
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Ident("b".into()),
            TokenKind::Eof,
        ]
    );
}

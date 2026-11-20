//! Semantic highlighting for AngelScript: the shared lexical layer from
//! [`parser_rs::semantic`] (keywords, literals including heredocs,
//! operators, comments), plus identifier classification: namespaces,
//! classes, interfaces, enums and their values, funcdefs, functions,
//! methods, constructors, fields, virtual properties, parameters, locals,
//! globals, constants, and the contextual keywords (`this`, `super`,
//! `get`, `set`, `override`, `final`, `shared`, ...).
//!
//! Like every language here, it works on the tolerant token stream, not
//! the AST, so half-typed code still highlights. The key step is spotting
//! a *type* (`const ns::Foo<int>@ &in`) in declaration position. All its
//! names are marked as types, and the name right after it as a
//! declaration.
//!
//! The same pass records every name as a symbol [`Occurrence`]
//! ([`semantic_tokens_and_symbols`]), so the editor's symbol index agrees
//! with the highlighting:
//!
//! - **Top-level and namespace declarations** (functions, globals,
//!   classes, interfaces, enums and their values, funcdefs, typedefs,
//!   namespaces) are project-wide, in their namespace: each occurrence
//!   records the namespace it's declared in or used from and the
//!   qualifier written before it (`ns::` in `ns::Foo`, `::` for the
//!   global one), and the editor resolves it across the project (see
//!   [`Occurrence::resolve`]). An enum's name in a qualifier
//!   (`Color::Red`) isn't a namespace: enum values live in their enum's
//!   namespace, reachable with or without it.
//! - **Class members** (fields, methods, constructors, virtual
//!   properties) are [`SymScope::Member`], as is any name after `.`. A
//!   method can use its class's members without `this.`: a bare name in a
//!   class that names one of its members resolves to it once the whole
//!   file is read.
//! - **Parameters and locals** are scoped to their function (methods,
//!   lambdas and property accessors included), numbered per file.

use std::collections::{HashMap, HashSet};

use crate::lexer::SPEC;
use crate::token::{Kw, Punct, Token, TokenKind, PRIMITIVES};
use parser_rs::semantic::{self, comment_tokens, lexical_type};
use parser_rs::{
    Modifiers, Occurrence, Scope as SymScope, SemanticModifier as M, SemanticToken, SemanticType as T,
    SymbolKind as K,
};

/// Every semantic token in `src`, unencoded.
pub fn semantic_tokens(src: &str) -> Vec<SemanticToken> {
    semantic_tokens_and_symbols(src).0
}

/// Every name declared or referenced in `src`.
pub fn symbols(src: &str) -> Vec<Occurrence> {
    semantic_tokens_and_symbols(src).1
}

/// Semantic tokens and symbol occurrences in one pass: they come from the
/// same classification.
pub fn semantic_tokens_and_symbols(src: &str) -> (Vec<SemanticToken>, Vec<Occurrence>) {
    let lexed = parser_rs::tokenize_lossy(src, &SPEC);
    let mut out = comment_tokens(&lexed.comments, src);
    let mut syms = Vec::new();
    Classifier::new(&lexed.tokens).run(&mut out, &mut syms);
    (out, syms)
}

fn occurrence(t: &Token, name: &str, kind: K, declaration: bool, scope: SymScope) -> Occurrence {
    Occurrence {
        name: name.to_string(),
        line: t.line,
        col: t.col,
        end_col: t.end_col,
        kind,
        declaration,
        scope,
        namespace: String::new(),
        qualifier: None,
    }
}

/// [`semantic_tokens`] in the editor's wire format.
pub fn encoded_semantic_tokens(src: &str) -> Vec<u32> {
    semantic::encode(semantic_tokens(src), src)
}

fn kind(t: Option<&Token>) -> Option<&TokenKind> {
    t.map(|t| &t.kind)
}

fn is_p(t: Option<&Token>, p: Punct) -> bool {
    kind(t) == Some(&TokenKind::Punct(p))
}

fn is_kw(t: Option<&Token>, k: Kw) -> bool {
    kind(t) == Some(&TokenKind::Keyword(k))
}

fn ident(t: Option<&Token>) -> Option<&str> {
    match kind(t) {
        Some(TokenKind::Ident(s)) => Some(s),
        _ => None,
    }
}

fn is_assign(t: Option<&Token>) -> bool {
    matches!(
        kind(t),
        Some(TokenKind::Punct(
            Punct::Assign
                | Punct::PlusAssign
                | Punct::MinusAssign
                | Punct::StarAssign
                | Punct::SlashAssign
                | Punct::PercentAssign
                | Punct::StarStarAssign
                | Punct::AmpAssign
                | Punct::PipeAssign
                | Punct::CaretAssign
                | Punct::ShlAssign
                | Punct::ShrAssign
                | Punct::UShrAssign
        ))
    )
}

/// If a type starts at `i`, the index just past it, after any `&in`-style
/// reference. Templates must balance (`>>`/`>>>` close several).
fn type_end(toks: &[Token], mut i: usize) -> Option<usize> {
    let at = |i: usize| toks.get(i);
    if is_kw(at(i), Kw::Const) {
        i += 1;
    }
    if is_p(at(i), Punct::ColonColon) {
        i += 1;
    }
    match kind(at(i))? {
        TokenKind::Ident(_) | TokenKind::Punct(Punct::Question) => {}
        TokenKind::Keyword(k) if PRIMITIVES.contains(*k) => {}
        _ => return None,
    }
    i += 1;
    while is_p(at(i), Punct::ColonColon) && ident(at(i + 1)).is_some() {
        i += 2;
    }
    if is_p(at(i), Punct::Lt) {
        let mut depth = 1usize;
        let mut j = i + 1;
        while depth > 0 {
            match kind(at(j)) {
                Some(TokenKind::Punct(Punct::Lt)) => depth += 1,
                Some(TokenKind::Punct(Punct::Gt)) => depth -= 1,
                Some(TokenKind::Punct(Punct::Shr)) => depth = depth.saturating_sub(2),
                Some(TokenKind::Punct(Punct::UShr)) => depth = depth.saturating_sub(3),
                Some(
                    TokenKind::Ident(_)
                    | TokenKind::Keyword(_)
                    | TokenKind::Punct(
                        Punct::Comma | Punct::ColonColon | Punct::At | Punct::LBracket | Punct::RBracket,
                    ),
                ) => {}
                _ => return Some(i), // not a template after all: `a < b`
            }
            j += 1;
        }
        i = j;
    }
    loop {
        if is_p(at(i), Punct::LBracket) && is_p(at(i + 1), Punct::RBracket) {
            i += 2;
        } else if is_p(at(i), Punct::At) {
            i += 1;
            if is_kw(at(i), Kw::Const) {
                i += 1;
            }
        } else {
            break;
        }
    }
    if is_p(at(i), Punct::Amp) {
        i += 1;
        if matches!(kind(at(i)), Some(TokenKind::Keyword(Kw::In | Kw::Out | Kw::InOut))) {
            i += 1;
        }
    }
    Some(i)
}

/// File-wide names collected up front.
#[derive(Default)]
struct Symbols {
    classes: HashSet<String>,
    interfaces: HashSet<String>,
    enums: HashSet<String>,
    enum_values: HashSet<String>,
    /// funcdefs and typedefs
    types: HashSet<String>,
    namespaces: HashSet<String>,
    constants: HashSet<String>,
}

struct FnScope {
    /// Its local scope number.
    id: u32,
    params: HashSet<String>,
    locals: HashSet<String>,
}

impl FnScope {
    /// A new function scope, numbered from `counter`.
    fn new(counter: &mut u32) -> Self {
        *counter += 1;
        FnScope { id: *counter, params: HashSet::new(), locals: HashSet::new() }
    }
}

enum Scope {
    Namespace(String),
    Class(String),
    Interface,
    Enum,
    Property,
    Function(FnScope),
    Block,
}

/// Contextual words that are keywords where they're used as such.
const MODIFIER_WORDS: &[&str] = &["shared", "external", "abstract", "final"];
const TAIL_WORDS: &[&str] = &["final", "override", "property", "explicit"];

/// Whether `name` is used as a contextual keyword here: a modifier after
/// a parameter list (`) const override {`), one before a declaration
/// (`shared class`), or `from` in an import.
fn contextual_keyword(name: &str, prev: Option<&Token>, next: Option<&Token>) -> bool {
    let tail = TAIL_WORDS.contains(&name)
        && matches!(
            kind(prev),
            Some(TokenKind::Punct(Punct::RParen) | TokenKind::Keyword(Kw::Const) | TokenKind::Ident(_))
        )
        && matches!(
            kind(next),
            Some(TokenKind::Punct(Punct::LBrace | Punct::Semicolon) | TokenKind::Ident(_) | TokenKind::Keyword(Kw::Const))
        );
    let modifier = MODIFIER_WORDS.contains(&name) && matches!(kind(next), Some(TokenKind::Keyword(_) | TokenKind::Ident(_)));
    let from = name == "from" && is_p(prev, Punct::RParen) && matches!(kind(next), Some(TokenKind::Str(_)));
    tail || modifier || from
}

struct Classifier<'a> {
    toks: &'a [Token],
    sym: Symbols,
}

impl<'a> Classifier<'a> {
    fn new(toks: &'a [Token]) -> Self {
        let mut sym = Symbols::default();
        let mut in_enum = false;
        for (i, t) in toks.iter().enumerate() {
            let name = |n: usize| ident(toks.get(i + n)).map(String::from);
            match &t.kind {
                TokenKind::Keyword(Kw::Class) => sym.classes.extend(name(1)),
                TokenKind::Keyword(Kw::Interface) => sym.interfaces.extend(name(1)),
                TokenKind::Keyword(Kw::Namespace) => sym.namespaces.extend(name(1)),
                TokenKind::Keyword(Kw::Enum) => {
                    sym.enums.extend(name(1));
                    in_enum = true;
                }
                TokenKind::Punct(Punct::RBrace) => in_enum = false,
                TokenKind::Ident(n)
                    if in_enum
                        && matches!(
                            kind(i.checked_sub(1).and_then(|j| toks.get(j))),
                            Some(TokenKind::Punct(Punct::LBrace | Punct::Comma))
                        ) =>
                {
                    sym.enum_values.insert(n.clone());
                }
                TokenKind::Keyword(Kw::Funcdef | Kw::Typedef) => {
                    // the name is the identifier right before `(` or `;`
                    let mut j = i + 1;
                    while let Some(t) = toks.get(j) {
                        if matches!(t.kind, TokenKind::Punct(Punct::LParen | Punct::Semicolon)) {
                            sym.types.extend(ident(toks.get(j - 1)).map(String::from));
                            break;
                        }
                        j += 1;
                    }
                }
                TokenKind::Keyword(Kw::Const) => {
                    if let Some(end) = type_end(toks, i) {
                        sym.constants.extend(ident(toks.get(end)).map(String::from));
                    }
                }
                _ => {}
            }
        }
        Classifier { toks, sym }
    }

    /// The namespace path written before the name at `i`: `A::B` in
    /// `A::B::name`, `::A` in `::A::name`, `::` in `::name`, `None` for a
    /// plain name. An enum's name in it is dropped: `Color::Red` is the
    /// value `Red`, in the namespace the enum is in.
    fn qualifier(&self, toks: &[Token], i: usize) -> Option<String> {
        let mut parts = Vec::new();
        let mut j = i;
        while j >= 2 && is_p(toks.get(j - 1), Punct::ColonColon) {
            let Some(part) = ident(toks.get(j - 2)) else { break };
            parts.push(part);
            j -= 2;
        }
        let absolute = j >= 1 && is_p(toks.get(j - 1), Punct::ColonColon);
        parts.reverse();
        if parts.last().is_some_and(|p| self.sym.enums.contains(*p)) {
            parts.pop();
        }
        match (absolute, parts.is_empty()) {
            (false, true) => None,
            (true, _) => Some(format!("::{}", parts.join("::"))),
            (false, false) => Some(parts.join("::")),
        }
    }

    /// The symbol kind of a name used as a type.
    fn type_symbol(&self, name: &str) -> K {
        if self.sym.classes.contains(name) {
            K::Class
        } else if self.sym.interfaces.contains(name) {
            K::Interface
        } else if self.sym.enums.contains(name) {
            K::Enum
        } else {
            K::Type
        }
    }

    /// The highlight for a name used as a type.
    fn type_kind(&self, name: &str) -> (T, Modifiers) {
        if self.sym.classes.contains(name) {
            (T::Class, Modifiers::NONE)
        } else if self.sym.interfaces.contains(name) {
            (T::Interface, Modifiers::NONE)
        } else if self.sym.enums.contains(name) {
            (T::Enum, Modifiers::NONE)
        } else {
            (T::Type, Modifiers::NONE)
        }
    }

    fn is_known_type(&self, name: &str) -> bool {
        self.sym.classes.contains(name)
            || self.sym.interfaces.contains(name)
            || self.sym.enums.contains(name)
            || self.sym.types.contains(name)
    }

    fn run(&self, out: &mut Vec<SemanticToken>, syms: &mut Vec<Occurrence>) {
        let toks = self.toks;
        let decl = Modifiers::NONE.with(M::Declaration);
        let mut scopes = 0u32;
        // Each class's members, and the project-scope references made in
        // each class: once the file is read, those naming a member of
        // their class are re-scoped to it (implicit `this`).
        let mut members: HashMap<String, HashSet<String>> = HashMap::new();
        let mut class_refs: Vec<(usize, String)> = Vec::new();

        let mut stack: Vec<Scope> = Vec::new();
        let mut pending: Option<Scope> = None;
        let mut pending_lambda = false;
        // parameter list being read: its paren depth
        let mut params_depth: Option<i32> = None;
        let mut depth = 0i32;
        // statement-level declaration list (`int a, b;`): its depth
        let mut decl_depth: Option<i32> = None;
        let mut const_stmt = false;
        // inside a `funcdef`/`typedef` declaration: its name is a type
        let mut type_decl_stmt = false;
        // indices of names inside a type, and of declared names
        let mut type_tokens: HashSet<usize> = HashSet::new();
        let mut decl_names: HashSet<usize> = HashSet::new();

        for (i, t) in toks.iter().enumerate() {
            let prev = i.checked_sub(1).and_then(|j| toks.get(j));
            let next = toks.get(i + 1);
            // Everything this token records, to place in its namespace.
            let recorded = syms.len();
            let namespace = stack
                .iter()
                .filter_map(|s| match s {
                    Scope::Namespace(n) => Some(n.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("::");

            // A type in declaration position marks its names and the name
            // being declared after it.
            let type_start = !type_tokens.contains(&i)
                && (matches!(
                    &t.kind,
                    TokenKind::Ident(_) | TokenKind::Keyword(Kw::Const) | TokenKind::Punct(Punct::ColonColon)
                ) || matches!(&t.kind, TokenKind::Keyword(k) if PRIMITIVES.contains(*k)));
            let decl_position = match kind(prev) {
                None => true,
                Some(TokenKind::Punct(
                    Punct::Semicolon | Punct::LBrace | Punct::RBrace | Punct::LParen | Punct::Comma,
                )) => true,
                Some(TokenKind::Keyword(
                    Kw::Private | Kw::Protected | Kw::Funcdef | Kw::Typedef | Kw::Import | Kw::Else | Kw::Do,
                )) => true,
                Some(TokenKind::Ident(w)) => MODIFIER_WORDS.contains(&w.as_str()),
                _ => false,
            } || (is_p(prev, Punct::Lt) && is_kw(i.checked_sub(2).and_then(|j| toks.get(j)), Kw::Cast));
            if type_start
                && decl_position
                && !decl_names.contains(&i)
                && let Some(end) = type_end(toks, i)
            {
                let declares = ident(toks.get(end)).is_some();
                let in_cast = is_p(prev, Punct::Lt);
                let constructs = is_p(toks.get(end), Punct::LParen)
                    && toks[i..end].iter().any(|t| t.kind == TokenKind::Punct(Punct::Lt));
                if declares || in_cast || constructs {
                    for j in i..end {
                        if ident(toks.get(j)).is_some() {
                            type_tokens.insert(j);
                        }
                    }
                    if declares {
                        decl_names.insert(end);
                    }
                }
            }

            let name = match &t.kind {
                TokenKind::Ident(name) => name.as_str(),
                k => {
                    match k {
                        TokenKind::Keyword(Kw::Namespace) => {
                            pending = Some(Scope::Namespace(ident(next).unwrap_or_default().to_string()));
                        }
                        TokenKind::Keyword(Kw::Class) => {
                            pending = Some(Scope::Class(ident(next).unwrap_or_default().to_string()));
                        }
                        TokenKind::Keyword(Kw::Interface) => pending = Some(Scope::Interface),
                        TokenKind::Keyword(Kw::Enum) => pending = Some(Scope::Enum),
                        TokenKind::Keyword(Kw::Const) if params_depth.is_none() => const_stmt = true,
                        TokenKind::Keyword(Kw::Funcdef | Kw::Typedef) => type_decl_stmt = true,
                        TokenKind::Punct(Punct::LParen) => {
                            if matches!(pending, Some(Scope::Function(_))) && params_depth.is_none() {
                                params_depth = Some(depth);
                            }
                            depth += 1;
                        }
                        TokenKind::Punct(Punct::RParen) => {
                            depth -= 1;
                            if params_depth == Some(depth) {
                                params_depth = None;
                                pending_lambda = false;
                            }
                        }
                        TokenKind::Punct(Punct::LBrace) => {
                            stack.push(pending.take().unwrap_or(Scope::Block));
                            decl_depth = None;
                            const_stmt = false;
                        }
                        TokenKind::Punct(Punct::RBrace) => {
                            stack.pop();
                            decl_depth = None;
                            const_stmt = false;
                        }
                        TokenKind::Punct(Punct::Semicolon) => {
                            pending = None;
                            decl_depth = None;
                            const_stmt = false;
                            type_decl_stmt = false;
                        }
                        _ => {}
                    }
                    if let Some(ty) = lexical_type(k) {
                        out.push(SemanticToken::at(t, ty, Modifiers::NONE));
                    }
                    continue;
                }
            };

            let in_class = stack.iter().rev().find_map(|s| match s {
                Scope::Class(c) => Some(c.as_str()),
                Scope::Interface => Some(""),
                Scope::Function(_) | Scope::Namespace(_) => None,
                _ => None,
            });
            let in_function = stack.iter().any(|s| matches!(s, Scope::Function(_)));
            let top_is_property = matches!(stack.last(), Some(Scope::Property));
            let top_is_enum = matches!(stack.last(), Some(Scope::Enum));
            let modified = Modifiers::NONE.with_if(is_assign(next), M::Modification);
            let fn_names = stack.iter().rev().find_map(|s| match s {
                Scope::Function(f) => Some(f),
                _ => None,
            });
            let is_param = fn_names.is_some_and(|f| f.params.contains(name));
            let is_local = fn_names.is_some_and(|f| f.locals.contains(name));
            let fn_id = fn_names.map_or(0, |f| f.id);
            let class = in_class.filter(|c| !c.is_empty()).map(String::from);
            let p = SymScope::Project;
            // A project-scope reference; in a class, it may turn out to be
            // a member.
            let mut global_ref = |syms: &mut Vec<Occurrence>, kind: K| {
                if let Some(c) = &class {
                    class_refs.push((syms.len(), c.clone()));
                }
                syms.push(occurrence(t, name, kind, false, p));
            };
            // A member declaration in the current class.
            let mut member_decl = |syms: &mut Vec<Occurrence>, kind: K| {
                if let Some(c) = &class {
                    members.entry(c.clone()).or_default().insert(name.to_string());
                }
                syms.push(occurrence(t, name, kind, true, SymScope::Member));
            };

            let token = if name == "this" || name == "super" {
                SemanticToken::at(t, T::Keyword, Modifiers::NONE)
            } else if name == "function" && is_p(next, Punct::LParen) {
                pending = Some(Scope::Function(FnScope::new(&mut scopes)));
                pending_lambda = true;
                SemanticToken::at(t, T::Keyword, Modifiers::NONE)
            } else if top_is_property && (name == "get" || name == "set") {
                let mut f = FnScope::new(&mut scopes);
                if name == "set" {
                    f.params.insert("value".to_string());
                }
                pending = Some(Scope::Function(f));
                SemanticToken::at(t, T::Keyword, Modifiers::NONE)
            } else if contextual_keyword(name, prev, next) && !decl_names.contains(&i) && !type_tokens.contains(&i) {
                SemanticToken::at(t, T::Keyword, Modifiers::NONE)
            } else if is_kw(prev, Kw::Namespace) {
                syms.push(occurrence(t, name, K::Namespace, true, p));
                SemanticToken::at(t, T::Namespace, decl)
            } else if is_kw(prev, Kw::Class) {
                syms.push(occurrence(t, name, K::Class, true, p));
                SemanticToken::at(t, T::Class, decl)
            } else if is_kw(prev, Kw::Interface) {
                syms.push(occurrence(t, name, K::Interface, true, p));
                SemanticToken::at(t, T::Interface, decl)
            } else if is_kw(prev, Kw::Enum) {
                syms.push(occurrence(t, name, K::Enum, true, p));
                SemanticToken::at(t, T::Enum, decl)
            } else if top_is_enum && matches!(kind(prev), Some(TokenKind::Punct(Punct::LBrace | Punct::Comma))) {
                syms.push(occurrence(t, name, K::EnumMember, true, p));
                SemanticToken::at(t, T::EnumMember, decl)
            } else if params_depth.is_some() && pending_lambda && matches!(kind(next), Some(TokenKind::Punct(Punct::Comma | Punct::RParen))) && !decl_names.contains(&i) {
                // `function(a, b)`: untyped lambda parameters
                if let Some(Scope::Function(f)) = pending.as_mut() {
                    f.params.insert(name.to_string());
                    syms.push(occurrence(t, name, K::Parameter, true, SymScope::Local(f.id)));
                }
                SemanticToken::at(t, T::Parameter, decl)
            } else if type_tokens.contains(&i) {
                if is_p(next, Punct::ColonColon) && !self.is_known_type(name) {
                    syms.push(occurrence(t, name, K::Namespace, false, p));
                    SemanticToken::at(t, T::Namespace, Modifiers::NONE)
                } else {
                    syms.push(occurrence(t, name, self.type_symbol(name), false, p));
                    let (ty, m) = self.type_kind(name);
                    SemanticToken::at(t, ty, m)
                }
            } else if in_class.is_some_and(|c| c == name) && is_p(next, Punct::LParen) && !in_function {
                // constructor / destructor
                pending = Some(Scope::Function(FnScope::new(&mut scopes)));
                member_decl(syms, K::Method);
                SemanticToken::at(t, T::Method, decl)
            } else if decl_names.contains(&i)
                || (is_p(prev, Punct::Comma) && decl_depth == Some(depth) && params_depth.is_none())
            {
                if params_depth.is_some() {
                    if let Some(Scope::Function(f)) = pending.as_mut() {
                        f.params.insert(name.to_string());
                        syms.push(occurrence(t, name, K::Parameter, true, SymScope::Local(f.id)));
                    }
                    SemanticToken::at(t, T::Parameter, decl)
                } else if type_decl_stmt {
                    // `funcdef void Cb(int)` / `typedef float real`; a
                    // funcdef's parameter list still reads as parameters
                    if is_p(next, Punct::LParen) {
                        pending = Some(Scope::Function(FnScope::new(&mut scopes)));
                    }
                    syms.push(occurrence(t, name, K::Type, true, p));
                    SemanticToken::at(t, T::Type, decl)
                } else if is_p(next, Punct::LParen) {
                    pending = Some(Scope::Function(FnScope::new(&mut scopes)));
                    let method = in_class.is_some() && !in_function;
                    if method {
                        member_decl(syms, K::Method);
                    } else {
                        syms.push(occurrence(t, name, K::Function, true, p));
                    }
                    SemanticToken::at(t, if method { T::Method } else { T::Function }, decl)
                } else if is_p(next, Punct::LBrace) && !in_function {
                    // a virtual property: `int x { get; set; }`
                    pending = Some(Scope::Property);
                    if in_class.is_some() {
                        member_decl(syms, K::Field);
                    } else {
                        syms.push(occurrence(t, name, K::Global, true, p));
                    }
                    SemanticToken::at(t, T::Property, decl)
                } else {
                    decl_depth = Some(depth);
                    let mods = decl.with_if(const_stmt, M::Readonly);
                    if in_function {
                        if let Some(Scope::Function(f)) =
                            stack.iter_mut().rev().find(|s| matches!(s, Scope::Function(_)))
                        {
                            f.locals.insert(name.to_string());
                            syms.push(occurrence(t, name, K::Local, true, SymScope::Local(f.id)));
                        }
                        SemanticToken::at(t, T::Variable, mods)
                    } else if in_class.is_some() {
                        member_decl(syms, K::Field);
                        SemanticToken::at(t, T::Property, mods)
                    } else {
                        let kind = if const_stmt { K::Constant } else { K::Global };
                        syms.push(occurrence(t, name, kind, true, p));
                        SemanticToken::at(t, T::Variable, mods)
                    }
                }
            } else if is_p(prev, Punct::Dot) {
                let method = is_p(next, Punct::LParen);
                syms.push(occurrence(t, name, if method { K::Method } else { K::Field }, false, SymScope::Member));
                SemanticToken::at(t, if method { T::Method } else { T::Property }, modified)
            } else if is_p(next, Punct::ColonColon) {
                if self.is_known_type(name) {
                    syms.push(occurrence(t, name, self.type_symbol(name), false, p));
                    let (ty, m) = self.type_kind(name);
                    SemanticToken::at(t, ty, m)
                } else {
                    syms.push(occurrence(t, name, K::Namespace, false, p));
                    SemanticToken::at(t, T::Namespace, Modifiers::NONE)
                }
            } else if self.sym.enum_values.contains(name) && (is_p(prev, Punct::ColonColon) || !is_param && !is_local) {
                syms.push(occurrence(t, name, K::EnumMember, false, p));
                SemanticToken::at(t, T::EnumMember, Modifiers::NONE)
            } else if is_param {
                syms.push(occurrence(t, name, K::Parameter, false, SymScope::Local(fn_id)));
                SemanticToken::at(t, T::Parameter, modified)
            } else if is_local {
                syms.push(occurrence(t, name, K::Local, false, SymScope::Local(fn_id)));
                let ty = if is_p(next, Punct::LParen) { T::Function } else { T::Variable };
                SemanticToken::at(t, ty, modified)
            } else if is_p(next, Punct::LParen) {
                if self.sym.classes.contains(name) {
                    syms.push(occurrence(t, name, K::Class, false, p));
                    SemanticToken::at(t, T::Class, Modifiers::NONE)
                } else {
                    global_ref(syms, K::Function);
                    SemanticToken::at(t, T::Function, Modifiers::NONE)
                }
            } else if self.is_known_type(name) {
                syms.push(occurrence(t, name, self.type_symbol(name), false, p));
                let (ty, m) = self.type_kind(name);
                SemanticToken::at(t, ty, m)
            } else if self.sym.namespaces.contains(name) {
                syms.push(occurrence(t, name, K::Namespace, false, p));
                SemanticToken::at(t, T::Namespace, Modifiers::NONE)
            } else {
                let readonly = self.sym.constants.contains(name);
                global_ref(syms, if readonly { K::Constant } else { K::Global });
                SemanticToken::at(t, T::Variable, modified.with_if(readonly, M::Readonly))
            };
            out.push(token);
            for o in syms[recorded..].iter_mut().filter(|o| o.scope == p) {
                o.namespace = namespace.clone();
                if !o.declaration {
                    o.qualifier = self.qualifier(toks, i);
                }
            }
        }

        // Implicit `this`: a bare name in a class that names one of its
        // members is that member.
        for (index, class) in class_refs {
            if members.get(&class).is_some_and(|m| m.contains(&syms[index].name)) {
                let o = &mut syms[index];
                o.scope = SymScope::Member;
                o.namespace.clear();
                o.qualifier = None;
                o.kind = if o.kind == K::Function { K::Method } else { K::Field };
            }
        }
    }
}

#[cfg(test)]
#[path = "semantic.test.rs"]
mod tests;

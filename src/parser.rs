//! A recursive-descent parser for AngelScript.
//!
//! - **Declaration or expression?** `Type name ...` vs. an expression
//!   statement is decided speculatively. The parser tries to read a type
//!   followed by a name, then rewinds ([`TokenStream::mark`] /
//!   [`TokenStream::reset`]) if that fails.
//! - **Templates.** `>>` and `>>>` close two and three template argument
//!   lists (`array<array<int>>`).
//! - **Contextual words.** `shared`, `final`, `override`, `get`, `this`,
//!   ... are identifiers that the parser recognizes in place.

use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Kw, Punct, Token, TokenKind, PRIMITIVES};
use parser_rs::{TokenParser, TokenStream};

pub use parser_rs::ParseError;

type PResult<T> = Result<T, ParseError>;

pub struct Parser {
    ts: TokenStream<TokenKind>,
    /// `>` still owed from a `>>`/`>>>` that closed fewer template lists
    /// than it spells.
    pending_gt: usize,
}

impl TokenParser for Parser {
    type Kind = TokenKind;

    fn tokens(&mut self) -> &mut TokenStream<TokenKind> {
        &mut self.ts
    }
}

fn kw(k: Kw) -> TokenKind {
    TokenKind::Keyword(k)
}

fn p(p: Punct) -> TokenKind {
    TokenKind::Punct(p)
}

/// The contextual modifier words allowed before a declaration.
const DECL_MODIFIERS: &[&str] = &["shared", "external", "abstract", "final"];

/// Binary operators: `(op, precedence)`, higher binds tighter. Follows
/// the AngelScript manual's table, where (unlike C) the bitwise operators
/// bind tighter than comparisons, and `xor`/`^^` sit with equality.
fn binop(k: &TokenKind) -> Option<(BinOp, u8)> {
    Some(match k {
        TokenKind::Keyword(Kw::Or) | TokenKind::Punct(Punct::OrOr) => (BinOp::Or, 1),
        TokenKind::Keyword(Kw::And) | TokenKind::Punct(Punct::AndAnd) => (BinOp::And, 2),
        TokenKind::Keyword(Kw::Xor) | TokenKind::Punct(Punct::CaretCaret) => (BinOp::Xor, 3),
        TokenKind::Punct(Punct::EqEq) => (BinOp::Eq, 3),
        TokenKind::Punct(Punct::NotEq) => (BinOp::NotEq, 3),
        TokenKind::Keyword(Kw::Is) => (BinOp::Is, 3),
        TokenKind::Punct(Punct::Lt) => (BinOp::Lt, 4),
        TokenKind::Punct(Punct::Gt) => (BinOp::Gt, 4),
        TokenKind::Punct(Punct::LtEq) => (BinOp::Le, 4),
        TokenKind::Punct(Punct::GtEq) => (BinOp::Ge, 4),
        TokenKind::Punct(Punct::Pipe) => (BinOp::BitOr, 5),
        TokenKind::Punct(Punct::Caret) => (BinOp::BitXor, 6),
        TokenKind::Punct(Punct::Amp) => (BinOp::BitAnd, 7),
        TokenKind::Punct(Punct::Shl) => (BinOp::Shl, 8),
        TokenKind::Punct(Punct::Shr) => (BinOp::Shr, 8),
        TokenKind::Punct(Punct::UShr) => (BinOp::UShr, 8),
        TokenKind::Punct(Punct::Plus) => (BinOp::Add, 9),
        TokenKind::Punct(Punct::Minus) => (BinOp::Sub, 9),
        TokenKind::Punct(Punct::Star) => (BinOp::Mul, 10),
        TokenKind::Punct(Punct::Slash) => (BinOp::Div, 10),
        TokenKind::Punct(Punct::Percent) => (BinOp::Mod, 10),
        TokenKind::Punct(Punct::StarStar) => (BinOp::Pow, 11),
        _ => return None,
    })
}

fn assign_op(k: &TokenKind) -> Option<AssignOp> {
    Some(match k {
        TokenKind::Punct(p) => match p {
            Punct::Assign => AssignOp::Assign,
            Punct::PlusAssign => AssignOp::Add,
            Punct::MinusAssign => AssignOp::Sub,
            Punct::StarAssign => AssignOp::Mul,
            Punct::SlashAssign => AssignOp::Div,
            Punct::PercentAssign => AssignOp::Mod,
            Punct::StarStarAssign => AssignOp::Pow,
            Punct::AmpAssign => AssignOp::BitAnd,
            Punct::PipeAssign => AssignOp::BitOr,
            Punct::CaretAssign => AssignOp::BitXor,
            Punct::ShlAssign => AssignOp::Shl,
            Punct::ShrAssign => AssignOp::Shr,
            Punct::UShrAssign => AssignOp::UShr,
            _ => return None,
        },
        _ => return None,
    })
}

/// Where declarations are being read.
#[derive(Clone, Copy)]
enum Ctx<'a> {
    Global,
    /// A class body, with the class name (to spot constructors).
    Class(&'a str),
}

/// What a declaration starting with a type turned out to be.
enum Decl {
    Function(FunctionDecl),
    Vars(Vec<VarDecl>),
    Property(PropertyDecl),
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            ts: TokenStream::new(tokens),
            pending_gt: 0,
        }
    }

    pub fn parse_str(src: &str) -> PResult<Script> {
        Parser::new(Lexer::tokenize(src)?).script()
    }

    // ----- token helpers --------------------------------------------------

    fn check(&self, k: &TokenKind) -> bool {
        self.ts.check(k)
    }

    fn eat(&mut self, k: &TokenKind) -> bool {
        self.ts.eat(k)
    }

    fn expect(&mut self, k: TokenKind) -> PResult<Token> {
        self.ts.expect(k)
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        self.ts.error(msg)
    }

    fn unexpected(&self, what: &str) -> ParseError {
        let found = self.ts.peek_kind().clone();
        self.error(format!("expected {what}, found {found}"))
    }

    fn line(&self) -> usize {
        self.ts.peek().line
    }

    /// Whether the next token is the identifier `word`.
    fn is_word(&self, word: &str) -> bool {
        matches!(self.ts.peek_kind(), TokenKind::Ident(w) if w == word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let yes = self.is_word(word);
        if yes {
            self.ts.advance();
        }
        yes
    }

    fn name(&mut self) -> PResult<Name> {
        match self.ts.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let t = self.ts.advance();
                Ok(Name { name, span: t.span() })
            }
            _ => Err(self.unexpected("a name")),
        }
    }

    /// `>` closing a template argument list, splitting `>>`/`>>>`.
    fn expect_gt(&mut self) -> PResult<()> {
        if self.pending_gt > 0 {
            self.pending_gt -= 1;
            return Ok(());
        }
        match self.ts.peek_kind() {
            TokenKind::Punct(Punct::Gt) => {}
            TokenKind::Punct(Punct::Shr) => self.pending_gt = 1,
            TokenKind::Punct(Punct::UShr) => self.pending_gt = 2,
            _ => return Err(self.unexpected("'>'")),
        }
        self.ts.advance();
        Ok(())
    }

    /// Runs `f` speculatively: on failure, rewinds and returns `None`.
    fn attempt<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> Option<T> {
        let (mark, gt) = (self.ts.mark(), self.pending_gt);
        match f(self) {
            Ok(v) => Some(v),
            Err(_) => {
                self.ts.reset(mark);
                self.pending_gt = gt;
                None
            }
        }
    }

    // ----- types ------------------------------------------------------------

    /// `const ns::Name<T>[]@ const`
    fn type_ref(&mut self) -> PResult<TypeRef> {
        let start = self.ts.peek().clone();
        let is_const = self.eat(&kw(Kw::Const));
        let mut scope = Vec::new();
        if self.eat(&p(Punct::ColonColon)) {
            scope.push(String::new());
        }
        let mut name = match self.ts.peek_kind().clone() {
            TokenKind::Keyword(k) if PRIMITIVES.contains(k) => {
                self.ts.advance();
                k.as_str().to_string()
            }
            TokenKind::Ident(n) => {
                self.ts.advance();
                n
            }
            TokenKind::Punct(Punct::Question) => {
                self.ts.advance();
                "?".to_string()
            }
            _ => return Err(self.unexpected("a type")),
        };
        while self.check(&p(Punct::ColonColon)) && matches!(self.ts.nth_kind(1), TokenKind::Ident(_)) {
            self.ts.advance();
            scope.push(std::mem::replace(&mut name, self.name()?.name));
        }
        let mut template = Vec::new();
        if self.check(&p(Punct::Lt)) {
            template = self
                .attempt(|this| {
                    this.ts.advance();
                    let mut args = vec![this.type_ref()?];
                    while this.eat(&p(Punct::Comma)) {
                        args.push(this.type_ref()?);
                    }
                    this.expect_gt()?;
                    Ok(args)
                })
                .unwrap_or_default();
        }
        let mut suffixes = Vec::new();
        // a `>>` that closed an inner template too: the rest belongs to
        // the outer type, so no suffixes here
        while self.pending_gt == 0 {
            if self.check(&p(Punct::LBracket)) && self.ts.nth_kind(1) == &p(Punct::RBracket) {
                self.ts.advance();
                self.ts.advance();
                suffixes.push(TypeSuffix::Array);
            } else if self.eat(&p(Punct::At)) {
                if self.eat(&kw(Kw::Const)) {
                    suffixes.push(TypeSuffix::ConstHandle);
                } else {
                    suffixes.push(TypeSuffix::Handle);
                }
            } else {
                break;
            }
        }
        Ok(TypeRef {
            is_const,
            scope,
            name,
            template,
            suffixes,
            span: start.span().join(self.ts.prev().span()),
        })
    }

    /// Whether a declaration (`Type name`) starts here, without consuming.
    fn at_decl(&mut self) -> bool {
        let (mark, gt) = (self.ts.mark(), self.pending_gt);
        let yes = self.type_ref().is_ok()
            && self.pending_gt == 0
            && matches!(self.ts.peek_kind(), TokenKind::Ident(_));
        self.ts.reset(mark);
        self.pending_gt = gt;
        yes
    }

    /// `&`, `&in`, `&out`, `&inout`
    fn by_ref(&mut self) -> Option<RefKind> {
        if !self.eat(&p(Punct::Amp)) {
            return None;
        }
        Some(if self.eat(&kw(Kw::In)) {
            RefKind::In
        } else if self.eat(&kw(Kw::Out)) {
            RefKind::Out
        } else if self.eat(&kw(Kw::InOut)) {
            RefKind::InOut
        } else {
            RefKind::Ref
        })
    }

    // ----- declarations -----------------------------------------------------

    pub fn script(&mut self) -> PResult<Script> {
        let items = self.items()?;
        if !self.ts.at_eof() {
            return Err(self.unexpected("a declaration"));
        }
        Ok(Script { items })
    }

    /// Items up to `}` or end of file.
    fn items(&mut self) -> PResult<Vec<Item>> {
        let mut items = Vec::new();
        while !self.ts.at_eof() && !self.check(&p(Punct::RBrace)) {
            if self.eat(&p(Punct::Semicolon)) {
                continue;
            }
            items.extend(self.item()?);
        }
        Ok(items)
    }

    fn modifiers(&mut self) -> Modifiers {
        let mut m = Modifiers::default();
        loop {
            match self.ts.peek_kind().clone() {
                TokenKind::Keyword(Kw::Private) => m.private = true,
                TokenKind::Keyword(Kw::Protected) => m.protected = true,
                TokenKind::Ident(w)
                    if DECL_MODIFIERS.contains(&w.as_str())
                        // still a modifier only if a declaration follows
                        && matches!(
                            self.ts.nth_kind(1),
                            TokenKind::Ident(_) | TokenKind::Keyword(_) | TokenKind::Punct(Punct::ColonColon)
                        ) =>
                {
                    match w.as_str() {
                        "shared" => m.shared = true,
                        "external" => m.external = true,
                        "abstract" => m.is_abstract = true,
                        _ => m.is_final = true,
                    }
                }
                _ => return m,
            }
            self.ts.advance();
        }
    }

    fn item(&mut self) -> PResult<Vec<Item>> {
        let line = self.line();
        if self.eat(&kw(Kw::Namespace)) {
            let name = self.name()?;
            self.expect(p(Punct::LBrace))?;
            let items = self.items()?;
            self.expect(p(Punct::RBrace))?;
            return Ok(vec![Item::Namespace { name, items, line }]);
        }
        let modifiers = self.modifiers();
        Ok(vec![match self.ts.peek_kind().clone() {
            TokenKind::Keyword(Kw::Class | Kw::Mixin) => Item::Class(self.class(modifiers, line)?),
            TokenKind::Keyword(Kw::Interface) => Item::Interface(self.interface(modifiers, line)?),
            TokenKind::Keyword(Kw::Enum) => Item::Enum(self.enum_decl(modifiers, line)?),
            TokenKind::Keyword(Kw::Funcdef) => Item::Funcdef(self.funcdef(modifiers, line)?),
            TokenKind::Keyword(Kw::Typedef) => {
                self.ts.advance();
                let base = self.type_ref()?;
                let name = self.name()?;
                self.expect(p(Punct::Semicolon))?;
                Item::Typedef { base, name, line }
            }
            TokenKind::Keyword(Kw::Import) => {
                self.ts.advance();
                let ret = self.type_ref()?;
                let ret_ref = self.by_ref().is_some();
                let name = self.name()?;
                let params = self.params()?;
                let mut func = FunctionDecl {
                    modifiers,
                    ret: Some(ret),
                    ret_ref,
                    name,
                    destructor: false,
                    params,
                    body: None,
                    line,
                };
                self.func_tail_modifiers(&mut func.modifiers);
                if !self.eat_word("from") {
                    return Err(self.unexpected("'from'"));
                }
                let from = match self.ts.peek_kind().clone() {
                    TokenKind::Str(s) => {
                        self.ts.advance();
                        s
                    }
                    _ => return Err(self.unexpected("a module name string")),
                };
                self.expect(p(Punct::Semicolon))?;
                Item::Import { func, from, line }
            }
            _ => {
                return Ok(match self.decl(modifiers, Ctx::Global, line)? {
                    Decl::Function(f) => vec![Item::Function(f)],
                    Decl::Property(prop) => vec![Item::Property(prop)],
                    Decl::Vars(vars) => vars.into_iter().map(Item::Var).collect(),
                });
            }
        }])
    }

    fn bases(&mut self) -> PResult<Vec<Name>> {
        let mut bases = Vec::new();
        if self.eat(&p(Punct::Colon)) {
            loop {
                bases.push(self.name()?);
                if !self.eat(&p(Punct::Comma)) {
                    break;
                }
            }
        }
        Ok(bases)
    }

    fn class(&mut self, modifiers: Modifiers, line: usize) -> PResult<ClassDecl> {
        let mixin = self.eat(&kw(Kw::Mixin));
        self.expect(kw(Kw::Class))?;
        let name = self.name()?;
        let bases = self.bases()?;
        if self.eat(&p(Punct::Semicolon)) {
            return Ok(ClassDecl {
                modifiers,
                mixin,
                name,
                bases,
                members: None,
                line,
            });
        }
        self.expect(p(Punct::LBrace))?;
        let mut members = Vec::new();
        while !self.check(&p(Punct::RBrace)) {
            if self.ts.at_eof() {
                return Err(self.unexpected("'}'"));
            }
            if self.eat(&p(Punct::Semicolon)) {
                continue;
            }
            let mline = self.line();
            let mods = self.modifiers();
            if self.check(&kw(Kw::Funcdef)) {
                members.push(Member::Funcdef(self.funcdef(mods, mline)?));
                continue;
            }
            match self.decl(mods, Ctx::Class(&name.name), mline)? {
                Decl::Function(f) => members.push(Member::Method(f)),
                Decl::Property(prop) => members.push(Member::Property(prop)),
                Decl::Vars(vars) => members.extend(vars.into_iter().map(Member::Var)),
            }
        }
        self.expect(p(Punct::RBrace))?;
        Ok(ClassDecl {
            modifiers,
            mixin,
            name,
            bases,
            members: Some(members),
            line,
        })
    }

    fn interface(&mut self, modifiers: Modifiers, line: usize) -> PResult<InterfaceDecl> {
        self.expect(kw(Kw::Interface))?;
        let name = self.name()?;
        let bases = self.bases()?;
        let mut methods = Vec::new();
        if !self.eat(&p(Punct::Semicolon)) {
            self.expect(p(Punct::LBrace))?;
            while !self.check(&p(Punct::RBrace)) {
                if self.ts.at_eof() {
                    return Err(self.unexpected("'}'"));
                }
                let mline = self.line();
                match self.decl(Modifiers::default(), Ctx::Global, mline)? {
                    Decl::Function(f) => methods.push(f),
                    _ => return Err(ParseError::at("interfaces may only declare methods", mline, 1)),
                }
            }
            self.expect(p(Punct::RBrace))?;
        }
        Ok(InterfaceDecl {
            modifiers,
            name,
            bases,
            methods,
            line,
        })
    }

    fn enum_decl(&mut self, modifiers: Modifiers, line: usize) -> PResult<EnumDecl> {
        self.expect(kw(Kw::Enum))?;
        let name = self.name()?;
        let mut values = Vec::new();
        if !self.eat(&p(Punct::Semicolon)) {
            self.expect(p(Punct::LBrace))?;
            while !self.check(&p(Punct::RBrace)) {
                let value = self.name()?;
                let init = if self.eat(&p(Punct::Assign)) {
                    Some(self.expr()?)
                } else {
                    None
                };
                values.push((value, init));
                if !self.eat(&p(Punct::Comma)) {
                    break;
                }
            }
            self.expect(p(Punct::RBrace))?;
        }
        Ok(EnumDecl {
            modifiers,
            name,
            values,
            line,
        })
    }

    fn funcdef(&mut self, modifiers: Modifiers, line: usize) -> PResult<FuncdefDecl> {
        self.expect(kw(Kw::Funcdef))?;
        let ret = self.type_ref()?;
        self.by_ref();
        let name = self.name()?;
        let params = self.params()?;
        self.expect(p(Punct::Semicolon))?;
        Ok(FuncdefDecl {
            modifiers,
            ret,
            name,
            params,
            line,
        })
    }

    /// `(void)`, `()`, or `(Type &in name = default, ...)`.
    fn params(&mut self) -> PResult<Vec<Param>> {
        self.expect(p(Punct::LParen))?;
        let mut params = Vec::new();
        if self.check(&kw(Kw::Void)) && self.ts.nth_kind(1) == &p(Punct::RParen) {
            self.ts.advance();
        } else if !self.check(&p(Punct::RParen)) {
            loop {
                let ty = self.type_ref()?;
                let by_ref = self.by_ref();
                let name = if matches!(self.ts.peek_kind(), TokenKind::Ident(_)) {
                    Some(self.name()?)
                } else {
                    None
                };
                let default = if self.eat(&p(Punct::Assign)) {
                    Some(self.expr()?)
                } else {
                    None
                };
                params.push(Param {
                    ty,
                    by_ref,
                    name,
                    default,
                });
                if !self.eat(&p(Punct::Comma)) {
                    break;
                }
            }
        }
        self.expect(p(Punct::RParen))?;
        Ok(params)
    }

    /// `const`, `final`, `override`, `property`, `explicit` after a
    /// parameter list.
    fn func_tail_modifiers(&mut self, m: &mut Modifiers) {
        loop {
            if self.eat(&kw(Kw::Const)) {
                m.const_method = true;
            } else if self.eat_word("final") {
                m.is_final = true;
            } else if self.eat_word("override") {
                m.is_override = true;
            } else if self.eat_word("property") {
                m.property = true;
            } else if self.eat_word("explicit") {
                m.explicit = true;
            } else {
                return;
            }
        }
    }

    /// A function body `{ ... }` or `;` (declaration only).
    fn func_body(&mut self) -> PResult<Option<Vec<Stmt>>> {
        if self.eat(&p(Punct::Semicolon)) {
            Ok(None)
        } else {
            Ok(Some(self.block()?))
        }
    }

    /// A declaration starting with a type (or a constructor/destructor in
    /// a class): a function, a virtual property, or variables.
    fn decl(&mut self, modifiers: Modifiers, ctx: Ctx, line: usize) -> PResult<Decl> {
        if let Ctx::Class(class) = ctx {
            let destructor = self.check(&p(Punct::Tilde));
            let ctor = self.is_word(class) && self.ts.nth_kind(1) == &p(Punct::LParen);
            if destructor || ctor {
                if destructor {
                    self.ts.advance();
                }
                let name = self.name()?;
                let params = self.params()?;
                let mut modifiers = modifiers;
                self.func_tail_modifiers(&mut modifiers);
                let body = self.func_body()?;
                return Ok(Decl::Function(FunctionDecl {
                    modifiers,
                    ret: None,
                    ret_ref: false,
                    name,
                    destructor,
                    params,
                    body,
                    line,
                }));
            }
        }
        let ty = self.type_ref()?;
        let ret_ref = self.by_ref().is_some();
        let name = self.name()?;

        if self.check(&p(Punct::LParen)) {
            let params = self.params()?;
            let mut modifiers = modifiers;
            self.func_tail_modifiers(&mut modifiers);
            let body = self.func_body()?;
            return Ok(Decl::Function(FunctionDecl {
                modifiers,
                ret: Some(ty),
                ret_ref,
                name,
                destructor: false,
                params,
                body,
                line,
            }));
        }
        if self.check(&p(Punct::LBrace)) {
            return Ok(Decl::Property(self.property(modifiers, ty, name, line)?));
        }
        Ok(Decl::Vars(self.var_rest(modifiers, ty, name, line)?))
    }

    /// `{ get const { ... } set { ... } }`
    fn property(&mut self, modifiers: Modifiers, ty: TypeRef, name: Name, line: usize) -> PResult<PropertyDecl> {
        self.expect(p(Punct::LBrace))?;
        let mut accessors = Vec::new();
        while !self.check(&p(Punct::RBrace)) {
            let aline = self.line();
            let kind = if self.eat_word("get") {
                AccessorKind::Get
            } else if self.eat_word("set") {
                AccessorKind::Set
            } else {
                return Err(self.unexpected("'get' or 'set'"));
            };
            let mut m = Modifiers::default();
            self.func_tail_modifiers(&mut m);
            let body = self.func_body()?;
            accessors.push(Accessor {
                kind,
                is_const: m.const_method,
                body,
                line: aline,
            });
        }
        self.expect(p(Punct::RBrace))?;
        Ok(PropertyDecl {
            modifiers,
            ty,
            name,
            accessors,
            line,
        })
    }

    /// The rest of `Type a [= init | (args)], b ...;` after the first name.
    fn var_rest(&mut self, modifiers: Modifiers, ty: TypeRef, first: Name, line: usize) -> PResult<Vec<VarDecl>> {
        let mut vars = Vec::new();
        let mut name = first;
        loop {
            let init = if self.eat(&p(Punct::Assign)) {
                Some(VarInit::Expr(self.init_expr()?))
            } else if self.check(&p(Punct::LParen)) {
                Some(VarInit::Args(self.args(Punct::LParen, Punct::RParen)?))
            } else {
                None
            };
            vars.push(VarDecl {
                modifiers: modifiers.clone(),
                ty: ty.clone(),
                name,
                init,
                line,
            });
            if !self.eat(&p(Punct::Comma)) {
                break;
            }
            name = self.name()?;
        }
        self.expect(p(Punct::Semicolon))?;
        Ok(vars)
    }

    // ----- statements -------------------------------------------------------

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(p(Punct::LBrace))?;
        let mut stmts = Vec::new();
        while !self.check(&p(Punct::RBrace)) {
            if self.ts.at_eof() {
                return Err(self.unexpected("'}'"));
            }
            self.stmt(&mut stmts)?;
        }
        self.expect(p(Punct::RBrace))?;
        Ok(stmts)
    }

    /// A `{ block }` or a single statement.
    fn body(&mut self) -> PResult<Vec<Stmt>> {
        if self.check(&p(Punct::LBrace)) {
            self.block()
        } else {
            let mut out = Vec::new();
            self.stmt(&mut out)?;
            Ok(out)
        }
    }

    fn paren_expr(&mut self) -> PResult<Expr> {
        self.expect(p(Punct::LParen))?;
        let e = self.expr()?;
        self.expect(p(Punct::RParen))?;
        Ok(e)
    }

    /// One statement into `out` (a multi-name declaration yields several).
    fn stmt(&mut self, out: &mut Vec<Stmt>) -> PResult<()> {
        let line = self.line();
        let stmt = match self.ts.peek_kind().clone() {
            TokenKind::Punct(Punct::Semicolon) => {
                self.ts.advance();
                return Ok(());
            }
            TokenKind::Punct(Punct::LBrace) => Stmt::Block {
                body: self.block()?,
                line,
            },
            TokenKind::Keyword(Kw::If) => {
                self.ts.advance();
                let cond = self.paren_expr()?;
                let then_body = self.body()?;
                let else_body = if self.eat(&kw(Kw::Else)) {
                    Some(self.body()?)
                } else {
                    None
                };
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                    line,
                }
            }
            TokenKind::Keyword(Kw::While) => {
                self.ts.advance();
                let cond = self.paren_expr()?;
                let body = self.body()?;
                Stmt::While { cond, body, line }
            }
            TokenKind::Keyword(Kw::Do) => {
                self.ts.advance();
                let body = self.body()?;
                self.expect(kw(Kw::While))?;
                let cond = self.paren_expr()?;
                self.expect(p(Punct::Semicolon))?;
                Stmt::DoWhile { body, cond, line }
            }
            TokenKind::Keyword(Kw::For) => self.for_stmt()?,
            TokenKind::Keyword(Kw::Switch) => self.switch_stmt()?,
            TokenKind::Keyword(Kw::Break) => {
                self.ts.advance();
                self.expect(p(Punct::Semicolon))?;
                Stmt::Break { line }
            }
            TokenKind::Keyword(Kw::Continue) => {
                self.ts.advance();
                self.expect(p(Punct::Semicolon))?;
                Stmt::Continue { line }
            }
            TokenKind::Keyword(Kw::Return) => {
                self.ts.advance();
                let value = if self.check(&p(Punct::Semicolon)) {
                    None
                } else {
                    Some(self.expr()?)
                };
                self.expect(p(Punct::Semicolon))?;
                Stmt::Return { value, line }
            }
            TokenKind::Keyword(Kw::Try) => {
                self.ts.advance();
                let body = self.block()?;
                self.expect(kw(Kw::Catch))?;
                let catch = self.block()?;
                Stmt::Try { body, catch, line }
            }
            _ if self.at_decl() => {
                let ty = self.type_ref()?;
                let name = self.name()?;
                out.extend(self.var_rest(Modifiers::default(), ty, name, line)?.into_iter().map(Stmt::Var));
                return Ok(());
            }
            _ => {
                let expr = self.expr()?;
                self.expect(p(Punct::Semicolon))?;
                Stmt::Expr { expr, line }
            }
        };
        out.push(stmt);
        Ok(())
    }

    fn for_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(kw(Kw::For))?;
        self.expect(p(Punct::LParen))?;
        let mut init = Vec::new();
        if !self.eat(&p(Punct::Semicolon)) {
            // a declaration or an expression statement, `;` included
            self.stmt(&mut init)?;
        }
        let cond = if self.check(&p(Punct::Semicolon)) {
            None
        } else {
            Some(self.expr()?)
        };
        self.expect(p(Punct::Semicolon))?;
        let mut incr = Vec::new();
        if !self.check(&p(Punct::RParen)) {
            loop {
                incr.push(self.expr()?);
                if !self.eat(&p(Punct::Comma)) {
                    break;
                }
            }
        }
        self.expect(p(Punct::RParen))?;
        let body = self.body()?;
        Ok(Stmt::For {
            init,
            cond,
            incr,
            body,
            line,
        })
    }

    fn switch_stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        self.expect(kw(Kw::Switch))?;
        let value = self.paren_expr()?;
        self.expect(p(Punct::LBrace))?;
        let mut cases = Vec::new();
        while !self.check(&p(Punct::RBrace)) {
            let cline = self.line();
            let mut labels = Vec::new();
            loop {
                if self.eat(&kw(Kw::Case)) {
                    labels.push(Some(self.expr()?));
                } else if self.eat(&kw(Kw::Default)) {
                    labels.push(None);
                } else {
                    break;
                }
                self.expect(p(Punct::Colon))?;
            }
            if labels.is_empty() {
                return Err(self.unexpected("'case' or 'default'"));
            }
            let mut body = Vec::new();
            while !matches!(
                self.ts.peek_kind(),
                TokenKind::Keyword(Kw::Case | Kw::Default) | TokenKind::Punct(Punct::RBrace) | TokenKind::Eof
            ) {
                self.stmt(&mut body)?;
            }
            cases.push(Case {
                labels,
                body,
                line: cline,
            });
        }
        self.expect(p(Punct::RBrace))?;
        Ok(Stmt::Switch { value, cases, line })
    }

    // ----- expressions ------------------------------------------------------

    pub fn expr(&mut self) -> PResult<Expr> {
        self.assign()
    }

    /// An initializer: an expression or an `{init, list}`.
    fn init_expr(&mut self) -> PResult<Expr> {
        if self.check(&p(Punct::LBrace)) {
            self.init_list()
        } else {
            self.assign()
        }
    }

    fn init_list(&mut self) -> PResult<Expr> {
        let open = self.ts.peek().clone();
        let items = self.args(Punct::LBrace, Punct::RBrace)?;
        Ok(Expr::new(ExprKind::InitList(items), open.span().join(self.ts.prev().span())))
    }

    /// `open expr, expr close` (items may be init lists).
    fn args(&mut self, open: Punct, close: Punct) -> PResult<Vec<Expr>> {
        self.expect(p(open))?;
        let mut items = Vec::new();
        while !self.check(&p(close)) {
            items.push(self.init_expr()?);
            if !self.eat(&p(Punct::Comma)) {
                break;
            }
        }
        self.expect(p(close))?;
        Ok(items)
    }

    fn assign(&mut self) -> PResult<Expr> {
        let target = self.ternary()?;
        if let Some(op) = assign_op(self.ts.peek_kind()) {
            self.ts.advance();
            let value = self.assign()?; // right-associative
            let span = target.span.join(value.span);
            return Ok(Expr::new(ExprKind::Assign(Box::new(target), op, Box::new(value)), span));
        }
        Ok(target)
    }

    fn ternary(&mut self) -> PResult<Expr> {
        let cond = self.binary(1)?;
        if !self.eat(&p(Punct::Question)) {
            return Ok(cond);
        }
        let then = self.assign()?;
        self.expect(p(Punct::Colon))?;
        let other = self.assign()?;
        let span = cond.span.join(other.span);
        Ok(Expr::new(ExprKind::Ternary(Box::new(cond), Box::new(then), Box::new(other)), span))
    }

    /// The next binary operator: `!is` is two tokens.
    fn peek_binop(&self) -> Option<(BinOp, u8, usize)> {
        if self.check(&p(Punct::Bang)) && self.ts.nth_kind(1) == &kw(Kw::Is) {
            return Some((BinOp::NotIs, 3, 2));
        }
        binop(self.ts.peek_kind()).map(|(op, prec)| (op, prec, 1))
    }

    /// Precedence climbing; `**` is right-associative.
    fn binary(&mut self, min: u8) -> PResult<Expr> {
        let mut left = self.unary()?;
        while let Some((op, prec, len)) = self.peek_binop() {
            if prec < min {
                break;
            }
            for _ in 0..len {
                self.ts.advance();
            }
            let next_min = if op == BinOp::Pow { prec } else { prec + 1 };
            let right = self.binary(next_min)?;
            let span = left.span.join(right.span);
            left = Expr::new(ExprKind::Binary(Box::new(left), op, Box::new(right)), span);
        }
        Ok(left)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.ts.peek_kind() {
            TokenKind::Punct(Punct::Minus) => UnOp::Neg,
            TokenKind::Punct(Punct::Plus) => UnOp::Plus,
            TokenKind::Punct(Punct::Bang) | TokenKind::Keyword(Kw::Not) => UnOp::Not,
            TokenKind::Punct(Punct::Tilde) => UnOp::BitNot,
            TokenKind::Punct(Punct::PlusPlus) => UnOp::PreInc,
            TokenKind::Punct(Punct::MinusMinus) => UnOp::PreDec,
            TokenKind::Punct(Punct::At) => UnOp::Handle,
            _ => return self.postfix(),
        };
        let op_tok = self.ts.advance();
        let operand = self.unary()?;
        let span = op_tok.span().join(operand.span);
        Ok(Expr::new(ExprKind::Unary(op, Box::new(operand)), span))
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            match self.ts.peek_kind().clone() {
                TokenKind::Punct(Punct::Dot) => {
                    self.ts.advance();
                    let name = self.name()?;
                    let span = e.span.join(name.span);
                    e = Expr::new(ExprKind::Member(Box::new(e), name.name), span);
                }
                TokenKind::Punct(Punct::LBracket) => {
                    let args = self.args(Punct::LBracket, Punct::RBracket)?;
                    let span = e.span.join(self.ts.prev().span());
                    e = Expr::new(ExprKind::Index(Box::new(e), args), span);
                }
                TokenKind::Punct(Punct::LParen) => {
                    let args = self.args(Punct::LParen, Punct::RParen)?;
                    let span = e.span.join(self.ts.prev().span());
                    e = Expr::new(ExprKind::Call(Box::new(e), args), span);
                }
                TokenKind::Punct(op @ (Punct::PlusPlus | Punct::MinusMinus)) => {
                    let t = self.ts.advance();
                    let op = if op == Punct::PlusPlus { PostOp::Inc } else { PostOp::Dec };
                    let span = e.span.join(t.span());
                    e = Expr::new(ExprKind::Postfix(Box::new(e), op), span);
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary(&mut self) -> PResult<Expr> {
        let tok = self.ts.peek().clone();
        let leaf = |this: &mut Self, kind| {
            this.ts.advance();
            Ok(Expr::new(kind, tok.span()))
        };
        match tok.kind.clone() {
            TokenKind::Int(v) => leaf(self, ExprKind::Int(v)),
            TokenKind::Real(v) => leaf(self, ExprKind::Real(v)),
            TokenKind::Str(s) => {
                // adjacent string literals are concatenated
                self.ts.advance();
                let mut s = s;
                while let TokenKind::Str(more) = self.ts.peek_kind().clone() {
                    self.ts.advance();
                    s.push_str(&more);
                }
                Ok(Expr::new(ExprKind::Str(s), tok.span().join(self.ts.prev().span())))
            }
            TokenKind::Keyword(Kw::True) => leaf(self, ExprKind::Bool(true)),
            TokenKind::Keyword(Kw::False) => leaf(self, ExprKind::Bool(false)),
            TokenKind::Keyword(Kw::Null) => leaf(self, ExprKind::Null),
            TokenKind::Punct(Punct::LParen) => {
                self.ts.advance();
                let inner = self.expr()?;
                let close = self.expect(p(Punct::RParen))?;
                Ok(Expr::new(inner.kind, tok.span().join(close.span())))
            }
            TokenKind::Punct(Punct::LBrace) => self.init_list(),
            TokenKind::Keyword(Kw::Cast) => {
                self.ts.advance();
                self.expect(p(Punct::Lt))?;
                let ty = self.type_ref()?;
                self.expect_gt()?;
                let inner = self.paren_expr()?;
                let span = tok.span().join(self.ts.prev().span());
                Ok(Expr::new(ExprKind::Cast(ty, Box::new(inner)), span))
            }
            TokenKind::Ident(w) if w == "function" && self.ts.nth_kind(1) == &p(Punct::LParen) => self.lambda(),
            TokenKind::Keyword(k) if PRIMITIVES.contains(k) => {
                // `int(x)`: a conversion / construction
                let ty = self.type_ref()?;
                let args = self.args(Punct::LParen, Punct::RParen)?;
                let span = tok.span().join(self.ts.prev().span());
                Ok(Expr::new(ExprKind::Construct(ty, args), span))
            }
            TokenKind::Ident(_) | TokenKind::Punct(Punct::ColonColon) => {
                // `array<int>(3)`: a templated type being constructed
                if let Some(ty) = self.attempt(|this| {
                    let ty = this.type_ref()?;
                    if ty.template.is_empty() || !this.check(&p(Punct::LParen)) {
                        return Err(this.error("not a construction"));
                    }
                    Ok(ty)
                }) {
                    let args = self.args(Punct::LParen, Punct::RParen)?;
                    let span = tok.span().join(self.ts.prev().span());
                    return Ok(Expr::new(ExprKind::Construct(ty, args), span));
                }
                let mut scope = Vec::new();
                if self.eat(&p(Punct::ColonColon)) {
                    scope.push(String::new());
                }
                let mut name = self.name()?;
                while self.check(&p(Punct::ColonColon)) {
                    self.ts.advance();
                    scope.push(std::mem::replace(&mut name, self.name()?).name);
                }
                Ok(Expr::new(ExprKind::Name(scope, name.name), tok.span().join(name.span)))
            }
            other => Err(self.error(format!("unexpected {other} in expression"))),
        }
    }

    /// `function(a, int b) { ... }`: parameter types are optional.
    fn lambda(&mut self) -> PResult<Expr> {
        let start = self.ts.advance(); // `function`
        self.expect(p(Punct::LParen))?;
        let mut params = Vec::new();
        while !self.check(&p(Punct::RParen)) {
            let name_only = matches!(self.ts.peek_kind(), TokenKind::Ident(_))
                && matches!(self.ts.nth_kind(1), TokenKind::Punct(Punct::Comma | Punct::RParen));
            let param = if name_only {
                let name = self.name()?;
                Param {
                    ty: TypeRef {
                        is_const: false,
                        scope: Vec::new(),
                        name: "auto".into(),
                        template: Vec::new(),
                        suffixes: Vec::new(),
                        span: name.span,
                    },
                    by_ref: None,
                    name: Some(name),
                    default: None,
                }
            } else {
                let ty = self.type_ref()?;
                let by_ref = self.by_ref();
                Param {
                    ty,
                    by_ref,
                    name: Some(self.name()?),
                    default: None,
                }
            };
            params.push(param);
            if !self.eat(&p(Punct::Comma)) {
                break;
            }
        }
        self.expect(p(Punct::RParen))?;
        let body = self.block()?;
        let decl = FunctionDecl {
            modifiers: Modifiers::default(),
            ret: None,
            ret_ref: false,
            name: Name {
                name: String::new(),
                span: start.span(),
            },
            destructor: false,
            params,
            body: Some(body),
            line: start.line,
        };
        let span = start.span().join(self.ts.prev().span());
        Ok(Expr::new(ExprKind::Lambda(Box::new(decl)), span))
    }
}

#[cfg(test)]
#[path = "parser.test.rs"]
mod tests;

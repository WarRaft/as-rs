/// [`Span`] and [`ColumnEncoding`] live in the shared `parser-rs` crate.
pub use parser_rs::{ColumnEncoding, Span};

/// An AngelScript script section (a file).
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub items: Vec<Item>,
}

/// A declared name with its exact location.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    pub name: String,
    pub span: Span,
}

/// Declaration modifiers. `shared`, `external`, `abstract`, `final`,
/// `override`, `explicit` and `property` are contextual words in the
/// language; `private`/`protected` are reserved.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Modifiers {
    pub shared: bool,
    pub external: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_override: bool,
    pub explicit: bool,
    pub property: bool,
    pub private: bool,
    pub protected: bool,
    /// `const` after a method's parameter list.
    pub const_method: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypeSuffix {
    /// `[]`
    Array,
    /// `@`
    Handle,
    /// `@ const`
    ConstHandle,
}

/// A type: `const ns::Name<T, U>[]@`. `?` (the "any type" of registered
/// functions) is a type named `?`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub is_const: bool,
    /// Namespace qualifiers; a leading `::` is an empty first entry.
    pub scope: Vec<String>,
    pub name: String,
    pub template: Vec<TypeRef>,
    pub suffixes: Vec<TypeSuffix>,
    pub span: Span,
}

impl TypeRef {
    /// The type as written, normalized: `const array<int>@`.
    pub fn display(&self) -> String {
        let mut s = String::new();
        if self.is_const {
            s.push_str("const ");
        }
        for ns in &self.scope {
            s.push_str(ns);
            s.push_str("::");
        }
        s.push_str(&self.name);
        if !self.template.is_empty() {
            let args: Vec<String> = self.template.iter().map(TypeRef::display).collect();
            s.push('<');
            s.push_str(&args.join(", "));
            s.push('>');
        }
        for suffix in &self.suffixes {
            s.push_str(match suffix {
                TypeSuffix::Array => "[]",
                TypeSuffix::Handle => "@",
                TypeSuffix::ConstHandle => "@ const",
            });
        }
        s
    }

    pub fn is_void(&self) -> bool {
        self.name == "void" && self.suffixes.is_empty() && self.scope.is_empty()
    }
}

/// `&`, `&in`, `&out`, `&inout` on a parameter or return type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RefKind {
    Ref,
    In,
    Out,
    InOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub ty: TypeRef,
    pub by_ref: Option<RefKind>,
    /// Parameter names are optional in declarations without a body.
    pub name: Option<Name>,
    pub default: Option<Expr>,
}

/// A function, method, constructor/destructor, or lambda.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub modifiers: Modifiers,
    /// `None` for constructors, destructors and lambdas.
    pub ret: Option<TypeRef>,
    pub ret_ref: bool,
    pub name: Name,
    /// `~Name()`
    pub destructor: bool,
    pub params: Vec<Param>,
    /// `None` for a declaration without a body (`;`).
    pub body: Option<Vec<Stmt>>,
    pub line: usize,
}

/// How a variable is initialized.
#[derive(Debug, Clone, PartialEq)]
pub enum VarInit {
    /// `= expr`
    Expr(Expr),
    /// `Type name(args)`
    Args(Vec<Expr>),
}

/// One declared variable: global, class member, or local.
/// `int a, b = 1;` declares two.
#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub modifiers: Modifiers,
    pub ty: TypeRef,
    pub name: Name,
    pub init: Option<VarInit>,
    pub line: usize,
}

impl VarDecl {
    pub fn exprs(&self) -> Vec<&Expr> {
        match &self.init {
            Some(VarInit::Expr(e)) => vec![e],
            Some(VarInit::Args(args)) => args.iter().collect(),
            None => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccessorKind {
    Get,
    Set,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Accessor {
    pub kind: AccessorKind,
    pub is_const: bool,
    pub body: Option<Vec<Stmt>>,
    pub line: usize,
}

/// A virtual property: `int count { get { ... } set { ... } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    pub modifiers: Modifiers,
    pub ty: TypeRef,
    pub name: Name,
    pub accessors: Vec<Accessor>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Var(VarDecl),
    Method(FunctionDecl),
    Property(PropertyDecl),
    Funcdef(FuncdefDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub modifiers: Modifiers,
    pub mixin: bool,
    pub name: Name,
    pub bases: Vec<Name>,
    /// `None` for an external declaration `class Name;`.
    pub members: Option<Vec<Member>>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub modifiers: Modifiers,
    pub name: Name,
    pub bases: Vec<Name>,
    pub methods: Vec<FunctionDecl>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub modifiers: Modifiers,
    pub name: Name,
    pub values: Vec<(Name, Option<Expr>)>,
    pub line: usize,
}

/// `funcdef RetType Name(params);`: a function signature type.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncdefDecl {
    pub modifiers: Modifiers,
    pub ret: TypeRef,
    pub name: Name,
    pub params: Vec<Param>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Namespace {
        name: Name,
        items: Vec<Item>,
        line: usize,
    },
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Enum(EnumDecl),
    Funcdef(FuncdefDecl),
    /// `typedef float real;`
    Typedef {
        base: TypeRef,
        name: Name,
        line: usize,
    },
    /// `import void f(int) from "module";`
    Import {
        func: FunctionDecl,
        from: String,
        line: usize,
    },
    Function(FunctionDecl),
    Var(VarDecl),
    Property(PropertyDecl),
}

impl Item {
    pub fn line(&self) -> usize {
        match self {
            Item::Namespace { line, .. } | Item::Typedef { line, .. } | Item::Import { line, .. } => *line,
            Item::Class(c) => c.line,
            Item::Interface(i) => i.line,
            Item::Enum(e) => e.line,
            Item::Funcdef(f) => f.line,
            Item::Function(f) => f.line,
            Item::Var(v) => v.line,
            Item::Property(p) => p.line,
        }
    }
}

/// A `case`/`default` group of a `switch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// `None` is `default`.
    pub labels: Vec<Option<Expr>>,
    pub body: Vec<Stmt>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Var(VarDecl),
    Expr {
        expr: Expr,
        line: usize,
    },
    Block {
        body: Vec<Stmt>,
        line: usize,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
        line: usize,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        line: usize,
    },
    DoWhile {
        body: Vec<Stmt>,
        cond: Expr,
        line: usize,
    },
    /// `for (init; cond; incr) body`: `init` is a declaration or an
    /// expression statement.
    For {
        init: Vec<Stmt>,
        cond: Option<Expr>,
        incr: Vec<Expr>,
        body: Vec<Stmt>,
        line: usize,
    },
    Switch {
        value: Expr,
        cases: Vec<Case>,
        line: usize,
    },
    Break {
        line: usize,
    },
    Continue {
        line: usize,
    },
    Return {
        value: Option<Expr>,
        line: usize,
    },
    Try {
        body: Vec<Stmt>,
        catch: Vec<Stmt>,
        line: usize,
    },
}

impl Stmt {
    pub fn line(&self) -> usize {
        match self {
            Stmt::Var(v) => v.line,
            Stmt::Expr { line, .. }
            | Stmt::Block { line, .. }
            | Stmt::If { line, .. }
            | Stmt::While { line, .. }
            | Stmt::DoWhile { line, .. }
            | Stmt::For { line, .. }
            | Stmt::Switch { line, .. }
            | Stmt::Break { line }
            | Stmt::Continue { line }
            | Stmt::Return { line, .. }
            | Stmt::Try { line, .. } => *line,
        }
    }

    /// The statement's own expressions (not those in nested statements).
    pub fn exprs(&self) -> Vec<&Expr> {
        match self {
            Stmt::Var(v) => v.exprs(),
            Stmt::Expr { expr, .. } => vec![expr],
            Stmt::If { cond, .. } | Stmt::While { cond, .. } | Stmt::DoWhile { cond, .. } => vec![cond],
            Stmt::For { cond, incr, .. } => cond.iter().chain(incr).collect(),
            Stmt::Switch { value, cases, .. } => std::iter::once(value)
                .chain(cases.iter().flat_map(|c| c.labels.iter().flatten()))
                .collect(),
            Stmt::Return { value, .. } => value.iter().collect(),
            Stmt::Block { .. } | Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Try { .. } => Vec::new(),
        }
    }

    /// Nested statement lists. A `for`'s `init` comes first.
    pub fn bodies(&self) -> Vec<&[Stmt]> {
        match self {
            Stmt::Block { body, .. } | Stmt::While { body, .. } | Stmt::DoWhile { body, .. } => vec![body],
            Stmt::If {
                then_body,
                else_body,
                ..
            } => std::iter::once(then_body.as_slice()).chain(else_body.as_deref()).collect(),
            Stmt::For { init, body, .. } => vec![init, body],
            Stmt::Switch { cases, .. } => cases.iter().map(|c| c.body.as_slice()).collect(),
            Stmt::Try { body, catch, .. } => vec![body, catch],
            Stmt::Var(_) | Stmt::Expr { .. } | Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Return { .. } => {
                Vec::new()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Compares only `kind`, ignoring `span`.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span }
    }

    /// Direct sub-expressions. A lambda's body is not included.
    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Member(obj, _) | ExprKind::Unary(_, obj) | ExprKind::Postfix(obj, _) | ExprKind::Cast(_, obj) => {
                vec![obj]
            }
            ExprKind::Index(obj, args) | ExprKind::Call(obj, args) => std::iter::once(&**obj).chain(args).collect(),
            ExprKind::Construct(_, args) | ExprKind::InitList(args) => args.iter().collect(),
            ExprKind::Binary(l, _, r) | ExprKind::Assign(l, _, r) => vec![l, r],
            ExprKind::Ternary(c, a, b) => vec![c, a, b],
            ExprKind::Int(_)
            | ExprKind::Real(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::Null
            | ExprKind::Name(..)
            | ExprKind::Lambda(_) => Vec::new(),
        }
    }

    /// `a.b.c` / `ns::f` as text, if this is a plain name or member chain.
    pub fn path(&self) -> Option<String> {
        match &self.kind {
            ExprKind::Name(scope, name) => Some(
                scope
                    .iter()
                    .map(|s| format!("{s}::"))
                    .chain([name.clone()])
                    .collect(),
            ),
            ExprKind::Member(obj, name) => Some(format!("{}.{name}", obj.path()?)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Null,
    /// A possibly namespace-qualified name: `(["ns"], "f")`. `this` and
    /// `super` are names too.
    Name(Vec<String>, String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Vec<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    /// `array<int>(3)`, `int(x)`: constructing a value of an explicit type.
    Construct(TypeRef, Vec<Expr>),
    /// `cast<Foo>(expr)`
    Cast(TypeRef, Box<Expr>),
    /// `function(a, b) { ... }`
    Lambda(Box<FunctionDecl>),
    /// `{1, 2, 3}`
    InitList(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Postfix(Box<Expr>, PostOp),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    Assign(Box<Expr>, AssignOp, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
    BitNot,
    PreInc,
    PreDec,
    /// `@x`: handle of
    Handle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostOp {
    Inc,
    Dec,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Or,
    Xor,
    And,
    Eq,
    NotEq,
    Is,
    NotIs,
    Lt,
    Gt,
    Le,
    Ge,
    BitOr,
    BitXor,
    BitAnd,
    Shl,
    Shr,
    UShr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
}

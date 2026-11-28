use super::*;

fn parse(src: &str) -> Script {
    Parser::parse_str(src).unwrap_or_else(|e| panic!("should parse: {e:?}"))
}

/// Parses `body` as the body of `void f()`.
fn body(body: &str) -> Vec<Stmt> {
    match parse(&format!("void f() {{ {body} }}")).items.remove(0) {
        Item::Function(f) => f.body.unwrap(),
        other => panic!("expected function, got {other:?}"),
    }
}

fn expr(src: &str) -> Expr {
    match body(&format!("x = {src};")).remove(0) {
        Stmt::Expr {
            expr:
                Expr {
                    kind: ExprKind::Assign(_, _, value),
                    ..
                },
            ..
        } => *value,
        other => panic!("expected assignment, got {other:?}"),
    }
}

fn name(n: &str) -> Box<Expr> {
    Box::new(Expr::new(ExprKind::Name(Vec::new(), n.into()), Span::whole_line(0)))
}

fn bin(l: Box<Expr>, op: BinOp, r: Box<Expr>) -> Box<Expr> {
    Box::new(Expr::new(ExprKind::Binary(l, op, r), Span::whole_line(0)))
}

#[test]
fn parses_top_level_declarations() {
    let s = parse(
        "namespace N { int x; }\n\
         shared class C : B, I { }\n\
         class E;\n\
         interface I { void f(); }\n\
         enum Color { Red, Green = 5 }\n\
         funcdef bool Pred(int);\n\
         typedef float real;\n\
         import void ext(int) from \"other\";\n\
         const int A = 1, B = 2;\n\
         int get_count() { return 0; }\n\
         int count { get { return 0; } }",
    );
    let kinds: Vec<&str> = s
        .items
        .iter()
        .map(|i| match i {
            Item::Namespace { .. } => "namespace",
            Item::Class(_) => "class",
            Item::Interface(_) => "interface",
            Item::Enum(_) => "enum",
            Item::Funcdef(_) => "funcdef",
            Item::Typedef { .. } => "typedef",
            Item::Import { .. } => "import",
            Item::Function(_) => "function",
            Item::Var(_) => "var",
            Item::Property(_) => "property",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "namespace", "class", "class", "interface", "enum", "funcdef", "typedef", "import", "var", "var",
            "function", "property"
        ]
    );
    let Item::Class(c) = &s.items[1] else { unreachable!() };
    assert!(c.modifiers.shared);
    assert_eq!(c.bases.len(), 2);
    let Item::Class(e) = &s.items[2] else { unreachable!() };
    assert!(e.members.is_none());
    let Item::Var(a) = &s.items[8] else { unreachable!() };
    assert!(a.ty.is_const);
}

#[test]
fn parses_class_members() {
    let s = parse(
        "class Unit {\n\
           private int hp = 10;\n\
           Unit(int h) { hp = h; }\n\
           ~Unit() {}\n\
           int get_hp() const final { return hp; }\n\
           float speed { get const { return 1; } set { } }\n\
           funcdef void Cb();\n\
         }",
    );
    let Item::Class(c) = &s.items[0] else { unreachable!() };
    let members = c.members.as_ref().unwrap();
    assert!(matches!(&members[0], Member::Var(v) if v.modifiers.private && v.name.name == "hp"));
    assert!(matches!(&members[1], Member::Method(f) if f.ret.is_none() && f.name.name == "Unit"));
    assert!(matches!(&members[2], Member::Method(f) if f.destructor));
    assert!(matches!(&members[3], Member::Method(f) if f.modifiers.const_method && f.modifiers.is_final));
    assert!(matches!(&members[4], Member::Property(p) if p.accessors.len() == 2 && p.accessors[0].is_const));
    assert!(matches!(&members[5], Member::Funcdef(_)));
}

#[test]
fn parses_types_with_templates_handles_and_refs() {
    let s = parse("void f(const array<array<int>>@ &in a, ::N::T[] b, ? &out c) {}");
    let Item::Function(f) = &s.items[0] else { unreachable!() };
    assert_eq!(f.params[0].ty.display(), "const array<array<int>>@");
    assert_eq!(f.params[0].by_ref, Some(RefKind::In));
    assert_eq!(f.params[1].ty.display(), "::N::T[]");
    assert_eq!(f.params[2].ty.name, "?");
}

#[test]
fn tells_declarations_from_expressions() {
    let stmts = body("Foo f; a < b; x = y; array<int> arr(3); Foo@ h = null; obj.m(); int a = 1, b;");
    let kinds: Vec<&str> = stmts
        .iter()
        .map(|s| match s {
            Stmt::Var(_) => "var",
            Stmt::Expr { .. } => "expr",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["var", "expr", "expr", "var", "var", "expr", "var", "var"]);
    assert!(matches!(&stmts[3], Stmt::Var(v) if matches!(v.init, Some(VarInit::Args(_)))));
}

#[test]
fn parses_control_flow() {
    let stmts = body(
        "if (a) b(); else { c(); }\n\
         while (x) x--;\n\
         do { } while (y);\n\
         for (int i = 0; i < 3; i++, j--) {}\n\
         for (;;) break;\n\
         switch (v) { case 1: case 2: f(); break; default: g(); }\n\
         try { } catch { }\n\
         return;",
    );
    assert!(matches!(&stmts[0], Stmt::If { else_body: Some(_), .. }));
    assert!(matches!(&stmts[3], Stmt::For { init, incr, .. } if init.len() == 1 && incr.len() == 2));
    assert!(matches!(&stmts[4], Stmt::For { cond: None, .. }));
    match &stmts[5] {
        Stmt::Switch { cases, .. } => {
            assert_eq!(cases[0].labels.len(), 2);
            assert_eq!(cases[1].labels, vec![None]);
        }
        other => panic!("expected switch, got {other:?}"),
    }
    assert!(matches!(&stmts[6], Stmt::Try { .. }));
}

#[test]
fn follows_angelscript_precedence() {
    // bitwise binds tighter than comparison (unlike C)
    assert_eq!(
        expr("a == b & c").kind,
        ExprKind::Binary(name("a"), BinOp::Eq, bin(name("b"), BinOp::BitAnd, name("c")))
    );
    // `and` tighter than `or`; `**` right-associative
    assert!(matches!(expr("a || b && c").kind, ExprKind::Binary(_, BinOp::Or, _)));
    assert_eq!(
        expr("a ** b ** c").kind,
        ExprKind::Binary(name("a"), BinOp::Pow, bin(name("b"), BinOp::Pow, name("c")))
    );
    // assignment and ternary are right-associative
    assert!(matches!(expr("a ? b : c ? d : e").kind, ExprKind::Ternary(_, _, ref e) if matches!(e.kind, ExprKind::Ternary(..))));
    assert!(matches!(expr("p !is null").kind, ExprKind::Binary(_, BinOp::NotIs, _)));
}

#[test]
fn parses_special_expressions() {
    assert!(matches!(expr("cast<Foo>(x)").kind, ExprKind::Cast(ref t, _) if t.name == "Foo"));
    assert!(matches!(expr("array<int>(3)").kind, ExprKind::Construct(ref t, _) if t.template.len() == 1));
    assert!(matches!(expr("int(2.5)").kind, ExprKind::Construct(..)));
    assert!(matches!(expr("function(a, int b) { return; }").kind, ExprKind::Lambda(ref f) if f.params.len() == 2));
    assert!(matches!(expr("{1, {2, 3}}").kind, ExprKind::InitList(ref v) if v.len() == 2));
    assert!(matches!(expr("@obj").kind, ExprKind::Unary(UnOp::Handle, _)));
    assert_eq!(expr("N::M::f").path().as_deref(), Some("N::M::f"));
    assert!(matches!(expr("\"a\" \"b\"").kind, ExprKind::Str(ref s) if s == "ab"));
    assert!(matches!(expr("a[1, 2]").kind, ExprKind::Index(_, ref args) if args.len() == 2));
}

#[test]
fn reports_errors() {
    let err = Parser::parse_str("void f() { x = ; }").unwrap_err();
    assert_eq!(err.message, "unexpected ';' in expression");
    let err = Parser::parse_str("class A { int x }").unwrap_err();
    assert!(err.message.contains("expected ';'"), "{}", err.message);
    let err = Parser::parse_str("void f() {").unwrap_err();
    assert!(err.message.contains("expected '}'"));
}

/// Unary `!` (and its spelling `not`): binds tighter than `&&`, `==`;
/// nests; `!is` stays the handle comparison.
#[test]
fn unary_not() {
    assert!(matches!(expr("!a").kind, ExprKind::Unary(UnOp::Not, _)));
    assert!(matches!(expr("not a").kind, ExprKind::Unary(UnOp::Not, _)));
    assert!(matches!(&expr("!!a").kind, ExprKind::Unary(UnOp::Not, inner) if matches!(inner.kind, ExprKind::Unary(UnOp::Not, _))));
    assert!(matches!(&expr("!a && b").kind, ExprKind::Binary(l, BinOp::And, _) if matches!(l.kind, ExprKind::Unary(UnOp::Not, _))));
    assert!(matches!(&expr("!a == b").kind, ExprKind::Binary(l, BinOp::Eq, _) if matches!(l.kind, ExprKind::Unary(UnOp::Not, _))));
    assert!(matches!(&expr("!f(!a)").kind, ExprKind::Unary(UnOp::Not, inner) if matches!(inner.kind, ExprKind::Call(..))));
    assert!(matches!(expr("a !is null").kind, ExprKind::Binary(_, BinOp::NotIs, _)));
}

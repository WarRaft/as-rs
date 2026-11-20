use super::*;
use crate::parser::Parser;

const SRC: &str = "class C {\n  int f(int a) {\n    return a + g(function(x) { return 42; });\n  }\n}";

#[test]
fn finds_nodes_in_methods_and_lambdas() {
    let s = Parser::parse_str(SRC).unwrap();
    match find_node_at(&s, 3, 12) {
        Some(AstRef::Expr(e)) => assert_eq!(e.kind, ExprKind::Name(Vec::new(), "a".into())),
        other => panic!("expected `a`, got {other:?}"),
    }
    // `42` inside the lambda
    match find_node_at(&s, 3, 39) {
        Some(AstRef::Expr(e)) => assert_eq!(e.kind, ExprKind::Int(42)),
        other => panic!("expected 42, got {other:?}"),
    }
    assert!(matches!(find_node_at(&s, 3, 1), Some(AstRef::Stmt(Stmt::Return { .. }))));
    assert!(matches!(find_node_at(&s, 2, 1), Some(AstRef::Member(Member::Method(_)))));
    assert!(matches!(find_node_at(&s, 1, 1), Some(AstRef::Item(Item::Class(_)))));
    assert!(find_node_at(&s, 99, 1).is_none());
}

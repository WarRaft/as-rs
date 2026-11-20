use super::*;
use crate::parser::Parser;

#[test]
fn renders_script_tree() {
    let s = Parser::parse_str("int add(int a, int b) {\n  return a + b;\n}").unwrap();
    assert_eq!(
        render_tree(&s),
        "Script\n\
         └─ Function int add(int a, int b) (line 1)\n   \
            └─ Return (line 2)\n      \
               └─ Binary Add\n         \
                  ├─ Name a\n         \
                  └─ Name b\n"
    );
}

#[test]
fn renders_classes_and_types() {
    let s = Parser::parse_str("shared class A : B { private array<int>@ xs; A() {} int n { get const { return 0; } } }").unwrap();
    let tree = render_tree(&s);
    assert!(tree.contains("shared Class A : B (line 1)"));
    assert!(tree.contains("Field private array<int>@ xs"));
    assert!(tree.contains("Method A() (line 1)"));
    assert!(tree.contains("Property int n"));
    assert!(tree.contains("get const"));
    assert!(render_dot(&s).starts_with("digraph AST {"));
}

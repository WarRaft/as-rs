use super::*;
use crate::parser::Parser;

fn lint_src(src: &str) -> Vec<Diagnostic> {
    lint(&Parser::parse_str(src).expect("source should parse"))
}

fn lint_body(body: &str) -> Vec<Diagnostic> {
    lint_src(&format!("void f() {{ {body} }}"))
}

fn has(d: &[Diagnostic], level: Level, text: &str) -> bool {
    d.iter().any(|d| d.level == level && d.message.contains(text))
}

#[test]
fn flags_duplicate_declarations_but_allows_overloads() {
    let d = lint_src("class A {} class A {}");
    assert!(has(&d, Level::Error, "duplicate declaration 'A'"));
    assert!(lint_src("void f() {} void f(int x) { x++; }").is_empty());
    let d = lint_src("class C { int x; float x; }");
    assert!(has(&d, Level::Error, "duplicate class member 'x'"));
    let d = lint_src("enum E { A, A }");
    assert!(has(&d, Level::Error, "duplicate enum value 'A'"));
}

#[test]
fn flags_unused_redeclared_and_shadowing_locals() {
    assert!(has(&lint_body("int a = 1;"), Level::Warning, "unused variable 'a'"));
    assert!(lint_body("int a = 1; print(a);").is_empty());
    assert!(has(&lint_body("int a; int a; a++;"), Level::Error, "already declared in this scope"));
    let d = lint_body("int a = 1; { int a = 2; a++; } a++;");
    assert!(has(&d, Level::Warning, "hides a variable"));
    assert!(has(&lint_src("void f(int p) { int p = 1; p++; }"), Level::Warning, "hides"));
    // `x = 1` writes, doesn't read
    assert!(has(&lint_body("int x; x = 1;"), Level::Warning, "unused variable 'x'"));
}

#[test]
fn checks_break_and_continue_placement() {
    assert!(has(&lint_body("break;"), Level::Error, "'break' outside a loop or switch"));
    assert!(has(&lint_body("continue;"), Level::Error, "'continue' outside a loop"));
    assert!(lint_body("switch (1) { case 1: break; }").is_empty());
    assert!(has(&lint_body("switch (1) { case 1: continue; }"), Level::Error, "'continue'"));
    assert!(lint_body("while (g()) { switch (1) { default: continue; } }").is_empty());
}

#[test]
fn requires_returns_on_all_paths() {
    let d = lint_src("int f(bool b) { if (b) return 1; }");
    assert!(has(&d, Level::Error, "not all paths of 'f' return a value"));
    assert!(lint_src("int f(bool b) { if (b) return 1; else return 2; }").is_empty());
    assert!(lint_src("int f() { while (true) { } }").iter().all(|d| !d.message.contains("not all paths")));
    assert!(lint_src("int f(int v) { switch (v) { case 1: return 1; default: return 0; } }").is_empty());
    assert!(lint_src("void f() { }").is_empty());
    let d = lint_src("class C { int v { get { } } }");
    assert!(has(&d, Level::Error, "not all paths of 'get' for 'v'"));
}

#[test]
fn flags_endless_loops_and_empty_ifs() {
    assert!(has(&lint_body("while (true) { g(); }"), Level::Warning, "always true"));
    assert!(has(&lint_body("for (;;) { g(); }"), Level::Warning, "no condition"));
    assert!(lint_body("for (;;) { if (g()) break; }").is_empty());
    assert!(has(&lint_body("if (g()) {}"), Level::Warning, "empty 'if' body"));
}

#[test]
fn checks_lambdas_and_accessors_as_functions() {
    let d = lint_body("g(function(a) { int unused; });");
    assert!(has(&d, Level::Warning, "unused variable 'unused'"));
    // `value` is the implicit parameter of `set`
    assert!(lint_src("class C { int v { set { g(value); } } }").is_empty());
}

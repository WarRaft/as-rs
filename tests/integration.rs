use as_rs::ast::Item;
use as_rs::{lint, parse};

const SAMPLE: &str = include_str!("fixtures/sample.as");

#[test]
fn sample_file_parses_successfully() {
    let script = parse(SAMPLE).expect("sample.as should parse");
    let names: Vec<String> = script
        .items
        .iter()
        .map(|i| match i {
            Item::Namespace { name, .. } => format!("namespace {}", name.name),
            Item::Function(f) => format!("function {}", f.name.name),
            Item::Var(v) => format!("var {}", v.name.name),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        names,
        [
            "namespace Game",
            "var units",
            "function countAlive",
            "function each",
            "function describe",
            "function main"
        ]
    );
}

#[test]
fn sample_file_has_no_lint_diagnostics() {
    let script = parse(SAMPLE).expect("sample.as should parse");
    let diagnostics = lint(&script);
    assert!(diagnostics.is_empty(), "expected sample.as to be clean, got {diagnostics:?}");
}

#[test]
fn reports_parse_error_with_location() {
    let err = parse("void f() {\n    int x = ;\n}").unwrap_err();
    assert_eq!((err.line, err.col), (2, 13));
}

#[test]
fn lint_catches_multiple_issues_at_once() {
    let src = "int f(bool b) {\n    int unused;\n    if (b) return 1;\n    break;\n}";
    let d = lint(&parse(src).unwrap());
    for needle in ["unused variable 'unused'", "not all paths of 'f'", "'break' outside"] {
        assert!(d.iter().any(|d| d.message.contains(needle)), "missing {needle:?} in {d:?}");
    }
}

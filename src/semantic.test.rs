use super::*;
use parser_rs::SemanticModifier as M;
use parser_rs::{Scope as S, SymbolKind as K};

const SRC: &str = include_str!("../tests/fixtures/sample.as");

/// `(type, modifiers)` of the `nth` whole-word occurrence of `word` on
/// `line` of `src`.
fn find(src: &str, line: usize, word: &str, nth: usize) -> (T, Modifiers) {
    let toks = semantic_tokens(src);
    let text = src.lines().nth(line - 1).unwrap();
    let word_char = |c: char| c.is_alphanumeric() || c == '_';
    let col = text
        .match_indices(word)
        .filter(|(i, _)| {
            !text[..*i].chars().last().is_some_and(word_char)
                && !text[i + word.len()..].chars().next().is_some_and(word_char)
        })
        .nth(nth)
        .map(|(i, _)| text[..i].chars().count() + 1)
        .unwrap_or_else(|| panic!("{word:?} not on line {line}: {text:?}"));
    let t = toks
        .iter()
        .find(|t| t.line == line && t.start_col == col)
        .unwrap_or_else(|| panic!("no token for {word:?} at {line}:{col}"));
    (t.ty, t.mods)
}

fn m(mods: &[M]) -> Modifiers {
    Modifiers::of(mods)
}

#[test]
fn classifies_declarations() {
    let s = SRC;
    assert_eq!(find(s, 6, "Game", 0), (T::Namespace, m(&[M::Declaration])));
    assert_eq!(find(s, 7, "Team", 0), (T::Enum, m(&[M::Declaration])));
    assert_eq!(find(s, 7, "Red", 0), (T::EnumMember, m(&[M::Declaration])));
    assert_eq!(find(s, 7, "Blue", 0), (T::EnumMember, m(&[M::Declaration])));
    assert_eq!(find(s, 9, "Callback", 0), (T::Type, m(&[M::Declaration])));
    assert_eq!(find(s, 9, "value", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(s, 11, "MAX_UNITS", 0), (T::Variable, m(&[M::Declaration, M::Readonly])));
    assert_eq!(find(s, 12, "created", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(s, 14, "IDamageable", 0), (T::Interface, m(&[M::Declaration])));
    assert_eq!(find(s, 15, "takeDamage", 0), (T::Method, m(&[M::Declaration])));
    assert_eq!(find(s, 15, "amount", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(s, 18, "shared", 0), (T::Keyword, m(&[])));
    assert_eq!(find(s, 18, "Unit", 0), (T::Class, m(&[M::Declaration])));
    assert_eq!(find(s, 18, "IDamageable", 0), (T::Interface, m(&[])));
    assert_eq!(find(s, 19, "hp", 0), (T::Property, m(&[M::Declaration])));
    assert_eq!(find(s, 20, "Team", 0), (T::Enum, m(&[])));
    assert_eq!(find(s, 20, "team", 0), (T::Property, m(&[M::Declaration])));
    assert_eq!(find(s, 22, "Unit", 0), (T::Method, m(&[M::Declaration]))); // constructor
    assert_eq!(find(s, 22, "startHp", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(s, 22, "Red", 0), (T::EnumMember, m(&[])));
    assert_eq!(find(s, 30, "health", 0), (T::Property, m(&[M::Declaration])));
    assert_eq!(find(s, 31, "get", 0), (T::Keyword, m(&[])));
    assert_eq!(find(s, 32, "set", 0), (T::Keyword, m(&[])));
    assert_eq!(find(s, 32, "value", 0), (T::Parameter, m(&[])));
    assert_eq!(find(s, 35, "override", 0), (T::Keyword, m(&[])));
    assert_eq!(find(s, 39, "opEquals", 0), (T::Method, m(&[M::Declaration])));
    assert_eq!(find(s, 45, "units", 0), (T::Variable, m(&[M::Declaration])));
    assert_eq!(find(s, 45, "array", 0), (T::Type, m(&[])));
    assert_eq!(find(s, 45, "Game", 0), (T::Namespace, m(&[])));
    assert_eq!(find(s, 47, "countAlive", 0), (T::Function, m(&[M::Declaration])));
    assert_eq!(find(s, 47, "list", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(s, 48, "alive", 0), (T::Variable, m(&[M::Declaration])));
}

#[test]
fn classifies_references() {
    let s = SRC;
    assert_eq!(find(s, 23, "hp", 0), (T::Variable, m(&[M::Modification])));
    assert_eq!(find(s, 23, "startHp", 0), (T::Parameter, m(&[])));
    assert_eq!(find(s, 40, "other", 0), (T::Parameter, m(&[])));
    assert_eq!(find(s, 40, "team", 0), (T::Property, m(&[]))); // other.team
    assert_eq!(find(s, 40, "team", 1), (T::Variable, m(&[]))); // implicit this.team
    assert_eq!(find(s, 49, "i", 1), (T::Variable, m(&[])));
    assert_eq!(find(s, 49, "length", 0), (T::Method, m(&[])));
    assert_eq!(find(s, 54, "alive", 0), (T::Variable, m(&[M::Modification])));
    assert_eq!(find(s, 61, "MAX_UNITS", 0), (T::Variable, m(&[M::Readonly])));
    assert_eq!(find(s, 62, "cb", 0), (T::Parameter, m(&[])));
    assert_eq!(find(s, 68, "Team", 0), (T::Enum, m(&[])));
    assert_eq!(find(s, 68, "Red", 0), (T::EnumMember, m(&[])));
    assert_eq!(find(s, 78, "Unit", 1), (T::Class, m(&[]))); // constructor call
    assert_eq!(find(s, 79, "insertLast", 0), (T::Method, m(&[])));
    assert_eq!(find(s, 82, "function", 0), (T::Keyword, m(&[])));
    assert_eq!(find(s, 82, "v", 0), (T::Parameter, m(&[M::Declaration])));
    assert_eq!(find(s, 90, "print", 0), (T::Function, m(&[])));
}

#[test]
fn includes_lexical_tokens() {
    assert!(semantic_tokens(SRC).iter().any(|t| t.line == 2 && t.ty == T::Comment));
    assert_eq!(find(SRC, 7, "enum", 0).0, T::Keyword);
    assert_eq!(find(SRC, 7, "2", 0).0, T::Number);
    assert_eq!(find(SRC, 69, "\"red\"", 0).0, T::String);
}

#[test]
fn still_highlights_unparsable_code() {
    let src = "class A {\n  void f(int x {\n    x++;\n";
    assert!(crate::parse(src).is_err());
    let toks = semantic_tokens(src);
    let at = |l, c| toks.iter().find(|t| t.line == l && t.start_col == c).map(|t| t.ty);
    assert_eq!(at(1, 7), Some(T::Class));
    assert_eq!(at(2, 8), Some(T::Method));
}

/// Every occurrence of `name` in `src` as (line, kind, declaration, scope).
fn occ(src: &str, name: &str) -> Vec<(usize, K, bool, S)> {
    symbols(src)
        .into_iter()
        .filter(|o| o.name == name)
        .map(|o| (o.line, o.kind, o.declaration, o.scope))
        .collect()
}

const CLASSES: &str = "\
namespace Game {
    enum Color { Red, Green }
    class Unit {
        int hp;
        Unit(int start) { hp = start; }
        void damage(int amount) {
            hp -= amount;
            heal(1);
            this.hp = hp;
        }
        void heal(int amount) { }
        int armor { get { return 0; } set { hp = value; } }
    }
}
void Spawn() {
    Game::Unit@ u = Game::Unit(10);
    u.damage(3);
    Game::Color c = Game::Red;
}
";

#[test]
fn types_and_namespaces_are_project_wide() {
    let p = S::Project;
    assert_eq!(occ(CLASSES, "Game")[0], (1, K::Namespace, true, p));
    assert!(occ(CLASSES, "Game").iter().skip(1).all(|o| *o == (o.0, K::Namespace, false, p)));
    assert_eq!(occ(CLASSES, "Color"), vec![(2, K::Enum, true, p), (18, K::Enum, false, p)]);
    assert_eq!(occ(CLASSES, "Red"), vec![(2, K::EnumMember, true, p), (18, K::EnumMember, false, p)]);
    assert_eq!(occ(CLASSES, "Spawn"), vec![(15, K::Function, true, p)]);
    let unit = occ(CLASSES, "Unit");
    assert_eq!(unit[0], (3, K::Class, true, p));
    // The constructor is a member named like the class.
    assert_eq!(unit[1], (5, K::Method, true, S::Member));
    assert!(unit.iter().skip(2).all(|o| o.1 == K::Class && !o.2 && o.3 == p));
}

#[test]
fn members_with_and_without_this_are_members() {
    let m = S::Member;
    assert_eq!(
        occ(CLASSES, "hp"),
        vec![
            (4, K::Field, true, m),
            (5, K::Field, false, m),  // implicit this, in the constructor
            (7, K::Field, false, m),  // ... and in a method
            (9, K::Field, false, m),  // this.hp
            (9, K::Field, false, m),  // = hp
            (12, K::Field, false, m), // in a property setter
        ]
    );
    assert_eq!(occ(CLASSES, "heal"), vec![(8, K::Method, false, m), (11, K::Method, true, m)]);
    assert_eq!(occ(CLASSES, "damage"), vec![(6, K::Method, true, m), (17, K::Method, false, m)]);
    assert_eq!(occ(CLASSES, "armor"), vec![(12, K::Field, true, m)]);
}

#[test]
fn parameters_and_locals_belong_to_their_function() {
    // damage's `amount` and heal's `amount` are different symbols.
    let amount = occ(CLASSES, "amount");
    assert_eq!(amount.len(), 3);
    assert_eq!((amount[0].2, amount[1].2), (true, false));
    assert_eq!(amount[0].3, amount[1].3);
    assert_ne!(amount[0].3, amount[2].3);
    // The setter's implicit `value`: a parameter with no declaration.
    let value = occ(CLASSES, "value");
    assert_eq!(value.len(), 1);
    assert!(matches!(value[0], (12, K::Parameter, false, S::Local(_))));
    // A local in a free function.
    let u = occ(CLASSES, "u");
    assert!(matches!(u[0], (16, K::Local, true, S::Local(_))));
    assert_eq!(u[0].3, u[1].3);
}

#[test]
fn a_name_that_isnt_a_member_stays_global() {
    let src = "int count;\nclass C { void f() { count++; g(); } }\nvoid g() { }";
    assert_eq!(occ(src, "count"), vec![(1, K::Global, true, S::Project), (2, K::Global, false, S::Project)]);
    assert_eq!(occ(src, "g"), vec![(2, K::Function, false, S::Project), (3, K::Function, true, S::Project)]);
}

#[test]
fn lambdas_funcdefs_and_constants() {
    let src = "funcdef void Cb(int);\nconst int LIMIT = 5;\nvoid f() { Cb@ cb = function(n) { print(n + LIMIT); }; }";
    assert_eq!(occ(src, "Cb"), vec![(1, K::Type, true, S::Project), (3, K::Type, false, S::Project)]);
    assert_eq!(occ(src, "LIMIT"), vec![(2, K::Constant, true, S::Project), (3, K::Constant, false, S::Project)]);
    let n = occ(src, "n");
    assert!(matches!(n[0], (3, K::Parameter, true, S::Local(_))));
    assert_eq!(n[0].3, n[1].3);
}

/// (namespace, qualifier, declaration) of every occurrence of `name`.
fn places(src: &str, name: &str) -> Vec<(String, Option<String>, bool)> {
    symbols(src)
        .into_iter()
        .filter(|o| o.name == name)
        .map(|o| (o.namespace, o.qualifier, o.declaration))
        .collect()
}

fn place(ns: &str, qualifier: Option<&str>, decl: bool) -> (String, Option<String>, bool) {
    (ns.into(), qualifier.map(String::from), decl)
}

#[test]
fn records_namespaces_and_qualifiers() {
    let src = "\
namespace A {
    namespace B { class C { } }
    void f() { B::C@ c; A::B::C@ d; ::A::B::C@ e; ::g(); }
}
void g() { A::B::C@ x; }
";
    assert_eq!(
        places(src, "C"),
        vec![
            place("A::B", None, true),
            place("A", Some("B"), false),
            place("A", Some("A::B"), false),
            place("A", Some("::A::B"), false),
            place("", Some("A::B"), false),
        ]
    );
    // Namespaces are names in their parent namespace.
    assert_eq!(places(src, "B")[0], place("A", None, true));
    assert_eq!(places(src, "g"), vec![place("A", Some("::"), false), place("", None, true)]);
}

#[test]
fn enum_names_in_qualifiers_arent_namespaces() {
    let src = "namespace N { enum Color { Red } }\nvoid f() { N::Color c = N::Color::Red; int r = N::Red; }";
    assert_eq!(
        places(src, "Red"),
        vec![place("N", None, true), place("", Some("N"), false), place("", Some("N"), false)]
    );
}

#[test]
fn members_and_locals_have_no_namespace() {
    let src = "namespace N { class K { int hp; void f(int a) { hp = a; } } }";
    for o in symbols(src).into_iter().filter(|o| o.scope != S::Project) {
        assert_eq!((o.namespace.as_str(), o.qualifier), ("", None), "{}", o.name);
    }
}

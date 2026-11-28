use super::*;
use parser_rs::apply_edits;

fn fmt(src: &str) -> String {
    apply_edits(src, &format(src, "\t", 1..=usize::MAX))
}

#[test]
fn indents_classes_and_switches() {
    let src = "\
namespace N {
class A : B
{
int x;
void f(int k)
{
switch (k)
{
case 1:
x = 1;
break;
default:
{
x = 2;
}
}
}
}
}
";
    let want = "\
namespace N {
\tclass A : B
\t{
\t\tint x;
\t\tvoid f(int k)
\t\t{
\t\t\tswitch (k)
\t\t\t{
\t\t\tcase 1:
\t\t\t\tx = 1;
\t\t\t\tbreak;
\t\t\tdefault:
\t\t\t{
\t\t\t\tx = 2;
\t\t\t}
\t\t\t}
\t\t}
\t}
}
";
    assert_eq!(fmt(src), want);
}

#[test]
fn a_heredoc_stays() {
    let src = "void f() {\nstring s = \"\"\"\n  a\n\"\"\";\n}\n";
    assert_eq!(fmt(src), "void f() {\n\tstring s = \"\"\"\n  a\n\"\"\";\n}\n");
}

use crate::ast::*;
pub use parser_rs::Node;

/// Builds a visualizable tree out of a parsed script.
pub fn build_tree(script: &Script) -> Node {
    Node::with_children("Script", script.items.iter().map(item_node).collect()).tag("root", None)
}

/// Renders the script's AST as an indented tree.
pub fn render_tree(script: &Script) -> String {
    parser_rs::render_tree(&build_tree(script))
}

/// Renders the script's AST as a Graphviz DOT graph.
pub fn render_dot(script: &Script) -> String {
    parser_rs::render_dot(&build_tree(script))
}

fn line_span(line: usize) -> Option<Span> {
    Some(Span::whole_line(line))
}

fn mods(m: &Modifiers) -> String {
    let mut s = String::new();
    for (on, word) in [
        (m.private, "private "),
        (m.protected, "protected "),
        (m.shared, "shared "),
        (m.external, "external "),
        (m.is_abstract, "abstract "),
    ] {
        if on {
            s.push_str(word);
        }
    }
    s
}

fn tail(m: &Modifiers) -> String {
    let mut s = String::new();
    for (on, word) in [
        (m.const_method, " const"),
        (m.is_final, " final"),
        (m.is_override, " override"),
        (m.property, " property"),
        (m.explicit, " explicit"),
    ] {
        if on {
            s.push_str(word);
        }
    }
    s
}

fn params(ps: &[Param]) -> String {
    ps.iter()
        .map(|p| {
            let mut s = p.ty.display();
            s.push_str(match p.by_ref {
                None => "",
                Some(RefKind::Ref) => " &",
                Some(RefKind::In) => " &in",
                Some(RefKind::Out) => " &out",
                Some(RefKind::InOut) => " &inout",
            });
            if let Some(n) = &p.name {
                s.push(' ');
                s.push_str(&n.name);
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn function_node(keyword: &str, f: &FunctionDecl) -> Node {
    let ret = match &f.ret {
        Some(r) => format!("{}{} ", r.display(), if f.ret_ref { "&" } else { "" }),
        None => String::new(),
    };
    let tilde = if f.destructor { "~" } else { "" };
    let label = format!(
        "{keyword} {}{ret}{tilde}{}({}){} (line {})",
        mods(&f.modifiers),
        f.name.name,
        params(&f.params),
        tail(&f.modifiers),
        f.line
    );
    let children = f.body.iter().flatten().map(stmt_node).collect();
    Node::with_children(label, children).tag("sig", line_span(f.line))
}

fn var_node(keyword: &str, v: &VarDecl) -> Node {
    let children = v.exprs().into_iter().map(expr_node).collect();
    Node::with_children(
        format!("{keyword} {}{} {} (line {})", mods(&v.modifiers), v.ty.display(), v.name.name, v.line),
        children,
    )
    .tag(if keyword == "Local" { "stmt" } else { "decl" }, line_span(v.line))
}

fn property_node(prop: &PropertyDecl) -> Node {
    let children = prop
        .accessors
        .iter()
        .map(|a| {
            let kw = match a.kind {
                AccessorKind::Get => "get",
                AccessorKind::Set => "set",
            };
            let c = if a.is_const { " const" } else { "" };
            Node::with_children(format!("{kw}{c}"), a.body.iter().flatten().map(stmt_node).collect())
                .tag("sig", line_span(a.line))
        })
        .collect();
    Node::with_children(
        format!("Property {}{} {} (line {})", mods(&prop.modifiers), prop.ty.display(), prop.name.name, prop.line),
        children,
    )
    .tag("decl", line_span(prop.line))
}

fn funcdef_label(f: &FuncdefDecl) -> String {
    format!("Funcdef {} {}({}) (line {})", f.ret.display(), f.name.name, params(&f.params), f.line)
}

fn item_node(item: &Item) -> Node {
    match item {
        Item::Namespace { name, items, line } => Node::with_children(
            format!("Namespace {} (line {line})", name.name),
            items.iter().map(item_node).collect(),
        )
        .tag("decl", line_span(*line)),
        Item::Class(c) => {
            let mut label = format!("{}Class {}", mods(&c.modifiers), c.name.name);
            if c.mixin {
                label = format!("Mixin {label}");
            }
            if c.modifiers.is_final {
                label.push_str(" final");
            }
            if !c.bases.is_empty() {
                let bases: Vec<&str> = c.bases.iter().map(|b| b.name.as_str()).collect();
                label.push_str(&format!(" : {}", bases.join(", ")));
            }
            label.push_str(&format!(" (line {})", c.line));
            let children = c
                .members
                .iter()
                .flatten()
                .map(|m| match m {
                    Member::Var(v) => var_node("Field", v),
                    Member::Method(f) => function_node("Method", f),
                    Member::Property(prop) => property_node(prop),
                    Member::Funcdef(f) => Node::leaf(funcdef_label(f)).tag("decl", line_span(f.line)),
                })
                .collect();
            Node::with_children(label, children).tag("decl", line_span(c.line))
        }
        Item::Interface(i) => Node::with_children(
            format!("Interface {} (line {})", i.name.name, i.line),
            i.methods.iter().map(|f| function_node("Method", f)).collect(),
        )
        .tag("decl", line_span(i.line)),
        Item::Enum(e) => Node::with_children(
            format!("Enum {} (line {})", e.name.name, e.line),
            e.values
                .iter()
                .map(|(n, v)| {
                    Node::with_children(n.name.clone(), v.iter().map(expr_node).collect()).tag("decl", Some(n.span))
                })
                .collect(),
        )
        .tag("decl", line_span(e.line)),
        Item::Funcdef(f) => Node::leaf(funcdef_label(f)).tag("decl", line_span(f.line)),
        Item::Typedef { base, name, line } => {
            Node::leaf(format!("Typedef {} {} (line {line})", base.display(), name.name)).tag("decl", line_span(*line))
        }
        Item::Import { func, from, line } => {
            let mut node = function_node("Import", func);
            node.label = format!("{} from {from:?}", node.label);
            node.span = line_span(*line);
            node
        }
        Item::Function(f) => function_node("Function", f),
        Item::Var(v) => var_node("Global", v),
        Item::Property(prop) => property_node(prop),
    }
}

fn container(label: &str, line: usize, children: Vec<Node>) -> Node {
    Node::with_children(label, children).tag("container", line_span(line))
}

fn stmts(body: &[Stmt]) -> Vec<Node> {
    body.iter().map(stmt_node).collect()
}

fn stmt_node(s: &Stmt) -> Node {
    let line = s.line();
    let (label, children) = match s {
        Stmt::Var(v) => return var_node("Local", v),
        Stmt::Expr { expr, .. } => ("Expr".to_string(), vec![expr_node(expr)]),
        Stmt::Block { body, .. } => ("Block".to_string(), stmts(body)),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            let mut c = vec![
                container("Condition", line, vec![expr_node(cond)]),
                container("Then", line, stmts(then_body)),
            ];
            if let Some(b) = else_body {
                c.push(container("Else", line, stmts(b)));
            }
            ("If".to_string(), c)
        }
        Stmt::While { cond, body, .. } => (
            "While".to_string(),
            vec![container("Condition", line, vec![expr_node(cond)]), container("Body", line, stmts(body))],
        ),
        Stmt::DoWhile { body, cond, .. } => (
            "DoWhile".to_string(),
            vec![container("Body", line, stmts(body)), container("Condition", line, vec![expr_node(cond)])],
        ),
        Stmt::For {
            init,
            cond,
            incr,
            body,
            ..
        } => (
            "For".to_string(),
            vec![
                container("Init", line, stmts(init)),
                container("Condition", line, cond.iter().map(expr_node).collect()),
                container("Increment", line, incr.iter().map(expr_node).collect()),
                container("Body", line, stmts(body)),
            ],
        ),
        Stmt::Switch { value, cases, .. } => {
            let mut c = vec![expr_node(value)];
            for case in cases {
                let labels: Vec<Node> = case
                    .labels
                    .iter()
                    .map(|l| match l {
                        Some(e) => expr_node(e),
                        None => Node::leaf("default").tag("container", line_span(case.line)),
                    })
                    .collect();
                c.push(container("Case", case.line, labels.into_iter().chain(stmts(&case.body)).collect()));
            }
            ("Switch".to_string(), c)
        }
        Stmt::Break { .. } => ("Break".to_string(), Vec::new()),
        Stmt::Continue { .. } => ("Continue".to_string(), Vec::new()),
        Stmt::Return { value, .. } => ("Return".to_string(), value.iter().map(expr_node).collect()),
        Stmt::Try { body, catch, .. } => (
            "Try".to_string(),
            vec![container("Body", line, stmts(body)), container("Catch", line, stmts(catch))],
        ),
    };
    Node::with_children(format!("{label} (line {line})"), children).tag("stmt", line_span(line))
}

fn expr_node(e: &Expr) -> Node {
    let span = Some(e.span);
    let (label, kind, children) = match &e.kind {
        ExprKind::Int(v) => (format!("Int {v}"), "lit", Vec::new()),
        ExprKind::Real(v) => (format!("Real {v}"), "lit", Vec::new()),
        ExprKind::Str(s) => (format!("String {s:?}"), "lit", Vec::new()),
        ExprKind::Bool(b) => (format!("Bool {b}"), "lit", Vec::new()),
        ExprKind::Null => ("Null".to_string(), "lit", Vec::new()),
        ExprKind::Name(..) => (format!("Name {}", e.path().unwrap_or_default()), "expr", Vec::new()),
        ExprKind::Member(obj, name) => (format!("Member {name}"), "expr", vec![expr_node(obj)]),
        ExprKind::Index(obj, args) => (
            "Index".to_string(),
            "expr",
            std::iter::once(expr_node(obj)).chain(args.iter().map(expr_node)).collect(),
        ),
        ExprKind::Call(f, args) => match f.path() {
            Some(path) => (format!("Call {path}"), "expr", args.iter().map(expr_node).collect()),
            None => (
                "Call".to_string(),
                "expr",
                std::iter::once(expr_node(f)).chain(args.iter().map(expr_node)).collect(),
            ),
        },
        ExprKind::Construct(ty, args) => (format!("Construct {}", ty.display()), "expr", args.iter().map(expr_node).collect()),
        ExprKind::Cast(ty, inner) => (format!("Cast<{}>", ty.display()), "expr", vec![expr_node(inner)]),
        ExprKind::Lambda(f) => {
            let mut node = function_node("Lambda", f);
            node.span = span;
            return node;
        }
        ExprKind::InitList(items) => ("InitList".to_string(), "expr", items.iter().map(expr_node).collect()),
        ExprKind::Unary(op, inner) => (format!("Unary {op:?}"), "expr", vec![expr_node(inner)]),
        ExprKind::Postfix(inner, op) => (format!("Postfix {op:?}"), "expr", vec![expr_node(inner)]),
        ExprKind::Binary(l, op, r) => (format!("Binary {op:?}"), "expr", vec![expr_node(l), expr_node(r)]),
        ExprKind::Assign(l, op, r) => (format!("Assign {op:?}"), "expr", vec![expr_node(l), expr_node(r)]),
        ExprKind::Ternary(c, a, b) => ("Ternary".to_string(), "expr", vec![expr_node(c), expr_node(a), expr_node(b)]),
    };
    Node::with_children(label, children).tag(kind, span)
}

#[cfg(test)]
#[path = "printer.test.rs"]
mod tests;

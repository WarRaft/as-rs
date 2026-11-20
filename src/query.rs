//! Position-based lookup over a parsed [`Script`]: "which AST node is at
//! (or covers) this location". Coordinates are [`Span`]'s (line,
//! codepoint column).

use crate::ast::*;

/// A reference to whichever kind of AST node a query matched.
#[derive(Debug, Clone, Copy)]
pub enum AstRef<'a> {
    Item(&'a Item),
    Member(&'a Member),
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

impl AstRef<'_> {
    /// Declarations and statements carry only their starting line;
    /// expressions an exact span.
    pub fn span(&self) -> Span {
        match self {
            AstRef::Item(i) => Span::whole_line(i.line()),
            AstRef::Member(m) => Span::whole_line(match m {
                Member::Var(v) => v.line,
                Member::Method(f) => f.line,
                Member::Property(p) => p.line,
                Member::Funcdef(f) => f.line,
            }),
            AstRef::Stmt(s) => Span::whole_line(s.line()),
            AstRef::Expr(e) => e.span,
        }
    }
}

#[derive(Clone, Copy)]
struct Q {
    line: usize,
    sc: usize,
    ec: usize,
}

impl Q {
    fn hit<'a>(self, node: AstRef<'a>) -> Option<AstRef<'a>> {
        node.span().contains(self.line, self.sc, self.ec).then_some(node)
    }
}

/// Finds the most specific (deepest) node whose span contains the
/// single-line range `[start_col, end_col)` on `line`.
pub fn find_node_in(script: &Script, line: usize, start_col: usize, end_col: usize) -> Option<AstRef<'_>> {
    let q = Q {
        line,
        sc: start_col,
        ec: end_col,
    };
    in_items(&script.items, q)
}

/// Finds the most specific node at a single position.
pub fn find_node_at(script: &Script, line: usize, col: usize) -> Option<AstRef<'_>> {
    find_node_in(script, line, col, col)
}

fn in_items(items: &[Item], q: Q) -> Option<AstRef<'_>> {
    items.iter().find_map(|i| in_item(i, q))
}

fn in_item(item: &Item, q: Q) -> Option<AstRef<'_>> {
    let child = match item {
        Item::Namespace { items, .. } => in_items(items, q),
        Item::Class(c) => c.members.iter().flatten().find_map(|m| in_member(m, q)),
        Item::Interface(i) => i.methods.iter().find_map(|f| in_function(f, q)),
        Item::Enum(e) => e.values.iter().filter_map(|(_, v)| v.as_ref()).find_map(|v| in_expr(v, q)),
        Item::Function(f) => in_function(f, q),
        Item::Var(v) => v.exprs().into_iter().find_map(|e| in_expr(e, q)),
        Item::Property(p) => in_property(p, q),
        Item::Funcdef(_) | Item::Typedef { .. } | Item::Import { .. } => None,
    };
    child.or_else(|| q.hit(AstRef::Item(item)))
}

fn in_member(m: &Member, q: Q) -> Option<AstRef<'_>> {
    let child = match m {
        Member::Var(v) => v.exprs().into_iter().find_map(|e| in_expr(e, q)),
        Member::Method(f) => in_function(f, q),
        Member::Property(p) => in_property(p, q),
        Member::Funcdef(_) => None,
    };
    child.or_else(|| q.hit(AstRef::Member(m)))
}

fn in_property(p: &PropertyDecl, q: Q) -> Option<AstRef<'_>> {
    p.accessors.iter().find_map(|a| in_stmts(a.body.as_deref().unwrap_or(&[]), q))
}

fn in_function(f: &FunctionDecl, q: Q) -> Option<AstRef<'_>> {
    f.params
        .iter()
        .filter_map(|p| p.default.as_ref())
        .find_map(|e| in_expr(e, q))
        .or_else(|| in_stmts(f.body.as_deref().unwrap_or(&[]), q))
}

fn in_stmts(stmts: &[Stmt], q: Q) -> Option<AstRef<'_>> {
    stmts.iter().find_map(|s| in_stmt(s, q))
}

fn in_stmt(s: &Stmt, q: Q) -> Option<AstRef<'_>> {
    s.exprs()
        .into_iter()
        .find_map(|e| in_expr(e, q))
        .or_else(|| s.bodies().into_iter().find_map(|b| in_stmts(b, q)))
        .or_else(|| q.hit(AstRef::Stmt(s)))
}

fn in_expr(e: &Expr, q: Q) -> Option<AstRef<'_>> {
    let child = match &e.kind {
        ExprKind::Lambda(f) => in_function(f, q),
        _ => e.children().into_iter().find_map(|c| in_expr(c, q)),
    };
    child.or_else(|| q.hit(AstRef::Expr(e)))
}

#[cfg(test)]
#[path = "query.test.rs"]
mod tests;

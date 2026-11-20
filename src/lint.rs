//! Lint rules for AngelScript, mirroring the compiler's own errors and
//! warnings where it can tell them without type information.

use crate::ast::*;
use std::collections::HashMap;

pub use parser_rs::{Diagnostic, Level};
use parser_rs::rule;

/// Runs every lint rule over a script and returns the diagnostics found,
/// sorted by source line.
pub fn lint(script: &Script) -> Vec<Diagnostic> {
    let mut l = Linter::default();
    l.items(&script.items);
    l.diags.sort_by_key(|d| d.line);
    l.diags
}

struct Local {
    name: String,
    line: usize,
    used: bool,
    /// Parameters are never reported as unused.
    param: bool,
}

#[derive(Default)]
struct Linter {
    diags: Vec<Diagnostic>,
    locals: Vec<Local>,
    blocks: Vec<usize>,
    /// Open loops and switches of the current function, innermost last
    /// (`true` = a loop). `break` needs either; `continue` needs a loop.
    breakables: Vec<bool>,
}

fn duplicates<'a>(what: &str, names: impl Iterator<Item = (&'a str, usize)>, diags: &mut Vec<Diagnostic>) {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (name, line) in names {
        if let Some(&first) = seen.get(name) {
            diags.push(Diagnostic::error(rule::DUPLICATE_DECLARATION, 
                format!("duplicate {what} '{name}' (first declared at line {first})"),
                line,
            ));
        } else {
            seen.insert(name, line);
        }
    }
}

impl Linter {
    fn items(&mut self, items: &[Item]) {
        // functions may be overloaded; every other name must be unique
        duplicates(
            "declaration",
            items.iter().filter_map(|i| match i {
                Item::Class(c) => Some((c.name.name.as_str(), c.line)),
                Item::Interface(i) => Some((i.name.name.as_str(), i.line)),
                Item::Enum(e) => Some((e.name.name.as_str(), e.line)),
                Item::Funcdef(f) => Some((f.name.name.as_str(), f.line)),
                Item::Typedef { name, line, .. } => Some((name.name.as_str(), *line)),
                Item::Var(v) => Some((v.name.name.as_str(), v.line)),
                Item::Property(prop) => Some((prop.name.name.as_str(), prop.line)),
                _ => None,
            }),
            &mut self.diags,
        );
        for item in items {
            match item {
                Item::Namespace { items, .. } => self.items(items),
                Item::Class(c) => self.class(c),
                Item::Interface(_) | Item::Funcdef(_) | Item::Typedef { .. } | Item::Import { .. } => {}
                Item::Enum(e) => {
                    duplicates(
                        "enum value",
                        e.values.iter().map(|(n, _)| (n.name.as_str(), n.span.start_line)),
                        &mut self.diags,
                    );
                    for (_, value) in &e.values {
                        self.exprs(value.iter());
                    }
                }
                Item::Function(f) => self.function(f),
                Item::Var(v) => self.exprs(v.exprs().into_iter()),
                Item::Property(prop) => self.property(prop),
            }
        }
    }

    fn class(&mut self, c: &ClassDecl) {
        let Some(members) = &c.members else {
            return;
        };
        duplicates(
            "class member",
            members.iter().filter_map(|m| match m {
                Member::Var(v) => Some((v.name.name.as_str(), v.line)),
                Member::Property(prop) => Some((prop.name.name.as_str(), prop.line)),
                _ => None,
            }),
            &mut self.diags,
        );
        for m in members {
            match m {
                Member::Var(v) => self.exprs(v.exprs().into_iter()),
                Member::Method(f) => self.function(f),
                Member::Property(prop) => self.property(prop),
                Member::Funcdef(_) => {}
            }
        }
    }

    fn property(&mut self, prop: &PropertyDecl) {
        for acc in &prop.accessors {
            let Some(body) = &acc.body else {
                continue;
            };
            let params: Vec<(String, usize)> = match acc.kind {
                AccessorKind::Set => vec![("value".to_string(), acc.line)],
                AccessorKind::Get => Vec::new(),
            };
            self.body(&params, body);
            if acc.kind == AccessorKind::Get && !always_returns(body) {
                self.diags.push(Diagnostic::error(rule::MISSING_RETURN, 
                    format!("not all paths of 'get' for '{}' return a value", prop.name.name),
                    acc.line,
                ));
            }
        }
    }

    fn function(&mut self, f: &FunctionDecl) {
        let Some(body) = &f.body else {
            return;
        };
        let params: Vec<(String, usize)> = f
            .params
            .iter()
            .filter_map(|p| p.name.as_ref().map(|n| (n.name.clone(), f.line)))
            .collect();
        for p in &f.params {
            self.exprs(p.default.iter());
        }
        self.body(&params, body);
        let returns_value = f.ret.as_ref().is_some_and(|r| !r.is_void());
        if returns_value && !always_returns(body) {
            self.diags.push(Diagnostic::error(rule::MISSING_RETURN, 
                format!("not all paths of '{}' return a value", f.name.name),
                f.line,
            ));
        }
    }

    /// A function body, with its own locals and loops.
    fn body(&mut self, params: &[(String, usize)], body: &[Stmt]) {
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_blocks = std::mem::take(&mut self.blocks);
        let saved_loops = std::mem::take(&mut self.breakables);
        // parameters live in a scope around the body's, so a local of the
        // same name hides one rather than redeclaring it
        self.open();
        for (name, line) in params {
            self.locals.push(Local {
                name: name.clone(),
                line: *line,
                used: false,
                param: true,
            });
        }
        self.scoped(body);
        self.close();
        self.locals = saved_locals;
        self.blocks = saved_blocks;
        self.breakables = saved_loops;
    }

    fn open(&mut self) {
        self.blocks.push(self.locals.len());
    }

    fn close(&mut self) {
        let start = self.blocks.pop().unwrap_or(0);
        for l in self.locals.drain(start..) {
            if !l.used && !l.param && !l.name.starts_with('_') {
                self.diags
                    .push(Diagnostic::warning(rule::UNUSED_LOCAL, format!("unused variable '{}'", l.name), l.line));
            }
        }
    }

    fn declare(&mut self, name: &str, line: usize) {
        let block_start = self.blocks.last().copied().unwrap_or(0);
        if let Some(i) = self.locals.iter().rposition(|l| l.name == name) {
            if i >= block_start {
                self.diags.push(Diagnostic::error(rule::DUPLICATE_DECLARATION, 
                    format!("'{name}' is already declared in this scope (line {})", self.locals[i].line),
                    line,
                ));
            } else {
                self.diags.push(Diagnostic::warning(rule::SHADOWED_VARIABLE, 
                    format!("'{name}' hides a variable of the same name in an outer scope"),
                    line,
                ));
            }
        }
        self.locals.push(Local {
            name: name.to_string(),
            line,
            used: false,
            param: false,
        });
    }

    fn block(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn scoped(&mut self, stmts: &[Stmt]) {
        self.open();
        self.block(stmts);
        self.close();
    }

    fn breakable(&mut self, is_loop: bool, f: impl FnOnce(&mut Self)) {
        self.breakables.push(is_loop);
        f(self);
        self.breakables.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Var(v) => {
                self.exprs(v.exprs().into_iter());
                self.declare(&v.name.name, v.line);
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Block { body, .. } => self.scoped(body),
            Stmt::If {
                cond,
                then_body,
                else_body,
                line,
            } => {
                self.expr(cond);
                if then_body.is_empty() {
                    self.diags.push(Diagnostic::warning(rule::EMPTY_BRANCH, "empty 'if' body", *line));
                }
                self.scoped(then_body);
                if let Some(body) = else_body {
                    self.scoped(body);
                }
            }
            Stmt::While { cond, body, line } | Stmt::DoWhile { body, cond, line } => {
                self.expr(cond);
                if is_true(cond) && !exits_loop(body) {
                    self.diags.push(Diagnostic::warning(rule::INFINITE_LOOP, 
                        "loop condition is always true and the body has no 'break' or 'return'",
                        *line,
                    ));
                }
                self.breakable(true, |l| l.scoped(body));
            }
            Stmt::For {
                init,
                cond,
                incr,
                body,
                line,
            } => {
                self.open();
                self.block(init);
                self.exprs(cond.iter());
                self.exprs(incr.iter());
                if cond.as_ref().is_none_or(is_true) && !exits_loop(body) {
                    self.diags.push(Diagnostic::warning(rule::INFINITE_LOOP, 
                        "loop has no condition and the body has no 'break' or 'return'",
                        *line,
                    ));
                }
                self.breakable(true, |l| l.scoped(body));
                self.close();
            }
            Stmt::Switch { value, cases, .. } => {
                self.expr(value);
                self.breakable(false, |l| {
                    l.open();
                    for c in cases {
                        l.exprs(c.labels.iter().flatten());
                        l.block(&c.body);
                    }
                    l.close();
                });
            }
            Stmt::Break { line } => {
                if self.breakables.is_empty() {
                    self.diags
                        .push(Diagnostic::error(rule::BREAK_OUTSIDE_LOOP, "'break' outside a loop or switch", *line));
                }
            }
            Stmt::Continue { line } => {
                if !self.breakables.contains(&true) {
                    self.diags.push(Diagnostic::error(rule::CONTINUE_OUTSIDE_LOOP, "'continue' outside a loop", *line));
                }
            }
            Stmt::Return { value, .. } => self.exprs(value.iter()),
            Stmt::Try { body, catch, .. } => {
                self.scoped(body);
                self.scoped(catch);
            }
        }
    }

    fn exprs<'a>(&mut self, exprs: impl Iterator<Item = &'a Expr>) {
        for e in exprs {
            self.expr(e);
        }
    }

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Name(scope, name) if scope.is_empty() => {
                if let Some(l) = self.locals.iter_mut().rev().find(|l| &l.name == name) {
                    l.used = true;
                }
            }
            // a plain `x = v` writes `x` without reading it
            ExprKind::Assign(target, AssignOp::Assign, value) if matches!(target.kind, ExprKind::Name(..)) => {
                self.expr(value);
                return;
            }
            ExprKind::Lambda(f) => {
                self.function(f);
                return;
            }
            _ => {}
        }
        for child in e.children() {
            self.expr(child);
        }
    }
}

fn is_true(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Bool(true))
}

/// Whether `stmts`, run to the end, always leave via `return` (or never
/// finish: an endless loop).
fn always_returns(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match s {
        Stmt::Return { .. } => true,
        Stmt::Block { body, .. } => always_returns(body),
        Stmt::If {
            then_body,
            else_body: Some(else_body),
            ..
        } => always_returns(then_body) && always_returns(else_body),
        Stmt::While { cond, body, .. } | Stmt::DoWhile { cond, body, .. } => is_true(cond) && !breaks(body),
        Stmt::For { cond, body, .. } => cond.as_ref().is_none_or(is_true) && !breaks(body),
        Stmt::Switch { cases, .. } => {
            cases.iter().any(|c| c.labels.contains(&None))
                && cases.iter().all(|c| c.body.is_empty() || always_returns(&c.body))
        }
        Stmt::Try { body, catch, .. } => always_returns(body) && always_returns(catch),
        _ => false,
    })
}

/// Whether a loop body contains a `break` for this loop.
fn breaks(body: &[Stmt]) -> bool {
    body.iter().any(|s| match s {
        Stmt::Break { .. } => true,
        Stmt::While { .. } | Stmt::DoWhile { .. } | Stmt::For { .. } | Stmt::Switch { .. } => false,
        _ => s.bodies().into_iter().any(breaks),
    })
}

/// Whether a loop body can leave the loop: its own `break`, or a `return`.
fn exits_loop(body: &[Stmt]) -> bool {
    breaks(body) || returns_anywhere(body)
}

fn returns_anywhere(body: &[Stmt]) -> bool {
    body.iter().any(|s| matches!(s, Stmt::Return { .. }) || s.bodies().into_iter().any(returns_anywhere))
}

#[cfg(test)]
#[path = "lint.test.rs"]
mod tests;

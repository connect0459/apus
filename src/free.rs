use crate::ast::{CaseLabel, Expr, ExprKind, Stmt, StmtKind};

pub fn names_in(stmts: &[Stmt]) -> Vec<String> {
    let mut found = Vec::new();
    statements(stmts, &mut found);
    found
}

fn note(name: &str, found: &mut Vec<String>) {
    if !found.iter().any(|n| n == name) {
        found.push(name.to_string());
    }
}

fn statements(stmts: &[Stmt], found: &mut Vec<String>) {
    for stmt in stmts {
        statement(stmt, found);
    }
}

fn optional_body(body: Option<&[Stmt]>, found: &mut Vec<String>) {
    if let Some(body) = body {
        statements(body, found);
    }
}

fn statement(stmt: &Stmt, found: &mut Vec<String>) {
    match stmt.kind() {
        StmtKind::Let { init, .. } => {
            if let Some(init) = init {
                expression(init, found);
            }
        }
        StmtKind::Assign { target, value, .. } => {
            expression(target, found);
            expression(value, found);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                expression(value, found);
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) => {}
        StmtKind::Labeled { stmt, .. } => statement(stmt, found),
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            expression(cond, found);
            statements(then_body, found);
            optional_body(else_body.as_deref(), found);
        }
        StmtKind::IfLet {
            value,
            then_body,
            else_body,
            ..
        } => {
            expression(value, found);
            statements(then_body, found);
            optional_body(else_body.as_deref(), found);
        }
        StmtKind::While { cond, body } => {
            expression(cond, found);
            statements(body, found);
        }
        StmtKind::WhileLet { value, body, .. } => {
            expression(value, found);
            statements(body, found);
        }
        StmtKind::Guard { cond, else_body } => {
            expression(cond, found);
            statements(else_body, found);
        }
        StmtKind::GuardLet {
            value, else_body, ..
        } => {
            expression(value, found);
            statements(else_body, found);
        }
        StmtKind::For { iter, body, .. } | StmtKind::ForPair { iter, body, .. } => {
            expression(iter, found);
            statements(body, found);
        }
        StmtKind::LocalFunc(func) => statements(func.body(), found),
        StmtKind::LetTuple { init, .. } => expression(init, found),
        StmtKind::Switch { subject, cases } => {
            expression(subject, found);
            for case in cases {
                if let CaseLabel::Values(values) = case.label() {
                    for value in values {
                        expression(value, found);
                    }
                }
                statements(case.body(), found);
            }
        }
        StmtKind::Expr(expr) => expression(expr, found),
    }
}

fn expression(expr: &Expr, found: &mut Vec<String>) {
    match expr.kind() {
        ExprKind::Ident(name) => note(name, found),
        ExprKind::Nil
        | ExprKind::ChainValue
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::GenericName { .. } => {}
        ExprKind::ArrayLiteral(items) | ExprKind::Tuple(items, _) => {
            items.iter().for_each(|e| expression(e, found))
        }
        ExprKind::DictLiteral(pairs) => {
            for (key, value) in pairs {
                expression(key, found);
                expression(value, found);
            }
        }
        ExprKind::Index { base, index } => {
            expression(base, found);
            expression(index, found);
        }
        ExprKind::IndexDefault {
            base,
            index,
            default,
        } => {
            expression(base, found);
            expression(index, found);
            expression(default, found);
        }
        ExprKind::Unwrap(inner)
        | ExprKind::Unary { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::Member { base: inner, .. } => expression(inner, found),
        ExprKind::Coalesce { lhs, rhs } | ExprKind::Binary { lhs, rhs, .. } => {
            expression(lhs, found);
            expression(rhs, found);
        }
        ExprKind::OptionalChain { subject, link } => {
            expression(subject, found);
            expression(link, found);
        }
        ExprKind::Range { lo, hi, .. } => {
            expression(lo, found);
            expression(hi, found);
        }
        ExprKind::Call { callee, args } => {
            note(callee, found);
            args.iter().for_each(|a| expression(a.value(), found));
        }
        ExprKind::GenericCall { callee, args, .. } => {
            note(callee, found);
            args.iter().for_each(|a| expression(a.value(), found));
        }
        ExprKind::TypeCall { args, .. }
        | ExprKind::ImplicitMember {
            args: Some(args), ..
        } => {
            args.iter().for_each(|a| expression(a.value(), found));
        }
        ExprKind::ImplicitMember { args: None, .. } => {}
        ExprKind::MemberCall { base, args, .. }
        | ExprKind::GenericMemberCall { base, args, .. } => {
            expression(base, found);
            args.iter().for_each(|a| expression(a.value(), found));
        }
        ExprKind::Apply { func, args } => {
            expression(func, found);
            args.iter().for_each(|a| expression(a.value(), found));
        }
        ExprKind::Closure(closure) => statements(closure.body(), found),
    }
}

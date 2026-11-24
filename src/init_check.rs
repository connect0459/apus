use crate::ast::{Expr, ExprKind, Stmt, StmtKind};

const SELF: &str = "self";

#[derive(Debug, Clone, PartialEq)]
pub struct InitViolation {
    line: u32,
    message: String,
}

impl InitViolation {
    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn check_initialization(
    body: &[Stmt],
    members: &[&str],
    init_line: u32,
) -> Result<(), InitViolation> {
    let mut pending: Vec<&str> = members.to_vec();
    for stmt in body {
        if pending.is_empty() {
            return Ok(());
        }
        if let Some(member) = plain_member_assignment(stmt) {
            pending.retain(|m| *m != member);
            continue;
        }
        if mentions_self(stmt) {
            return Err(InitViolation {
                line: stmt.line(),
                message: "'self' is used before every member is initialized".to_string(),
            });
        }
        if returns(stmt) {
            return Err(InitViolation {
                line: stmt.line(),
                message: "an initializer returns before every member is initialized".to_string(),
            });
        }
    }
    match pending.first() {
        None => Ok(()),
        Some(member) => Err(InitViolation {
            line: init_line,
            message: format!("the initializer does not initialize the member '{member}'"),
        }),
    }
}

fn plain_member_assignment(stmt: &Stmt) -> Option<&str> {
    let StmtKind::Assign {
        target,
        op: None,
        value,
    } = stmt.kind()
    else {
        return None;
    };
    let ExprKind::Member { base, name } = target.kind() else {
        return None;
    };
    let is_self = matches!(base.kind(), ExprKind::Ident(n) if n == SELF);
    (is_self && !expr_mentions_self(value)).then_some(name.as_str())
}

fn mentions_self(stmt: &Stmt) -> bool {
    let any_stmts = |stmts: &[Stmt]| stmts.iter().any(mentions_self);
    match stmt.kind() {
        StmtKind::Let { init, .. } => init.as_ref().is_some_and(expr_mentions_self),
        StmtKind::Assign { target, value, .. } => {
            expr_mentions_self(target) || expr_mentions_self(value)
        }
        StmtKind::Return(value) => value.as_ref().is_some_and(expr_mentions_self),
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_mentions_self(cond)
                || any_stmts(then_body)
                || else_body.as_deref().is_some_and(any_stmts)
        }
        StmtKind::While { cond, body } => expr_mentions_self(cond) || any_stmts(body),
        StmtKind::For { iter, body, .. } => expr_mentions_self(iter) || any_stmts(body),
        StmtKind::Switch { subject, cases } => {
            expr_mentions_self(subject) || cases.iter().any(|c| any_stmts(c.body()))
        }
        StmtKind::Expr(expr) => expr_mentions_self(expr),
    }
}

fn expr_mentions_self(expr: &Expr) -> bool {
    match expr.kind() {
        ExprKind::Ident(name) => name == SELF,
        ExprKind::Int(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::GenericName { .. } => {
            false
        }
        ExprKind::Unary { expr, .. } => expr_mentions_self(expr),
        ExprKind::Binary { lhs, rhs, .. } => expr_mentions_self(lhs) || expr_mentions_self(rhs),
        ExprKind::Range { lo, hi, .. } => expr_mentions_self(lo) || expr_mentions_self(hi),
        ExprKind::Call { args, .. } | ExprKind::GenericCall { args, .. } => {
            args.iter().any(|a| expr_mentions_self(a.value()))
        }
        ExprKind::Member { base, .. } => expr_mentions_self(base),
        ExprKind::MemberCall { base, args, .. } => {
            expr_mentions_self(base) || args.iter().any(|a| expr_mentions_self(a.value()))
        }
    }
}

fn returns(stmt: &Stmt) -> bool {
    let any_returns = |stmts: &[Stmt]| stmts.iter().any(returns);
    match stmt.kind() {
        StmtKind::Return(_) => true,
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => any_returns(then_body) || else_body.as_deref().is_some_and(any_returns),
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => any_returns(body),
        StmtKind::Switch { cases, .. } => cases.iter().any(|c| any_returns(c.body())),
        StmtKind::Let { .. } | StmtKind::Assign { .. } | StmtKind::Expr(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Decl;
    use crate::lexer::lex;
    use crate::parser::parse_item;

    fn verdict(members: &[&str], body: &str) -> Result<(), InitViolation> {
        let src = format!("init() {{\n{body}\n}}");
        let Decl::Func(init) = parse_item(&lex(&src).unwrap()).unwrap() else {
            panic!("expected an initializer");
        };
        check_initialization(init.body(), members, init.line())
    }

    #[test]
    fn assigning_every_member_at_the_top_level_is_enough() {
        assert!(verdict(&["a", "b"], "self.a = 1\nself.b = 2").is_ok());
    }

    #[test]
    fn the_order_of_the_assignments_does_not_matter() {
        assert!(verdict(&["a", "b"], "self.b = 2\nself.a = 1").is_ok());
    }

    #[test]
    fn a_type_without_members_needs_no_assignment() {
        assert!(verdict(&[], "").is_ok());
    }

    #[test]
    fn a_missing_member_is_named_in_the_message() {
        let violation = verdict(&["a", "b"], "self.a = 1").unwrap_err();
        assert!(violation.message().contains("'b'"));
        assert_eq!(violation.line(), 1);
    }

    #[test]
    fn statements_after_the_last_member_are_free() {
        assert!(verdict(&["a"], "self.a = 1\nlet copy = self.a\nreturn").is_ok());
    }

    #[test]
    fn a_statement_that_does_not_mention_self_may_come_first() {
        assert!(verdict(&["a"], "var n = 1\nn += 1\nself.a = n").is_ok());
    }

    #[test]
    fn using_self_in_a_value_before_the_last_member_is_rejected() {
        let violation = verdict(&["a", "b"], "self.a = 1\nself.b = self.a").unwrap_err();
        assert!(violation.message().contains("before"));
        assert_eq!(violation.line(), 3);
    }

    #[test]
    fn a_compound_assignment_counts_as_a_use() {
        assert!(verdict(&["a"], "self.a += 1\nself.a = 1").is_err());
    }

    #[test]
    fn a_nested_use_of_self_is_found() {
        assert!(
            verdict(
                &["a"],
                "if true { while true { print(self.a) } }\nself.a = 1"
            )
            .is_err()
        );
        assert!(verdict(&["a"], "for i in 0..<2 { print(self.a) }\nself.a = 1").is_err());
        assert!(
            verdict(
                &["a"],
                "switch 1 { case .x: print(self.a)\ndefault: print(2) }\nself.a = 1"
            )
            .is_err()
        );
        assert!(verdict(&["a"], "foo(1, self.a)\nself.a = 1").is_err());
        assert!(verdict(&["a"], "let x = -self.a\nself.a = 1").is_err());
        assert!(verdict(&["a"], "let x = self.m(1)\nself.a = 1").is_err());
        assert!(verdict(&["a"], "let x = Foo<Int>(self.a)\nself.a = 1").is_err());
        assert!(verdict(&["a"], "for i in 0..<self.a { }\nself.a = 1").is_err());
    }

    #[test]
    fn a_nested_return_before_the_last_member_is_rejected() {
        let violation = verdict(&["a"], "if true { return }\nself.a = 1").unwrap_err();
        assert!(violation.message().contains("returns"));
        assert!(verdict(&["a"], "while true { return }\nself.a = 1").is_err());
        assert!(verdict(&["a"], "switch 1 { default: return }\nself.a = 1").is_err());
    }

    #[test]
    fn an_assignment_inside_a_branch_does_not_count() {
        assert!(verdict(&["a"], "if true { self.a = 1 } else { self.a = 2 }").is_err());
    }
}

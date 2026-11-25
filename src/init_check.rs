use std::collections::HashSet;

use crate::ast::{CaseLabel, Expr, ExprKind, Stmt, StmtKind};

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

#[derive(Debug, Clone, Copy)]
pub struct InitScope<'a> {
    members: &'a [&'a str],
    methods: &'a [&'a str],
    params: &'a [&'a str],
    own: Option<&'a [&'a str]>,
}

impl<'a> InitScope<'a> {
    pub fn new(members: &'a [&'a str], methods: &'a [&'a str], params: &'a [&'a str]) -> Self {
        Self {
            members,
            methods,
            params,
            own: None,
        }
    }

    pub fn with_superclass(mut self, own: &'a [&'a str]) -> Self {
        self.own = Some(own);
        self
    }
}

struct Scan<'a> {
    scope: InitScope<'a>,
    locals: Vec<HashSet<String>>,
}

pub fn check_initialization(
    body: &[Stmt],
    scope: InitScope<'_>,
    init_line: u32,
) -> Result<(), InitViolation> {
    let mut scan = Scan {
        scope,
        locals: vec![scope.params.iter().map(|p| (*p).to_string()).collect()],
    };
    let mut pending: Vec<&str> = scope.own.unwrap_or(scope.members).to_vec();
    let mut super_pending = scope.own.is_some();
    for stmt in body {
        if pending.is_empty() && !super_pending {
            return Ok(());
        }
        if let Some(member) = scan.plain_member_assignment(stmt)
            && scope.own.is_none_or(|own| own.contains(&member))
        {
            pending.retain(|m| *m != member);
            continue;
        }
        if super_pending && scan.is_super_init(stmt) {
            if !pending.is_empty() {
                return Err(InitViolation {
                    line: stmt.line(),
                    message: "super.init is called before every member is initialized".to_string(),
                });
            }
            super_pending = false;
            continue;
        }
        let waiting_for = if pending.is_empty() {
            "super.init is called"
        } else {
            "every member is initialized"
        };
        if scan.stmt_uses_self(stmt) {
            return Err(InitViolation {
                line: stmt.line(),
                message: format!("'self' is used before {waiting_for}"),
            });
        }
        if returns(stmt) {
            return Err(InitViolation {
                line: stmt.line(),
                message: format!("an initializer returns before {waiting_for}"),
            });
        }
    }
    if let Some(member) = pending.first() {
        return Err(InitViolation {
            line: init_line,
            message: format!("the initializer does not initialize the member '{member}'"),
        });
    }
    if super_pending {
        return Err(InitViolation {
            line: init_line,
            message: "the initializer does not call super.init".to_string(),
        });
    }
    Ok(())
}

pub fn check_enum_initialization(
    body: &[Stmt],
    scope: InitScope<'_>,
    init_line: u32,
) -> Result<(), InitViolation> {
    let mut scan = Scan {
        scope,
        locals: vec![scope.params.iter().map(|p| (*p).to_string()).collect()],
    };
    if scan.assigns_self(body)? {
        Ok(())
    } else {
        Err(InitViolation {
            line: init_line,
            message: "the initializer does not assign 'self' on every path".to_string(),
        })
    }
}

impl Scan<'_> {
    fn is_super_init(&self, stmt: &Stmt) -> bool {
        matches!(
            stmt.kind(),
            StmtKind::Expr(expr) if matches!(
                expr.kind(),
                ExprKind::MemberCall { base, name, args }
                    if name == "init"
                        && matches!(base.kind(), ExprKind::Ident(n) if n == "super")
                        && !args.iter().any(|a| self.expr_uses_self(a.value()))
            )
        )
    }

    fn assigns_self(&mut self, stmts: &[Stmt]) -> Result<bool, InitViolation> {
        for stmt in stmts {
            if self.plain_self_assignment(stmt) || self.branches_assign_self(stmt)? {
                return Ok(true);
            }
            if self.stmt_uses_self(stmt) {
                return Err(InitViolation {
                    line: stmt.line(),
                    message: "'self' is used before it is assigned".to_string(),
                });
            }
            if returns(stmt) {
                return Err(InitViolation {
                    line: stmt.line(),
                    message: "an initializer returns before it assigns 'self'".to_string(),
                });
            }
        }
        Ok(false)
    }

    fn branches_assign_self(&mut self, stmt: &Stmt) -> Result<bool, InitViolation> {
        let StmtKind::If {
            cond,
            then_body,
            else_body: Some(else_body),
        } = stmt.kind()
        else {
            return Ok(false);
        };
        if self.expr_uses_self(cond) {
            return Ok(false);
        }
        self.locals.push(HashSet::new());
        let then_assigns = self.assigns_self(then_body);
        self.locals.pop();
        self.locals.push(HashSet::new());
        let else_assigns = self.assigns_self(else_body);
        self.locals.pop();
        Ok(then_assigns? && else_assigns?)
    }

    fn plain_self_assignment(&self, stmt: &Stmt) -> bool {
        matches!(
            stmt.kind(),
            StmtKind::Assign { target, op: None, value }
                if matches!(target.kind(), ExprKind::Ident(n) if n == SELF)
                    && !self.expr_uses_self(value)
        )
    }

    fn is_member(&self, name: &str) -> bool {
        self.scope.members.contains(&name) && !self.locals.iter().any(|set| set.contains(name))
    }

    fn declare(&mut self, name: &str) {
        if let Some(set) = self.locals.last_mut() {
            set.insert(name.to_string());
        }
    }

    fn plain_member_assignment<'s>(&self, stmt: &'s Stmt) -> Option<&'s str> {
        let StmtKind::Assign {
            target,
            op: None,
            value,
        } = stmt.kind()
        else {
            return None;
        };
        let name = match target.kind() {
            ExprKind::Member { base, name } if matches!(base.kind(), ExprKind::Ident(n) if n == SELF) => {
                name
            }
            ExprKind::Ident(name) if self.is_member(name) => name,
            _ => return None,
        };
        (!self.expr_uses_self(value)).then_some(name.as_str())
    }

    fn block_uses_self(&mut self, stmts: &[Stmt]) -> bool {
        self.locals.push(HashSet::new());
        let used = self.stmts_use_self(stmts);
        self.locals.pop();
        used
    }

    fn stmts_use_self(&mut self, stmts: &[Stmt]) -> bool {
        let mut used = false;
        for stmt in stmts {
            used |= self.stmt_uses_self(stmt);
        }
        used
    }

    fn stmt_uses_self(&mut self, stmt: &Stmt) -> bool {
        match stmt.kind() {
            StmtKind::Let { name, init, .. } => {
                let used = init.as_ref().is_some_and(|e| self.expr_uses_self(e));
                self.declare(name);
                used
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr_uses_self(target) || self.expr_uses_self(value)
            }
            StmtKind::Return(value) => value.as_ref().is_some_and(|e| self.expr_uses_self(e)),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let mut used = self.expr_uses_self(cond);
                used |= self.block_uses_self(then_body);
                if let Some(body) = else_body {
                    used |= self.block_uses_self(body);
                }
                used
            }
            StmtKind::Guard { cond, else_body } => {
                let used = self.expr_uses_self(cond);
                used | self.block_uses_self(else_body)
            }
            StmtKind::GuardLet {
                name,
                value,
                else_body,
                ..
            } => {
                let used = self.expr_uses_self(value) | self.block_uses_self(else_body);
                self.declare(name);
                used
            }
            StmtKind::WhileLet {
                name, value, body, ..
            } => {
                let mut used = self.expr_uses_self(value);
                self.locals.push(HashSet::new());
                self.declare(name);
                used |= self.stmts_use_self(body);
                self.locals.pop();
                used
            }
            StmtKind::IfLet {
                name,
                value,
                then_body,
                else_body,
                ..
            } => {
                let mut used = self.expr_uses_self(value);
                self.locals.push(HashSet::new());
                self.declare(name);
                used |= self.stmts_use_self(then_body);
                self.locals.pop();
                if let Some(body) = else_body {
                    used |= self.block_uses_self(body);
                }
                used
            }
            StmtKind::While { cond, body } => {
                let used = self.expr_uses_self(cond);
                used | self.block_uses_self(body)
            }
            StmtKind::For { var, iter, body } => {
                let mut used = self.expr_uses_self(iter);
                self.locals.push(HashSet::new());
                self.declare(var);
                used |= self.stmts_use_self(body);
                self.locals.pop();
                used
            }
            StmtKind::Switch { subject, cases } => {
                let mut used = self.expr_uses_self(subject);
                for case in cases {
                    self.locals.push(HashSet::new());
                    if let CaseLabel::Patterns(patterns) = case.label() {
                        for name in patterns
                            .iter()
                            .filter_map(|p| p.bindings())
                            .flatten()
                            .flatten()
                        {
                            self.declare(name);
                        }
                    }
                    used |= self.stmts_use_self(case.body());
                    self.locals.pop();
                }
                used
            }
            StmtKind::Expr(expr) => self.expr_uses_self(expr),
        }
    }

    fn expr_uses_self(&self, expr: &Expr) -> bool {
        match expr.kind() {
            ExprKind::Ident(name) => name == SELF || self.is_member(name),
            ExprKind::Unwrap(inner) => self.expr_uses_self(inner),
            ExprKind::Coalesce { lhs, rhs } => self.expr_uses_self(lhs) || self.expr_uses_self(rhs),
            ExprKind::OptionalChain { subject, link } => {
                self.expr_uses_self(subject) || self.expr_uses_self(link)
            }
            ExprKind::ChainValue => false,
            ExprKind::Nil
            | ExprKind::Int(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::GenericName { .. } => false,
            ExprKind::Unary { expr, .. } => self.expr_uses_self(expr),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr_uses_self(lhs) || self.expr_uses_self(rhs)
            }
            ExprKind::Range { lo, hi, .. } => self.expr_uses_self(lo) || self.expr_uses_self(hi),
            ExprKind::Call { callee, args } => {
                self.scope.methods.contains(&callee.as_str())
                    || args.iter().any(|a| self.expr_uses_self(a.value()))
            }
            ExprKind::GenericCall { args, .. } => {
                args.iter().any(|a| self.expr_uses_self(a.value()))
            }
            ExprKind::Member { base, .. } => self.expr_uses_self(base),
            ExprKind::MemberCall { base, args, .. } => {
                self.expr_uses_self(base) || args.iter().any(|a| self.expr_uses_self(a.value()))
            }
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
        StmtKind::IfLet {
            then_body,
            else_body,
            ..
        } => any_returns(then_body) || else_body.as_deref().is_some_and(any_returns),
        StmtKind::Guard { else_body, .. } | StmtKind::GuardLet { else_body, .. } => {
            any_returns(else_body)
        }
        StmtKind::While { body, .. }
        | StmtKind::WhileLet { body, .. }
        | StmtKind::For { body, .. } => any_returns(body),
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

    fn verdict_with(
        members: &[&str],
        methods: &[&str],
        header: &str,
        params: &[&str],
        body: &str,
    ) -> Result<(), InitViolation> {
        let src = format!("init({header}) {{\n{body}\n}}");
        let Decl::Func(init) = parse_item(&lex(&src).unwrap()).unwrap() else {
            panic!("expected an initializer");
        };
        check_initialization(
            init.body(),
            InitScope::new(members, methods, params),
            init.line(),
        )
    }

    fn verdict(members: &[&str], body: &str) -> Result<(), InitViolation> {
        verdict_with(members, &[], "", &[], body)
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

    #[test]
    fn a_member_assigned_by_its_bare_name_counts() {
        assert!(verdict(&["a", "b"], "a = 1\nb = 2").is_ok());
    }

    #[test]
    fn reading_a_member_by_its_bare_name_before_the_last_member_is_rejected() {
        let violation = verdict(&["a", "b"], "a = 1\nb = a").unwrap_err();
        assert!(violation.message().contains("before"));
    }

    #[test]
    fn a_compound_assignment_to_a_bare_member_is_a_use() {
        assert!(verdict(&["a"], "a += 1\na = 1").is_err());
    }

    #[test]
    fn calling_a_method_by_its_bare_name_is_a_use_of_self() {
        assert!(verdict_with(&["a"], &["f"], "", &[], "let x = f()\na = 1").is_err());
        assert!(verdict_with(&["a"], &["f"], "", &[], "a = 1\nlet x = f()").is_ok());
    }

    #[test]
    fn calling_a_function_that_is_not_a_method_is_not_a_use_of_self() {
        assert!(verdict_with(&["a"], &["f"], "", &[], "let x = g()\na = 1").is_ok());
    }

    #[test]
    fn a_parameter_shadows_a_member() {
        assert!(verdict_with(&["a"], &[], "a: Int", &["a"], "let x = a\nself.a = x").is_ok());
        assert!(verdict_with(&["a"], &[], "a: Int", &["a"], "a = 1").is_err());
    }

    #[test]
    fn a_local_declared_earlier_shadows_a_member() {
        assert!(verdict(&["a"], "var m = 1\nm += 1\nlet a = m\nself.a = a").is_ok());
    }

    #[test]
    fn a_local_declared_later_does_not_shadow_an_earlier_use() {
        assert!(verdict(&["a"], "print(a)\nlet a = 1\nself.a = a").is_err());
    }

    #[test]
    fn a_local_in_a_nested_scope_shadows_only_inside_it() {
        assert!(verdict(&["a"], "if true { let a = 1\nprint(a) }\nself.a = 1").is_ok());
        assert!(verdict(&["a"], "if true { let a = 1 }\nprint(a)\nself.a = 1").is_err());
    }

    #[test]
    fn a_loop_variable_and_a_pattern_binding_shadow_a_member() {
        assert!(verdict(&["a"], "for a in 0..<2 { print(a) }\nself.a = 1").is_ok());
        assert!(
            verdict(
                &["a"],
                "switch 1 { case .x(let a): print(a)\ndefault: print(2) }\nself.a = 1"
            )
            .is_ok()
        );
        assert!(verdict(&["a"], "if true { } else { print(a) }\nself.a = 1").is_err());
        assert!(verdict(&["a"], "while a > 0 { }\nself.a = 1").is_err());
    }

    fn enum_verdict(body: &str) -> Result<(), InitViolation> {
        let src = format!("init() {{\n{body}\n}}");
        let Decl::Func(init) = parse_item(&lex(&src).unwrap()).unwrap() else {
            panic!("expected an initializer");
        };
        check_enum_initialization(init.body(), InitScope::new(&[], &[], &[]), init.line())
    }

    #[test]
    fn assigning_self_at_the_top_level_initializes_an_enum() {
        assert!(enum_verdict("self = E.a").is_ok());
    }

    #[test]
    fn assigning_self_in_both_branches_initializes_an_enum() {
        assert!(enum_verdict("if true { self = E.a } else { self = E.b }").is_ok());
    }

    #[test]
    fn assigning_self_in_one_branch_does_not_initialize_an_enum() {
        assert!(enum_verdict("if true { self = E.a }").is_err());
        assert!(enum_verdict("if true { } else { self = E.a }").is_err());
    }

    #[test]
    fn an_enum_initializer_without_an_assignment_is_rejected() {
        assert!(enum_verdict("let x = 1").is_err());
    }

    #[test]
    fn using_self_before_assigning_it_is_rejected_for_an_enum() {
        assert!(enum_verdict("print(self == E.a)\nself = E.a").is_err());
    }

    #[test]
    fn returning_before_assigning_self_is_rejected_for_an_enum() {
        assert!(enum_verdict("return\nself = E.a").is_err());
    }

    fn subclass_verdict(body: &str) -> Result<(), InitViolation> {
        let src = format!("init() {{\n{body}\n}}");
        let Decl::Func(init) = parse_item(&lex(&src).unwrap()).unwrap() else {
            panic!("expected an initializer");
        };
        let members = ["x", "y"];
        let own = ["y"];
        check_initialization(
            init.body(),
            InitScope::new(&members, &[], &[]).with_superclass(&own),
            init.line(),
        )
    }

    #[test]
    fn own_members_then_super_init_initialize_a_subclass() {
        assert!(subclass_verdict("self.y = 1\nsuper.init()").is_ok());
    }

    #[test]
    fn a_subclass_initializer_without_super_init_is_rejected() {
        assert!(subclass_verdict("self.y = 1").is_err());
    }

    #[test]
    fn super_init_before_the_own_members_is_rejected() {
        assert!(subclass_verdict("super.init()\nself.y = 1").is_err());
    }

    #[test]
    fn using_self_between_the_members_and_super_init_is_rejected() {
        assert!(subclass_verdict("self.y = 1\nprint(self.y)\nsuper.init()").is_err());
    }

    #[test]
    fn assigning_an_inherited_member_before_super_init_is_rejected() {
        assert!(subclass_verdict("self.y = 1\nself.x = 2\nsuper.init()").is_err());
    }

    #[test]
    fn returning_before_super_init_is_rejected() {
        assert!(subclass_verdict("self.y = 1\nreturn\nsuper.init()").is_err());
    }

    #[test]
    fn a_super_init_argument_may_not_use_self() {
        assert!(subclass_verdict("self.y = 1\nsuper.init(self.y)").is_err());
    }
}

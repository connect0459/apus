use crate::ast::*;
use crate::lexer::{Spanned, Token};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    line: u32,
    message: String,
}

impl ParseError {
    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

type PResult<T> = Result<T, ParseError>;

const KEYWORDS: [&str; 12] = [
    "func", "struct", "var", "let", "if", "else", "while", "for", "in", "return", "true", "false",
];

const LEVEL_OR: usize = 0;
const LEVEL_RANGE: usize = 3;
const LEVEL_UNARY: usize = 6;

const BINARY_LEVELS: [&[(&str, BinOp)]; 6] = [
    &[("||", BinOp::Or)],
    &[("&&", BinOp::And)],
    &[
        ("==", BinOp::Eq),
        ("!=", BinOp::Ne),
        ("<=", BinOp::Le),
        (">=", BinOp::Ge),
        ("<", BinOp::Lt),
        (">", BinOp::Gt),
    ],
    &[],
    &[("+", BinOp::Add), ("-", BinOp::Sub)],
    &[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Rem)],
];

const ASSIGN_OPS: [(&str, Option<BinOp>); 6] = [
    ("=", None),
    ("+=", Some(BinOp::Add)),
    ("-=", Some(BinOp::Sub)),
    ("*=", Some(BinOp::Mul)),
    ("/=", Some(BinOp::Div)),
    ("%=", Some(BinOp::Rem)),
];

pub fn parse_item(tokens: &[Spanned]) -> PResult<Decl> {
    let mut parser = Parser { tokens, pos: 0 };
    let decl = parser.decl()?;
    match parser.tokens.get(parser.pos) {
        Some(t) => Err(ParseError {
            line: t.line(),
            message: "unexpected token after the end of the item".to_string(),
        }),
        None => Ok(decl),
    }
}

struct Parser<'a> {
    tokens: &'a [Spanned],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(Spanned::token)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(Spanned::token)
    }

    fn line(&self) -> u32 {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .map_or(1, |t| t.line())
    }

    fn fail<T>(&self, message: &str) -> PResult<T> {
        Err(ParseError {
            line: self.line(),
            message: message.to_string(),
        })
    }

    fn on_same_line(&self) -> bool {
        match (
            self.pos.checked_sub(1).and_then(|i| self.tokens.get(i)),
            self.tokens.get(self.pos),
        ) {
            (Some(prev), Some(cur)) => prev.line() == cur.line(),
            _ => false,
        }
    }

    fn previous_is_closing_brace(&self) -> bool {
        self.pos
            .checked_sub(1)
            .and_then(|i| self.tokens.get(i))
            .is_some_and(|t| *t.token() == Token::Punct("}".to_string()))
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Token::Punct(s)) if s == p)
    }

    fn is_keyword(&self, k: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(s)) if s == k)
    }

    fn eat_punct(&mut self, p: &str) -> bool {
        let found = self.is_punct(p);
        if found {
            self.pos += 1;
        }
        found
    }

    fn eat_keyword(&mut self, k: &str) -> bool {
        let found = self.is_keyword(k);
        if found {
            self.pos += 1;
        }
        found
    }

    fn expect_punct(&mut self, p: &str) -> PResult<()> {
        if self.eat_punct(p) {
            Ok(())
        } else {
            self.fail(&format!("expected '{p}'"))
        }
    }

    fn expect_keyword(&mut self, k: &str) -> PResult<()> {
        if self.eat_keyword(k) {
            Ok(())
        } else {
            self.fail(&format!("expected '{k}'"))
        }
    }

    fn peek_name(&self) -> Option<&str> {
        match self.peek() {
            Some(Token::Ident(s)) if !KEYWORDS.contains(&s.as_str()) => Some(s),
            _ => None,
        }
    }

    fn name(&mut self) -> PResult<String> {
        match self.peek_name().map(str::to_string) {
            Some(n) => {
                self.pos += 1;
                Ok(n)
            }
            None => self.fail("expected a name"),
        }
    }

    fn decl(&mut self) -> PResult<Decl> {
        if self.eat_keyword("func") {
            self.func_decl().map(Decl::Func)
        } else if self.eat_keyword("struct") {
            self.struct_decl().map(Decl::Struct)
        } else {
            self.fail("expected 'func' or 'struct'")
        }
    }

    fn func_decl(&mut self) -> PResult<FuncDecl> {
        let name = self.name()?;
        self.expect_punct("(")?;
        let mut params = Vec::new();
        while !self.is_punct(")") {
            if !params.is_empty() {
                self.expect_punct(",")?;
            }
            params.push(self.param()?);
        }
        self.expect_punct(")")?;
        let ret = if self.eat_punct("->") {
            Some(self.name()?)
        } else {
            None
        };
        let body = self.block()?;
        Ok(FuncDecl::new(name, params, ret, body))
    }

    fn param(&mut self) -> PResult<Param> {
        let first = match self.peek() {
            Some(Token::Ident(s)) if s == "_" => {
                self.pos += 1;
                None
            }
            _ => Some(self.name()?),
        };
        let (label, name) = match (first, self.peek_name().is_some()) {
            (first, true) => (first, self.name()?),
            (Some(n), false) => (Some(n.clone()), n),
            (None, false) => return self.fail("expected a parameter name after '_'"),
        };
        self.expect_punct(":")?;
        let ty = self.name()?;
        Ok(Param::new(label, name, ty))
    }

    fn struct_decl(&mut self) -> PResult<StructDecl> {
        let name = self.name()?;
        self.expect_punct("{")?;
        let mut fields = Vec::new();
        loop {
            while self.eat_punct(";") {}
            if self.eat_punct("}") {
                break;
            }
            let mutable = if self.eat_keyword("var") {
                true
            } else if self.eat_keyword("let") {
                false
            } else {
                return self.fail("expected 'var' or 'let' in a struct body");
            };
            let field_name = self.name()?;
            self.expect_punct(":")?;
            let ty = self.name()?;
            fields.push(Field::new(field_name, mutable, ty));
            self.end_of_statement()?;
        }
        Ok(StructDecl::new(name, fields))
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect_punct("{")?;
        let mut stmts = Vec::new();
        loop {
            while self.eat_punct(";") {}
            if self.eat_punct("}") {
                return Ok(stmts);
            }
            if self.peek().is_none() {
                return self.fail("unterminated block, expected '}'");
            }
            stmts.push(self.stmt()?);
            self.end_of_statement()?;
        }
    }

    fn end_of_statement(&self) -> PResult<()> {
        let ended = self.is_punct(";")
            || self.is_punct("}")
            || self.peek().is_none()
            || !self.on_same_line()
            || self.previous_is_closing_brace();
        if ended {
            Ok(())
        } else {
            self.fail("expected a newline or ';' between statements")
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        if self.is_keyword("var") || self.is_keyword("let") {
            self.let_stmt()
        } else if self.eat_keyword("if") {
            self.if_stmt()
        } else if self.eat_keyword("while") {
            let cond = self.expr()?;
            let body = self.block()?;
            Ok(Stmt::While { cond, body })
        } else if self.eat_keyword("for") {
            let var = self.name()?;
            self.expect_keyword("in")?;
            let iter = self.expr()?;
            let body = self.block()?;
            Ok(Stmt::For { var, iter, body })
        } else if self.eat_keyword("return") {
            let has_value = self.on_same_line() && !self.is_punct("}") && !self.is_punct(";");
            Ok(Stmt::Return(if has_value {
                Some(self.expr()?)
            } else {
                None
            }))
        } else {
            self.expr_or_assign()
        }
    }

    fn let_stmt(&mut self) -> PResult<Stmt> {
        let mutable = self.is_keyword("var");
        self.pos += 1;
        let name = self.name()?;
        let ty = if self.eat_punct(":") {
            Some(self.name()?)
        } else {
            None
        };
        let init = if self.eat_punct("=") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            init,
        })
    }

    fn if_stmt(&mut self) -> PResult<Stmt> {
        let cond = self.expr()?;
        let then_body = self.block()?;
        let else_body = if self.eat_keyword("else") {
            if self.eat_keyword("if") {
                Some(vec![self.if_stmt()?])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_body,
            else_body,
        })
    }

    fn expr_or_assign(&mut self) -> PResult<Stmt> {
        let target = self.expr()?;
        let op = ASSIGN_OPS
            .iter()
            .find(|(p, _)| self.on_same_line() && self.is_punct(p));
        let Some((_, op)) = op else {
            return Ok(Stmt::Expr(target));
        };
        if !matches!(target, Expr::Ident(_) | Expr::Member { .. }) {
            return self.fail("the left side of an assignment must be a variable or a member");
        }
        self.pos += 1;
        let value = self.expr()?;
        Ok(Stmt::Assign {
            target,
            op: *op,
            value,
        })
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.binary(LEVEL_OR)
    }

    fn binary(&mut self, level: usize) -> PResult<Expr> {
        if level == LEVEL_UNARY {
            return self.unary();
        }
        if level == LEVEL_RANGE {
            return self.range();
        }
        let mut lhs = self.binary(level + 1)?;
        while let Some(op) = self.binary_op_at(level) {
            self.pos += 1;
            let rhs = self.binary(level + 1)?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn binary_op_at(&self, level: usize) -> Option<BinOp> {
        if !self.on_same_line() {
            return None;
        }
        BINARY_LEVELS[level]
            .iter()
            .find(|(p, _)| self.is_punct(p))
            .map(|(_, op)| *op)
    }

    fn range(&mut self) -> PResult<Expr> {
        let lo = self.binary(LEVEL_RANGE + 1)?;
        let inclusive = if !self.on_same_line() {
            return Ok(lo);
        } else if self.is_punct("..<") {
            false
        } else if self.is_punct("...") {
            true
        } else {
            return Ok(lo);
        };
        self.pos += 1;
        let hi = self.binary(LEVEL_RANGE + 1)?;
        Ok(Expr::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            inclusive,
        })
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = if self.is_punct("-") {
            Some(UnaryOp::Neg)
        } else if self.is_punct("!") {
            Some(UnaryOp::Not)
        } else {
            None
        };
        match op {
            Some(op) => {
                self.pos += 1;
                Ok(Expr::Unary {
                    op,
                    expr: Box::new(self.unary()?),
                })
            }
            None => self.postfix(),
        }
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.primary()?;
        while self.on_same_line() {
            if self.eat_punct(".") {
                let name = self.name()?;
                expr = Expr::Member {
                    base: Box::new(expr),
                    name,
                };
            } else if self.is_punct("(") {
                let Expr::Ident(callee) = expr else {
                    return self.fail("only a named function can be called");
                };
                expr = Expr::Call {
                    callee,
                    args: self.args()?,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn args(&mut self) -> PResult<Vec<Arg>> {
        self.expect_punct("(")?;
        let mut args = Vec::new();
        while !self.is_punct(")") {
            if !args.is_empty() {
                self.expect_punct(",")?;
            }
            let labeled = self.peek_name().is_some()
                && matches!(self.peek_at(1), Some(Token::Punct(p)) if p == ":");
            let label = if labeled {
                let l = self.name()?;
                self.expect_punct(":")?;
                Some(l)
            } else {
                None
            };
            args.push(Arg::new(label, self.expr()?));
        }
        self.expect_punct(")")?;
        Ok(args)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let expr = match self.peek() {
            Some(Token::Int(n)) => Expr::Int(*n),
            Some(Token::Str(s)) => Expr::Str(s.clone()),
            Some(Token::Ident(s)) if s == "true" => Expr::Bool(true),
            Some(Token::Ident(s)) if s == "false" => Expr::Bool(false),
            Some(Token::Ident(s)) if !KEYWORDS.contains(&s.as_str()) => Expr::Ident(s.clone()),
            Some(Token::Punct(p)) if p == "(" => {
                self.pos += 1;
                let inner = self.expr()?;
                self.expect_punct(")")?;
                return Ok(inner);
            }
            _ => return self.fail("expected an expression"),
        };
        self.pos += 1;
        Ok(expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn try_parse(src: &str) -> Result<Decl, ParseError> {
        parse_item(&lex(src).unwrap())
    }

    fn func(src: &str) -> FuncDecl {
        match try_parse(src).unwrap() {
            Decl::Func(f) => f,
            other => panic!("expected a func, got {other:?}"),
        }
    }

    fn body(src: &str) -> Vec<Stmt> {
        func(&format!("func f() {{\n{src}\n}}")).body().to_vec()
    }

    fn expr(src: &str) -> Expr {
        match body(&format!("let x = {src}")).remove(0) {
            Stmt::Let { init: Some(e), .. } => e,
            other => panic!("expected a let with initializer, got {other:?}"),
        }
    }

    fn int(n: i64) -> Expr {
        Expr::Int(n)
    }

    fn id(s: &str) -> Expr {
        Expr::Ident(s.to_string())
    }

    fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    fn param(label: Option<&str>, name: &str, ty: &str) -> Param {
        Param::new(label.map(str::to_string), name.to_string(), ty.to_string())
    }

    fn field(name: &str, mutable: bool, ty: &str) -> Field {
        Field::new(name.to_string(), mutable, ty.to_string())
    }

    #[test]
    fn parameters_without_a_label_are_marked_by_an_underscore() {
        assert_eq!(
            func("func f(_ a: Int) { }").params(),
            vec![param(None, "a", "Int")]
        );
    }

    #[test]
    fn a_parameter_name_doubles_as_its_label_unless_renamed() {
        assert_eq!(
            func("func f(a: Int, outer inner: Bool) { }").params(),
            vec![
                param(Some("a"), "a", "Int"),
                param(Some("outer"), "inner", "Bool")
            ]
        );
    }

    #[test]
    fn a_declaration_is_known_by_its_name() {
        assert_eq!(try_parse("func f() { }").unwrap().name(), "f");
        assert_eq!(try_parse("struct P { }").unwrap().name(), "P");
    }

    #[test]
    fn a_return_type_is_optional() {
        assert_eq!(func("func f() { }").ret(), None);
        assert_eq!(func("func f() -> Int { return 1 }").ret(), Some("Int"));
    }

    #[test]
    fn struct_fields_remember_whether_they_are_mutable() {
        let Decl::Struct(s) = try_parse("struct P { var x: Int; let y: Bool }").unwrap() else {
            panic!("expected a struct");
        };
        assert_eq!(s.name(), "P");
        assert_eq!(
            s.fields(),
            vec![field("x", true, "Int"), field("y", false, "Bool")]
        );
    }

    #[test]
    fn struct_fields_may_be_separated_by_newlines() {
        let Decl::Struct(s) = try_parse("struct P {\n var x: Int\n var y: Int\n}").unwrap() else {
            panic!("expected a struct");
        };
        assert_eq!(s.fields().len(), 2);
    }

    #[test]
    fn declarations_may_omit_the_type_or_the_initializer() {
        assert_eq!(
            body("var a = 1\nlet b: Int = 2\nvar c: Int"),
            vec![
                Stmt::Let {
                    name: "a".into(),
                    mutable: true,
                    ty: None,
                    init: Some(int(1))
                },
                Stmt::Let {
                    name: "b".into(),
                    mutable: false,
                    ty: Some("Int".into()),
                    init: Some(int(2)),
                },
                Stmt::Let {
                    name: "c".into(),
                    mutable: true,
                    ty: Some("Int".into()),
                    init: None
                },
            ]
        );
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            expr("1 + 2 * 3"),
            bin(BinOp::Add, int(1), bin(BinOp::Mul, int(2), int(3)))
        );
    }

    #[test]
    fn binary_operators_of_equal_precedence_associate_to_the_left() {
        assert_eq!(
            expr("1 - 2 - 3"),
            bin(BinOp::Sub, bin(BinOp::Sub, int(1), int(2)), int(3))
        );
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(
            expr("(1 + 2) * 3"),
            bin(BinOp::Mul, bin(BinOp::Add, int(1), int(2)), int(3))
        );
    }

    #[test]
    fn comparison_binds_looser_than_arithmetic_and_logic_looser_still() {
        assert_eq!(
            expr("a + 1 < b || c && d"),
            bin(
                BinOp::Or,
                bin(BinOp::Lt, bin(BinOp::Add, id("a"), int(1)), id("b")),
                bin(BinOp::And, id("c"), id("d")),
            )
        );
    }

    #[test]
    fn unary_operators_apply_to_the_operand_that_follows() {
        assert_eq!(
            expr("-a * !b"),
            bin(
                BinOp::Mul,
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(id("a"))
                },
                Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(id("b"))
                },
            )
        );
    }

    #[test]
    fn literals_cover_integers_booleans_and_strings() {
        assert_eq!(expr("true"), Expr::Bool(true));
        assert_eq!(expr("false"), Expr::Bool(false));
        assert_eq!(expr("\"hi\""), Expr::Str("hi".to_string()));
    }

    #[test]
    fn calls_carry_optional_argument_labels() {
        assert_eq!(
            expr("P(x: 1, 2)"),
            Expr::Call {
                callee: "P".to_string(),
                args: vec![
                    Arg::new(Some("x".to_string()), int(1)),
                    Arg::new(None, int(2))
                ],
            }
        );
    }

    #[test]
    fn member_access_chains_to_the_left() {
        assert_eq!(
            expr("a.b.c"),
            Expr::Member {
                base: Box::new(Expr::Member {
                    base: Box::new(id("a")),
                    name: "b".to_string()
                }),
                name: "c".to_string(),
            }
        );
    }

    #[test]
    fn ranges_are_half_open_or_closed_and_bind_looser_than_addition() {
        assert_eq!(
            expr("0..<n + 1"),
            Expr::Range {
                lo: Box::new(int(0)),
                hi: Box::new(bin(BinOp::Add, id("n"), int(1))),
                inclusive: false,
            }
        );
        assert_eq!(
            expr("1...3"),
            Expr::Range {
                lo: Box::new(int(1)),
                hi: Box::new(int(3)),
                inclusive: true
            }
        );
    }

    #[test]
    fn assignment_may_combine_with_an_arithmetic_operator() {
        assert_eq!(
            body("s = 1\ns += 2\np.x *= 3"),
            vec![
                Stmt::Assign {
                    target: id("s"),
                    op: None,
                    value: int(1)
                },
                Stmt::Assign {
                    target: id("s"),
                    op: Some(BinOp::Add),
                    value: int(2)
                },
                Stmt::Assign {
                    target: Expr::Member {
                        base: Box::new(id("p")),
                        name: "x".to_string()
                    },
                    op: Some(BinOp::Mul),
                    value: int(3),
                },
            ]
        );
    }

    #[test]
    fn only_variables_and_members_can_be_assigned() {
        assert!(try_parse("func f() { 1 = 2 }").is_err());
    }

    #[test]
    fn if_chains_nest_else_if_inside_the_else_branch() {
        let stmts = body("if a { return 1 } else if b { return 2 } else { return 3 }");
        let Stmt::If {
            else_body: Some(outer_else),
            ..
        } = &stmts[0]
        else {
            panic!("expected an if with else");
        };
        let Stmt::If {
            else_body: Some(inner_else),
            ..
        } = &outer_else[0]
        else {
            panic!("expected a nested if with else");
        };
        assert_eq!(inner_else, &vec![Stmt::Return(Some(int(3)))]);
    }

    #[test]
    fn while_loops_take_a_condition_and_a_body() {
        assert_eq!(
            body("while a < 3 { a += 1 }"),
            vec![Stmt::While {
                cond: bin(BinOp::Lt, id("a"), int(3)),
                body: vec![Stmt::Assign {
                    target: id("a"),
                    op: Some(BinOp::Add),
                    value: int(1)
                }],
            }]
        );
    }

    #[test]
    fn for_loops_iterate_a_variable_over_a_range() {
        assert_eq!(
            body("for k in 0..<3 { }"),
            vec![Stmt::For {
                var: "k".to_string(),
                iter: Expr::Range {
                    lo: Box::new(int(0)),
                    hi: Box::new(int(3)),
                    inclusive: false
                },
                body: vec![],
            }]
        );
    }

    #[test]
    fn return_takes_a_value_only_when_one_follows_on_the_same_line() {
        assert_eq!(body("return"), vec![Stmt::Return(None)]);
        assert_eq!(
            body("return\n1"),
            vec![Stmt::Return(None), Stmt::Expr(int(1))]
        );
        assert_eq!(body("return 1"), vec![Stmt::Return(Some(int(1)))]);
    }

    #[test]
    fn statements_are_separated_by_newlines_or_semicolons() {
        assert_eq!(body("a = 1; b = 2\nc = 3").len(), 3);
    }

    #[test]
    fn two_statements_on_one_line_need_a_separator() {
        assert!(try_parse("func f() { a = 1 b = 2 }").is_err());
    }

    #[test]
    fn an_expression_ends_at_the_end_of_its_line() {
        assert_eq!(
            body("let a = 1\n-2"),
            vec![
                Stmt::Let {
                    name: "a".into(),
                    mutable: false,
                    ty: None,
                    init: Some(int(1))
                },
                Stmt::Expr(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(int(2))
                }),
            ]
        );
    }

    #[test]
    fn keywords_cannot_be_used_as_names() {
        assert!(try_parse("func f() { let while = 1 }").is_err());
    }

    #[test]
    fn boolean_literals_cannot_be_used_as_names() {
        assert!(try_parse("func f() { let false = 1 }").is_err());
        assert!(try_parse("func f() { let true = 1 }").is_err());
    }

    #[test]
    fn tokens_after_the_closing_brace_are_rejected() {
        let mut tokens = lex("func f() { }").unwrap();
        tokens.extend(lex("x").unwrap());
        assert!(parse_item(&tokens).is_err());
    }

    #[test]
    fn errors_report_the_line_of_the_offending_token() {
        let err = try_parse("func f() {\n  let a = \n  )\n}").unwrap_err();
        assert_eq!(err.line(), 3);
    }

    #[test]
    fn a_block_without_its_closing_brace_is_an_error() {
        assert!(try_parse("func f() { let a = 1").is_err());
    }
}

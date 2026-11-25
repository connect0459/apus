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

const KEYWORDS: [&str; 20] = [
    "func", "struct", "class", "enum", "init", "mutating", "override", "case", "switch", "default",
    "var", "let", "if", "else", "while", "for", "in", "return", "true", "false",
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
        ("===", BinOp::Is),
        ("!==", BinOp::IsNot),
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
    parse_with(tokens, false)
}

pub fn parse_signature(tokens: &[Spanned]) -> PResult<Decl> {
    parse_with(tokens, true)
}

fn parse_with(tokens: &[Spanned], header_only: bool) -> PResult<Decl> {
    let mut parser = Parser {
        tokens,
        pos: 0,
        header_only,
    };
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
    header_only: bool,
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

    fn type_ref(&mut self) -> PResult<TypeRef> {
        let name = self.name()?;
        let args = if self.is_punct("<") {
            self.type_args()?
        } else {
            Vec::new()
        };
        Ok(TypeRef::new(name, args))
    }

    fn type_args(&mut self) -> PResult<Vec<TypeRef>> {
        self.expect_punct("<")?;
        let mut args = vec![self.type_ref()?];
        while self.eat_punct(",") {
            args.push(self.type_ref()?);
        }
        self.expect_punct(">")?;
        Ok(args)
    }

    fn generic_params(&mut self) -> PResult<Vec<String>> {
        if !self.eat_punct("<") {
            return Ok(Vec::new());
        }
        let mut names = vec![self.name()?];
        while self.eat_punct(",") {
            let name = self.name()?;
            if names.contains(&name) {
                return self.fail("duplicate generic parameter");
            }
            names.push(name);
        }
        self.expect_punct(">")?;
        Ok(names)
    }

    fn decl(&mut self) -> PResult<Decl> {
        let line = self.line();
        if self.eat_keyword("override") {
            self.expect_keyword("func")?;
            self.func_decl(line).map(|f| Decl::Func(f.as_override()))
        } else if self.eat_keyword("mutating") {
            self.expect_keyword("func")?;
            self.func_decl(line).map(|f| Decl::Func(f.as_mutating()))
        } else if self.eat_keyword("init") {
            self.init_decl(line).map(Decl::Func)
        } else if self.eat_keyword("func") {
            self.func_decl(line).map(Decl::Func)
        } else if self.eat_keyword("struct") {
            self.struct_decl(line, false).map(Decl::Struct)
        } else if self.eat_keyword("class") {
            self.struct_decl(line, true).map(Decl::Struct)
        } else if self.eat_keyword("enum") {
            self.enum_decl(line).map(Decl::Enum)
        } else {
            self.fail("expected 'func', 'struct', 'class' or 'enum'")
        }
    }

    fn func_decl(&mut self, line: u32) -> PResult<FuncDecl> {
        let name = self.name()?;
        let generics = self.generic_params()?;
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
            Some(self.type_ref()?)
        } else {
            None
        };
        let body = if self.header_only {
            self.pos = self.tokens.len();
            Vec::new()
        } else {
            self.block()?
        };
        Ok(FuncDecl::new(line, name, params, ret, body).with_generics(generics))
    }

    fn init_decl(&mut self, line: u32) -> PResult<FuncDecl> {
        let generics = self.generic_params()?;
        self.expect_punct("(")?;
        let mut params = Vec::new();
        while !self.is_punct(")") {
            if !params.is_empty() {
                self.expect_punct(",")?;
            }
            params.push(self.param()?);
        }
        self.expect_punct(")")?;
        let body = if self.header_only {
            self.pos = self.tokens.len();
            Vec::new()
        } else {
            self.block()?
        };
        Ok(FuncDecl::new(line, "init".to_string(), params, None, body)
            .with_generics(generics)
            .as_init())
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
        let ty = self.type_ref()?;
        Ok(Param::new(label, name, ty))
    }

    fn struct_decl(&mut self, line: u32, class: bool) -> PResult<StructDecl> {
        let name = self.name()?;
        let generics = self.generic_params()?;
        let superclass = if class && self.eat_punct(":") {
            let superclass = self.name()?;
            if self.is_punct("<") {
                return self.fail(
                    "a superclass cannot take type arguments: generic classes cannot take part in inheritance yet",
                );
            }
            Some(superclass)
        } else {
            None
        };
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
                return self.fail("expected 'var' or 'let' in a struct or class body");
            };
            let field_name = self.name()?;
            self.expect_punct(":")?;
            let ty = self.type_ref()?;
            fields.push(Field::new(field_name, mutable, ty));
            self.end_of_statement()?;
        }
        let decl = StructDecl::new(line, name, fields).with_generics(generics);
        let decl = match superclass {
            Some(superclass) => decl.with_superclass(superclass),
            None => decl,
        };
        Ok(if class { decl.as_class() } else { decl })
    }

    fn enum_decl(&mut self, line: u32) -> PResult<EnumDecl> {
        let name = self.name()?;
        let generics = self.generic_params()?;
        self.expect_punct("{")?;
        let mut cases = Vec::new();
        loop {
            while self.eat_punct(";") {}
            if self.eat_punct("}") {
                break;
            }
            self.expect_keyword("case")?;
            cases.push(self.enum_case_decl()?);
            while self.eat_punct(",") {
                cases.push(self.enum_case_decl()?);
            }
            self.end_of_statement()?;
        }
        Ok(EnumDecl::new(line, name, cases).with_generics(generics))
    }

    fn enum_case_decl(&mut self) -> PResult<EnumCaseDecl> {
        let name = self.name()?;
        let mut payload = Vec::new();
        if self.eat_punct("(") {
            loop {
                payload.push(self.type_ref()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(")")?;
        }
        Ok(EnumCaseDecl::new(name, payload))
    }

    fn switch_kind(&mut self) -> PResult<StmtKind> {
        let subject = self.expr()?;
        self.expect_punct("{")?;
        let mut cases = Vec::new();
        loop {
            while self.eat_punct(";") {}
            if self.eat_punct("}") {
                return Ok(StmtKind::Switch { subject, cases });
            }
            cases.push(self.switch_case()?);
        }
    }

    fn switch_case(&mut self) -> PResult<SwitchCase> {
        let line = self.line();
        let label = if self.eat_keyword("default") {
            CaseLabel::Default
        } else if self.eat_keyword("case") {
            let mut patterns = vec![self.pattern()?];
            while self.eat_punct(",") {
                patterns.push(self.pattern()?);
            }
            CaseLabel::Patterns(patterns)
        } else {
            return self.fail("expected 'case' or 'default' in a switch body");
        };
        self.expect_punct(":")?;
        let mut body = Vec::new();
        loop {
            while self.eat_punct(";") {}
            if self.is_punct("}") || self.is_keyword("case") || self.is_keyword("default") {
                return Ok(SwitchCase::new(line, label, body));
            }
            if self.peek().is_none() {
                return self.fail("unterminated switch, expected '}'");
            }
            body.push(self.stmt()?);
            self.end_of_statement()?;
        }
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        let type_name = if self.is_punct(".") {
            None
        } else {
            Some(self.name()?)
        };
        self.expect_punct(".")?;
        let case = self.name()?;
        let bindings = if self.eat_punct("(") {
            let mut bindings = Vec::new();
            loop {
                bindings.push(self.binding()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(")")?;
            Some(bindings)
        } else {
            None
        };
        Ok(Pattern::new(type_name, case, bindings))
    }

    fn binding(&mut self) -> PResult<Option<String>> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s == "_") {
            self.pos += 1;
            return Ok(None);
        }
        self.expect_keyword("let")?;
        self.name().map(Some)
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
        let line = self.line();
        let kind = if self.is_keyword("var") || self.is_keyword("let") {
            self.let_kind()?
        } else if self.eat_keyword("if") {
            self.if_kind()?
        } else if self.eat_keyword("switch") {
            self.switch_kind()?
        } else if self.eat_keyword("while") {
            let cond = self.expr()?;
            let body = self.block()?;
            StmtKind::While { cond, body }
        } else if self.eat_keyword("for") {
            let var = self.name()?;
            self.expect_keyword("in")?;
            let iter = self.expr()?;
            let body = self.block()?;
            StmtKind::For { var, iter, body }
        } else if self.eat_keyword("return") {
            let has_value = self.on_same_line() && !self.is_punct("}") && !self.is_punct(";");
            StmtKind::Return(if has_value { Some(self.expr()?) } else { None })
        } else {
            self.expr_or_assign_kind()?
        };
        Ok(Stmt::new(line, kind))
    }

    fn let_kind(&mut self) -> PResult<StmtKind> {
        let mutable = self.is_keyword("var");
        self.pos += 1;
        let name = self.name()?;
        let ty = if self.eat_punct(":") {
            Some(self.type_ref()?)
        } else {
            None
        };
        let init = if self.eat_punct("=") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(StmtKind::Let {
            name,
            mutable,
            ty,
            init,
        })
    }

    fn if_kind(&mut self) -> PResult<StmtKind> {
        let cond = self.expr()?;
        let then_body = self.block()?;
        let else_body = if self.eat_keyword("else") {
            let line = self.line();
            if self.eat_keyword("if") {
                Some(vec![Stmt::new(line, self.if_kind()?)])
            } else {
                Some(self.block()?)
            }
        } else {
            None
        };
        Ok(StmtKind::If {
            cond,
            then_body,
            else_body,
        })
    }

    fn expr_or_assign_kind(&mut self) -> PResult<StmtKind> {
        let target = self.expr()?;
        let op = ASSIGN_OPS
            .iter()
            .find(|(p, _)| self.on_same_line() && self.is_punct(p));
        let Some((_, op)) = op else {
            return Ok(StmtKind::Expr(target));
        };
        if !matches!(target.kind(), ExprKind::Ident(_) | ExprKind::Member { .. }) {
            return self.fail("the left side of an assignment must be a variable or a member");
        }
        self.pos += 1;
        let value = self.expr()?;
        Ok(StmtKind::Assign {
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
            let line = lhs.line();
            lhs = Expr::new(
                line,
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            );
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
        let line = lo.line();
        Ok(Expr::new(
            line,
            ExprKind::Range {
                lo: Box::new(lo),
                hi: Box::new(hi),
                inclusive,
            },
        ))
    }

    fn unary(&mut self) -> PResult<Expr> {
        let line = self.line();
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
                let operand = self.unary()?;
                Ok(Expr::new(
                    line,
                    ExprKind::Unary {
                        op,
                        expr: Box::new(operand),
                    },
                ))
            }
            None => self.postfix(),
        }
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.primary()?;
        while self.on_same_line() {
            let line = expr.line();
            if let ExprKind::Ident(name) = expr.kind().clone()
                && let Some(type_args) = self.generic_name_args()
            {
                expr = Expr::new(line, ExprKind::GenericName { name, type_args });
                continue;
            }
            if self.eat_punct(".") {
                let name = self.name()?;
                expr = Expr::new(
                    line,
                    ExprKind::Member {
                        base: Box::new(expr),
                        name,
                    },
                );
            } else if self.is_punct("(") {
                if let ExprKind::Member { base, name } = expr.kind().clone() {
                    expr = Expr::new(
                        line,
                        ExprKind::MemberCall {
                            base,
                            name,
                            args: self.args()?,
                        },
                    );
                    continue;
                }
                if let ExprKind::GenericName { name, type_args } = expr.kind().clone() {
                    expr = Expr::new(
                        line,
                        ExprKind::GenericCall {
                            callee: name,
                            type_args,
                            args: self.args()?,
                        },
                    );
                    continue;
                }
                let ExprKind::Ident(callee) = expr.kind().clone() else {
                    return self.fail("only a named function can be called");
                };
                expr = Expr::new(
                    line,
                    ExprKind::Call {
                        callee,
                        args: self.args()?,
                    },
                );
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn generic_name_args(&mut self) -> Option<Vec<TypeRef>> {
        if !self.is_punct("<") {
            return None;
        }
        let start = self.pos;
        let parsed = self
            .type_args()
            .ok()
            .filter(|_| self.on_same_line() && (self.is_punct(".") || self.is_punct("(")));
        if parsed.is_none() {
            self.pos = start;
        }
        parsed
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
        let line = self.line();
        let kind = match self.peek() {
            Some(Token::Int(n)) => ExprKind::Int(*n),
            Some(Token::Str(s)) => ExprKind::Str(s.clone()),
            Some(Token::Ident(s)) if s == "true" => ExprKind::Bool(true),
            Some(Token::Ident(s)) if s == "false" => ExprKind::Bool(false),
            Some(Token::Ident(s)) if !KEYWORDS.contains(&s.as_str()) => ExprKind::Ident(s.clone()),
            Some(Token::Punct(p)) if p == "(" => {
                self.pos += 1;
                let inner = self.expr()?;
                self.expect_punct(")")?;
                return Ok(inner);
            }
            _ => return self.fail("expected an expression"),
        };
        self.pos += 1;
        Ok(Expr::new(line, kind))
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
        match body(&format!("let x = {src}")).remove(0).kind() {
            StmtKind::Let { init: Some(e), .. } => e.clone(),
            other => panic!("expected a let with initializer, got {other:?}"),
        }
    }

    fn ex(kind: ExprKind) -> Expr {
        Expr::new(0, kind)
    }

    fn st(kind: StmtKind) -> Stmt {
        Stmt::new(0, kind)
    }

    fn int(n: i64) -> Expr {
        ex(ExprKind::Int(n))
    }

    #[test]
    fn a_class_may_name_a_superclass() {
        let Decl::Struct(class) = try_parse("class B: A { var y: Int }").unwrap() else {
            panic!("expected a class");
        };
        assert_eq!(class.superclass(), Some("A"));
        assert_eq!(class.fields().len(), 1);
    }

    #[test]
    fn a_class_has_no_superclass_by_default() {
        let Decl::Struct(class) = try_parse("class A { var x: Int }").unwrap() else {
            panic!("expected a class");
        };
        assert_eq!(class.superclass(), None);
    }

    #[test]
    fn a_struct_cannot_name_a_superclass() {
        assert!(try_parse("struct S: A { var x: Int }").is_err());
    }

    #[test]
    fn override_marks_a_function_as_overriding() {
        let f = func("override func f() { }");
        assert!(f.is_override());
        assert!(!func("func f() { }").is_override());
    }

    #[test]
    fn override_must_be_followed_by_func() {
        assert!(try_parse("override init() { }").is_err());
    }

    #[test]
    fn identity_operators_bind_like_equality() {
        let e = expr("a === b && c !== d");
        assert_eq!(
            e,
            bin(
                BinOp::And,
                bin(BinOp::Is, id("a"), id("b")),
                bin(BinOp::IsNot, id("c"), id("d"))
            )
        );
    }

    fn id(s: &str) -> Expr {
        ex(ExprKind::Ident(s.to_string()))
    }

    fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
        ex(ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn unary(op: UnaryOp, e: Expr) -> Expr {
        ex(ExprKind::Unary {
            op,
            expr: Box::new(e),
        })
    }

    fn member(base: Expr, name: &str) -> Expr {
        ex(ExprKind::Member {
            base: Box::new(base),
            name: name.to_string(),
        })
    }

    fn range(lo: Expr, hi: Expr, inclusive: bool) -> Expr {
        ex(ExprKind::Range {
            lo: Box::new(lo),
            hi: Box::new(hi),
            inclusive,
        })
    }

    fn declare(name: &str, mutable: bool, ty: Option<&str>, init: Option<Expr>) -> Stmt {
        st(StmtKind::Let {
            name: name.to_string(),
            mutable,
            ty: ty.map(TypeRef::from),
            init,
        })
    }

    fn assign(target: Expr, op: Option<BinOp>, value: Expr) -> Stmt {
        st(StmtKind::Assign { target, op, value })
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
    fn a_class_declaration_is_a_struct_declaration_marked_as_a_class() {
        let Decl::Struct(class) = try_parse("class C { var x: Int }").unwrap() else {
            panic!("expected a class");
        };
        assert!(class.is_class());
        assert_eq!(class.fields().len(), 1);
        let Decl::Struct(plain) = try_parse("struct C { var x: Int }").unwrap() else {
            panic!("expected a struct");
        };
        assert!(!plain.is_class());
    }

    #[test]
    fn a_function_is_neither_mutating_nor_an_initializer_by_default() {
        let f = func("func f() { }");
        assert!(!f.is_mutating());
        assert!(!f.is_init());
        assert_eq!(f.owner(), None);
    }

    #[test]
    fn mutating_marks_a_function_as_mutating() {
        let f = func("mutating func f() { }");
        assert!(f.is_mutating());
        assert_eq!(f.name(), "f");
    }

    #[test]
    fn an_initializer_is_a_function_named_init_without_a_result() {
        let f = func("init(x: Int, y: Int) { self.x = x }");
        assert!(f.is_init());
        assert_eq!(f.name(), "init");
        assert_eq!(f.params().len(), 2);
        assert_eq!(f.ret(), None);
        assert_eq!(f.body().len(), 1);
    }

    #[test]
    fn an_initializer_records_the_type_parameters_it_declares() {
        assert_eq!(func("init<T>(x: T) { }").generics(), ["T"]);
    }

    #[test]
    fn mutating_must_be_followed_by_func() {
        assert!(try_parse("mutating init() { }").is_err());
    }

    #[test]
    fn giving_a_function_an_owner_qualifies_its_name() {
        let f = func("func bump() { }").with_owner("Counter");
        assert_eq!(f.owner(), Some("Counter"));
        assert_eq!(f.name(), "Counter.bump");
    }

    #[test]
    fn a_signature_parse_of_an_initializer_skips_its_body() {
        let Decl::Func(f) = parse_signature(&lex("init(x: Int) { self.x = x }").unwrap()).unwrap()
        else {
            panic!("expected a func");
        };
        assert!(f.is_init());
        assert!(f.body().is_empty());
    }

    #[test]
    fn init_and_mutating_cannot_be_used_as_names() {
        assert!(try_parse("func f() { let init = 1 }").is_err());
        assert!(try_parse("func f() { let mutating = 1 }").is_err());
    }

    #[test]
    fn a_function_remembers_the_line_it_starts_on() {
        assert_eq!(func("\n\nfunc f() { }").line(), 3);
    }

    #[test]
    fn a_struct_remembers_the_line_it_starts_on() {
        let Decl::Struct(s) = try_parse("\nstruct P { }").unwrap() else {
            panic!("expected a struct");
        };
        assert_eq!(s.line(), 2);
    }

    #[test]
    fn a_return_type_is_optional() {
        assert_eq!(func("func f() { }").ret(), None);
        assert_eq!(
            func("func f() -> Int { return 1 }").ret(),
            Some(&TypeRef::from("Int"))
        );
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
                declare("a", true, None, Some(int(1))),
                declare("b", false, Some("Int"), Some(int(2))),
                declare("c", true, Some("Int"), None),
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
                unary(UnaryOp::Neg, id("a")),
                unary(UnaryOp::Not, id("b")),
            )
        );
    }

    #[test]
    fn literals_cover_integers_booleans_and_strings() {
        assert_eq!(expr("true"), ex(ExprKind::Bool(true)));
        assert_eq!(expr("false"), ex(ExprKind::Bool(false)));
        assert_eq!(expr("\"hi\""), ex(ExprKind::Str("hi".to_string())));
    }

    #[test]
    fn calls_carry_optional_argument_labels() {
        assert_eq!(
            expr("P(x: 1, 2)"),
            ex(ExprKind::Call {
                callee: "P".to_string(),
                args: vec![
                    Arg::new(Some("x".to_string()), int(1)),
                    Arg::new(None, int(2))
                ],
            })
        );
    }

    #[test]
    fn member_access_chains_to_the_left() {
        assert_eq!(expr("a.b.c"), member(member(id("a"), "b"), "c"));
    }

    #[test]
    fn ranges_are_half_open_or_closed_and_bind_looser_than_addition() {
        assert_eq!(
            expr("0..<n + 1"),
            range(int(0), bin(BinOp::Add, id("n"), int(1)), false)
        );
        assert_eq!(expr("1...3"), range(int(1), int(3), true));
    }

    #[test]
    fn assignment_may_combine_with_an_arithmetic_operator() {
        assert_eq!(
            body("s = 1\ns += 2\np.x *= 3"),
            vec![
                assign(id("s"), None, int(1)),
                assign(id("s"), Some(BinOp::Add), int(2)),
                assign(member(id("p"), "x"), Some(BinOp::Mul), int(3)),
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
        let StmtKind::If {
            else_body: Some(outer_else),
            ..
        } = stmts[0].kind()
        else {
            panic!("expected an if with else");
        };
        let StmtKind::If {
            else_body: Some(inner_else),
            ..
        } = outer_else[0].kind()
        else {
            panic!("expected a nested if with else");
        };
        assert_eq!(inner_else, &vec![st(StmtKind::Return(Some(int(3))))]);
    }

    #[test]
    fn while_loops_take_a_condition_and_a_body() {
        assert_eq!(
            body("while a < 3 { a += 1 }"),
            vec![st(StmtKind::While {
                cond: bin(BinOp::Lt, id("a"), int(3)),
                body: vec![assign(id("a"), Some(BinOp::Add), int(1))],
            })]
        );
    }

    #[test]
    fn for_loops_iterate_a_variable_over_a_range() {
        assert_eq!(
            body("for k in 0..<3 { }"),
            vec![st(StmtKind::For {
                var: "k".to_string(),
                iter: range(int(0), int(3), false),
                body: vec![],
            })]
        );
    }

    #[test]
    fn return_takes_a_value_only_when_one_follows_on_the_same_line() {
        assert_eq!(body("return"), vec![st(StmtKind::Return(None))]);
        assert_eq!(
            body("return\n1"),
            vec![st(StmtKind::Return(None)), st(StmtKind::Expr(int(1)))]
        );
        assert_eq!(body("return 1"), vec![st(StmtKind::Return(Some(int(1))))]);
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
                declare("a", false, None, Some(int(1))),
                st(StmtKind::Expr(unary(UnaryOp::Neg, int(2)))),
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

    #[test]
    fn statements_remember_the_line_they_start_on() {
        let lines: Vec<u32> = body("a = 1\n\nb = 2").iter().map(Stmt::line).collect();
        assert_eq!(lines, vec![2, 4]);
    }

    #[test]
    fn expressions_remember_the_line_they_start_on() {
        let stmts = body("a = 1\nb = a + 2");
        let StmtKind::Assign { value, .. } = stmts[1].kind() else {
            panic!("expected an assignment");
        };
        let ExprKind::Binary { lhs, rhs, .. } = value.kind() else {
            panic!("expected a binary expression");
        };
        assert_eq!((value.line(), lhs.line(), rhs.line()), (3, 3, 3));
    }

    #[test]
    fn an_operand_may_continue_on_the_next_line_after_its_operator() {
        let stmts = body("let a = 1 +\n  2");
        let StmtKind::Let {
            init: Some(value), ..
        } = stmts[0].kind()
        else {
            panic!("expected a declaration");
        };
        let ExprKind::Binary { lhs, rhs, .. } = value.kind() else {
            panic!("expected a binary expression");
        };
        assert_eq!((lhs.line(), rhs.line()), (2, 3));
    }

    #[test]
    fn nodes_with_the_same_shape_are_equal_whatever_their_lines() {
        assert_eq!(
            Expr::new(1, ExprKind::Int(7)),
            Expr::new(9, ExprKind::Int(7))
        );
        assert_ne!(
            Expr::new(1, ExprKind::Int(7)),
            Expr::new(1, ExprKind::Int(8))
        );
    }

    #[test]
    fn a_signature_parse_ignores_the_body() {
        let decl =
            parse_signature(&lex("func f(a: Int) -> Bool { this is not code }").unwrap()).unwrap();
        match decl {
            Decl::Func(f) => {
                assert_eq!(f.name(), "f");
                assert_eq!(f.params().len(), 1);
                assert_eq!(f.ret(), Some(&TypeRef::from("Bool")));
                assert!(f.body().is_empty());
            }
            other => panic!("expected a func, got {other:?}"),
        }
    }

    #[test]
    fn a_signature_parse_of_a_struct_keeps_its_fields() {
        let decl = parse_signature(&lex("struct P { var x: Int }").unwrap()).unwrap();
        assert_eq!(
            decl,
            parse_item(&lex("struct P { var x: Int }").unwrap()).unwrap()
        );
    }

    #[test]
    fn an_enum_lists_its_cases_across_case_lines() {
        let decl = try_parse("enum Color {\n  case red, green\n  case blue\n}").unwrap();
        assert_eq!(
            decl,
            Decl::Enum(EnumDecl::new(
                1,
                "Color".to_string(),
                vec![
                    EnumCaseDecl::new("red".into(), vec![]),
                    EnumCaseDecl::new("green".into(), vec![]),
                    EnumCaseDecl::new("blue".into(), vec![]),
                ]
            ))
        );
    }

    #[test]
    fn an_enum_body_accepts_only_cases() {
        assert!(try_parse("enum Color { var x: Int }").is_err());
    }

    #[test]
    fn a_switch_has_case_labels_a_default_and_bodies() {
        let stmts = body("switch c {\ncase .red, Color.green:\nprint(1)\ndefault:\nprint(2)\n}");
        let StmtKind::Switch { cases, .. } = stmts[0].kind() else {
            panic!("expected a switch");
        };
        assert_eq!(cases.len(), 2);
        assert_eq!(
            *cases[0].label(),
            CaseLabel::Patterns(vec![
                Pattern::new(None, "red".into(), None),
                Pattern::new(Some("Color".into()), "green".into(), None),
            ])
        );
        assert_eq!(cases[0].body().len(), 1);
        assert_eq!(*cases[1].label(), CaseLabel::Default);
    }

    #[test]
    fn a_switch_case_may_have_an_empty_body() {
        let stmts = body("switch c {\ncase .red:\ndefault:\n}");
        let StmtKind::Switch { cases, .. } = stmts[0].kind() else {
            panic!("expected a switch");
        };
        assert!(cases[0].body().is_empty());
    }

    #[test]
    fn a_statement_directly_in_a_switch_body_is_an_error() {
        assert!(try_parse("func f() { switch c { print(1) } }").is_err());
    }

    #[test]
    fn enum_cases_may_carry_payload_types() {
        let decl = try_parse("enum Shape { case circle(Int), rect(Int, Int), none }").unwrap();
        let Decl::Enum(shape) = decl else {
            panic!("expected an enum");
        };
        assert!(shape.has_payload());
        assert_eq!(
            shape.cases()[1].payload(),
            [TypeRef::from("Int"), TypeRef::from("Int")]
        );
        assert!(shape.cases()[2].payload().is_empty());
    }

    #[test]
    fn a_pattern_binds_payload_values_or_ignores_them() {
        let stmts = body("switch s {\ncase .rect(let w, _):\nprint(w)\ndefault:\nprint(0)\n}");
        let StmtKind::Switch { cases, .. } = stmts[0].kind() else {
            panic!("expected a switch");
        };
        let CaseLabel::Patterns(patterns) = cases[0].label() else {
            panic!("expected patterns");
        };
        assert_eq!(
            patterns[0].bindings(),
            Some(&[Some("w".to_string()), None][..])
        );
    }

    #[test]
    fn calling_a_member_parses_as_a_member_call() {
        let call = expr("Shape.circle(5)");
        let ExprKind::MemberCall { name, args, .. } = call.kind() else {
            panic!("expected a member call");
        };
        assert_eq!(name, "circle");
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn declarations_list_their_type_parameters() {
        assert_eq!(
            func("func id<T, U>(_ x: T) -> T { return x }").generics(),
            ["T", "U"]
        );
        let Decl::Struct(boxed) = try_parse("struct Box<T> { var value: T }").unwrap() else {
            panic!("expected a struct");
        };
        assert_eq!(boxed.generics(), ["T"]);
        let Decl::Enum(maybe) = try_parse("enum Maybe<T> { case some(T), none }").unwrap() else {
            panic!("expected an enum");
        };
        assert_eq!(maybe.generics(), ["T"]);
    }

    #[test]
    fn a_duplicate_type_parameter_is_an_error() {
        assert!(try_parse("func f<T, T>() { }").is_err());
    }

    #[test]
    fn type_positions_accept_type_arguments_including_nested_ones() {
        let decl = func("func f(_ x: Box<Maybe<Int>>) -> Pair<Int, Bool> { }");
        assert_eq!(decl.params()[0].ty().to_string(), "Box<Maybe<Int>>");
        assert_eq!(decl.ret().unwrap().to_string(), "Pair<Int,Bool>");
    }

    #[test]
    fn a_name_with_type_arguments_before_a_call_or_member_is_generic() {
        let call = expr("Box<Int>(value: 1)");
        let ExprKind::GenericCall {
            callee, type_args, ..
        } = call.kind()
        else {
            panic!("expected a generic call");
        };
        assert_eq!(callee, "Box");
        assert_eq!(type_args.len(), 1);
        let member = expr("Maybe<Int>.none");
        let ExprKind::Member { base, .. } = member.kind() else {
            panic!("expected a member");
        };
        assert!(matches!(base.kind(), ExprKind::GenericName { .. }));
    }

    #[test]
    fn comparisons_are_not_mistaken_for_type_arguments() {
        let e = expr("a < b");
        assert!(matches!(e.kind(), ExprKind::Binary { .. }));
        let e = expr("a < b && c > (d)");
        assert!(matches!(e.kind(), ExprKind::Binary { .. }));
    }
}

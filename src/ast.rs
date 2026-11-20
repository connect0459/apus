pub type TypeName = String;

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Func(FuncDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
}

impl Decl {
    pub fn name(&self) -> &str {
        match self {
            Decl::Func(f) => f.name(),
            Decl::Struct(s) => s.name(),
            Decl::Enum(e) => e.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    line: u32,
    name: String,
    params: Vec<Param>,
    ret: Option<TypeName>,
    body: Vec<Stmt>,
}

impl FuncDecl {
    pub fn new(
        line: u32,
        name: String,
        params: Vec<Param>,
        ret: Option<TypeName>,
        body: Vec<Stmt>,
    ) -> Self {
        Self {
            line,
            name,
            params,
            ret,
            body,
        }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &[Param] {
        &self.params
    }

    pub fn ret(&self) -> Option<&str> {
        self.ret.as_deref()
    }

    pub fn body(&self) -> &[Stmt] {
        &self.body
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    label: Option<String>,
    name: String,
    ty: TypeName,
}

impl Param {
    pub fn new(label: Option<String>, name: String, ty: TypeName) -> Self {
        Self { label, name, ty }
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &str {
        &self.ty
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    line: u32,
    name: String,
    fields: Vec<Field>,
}

impl StructDecl {
    pub fn new(line: u32, name: String, fields: Vec<Field>) -> Self {
        Self { line, name, fields }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    line: u32,
    name: String,
    cases: Vec<String>,
}

impl EnumDecl {
    pub fn new(line: u32, name: String, cases: Vec<String>) -> Self {
        Self { line, name, cases }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cases(&self) -> &[String] {
        &self.cases
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    name: String,
    mutable: bool,
    ty: TypeName,
}

impl Field {
    pub fn new(name: String, mutable: bool, ty: TypeName) -> Self {
        Self { name, mutable, ty }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub fn ty(&self) -> &str {
        &self.ty
    }
}

#[derive(Debug, Clone)]
pub struct Stmt {
    line: u32,
    kind: StmtKind,
}

impl Stmt {
    pub fn new(line: u32, kind: StmtKind) -> Self {
        Self { line, kind }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn kind(&self) -> &StmtKind {
        &self.kind
    }
}

impl PartialEq for Stmt {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let {
        name: String,
        mutable: bool,
        ty: Option<TypeName>,
        init: Option<Expr>,
    },
    Assign {
        target: Expr,
        op: Option<BinOp>,
        value: Expr,
    },
    Return(Option<Expr>),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    Switch {
        subject: Expr,
        cases: Vec<SwitchCase>,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    line: u32,
    label: CaseLabel,
    body: Vec<Stmt>,
}

impl SwitchCase {
    pub fn new(line: u32, label: CaseLabel, body: Vec<Stmt>) -> Self {
        Self { line, label, body }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn label(&self) -> &CaseLabel {
        &self.label
    }

    pub fn body(&self) -> &[Stmt] {
        &self.body
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseLabel {
    Patterns(Vec<Pattern>),
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    type_name: Option<TypeName>,
    case: String,
}

impl Pattern {
    pub fn new(type_name: Option<TypeName>, case: String) -> Self {
        Self { type_name, case }
    }

    pub fn type_name(&self) -> Option<&str> {
        self.type_name.as_deref()
    }

    pub fn case(&self) -> &str {
        &self.case
    }
}

#[derive(Debug, Clone)]
pub struct Expr {
    line: u32,
    kind: ExprKind,
}

impl Expr {
    pub fn new(line: u32, kind: ExprKind) -> Self {
        Self { line, kind }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn kind(&self) -> &ExprKind {
        &self.kind
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Bool(bool),
    Str(String),
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Range {
        lo: Box<Expr>,
        hi: Box<Expr>,
        inclusive: bool,
    },
    Call {
        callee: String,
        args: Vec<Arg>,
    },
    Member {
        base: Box<Expr>,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    label: Option<String>,
    value: Expr,
}

impl Arg {
    pub fn new(label: Option<String>, value: Expr) -> Self {
        Self { label, value }
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn value(&self) -> &Expr {
        &self.value
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

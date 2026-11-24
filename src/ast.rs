#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    name: String,
    args: Vec<TypeRef>,
}

impl TypeRef {
    pub fn new(name: String, args: Vec<TypeRef>) -> Self {
        Self { name, args }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn args(&self) -> &[TypeRef] {
        &self.args
    }
}

impl From<&str> for TypeRef {
    fn from(name: &str) -> Self {
        Self::new(name.to_string(), Vec::new())
    }
}

impl From<String> for TypeRef {
    fn from(name: String) -> Self {
        Self::new(name, Vec::new())
    }
}

impl std::fmt::Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)?;
        if !self.args.is_empty() {
            f.write_str("<")?;
            for (index, arg) in self.args.iter().enumerate() {
                if index > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{arg}")?;
            }
            f.write_str(">")?;
        }
        Ok(())
    }
}

pub type TypeName = TypeRef;

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
    generics: Vec<String>,
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
            generics: Vec::new(),
            params,
            ret,
            body,
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
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

    pub fn ret(&self) -> Option<&TypeRef> {
        self.ret.as_ref()
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
    pub fn new(label: Option<String>, name: String, ty: impl Into<TypeRef>) -> Self {
        Self {
            label,
            name,
            ty: ty.into(),
        }
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &TypeRef {
        &self.ty
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    line: u32,
    name: String,
    generics: Vec<String>,
    fields: Vec<Field>,
    class: bool,
}

impl StructDecl {
    pub fn new(line: u32, name: String, fields: Vec<Field>) -> Self {
        Self {
            line,
            name,
            generics: Vec::new(),
            fields,
            class: false,
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn as_class(mut self) -> Self {
        self.class = true;
        self
    }

    pub fn is_class(&self) -> bool {
        self.class
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
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
pub struct EnumCaseDecl {
    name: String,
    payload: Vec<TypeRef>,
}

impl EnumCaseDecl {
    pub fn new(name: String, payload: Vec<TypeRef>) -> Self {
        Self { name, payload }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn payload(&self) -> &[TypeRef] {
        &self.payload
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    line: u32,
    name: String,
    generics: Vec<String>,
    cases: Vec<EnumCaseDecl>,
}

impl EnumDecl {
    pub fn new(line: u32, name: String, cases: Vec<EnumCaseDecl>) -> Self {
        Self {
            line,
            name,
            generics: Vec::new(),
            cases,
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
    }

    pub fn has_payload(&self) -> bool {
        self.cases.iter().any(|c| !c.payload.is_empty())
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cases(&self) -> &[EnumCaseDecl] {
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
    pub fn new(name: String, mutable: bool, ty: impl Into<TypeRef>) -> Self {
        Self {
            name,
            mutable,
            ty: ty.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub fn ty(&self) -> &TypeRef {
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
    type_name: Option<String>,
    case: String,
    bindings: Option<Vec<Option<String>>>,
}

impl Pattern {
    pub fn new(
        type_name: Option<String>,
        case: String,
        bindings: Option<Vec<Option<String>>>,
    ) -> Self {
        Self {
            type_name,
            case,
            bindings,
        }
    }

    pub fn bindings(&self) -> Option<&[Option<String>]> {
        self.bindings.as_deref()
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
    MemberCall {
        base: Box<Expr>,
        name: String,
        args: Vec<Arg>,
    },
    GenericName {
        name: String,
        type_args: Vec<TypeRef>,
    },
    GenericCall {
        callee: String,
        type_args: Vec<TypeRef>,
        args: Vec<Arg>,
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

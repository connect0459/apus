pub const MAX_TUPLE: usize = 5;

pub fn tuple_type_name(count: usize, labels: &[Option<String>]) -> String {
    if labels.iter().all(Option::is_none) {
        return format!("$Tuple{count}");
    }
    let shown: Vec<&str> = labels.iter().map(|l| l.as_deref().unwrap_or("")).collect();
    format!("$Tuple{count}({})", shown.join(","))
}

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
    Protocol(ProtocolDecl),
}

impl Decl {
    pub fn name(&self) -> &str {
        match self {
            Decl::Func(f) => f.name(),
            Decl::Struct(s) => s.name(),
            Decl::Enum(e) => e.name(),
            Decl::Protocol(p) => p.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    line: u32,
    name: String,
    generics: Vec<String>,
    bounds: Vec<Vec<String>>,
    params: Vec<Param>,
    ret: Option<TypeName>,
    body: Vec<Stmt>,
    owner: Option<String>,
    mutating: bool,
    overriding: bool,
    init: bool,
    deinit: bool,
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
            bounds: Vec::new(),
            params,
            ret,
            body,
            owner: None,
            mutating: false,
            overriding: false,
            init: false,
            deinit: false,
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn with_bounds(mut self, bounds: Vec<Vec<String>>) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn bound(&self, index: usize) -> &[String] {
        self.bounds.get(index).map_or(&[], Vec::as_slice)
    }

    pub fn as_mutating(mut self) -> Self {
        self.mutating = true;
        self
    }

    pub fn as_deinit(mut self) -> Self {
        self.deinit = true;
        self
    }

    pub fn as_override(mut self) -> Self {
        self.overriding = true;
        self
    }

    pub fn as_init(mut self) -> Self {
        self.init = true;
        self
    }

    pub fn with_owner(mut self, owner: &str) -> Self {
        self.name = if self.init {
            let shape: Vec<String> = self
                .params
                .iter()
                .map(|p| format!("{}:{}", p.label().unwrap_or("_"), p.ty()))
                .collect();
            format!("{owner}.init({})", shape.join(","))
        } else {
            format!("{owner}.{}", self.name)
        };
        self.owner = Some(owner.to_string());
        self
    }

    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    pub fn is_mutating(&self) -> bool {
        self.mutating
    }

    pub fn is_init(&self) -> bool {
        self.init
    }

    pub fn is_deinit(&self) -> bool {
        self.deinit
    }

    pub fn is_override(&self) -> bool {
        self.overriding
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
pub struct ProtocolDecl {
    line: u32,
    name: String,
    requirements: Vec<FuncDecl>,
    associated: Vec<String>,
}

impl ProtocolDecl {
    pub fn new(line: u32, name: String, requirements: Vec<FuncDecl>) -> Self {
        Self {
            line,
            name,
            requirements,
            associated: Vec::new(),
        }
    }

    pub fn with_associated(mut self, associated: Vec<String>) -> Self {
        self.associated = associated;
        self
    }

    pub fn associated(&self) -> &[String] {
        &self.associated
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn requirements(&self) -> &[FuncDecl] {
        &self.requirements
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    line: u32,
    name: String,
    generics: Vec<String>,
    bounds: Vec<Vec<String>>,
    fields: Vec<Field>,
    class: bool,
    superclass: Option<TypeRef>,
    conformances: Vec<String>,
    aliases: Vec<(String, TypeRef)>,
}

impl StructDecl {
    pub fn new(line: u32, name: String, fields: Vec<Field>) -> Self {
        Self {
            line,
            name,
            generics: Vec::new(),
            bounds: Vec::new(),
            fields,
            class: false,
            superclass: None,
            conformances: Vec::new(),
            aliases: Vec::new(),
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn with_bounds(mut self, bounds: Vec<Vec<String>>) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn bound(&self, index: usize) -> &[String] {
        self.bounds.get(index).map_or(&[], Vec::as_slice)
    }

    pub fn with_conformances(mut self, conformances: Vec<String>) -> Self {
        self.conformances = conformances;
        self
    }

    pub fn conformances(&self) -> &[String] {
        &self.conformances
    }

    pub fn with_aliases(mut self, aliases: Vec<(String, TypeRef)>) -> Self {
        self.aliases = aliases;
        self
    }

    pub fn aliases(&self) -> &[(String, TypeRef)] {
        &self.aliases
    }

    pub fn without_superclass(mut self) -> Self {
        self.superclass = None;
        self
    }

    pub fn as_class(mut self) -> Self {
        self.class = true;
        self
    }

    pub fn with_superclass(mut self, superclass: TypeRef) -> Self {
        self.superclass = Some(superclass);
        self
    }

    pub fn is_class(&self) -> bool {
        self.class
    }

    pub fn superclass(&self) -> Option<&TypeRef> {
        self.superclass.as_ref()
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
    labels: Vec<Option<String>>,
}

impl EnumCaseDecl {
    pub fn new(name: String, payload: Vec<TypeRef>) -> Self {
        let labels = vec![None; payload.len()];
        Self {
            name,
            payload,
            labels,
        }
    }

    pub fn with_labels(mut self, labels: Vec<Option<String>>) -> Self {
        self.labels = labels;
        self
    }

    pub fn labels(&self) -> &[Option<String>] {
        &self.labels
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
    bounds: Vec<Vec<String>>,
    cases: Vec<EnumCaseDecl>,
    conformances: Vec<String>,
    aliases: Vec<(String, TypeRef)>,
}

impl EnumDecl {
    pub fn new(line: u32, name: String, cases: Vec<EnumCaseDecl>) -> Self {
        Self {
            line,
            name,
            generics: Vec::new(),
            bounds: Vec::new(),
            cases,
            conformances: Vec::new(),
            aliases: Vec::new(),
        }
    }

    pub fn with_generics(mut self, generics: Vec<String>) -> Self {
        self.generics = generics;
        self
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
    }

    pub fn with_bounds(mut self, bounds: Vec<Vec<String>>) -> Self {
        self.bounds = bounds;
        self
    }

    pub fn bound(&self, index: usize) -> &[String] {
        self.bounds.get(index).map_or(&[], Vec::as_slice)
    }

    pub fn with_conformances(mut self, conformances: Vec<String>) -> Self {
        self.conformances = conformances;
        self
    }

    pub fn conformances(&self) -> &[String] {
        &self.conformances
    }

    pub fn with_aliases(mut self, aliases: Vec<(String, TypeRef)>) -> Self {
        self.aliases = aliases;
        self
    }

    pub fn aliases(&self) -> &[(String, TypeRef)] {
        &self.aliases
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
    unowned: bool,
    weak: bool,
}

impl Field {
    pub fn new(name: String, mutable: bool, ty: impl Into<TypeRef>) -> Self {
        Self {
            name,
            mutable,
            ty: ty.into(),
            unowned: false,
            weak: false,
        }
    }

    pub fn as_weak(mut self) -> Self {
        self.weak = true;
        self
    }

    pub fn is_weak(&self) -> bool {
        self.weak
    }

    pub fn as_unowned(mut self) -> Self {
        self.unowned = true;
        self
    }

    pub fn is_unowned(&self) -> bool {
        self.unowned
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
    Break(Option<String>),
    Continue(Option<String>),
    Labeled {
        label: String,
        stmt: Box<Stmt>,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    IfLet {
        name: String,
        mutable: bool,
        value: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    IfConds {
        conditions: Vec<Condition>,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    WhileConds {
        conditions: Vec<Condition>,
        body: Vec<Stmt>,
    },
    GuardConds {
        conditions: Vec<Condition>,
        else_body: Vec<Stmt>,
    },
    IfCase {
        label: CaseLabel,
        subject: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    WhileLet {
        name: String,
        mutable: bool,
        value: Expr,
        body: Vec<Stmt>,
    },
    Guard {
        cond: Expr,
        else_body: Vec<Stmt>,
    },
    GuardLet {
        name: String,
        mutable: bool,
        value: Expr,
        else_body: Vec<Stmt>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    ForPair {
        key: String,
        value: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    LocalFunc(Box<FuncDecl>),
    LetTuple {
        names: Vec<Option<String>>,
        mutable: bool,
        init: Expr,
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
    guard: Option<Expr>,
    body: Vec<Stmt>,
}

impl SwitchCase {
    pub fn new(line: u32, label: CaseLabel, body: Vec<Stmt>) -> Self {
        Self {
            line,
            label,
            guard: None,
            body,
        }
    }

    pub fn with_guard(mut self, guard: Option<Expr>) -> Self {
        self.guard = guard;
        self
    }

    pub fn guard(&self) -> Option<&Expr> {
        self.guard.as_ref()
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
pub enum Condition {
    Expr(Expr),
    Let {
        name: String,
        mutable: bool,
        value: Expr,
    },
    Case {
        label: CaseLabel,
        subject: Expr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TuplePart {
    Wild,
    Bind(String),
    Value(Expr),
    Tuple(Vec<TuplePart>),
    Enum(Pattern),
}

impl TuplePart {
    pub fn leaves(alternatives: &[Vec<TuplePart>]) -> Vec<&TuplePart> {
        let mut found = Vec::new();
        for part in alternatives.iter().flatten() {
            part.collect_leaves(&mut found);
        }
        found
    }

    fn collect_leaves<'a>(&'a self, found: &mut Vec<&'a TuplePart>) {
        match self {
            TuplePart::Tuple(parts) => parts.iter().for_each(|part| part.collect_leaves(found)),
            leaf => found.push(leaf),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseLabel {
    Patterns(Vec<Pattern>),
    Values(Vec<Expr>),
    Tuples(Vec<Vec<TuplePart>>),
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    type_name: Option<String>,
    case: String,
    bindings: Option<Vec<Option<String>>>,
    labels: Vec<Option<String>>,
}

impl Pattern {
    pub fn new(
        type_name: Option<String>,
        case: String,
        bindings: Option<Vec<Option<String>>>,
    ) -> Self {
        let labels = vec![None; bindings.as_ref().map_or(0, Vec::len)];
        Self {
            type_name,
            case,
            bindings,
            labels,
        }
    }

    pub fn with_labels(mut self, labels: Vec<Option<String>>) -> Self {
        self.labels = labels;
        self
    }

    pub fn labels(&self) -> &[Option<String>] {
        &self.labels
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
    depth: u32,
}

fn depth_of(kind: &ExprKind) -> u32 {
    let deepest = |args: &[Arg]| args.iter().map(|a| a.value().depth).max().unwrap_or(0);
    1 + match kind {
        ExprKind::Nil
        | ExprKind::Closure(_)
        | ExprKind::ChainValue
        | ExprKind::Int(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Ident(_)
        | ExprKind::GenericName { .. } => 0,
        ExprKind::ImplicitMember { args, .. } => args.as_deref().map_or(0, deepest),
        ExprKind::ArrayLiteral(items) | ExprKind::Tuple(items, _) => {
            items.iter().map(|e| e.depth).max().unwrap_or(0)
        }
        ExprKind::DictLiteral(pairs) => pairs
            .iter()
            .map(|(k, v)| k.depth.max(v.depth))
            .max()
            .unwrap_or(0),
        ExprKind::Index { base, index } => base.depth.max(index.depth),
        ExprKind::IndexDefault {
            base,
            index,
            default,
        } => base.depth.max(index.depth).max(default.depth),
        ExprKind::Unwrap(expr)
        | ExprKind::Unary { expr, .. }
        | ExprKind::Cast { expr, .. }
        | ExprKind::Member { base: expr, .. } => expr.depth,
        ExprKind::Coalesce { lhs, rhs } | ExprKind::Binary { lhs, rhs, .. } => {
            lhs.depth.max(rhs.depth)
        }
        ExprKind::OptionalChain { subject, link } => subject.depth.max(link.depth),
        ExprKind::Range { lo, hi, .. } => lo.depth.max(hi.depth),
        ExprKind::Call { args, .. }
        | ExprKind::GenericCall { args, .. }
        | ExprKind::TypeCall { args, .. } => deepest(args),
        ExprKind::Apply { func, args } => func.depth.max(deepest(args)),
        ExprKind::MemberCall { base, args, .. }
        | ExprKind::GenericMemberCall { base, args, .. } => base.depth.max(deepest(args)),
    }
}

impl Expr {
    pub fn new(line: u32, kind: ExprKind) -> Self {
        let depth = depth_of(&kind);
        Self { line, kind, depth }
    }

    pub fn depth(&self) -> usize {
        self.depth as usize
    }

    pub fn into_kind(self) -> ExprKind {
        self.kind
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
pub struct ClosureParam {
    name: String,
    ty: Option<TypeRef>,
}

impl ClosureParam {
    pub fn new(name: String, ty: Option<TypeRef>) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> Option<&TypeRef> {
        self.ty.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    Strong,
    Weak,
    Unowned,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaptureEntry {
    name: String,
    kind: CaptureKind,
    value: Option<Expr>,
}

impl CaptureEntry {
    pub fn new(name: String, kind: CaptureKind) -> Self {
        Self {
            name,
            kind,
            value: None,
        }
    }

    pub fn with_value(mut self, value: Expr) -> Self {
        self.value = Some(value);
        self
    }

    pub fn value(&self) -> Option<&Expr> {
        self.value.as_ref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> CaptureKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosureExpr {
    captures: Vec<CaptureEntry>,
    params: Option<Vec<ClosureParam>>,
    ret: Option<TypeRef>,
    body: Vec<Stmt>,
}

impl ClosureExpr {
    pub fn new(params: Option<Vec<ClosureParam>>, ret: Option<TypeRef>, body: Vec<Stmt>) -> Self {
        Self {
            captures: Vec::new(),
            params,
            ret,
            body,
        }
    }

    pub fn with_captures(mut self, captures: Vec<CaptureEntry>) -> Self {
        self.captures = captures;
        self
    }

    pub fn captures(&self) -> &[CaptureEntry] {
        &self.captures
    }

    pub fn params(&self) -> Option<&[ClosureParam]> {
        self.params.as_deref()
    }

    pub fn ret(&self) -> Option<&TypeRef> {
        self.ret.as_ref()
    }

    pub fn body(&self) -> &[Stmt] {
        &self.body
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Nil,
    ArrayLiteral(Vec<Expr>),
    DictLiteral(Vec<(Expr, Expr)>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    IndexDefault {
        base: Box<Expr>,
        index: Box<Expr>,
        default: Box<Expr>,
    },
    Unwrap(Box<Expr>),
    Coalesce {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: TypeRef,
        forced: bool,
        checked: bool,
    },
    OptionalChain {
        subject: Box<Expr>,
        link: Box<Expr>,
    },
    ChainValue,
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
    ImplicitMember {
        name: String,
        args: Option<Vec<Arg>>,
    },
    GenericMemberCall {
        base: Box<Expr>,
        name: String,
        type_args: Vec<TypeRef>,
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
    TypeCall {
        ty: TypeRef,
        args: Vec<Arg>,
    },
    Apply {
        func: Box<Expr>,
        args: Vec<Arg>,
    },
    Tuple(Vec<Expr>, Vec<Option<String>>),
    Closure(Box<ClosureExpr>),
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
    Is,
    IsNot,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

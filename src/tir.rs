use crate::ast::{BinOp, UnaryOp};
use crate::types::{StructId, Ty};

#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    name: String,
    class: StructId,
    drop: String,
    parent: Option<Box<Descriptor>>,
    methods: Vec<String>,
}

impl Descriptor {
    pub fn new(
        name: String,
        class: StructId,
        drop: String,
        parent: Option<Box<Descriptor>>,
        methods: Vec<String>,
    ) -> Self {
        Self {
            name,
            class,
            drop,
            parent,
            methods,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn class(&self) -> StructId {
        self.class
    }

    pub fn drop(&self) -> &str {
        &self.drop
    }

    pub fn parent(&self) -> Option<&Descriptor> {
        self.parent.as_deref()
    }

    pub fn methods(&self) -> &[String] {
        &self.methods
    }

    pub fn chain(&self) -> Vec<&Descriptor> {
        let mut found = vec![self];
        let mut current = self.parent();
        while let Some(parent) = current {
            found.push(parent);
            current = parent.parent();
        }
        found
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrOp {
    Concat,
    Count,
    IsEmpty,
    FromInt,
    HasPrefix,
    HasSuffix,
    Contains,
}

impl StrOp {
    pub fn result(self) -> Ty {
        match self {
            StrOp::Concat | StrOp::FromInt => Ty::Str,
            StrOp::Count => Ty::Int,
            StrOp::IsEmpty | StrOp::HasPrefix | StrOp::HasSuffix | StrOp::Contains => Ty::Bool,
        }
    }

    pub fn allocates(self) -> bool {
        matches!(self, StrOp::Concat | StrOp::FromInt)
    }

    pub fn code(self) -> u8 {
        match self {
            StrOp::Concat => 1,
            StrOp::Count => 2,
            StrOp::IsEmpty => 3,
            StrOp::FromInt => 4,
            StrOp::HasPrefix => 5,
            StrOp::HasSuffix => 6,
            StrOp::Contains => 7,
        }
    }
}

pub const TAG_SLOT: usize = 0;
pub const RC_SLOT: usize = 1;
pub const DESC_SLOT: usize = 2;
pub const DESC_PARENT: usize = 1;
pub const DESC_METHODS: usize = 2;
pub const UNOWNED_SLOT: usize = 3;
pub const HEADER_SLOTS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalId(usize);

impl LocalId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TStruct {
    name: String,
    fields: Vec<Ty>,
    starts: Vec<usize>,
}

impl TStruct {
    pub fn new(name: String, fields: Vec<Ty>, starts: Vec<usize>) -> Self {
        Self {
            name,
            fields,
            starts,
        }
    }

    pub fn starts(&self) -> &[usize] {
        &self.starts
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[Ty] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TFunc {
    name: String,
    param_count: usize,
    ret: Ty,
    locals: Vec<Ty>,
    body: Vec<TStmt>,
}

impl TFunc {
    pub fn new(
        name: String,
        param_count: usize,
        ret: Ty,
        locals: Vec<Ty>,
        body: Vec<TStmt>,
    ) -> Self {
        Self {
            name,
            param_count,
            ret,
            locals,
            body,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn param_types(&self) -> &[Ty] {
        &self.locals[..self.param_count]
    }

    pub fn ret(&self) -> Ty {
        self.ret
    }

    pub fn locals(&self) -> &[Ty] {
        &self.locals
    }

    pub fn body(&self) -> &[TStmt] {
        &self.body
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TStmt {
    Let {
        local: LocalId,
        init: TExpr,
    },
    Assign {
        local: LocalId,
        value: TExpr,
    },
    AssignField {
        local: LocalId,
        index: usize,
        value: TExpr,
    },
    AssignPath {
        base: TExpr,
        index: usize,
        value: TExpr,
    },
    Return(Option<TExpr>),
    Break(usize),
    Continue(usize),
    Block(Vec<TStmt>),
    If {
        cond: TExpr,
        then_body: Vec<TStmt>,
        else_body: Vec<TStmt>,
    },
    While {
        cond: TExpr,
        body: Vec<TStmt>,
    },
    ForRange {
        local: LocalId,
        lo: TExpr,
        hi: TExpr,
        inclusive: bool,
        body: Vec<TStmt>,
    },
    Print(TExpr),
    Fatal(String),
    Eval(TExpr),
    Release(TExpr),
    Clear(LocalId),
    Free(TExpr),
    ReleaseUnowned(TExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TExpr {
    Int(i64),
    Bool(bool),
    Str(String),
    Local(LocalId, Ty),
    Unary {
        op: UnaryOp,
        expr: Box<TExpr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<TExpr>,
        rhs: Box<TExpr>,
    },
    Call {
        callee: String,
        type_args: Vec<Ty>,
        args: Vec<TExpr>,
        ret: Ty,
    },
    Virtual {
        cases: Vec<(i64, String)>,
        default: String,
        args: Vec<TExpr>,
        ret: Ty,
    },
    StructInit {
        ty: Ty,
        args: Vec<TExpr>,
    },
    New {
        ty: Ty,
        args: Vec<TExpr>,
    },
    Field {
        base: Box<TExpr>,
        index: usize,
        ty: Ty,
    },
    EnumCase {
        ty: Ty,
        index: usize,
    },
    Zero(Ty),
    Retain(Box<TExpr>),
    Moved(Box<TExpr>),
    Bind {
        local: LocalId,
        init: Box<TExpr>,
    },
    FuncAddr(String),
    ClassDesc(Ty),
    Descriptor(Box<Descriptor>),
    VCall {
        slot: usize,
        args: Vec<TExpr>,
        ret: Ty,
    },
    InstanceOf {
        object: Box<TExpr>,
        class: Box<TExpr>,
    },
    StrOp {
        op: StrOp,
        args: Vec<TExpr>,
    },
    Witness {
        protocol: String,
        method: String,
        self_ty: Ty,
        args: Vec<TExpr>,
        ret: Ty,
    },
    Dispatch {
        protocol: String,
        method: String,
        args: Vec<TExpr>,
        ret: Ty,
    },
    Select {
        cond: Box<TExpr>,
        then: Box<TExpr>,
        otherwise: Box<TExpr>,
        ty: Ty,
    },
    Unwrap {
        base: Box<TExpr>,
        ty: Ty,
    },
    RetainUnowned(Box<TExpr>),
    LoadUnowned(Box<TExpr>),
    RetainWeak(Box<TExpr>),
    LoadWeak(Box<TExpr>),
}

impl TExpr {
    pub fn ty(&self) -> Ty {
        match self {
            TExpr::Int(_) => Ty::Int,
            TExpr::Bool(_) => Ty::Bool,
            TExpr::Str(_) => Ty::Str,
            TExpr::Local(_, ty) => *ty,
            TExpr::Unary { expr, .. } => expr.ty(),
            TExpr::Binary { op, .. } => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => Ty::Int,
                _ => Ty::Bool,
            },
            TExpr::Call { ret, .. }
            | TExpr::Virtual { ret, .. }
            | TExpr::Witness { ret, .. }
            | TExpr::VCall { ret, .. }
            | TExpr::Dispatch { ret, .. } => *ret,
            TExpr::Retain(inner)
            | TExpr::Moved(inner)
            | TExpr::RetainUnowned(inner)
            | TExpr::LoadUnowned(inner)
            | TExpr::RetainWeak(inner)
            | TExpr::LoadWeak(inner) => inner.ty(),
            TExpr::Bind { init, .. } => init.ty(),
            TExpr::FuncAddr(_) | TExpr::ClassDesc(_) | TExpr::Descriptor(_) => Ty::Int,
            TExpr::InstanceOf { .. } => Ty::Bool,
            TExpr::StrOp { op, .. } => op.result(),
            TExpr::Unwrap { ty, .. } | TExpr::Select { ty, .. } => *ty,
            TExpr::StructInit { ty, .. }
            | TExpr::New { ty, .. }
            | TExpr::Field { ty, .. }
            | TExpr::EnumCase { ty, .. }
            | TExpr::Zero(ty) => *ty,
        }
    }
}

type TyMap<'a, E> = &'a mut dyn FnMut(Ty) -> Result<Ty, E>;
type CallMap<'a, E> = &'a mut dyn FnMut(String, Vec<Ty>) -> Result<(String, Vec<Ty>), E>;
type WitnessMap<'a, E> = &'a mut dyn FnMut(&str, &str, Ty) -> Result<Witnessed, E>;
type DispatchMap<'a, E> = &'a mut dyn FnMut(&str, &str) -> Result<Option<Vec<(i64, String)>>, E>;
type DescribeMap<'a, E> = &'a mut dyn FnMut(Ty) -> Result<Option<Box<Descriptor>>, E>;

#[derive(Debug, Clone, PartialEq)]
pub enum Witnessed {
    Keep,
    Call { callee: String, type_args: Vec<Ty> },
    Operator(BinOp),
    Slot(usize),
}

struct Rewriter<'a, E> {
    ty: TyMap<'a, E>,
    call: CallMap<'a, E>,
    witness: WitnessMap<'a, E>,
    dispatch: DispatchMap<'a, E>,
    describe: DescribeMap<'a, E>,
}

impl TFunc {
    pub fn rewrite<E>(
        &self,
        name: String,
        ty: TyMap<'_, E>,
        call: CallMap<'_, E>,
        witness: WitnessMap<'_, E>,
        dispatch: DispatchMap<'_, E>,
        describe: DescribeMap<'_, E>,
    ) -> Result<TFunc, E> {
        let mut rewriter = Rewriter {
            ty,
            call,
            witness,
            dispatch,
            describe,
        };
        let ret = (rewriter.ty)(self.ret)?;
        let locals = self
            .locals
            .iter()
            .map(|t| (rewriter.ty)(*t))
            .collect::<Result<Vec<_>, E>>()?;
        let body = rewriter.stmts(&self.body)?;
        Ok(TFunc::new(name, self.param_count, ret, locals, body))
    }
}

impl<E> Rewriter<'_, E> {
    fn stmts(&mut self, stmts: &[TStmt]) -> Result<Vec<TStmt>, E> {
        stmts.iter().map(|s| self.stmt(s)).collect()
    }

    fn stmt(&mut self, stmt: &TStmt) -> Result<TStmt, E> {
        Ok(match stmt {
            TStmt::Let { local, init } => TStmt::Let {
                local: *local,
                init: self.expr(init)?,
            },
            TStmt::Assign { local, value } => TStmt::Assign {
                local: *local,
                value: self.expr(value)?,
            },
            TStmt::AssignField {
                local,
                index,
                value,
            } => TStmt::AssignField {
                local: *local,
                index: *index,
                value: self.expr(value)?,
            },
            TStmt::AssignPath { base, index, value } => TStmt::AssignPath {
                base: self.expr(base)?,
                index: *index,
                value: self.expr(value)?,
            },
            TStmt::Return(value) => TStmt::Return(match value {
                Some(v) => Some(self.expr(v)?),
                None => None,
            }),
            TStmt::If {
                cond,
                then_body,
                else_body,
            } => TStmt::If {
                cond: self.expr(cond)?,
                then_body: self.stmts(then_body)?,
                else_body: self.stmts(else_body)?,
            },
            TStmt::Break(depth) => TStmt::Break(*depth),
            TStmt::Continue(depth) => TStmt::Continue(*depth),
            TStmt::Block(body) => TStmt::Block(self.stmts(body)?),
            TStmt::While { cond, body } => TStmt::While {
                cond: self.expr(cond)?,
                body: self.stmts(body)?,
            },
            TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => TStmt::ForRange {
                local: *local,
                lo: self.expr(lo)?,
                hi: self.expr(hi)?,
                inclusive: *inclusive,
                body: self.stmts(body)?,
            },
            TStmt::Print(expr) => TStmt::Print(self.expr(expr)?),
            TStmt::Fatal(text) => TStmt::Fatal(text.clone()),
            TStmt::Eval(expr) => TStmt::Eval(self.expr(expr)?),
            TStmt::Release(expr) => TStmt::Release(self.expr(expr)?),
            TStmt::Clear(local) => TStmt::Clear(*local),
            TStmt::Free(expr) => TStmt::Free(self.expr(expr)?),
            TStmt::ReleaseUnowned(expr) => TStmt::ReleaseUnowned(self.expr(expr)?),
        })
    }

    #[inline(never)]
    fn witness_expr(
        &mut self,
        protocol: &str,
        method: &str,
        self_ty: Ty,
        args: &[TExpr],
        ret: Ty,
    ) -> Result<TExpr, E> {
        let self_ty = (self.ty)(self_ty)?;
        let ret = (self.ty)(ret)?;
        let mut args = self.exprs(args)?;
        Ok(match (self.witness)(protocol, method, self_ty)? {
            Witnessed::Keep => TExpr::Witness {
                protocol: protocol.to_string(),
                method: method.to_string(),
                self_ty,
                args,
                ret,
            },
            Witnessed::Call { callee, type_args } => {
                let (callee, type_args) = (self.call)(callee, type_args)?;
                TExpr::Call {
                    callee,
                    type_args,
                    args,
                    ret,
                }
            }
            Witnessed::Operator(op) => {
                let rhs = args.pop();
                let lhs = args.pop();
                match (lhs, rhs) {
                    (Some(lhs), Some(rhs)) => TExpr::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    _ => TExpr::Bool(false),
                }
            }
            Witnessed::Slot(slot) => TExpr::VCall { slot, args, ret },
        })
    }

    #[inline(never)]
    fn dispatch_expr(
        &mut self,
        protocol: &str,
        method: &str,
        args: &[TExpr],
        ret: Ty,
    ) -> Result<TExpr, E> {
        let ret = (self.ty)(ret)?;
        let args = self.exprs(args)?;
        let keep = |args: Vec<TExpr>| TExpr::Dispatch {
            protocol: protocol.to_string(),
            method: method.to_string(),
            args,
            ret,
        };
        Ok(match (self.dispatch)(protocol, method)? {
            None => keep(args),
            Some(mut cases) => {
                let Some((_, last)) = cases.pop() else {
                    return Ok(keep(args));
                };
                let mut renamed = Vec::with_capacity(cases.len());
                for (tag, callee) in cases {
                    renamed.push((tag, (self.call)(callee, Vec::new())?.0));
                }
                TExpr::Virtual {
                    cases: renamed,
                    default: (self.call)(last, Vec::new())?.0,
                    args,
                    ret,
                }
            }
        })
    }

    fn exprs(&mut self, exprs: &[TExpr]) -> Result<Vec<TExpr>, E> {
        exprs.iter().map(|e| self.expr(e)).collect()
    }

    fn expr(&mut self, expr: &TExpr) -> Result<TExpr, E> {
        Ok(match expr {
            TExpr::Int(_) | TExpr::Bool(_) | TExpr::Str(_) => expr.clone(),
            TExpr::Local(local, ty) => TExpr::Local(*local, (self.ty)(*ty)?),
            TExpr::Unary { op, expr } => TExpr::Unary {
                op: *op,
                expr: Box::new(self.expr(expr)?),
            },
            TExpr::Binary { op, lhs, rhs } => TExpr::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs)?),
                rhs: Box::new(self.expr(rhs)?),
            },
            TExpr::Call {
                callee,
                type_args,
                args,
                ret,
            } => {
                let type_args = type_args
                    .iter()
                    .map(|t| (self.ty)(*t))
                    .collect::<Result<Vec<_>, E>>()?;
                let (callee, type_args) = (self.call)(callee.clone(), type_args)?;
                TExpr::Call {
                    callee,
                    type_args,
                    args: self.exprs(args)?,
                    ret: (self.ty)(*ret)?,
                }
            }
            TExpr::Virtual {
                cases,
                default,
                args,
                ret,
            } => {
                let mut renamed = Vec::with_capacity(cases.len());
                for (tag, callee) in cases {
                    renamed.push((*tag, (self.call)(callee.clone(), Vec::new())?.0));
                }
                TExpr::Virtual {
                    cases: renamed,
                    default: (self.call)(default.clone(), Vec::new())?.0,
                    args: self.exprs(args)?,
                    ret: (self.ty)(*ret)?,
                }
            }
            TExpr::Witness {
                protocol,
                method,
                self_ty,
                args,
                ret,
            } => self.witness_expr(protocol, method, *self_ty, args, *ret)?,
            TExpr::Dispatch {
                protocol,
                method,
                args,
                ret,
            } => self.dispatch_expr(protocol, method, args, *ret)?,
            TExpr::StructInit { ty, args } => TExpr::StructInit {
                ty: (self.ty)(*ty)?,
                args: self.exprs(args)?,
            },
            TExpr::New { ty, args } => {
                let ty = (self.ty)(*ty)?;
                let mut args = self.exprs(args)?;
                if let Some(descriptor) = (self.describe)(ty)?
                    && let Some(slot) = args.get_mut(DESC_SLOT)
                {
                    *slot = TExpr::Descriptor(descriptor);
                }
                TExpr::New { ty, args }
            }
            TExpr::ClassDesc(ty) => {
                let ty = (self.ty)(*ty)?;
                match (self.describe)(ty)? {
                    Some(descriptor) => TExpr::Descriptor(descriptor),
                    None => TExpr::ClassDesc(ty),
                }
            }
            TExpr::Descriptor(_) => expr.clone(),
            TExpr::VCall { slot, args, ret } => TExpr::VCall {
                slot: *slot,
                args: self.exprs(args)?,
                ret: (self.ty)(*ret)?,
            },
            TExpr::InstanceOf { object, class } => TExpr::InstanceOf {
                object: Box::new(self.expr(object)?),
                class: Box::new(self.expr(class)?),
            },
            TExpr::Field { base, index, ty } => TExpr::Field {
                base: Box::new(self.expr(base)?),
                index: *index,
                ty: (self.ty)(*ty)?,
            },
            TExpr::EnumCase { ty, index } => TExpr::EnumCase {
                ty: (self.ty)(*ty)?,
                index: *index,
            },
            TExpr::Zero(ty) => TExpr::Zero((self.ty)(*ty)?),
            TExpr::Retain(inner) => TExpr::Retain(Box::new(self.expr(inner)?)),
            TExpr::Moved(inner) => TExpr::Moved(Box::new(self.expr(inner)?)),
            TExpr::Bind { local, init } => TExpr::Bind {
                local: *local,
                init: Box::new(self.expr(init)?),
            },
            TExpr::FuncAddr(_) => expr.clone(),
            TExpr::StrOp { op, args } => TExpr::StrOp {
                op: *op,
                args: self.exprs(args)?,
            },
            TExpr::Select {
                cond,
                then,
                otherwise,
                ty,
            } => TExpr::Select {
                cond: Box::new(self.expr(cond)?),
                then: Box::new(self.expr(then)?),
                otherwise: Box::new(self.expr(otherwise)?),
                ty: (self.ty)(*ty)?,
            },
            TExpr::Unwrap { base, ty } => TExpr::Unwrap {
                base: Box::new(self.expr(base)?),
                ty: (self.ty)(*ty)?,
            },
            TExpr::RetainUnowned(inner) => TExpr::RetainUnowned(Box::new(self.expr(inner)?)),
            TExpr::LoadUnowned(inner) => TExpr::LoadUnowned(Box::new(self.expr(inner)?)),
            TExpr::RetainWeak(inner) => TExpr::RetainWeak(Box::new(self.expr(inner)?)),
            TExpr::LoadWeak(inner) => TExpr::LoadWeak(Box::new(self.expr(inner)?)),
        })
    }
}

use crate::ast::{BinOp, UnaryOp};
use crate::types::Ty;

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
}

impl TStruct {
    pub fn new(name: String, fields: Vec<Ty>) -> Self {
        Self { name, fields }
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
    Return(Option<TExpr>),
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
    Eval(TExpr),
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
        args: Vec<TExpr>,
        ret: Ty,
    },
    StructInit {
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
            TExpr::Call { ret, .. } => *ret,
            TExpr::StructInit { ty, .. } | TExpr::Field { ty, .. } | TExpr::EnumCase { ty, .. } => {
                *ty
            }
        }
    }
}

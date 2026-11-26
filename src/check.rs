use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{
    Arg, BinOp, CaseLabel, ClosureExpr, Expr, ExprKind, FuncDecl, Pattern, Stmt, StmtKind,
    SwitchCase, TypeRef, UnaryOp,
};
use crate::init_check::{InitScope, check_enum_initialization, check_initialization};
use crate::sigs::{
    ARRAY, ARRAY_COUNT, COMPARABLE, CONTAINS, CResult, DICT_COUNT, DICT_USED, DICTIONARY, EQUAL,
    EQUATABLE, EnumSig, FATAL_ERROR, FieldSig, FuncSig, HASH, JOINED, KEYS, NONE_CASE, PRINT,
    ParamSig, REPEATING, SET, SOME_CASE, SPLIT, TO_SET, VALUES, bound_includes, scope_of,
};
use crate::tir::{ArrayOp, DictOp, LocalId, StrOp, TExpr, TFunc, TStmt};
use crate::types::{StructId, Ty};

const SUPER: &str = "super";

pub use crate::sigs::{CheckError, Signatures};

pub fn check_func(func: &FuncDecl, sigs: &Signatures) -> CResult<TFunc> {
    let sig = sigs.func(func.name()).ok_or_else(|| {
        CheckError::at(
            func.line(),
            format!("function '{}' has no signature", func.name()),
        )
    })?;
    let mut cx = FnCx {
        sigs,
        scope: sigs.member_scope(func.owner(), sig.generics(), sig.bounds(), func.line())?,
        expected: None,
        ret: sig.ret(),
        locals: Vec::new(),
        scopes: vec![HashMap::new()],
        init_self: None,
        cell_self: None,
        cell_call: CellCall::default(),
        super_initialized: false,
        chain_values: Vec::new(),
        targets: Vec::new(),
        pending_label: None,
        bounds: sig.bounds().to_vec(),
        name: func.name().to_string(),
        owner: func.name().to_string(),
        counter: Rc::new(Cell::new(0)),
        captured: Vec::new(),
        environment: Vec::new(),
        labels: HashMap::new(),
    };
    let receiver = sig.has_receiver();
    let mut cell_enum = None;
    if let Some(self_sig) = sig.params().first().filter(|_| receiver) {
        match cx.sigs.cell_enum(self_sig.ty()) {
            Some(enum_ty) => {
                let cell = cx.declare(func.line(), "self$cell", self_sig.ty(), false)?;
                cell_enum = Some((cell, enum_ty));
            }
            None => {
                cx.declare(func.line(), "self", self_sig.ty(), sig.is_mutating())?;
            }
        }
    }
    for (param, param_sig) in func
        .params()
        .iter()
        .zip(sig.params().iter().skip(usize::from(receiver)))
    {
        cx.declare(func.line(), param.name(), param_sig.ty(), false)?;
    }
    let prelude = if func.is_init() {
        init_prelude(&mut cx, func, sig.ret())?
    } else if let Some((cell, enum_ty)) = cell_enum {
        let value = cx.declare(func.line(), "self", enum_ty, true)?;
        cx.cell_self = Some(CellSelf {
            cell,
            value,
            ty: enum_ty,
        });
        vec![TStmt::Let {
            local: value,
            init: TExpr::Field {
                base: Box::new(TExpr::Local(cell, cx.locals[cell.index()])),
                index: 0,
                ty: enum_ty,
            },
        }]
    } else {
        Vec::new()
    };
    let mut body = cx.block(func.body())?;
    if let Some(cell_self) = cx.cell_self {
        body.splice(0..0, prelude);
        body.push(cell_self.write_back());
    } else if let Some(local) = cx.init_self {
        body.splice(0..0, prelude);
        body.push(TStmt::Return(Some(TExpr::Local(local, sig.ret()))));
    }
    if sig.ret() != Ty::Void && !func.is_init() && !always_returns(func.body()) {
        return Err(CheckError::at(
            func.line(),
            format!(
                "function '{}' must return a value on every path",
                func.name()
            ),
        ));
    }
    let typed = TFunc::new(
        func.name().to_string(),
        func.params().len() + usize::from(receiver),
        sig.ret(),
        cx.locals,
        body,
    );
    Ok(crate::capture::lower(&typed, &cx.captured, &[], sigs))
}

impl CellSelf {
    fn write_back(self) -> TStmt {
        TStmt::AssignField {
            local: self.cell,
            index: 0,
            value: TExpr::Local(self.value, self.ty),
        }
    }
}

fn init_prelude(cx: &mut FnCx, func: &FuncDecl, ty: Ty) -> CResult<Vec<TStmt>> {
    let methods = cx.sigs.method_names(ty);
    let methods: Vec<&str> = methods.iter().map(String::as_str).collect();
    let params: Vec<&str> = func.params().iter().map(|p| p.name()).collect();
    let init = if cx.sigs.is_enum_ty(ty) {
        let scope = InitScope::new(&[], &methods, &params);
        check_enum_initialization(func.body(), scope, func.line())
            .map_err(|violation| CheckError::at(violation.line(), violation.message()))?;
        match ty {
            Ty::Struct(id) => zero_struct(cx, func, id, ty)?,
            _ => TExpr::Zero(ty),
        }
    } else {
        let (Ty::Struct(id) | Ty::Class(id)) = ty else {
            return Err(CheckError::at(
                func.line(),
                "internal error: an initializer needs a struct, a class or an enum",
            ));
        };
        let sig = cx
            .sigs
            .struct_sig(id)
            .ok_or_else(|| CheckError::at(func.line(), "internal error: unknown struct"))?;
        let members: Vec<&str> = sig.members().map(|(_, f)| f.name()).collect();
        let parent = cx.sigs.superclass(ty);
        let inherited = match parent {
            Some(Ty::Class(parent)) => cx
                .sigs
                .struct_sig(parent)
                .map_or(0, |s| s.members().count()),
            _ => 0,
        };
        let own = &members[inherited.min(members.len())..];
        let scope = InitScope::new(&members, &methods, &params);
        let scope = if parent.is_some() {
            scope.with_superclass(own)
        } else {
            scope
        };
        check_initialization(func.body(), scope, func.line())
            .map_err(|violation| CheckError::at(violation.line(), violation.message()))?;
        zero_struct(cx, func, id, ty)?
    };
    let local = cx.declare(func.line(), "self", ty, true)?;
    cx.init_self = Some(local);
    Ok(vec![TStmt::Let { local, init }])
}

fn zero_struct(cx: &FnCx, func: &FuncDecl, id: StructId, ty: Ty) -> CResult<TExpr> {
    let sig = cx
        .sigs
        .struct_sig(id)
        .ok_or_else(|| CheckError::at(func.line(), "internal error: unknown struct"))?;
    let zeros: Vec<TExpr> = sig.members().map(|(_, f)| TExpr::Zero(f.ty())).collect();
    Ok(cx.sigs.construct(ty, zeros))
}

fn is_fatal_error(stmt: &StmtKind) -> bool {
    matches!(
        stmt,
        StmtKind::Expr(expr)
            if matches!(expr.kind(), ExprKind::Call { callee, .. } if callee == FATAL_ERROR)
    )
}

fn always_returns(stmts: &[Stmt]) -> bool {
    match stmts.last().map(Stmt::kind) {
        Some(StmtKind::Return(_)) => true,
        Some(other) if is_fatal_error(other) => true,
        Some(StmtKind::If {
            then_body,
            else_body: Some(else_body),
            ..
        }) => always_returns(then_body) && always_returns(else_body),
        Some(StmtKind::IfLet {
            then_body,
            else_body: Some(else_body),
            ..
        }) => always_returns(then_body) && always_returns(else_body),
        Some(StmtKind::Switch { cases, .. }) => {
            !cases.is_empty() && cases.iter().all(|case| always_returns(case.body()))
        }
        _ => false,
    }
}

fn chain_branches(
    mut branches: Vec<(TExpr, Vec<TStmt>)>,
    default_body: Option<Vec<TStmt>>,
) -> Vec<TStmt> {
    let mut tail = default_body;
    while let Some((condition, body)) = branches.pop() {
        tail = Some(match tail {
            None => body,
            Some(else_body) => vec![TStmt::If {
                cond: condition,
                then_body: body,
                else_body,
            }],
        });
    }
    tail.unwrap_or_default()
}

fn synthetic_cx<'a>(sigs: &'a Signatures, name: &str, ret: Ty) -> FnCx<'a> {
    FnCx {
        sigs,
        scope: Vec::new(),
        expected: None,
        ret,
        locals: Vec::new(),
        scopes: vec![HashMap::new()],
        init_self: None,
        cell_self: None,
        cell_call: CellCall::default(),
        super_initialized: false,
        chain_values: Vec::new(),
        targets: Vec::new(),
        pending_label: None,
        bounds: Vec::new(),
        name: name.to_string(),
        owner: name.to_string(),
        counter: Rc::new(Cell::new(0)),
        captured: Vec::new(),
        environment: Vec::new(),
        labels: HashMap::new(),
    }
}

fn array_count(array: LocalId, ty: Ty) -> TExpr {
    TExpr::Field {
        base: Box::new(TExpr::Local(array, ty)),
        index: ARRAY_COUNT,
        ty: Ty::Int,
    }
}

fn array_get(array: LocalId, ty: Ty, position: LocalId, elem: Ty) -> TExpr {
    TExpr::ArrayOp {
        op: ArrayOp::Get,
        elem,
        args: vec![TExpr::Local(array, ty), TExpr::Local(position, Ty::Int)],
        ty: elem,
    }
}

fn return_if(cond: TExpr, value: bool) -> TStmt {
    TStmt::If {
        cond,
        then_body: vec![TStmt::Return(Some(TExpr::Bool(value)))],
        else_body: Vec::new(),
    }
}

fn step(position: LocalId) -> TStmt {
    TStmt::Assign {
        local: position,
        value: TExpr::Binary {
            op: BinOp::Add,
            lhs: Box::new(TExpr::Local(position, Ty::Int)),
            rhs: Box::new(TExpr::Int(1)),
        },
    }
}

fn each_position(array: LocalId, ty: Ty, position: LocalId, body: Vec<TStmt>) -> Vec<TStmt> {
    vec![
        TStmt::Let {
            local: position,
            init: TExpr::Int(0),
        },
        TStmt::While {
            cond: TExpr::Binary {
                op: BinOp::Lt,
                lhs: Box::new(TExpr::Local(position, Ty::Int)),
                rhs: Box::new(array_count(array, ty)),
            },
            body: body.into_iter().chain([step(position)]).collect(),
        },
    ]
}

pub fn equality_function(sigs: &Signatures, ty: Ty, name: String) -> CResult<TFunc> {
    let mut cx = synthetic_cx(sigs, &name, Ty::Bool);
    let left = cx.declare(0, "left", ty, false)?;
    let right = cx.declare(0, "right", ty, false)?;
    let body = if let Some((key, value)) = sigs.table_parts(ty) {
        let position = cx.temp(Ty::Int);
        let at = cx.temp(Ty::Int);
        let entry = |op: DictOp, table: LocalId, index: TExpr, result: Ty| {
            table_op(op, key, value, vec![local(table, ty), index], result)
        };
        let mut checks = vec![
            TStmt::Let {
                local: at,
                init: entry(
                    DictOp::Find,
                    right,
                    entry(DictOp::Key, left, local(position, Ty::Int), key),
                    Ty::Int,
                ),
            },
            return_if(compare(BinOp::Lt, local(at, Ty::Int), TExpr::Int(0)), false),
        ];
        if value != Ty::Void {
            let same = cx.equality_of(
                0,
                value,
                entry(DictOp::Value, left, local(position, Ty::Int), value),
                entry(DictOp::Value, right, local(at, Ty::Int), value),
            )?;
            checks.push(return_if(
                TExpr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(same),
                },
                false,
            ));
        }
        let differs = compare(
            BinOp::Ne,
            TExpr::Field {
                base: Box::new(local(left, ty)),
                index: DICT_COUNT,
                ty: Ty::Int,
            },
            TExpr::Field {
                base: Box::new(local(right, ty)),
                index: DICT_COUNT,
                ty: Ty::Int,
            },
        );
        vec![
            return_if(differs, false),
            TStmt::Let {
                local: position,
                init: TExpr::Int(0),
            },
            TStmt::While {
                cond: compare(
                    BinOp::Lt,
                    local(position, Ty::Int),
                    TExpr::Field {
                        base: Box::new(local(left, ty)),
                        index: DICT_USED,
                        ty: Ty::Int,
                    },
                ),
                body: vec![
                    TStmt::If {
                        cond: entry(DictOp::Live, left, local(position, Ty::Int), Ty::Bool),
                        then_body: checks,
                        else_body: Vec::new(),
                    },
                    step(position),
                ],
            },
            TStmt::Return(Some(TExpr::Bool(true))),
        ]
    } else if let Some(elem) = sigs.array_element(ty) {
        let position = cx.temp(Ty::Int);
        let same = cx.equality_of(
            0,
            elem,
            array_get(left, ty, position, elem),
            array_get(right, ty, position, elem),
        )?;
        let differs = TExpr::Binary {
            op: BinOp::Ne,
            lhs: Box::new(array_count(left, ty)),
            rhs: Box::new(array_count(right, ty)),
        };
        let mut body = vec![return_if(differs, false)];
        body.extend(each_position(
            left,
            ty,
            position,
            vec![return_if(
                TExpr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(same),
                },
                false,
            )],
        ));
        body.push(TStmt::Return(Some(TExpr::Bool(true))));
        body
    } else {
        let same = cx.equality_of(0, ty, TExpr::Local(left, ty), TExpr::Local(right, ty))?;
        vec![TStmt::Return(Some(same))]
    };
    Ok(TFunc::new(name, 2, Ty::Bool, cx.locals, body))
}

fn local(id: LocalId, ty: Ty) -> TExpr {
    TExpr::Local(id, ty)
}

fn compare(op: BinOp, lhs: TExpr, rhs: TExpr) -> TExpr {
    TExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn text_op(op: StrOp, args: Vec<TExpr>) -> TExpr {
    TExpr::StrOp { op, args }
}

fn table_op(op: DictOp, key: Ty, value: Ty, args: Vec<TExpr>, ty: Ty) -> TExpr {
    TExpr::DictOp {
        op,
        key,
        value,
        hash: String::new(),
        equal: String::new(),
        args,
        ty,
    }
}

fn empty_table(sigs: &Signatures, ty: Ty) -> TExpr {
    sigs.construct(ty, vec![TExpr::Int(0); 5])
}

fn empty_array(sigs: &Signatures, ty: Ty, elem: Ty) -> TExpr {
    sigs.construct(
        ty,
        vec![
            TExpr::Int(0),
            TExpr::Int(0),
            TExpr::ArrayOp {
                op: ArrayOp::Buffer,
                elem,
                args: Vec::new(),
                ty: Ty::Int,
            },
        ],
    )
}

fn hash_value(ty: Ty, expr: TExpr) -> TExpr {
    match ty {
        Ty::Int | Ty::Char | Ty::Enum(_) => expr,
        Ty::Bool => TExpr::Select {
            cond: Box::new(expr),
            then: Box::new(TExpr::Int(1)),
            otherwise: Box::new(TExpr::Int(0)),
            ty: Ty::Int,
        },
        Ty::Str => text_op(StrOp::Hash, vec![expr]),
        other => TExpr::Call {
            callee: HASH.to_string(),
            type_args: vec![other],
            args: vec![expr],
            ret: Ty::Int,
        },
    }
}

fn mix(acc: TExpr, value: TExpr) -> TExpr {
    TExpr::HashMix {
        acc: Box::new(acc),
        value: Box::new(value),
    }
}

pub fn hash_function(sigs: &Signatures, ty: Ty, name: String) -> CResult<TFunc> {
    let mut cx = synthetic_cx(sigs, &name, Ty::Int);
    let subject = cx.declare(0, "subject", ty, false)?;
    let body = if let Some(elem) = sigs.array_element(ty) {
        let position = cx.temp(Ty::Int);
        let acc = cx.temp(Ty::Int);
        let mut body = vec![TStmt::Let {
            local: acc,
            init: array_count(subject, ty),
        }];
        body.extend(each_position(
            subject,
            ty,
            position,
            vec![TStmt::Assign {
                local: acc,
                value: mix(
                    local(acc, Ty::Int),
                    hash_value(elem, array_get(subject, ty, position, elem)),
                ),
            }],
        ));
        body.push(TStmt::Return(Some(local(acc, Ty::Int))));
        body
    } else if let Some(inner) = sigs.unwrapped(ty) {
        let tag = TExpr::Field {
            base: Box::new(local(subject, ty)),
            index: 0,
            ty: Ty::Int,
        };
        let payload = TExpr::Field {
            base: Box::new(local(subject, ty)),
            index: 1,
            ty: inner,
        };
        vec![TStmt::Return(Some(TExpr::Select {
            cond: Box::new(compare(BinOp::Eq, tag, TExpr::Int(NONE_CASE as i64))),
            then: Box::new(TExpr::Int(0)),
            otherwise: Box::new(mix(TExpr::Int(1), hash_value(inner, payload))),
            ty: Ty::Int,
        }))]
    } else {
        let leaves = sigs.equality_leaves(0, ty)?;
        let mut acc = TExpr::Int(17);
        for path in &leaves {
            let reach = path
                .iter()
                .fold(local(subject, ty), |base, (index, leaf)| TExpr::Field {
                    base: Box::new(base),
                    index: *index,
                    ty: *leaf,
                });
            let leaf = path.last().map_or(ty, |(_, leaf)| *leaf);
            acc = mix(acc, hash_value(leaf, reach));
        }
        vec![TStmt::Return(Some(acc))]
    };
    Ok(TFunc::new(name, 1, Ty::Int, cx.locals, body))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOp {
    Union,
    Intersection,
    Subtracting,
    Symmetric,
    Subset,
    StrictSubset,
    Disjoint,
}

impl SetOp {
    pub fn callee(self) -> &'static str {
        match self {
            SetOp::Union => "$union",
            SetOp::Intersection => "$intersection",
            SetOp::Subtracting => "$subtracting",
            SetOp::Symmetric => "$symmetric",
            SetOp::Subset => "$subset",
            SetOp::StrictSubset => "$strictSubset",
            SetOp::Disjoint => "$disjoint",
        }
    }

    pub fn from_callee(callee: &str) -> Option<SetOp> {
        [
            SetOp::Union,
            SetOp::Intersection,
            SetOp::Subtracting,
            SetOp::Symmetric,
            SetOp::Subset,
            SetOp::StrictSubset,
            SetOp::Disjoint,
        ]
        .into_iter()
        .find(|op| op.callee() == callee)
    }

    fn is_predicate(self) -> bool {
        matches!(self, SetOp::Subset | SetOp::StrictSubset | SetOp::Disjoint)
    }
}

pub fn set_function(sigs: &Signatures, ty: Ty, name: String, op: SetOp) -> CResult<TFunc> {
    let (member, _) = sigs
        .table_parts(ty)
        .ok_or_else(|| CheckError::at(0, "internal error: a set operation needs a set"))?;
    let ret = if op.is_predicate() { Ty::Bool } else { ty };
    let mut cx = synthetic_cx(sigs, &name, ret);
    let left = cx.declare(0, "left", ty, false)?;
    let right = cx.declare(0, "right", ty, false)?;
    let result = cx.temp(ty);
    let position = cx.temp(Ty::Int);
    let key_of = |table: LocalId| {
        table_op(
            DictOp::Key,
            member,
            Ty::Void,
            vec![local(table, ty), local(position, Ty::Int)],
            member,
        )
    };
    let find_in = |table: LocalId, wanted: TExpr| {
        table_op(
            DictOp::Find,
            member,
            Ty::Void,
            vec![local(table, ty), wanted],
            Ty::Int,
        )
    };
    let add = |wanted: TExpr| TStmt::Assign {
        local: result,
        value: table_op(
            DictOp::Set,
            member,
            Ty::Void,
            vec![local(result, ty), wanted],
            ty,
        ),
    };
    let records = |table: LocalId, body: Vec<TStmt>| {
        vec![
            TStmt::Let {
                local: position,
                init: TExpr::Int(0),
            },
            TStmt::While {
                cond: compare(
                    BinOp::Lt,
                    local(position, Ty::Int),
                    TExpr::Field {
                        base: Box::new(local(table, ty)),
                        index: DICT_USED,
                        ty: Ty::Int,
                    },
                ),
                body: vec![
                    TStmt::If {
                        cond: table_op(
                            DictOp::Live,
                            member,
                            Ty::Void,
                            vec![local(table, ty), local(position, Ty::Int)],
                            Ty::Bool,
                        ),
                        then_body: body,
                        else_body: Vec::new(),
                    },
                    step(position),
                ],
            },
        ]
    };
    let when = |cond: TExpr, then_body: Vec<TStmt>| TStmt::If {
        cond,
        then_body,
        else_body: Vec::new(),
    };
    let absent =
        |table: LocalId, wanted: TExpr| compare(BinOp::Lt, find_in(table, wanted), TExpr::Int(0));
    let present =
        |table: LocalId, wanted: TExpr| compare(BinOp::Ge, find_in(table, wanted), TExpr::Int(0));
    let fresh = |init: TExpr| TStmt::Let {
        local: result,
        init,
    };
    let count = |table: LocalId| TExpr::Field {
        base: Box::new(local(table, ty)),
        index: DICT_COUNT,
        ty: Ty::Int,
    };
    let mut body = Vec::new();
    match op {
        SetOp::Union => {
            body.push(fresh(local(left, ty)));
            body.extend(records(right, vec![add(key_of(right))]));
        }
        SetOp::Intersection => {
            body.push(fresh(empty_table(sigs, ty)));
            body.extend(records(
                left,
                vec![when(present(right, key_of(left)), vec![add(key_of(left))])],
            ));
        }
        SetOp::Subtracting => {
            body.push(fresh(empty_table(sigs, ty)));
            body.extend(records(
                left,
                vec![when(absent(right, key_of(left)), vec![add(key_of(left))])],
            ));
        }
        SetOp::Symmetric => {
            body.push(fresh(empty_table(sigs, ty)));
            body.extend(records(
                left,
                vec![when(absent(right, key_of(left)), vec![add(key_of(left))])],
            ));
            body.extend(records(
                right,
                vec![when(absent(left, key_of(right)), vec![add(key_of(right))])],
            ));
        }
        SetOp::Subset | SetOp::StrictSubset => {
            let too_big = if op == SetOp::Subset {
                compare(BinOp::Gt, count(left), count(right))
            } else {
                compare(BinOp::Ge, count(left), count(right))
            };
            body.push(return_if(too_big, false));
            body.extend(records(
                left,
                vec![return_if(absent(right, key_of(left)), false)],
            ));
            body.push(TStmt::Return(Some(TExpr::Bool(true))));
        }
        SetOp::Disjoint => {
            body.extend(records(
                left,
                vec![return_if(present(right, key_of(left)), false)],
            ));
            body.push(TStmt::Return(Some(TExpr::Bool(true))));
        }
    }
    if !op.is_predicate() {
        body.push(TStmt::Return(Some(local(result, ty))));
    }
    Ok(TFunc::new(name, 2, ret, cx.locals, body))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HofOp {
    Map,
    Filter,
    Reduce,
    ForEach,
    ContainsWhere,
    FirstWhere,
    FirstIndexWhere,
    AllSatisfy,
    SortedBy,
    Sorted,
    CompactMap,
    FlatMap,
    CountWhere,
    MinBy,
    MaxBy,
    Min,
    Max,
}

impl HofOp {
    const ALL: [HofOp; 17] = [
        HofOp::Map,
        HofOp::Filter,
        HofOp::Reduce,
        HofOp::ForEach,
        HofOp::ContainsWhere,
        HofOp::FirstWhere,
        HofOp::FirstIndexWhere,
        HofOp::AllSatisfy,
        HofOp::SortedBy,
        HofOp::Sorted,
        HofOp::CompactMap,
        HofOp::FlatMap,
        HofOp::CountWhere,
        HofOp::MinBy,
        HofOp::MaxBy,
        HofOp::Min,
        HofOp::Max,
    ];

    pub fn callee(self) -> &'static str {
        match self {
            HofOp::Map => "$map",
            HofOp::Filter => "$filter",
            HofOp::Reduce => "$reduce",
            HofOp::ForEach => "$forEach",
            HofOp::ContainsWhere => "$containsWhere",
            HofOp::FirstWhere => "$firstWhere",
            HofOp::FirstIndexWhere => "$firstIndexWhere",
            HofOp::AllSatisfy => "$allSatisfy",
            HofOp::SortedBy => "$sortedBy",
            HofOp::Sorted => "$sorted",
            HofOp::CompactMap => "$compactMap",
            HofOp::FlatMap => "$flatMap",
            HofOp::CountWhere => "$countWhere",
            HofOp::MinBy => "$minBy",
            HofOp::MaxBy => "$maxBy",
            HofOp::Min => "$min",
            HofOp::Max => "$max",
        }
    }

    pub fn from_callee(callee: &str) -> Option<HofOp> {
        Self::ALL.into_iter().find(|op| op.callee() == callee)
    }
}

pub fn higher_order_function(
    sigs: &Signatures,
    op: HofOp,
    args: &[Ty],
    name: String,
) -> CResult<TFunc> {
    let array = *args.first().ok_or_else(|| {
        CheckError::at(0, "internal error: a higher-order function needs an array")
    })?;
    let elem = sigs
        .array_element(array)
        .ok_or_else(|| CheckError::at(0, "internal error: not an array"))?;
    let func_ty = args.get(1).copied();
    let (params, result) = func_ty
        .and_then(|ty| sigs.func_parts(ty))
        .unwrap_or((Vec::new(), Ty::Void));
    let ret = match op {
        HofOp::Map => sigs.array_of(result, 0)?,
        HofOp::Filter | HofOp::SortedBy | HofOp::Sorted => array,
        HofOp::Reduce => result,
        HofOp::ForEach => Ty::Void,
        HofOp::ContainsWhere | HofOp::AllSatisfy => Ty::Bool,
        HofOp::FirstWhere | HofOp::MinBy | HofOp::MaxBy | HofOp::Min | HofOp::Max => {
            sigs.optional_of(elem)?
        }
        HofOp::FirstIndexWhere => sigs.optional_of(Ty::Int)?,
        HofOp::CompactMap => {
            let inner = sigs
                .unwrapped(result)
                .ok_or_else(|| CheckError::at(0, "compactMap needs an optional result"))?;
            sigs.array_of(inner, 0)?
        }
        HofOp::FlatMap => result,
        HofOp::CountWhere => Ty::Int,
    };
    let mut cx = synthetic_cx(sigs, &name, ret);
    let source = cx.declare(0, "source", array, false)?;
    let initial = if op == HofOp::Reduce {
        Some(cx.declare(0, "initial", result, false)?)
    } else {
        None
    };
    let callable = match func_ty {
        Some(ty) if has_closure(op) => Some(cx.declare(0, "body", ty, false)?),
        _ => None,
    };
    let position = cx.temp(Ty::Int);
    let item = array_get(source, array, position, elem);
    let call = |arguments: Vec<TExpr>| -> TExpr {
        let (Some(callable), Some(ty)) = (callable, func_ty) else {
            return TExpr::Bool(false);
        };
        TExpr::CallValue {
            func: Box::new(local(callable, ty)),
            args: arguments,
            params: params.clone(),
            ret: result,
        }
    };
    let mut body = Vec::new();
    match op {
        HofOp::Map | HofOp::Filter => {
            let built = cx.temp(ret);
            let (kept, stored) = if op == HofOp::Map {
                (None, call(vec![item]))
            } else {
                (Some(call(vec![item.clone()])), item)
            };
            let append = TStmt::Assign {
                local: built,
                value: TExpr::ArrayOp {
                    op: ArrayOp::Append,
                    elem: if op == HofOp::Map { result } else { elem },
                    args: vec![local(built, ret), stored],
                    ty: ret,
                },
            };
            let step_body = match kept {
                Some(condition) => TStmt::If {
                    cond: condition,
                    then_body: vec![append],
                    else_body: Vec::new(),
                },
                None => append,
            };
            body.push(TStmt::Let {
                local: built,
                init: empty_array(sigs, ret, if op == HofOp::Map { result } else { elem }),
            });
            body.extend(each_position(source, array, position, vec![step_body]));
            body.push(TStmt::Return(Some(local(built, ret))));
        }
        HofOp::Reduce => {
            let accumulator = cx.temp(result);
            let Some(initial) = initial else {
                return Err(CheckError::at(
                    0,
                    "internal error: reduce has no initial value",
                ));
            };
            body.push(TStmt::Let {
                local: accumulator,
                init: local(initial, result),
            });
            body.extend(each_position(
                source,
                array,
                position,
                vec![TStmt::Assign {
                    local: accumulator,
                    value: call(vec![local(accumulator, result), item]),
                }],
            ));
            body.push(TStmt::Return(Some(local(accumulator, result))));
        }
        HofOp::ForEach => {
            body.extend(each_position(
                source,
                array,
                position,
                vec![TStmt::Eval(call(vec![item]))],
            ));
        }
        HofOp::ContainsWhere => {
            body.extend(each_position(
                source,
                array,
                position,
                vec![return_if(call(vec![item]), true)],
            ));
            body.push(TStmt::Return(Some(TExpr::Bool(false))));
        }
        HofOp::AllSatisfy => {
            body.extend(each_position(
                source,
                array,
                position,
                vec![return_if(
                    TExpr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(call(vec![item])),
                    },
                    false,
                )],
            ));
            body.push(TStmt::Return(Some(TExpr::Bool(true))));
        }
        HofOp::FirstWhere | HofOp::FirstIndexWhere => {
            let wrapped = if op == HofOp::FirstWhere {
                item.clone()
            } else {
                local(position, Ty::Int)
            };
            body.extend(each_position(
                source,
                array,
                position,
                vec![TStmt::If {
                    cond: call(vec![item]),
                    then_body: vec![TStmt::Return(Some(TExpr::StructInit {
                        ty: ret,
                        args: vec![TExpr::Int(SOME_CASE as i64), wrapped],
                    }))],
                    else_body: Vec::new(),
                }],
            ));
            body.push(TStmt::Return(Some(cx.none_of(ret))));
        }
        HofOp::CompactMap => {
            let inner = sigs
                .unwrapped(result)
                .ok_or_else(|| CheckError::at(0, "compactMap needs an optional result"))?;
            let built = cx.temp(ret);
            let produced = cx.temp(result);
            body.push(TStmt::Let {
                local: built,
                init: empty_array(sigs, ret, inner),
            });
            body.extend(each_position(
                source,
                array,
                position,
                vec![
                    TStmt::Let {
                        local: produced,
                        init: call(vec![item]),
                    },
                    TStmt::If {
                        cond: compare(
                            BinOp::Ne,
                            TExpr::Field {
                                base: Box::new(local(produced, result)),
                                index: 0,
                                ty: Ty::Int,
                            },
                            TExpr::Int(NONE_CASE as i64),
                        ),
                        then_body: vec![TStmt::Assign {
                            local: built,
                            value: TExpr::ArrayOp {
                                op: ArrayOp::Append,
                                elem: inner,
                                args: vec![
                                    local(built, ret),
                                    TExpr::Field {
                                        base: Box::new(local(produced, result)),
                                        index: 1,
                                        ty: inner,
                                    },
                                ],
                                ty: ret,
                            },
                        }],
                        else_body: Vec::new(),
                    },
                ],
            ));
            body.push(TStmt::Return(Some(local(built, ret))));
        }
        HofOp::FlatMap => {
            let inner = sigs
                .array_element(result)
                .ok_or_else(|| CheckError::at(0, "flatMap needs an array result"))?;
            let built = cx.temp(ret);
            let chunk = cx.temp(result);
            let inner_position = cx.temp(Ty::Int);
            body.push(TStmt::Let {
                local: built,
                init: empty_array(sigs, ret, inner),
            });
            let mut each = vec![TStmt::Let {
                local: chunk,
                init: call(vec![item]),
            }];
            each.extend(each_position(
                chunk,
                result,
                inner_position,
                vec![TStmt::Assign {
                    local: built,
                    value: TExpr::ArrayOp {
                        op: ArrayOp::Append,
                        elem: inner,
                        args: vec![
                            local(built, ret),
                            array_get(chunk, result, inner_position, inner),
                        ],
                        ty: ret,
                    },
                }],
            ));
            body.extend(each_position(source, array, position, each));
            body.push(TStmt::Return(Some(local(built, ret))));
        }
        HofOp::CountWhere => {
            let total = cx.temp(Ty::Int);
            body.push(TStmt::Let {
                local: total,
                init: TExpr::Int(0),
            });
            body.extend(each_position(
                source,
                array,
                position,
                vec![TStmt::If {
                    cond: call(vec![item]),
                    then_body: vec![TStmt::Assign {
                        local: total,
                        value: compare(BinOp::Add, local(total, Ty::Int), TExpr::Int(1)),
                    }],
                    else_body: Vec::new(),
                }],
            ));
            body.push(TStmt::Return(Some(local(total, Ty::Int))));
        }
        HofOp::MinBy | HofOp::MaxBy | HofOp::Min | HofOp::Max => {
            let best = cx.temp(elem);
            let less = |left: TExpr, right: TExpr| -> TExpr {
                if matches!(op, HofOp::Min | HofOp::Max) {
                    compare(BinOp::Lt, left, right)
                } else {
                    call(vec![left, right])
                }
            };
            let better = if matches!(op, HofOp::MinBy | HofOp::Min) {
                less(item.clone(), local(best, elem))
            } else {
                less(local(best, elem), item.clone())
            };
            body.push(return_if_none(
                compare(BinOp::Eq, array_count(source, array), TExpr::Int(0)),
                cx.none_of(ret),
            ));
            body.push(TStmt::Let {
                local: best,
                init: TExpr::ArrayOp {
                    op: ArrayOp::Get,
                    elem,
                    args: vec![local(source, array), TExpr::Int(0)],
                    ty: elem,
                },
            });
            body.extend(each_position(
                source,
                array,
                position,
                vec![TStmt::If {
                    cond: better,
                    then_body: vec![TStmt::Assign {
                        local: best,
                        value: item,
                    }],
                    else_body: Vec::new(),
                }],
            ));
            body.push(TStmt::Return(Some(TExpr::StructInit {
                ty: ret,
                args: vec![TExpr::Int(SOME_CASE as i64), local(best, elem)],
            })));
        }
        HofOp::SortedBy | HofOp::Sorted => {
            let less = |left: TExpr, right: TExpr| -> TExpr {
                if op == HofOp::Sorted {
                    compare(BinOp::Lt, left, right)
                } else {
                    call(vec![left, right])
                }
            };
            body = sort_body(&mut cx, source, array, elem, &less);
        }
    }
    Ok(TFunc::new(
        name,
        cx.locals.len().min(param_count(op, func_ty)),
        ret,
        cx.locals,
        body,
    ))
}

fn has_closure(op: HofOp) -> bool {
    !matches!(op, HofOp::Sorted | HofOp::Min | HofOp::Max)
}

fn param_count(op: HofOp, func_ty: Option<Ty>) -> usize {
    let mut count = 1;
    if op == HofOp::Reduce {
        count += 1;
    }
    if func_ty.is_some() && has_closure(op) {
        count += 1;
    }
    count
}

fn return_if_none(cond: TExpr, none: TExpr) -> TStmt {
    TStmt::If {
        cond,
        then_body: vec![TStmt::Return(Some(none))],
        else_body: Vec::new(),
    }
}

fn sort_body(
    cx: &mut FnCx,
    source: LocalId,
    array: Ty,
    elem: Ty,
    less: &dyn Fn(TExpr, TExpr) -> TExpr,
) -> Vec<TStmt> {
    let current = cx.temp(array);
    let buffer = cx.temp(array);
    let spare = cx.temp(array);
    let total = cx.temp(Ty::Int);
    let width = cx.temp(Ty::Int);
    let start = cx.temp(Ty::Int);
    let middle = cx.temp(Ty::Int);
    let end = cx.temp(Ty::Int);
    let left = cx.temp(Ty::Int);
    let right = cx.temp(Ty::Int);
    let out = cx.temp(Ty::Int);
    let take_right = cx.temp(Ty::Bool);
    let int = |id: LocalId| local(id, Ty::Int);
    let get = |table: LocalId, at: LocalId| TExpr::ArrayOp {
        op: ArrayOp::Get,
        elem,
        args: vec![local(table, array), int(at)],
        ty: elem,
    };
    let put = TStmt::Assign {
        local: buffer,
        value: TExpr::ArrayOp {
            op: ArrayOp::Set,
            elem,
            args: vec![local(buffer, array), int(out), get(current, right)],
            ty: array,
        },
    };
    let put_left = TStmt::Assign {
        local: buffer,
        value: TExpr::ArrayOp {
            op: ArrayOp::Set,
            elem,
            args: vec![local(buffer, array), int(out), get(current, left)],
            ty: array,
        },
    };
    let bump = |id: LocalId, by: i64| TStmt::Assign {
        local: id,
        value: compare(BinOp::Add, int(id), TExpr::Int(by)),
    };
    let clamp = |id: LocalId| TStmt::If {
        cond: compare(BinOp::Gt, int(id), int(total)),
        then_body: vec![TStmt::Assign {
            local: id,
            value: int(total),
        }],
        else_body: Vec::new(),
    };
    let merge_step = vec![
        TStmt::Let {
            local: take_right,
            init: TExpr::Bool(false),
        },
        TStmt::If {
            cond: compare(BinOp::Ge, int(left), int(middle)),
            then_body: vec![TStmt::Assign {
                local: take_right,
                value: TExpr::Bool(true),
            }],
            else_body: vec![TStmt::If {
                cond: compare(BinOp::Lt, int(right), int(end)),
                then_body: vec![TStmt::Assign {
                    local: take_right,
                    value: less(get(current, right), get(current, left)),
                }],
                else_body: Vec::new(),
            }],
        },
        TStmt::If {
            cond: local(take_right, Ty::Bool),
            then_body: vec![put, bump(right, 1)],
            else_body: vec![put_left, bump(left, 1)],
        },
        bump(out, 1),
    ];
    let one_run = vec![
        TStmt::Let {
            local: middle,
            init: compare(BinOp::Add, int(start), int(width)),
        },
        clamp(middle),
        TStmt::Let {
            local: end,
            init: compare(
                BinOp::Add,
                int(start),
                compare(BinOp::Mul, int(width), TExpr::Int(2)),
            ),
        },
        clamp(end),
        TStmt::Let {
            local: left,
            init: int(start),
        },
        TStmt::Let {
            local: right,
            init: int(middle),
        },
        TStmt::Let {
            local: out,
            init: int(start),
        },
        TStmt::While {
            cond: compare(BinOp::Lt, int(out), int(end)),
            body: merge_step,
        },
        TStmt::Assign {
            local: start,
            value: compare(
                BinOp::Add,
                int(start),
                compare(BinOp::Mul, int(width), TExpr::Int(2)),
            ),
        },
    ];
    let one_pass = vec![
        TStmt::Let {
            local: start,
            init: TExpr::Int(0),
        },
        TStmt::While {
            cond: compare(BinOp::Lt, int(start), int(total)),
            body: one_run,
        },
        TStmt::Let {
            local: spare,
            init: local(current, array),
        },
        TStmt::Assign {
            local: current,
            value: local(buffer, array),
        },
        TStmt::Assign {
            local: buffer,
            value: local(spare, array),
        },
        TStmt::Assign {
            local: width,
            value: compare(BinOp::Mul, int(width), TExpr::Int(2)),
        },
    ];
    vec![
        TStmt::Let {
            local: current,
            init: local(source, array),
        },
        TStmt::Let {
            local: buffer,
            init: local(source, array),
        },
        TStmt::Let {
            local: total,
            init: array_count(source, array),
        },
        TStmt::Let {
            local: width,
            init: TExpr::Int(1),
        },
        TStmt::While {
            cond: compare(BinOp::Lt, int(width), int(total)),
            body: one_pass,
        },
        TStmt::Return(Some(local(current, array))),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOp {
    MapValues,
    Filter,
    ForEach,
    ContainsWhere,
    AllSatisfy,
    Grouping,
}

impl EntryOp {
    const ALL: [EntryOp; 6] = [
        EntryOp::MapValues,
        EntryOp::Filter,
        EntryOp::ForEach,
        EntryOp::ContainsWhere,
        EntryOp::AllSatisfy,
        EntryOp::Grouping,
    ];

    pub fn callee(self) -> &'static str {
        match self {
            EntryOp::MapValues => "$mapValues",
            EntryOp::Filter => "$filterEntries",
            EntryOp::ForEach => "$forEachEntry",
            EntryOp::ContainsWhere => "$containsEntry",
            EntryOp::AllSatisfy => "$allEntries",
            EntryOp::Grouping => "$grouping",
        }
    }

    pub fn from_callee(callee: &str) -> Option<EntryOp> {
        Self::ALL.into_iter().find(|op| op.callee() == callee)
    }
}

pub fn entry_function(sigs: &Signatures, op: EntryOp, args: &[Ty], name: String) -> CResult<TFunc> {
    let source = *args
        .first()
        .ok_or_else(|| CheckError::at(0, "internal error: an entry function needs a collection"))?;
    let func_ty = *args
        .get(1)
        .ok_or_else(|| CheckError::at(0, "internal error: an entry function needs a closure"))?;
    let (params, result) = sigs
        .func_parts(func_ty)
        .ok_or_else(|| CheckError::at(0, "internal error: not a function"))?;
    if op == EntryOp::Grouping {
        return grouping_function(sigs, source, func_ty, params, result, name);
    }
    let (key, value) = sigs
        .table_parts(source)
        .ok_or_else(|| CheckError::at(0, "internal error: not a dictionary"))?;
    let ret = match op {
        EntryOp::MapValues => sigs.dictionary_of(key, result, 0)?,
        EntryOp::Filter => source,
        EntryOp::ForEach => Ty::Void,
        EntryOp::ContainsWhere | EntryOp::AllSatisfy => Ty::Bool,
        EntryOp::Grouping => source,
    };
    let mut cx = synthetic_cx(sigs, &name, ret);
    let table = cx.declare(0, "table", source, false)?;
    let body_fn = cx.declare(0, "body", func_ty, false)?;
    let result_table = cx.temp(if matches!(op, EntryOp::MapValues | EntryOp::Filter) {
        ret
    } else {
        Ty::Int
    });
    let position = cx.temp(Ty::Int);
    let entry = |kind: DictOp, of: Ty| {
        table_op(
            kind,
            key,
            value,
            vec![local(table, source), local(position, Ty::Int)],
            of,
        )
    };
    let call = |arguments: Vec<TExpr>| TExpr::CallValue {
        func: Box::new(local(body_fn, func_ty)),
        args: arguments,
        params: params.clone(),
        ret: result,
    };
    let pair = || vec![entry(DictOp::Key, key), entry(DictOp::Value, value)];
    let walk = |inner: Vec<TStmt>| {
        vec![
            TStmt::Let {
                local: position,
                init: TExpr::Int(0),
            },
            TStmt::While {
                cond: compare(
                    BinOp::Lt,
                    local(position, Ty::Int),
                    TExpr::Field {
                        base: Box::new(local(table, source)),
                        index: DICT_USED,
                        ty: Ty::Int,
                    },
                ),
                body: vec![
                    TStmt::If {
                        cond: entry(DictOp::Live, Ty::Bool),
                        then_body: inner,
                        else_body: Vec::new(),
                    },
                    step(position),
                ],
            },
        ]
    };
    let mut body = Vec::new();
    match op {
        EntryOp::MapValues | EntryOp::Filter => {
            let stored = if op == EntryOp::MapValues {
                call(vec![entry(DictOp::Value, value)])
            } else {
                entry(DictOp::Value, value)
            };
            let element = if op == EntryOp::MapValues {
                result
            } else {
                value
            };
            let add = TStmt::Assign {
                local: result_table,
                value: table_op(
                    DictOp::Set,
                    key,
                    element,
                    vec![local(result_table, ret), entry(DictOp::Key, key), stored],
                    ret,
                ),
            };
            let inner = if op == EntryOp::Filter {
                vec![TStmt::If {
                    cond: call(pair()),
                    then_body: vec![add],
                    else_body: Vec::new(),
                }]
            } else {
                vec![add]
            };
            body.push(TStmt::Let {
                local: result_table,
                init: empty_table(sigs, ret),
            });
            body.extend(walk(inner));
            body.push(TStmt::Return(Some(local(result_table, ret))));
        }
        EntryOp::ForEach => body.extend(walk(vec![TStmt::Eval(call(pair()))])),
        EntryOp::ContainsWhere => {
            body.extend(walk(vec![return_if(call(pair()), true)]));
            body.push(TStmt::Return(Some(TExpr::Bool(false))));
        }
        EntryOp::AllSatisfy => {
            body.extend(walk(vec![return_if(
                TExpr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(call(pair())),
                },
                false,
            )]));
            body.push(TStmt::Return(Some(TExpr::Bool(true))));
        }
        EntryOp::Grouping => {}
    }
    Ok(TFunc::new(name, 2, ret, cx.locals, body))
}

fn grouping_function(
    sigs: &Signatures,
    array: Ty,
    func_ty: Ty,
    params: Vec<Ty>,
    key: Ty,
    name: String,
) -> CResult<TFunc> {
    let element = sigs
        .array_element(array)
        .ok_or_else(|| CheckError::at(0, "internal error: grouping needs an array"))?;
    let groups = array;
    let value = sigs.array_of(element, 0)?;
    let ret = sigs.dictionary_of(key, value, 0)?;
    let mut cx = synthetic_cx(sigs, &name, ret);
    let source = cx.declare(0, "source", groups, false)?;
    let body_fn = cx.declare(0, "body", func_ty, false)?;
    let result = cx.temp(ret);
    let position = cx.temp(Ty::Int);
    let held = cx.temp(ret);
    let key_local = cx.temp(key);
    let at = cx.temp(Ty::Int);
    let item = array_get(source, groups, position, element);
    let classify = TExpr::CallValue {
        func: Box::new(local(body_fn, func_ty)),
        args: vec![item.clone()],
        params,
        ret: key,
    };
    let ensured = table_op(
        DictOp::Ensure,
        key,
        value,
        vec![
            local(result, ret),
            TExpr::Bind {
                local: key_local,
                init: Box::new(classify),
            },
            empty_array(sigs, value, element),
        ],
        ret,
    );
    let located = table_op(
        DictOp::Find,
        key,
        value,
        vec![local(held, ret), local(key_local, key)],
        Ty::Int,
    );
    let current = table_op(
        DictOp::Value,
        key,
        value,
        vec![local(held, ret), local(at, Ty::Int)],
        value,
    );
    let extended = TExpr::ArrayOp {
        op: ArrayOp::Append,
        elem: element,
        args: vec![current, item],
        ty: value,
    };
    let store = table_op(
        DictOp::Store,
        key,
        value,
        vec![
            TExpr::Bind {
                local: held,
                init: Box::new(ensured),
            },
            TExpr::Bind {
                local: at,
                init: Box::new(located),
            },
            extended,
        ],
        ret,
    );
    let mut body = vec![TStmt::Let {
        local: result,
        init: empty_table(sigs, ret),
    }];
    body.extend(each_position(
        source,
        groups,
        position,
        vec![TStmt::Assign {
            local: result,
            value: store,
        }],
    ));
    body.push(TStmt::Return(Some(local(result, ret))));
    Ok(TFunc::new(name, 2, ret, cx.locals, body))
}

pub fn env_drop_function(sigs: &Signatures, count: usize, name: String) -> CResult<TFunc> {
    let _ = sigs;
    let env = LocalId::new(0);
    let class = Ty::Class(StructId::new(0));
    let mut body: Vec<TStmt> = (0..count)
        .map(|index| {
            TStmt::Release(TExpr::EnvCell {
                env: Box::new(TExpr::Local(env, Ty::Int)),
                index,
                ty: class,
            })
        })
        .collect();
    body.push(TStmt::Free(TExpr::Local(env, Ty::Int)));
    Ok(TFunc::new(name, 1, Ty::Void, vec![Ty::Int], body))
}

pub fn set_from_array_function(sigs: &Signatures, array: Ty, name: String) -> CResult<TFunc> {
    let element = sigs
        .array_element(array)
        .ok_or_else(|| CheckError::at(0, "internal error: a set is built from an array"))?;
    let set = sigs.set_of(element, 0)?;
    let mut cx = synthetic_cx(sigs, &name, set);
    let source = cx.declare(0, "source", array, false)?;
    let result = cx.temp(set);
    let position = cx.temp(Ty::Int);
    let mut body = vec![TStmt::Let {
        local: result,
        init: empty_table(sigs, set),
    }];
    body.extend(each_position(
        source,
        array,
        position,
        vec![TStmt::Assign {
            local: result,
            value: table_op(
                DictOp::Set,
                element,
                Ty::Void,
                vec![
                    local(result, set),
                    array_get(source, array, position, element),
                ],
                set,
            ),
        }],
    ));
    body.push(TStmt::Return(Some(local(result, set))));
    Ok(TFunc::new(name, 1, set, cx.locals, body))
}

pub fn repeating_function(sigs: &Signatures, array: Ty, name: String) -> CResult<TFunc> {
    let element = sigs
        .array_element(array)
        .ok_or_else(|| CheckError::at(0, "internal error: repeating needs an array"))?;
    let mut cx = synthetic_cx(sigs, &name, array);
    let value = cx.declare(0, "value", element, false)?;
    let count = cx.declare(0, "count", Ty::Int, false)?;
    let result = cx.temp(array);
    let position = cx.temp(Ty::Int);
    let body = vec![
        TStmt::If {
            cond: compare(BinOp::Lt, local(count, Ty::Int), TExpr::Int(0)),
            then_body: vec![TStmt::Fatal(
                "Fatal error: Array count must not be negative\n".to_string(),
            )],
            else_body: Vec::new(),
        },
        TStmt::Let {
            local: result,
            init: empty_array(sigs, array, element),
        },
        TStmt::Let {
            local: position,
            init: TExpr::Int(0),
        },
        TStmt::While {
            cond: compare(BinOp::Lt, local(position, Ty::Int), local(count, Ty::Int)),
            body: vec![
                TStmt::Assign {
                    local: result,
                    value: TExpr::ArrayOp {
                        op: ArrayOp::Append,
                        elem: element,
                        args: vec![local(result, array), local(value, element)],
                        ty: array,
                    },
                },
                step(position),
            ],
        },
        TStmt::Return(Some(local(result, array))),
    ];
    Ok(TFunc::new(name, 2, array, cx.locals, body))
}

pub fn table_listing_function(
    sigs: &Signatures,
    ty: Ty,
    name: String,
    keys: bool,
) -> CResult<TFunc> {
    let (key, value) = sigs
        .table_parts(ty)
        .ok_or_else(|| CheckError::at(0, "internal error: a listing needs a dictionary"))?;
    let element = if keys { key } else { value };
    let listing = sigs.array_of(element, 0)?;
    let mut cx = synthetic_cx(sigs, &name, listing);
    let table = cx.declare(0, "table", ty, false)?;
    let result = cx.temp(listing);
    let position = cx.temp(Ty::Int);
    let item = table_op(
        if keys { DictOp::Key } else { DictOp::Value },
        key,
        value,
        vec![local(table, ty), local(position, Ty::Int)],
        element,
    );
    let live = table_op(
        DictOp::Live,
        key,
        value,
        vec![local(table, ty), local(position, Ty::Int)],
        Ty::Bool,
    );
    let body = vec![
        TStmt::Let {
            local: result,
            init: empty_array(sigs, listing, element),
        },
        TStmt::Let {
            local: position,
            init: TExpr::Int(0),
        },
        TStmt::While {
            cond: compare(
                BinOp::Lt,
                local(position, Ty::Int),
                TExpr::Field {
                    base: Box::new(local(table, ty)),
                    index: DICT_USED,
                    ty: Ty::Int,
                },
            ),
            body: vec![
                TStmt::If {
                    cond: live,
                    then_body: vec![TStmt::Assign {
                        local: result,
                        value: TExpr::ArrayOp {
                            op: ArrayOp::Append,
                            elem: element,
                            args: vec![local(result, listing), item],
                            ty: listing,
                        },
                    }],
                    else_body: Vec::new(),
                },
                step(position),
            ],
        },
        TStmt::Return(Some(local(result, listing))),
    ];
    Ok(TFunc::new(name, 1, listing, cx.locals, body))
}

pub fn split_function(sigs: &Signatures, name: String) -> CResult<TFunc> {
    let parts_ty = sigs.array_of(Ty::Str, 0)?;
    let mut cx = synthetic_cx(sigs, &name, parts_ty);
    let text = cx.declare(0, "text", Ty::Str, false)?;
    let separator = cx.declare(0, "separator", Ty::Str, false)?;
    let parts = cx.temp(parts_ty);
    let length = cx.temp(Ty::Int);
    let width = cx.temp(Ty::Int);
    let start = cx.temp(Ty::Int);
    let found = cx.temp(Ty::Int);
    let push = |piece: TExpr| TStmt::Assign {
        local: parts,
        value: TExpr::ArrayOp {
            op: ArrayOp::Append,
            elem: Ty::Str,
            args: vec![local(parts, parts_ty), piece],
            ty: parts_ty,
        },
    };
    let slice =
        |from: TExpr, to: TExpr| text_op(StrOp::ByteSlice, vec![local(text, Ty::Str), from, to]);
    let empty = sigs.construct(
        parts_ty,
        vec![
            TExpr::Int(0),
            TExpr::Int(0),
            TExpr::ArrayOp {
                op: ArrayOp::Buffer,
                elem: Ty::Str,
                args: Vec::new(),
                ty: Ty::Int,
            },
        ],
    );
    let int = |id: LocalId| local(id, Ty::Int);
    let body = vec![
        TStmt::Let {
            local: parts,
            init: empty,
        },
        TStmt::Let {
            local: length,
            init: text_op(StrOp::Count, vec![local(text, Ty::Str)]),
        },
        TStmt::Let {
            local: width,
            init: text_op(StrOp::Count, vec![local(separator, Ty::Str)]),
        },
        TStmt::Let {
            local: start,
            init: TExpr::Int(0),
        },
        TStmt::If {
            cond: compare(BinOp::Eq, int(width), TExpr::Int(0)),
            then_body: vec![
                TStmt::If {
                    cond: compare(BinOp::Ne, int(length), TExpr::Int(0)),
                    then_body: vec![push(local(text, Ty::Str))],
                    else_body: Vec::new(),
                },
                TStmt::Return(Some(local(parts, parts_ty))),
            ],
            else_body: Vec::new(),
        },
        TStmt::While {
            cond: compare(BinOp::Le, int(start), int(length)),
            body: vec![
                TStmt::Let {
                    local: found,
                    init: text_op(
                        StrOp::ByteIndexFrom,
                        vec![local(text, Ty::Str), local(separator, Ty::Str), int(start)],
                    ),
                },
                TStmt::If {
                    cond: compare(BinOp::Lt, int(found), TExpr::Int(0)),
                    then_body: vec![
                        TStmt::If {
                            cond: compare(BinOp::Lt, int(start), int(length)),
                            then_body: vec![push(slice(int(start), int(length)))],
                            else_body: Vec::new(),
                        },
                        TStmt::Assign {
                            local: start,
                            value: compare(BinOp::Add, int(length), TExpr::Int(1)),
                        },
                    ],
                    else_body: vec![
                        TStmt::If {
                            cond: compare(BinOp::Gt, int(found), int(start)),
                            then_body: vec![push(slice(int(start), int(found)))],
                            else_body: Vec::new(),
                        },
                        TStmt::Assign {
                            local: start,
                            value: compare(BinOp::Add, int(found), int(width)),
                        },
                    ],
                },
            ],
        },
        TStmt::Return(Some(local(parts, parts_ty))),
    ];
    Ok(TFunc::new(name, 2, parts_ty, cx.locals, body))
}

pub fn joined_function(sigs: &Signatures, name: String) -> CResult<TFunc> {
    let parts_ty = sigs.array_of(Ty::Str, 0)?;
    let mut cx = synthetic_cx(sigs, &name, Ty::Str);
    let parts = cx.declare(0, "parts", parts_ty, false)?;
    let separator = cx.declare(0, "separator", Ty::Str, false)?;
    let result = cx.temp(Ty::Str);
    let position = cx.temp(Ty::Int);
    let append = |piece: TExpr| TStmt::Assign {
        local: result,
        value: text_op(StrOp::Concat, vec![local(result, Ty::Str), piece]),
    };
    let body = vec![
        TStmt::Let {
            local: result,
            init: TExpr::Str(String::new()),
        },
        TStmt::Let {
            local: position,
            init: TExpr::Int(0),
        },
        TStmt::While {
            cond: compare(
                BinOp::Lt,
                local(position, Ty::Int),
                array_count(parts, parts_ty),
            ),
            body: vec![
                TStmt::If {
                    cond: compare(BinOp::Gt, local(position, Ty::Int), TExpr::Int(0)),
                    then_body: vec![append(local(separator, Ty::Str))],
                    else_body: Vec::new(),
                },
                append(array_get(parts, parts_ty, position, Ty::Str)),
                step(position),
            ],
        },
        TStmt::Return(Some(local(result, Ty::Str))),
    ];
    Ok(TFunc::new(name, 2, Ty::Str, cx.locals, body))
}

pub fn contains_function(sigs: &Signatures, ty: Ty, name: String) -> CResult<TFunc> {
    let elem = sigs
        .array_element(ty)
        .ok_or_else(|| CheckError::at(0, "internal error: contains needs an array"))?;
    let mut cx = synthetic_cx(sigs, &name, Ty::Bool);
    let array = cx.declare(0, "array", ty, false)?;
    let wanted = cx.declare(0, "wanted", elem, false)?;
    let position = cx.temp(Ty::Int);
    let same = cx.equality_of(
        0,
        elem,
        array_get(array, ty, position, elem),
        TExpr::Local(wanted, elem),
    )?;
    let mut body = each_position(array, ty, position, vec![return_if(same, true)]);
    body.push(TStmt::Return(Some(TExpr::Bool(false))));
    Ok(TFunc::new(name, 2, Ty::Bool, cx.locals, body))
}

pub fn boxed_requirement(
    sigs: &Signatures,
    name: String,
    boxed: Ty,
    value: Ty,
    qualified: &str,
) -> CResult<TFunc> {
    let sig = sigs
        .func(qualified)
        .ok_or_else(|| CheckError::at(0, format!("cannot find '{qualified}'")))?;
    let mut locals = vec![boxed];
    locals.extend(sig.params().iter().skip(1).map(ParamSig::ty));
    let mut args = vec![TExpr::Field {
        base: Box::new(TExpr::Local(LocalId::new(0), boxed)),
        index: crate::tir::HEADER_SLOTS,
        ty: value,
    }];
    args.extend(
        locals
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, ty)| TExpr::Local(LocalId::new(index), *ty)),
    );
    let ret = sig.ret();
    let call = TExpr::Call {
        callee: qualified.to_string(),
        type_args: Vec::new(),
        args,
        ret,
    };
    let body = if ret == Ty::Void {
        vec![TStmt::Eval(call), TStmt::Return(None)]
    } else {
        vec![TStmt::Return(Some(call))]
    };
    Ok(TFunc::new(name, locals.len(), ret, locals, body))
}

fn always_exits(stmts: &[Stmt]) -> bool {
    exits(stmts, false, &[])
}

fn exits(stmts: &[Stmt], in_switch: bool, inner_labels: &[String]) -> bool {
    match stmts.last().map(Stmt::kind) {
        Some(StmtKind::Return(_) | StmtKind::Continue(_)) => true,
        Some(other) if is_fatal_error(other) => true,
        Some(StmtKind::Break(None)) => !in_switch,
        Some(StmtKind::Break(Some(label))) => !inner_labels.contains(label),
        Some(StmtKind::If {
            then_body,
            else_body: Some(else_body),
            ..
        })
        | Some(StmtKind::IfLet {
            then_body,
            else_body: Some(else_body),
            ..
        }) => {
            exits(then_body, in_switch, inner_labels) && exits(else_body, in_switch, inner_labels)
        }
        Some(StmtKind::Switch { cases, .. }) => {
            !cases.is_empty()
                && cases
                    .iter()
                    .all(|case| exits(case.body(), true, inner_labels))
        }
        Some(StmtKind::Labeled { label, stmt }) => {
            let mut labels = inner_labels.to_vec();
            labels.push(label.clone());
            exits(std::slice::from_ref(stmt), in_switch, &labels)
        }
        _ => false,
    }
}

struct EnumTarget<'e> {
    def: usize,
    explicit: Option<&'e [TypeRef]>,
    name: String,
}

#[derive(Clone, Copy)]
enum Update<'e> {
    Assign {
        op: Option<BinOp>,
        value: &'e Expr,
    },
    Append(&'e [Arg]),
    Insert(&'e [Arg]),
    RemoveLast(&'e [Arg]),
    Remove(&'e [Arg]),
    Unique,
    Sort(&'e [Arg]),
    Algebra(SetOp, &'e str, &'e [Arg]),
    RemoveAll(&'e [Arg]),
    Entry {
        key: &'e Expr,
        default: Option<&'e Expr>,
        op: Option<BinOp>,
        value: &'e Expr,
    },
    SetInsert(&'e [Arg]),
    SetRemove(&'e [Arg]),
    Default {
        key: &'e Expr,
        default: &'e Expr,
        inner: &'e Update<'e>,
    },
    Within {
        index: &'e Expr,
        inner: &'e Update<'e>,
    },
    Element {
        index: &'e Expr,
        op: Option<BinOp>,
        value: &'e Expr,
    },
}

struct AssignPath {
    value: TExpr,
    root: String,
    root_mutable: bool,
    crossed_class: bool,
}

#[derive(Clone, Copy)]
struct Binding {
    local: LocalId,
    ty: Ty,
    mutable: bool,
}

struct FnCx<'a> {
    sigs: &'a Signatures,
    scope: Vec<(String, Ty)>,
    expected: Option<Ty>,
    ret: Ty,
    locals: Vec<Ty>,
    scopes: Vec<HashMap<String, Binding>>,
    init_self: Option<LocalId>,
    cell_self: Option<CellSelf>,
    cell_call: CellCall,
    super_initialized: bool,
    chain_values: Vec<TExpr>,
    targets: Vec<Target>,
    pending_label: Option<String>,
    bounds: Vec<Vec<String>>,
    name: String,
    owner: String,
    counter: Rc<Cell<usize>>,
    captured: Vec<LocalId>,
    environment: Vec<(LocalId, usize)>,
    labels: HashMap<LocalId, Vec<Option<String>>>,
}

#[derive(Clone, Copy, PartialEq)]
enum TargetKind {
    Loop,
    Switch,
}

struct Target {
    label: Option<String>,
    kind: TargetKind,
}

#[derive(Clone, Copy)]
struct CellSelf {
    cell: LocalId,
    value: LocalId,
    ty: Ty,
}

#[derive(Default)]
struct CellCall {
    allowed: bool,
    pending: Option<(TStmt, TStmt)>,
}

impl<'a> FnCx<'a> {
    fn name_of(&self, ty: Ty) -> String {
        self.sigs.display(ty, &self.scope)
    }

    fn expect(&self, line: u32, expected: Ty, found: Ty) -> CResult<()> {
        if expected == found || self.sigs.is_subclass(found, expected) {
            Ok(())
        } else {
            Err(CheckError::at(
                line,
                format!(
                    "expected {}, but found {}",
                    self.name_of(expected),
                    self.name_of(found)
                ),
            ))
        }
    }

    fn convertible(&self, expected: Ty, found: Ty) -> bool {
        expected == found
            || self.sigs.is_subclass(found, expected)
            || self.boxable(expected, found)
            || self
                .sigs
                .unwrapped(expected)
                .is_some_and(|inner| self.convertible(inner, found))
    }

    fn temp(&mut self, ty: Ty) -> LocalId {
        let local = LocalId::new(self.locals.len());
        self.locals.push(ty);
        local
    }

    fn field_of(&mut self, line: u32, base: TExpr, name: &str) -> CResult<TExpr> {
        if base.ty() == Ty::Char {
            return self.character_property(line, base, name);
        }
        if base.ty() == Ty::Str {
            let op = match name {
                "count" => StrOp::Count,
                "isEmpty" => StrOp::IsEmpty,
                "first" | "last" => return Ok(self.string_end(base, name == "first")),
                _ => {
                    return Err(CheckError::at(
                        line,
                        format!("String has no member '{name}'"),
                    ));
                }
            };
            return Ok(TExpr::StrOp {
                op,
                args: vec![base],
            });
        }
        if let Some(elem) = self.sigs.array_element(base.ty())
            && matches!(name, "first" | "last")
        {
            return self.array_end(base, elem, name == "first");
        }
        if let Some((key, value)) = self.sigs.table_parts(base.ty()) {
            let ty = base.ty();
            let set = self.sigs.is_set(ty);
            match name {
                "isEmpty" => {
                    return Ok(compare(
                        BinOp::Eq,
                        TExpr::Field {
                            base: Box::new(base),
                            index: DICT_COUNT,
                            ty: Ty::Int,
                        },
                        TExpr::Int(0),
                    ));
                }
                "keys" | "values" if !set => {
                    let element = if name == "keys" { key } else { value };
                    return Ok(TExpr::Call {
                        callee: if name == "keys" { KEYS } else { VALUES }.to_string(),
                        type_args: vec![ty],
                        args: vec![base],
                        ret: self.sigs.array_of(element, line)?,
                    });
                }
                _ => {}
            }
        }
        if name == "isEmpty" && self.sigs.array_element(base.ty()).is_some() {
            return Ok(TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(TExpr::Field {
                    base: Box::new(base),
                    index: ARRAY_COUNT,
                    ty: Ty::Int,
                }),
                rhs: Box::new(TExpr::Int(0)),
            });
        }
        let (index, field) = self.member_of(line, base.ty(), name)?;
        let ty = field.ty();
        Ok(TExpr::Field {
            base: Box::new(base),
            index,
            ty,
        })
    }

    fn instance_test(&mut self, subject: TExpr, target: Ty) -> (TExpr, LocalId) {
        let object = self.temp(subject.ty());
        let condition = TExpr::InstanceOf {
            object: Box::new(TExpr::Bind {
                local: object,
                init: Box::new(subject),
            }),
            class: Box::new(TExpr::ClassDesc(target)),
        };
        (condition, object)
    }

    fn cast(
        &mut self,
        line: u32,
        expr: &Expr,
        ty: &TypeRef,
        forced: bool,
        checked: bool,
    ) -> CResult<TExpr> {
        let value = self.value(expr)?;
        let target = self.sigs.resolve_type(ty, &self.scope, line)?;
        if self.sigs.existential_protocol(target).is_some() {
            return Err(CheckError::at(
                line,
                "'is' and 'as' cannot cast to a protocol type yet",
            ));
        }
        if !target.is_class() {
            return Err(CheckError::at(
                line,
                format!(
                    "'is' and 'as' cast class references, but {} is not a class",
                    self.name_of(target)
                ),
            ));
        }
        let source = value.ty();
        let (class, through_optional) = if source.is_class() {
            (source, false)
        } else if let Some(inner) = self.sigs.unwrapped(source).filter(|t| t.is_class()) {
            (inner, true)
        } else {
            return Err(CheckError::at(
                line,
                format!(
                    "'is' and 'as' cast class references, but found {}",
                    self.name_of(source)
                ),
            ));
        };
        if !self.sigs.is_subclass(class, target) && !self.sigs.is_subclass(target, class) {
            return Err(CheckError::at(
                line,
                format!(
                    "cannot cast {} to {}: the classes are unrelated",
                    self.name_of(class),
                    self.name_of(target)
                ),
            ));
        }
        let optional_target = self.sigs.optional_of(target)?;
        let result = if through_optional {
            let held = self.temp(source);
            let payload = TExpr::Field {
                base: Box::new(TExpr::Local(held, source)),
                index: 1,
                ty: class,
            };
            let present = Self::is_some(held, value);
            let inner = self.cast_class(payload, target, optional_target, checked);
            let otherwise = if checked {
                self.none_of(optional_target)
            } else {
                TExpr::Bool(false)
            };
            TExpr::Select {
                cond: Box::new(present),
                ty: otherwise.ty(),
                then: Box::new(inner),
                otherwise: Box::new(otherwise),
            }
        } else {
            self.cast_class(value, target, optional_target, checked)
        };
        Ok(if forced {
            TExpr::Unwrap {
                base: Box::new(result),
                ty: target,
            }
        } else {
            result
        })
    }

    fn cast_class(
        &mut self,
        subject: TExpr,
        target: Ty,
        optional_target: Ty,
        checked: bool,
    ) -> TExpr {
        let (condition, object) = self.instance_test(subject, target);
        if !checked {
            return condition;
        }
        TExpr::Select {
            cond: Box::new(condition),
            then: Box::new(TExpr::StructInit {
                ty: optional_target,
                args: vec![TExpr::Int(SOME_CASE as i64), TExpr::Local(object, target)],
            }),
            otherwise: Box::new(self.none_of(optional_target)),
            ty: optional_target,
        }
    }

    fn optional_chain(&mut self, line: u32, subject: &Expr) -> CResult<(LocalId, TExpr, TExpr)> {
        let value = self.value(subject)?;
        let optional = value.ty();
        let inner = self.sigs.unwrapped(optional).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'?.' needs an optional, but found {}",
                    self.name_of(optional)
                ),
            )
        })?;
        let held = self.temp(optional);
        let payload = TExpr::Field {
            base: Box::new(TExpr::Local(held, optional)),
            index: 1,
            ty: inner,
        };
        Ok((held, value, payload))
    }

    fn is_some(held: LocalId, value: TExpr) -> TExpr {
        TExpr::Binary {
            op: BinOp::Ne,
            lhs: Box::new(TExpr::Field {
                base: Box::new(TExpr::Bind {
                    local: held,
                    init: Box::new(value),
                }),
                index: 0,
                ty: Ty::Int,
            }),
            rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
        }
    }

    fn chain_result(&self, condition: TExpr, access: TExpr) -> CResult<TExpr> {
        let result = access.ty();
        let (ty, then) = if self.sigs.unwrapped(result).is_some() {
            (result, access)
        } else {
            let optional = self.sigs.optional_of(result)?;
            (
                optional,
                TExpr::StructInit {
                    ty: optional,
                    args: vec![TExpr::Int(SOME_CASE as i64), access],
                },
            )
        };
        Ok(TExpr::Select {
            cond: Box::new(condition),
            then: Box::new(then),
            otherwise: Box::new(self.none_of(ty)),
            ty,
        })
    }

    fn coalesce(&mut self, line: u32, lhs: &Expr, rhs: &Expr) -> CResult<TExpr> {
        let value = self.value(lhs)?;
        let optional = value.ty();
        let inner = self.sigs.unwrapped(optional).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'??' needs an optional on its left, but found {}",
                    self.name_of(optional)
                ),
            )
        })?;
        self.expected = Some(if matches!(rhs.kind(), ExprKind::Nil) {
            optional
        } else {
            inner
        });
        let fallback = self.value(rhs)?;
        let held = self.temp(optional);
        let condition = Self::is_some(held, value);
        let (ty, then) = if self.convertible(inner, fallback.ty()) {
            (
                inner,
                TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 1,
                    ty: inner,
                },
            )
        } else if self.convertible(optional, fallback.ty()) {
            (optional, TExpr::Local(held, optional))
        } else {
            self.expect(line, inner, fallback.ty())?;
            (inner, TExpr::Local(held, optional))
        };
        let otherwise = self.coerce(line, ty, fallback)?;
        Ok(TExpr::Select {
            cond: Box::new(condition),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
            ty,
        })
    }

    fn boxable(&self, expected: Ty, found: Ty) -> bool {
        self.sigs
            .existential_protocol(expected)
            .is_some_and(|protocol| {
                !found.is_class()
                    && self.sigs.type_arguments(found).is_empty()
                    && self.sigs.conforms(found, &protocol)
            })
    }

    fn coerce(&self, line: u32, expected: Ty, value: TExpr) -> CResult<TExpr> {
        let found = value.ty();
        if expected == found || self.sigs.is_subclass(found, expected) {
            return Ok(value);
        }
        if let Some(protocol) = self.sigs.existential_protocol(expected)
            && !found.is_class()
        {
            if !self.sigs.conforms(found, &protocol) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "type '{}' does not conform to the protocol '{protocol}'",
                        self.name_of(found)
                    ),
                ));
            }
            if !self.boxable(expected, found) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "{} is generic and cannot be stored in a value of the protocol '{protocol}' yet",
                        self.name_of(found)
                    ),
                ));
            }
            let boxed = self.sigs.box_of(found, line)?;
            return Ok(self.sigs.construct(boxed, vec![value]));
        }
        if let Some(inner) = self.sigs.unwrapped(expected)
            && self.convertible(inner, found)
        {
            let payload = self.coerce(line, inner, value)?;
            return Ok(TExpr::StructInit {
                ty: expected,
                args: vec![TExpr::Int(SOME_CASE as i64), payload],
            });
        }
        self.expect(line, expected, found)?;
        Ok(value)
    }

    fn none_of(&self, ty: Ty) -> TExpr {
        let inner = self.sigs.unwrapped(ty).unwrap_or(Ty::Int);
        TExpr::StructInit {
            ty,
            args: vec![TExpr::Int(NONE_CASE as i64), TExpr::Zero(inner)],
        }
    }

    fn pad_payload(&self, ty: Ty, mut args: Vec<TExpr>) -> Vec<TExpr> {
        if let Ty::Struct(id) = ty
            && let Some(layout) = self.sigs.struct_sig(id)
        {
            for field in layout.fields().iter().skip(args.len()) {
                args.push(TExpr::Zero(field.ty()));
            }
        }
        args
    }

    fn declare(&mut self, line: u32, name: &str, ty: Ty, mutable: bool) -> CResult<LocalId> {
        let local = LocalId::new(self.locals.len());
        let Some(scope) = self.scopes.last_mut() else {
            return Err(CheckError::at(line, "internal error: no open scope"));
        };
        if scope.contains_key(name) {
            return Err(CheckError::at(
                line,
                format!("'{name}' is already declared in this scope"),
            ));
        }
        scope.insert(name.to_string(), Binding { local, ty, mutable });
        self.locals.push(ty);
        Ok(local)
    }

    fn lookup(&self, name: &str, line: u32) -> CResult<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .ok_or_else(|| CheckError::at(line, format!("cannot find '{name}' in scope")))
    }

    fn block(&mut self, stmts: &[Stmt]) -> CResult<Vec<TStmt>> {
        self.scopes.push(HashMap::new());
        let mut checked = Vec::with_capacity(stmts.len());
        let mut result = Ok(());
        for stmt in stmts {
            result = self.lower_stmt(stmt, &mut checked);
            if result.is_err() {
                break;
            }
        }
        self.scopes.pop();
        result.map(|()| checked)
    }

    fn in_target<R>(
        &mut self,
        kind: TargetKind,
        check: impl FnOnce(&mut Self) -> CResult<R>,
    ) -> CResult<R> {
        let label = self.pending_label.take();
        self.targets.push(Target { label, kind });
        let result = check(self);
        self.targets.pop();
        result
    }

    fn jump_depth(&self, line: u32, keyword: &str, label: Option<&str>) -> CResult<usize> {
        let is_continue = keyword == "continue";
        let mut candidates = self.targets.iter().rev().enumerate();
        match label {
            Some(label) => {
                let found = candidates.find(|(_, t)| t.label.as_deref() == Some(label));
                match found {
                    Some((_, t)) if is_continue && t.kind == TargetKind::Switch => {
                        Err(CheckError::at(
                            line,
                            format!("'continue' cannot target '{label}': it labels a switch"),
                        ))
                    }
                    Some((depth, _)) => Ok(depth),
                    None => Err(CheckError::at(
                        line,
                        format!("cannot find the label '{label}' for '{keyword}'"),
                    )),
                }
            }
            None => candidates
                .find(|(_, t)| !is_continue || t.kind == TargetKind::Loop)
                .map(|(depth, _)| depth)
                .ok_or_else(|| {
                    let place = if is_continue {
                        "a loop"
                    } else {
                        "a loop or a switch"
                    };
                    CheckError::at(line, format!("'{keyword}' is only allowed inside {place}"))
                }),
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt, out: &mut Vec<TStmt>) -> CResult<()> {
        match stmt.kind() {
            StmtKind::Switch { subject, cases } => {
                let mut lowered = Vec::new();
                self.in_target(TargetKind::Switch, |cx| {
                    cx.switch_stmt(stmt.line(), subject, cases, &mut lowered)
                })?;
                out.push(TStmt::Block(lowered));
                Ok(())
            }
            StmtKind::Labeled { label, stmt: inner } => {
                if self
                    .targets
                    .iter()
                    .any(|t| t.label.as_deref() == Some(label))
                {
                    return Err(CheckError::at(
                        stmt.line(),
                        format!("the label '{label}' is already used by an enclosing statement"),
                    ));
                }
                self.pending_label = Some(label.clone());
                let result = self.lower_stmt(inner, out);
                self.pending_label = None;
                result
            }
            StmtKind::Break(label) => {
                let depth = self.jump_depth(stmt.line(), "break", label.as_deref())?;
                out.push(TStmt::Break(depth));
                Ok(())
            }
            StmtKind::Continue(label) => {
                let depth = self.jump_depth(stmt.line(), "continue", label.as_deref())?;
                out.push(TStmt::Continue(depth));
                Ok(())
            }
            StmtKind::WhileLet {
                name,
                mutable,
                value,
                body,
            } => self.while_let(stmt.line(), name, *mutable, value, body, out),
            StmtKind::Guard { cond, else_body } => {
                self.require_exit(stmt.line(), else_body)?;
                let cond = self.condition(cond)?;
                let leave = self.block(else_body)?;
                out.push(TStmt::If {
                    cond: TExpr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(cond),
                    },
                    then_body: leave,
                    else_body: Vec::new(),
                });
                Ok(())
            }
            StmtKind::GuardLet {
                name,
                mutable,
                value,
                else_body,
            } => self.guard_let(stmt.line(), name, *mutable, value, else_body, out),
            StmtKind::Expr(expr) if matches!(expr.kind(), ExprKind::OptionalChain { .. }) => {
                let ExprKind::OptionalChain { subject, link } = expr.kind() else {
                    return Ok(());
                };
                self.chain_statements(stmt.line(), subject, link, out)
            }
            StmtKind::Expr(expr) if self.is_self_init(expr) => {
                self.self_init(stmt.line(), expr, out)
            }
            StmtKind::Expr(expr) if self.is_super_init(expr) => {
                self.super_init(stmt.line(), expr, out)
            }
            StmtKind::Expr(_) => {
                self.cell_call.allowed = true;
                let checked = self.stmt(stmt);
                self.cell_call.allowed = false;
                let checked = checked?;
                let pending = self.cell_call.pending.take();
                out.extend(pending.as_ref().map(|(before, _)| before.clone()));
                out.push(checked);
                out.extend(pending.map(|(_, after)| after));
                Ok(())
            }
            _ => {
                out.push(self.stmt(stmt)?);
                Ok(())
            }
        }
    }

    fn if_let(
        &mut self,
        line: u32,
        name: &str,
        mutable: bool,
        value: &Expr,
        then_body: &[Stmt],
        else_body: Option<&[Stmt]>,
    ) -> CResult<TStmt> {
        let value = self.value(value)?;
        let optional = value.ty();
        let inner = self.sigs.unwrapped(optional).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'if let' needs an optional, but found {}",
                    self.name_of(optional)
                ),
            )
        })?;
        let held = self.temp(optional);
        self.scopes.push(HashMap::new());
        let bound = self.declare(line, name, inner, mutable);
        let body = bound.and_then(|local| {
            let mut body = vec![TStmt::Let {
                local,
                init: TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 1,
                    ty: inner,
                },
            }];
            body.extend(self.block(then_body)?);
            Ok(body)
        });
        self.scopes.pop();
        let body = body?;
        let otherwise = match else_body {
            Some(stmts) => self.block(stmts)?,
            None => Vec::new(),
        };
        let test = TStmt::If {
            cond: TExpr::Binary {
                op: BinOp::Ne,
                lhs: Box::new(TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 0,
                    ty: Ty::Int,
                }),
                rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
            },
            then_body: body,
            else_body: otherwise,
        };
        Ok(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: vec![
                TStmt::Let {
                    local: held,
                    init: value,
                },
                test,
            ],
            else_body: Vec::new(),
        })
    }

    fn require_exit(&self, line: u32, else_body: &[Stmt]) -> CResult<()> {
        if always_exits(else_body) {
            Ok(())
        } else {
            Err(CheckError::at(
                line,
                "the 'else' body of a 'guard' must leave the scope: end every path with 'return', 'break' or 'continue'",
            ))
        }
    }

    fn guard_let(
        &mut self,
        line: u32,
        name: &str,
        mutable: bool,
        value: &Expr,
        else_body: &[Stmt],
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        self.require_exit(line, else_body)?;
        let value = self.value(value)?;
        let optional = value.ty();
        let inner = self.sigs.unwrapped(optional).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'guard let' needs an optional, but found {}",
                    self.name_of(optional)
                ),
            )
        })?;
        let held = self.temp(optional);
        let leave = self.block(else_body)?;
        out.push(TStmt::Let {
            local: held,
            init: value,
        });
        out.push(TStmt::If {
            cond: TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 0,
                    ty: Ty::Int,
                }),
                rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
            },
            then_body: leave,
            else_body: Vec::new(),
        });
        let local = self.declare(line, name, inner, mutable)?;
        out.push(TStmt::Let {
            local,
            init: TExpr::Field {
                base: Box::new(TExpr::Local(held, optional)),
                index: 1,
                ty: inner,
            },
        });
        Ok(())
    }

    fn while_let(
        &mut self,
        line: u32,
        name: &str,
        mutable: bool,
        value: &Expr,
        body: &[Stmt],
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let value = self.value(value)?;
        let optional = value.ty();
        let inner = self.sigs.unwrapped(optional).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'while let' needs an optional, but found {}",
                    self.name_of(optional)
                ),
            )
        })?;
        let running = self.temp(Ty::Bool);
        let held = self.temp(optional);
        self.scopes.push(HashMap::new());
        let bound = self.declare(line, name, inner, mutable);
        let checked = bound.and_then(|local| {
            let mut checked = vec![TStmt::Let {
                local,
                init: TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 1,
                    ty: inner,
                },
            }];
            checked.extend(self.in_target(TargetKind::Loop, |cx| cx.block(body))?);
            Ok(checked)
        });
        self.scopes.pop();
        let checked = checked?;
        let test = TStmt::If {
            cond: TExpr::Binary {
                op: BinOp::Ne,
                lhs: Box::new(TExpr::Field {
                    base: Box::new(TExpr::Local(held, optional)),
                    index: 0,
                    ty: Ty::Int,
                }),
                rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
            },
            then_body: checked,
            else_body: vec![TStmt::Assign {
                local: running,
                value: TExpr::Bool(false),
            }],
        };
        out.push(TStmt::Let {
            local: running,
            init: TExpr::Bool(true),
        });
        out.push(TStmt::While {
            cond: TExpr::Local(running, Ty::Bool),
            body: vec![TStmt::If {
                cond: TExpr::Bool(true),
                then_body: vec![
                    TStmt::Let {
                        local: held,
                        init: value,
                    },
                    test,
                ],
                else_body: Vec::new(),
            }],
        });
        Ok(())
    }

    fn chain_statements(
        &mut self,
        line: u32,
        subject: &Expr,
        link: &Expr,
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let (held, value, payload) = self.optional_chain(line, subject)?;
        let optional = value.ty();
        self.chain_values.push(payload);
        let mut body = Vec::new();
        let checked = match link.kind() {
            ExprKind::OptionalChain {
                subject: next,
                link: rest,
            } => self.chain_statements(line, next, rest, &mut body),
            _ => self.expr(link).map(|call| body.push(TStmt::Eval(call))),
        };
        self.chain_values.pop();
        checked?;
        out.push(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: vec![
                TStmt::Let {
                    local: held,
                    init: value,
                },
                TStmt::If {
                    cond: TExpr::Binary {
                        op: BinOp::Ne,
                        lhs: Box::new(TExpr::Field {
                            base: Box::new(TExpr::Local(held, optional)),
                            index: 0,
                            ty: Ty::Int,
                        }),
                        rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
                    },
                    then_body: body,
                    else_body: Vec::new(),
                },
            ],
            else_body: Vec::new(),
        });
        Ok(())
    }

    fn switch_stmt(
        &mut self,
        line: u32,
        subject: &Expr,
        cases: &[SwitchCase],
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let subject_value = self.value(subject)?;
        let subject_ty = subject_value.ty();
        if matches!(subject_ty, Ty::Int | Ty::Bool | Ty::Str | Ty::Char) {
            return self.value_switch(line, subject_value, cases, out);
        }
        let sigs = self.sigs;
        let Some(sig) = sigs.enum_of(subject_ty) else {
            return Err(CheckError::at(
                line,
                format!(
                    "switch is supported only on enums, Int, Bool and String, not {}",
                    sigs.type_name(subject_ty)
                ),
            ));
        };
        if cases.is_empty() {
            return Err(CheckError::at(line, "a switch needs at least one case"));
        }
        let temporary = LocalId::new(self.locals.len());
        self.locals.push(subject_ty);
        out.push(TStmt::Let {
            local: temporary,
            init: subject_value,
        });

        let mut covered = vec![false; sig.cases().len()];
        let mut default_body: Option<Vec<TStmt>> = None;
        let mut branches: Vec<(TExpr, Vec<TStmt>)> = Vec::new();
        for (position, case) in cases.iter().enumerate() {
            match case.label() {
                CaseLabel::Default => {
                    if position + 1 != cases.len() {
                        return Err(CheckError::at(
                            case.line(),
                            "'default' must be the last case of a switch",
                        ));
                    }
                    default_body = Some(self.block(case.body())?);
                }
                CaseLabel::Values(_) => {
                    return Err(CheckError::at(
                        case.line(),
                        format!("a case of '{}' cannot be a value", sig.base_name()),
                    ));
                }
                CaseLabel::Patterns(patterns) => {
                    self.scopes.push(HashMap::new());
                    let outcome = self.case_patterns(
                        case,
                        patterns,
                        (&sig, temporary, subject_ty),
                        &mut covered,
                    );
                    let checked =
                        outcome.and_then(|tests| self.block(case.body()).map(|body| (tests, body)));
                    self.scopes.pop();
                    let (tests, body) = checked?;
                    if tests.is_empty() {
                        return Err(CheckError::at(case.line(), "a case needs a pattern"));
                    }
                    if tests.iter().all(|(_, prelude)| prelude.is_empty()) {
                        let mut combined: Option<TExpr> = None;
                        for (test, _) in tests {
                            combined = Some(match combined.take() {
                                None => test,
                                Some(previous) => TExpr::Binary {
                                    op: BinOp::Or,
                                    lhs: Box::new(previous),
                                    rhs: Box::new(test),
                                },
                            });
                        }
                        if let Some(condition) = combined {
                            branches.push((condition, body));
                        }
                    } else {
                        for (test, mut prelude) in tests {
                            prelude.extend(body.iter().cloned());
                            branches.push((test, prelude));
                        }
                    }
                }
            }
        }

        let missing = covered.iter().position(|c| !c);
        if let (None, Some(missing)) = (&default_body, missing) {
            return Err(CheckError::at(
                line,
                format!(
                    "switch must be exhaustive: missing case '{}'",
                    sig.cases()[missing].name()
                ),
            ));
        }
        out.extend(chain_branches(branches, default_body));
        Ok(())
    }

    fn value_switch(
        &mut self,
        line: u32,
        subject: TExpr,
        cases: &[SwitchCase],
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let ty = subject.ty();
        if cases.is_empty() {
            return Err(CheckError::at(line, "a switch needs at least one case"));
        }
        let temporary = self.temp(ty);
        out.push(TStmt::Let {
            local: temporary,
            init: subject,
        });
        let mut default_body: Option<Vec<TStmt>> = None;
        let mut branches: Vec<(TExpr, Vec<TStmt>)> = Vec::new();
        let mut bools = [false, false];
        for (position, case) in cases.iter().enumerate() {
            match case.label() {
                CaseLabel::Default => {
                    if position + 1 != cases.len() {
                        return Err(CheckError::at(
                            case.line(),
                            "'default' must be the last case of a switch",
                        ));
                    }
                    default_body = Some(self.block(case.body())?);
                }
                CaseLabel::Patterns(_) => {
                    return Err(CheckError::at(
                        case.line(),
                        format!("an enum case cannot match a {}", self.name_of(ty)),
                    ));
                }
                CaseLabel::Values(values) => {
                    let mut condition: Option<TExpr> = None;
                    for value in values {
                        let test =
                            self.value_test(case.line(), temporary, ty, value, &mut bools)?;
                        condition = Some(match condition {
                            None => test,
                            Some(before) => TExpr::Binary {
                                op: BinOp::Or,
                                lhs: Box::new(before),
                                rhs: Box::new(test),
                            },
                        });
                    }
                    let Some(condition) = condition else {
                        return Err(CheckError::at(case.line(), "a case needs a pattern"));
                    };
                    branches.push((condition, self.block(case.body())?));
                }
            }
        }
        if default_body.is_none() {
            if ty == Ty::Bool {
                if !(bools[0] && bools[1]) {
                    let missing = if bools[0] { "false" } else { "true" };
                    return Err(CheckError::at(
                        line,
                        format!("switch must be exhaustive: missing case '{missing}'"),
                    ));
                }
            } else {
                return Err(CheckError::at(
                    line,
                    format!(
                        "switch must be exhaustive: a switch over {} needs a 'default' case",
                        self.name_of(ty)
                    ),
                ));
            }
        }
        out.extend(chain_branches(branches, default_body));
        Ok(())
    }

    fn literal_pattern(&mut self, line: u32, expr: &Expr, expected: Ty) -> CResult<TExpr> {
        self.expected = Some(expected);
        let value = self.value(expr)?;
        let literal = match &value {
            TExpr::Int(_) | TExpr::Bool(_) | TExpr::Str(_) | TExpr::Char(_) => true,
            TExpr::Unary {
                op: UnaryOp::Neg,
                expr,
            } => matches!(**expr, TExpr::Int(_)),
            _ => false,
        };
        if literal {
            Ok(value)
        } else {
            Err(CheckError::at(line, "a case value must be a literal"))
        }
    }

    fn value_test(
        &mut self,
        line: u32,
        local: LocalId,
        ty: Ty,
        pattern: &Expr,
        bools: &mut [bool; 2],
    ) -> CResult<TExpr> {
        let subject = TExpr::Local(local, ty);
        if let ExprKind::Range { lo, hi, inclusive } = pattern.kind() {
            if !matches!(ty, Ty::Int | Ty::Char) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "a range pattern needs an Int or a Character, not {}",
                        self.name_of(ty)
                    ),
                ));
            }
            let lo = self.literal_pattern(line, lo, ty)?;
            let hi = self.literal_pattern(line, hi, ty)?;
            self.expect(line, ty, lo.ty())?;
            self.expect(line, ty, hi.ty())?;
            let upper = if *inclusive { BinOp::Le } else { BinOp::Lt };
            return Ok(TExpr::Binary {
                op: BinOp::And,
                lhs: Box::new(TExpr::Binary {
                    op: BinOp::Ge,
                    lhs: Box::new(subject.clone()),
                    rhs: Box::new(lo),
                }),
                rhs: Box::new(TExpr::Binary {
                    op: upper,
                    lhs: Box::new(subject),
                    rhs: Box::new(hi),
                }),
            });
        }
        let value = self.literal_pattern(line, pattern, ty)?;
        self.expect(line, ty, value.ty())?;
        if let TExpr::Bool(b) = value {
            bools[usize::from(b)] = true;
        }
        Ok(TExpr::Binary {
            op: BinOp::Eq,
            lhs: Box::new(subject),
            rhs: Box::new(value),
        })
    }

    fn case_patterns(
        &mut self,
        case: &SwitchCase,
        patterns: &[Pattern],
        subject: (&EnumSig, LocalId, Ty),
        covered: &mut [bool],
    ) -> CResult<Vec<(TExpr, Vec<TStmt>)>> {
        let (sig, temporary, subject_ty) = subject;
        let mut tests: Vec<(TExpr, Vec<TStmt>)> = Vec::new();
        let mut first_names: Option<Vec<String>> = None;
        for pattern in patterns {
            let mut prelude = Vec::new();
            if pattern.type_name().is_some_and(|n| n != sig.base_name()) {
                return Err(CheckError::at(
                    case.line(),
                    format!("expected a case of '{}'", sig.base_name()),
                ));
            }
            let index = sig.case_index(pattern.case()).ok_or_else(|| {
                CheckError::at(
                    case.line(),
                    format!("'{}' has no case '{}'", sig.name(), pattern.case()),
                )
            })?;
            covered[index] = true;
            if let Some(bindings) = pattern.bindings() {
                self.bind_payload(
                    case.line(),
                    sig,
                    index,
                    (temporary, subject_ty),
                    (bindings, pattern.labels()),
                    &mut prelude,
                )?;
            }
            let mut names: Vec<String> = pattern
                .bindings()
                .unwrap_or_default()
                .iter()
                .flatten()
                .cloned()
                .collect();
            names.sort();
            match &first_names {
                None => first_names = Some(names),
                Some(expected) if *expected != names => {
                    return Err(CheckError::at(
                        case.line(),
                        "the patterns of a case must bind the same names",
                    ));
                }
                Some(_) => {}
            }
            let tag = if sig.has_payload() {
                TExpr::Field {
                    base: Box::new(TExpr::Local(temporary, subject_ty)),
                    index: 0,
                    ty: Ty::Int,
                }
            } else {
                TExpr::Local(temporary, subject_ty)
            };
            let test = TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(tag),
                rhs: Box::new(self.case_tag(sig, subject_ty, index)),
            };
            tests.push((test, prelude));
        }
        Ok(tests)
    }

    fn case_tag(&self, sig: &EnumSig, ty: Ty, index: usize) -> TExpr {
        if sig.has_payload() {
            TExpr::Int(index as i64)
        } else {
            TExpr::EnumCase { ty, index }
        }
    }

    fn bind_payload(
        &mut self,
        line: u32,
        sig: &EnumSig,
        index: usize,
        subject: (LocalId, Ty),
        pattern: (&[Option<String>], &[Option<String>]),
        prelude: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let (bindings, given) = pattern;
        let case = &sig.cases()[index];
        for (position, label) in given.iter().enumerate() {
            let declared = case.labels().get(position).and_then(|l| l.as_deref());
            if label.is_some() && label.as_deref() != declared {
                return Err(CheckError::at(
                    line,
                    format!(
                        "the label {:?} of the pattern does not match the label {:?} of '{}.{}'",
                        label,
                        declared,
                        sig.name(),
                        case.name()
                    ),
                ));
            }
        }
        if bindings.len() != case.payload().len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}.{}' carries {} value(s), but the pattern binds {}",
                    sig.name(),
                    case.name(),
                    case.payload().len(),
                    bindings.len()
                ),
            ));
        }
        let (temporary, subject_ty) = subject;
        let mut bound_here: Vec<&str> = Vec::new();
        for (position, (binding, ty)) in bindings.iter().zip(case.payload()).enumerate() {
            let Some(name) = binding else { continue };
            if bound_here.contains(&name.as_str()) {
                return Err(CheckError::at(
                    line,
                    format!("'{name}' is already declared in this scope"),
                ));
            }
            bound_here.push(name);
            let shared = self
                .scopes
                .last()
                .and_then(|scope| scope.get(name))
                .copied();
            let local = match shared {
                Some(existing) if existing.ty == *ty => existing.local,
                Some(existing) => {
                    return Err(CheckError::at(
                        line,
                        format!(
                            "'{name}' is bound with the types {} and {} by the patterns of one case",
                            self.name_of(existing.ty),
                            self.name_of(*ty)
                        ),
                    ));
                }
                None => self.declare(line, name, *ty, false)?,
            };
            prelude.push(TStmt::Let {
                local,
                init: TExpr::Field {
                    base: Box::new(TExpr::Local(temporary, subject_ty)),
                    index: case.slots()[position],
                    ty: *ty,
                },
            });
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> CResult<TStmt> {
        let line = stmt.line();
        match stmt.kind() {
            StmtKind::Let {
                name,
                mutable,
                ty,
                init,
            } => {
                let Some(init) = init else {
                    return Err(CheckError::at(
                        line,
                        "a variable must be initialized where it is declared",
                    ));
                };
                let annotated = match ty {
                    Some(annotation) => {
                        Some(self.sigs.resolve_type(annotation, &self.scope, line)?)
                    }
                    None => None,
                };
                self.expected = annotated;
                let mut value = self.value(init)?;
                if let Some(annotated) = annotated {
                    value = self.coerce(line, annotated, value)?;
                }
                let declared = annotated.unwrap_or_else(|| value.ty());
                let local = self.declare(line, name, declared, *mutable)?;
                Ok(TStmt::Let { local, init: value })
            }
            StmtKind::Assign { target, op, value } => self.assign(line, target, *op, value),
            StmtKind::Return(value) => self.return_stmt(line, value.as_ref()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond = self.condition(cond)?;
                let then_body = self.block(then_body)?;
                let else_body = match else_body {
                    Some(body) => self.block(body)?,
                    None => Vec::new(),
                };
                Ok(TStmt::If {
                    cond,
                    then_body,
                    else_body,
                })
            }
            StmtKind::IfLet {
                name,
                value,
                then_body,
                else_body,
                mutable,
            } => self.if_let(line, name, *mutable, value, then_body, else_body.as_deref()),
            StmtKind::While { cond, body } => {
                let cond = self.condition(cond)?;
                let body = self.in_target(TargetKind::Loop, |cx| cx.block(body))?;
                Ok(TStmt::While { cond, body })
            }
            StmtKind::For { var, iter, body } => self.for_stmt(line, var, iter, body),
            StmtKind::ForPair {
                key,
                value,
                iter,
                body,
            } => self.for_pair(line, key, value, iter, body),
            StmtKind::LocalFunc(func) => self.local_function(line, func),
            StmtKind::Expr(expr) => self.expr_stmt(expr),
            StmtKind::Switch { .. }
            | StmtKind::Labeled { .. }
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::WhileLet { .. }
            | StmtKind::Guard { .. }
            | StmtKind::GuardLet { .. } => Err(CheckError::at(
                line,
                "internal error: this statement must be lowered into a block",
            )),
        }
    }

    fn assign(
        &mut self,
        line: u32,
        target: &Expr,
        op: Option<BinOp>,
        value: &Expr,
    ) -> CResult<TStmt> {
        match target.kind() {
            ExprKind::Index { base, index } => {
                let dictionary = self.value(base).is_ok_and(|v| {
                    self.sigs.table_parts(v.ty()).is_some() && !self.sigs.is_set(v.ty())
                });
                if dictionary {
                    let entry = Update::Entry {
                        key: index,
                        default: None,
                        op,
                        value,
                    };
                    return self.update(line, base, &entry);
                }
                self.update(line, base, &Update::Element { index, op, value })
            }
            ExprKind::IndexDefault {
                base,
                index,
                default,
            } => {
                let entry = Update::Entry {
                    key: index,
                    default: Some(default),
                    op,
                    value,
                };
                self.update(line, base, &entry)
            }
            ExprKind::Ident(_) | ExprKind::Member { .. } => {
                self.update(line, target, &Update::Assign { op, value })
            }
            _ => Err(CheckError::at(
                line,
                "the left side of an assignment must be assignable",
            )),
        }
    }

    fn update(&mut self, line: u32, target: &Expr, update: &Update) -> CResult<TStmt> {
        let mut stmts = self.update_stmts(line, target, update)?;
        Ok(if stmts.len() == 1 {
            stmts.remove(0)
        } else {
            TStmt::If {
                cond: TExpr::Bool(true),
                then_body: stmts,
                else_body: Vec::new(),
            }
        })
    }

    fn update_stmts(&mut self, line: u32, target: &Expr, update: &Update) -> CResult<Vec<TStmt>> {
        match target.kind() {
            ExprKind::Ident(name) => Ok(vec![self.assign_variable(line, name, update)?]),
            ExprKind::Member { base, name } => {
                let mut stmts = self.element_prefix(line, base)?;
                stmts.push(self.assign_member(line, base, name, update)?);
                Ok(stmts)
            }
            ExprKind::IndexDefault {
                base,
                index,
                default,
            } => self.update_stmts(
                line,
                base,
                &Update::Default {
                    key: index,
                    default,
                    inner: update,
                },
            ),
            ExprKind::Index { base, index } => {
                let mut stmts = self.update_stmts(line, base, &Update::Unique)?;
                let within = Update::Within {
                    index,
                    inner: update,
                };
                stmts.extend(self.update_stmts(line, base, &within)?);
                Ok(stmts)
            }
            _ => Err(CheckError::at(
                line,
                "only a variable, a member or an element can be changed",
            )),
        }
    }

    fn element_prefix(&mut self, line: u32, expr: &Expr) -> CResult<Vec<TStmt>> {
        match expr.kind() {
            ExprKind::Member { base, .. } => self.element_prefix(line, base),
            ExprKind::Index { base, .. } => {
                let element = self.value(expr)?.ty();
                if element.is_struct() {
                    self.update_stmts(line, base, &Update::Unique)
                } else {
                    Ok(Vec::new())
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    fn updated(
        &mut self,
        line: u32,
        update: &Update,
        target_ty: Ty,
        current: TExpr,
    ) -> CResult<TExpr> {
        match update {
            Update::Assign { op, value } => {
                self.expected = Some(target_ty);
                let rhs = self.value(value)?;
                self.assigned_value(line, target_ty, current, *op, rhs)
            }
            Update::Append(args) => {
                let elem = self.array_target(line, target_ty)?;
                let [arg] = args else {
                    return Err(CheckError::at(line, "append takes one unlabeled value"));
                };
                if arg.label().is_some() {
                    return Err(CheckError::at(line, "append takes one unlabeled value"));
                }
                self.expected = Some(elem);
                let value = self.value(arg.value())?;
                let value = self.coerce(line, elem, value)?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Append,
                    elem,
                    args: vec![current, value],
                    ty: target_ty,
                })
            }
            Update::Insert(args) => {
                let elem = self.array_target(line, target_ty)?;
                let usage = "insert takes a value and 'at:' with a position";
                let [value, at] = args else {
                    return Err(CheckError::at(line, usage));
                };
                if value.label().is_some() || at.label() != Some("at") {
                    return Err(CheckError::at(line, usage));
                }
                self.expected = Some(elem);
                let value = self.value(value.value())?;
                let value = self.coerce(line, elem, value)?;
                let position = self.int_value(at.value())?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Insert,
                    elem,
                    args: vec![current, position, value],
                    ty: target_ty,
                })
            }
            Update::Entry {
                key: key_expr,
                default,
                op,
                value,
            } => {
                let Some((key, stored)) = self
                    .sigs
                    .table_parts(target_ty)
                    .filter(|_| !self.sigs.is_set(target_ty))
                else {
                    return Err(CheckError::at(
                        line,
                        format!("{} is not a dictionary", self.name_of(target_ty)),
                    ));
                };
                self.expected = Some(key);
                let wanted = self.value(key_expr)?;
                let wanted = self.coerce(line, key, wanted)?;
                if op.is_none() && matches!(value.kind(), ExprKind::Nil) {
                    return Ok(table_op(
                        DictOp::Remove,
                        key,
                        stored,
                        vec![current, wanted],
                        target_ty,
                    ));
                }
                self.expected = Some(stored);
                let rhs = self.value(value)?;
                let held = self.temp(key);
                let (key_arg, new_value) = match op {
                    None => (wanted, self.coerce(line, stored, rhs)?),
                    Some(op) => {
                        let Some(default) = default else {
                            return Err(CheckError::at(
                                line,
                                "changing a dictionary value in place needs 'd[key, default: x]'",
                            ));
                        };
                        self.expected = Some(stored);
                        let fallback = self.value(default)?;
                        let fallback = self.coerce(line, stored, fallback)?;
                        let at = self.temp(Ty::Int);
                        let found = table_op(
                            DictOp::Find,
                            key,
                            stored,
                            vec![current.clone(), local(held, key)],
                            Ty::Int,
                        );
                        let present = TExpr::Select {
                            cond: Box::new(compare(
                                BinOp::Lt,
                                TExpr::Bind {
                                    local: at,
                                    init: Box::new(found),
                                },
                                TExpr::Int(0),
                            )),
                            then: Box::new(fallback),
                            otherwise: Box::new(table_op(
                                DictOp::Value,
                                key,
                                stored,
                                vec![current.clone(), local(at, Ty::Int)],
                                stored,
                            )),
                            ty: stored,
                        };
                        let changed = self.assigned_value(line, stored, present, Some(*op), rhs)?;
                        (
                            TExpr::Bind {
                                local: held,
                                init: Box::new(wanted),
                            },
                            changed,
                        )
                    }
                };
                Ok(table_op(
                    DictOp::Set,
                    key,
                    stored,
                    vec![current, key_arg, new_value],
                    target_ty,
                ))
            }
            Update::Default {
                key: key_expr,
                default,
                inner,
            } => {
                let Some((key, stored)) = self
                    .sigs
                    .table_parts(target_ty)
                    .filter(|_| !self.sigs.is_set(target_ty))
                else {
                    return Err(CheckError::at(
                        line,
                        format!("{} is not a dictionary", self.name_of(target_ty)),
                    ));
                };
                self.expected = Some(key);
                let wanted = self.value(key_expr)?;
                let wanted = self.coerce(line, key, wanted)?;
                self.expected = Some(stored);
                let fallback = self.value(default)?;
                let fallback = self.coerce(line, stored, fallback)?;
                let held = self.temp(target_ty);
                let key_local = self.temp(key);
                let at = self.temp(Ty::Int);
                let ensured = table_op(
                    DictOp::Ensure,
                    key,
                    stored,
                    vec![
                        current,
                        TExpr::Bind {
                            local: key_local,
                            init: Box::new(wanted),
                        },
                        fallback,
                    ],
                    target_ty,
                );
                let position = table_op(
                    DictOp::Find,
                    key,
                    stored,
                    vec![local(held, target_ty), local(key_local, key)],
                    Ty::Int,
                );
                let present = table_op(
                    DictOp::Value,
                    key,
                    stored,
                    vec![local(held, target_ty), local(at, Ty::Int)],
                    stored,
                );
                let changed = self.updated(line, inner, stored, present)?;
                Ok(table_op(
                    DictOp::Store,
                    key,
                    stored,
                    vec![
                        TExpr::Bind {
                            local: held,
                            init: Box::new(ensured),
                        },
                        TExpr::Bind {
                            local: at,
                            init: Box::new(position),
                        },
                        changed,
                    ],
                    target_ty,
                ))
            }
            Update::SetInsert(args) | Update::SetRemove(args) => {
                let Some((element, _)) = self
                    .sigs
                    .table_parts(target_ty)
                    .filter(|_| self.sigs.is_set(target_ty))
                else {
                    return Err(CheckError::at(
                        line,
                        format!("{} is not a set", self.name_of(target_ty)),
                    ));
                };
                let [arg] = args else {
                    return Err(CheckError::at(line, "this takes one unlabeled value"));
                };
                if arg.label().is_some() {
                    return Err(CheckError::at(line, "this takes one unlabeled value"));
                }
                self.expected = Some(element);
                let member = self.value(arg.value())?;
                let member = self.coerce(line, element, member)?;
                let op = if matches!(update, Update::SetInsert(_)) {
                    DictOp::Set
                } else {
                    DictOp::Remove
                };
                Ok(table_op(
                    op,
                    element,
                    Ty::Void,
                    vec![current, member],
                    target_ty,
                ))
            }
            Update::RemoveAll(args) if self.sigs.table_parts(target_ty).is_some() => {
                if !args.is_empty() {
                    return Err(CheckError::at(line, "removeAll takes no arguments"));
                }
                Ok(empty_table(self.sigs, target_ty))
            }
            Update::RemoveAll(args) => {
                let elem = self.array_target(line, target_ty)?;
                if !args.is_empty() {
                    return Err(CheckError::at(line, "removeAll takes no arguments"));
                }
                let buffer = TExpr::ArrayOp {
                    op: ArrayOp::Buffer,
                    elem,
                    args: Vec::new(),
                    ty: Ty::Int,
                };
                Ok(self
                    .sigs
                    .construct(target_ty, vec![TExpr::Int(0), TExpr::Int(0), buffer]))
            }
            Update::Algebra(op, name, args) => {
                if !self.sigs.is_set(target_ty) {
                    return Err(CheckError::at(
                        line,
                        format!("{} is not a set", self.name_of(target_ty)),
                    ));
                }
                let other = self.set_operand(line, name, None, target_ty, args)?;
                Ok(TExpr::Call {
                    callee: op.callee().to_string(),
                    type_args: vec![target_ty],
                    args: vec![current, other],
                    ret: target_ty,
                })
            }
            Update::Sort(args) => {
                let elem = self.array_target(line, target_ty)?;
                match self.array_closure_method(line, &current, elem, "sorted", args)? {
                    Some(sorted) => Ok(sorted),
                    None => Err(CheckError::at(
                        line,
                        "sort takes no argument or a comparison closure",
                    )),
                }
            }
            Update::Unique => {
                let elem = self.array_target(line, target_ty)?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Unique,
                    elem,
                    args: vec![current],
                    ty: target_ty,
                })
            }
            Update::Within { index, inner } => {
                let elem = self.array_target(line, target_ty)?;
                let position = self.int_value(index)?;
                let held = self.temp(Ty::Int);
                let element = TExpr::ArrayOp {
                    op: ArrayOp::Get,
                    elem,
                    args: vec![current.clone(), TExpr::Local(held, Ty::Int)],
                    ty: elem,
                };
                let changed = self.updated(line, inner, elem, element)?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Set,
                    elem,
                    args: vec![
                        current,
                        TExpr::Bind {
                            local: held,
                            init: Box::new(position),
                        },
                        changed,
                    ],
                    ty: target_ty,
                })
            }
            Update::Remove(args) => {
                let elem = self.array_target(line, target_ty)?;
                let usage = "remove takes 'at:' with a position";
                let [at] = args else {
                    return Err(CheckError::at(line, usage));
                };
                if at.label() != Some("at") {
                    return Err(CheckError::at(line, usage));
                }
                let position = self.int_value(at.value())?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Remove,
                    elem,
                    args: vec![current, position],
                    ty: target_ty,
                })
            }
            Update::RemoveLast(args) => {
                let elem = self.array_target(line, target_ty)?;
                if !args.is_empty() {
                    return Err(CheckError::at(line, "removeLast takes no arguments"));
                }
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::RemoveLast,
                    elem,
                    args: vec![current],
                    ty: target_ty,
                })
            }
            Update::Element { index, op, value } => {
                let elem = self.array_target(line, target_ty)?;
                let position = self.int_value(index)?;
                let held = self.temp(Ty::Int);
                self.expected = Some(elem);
                let rhs = self.value(value)?;
                let element = TExpr::ArrayOp {
                    op: ArrayOp::Get,
                    elem,
                    args: vec![current.clone(), TExpr::Local(held, Ty::Int)],
                    ty: elem,
                };
                let stored = self.assigned_value(line, elem, element, *op, rhs)?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Set,
                    elem,
                    args: vec![
                        current,
                        TExpr::Bind {
                            local: held,
                            init: Box::new(position),
                        },
                        stored,
                    ],
                    ty: target_ty,
                })
            }
        }
    }

    fn array_target(&self, line: u32, ty: Ty) -> CResult<Ty> {
        self.sigs
            .array_element(ty)
            .ok_or_else(|| CheckError::at(line, format!("{} is not an array", self.name_of(ty))))
    }

    fn assign_variable(&mut self, line: u32, name: &str, update: &Update) -> CResult<TStmt> {
        let binding = match self.lookup(name, line) {
            Ok(binding) => binding,
            Err(error) => {
                return match self.implicit_member(line, name) {
                    Some((receiver, _, _)) => {
                        self.assign_through(line, "self", receiver, name, update)
                    }
                    None => Err(error),
                };
            }
        };
        if !binding.mutable {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
        let current = TExpr::Local(binding.local, binding.ty);
        let new_value = self.updated(line, update, binding.ty, current)?;
        Ok(TStmt::Assign {
            local: binding.local,
            value: new_value,
        })
    }

    fn assign_member(
        &mut self,
        line: u32,
        base: &Expr,
        name: &str,
        update: &Update,
    ) -> CResult<TStmt> {
        if let ExprKind::Ident(var) = base.kind()
            && let Ok(binding) = self.lookup(var, line)
        {
            return self.assign_through(line, var, binding, name, update);
        }
        self.assign_nested(line, base, name, update)
    }

    fn assign_nested(
        &mut self,
        line: u32,
        base: &Expr,
        name: &str,
        update: &Update,
    ) -> CResult<TStmt> {
        let path = self.assignable_path(line, base)?;
        let container = path.value.ty();
        let (index, field) = self.member_of(line, container, name)?;
        let initializing = path.root == "self" && self.init_self.is_some();
        if !field.is_mutable() && !initializing {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
        if !path.crossed_class && !container.is_class() && !path.root_mutable {
            return Err(CheckError::at(
                line,
                format!(
                    "cannot assign to a member of '{}': it is a constant",
                    path.root
                ),
            ));
        }
        let field_ty = field.ty();
        let current = TExpr::Field {
            base: Box::new(path.value.clone()),
            index,
            ty: field_ty,
        };
        let new_value = self.updated(line, update, field_ty, current)?;
        Ok(TStmt::AssignPath {
            base: path.value,
            index,
            value: new_value,
        })
    }

    fn assignable_path(&mut self, line: u32, expr: &Expr) -> CResult<AssignPath> {
        match expr.kind() {
            ExprKind::Ident(name) => match self.lookup(name, line) {
                Ok(binding) => Ok(AssignPath {
                    value: TExpr::Local(binding.local, binding.ty),
                    root: name.clone(),
                    root_mutable: binding.mutable,
                    crossed_class: false,
                }),
                Err(error) => match self.implicit_member(line, name) {
                    Some((receiver, _, _)) => {
                        let root = AssignPath {
                            value: TExpr::Local(receiver.local, receiver.ty),
                            root: "self".to_string(),
                            root_mutable: receiver.mutable,
                            crossed_class: false,
                        };
                        self.hop(line, root, name)
                    }
                    None => Err(error),
                },
            },
            ExprKind::Member { base, name } => {
                let path = self.assignable_path(line, base)?;
                self.hop(line, path, name)
            }
            ExprKind::Index { base, index } => {
                let array = self.value(base)?;
                let elem = self.array_target(line, array.ty())?;
                let position = self.int_value(index)?;
                Ok(AssignPath {
                    value: TExpr::ArrayOp {
                        op: ArrayOp::Get,
                        elem,
                        args: vec![array, position],
                        ty: elem,
                    },
                    root: "an array element".to_string(),
                    root_mutable: true,
                    crossed_class: elem.is_class(),
                })
            }
            _ => Err(CheckError::at(
                line,
                "only a member of a variable can be assigned",
            )),
        }
    }

    fn hop(&self, line: u32, path: AssignPath, name: &str) -> CResult<AssignPath> {
        let container = path.value.ty();
        let (index, field) = self.member_of(line, container, name)?;
        if field.is_unowned() || field.is_weak() {
            return Err(CheckError::at(
                line,
                format!("cannot assign through the unowned or weak member '{name}'"),
            ));
        }
        let member_ty = field.ty();
        let initializing = path.root == "self" && self.init_self.is_some();
        if !member_ty.is_class() && !field.is_mutable() && !initializing {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
        Ok(AssignPath {
            value: TExpr::Field {
                base: Box::new(path.value),
                index,
                ty: member_ty,
            },
            crossed_class: path.crossed_class || container.is_class(),
            root: path.root,
            root_mutable: path.root_mutable,
        })
    }

    fn implicit_member(&self, line: u32, name: &str) -> Option<(Binding, usize, FieldSig)> {
        let receiver = self.lookup("self", line).ok()?;
        let (index, field) = self.member_of(line, receiver.ty, name).ok()?;
        Some((receiver, index, field))
    }

    fn assign_through(
        &mut self,
        line: u32,
        var: &str,
        binding: Binding,
        name: &str,
        update: &Update,
    ) -> CResult<TStmt> {
        if !binding.mutable && !binding.ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("cannot assign to a member of '{var}': it is a constant"),
            ));
        }
        let (index, field) = self.member_of(line, binding.ty, name)?;
        let initializing = var == "self" && self.init_self.is_some();
        if !field.is_mutable() && !initializing {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
        let field_ty = field.ty();
        let current = TExpr::Field {
            base: Box::new(TExpr::Local(binding.local, binding.ty)),
            index,
            ty: field_ty,
        };
        let new_value = self.updated(line, update, field_ty, current)?;
        Ok(TStmt::AssignField {
            local: binding.local,
            index,
            value: new_value,
        })
    }

    fn assigned_value(
        &self,
        line: u32,
        target_ty: Ty,
        current: TExpr,
        op: Option<BinOp>,
        rhs: TExpr,
    ) -> CResult<TExpr> {
        match op {
            None => self.coerce(line, target_ty, rhs),
            Some(BinOp::Add) if self.sigs.array_element(target_ty).is_some() => {
                self.expect(line, target_ty, rhs.ty())?;
                let elem = self.array_target(line, target_ty)?;
                Ok(TExpr::ArrayOp {
                    op: ArrayOp::Concat,
                    elem,
                    args: vec![current, rhs],
                    ty: target_ty,
                })
            }
            Some(BinOp::Add) if target_ty == Ty::Str => {
                self.expect(line, Ty::Str, rhs.ty())?;
                Ok(TExpr::StrOp {
                    op: StrOp::Concat,
                    args: vec![current, rhs],
                })
            }
            Some(op) => {
                self.expect(line, Ty::Int, target_ty)?;
                self.expect(line, Ty::Int, rhs.ty())?;
                Ok(TExpr::Binary {
                    op,
                    lhs: Box::new(current),
                    rhs: Box::new(rhs),
                })
            }
        }
    }

    fn member_of(&self, line: u32, ty: Ty, name: &str) -> CResult<(usize, FieldSig)> {
        let (Ty::Struct(id) | Ty::Class(id)) = ty else {
            return Err(CheckError::at(
                line,
                format!("a value of type {} has no members", self.name_of(ty)),
            ));
        };
        if self.sigs.enum_of(ty).is_some() {
            return Err(CheckError::at(
                line,
                format!("a value of type {} has no members", self.name_of(ty)),
            ));
        }
        let sig = self
            .sigs
            .struct_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown struct"))?;
        sig.field(name)
            .map(|(index, field)| (index, field.clone()))
            .ok_or_else(|| CheckError::at(line, format!("'{}' has no member '{name}'", sig.name())))
    }

    fn return_stmt(&mut self, line: u32, value: Option<&Expr>) -> CResult<TStmt> {
        if let Some(local) = self.init_self {
            return match value {
                None => Ok(TStmt::Return(Some(TExpr::Local(local, self.ret)))),
                Some(_) => Err(CheckError::at(
                    line,
                    "an initializer does not return a value",
                )),
            };
        }
        if let Some(cell_self) = self.cell_self {
            return self.return_writing_back(line, value, cell_self);
        }
        match (value, self.ret) {
            (None, Ty::Void) => Ok(TStmt::Return(None)),
            (None, _) => Err(CheckError::at(line, "this function must return a value")),
            (Some(_), Ty::Void) => Err(CheckError::at(
                line,
                "this function does not return a value",
            )),
            (Some(expr), ret) => {
                self.expected = Some(ret);
                let value = self.value(expr)?;
                let value = self.coerce(line, ret, value)?;
                Ok(TStmt::Return(Some(value)))
            }
        }
    }

    fn return_writing_back(
        &mut self,
        line: u32,
        value: Option<&Expr>,
        cell_self: CellSelf,
    ) -> CResult<TStmt> {
        let mut body = Vec::new();
        let result = match (value, self.ret) {
            (None, Ty::Void) => None,
            (None, _) => return Err(CheckError::at(line, "this function must return a value")),
            (Some(_), Ty::Void) => {
                return Err(CheckError::at(
                    line,
                    "this function does not return a value",
                ));
            }
            (Some(expr), ret) => {
                self.expected = Some(ret);
                let value = self.value(expr)?;
                let value = self.coerce(line, ret, value)?;
                let local = LocalId::new(self.locals.len());
                self.locals.push(ret);
                body.push(TStmt::Let { local, init: value });
                Some(TExpr::Local(local, ret))
            }
        };
        body.push(cell_self.write_back());
        body.push(TStmt::Return(result));
        Ok(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: body,
            else_body: Vec::new(),
        })
    }

    fn for_stmt(&mut self, line: u32, var: &str, iter: &Expr, body: &[Stmt]) -> CResult<TStmt> {
        let ExprKind::Range { lo, hi, inclusive } = iter.kind() else {
            let text = self.value(iter)?;
            if let Some(elem) = self.sigs.array_element(text.ty()) {
                return self.for_elements(line, var, text, elem, body);
            }
            if let Some((key, value)) = self.sigs.table_parts(text.ty()) {
                if !self.sigs.is_set(text.ty()) {
                    return Err(CheckError::at(
                        line,
                        "iterate a dictionary with 'for (key, value) in'",
                    ));
                }
                return self.for_table(line, text, key, value, var, None, body);
            }
            if text.ty() != Ty::Str {
                return Err(CheckError::at(
                    line,
                    "a for loop iterates over a range, a string or an array",
                ));
            }
            return self.for_characters(line, var, text, body);
        };
        let lo = self.int_value(lo)?;
        let hi = self.int_value(hi)?;
        self.scopes.push(HashMap::new());
        let result = self.declare(line, var, Ty::Int, false).and_then(|local| {
            let body = self.in_target(TargetKind::Loop, |cx| cx.block(body))?;
            Ok(TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive: *inclusive,
                body,
            })
        });
        self.scopes.pop();
        result
    }

    fn array_method(
        &mut self,
        line: u32,
        base: &Expr,
        receiver: TExpr,
        elem: Ty,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        if let Some(call) = self.array_closure_method(line, &receiver, elem, name, args)? {
            return Ok(call);
        }
        match name {
            "contains" => {
                let [arg] = args else {
                    return Err(CheckError::at(line, "contains takes one unlabeled value"));
                };
                if arg.label().is_some() {
                    return Err(CheckError::at(line, "contains takes one unlabeled value"));
                }
                self.expected = Some(elem);
                let value = self.value(arg.value())?;
                let value = self.coerce(line, elem, value)?;
                let ty = receiver.ty();
                self.equality_of(line, elem, TExpr::Zero(elem), TExpr::Zero(elem))?;
                Ok(TExpr::Call {
                    callee: CONTAINS.to_string(),
                    type_args: vec![ty],
                    args: vec![receiver, value],
                    ret: Ty::Bool,
                })
            }
            "joined" => self.array_joined(line, receiver, args),
            "removeLast" | "popLast" | "remove" => {
                self.array_taking(line, base, receiver, elem, name, args)
            }
            "append" | "insert" | "removeAll" => Err(CheckError::at(
                line,
                format!("'{name}' changes the array: write it as a statement of its own"),
            )),
            _ => Err(CheckError::at(
                line,
                format!("{} has no method '{name}'", self.name_of(receiver.ty())),
            )),
        }
    }

    fn callable(
        &mut self,
        line: u32,
        arg: &Arg,
        params: Vec<Ty>,
        ret: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        if let ExprKind::Closure(closure) = arg.value().kind() {
            return self.closure_value(line, closure, Some(params), ret);
        }
        self.expected = ret.map(|r| sigs.func_ty(params.clone(), r));
        let value = self.value(arg.value())?;
        let Some((got_params, got_ret)) = sigs.func_parts(value.ty()) else {
            return Err(CheckError::at(
                line,
                format!(
                    "expected a function, but found {}",
                    self.name_of(value.ty())
                ),
            ));
        };
        if got_params != params || ret.is_some_and(|r| r != got_ret) {
            let shown = sigs.func_ty(params, ret.unwrap_or(got_ret));
            return Err(CheckError::at(
                line,
                format!(
                    "expected {}, but found {}",
                    self.name_of(shown),
                    self.name_of(value.ty())
                ),
            ));
        }
        Ok(value)
    }

    fn array_closure_method(
        &mut self,
        line: u32,
        receiver: &TExpr,
        elem: Ty,
        name: &str,
        args: &[Arg],
    ) -> CResult<Option<TExpr>> {
        let sigs = self.sigs;
        let array = receiver.ty();
        let closure_arg = |arg: &Arg, label: &str| {
            arg.label() == Some(label)
                || (arg.label().is_none() && matches!(arg.value().kind(), ExprKind::Closure(_)))
        };
        let operation = match (name, args) {
            ("map", [_]) => Some(HofOp::Map),
            ("filter", [_]) => Some(HofOp::Filter),
            ("forEach", [_]) => Some(HofOp::ForEach),
            ("allSatisfy", [_]) => Some(HofOp::AllSatisfy),
            ("reduce", [_, _]) => Some(HofOp::Reduce),
            ("contains", [arg]) if closure_arg(arg, "where") => Some(HofOp::ContainsWhere),
            ("first", [arg]) if closure_arg(arg, "where") => Some(HofOp::FirstWhere),
            ("firstIndex", [arg]) if closure_arg(arg, "where") => Some(HofOp::FirstIndexWhere),
            ("sorted", [arg]) if closure_arg(arg, "by") => Some(HofOp::SortedBy),
            ("sorted", []) => Some(HofOp::Sorted),
            ("compactMap", [_]) => Some(HofOp::CompactMap),
            ("flatMap", [_]) => Some(HofOp::FlatMap),
            ("count", [arg]) if closure_arg(arg, "where") => Some(HofOp::CountWhere),
            ("min", [arg]) if closure_arg(arg, "by") => Some(HofOp::MinBy),
            ("max", [arg]) if closure_arg(arg, "by") => Some(HofOp::MaxBy),
            ("min", []) => Some(HofOp::Min),
            ("max", []) => Some(HofOp::Max),
            _ => None,
        };
        let Some(op) = operation else {
            return Ok(None);
        };
        let usage = format!("'{name}' takes a closure");
        let unlabeled_only = matches!(
            op,
            HofOp::Map
                | HofOp::Filter
                | HofOp::ForEach
                | HofOp::AllSatisfy
                | HofOp::CompactMap
                | HofOp::FlatMap
        );
        if unlabeled_only && args.iter().any(|a| a.label().is_some()) {
            return Err(CheckError::at(line, usage));
        }
        let (extra, closure_at) = match op {
            HofOp::Reduce if args[0].label() == Some("into") => {
                let initial = self.value(args[0].value())?;
                (Some(initial), 1)
            }
            HofOp::Reduce => {
                if args[0].label().is_some() || args[1].label().is_some() {
                    return Err(CheckError::at(
                        line,
                        "reduce takes an initial value and a closure",
                    ));
                }
                let initial = self.value(args[0].value())?;
                (Some(initial), 1)
            }
            _ => (None, 0),
        };
        let (callable_args, ret): (Vec<Ty>, Option<Ty>) = match op {
            HofOp::Map => (vec![elem], None),
            HofOp::Filter
            | HofOp::ContainsWhere
            | HofOp::FirstWhere
            | HofOp::FirstIndexWhere
            | HofOp::AllSatisfy => (vec![elem], Some(Ty::Bool)),
            HofOp::ForEach => (vec![elem], Some(Ty::Void)),
            HofOp::CompactMap | HofOp::FlatMap => (vec![elem], None),
            HofOp::CountWhere => (vec![elem], Some(Ty::Bool)),
            HofOp::SortedBy | HofOp::MinBy | HofOp::MaxBy => (vec![elem, elem], Some(Ty::Bool)),
            HofOp::Reduce => {
                let initial = extra.as_ref().map_or(Ty::Void, TExpr::ty);
                (vec![initial, elem], Some(initial))
            }
            HofOp::Sorted | HofOp::Min | HofOp::Max => (Vec::new(), None),
        };
        if matches!(op, HofOp::Sorted | HofOp::Min | HofOp::Max) {
            if !matches!(elem, Ty::Int | Ty::Char | Ty::Str) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "{name}() needs elements that can be ordered, but {} cannot: use {name}(by:)",
                        self.name_of(elem)
                    ),
                ));
            }
            let ret = if op == HofOp::Sorted {
                array
            } else {
                sigs.optional_of(elem)?
            };
            return Ok(Some(TExpr::Call {
                callee: op.callee().to_string(),
                type_args: vec![array],
                args: vec![receiver.clone()],
                ret,
            }));
        }
        let function = if op == HofOp::Reduce && args[0].label() == Some("into") {
            let ExprKind::Closure(closure) = args[1].value().kind() else {
                return Err(CheckError::at(
                    line,
                    "reduce(into:) takes a closure that changes its first parameter",
                ));
            };
            let initial = extra.as_ref().map_or(Ty::Void, TExpr::ty);
            self.closure_into(line, closure, initial, elem)?
        } else {
            self.callable(line, &args[closure_at], callable_args, ret)?
        };
        let function_ty = function.ty();
        let Some((_, result)) = sigs.func_parts(function_ty) else {
            return Err(CheckError::at(line, "internal error: not a function"));
        };
        let ret = match op {
            HofOp::Map => {
                if result == Ty::Void {
                    return Err(CheckError::at(
                        line,
                        "map needs a closure that returns a value",
                    ));
                }
                sigs.array_of(result, line)?
            }
            HofOp::Filter | HofOp::SortedBy => array,
            HofOp::Reduce => result,
            HofOp::ForEach => Ty::Void,
            HofOp::ContainsWhere | HofOp::AllSatisfy => Ty::Bool,
            HofOp::FirstWhere | HofOp::MinBy | HofOp::MaxBy => sigs.optional_of(elem)?,
            HofOp::FirstIndexWhere => sigs.optional_of(Ty::Int)?,
            HofOp::Sorted | HofOp::Min | HofOp::Max => array,
            HofOp::CompactMap => {
                let Some(inner) = sigs.unwrapped(result) else {
                    return Err(CheckError::at(
                        line,
                        "compactMap needs a closure that returns an optional",
                    ));
                };
                sigs.array_of(inner, line)?
            }
            HofOp::FlatMap => {
                if sigs.array_element(result).is_none() {
                    return Err(CheckError::at(
                        line,
                        "flatMap needs a closure that returns an array",
                    ));
                }
                result
            }
            HofOp::CountWhere => Ty::Int,
        };
        let mut arguments = vec![receiver.clone()];
        arguments.extend(extra);
        arguments.push(function);
        Ok(Some(TExpr::Call {
            callee: op.callee().to_string(),
            type_args: vec![array, function_ty],
            args: arguments,
            ret,
        }))
    }

    fn array_taking(
        &mut self,
        line: u32,
        base: &Expr,
        receiver: TExpr,
        elem: Ty,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let ty = receiver.ty();
        let get = |array: TExpr, position: TExpr| TExpr::ArrayOp {
            op: ArrayOp::Get,
            elem,
            args: vec![array, position],
            ty: elem,
        };
        match name {
            "remove" => {
                let usage = "remove takes 'at:' with a position";
                let [at] = args else {
                    return Err(CheckError::at(line, usage));
                };
                if at.label() != Some("at") {
                    return Err(CheckError::at(line, usage));
                }
                let position = self.int_value(at.value())?;
                let held = self.temp(Ty::Int);
                let value = get(
                    receiver,
                    TExpr::Bind {
                        local: held,
                        init: Box::new(position),
                    },
                );
                let after = self.update_stmts(line, base, &Update::Remove(args))?;
                Ok(TExpr::Then {
                    value: Box::new(value),
                    after,
                })
            }
            _ => {
                if !args.is_empty() {
                    return Err(CheckError::at(line, format!("{name} takes no arguments")));
                }
                let held = self.temp(ty);
                let last = TExpr::Binary {
                    op: BinOp::Sub,
                    lhs: Box::new(array_count(held, ty)),
                    rhs: Box::new(TExpr::Int(1)),
                };
                let bound = TExpr::Bind {
                    local: held,
                    init: Box::new(receiver),
                };
                let removal = self.update_stmts(line, base, &Update::RemoveLast(args))?;
                if name == "removeLast" {
                    return Ok(TExpr::Then {
                        value: Box::new(get(bound, last)),
                        after: removal,
                    });
                }
                let optional = self.sigs.optional_of(elem)?;
                let empty = TExpr::Binary {
                    op: BinOp::Eq,
                    lhs: Box::new(TExpr::Field {
                        base: Box::new(bound),
                        index: ARRAY_COUNT,
                        ty: Ty::Int,
                    }),
                    rhs: Box::new(TExpr::Int(0)),
                };
                let value = TExpr::Select {
                    cond: Box::new(empty),
                    then: Box::new(self.none_of(optional)),
                    otherwise: Box::new(TExpr::StructInit {
                        ty: optional,
                        args: vec![
                            TExpr::Int(SOME_CASE as i64),
                            get(TExpr::Local(held, ty), last),
                        ],
                    }),
                    ty: optional,
                };
                let present = TExpr::Binary {
                    op: BinOp::Ne,
                    lhs: Box::new(TExpr::Field {
                        base: Box::new(self.value(base)?),
                        index: ARRAY_COUNT,
                        ty: Ty::Int,
                    }),
                    rhs: Box::new(TExpr::Int(0)),
                };
                Ok(TExpr::Then {
                    value: Box::new(value),
                    after: vec![TStmt::If {
                        cond: present,
                        then_body: removal,
                        else_body: Vec::new(),
                    }],
                })
            }
        }
    }

    fn array_end(&mut self, array: TExpr, elem: Ty, first: bool) -> CResult<TExpr> {
        let optional = self.sigs.optional_of(elem)?;
        let ty = array.ty();
        let held = self.temp(ty);
        let position = if first {
            TExpr::Int(0)
        } else {
            TExpr::Binary {
                op: BinOp::Sub,
                lhs: Box::new(array_count(held, ty)),
                rhs: Box::new(TExpr::Int(1)),
            }
        };
        Ok(TExpr::Select {
            cond: Box::new(TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(TExpr::Field {
                    base: Box::new(TExpr::Bind {
                        local: held,
                        init: Box::new(array),
                    }),
                    index: ARRAY_COUNT,
                    ty: Ty::Int,
                }),
                rhs: Box::new(TExpr::Int(0)),
            }),
            then: Box::new(self.none_of(optional)),
            otherwise: Box::new(TExpr::StructInit {
                ty: optional,
                args: vec![
                    TExpr::Int(SOME_CASE as i64),
                    TExpr::ArrayOp {
                        op: ArrayOp::Get,
                        elem,
                        args: vec![TExpr::Local(held, ty), position],
                        ty: elem,
                    },
                ],
            }),
            ty: optional,
        })
    }

    fn child(&self, name: String, ret: Ty) -> FnCx<'a> {
        FnCx {
            sigs: self.sigs,
            scope: self.scope.clone(),
            expected: None,
            ret,
            locals: Vec::new(),
            scopes: vec![HashMap::new()],
            init_self: None,
            cell_self: None,
            cell_call: CellCall::default(),
            super_initialized: false,
            chain_values: Vec::new(),
            targets: Vec::new(),
            pending_label: None,
            bounds: self.bounds.clone(),
            name,
            owner: self.owner.clone(),
            counter: Rc::clone(&self.counter),
            captured: Vec::new(),
            environment: Vec::new(),
            labels: HashMap::new(),
        }
    }

    fn generic_identity(&self) -> Vec<Ty> {
        let count = self
            .scope
            .iter()
            .filter(|(_, ty)| matches!(ty, Ty::Param(_)))
            .count();
        if count == 0 {
            vec![Ty::Int]
        } else {
            (0..count).map(Ty::Param).collect()
        }
    }

    fn next_closure_name(&self) -> String {
        let number = self.counter.get();
        self.counter.set(number + 1);
        format!("{}.$closure{}", self.owner, number)
    }

    #[allow(clippy::too_many_arguments)]
    fn lift(
        &mut self,
        line: u32,
        names: &[String],
        parameters: &[(String, Ty)],
        ret: Ty,
        self_name: Option<(&str, Ty)>,
        body: impl FnOnce(&mut FnCx<'a>) -> CResult<Vec<TStmt>>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let name = self.next_closure_name();
        let mut cx = self.child(name.clone(), ret);
        cx.declare(line, "$env", Ty::Int, false)?;
        for (param, ty) in parameters {
            cx.declare(line, param, *ty, false)?;
        }
        let mut candidates: Vec<String> = names.to_vec();
        if !candidates.iter().any(|n| n == "self") {
            candidates.push("self".to_string());
        }
        let mut captures = Vec::new();
        for candidate in &candidates {
            if parameters.iter().any(|(p, _)| p == candidate) {
                continue;
            }
            if self_name.is_some_and(|(own, _)| own == candidate) {
                continue;
            }
            let Ok(outer) = self.lookup(candidate, line) else {
                continue;
            };
            let inner = cx.declare(line, candidate, outer.ty, outer.mutable)?;
            cx.environment.push((inner, captures.len()));
            let already_a_cell = self.environment.iter().any(|(l, _)| *l == outer.local);
            if !already_a_cell && !self.captured.contains(&outer.local) {
                self.captured.push(outer.local);
            }
            captures.push(TExpr::Capture(outer.local, outer.ty));
        }
        let mut prologue = Vec::new();
        if let Some((own, ty)) = self_name {
            let local = cx.declare(line, own, ty, false)?;
            prologue.push(TStmt::Let {
                local,
                init: TExpr::Local(LocalId::new(0), ty),
            });
        }
        let mut statements = body(&mut cx)?;
        statements.splice(0..0, prologue);
        let param_types: Vec<Ty> = parameters.iter().map(|(_, ty)| *ty).collect();
        let typed = TFunc::new(
            name.clone(),
            1 + parameters.len(),
            cx.ret,
            cx.locals.clone(),
            statements,
        );
        let lowered = crate::capture::lower(&typed, &cx.captured, &cx.environment, sigs);
        sigs.register_closure(name.clone(), Rc::new(lowered));
        let ty = sigs.func_ty(param_types.clone(), cx.ret);
        Ok(TExpr::Closure {
            func: name,
            type_args: self.generic_identity(),
            captures,
            params: param_types,
            ret: cx.ret,
            ty,
            drop: String::new(),
        })
    }

    fn closure_value(
        &mut self,
        line: u32,
        closure: &ClosureExpr,
        expected_params: Option<Vec<Ty>>,
        expected_ret: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        if let (Some(list), Some(tys)) = (closure.params(), &expected_params)
            && list.len() != tys.len()
        {
            return Err(CheckError::at(
                line,
                format!(
                    "this closure takes {} parameter(s), but {} are expected",
                    list.len(),
                    tys.len()
                ),
            ));
        }
        let mut parameters: Vec<(String, Ty)> = Vec::new();
        match closure.params() {
            Some(list) => {
                for (index, param) in list.iter().enumerate() {
                    let ty = match param.ty() {
                        Some(annotation) => sigs.resolve_type(annotation, &self.scope, line)?,
                        None => expected_params
                            .as_ref()
                            .and_then(|tys| tys.get(index).copied())
                            .ok_or_else(|| {
                                CheckError::at(
                                    line,
                                    format!(
                                        "cannot infer the type of '{}': write it in the closure",
                                        param.name()
                                    ),
                                )
                            })?,
                    };
                    parameters.push((param.name().to_string(), ty));
                }
            }
            None => {
                for (index, ty) in expected_params.iter().flatten().enumerate() {
                    parameters.push((format!("${index}"), *ty));
                }
            }
        }
        if let Some(tys) = &expected_params
            && tys.len() != parameters.len()
        {
            return Err(CheckError::at(
                line,
                format!(
                    "this closure takes {} parameter(s), but {} are expected",
                    parameters.len(),
                    tys.len()
                ),
            ));
        }
        let annotated = match closure.ret() {
            Some(tr) if tr.name() == "Void" && tr.args().is_empty() => Some(Ty::Void),
            Some(tr) => Some(sigs.resolve_type(tr, &self.scope, line)?),
            None => expected_ret,
        };
        let names = crate::free::names_in(closure.body());
        let statements = closure.body();
        let single = match statements {
            [only] => match only.kind() {
                StmtKind::Expr(expr) => Some(expr),
                _ => None,
            },
            _ => None,
        };
        let ret = annotated.unwrap_or(Ty::Void);
        if annotated.is_none() && single.is_none() && !statements.is_empty() {
            return Err(CheckError::at(
                line,
                "cannot infer the result of this closure: write '-> Type' after its parameters",
            ));
        }
        self.lift(line, &names, &parameters, ret, None, move |cx| {
            let mut body: Vec<TStmt> = Vec::new();
            if annotated.is_none() {
                if let Some(expr) = single {
                    let value = cx.expr(expr)?;
                    if value.ty() == Ty::Void {
                        body.push(TStmt::Eval(value));
                    } else {
                        cx.ret = value.ty();
                        body.push(TStmt::Return(Some(value)));
                    }
                }
                return Ok(body);
            }
            let returns_value = ret != Ty::Void;
            let last_is_value = returns_value
                && matches!(statements.last().map(Stmt::kind), Some(StmtKind::Expr(_)));
            if last_is_value {
                let (init, last) = statements.split_at(statements.len() - 1);
                body = cx.block(init)?;
                if let Some(StmtKind::Expr(expr)) = last.first().map(Stmt::kind) {
                    cx.expected = Some(ret);
                    let value = cx.value(expr)?;
                    let value = cx.coerce(line, ret, value)?;
                    body.push(TStmt::Return(Some(value)));
                }
            } else {
                body = cx.block(statements)?;
                if returns_value && !always_returns(statements) {
                    return Err(CheckError::at(
                        line,
                        "this closure must return a value on every path",
                    ));
                }
            }
            Ok(body)
        })
    }

    fn closure_into(
        &mut self,
        line: u32,
        closure: &ClosureExpr,
        accumulator: Ty,
        elem: Ty,
    ) -> CResult<TExpr> {
        let (first, second) = match closure.params() {
            Some([a, b]) => (a.name().to_string(), b.name().to_string()),
            None => ("$0".to_string(), "$1".to_string()),
            Some(_) => {
                return Err(CheckError::at(
                    line,
                    "reduce(into:) takes a closure with two parameters",
                ));
            }
        };
        let names = crate::free::names_in(closure.body());
        let parameters = vec![(first.clone(), accumulator), (second, elem)];
        let statements = closure.body();
        self.lift(line, &names, &parameters, accumulator, None, move |cx| {
            let copy = LocalId::new(cx.locals.len());
            cx.locals.push(accumulator);
            if let Some(scope) = cx.scopes.first_mut() {
                scope.insert(
                    first.clone(),
                    Binding {
                        local: copy,
                        ty: accumulator,
                        mutable: true,
                    },
                );
            }
            let mut body = vec![TStmt::Let {
                local: copy,
                init: TExpr::Local(LocalId::new(1), accumulator),
            }];
            body.extend(cx.block(statements)?);
            body.push(TStmt::Return(Some(TExpr::Local(copy, accumulator))));
            Ok(body)
        })
    }

    fn local_function(&mut self, line: u32, func: &FuncDecl) -> CResult<TStmt> {
        let sigs = self.sigs;
        if !func.generics().is_empty() {
            return Err(CheckError::at(
                line,
                "a function declared inside another function cannot be generic yet",
            ));
        }
        let mut parameters = Vec::new();
        let mut labels = Vec::new();
        for param in func.params() {
            let ty = sigs.resolve_type(param.ty(), &self.scope, line)?;
            parameters.push((param.name().to_string(), ty));
            labels.push(param.label().map(str::to_string));
        }
        let ret = match func.ret() {
            Some(tr) if tr.name() == "Void" && tr.args().is_empty() => Ty::Void,
            Some(tr) => sigs.resolve_type(tr, &self.scope, line)?,
            None => Ty::Void,
        };
        let own_ty = sigs.func_ty(parameters.iter().map(|(_, ty)| *ty).collect(), ret);
        let names = crate::free::names_in(func.body());
        let body = func.body();
        let closure = self.lift(
            line,
            &names,
            &parameters,
            ret,
            Some((func.name(), own_ty)),
            |cx| {
                let statements = cx.block(body)?;
                if ret != Ty::Void && !always_returns(body) {
                    return Err(CheckError::at(
                        line,
                        format!(
                            "function '{}' must return a value on every path",
                            func.name()
                        ),
                    ));
                }
                Ok(statements)
            },
        )?;
        let local = self.declare(line, func.name(), own_ty, false)?;
        self.labels.insert(local, labels);
        Ok(TStmt::Let {
            local,
            init: closure,
        })
    }

    fn call_value(
        &mut self,
        line: u32,
        callee: &str,
        binding: Binding,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let labels = self.labels.get(&binding.local).cloned();
        let target = TExpr::Local(binding.local, binding.ty);
        self.apply_value(line, callee, target, labels, args)
    }

    fn apply_value(
        &mut self,
        line: u32,
        callee: &str,
        target: TExpr,
        labels: Option<Vec<Option<String>>>,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let Some((params, ret)) = sigs.func_parts(target.ty()) else {
            return Err(CheckError::at(
                line,
                format!("{} is not a function", self.name_of(target.ty())),
            ));
        };
        if args.len() != params.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{callee}' takes {} argument(s), but {} were given",
                    params.len(),
                    args.len()
                ),
            ));
        }
        let mut checked = Vec::with_capacity(args.len());
        for (index, (arg, param)) in args.iter().zip(&params).enumerate() {
            let wanted = labels
                .as_ref()
                .and_then(|l| l.get(index).cloned())
                .unwrap_or(None);
            self.expect_label(line, arg, wanted.as_deref())?;
            self.expected = Some(*param);
            let value = self.value(arg.value())?;
            checked.push(self.coerce(line, *param, value)?);
        }
        Ok(TExpr::CallValue {
            func: Box::new(target),
            args: checked,
            params,
            ret,
        })
    }

    fn array_literal(&mut self, line: u32, items: &[Expr], expected: Option<Ty>) -> CResult<TExpr> {
        if let Some(ty) = expected
            && self.sigs.is_set(ty)
        {
            return self.set_literal(line, items, ty);
        }
        let mut element = expected.and_then(|ty| self.sigs.array_element(ty));
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            self.expected = element;
            let mut value = self.value(item)?;
            match element {
                Some(ty) => value = self.coerce(item.line(), ty, value)?,
                None => element = Some(value.ty()),
            }
            values.push(value);
        }
        let Some(element) = element else {
            return Err(CheckError::at(
                line,
                "an empty array needs a type from its context, such as 'let x: [Int] = []'",
            ));
        };
        let ty = self.sigs.array_of(element, line)?;
        let count = i64::try_from(values.len()).unwrap_or(i64::MAX);
        let buffer = TExpr::ArrayOp {
            op: ArrayOp::Buffer,
            elem: element,
            args: values,
            ty: Ty::Int,
        };
        Ok(self
            .sigs
            .construct(ty, vec![TExpr::Int(count), TExpr::Int(count), buffer]))
    }

    fn subscript(&mut self, line: u32, base: &Expr, index: &Expr) -> CResult<TExpr> {
        let array = self.value(base)?;
        if array.ty() == Ty::Str {
            return self.string_subscript(array, index);
        }
        if let Some((key, value)) = self.sigs.table_parts(array.ty())
            && !self.sigs.is_set(array.ty())
        {
            return self.lookup_entry(line, array, key, value, index, None);
        }
        let Some(elem) = self.sigs.array_element(array.ty()) else {
            return Err(CheckError::at(
                line,
                format!("{} cannot be subscripted", self.name_of(array.ty())),
            ));
        };
        let position = self.int_value(index)?;
        Ok(TExpr::ArrayOp {
            op: ArrayOp::Get,
            elem,
            args: vec![array, position],
            ty: elem,
        })
    }

    fn dict_literal(
        &mut self,
        line: u32,
        pairs: &[(Expr, Expr)],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let hint = expected
            .and_then(|ty| self.sigs.table_parts(ty))
            .filter(|_| expected.is_some_and(|ty| !self.sigs.is_set(ty)));
        let mut key = hint.map(|(key, _)| key);
        let mut value = hint.map(|(_, value)| value);
        let mut entries = Vec::with_capacity(pairs.len());
        for (key_expr, value_expr) in pairs {
            self.expected = key;
            let mut k = self.value(key_expr)?;
            match key {
                Some(ty) => k = self.coerce(key_expr.line(), ty, k)?,
                None => key = Some(k.ty()),
            }
            self.expected = value;
            let mut v = self.value(value_expr)?;
            match value {
                Some(ty) => v = self.coerce(value_expr.line(), ty, v)?,
                None => value = Some(v.ty()),
            }
            entries.push((k, v));
        }
        let (Some(key), Some(value)) = (key, value) else {
            return Err(CheckError::at(
                line,
                "an empty dictionary needs a type from its context, such as 'let x: [String: Int] = [:]'",
            ));
        };
        let ty = self.sigs.dictionary_of(key, value, line)?;
        let mut table = empty_table(self.sigs, ty);
        for (k, v) in entries {
            table = table_op(DictOp::Set, key, value, vec![table, k, v], ty);
        }
        Ok(table)
    }

    fn set_literal(&mut self, line: u32, items: &[Expr], ty: Ty) -> CResult<TExpr> {
        let (element, _) = self
            .sigs
            .table_parts(ty)
            .ok_or_else(|| CheckError::at(line, "internal error: not a set"))?;
        let mut table = empty_table(self.sigs, ty);
        for item in items {
            self.expected = Some(element);
            let value = self.value(item)?;
            let value = self.coerce(item.line(), element, value)?;
            table = table_op(DictOp::Set, element, Ty::Void, vec![table, value], ty);
        }
        Ok(table)
    }

    fn lookup_entry(
        &mut self,
        line: u32,
        table: TExpr,
        key: Ty,
        value: Ty,
        key_expr: &Expr,
        default: Option<&Expr>,
    ) -> CResult<TExpr> {
        self.expected = Some(key);
        let wanted = self.value(key_expr)?;
        let wanted = self.coerce(line, key, wanted)?;
        let ty = table.ty();
        let held = self.temp(ty);
        let at = self.temp(Ty::Int);
        let found = table_op(
            DictOp::Find,
            key,
            value,
            vec![
                TExpr::Bind {
                    local: held,
                    init: Box::new(table),
                },
                wanted,
            ],
            Ty::Int,
        );
        let missing = compare(
            BinOp::Lt,
            TExpr::Bind {
                local: at,
                init: Box::new(found),
            },
            TExpr::Int(0),
        );
        let stored = table_op(
            DictOp::Value,
            key,
            value,
            vec![local(held, ty), local(at, Ty::Int)],
            value,
        );
        match default {
            None => {
                let optional = self.sigs.optional_of(value)?;
                Ok(TExpr::Select {
                    cond: Box::new(missing),
                    then: Box::new(self.none_of(optional)),
                    otherwise: Box::new(TExpr::StructInit {
                        ty: optional,
                        args: vec![TExpr::Int(SOME_CASE as i64), stored],
                    }),
                    ty: optional,
                })
            }
            Some(default) => {
                self.expected = Some(value);
                let fallback = self.value(default)?;
                let fallback = self.coerce(line, value, fallback)?;
                Ok(TExpr::Select {
                    cond: Box::new(missing),
                    then: Box::new(fallback),
                    otherwise: Box::new(stored),
                    ty: value,
                })
            }
        }
    }

    fn for_pair(
        &mut self,
        line: u32,
        key_name: &str,
        value_name: &str,
        iter: &Expr,
        body: &[Stmt],
    ) -> CResult<TStmt> {
        let table = self.value(iter)?;
        match self.sigs.table_parts(table.ty()) {
            Some((key, value)) if !self.sigs.is_set(table.ty()) => {
                self.for_table(line, table, key, value, key_name, Some(value_name), body)
            }
            _ => Err(CheckError::at(
                line,
                "a loop over (key, value) needs a dictionary",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn for_table(
        &mut self,
        line: u32,
        table: TExpr,
        key: Ty,
        value: Ty,
        key_name: &str,
        value_name: Option<&str>,
        body: &[Stmt],
    ) -> CResult<TStmt> {
        let ty = table.ty();
        let held = self.temp(ty);
        let position = self.temp(Ty::Int);
        let index = self.temp(Ty::Int);
        self.scopes.push(HashMap::new());
        let result = self
            .declare(line, key_name, key, false)
            .and_then(|key_local| {
                let value_local = match value_name {
                    Some(name) => Some(self.declare(line, name, value, false)?),
                    None => None,
                };
                let entry = |op: DictOp, ty_of: Ty| {
                    table_op(
                        op,
                        key,
                        value,
                        vec![local(held, ty), local(index, Ty::Int)],
                        ty_of,
                    )
                };
                let mut inner = vec![TStmt::Let {
                    local: key_local,
                    init: entry(DictOp::Key, key),
                }];
                if let Some(value_local) = value_local {
                    inner.push(TStmt::Let {
                        local: value_local,
                        init: entry(DictOp::Value, value),
                    });
                }
                inner.extend(self.in_target(TargetKind::Loop, |cx| cx.block(body))?);
                Ok(TStmt::While {
                    cond: compare(
                        BinOp::Lt,
                        local(position, Ty::Int),
                        TExpr::Field {
                            base: Box::new(local(held, ty)),
                            index: DICT_USED,
                            ty: Ty::Int,
                        },
                    ),
                    body: vec![
                        TStmt::Let {
                            local: index,
                            init: local(position, Ty::Int),
                        },
                        step(position),
                        TStmt::If {
                            cond: entry(DictOp::Live, Ty::Bool),
                            then_body: inner,
                            else_body: Vec::new(),
                        },
                    ],
                })
            });
        self.scopes.pop();
        Ok(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: vec![
                TStmt::Let {
                    local: held,
                    init: table,
                },
                TStmt::Let {
                    local: position,
                    init: TExpr::Int(0),
                },
                result?,
            ],
            else_body: Vec::new(),
        })
    }

    fn collection_init(
        &mut self,
        line: u32,
        name: &str,
        explicit: Option<&[TypeRef]>,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let declared = match explicit {
            Some(list) if !list.is_empty() => Some(sigs.resolve_type(
                &TypeRef::new(name.to_string(), list.to_vec()),
                &self.scope,
                line,
            )?),
            _ => None,
        };
        let fits = |ty: Ty| match name {
            SET => sigs.is_set(ty),
            DICTIONARY => sigs.table_parts(ty).is_some() && !sigs.is_set(ty),
            _ => sigs.array_element(ty).is_some(),
        };
        let contextual = declared.or_else(|| expected.filter(|ty| fits(*ty)));
        let unlabeled = |arg: &Arg| arg.label().is_none();
        match (name, args) {
            (DICTIONARY, []) => {
                let ty = contextual.ok_or_else(|| {
                    CheckError::at(
                        line,
                        "Dictionary needs its key and value types, such as '[String: Int]()'",
                    )
                })?;
                Ok(empty_table(sigs, ty))
            }
            (DICTIONARY, [grouping, by])
                if grouping.label() == Some("grouping") && by.label() == Some("by") =>
            {
                self.expected = None;
                let source = self.value(grouping.value())?;
                let Some(element) = sigs.array_element(source.ty()) else {
                    return Err(CheckError::at(
                        line,
                        format!("cannot group {}", self.name_of(source.ty())),
                    ));
                };
                let function = self.callable(line, by, vec![element], None)?;
                let function_ty = function.ty();
                let Some((_, key)) = sigs.func_parts(function_ty) else {
                    return Err(CheckError::at(line, "internal error: not a function"));
                };
                let groups = sigs.array_of(element, line)?;
                let ret = sigs.dictionary_of(key, groups, line)?;
                Ok(TExpr::Call {
                    callee: EntryOp::Grouping.callee().to_string(),
                    type_args: vec![source.ty(), function_ty],
                    args: vec![source, function],
                    ret,
                })
            }
            (DICTIONARY, _) => Err(CheckError::at(
                line,
                "Dictionary takes no arguments, or 'grouping:' and 'by:'",
            )),
            (SET, []) => {
                let ty = contextual.ok_or_else(|| {
                    CheckError::at(line, "Set needs its element type, such as 'Set<Int>()'")
                })?;
                Ok(empty_table(sigs, ty))
            }
            (ARRAY, []) => {
                let ty = contextual.ok_or_else(|| {
                    CheckError::at(line, "Array needs its element type, such as '[Int]()'")
                })?;
                let element = self.array_target(line, ty)?;
                Ok(empty_array(sigs, ty, element))
            }
            (SET | ARRAY, [arg]) if unlabeled(arg) => {
                let hint = contextual.and_then(|ty| match name {
                    SET => sigs.table_parts(ty).map(|(element, _)| element),
                    _ => sigs.array_element(ty),
                });
                self.expected = match hint {
                    Some(element) => Some(sigs.array_of(element, line)?),
                    None => None,
                };
                let source = self.value(arg.value())?;
                self.collection_from(line, name, source, contextual)
            }
            (ARRAY, [value, count])
                if unlabeled(value)
                    || (value.label() == Some("repeating") && count.label() == Some("count")) =>
            {
                if value.label() != Some("repeating") || count.label() != Some("count") {
                    return Err(CheckError::at(
                        line,
                        "Array takes 'repeating:' and 'count:'",
                    ));
                }
                let hint = contextual.and_then(|ty| sigs.array_element(ty));
                self.expected = hint;
                let mut fill = self.value(value.value())?;
                let element = match hint {
                    Some(element) => {
                        fill = self.coerce(line, element, fill)?;
                        element
                    }
                    None => fill.ty(),
                };
                let ty = sigs.array_of(element, line)?;
                let total = self.int_value(count.value())?;
                Ok(TExpr::Call {
                    callee: REPEATING.to_string(),
                    type_args: vec![ty],
                    args: vec![fill, total],
                    ret: ty,
                })
            }
            (SET, _) => Err(CheckError::at(
                line,
                "Set takes one sequence of its elements",
            )),
            _ => Err(CheckError::at(
                line,
                "Array takes a set or an array, or 'repeating:' and 'count:'",
            )),
        }
    }

    fn collection_from(
        &mut self,
        line: u32,
        name: &str,
        source: TExpr,
        contextual: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let ty = source.ty();
        if name == SET {
            if sigs.is_set(ty) {
                return self.coerce(line, contextual.unwrap_or(ty), source);
            }
            let Some(element) = sigs.array_element(ty) else {
                return Err(CheckError::at(
                    line,
                    format!("Set cannot be built from {}", self.name_of(ty)),
                ));
            };
            let set = sigs.set_of(element, line)?;
            let built = TExpr::Call {
                callee: TO_SET.to_string(),
                type_args: vec![ty],
                args: vec![source],
                ret: set,
            };
            return self.coerce(line, contextual.unwrap_or(set), built);
        }
        if sigs.array_element(ty).is_some() {
            return self.coerce(line, contextual.unwrap_or(ty), source);
        }
        if let Some((element, _)) = sigs.table_parts(ty).filter(|_| sigs.is_set(ty)) {
            let array = sigs.array_of(element, line)?;
            let listing = TExpr::Call {
                callee: KEYS.to_string(),
                type_args: vec![ty],
                args: vec![source],
                ret: array,
            };
            return self.coerce(line, contextual.unwrap_or(array), listing);
        }
        Err(CheckError::at(
            line,
            format!("Array cannot be built from {}", self.name_of(ty)),
        ))
    }

    fn set_operand(
        &mut self,
        line: u32,
        name: &str,
        label: Option<&str>,
        ty: Ty,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let usage = match label {
            Some(label) => format!("'{name}' takes one set with the label '{label}:'"),
            None => format!("'{name}' takes one unlabeled set"),
        };
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label() != label {
            return Err(CheckError::at(line, usage));
        }
        self.expected = Some(ty);
        let other = self.value(arg.value())?;
        self.coerce(line, ty, other)
    }

    fn is_entry_closure(&self, name: &str, args: &[Arg]) -> bool {
        match (name, args) {
            ("mapValues" | "filter" | "forEach" | "allSatisfy", [_]) => true,
            ("contains", [arg]) => {
                arg.label() == Some("where") || matches!(arg.value().kind(), ExprKind::Closure(_))
            }
            _ => false,
        }
    }

    fn dictionary_closure(
        &mut self,
        line: u32,
        receiver: TExpr,
        key: Ty,
        value: Ty,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let table = receiver.ty();
        let (op, params, expected_ret) = match name {
            "mapValues" => (EntryOp::MapValues, vec![value], None),
            "filter" => (EntryOp::Filter, vec![key, value], Some(Ty::Bool)),
            "forEach" => (EntryOp::ForEach, vec![key, value], Some(Ty::Void)),
            "allSatisfy" => (EntryOp::AllSatisfy, vec![key, value], Some(Ty::Bool)),
            _ => (EntryOp::ContainsWhere, vec![key, value], Some(Ty::Bool)),
        };
        if name != "contains" && args[0].label().is_some() {
            return Err(CheckError::at(line, format!("'{name}' takes a closure")));
        }
        let function = self.callable(line, &args[0], params, expected_ret)?;
        let function_ty = function.ty();
        let Some((_, result)) = sigs.func_parts(function_ty) else {
            return Err(CheckError::at(line, "internal error: not a function"));
        };
        let ret = match op {
            EntryOp::MapValues => {
                if result == Ty::Void {
                    return Err(CheckError::at(
                        line,
                        "mapValues needs a closure that returns a value",
                    ));
                }
                sigs.dictionary_of(key, result, line)?
            }
            EntryOp::Filter => table,
            EntryOp::ForEach => Ty::Void,
            _ => Ty::Bool,
        };
        Ok(TExpr::Call {
            callee: op.callee().to_string(),
            type_args: vec![table, function_ty],
            args: vec![receiver, function],
            ret,
        })
    }

    fn set_closure_name(&self, name: &str, args: &[Arg]) -> bool {
        match (name, args) {
            ("map" | "filter" | "forEach" | "allSatisfy" | "compactMap" | "flatMap", [_])
            | ("reduce", [_, _]) => true,
            ("sorted" | "min" | "max", []) => true,
            ("sorted" | "min" | "max", [arg]) => {
                arg.label() == Some("by") || matches!(arg.value().kind(), ExprKind::Closure(_))
            }
            ("count", [arg]) => arg.label() == Some("where"),
            ("contains" | "first", [arg]) => {
                arg.label() == Some("where") || matches!(arg.value().kind(), ExprKind::Closure(_))
            }
            _ => false,
        }
    }

    fn set_closure(
        &mut self,
        line: u32,
        receiver: TExpr,
        element: Ty,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let set = receiver.ty();
        let array = sigs.array_of(element, line)?;
        let listing = TExpr::Call {
            callee: KEYS.to_string(),
            type_args: vec![set],
            args: vec![receiver],
            ret: array,
        };
        let Some(call) = self.array_closure_method(line, &listing, element, name, args)? else {
            return Err(CheckError::at(
                line,
                format!("{} has no method '{name}'", self.name_of(set)),
            ));
        };
        if name == "filter" {
            return Ok(TExpr::Call {
                callee: TO_SET.to_string(),
                type_args: vec![array],
                args: vec![call],
                ret: set,
            });
        }
        Ok(call)
    }

    fn table_method(
        &mut self,
        line: u32,
        base: &Expr,
        receiver: TExpr,
        names: (Ty, Ty),
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let (key, value) = names;
        let ty = receiver.ty();
        let set = self.sigs.is_set(ty);
        match (set, name) {
            (false, "mapValues" | "filter" | "forEach" | "allSatisfy")
            | (false, "contains" | "first")
                if self.is_entry_closure(name, args) =>
            {
                self.dictionary_closure(line, receiver, key, value, name, args)
            }
            (true, _) if self.set_closure_name(name, args) => {
                self.set_closure(line, receiver, key, name, args)
            }
            (true, "union" | "intersection" | "subtracting" | "symmetricDifference") => {
                let op = match name {
                    "union" => SetOp::Union,
                    "intersection" => SetOp::Intersection,
                    "subtracting" => SetOp::Subtracting,
                    _ => SetOp::Symmetric,
                };
                let other = self.set_operand(line, name, None, ty, args)?;
                Ok(TExpr::Call {
                    callee: op.callee().to_string(),
                    type_args: vec![ty],
                    args: vec![receiver, other],
                    ret: ty,
                })
            }
            (
                true,
                "isSubset" | "isSuperset" | "isStrictSubset" | "isStrictSuperset" | "isDisjoint",
            ) => {
                let label = if name == "isDisjoint" { "with" } else { "of" };
                let other = self.set_operand(line, name, Some(label), ty, args)?;
                let (op, arguments) = match name {
                    "isSubset" => (SetOp::Subset, vec![receiver, other]),
                    "isSuperset" => (SetOp::Subset, vec![other, receiver]),
                    "isStrictSubset" => (SetOp::StrictSubset, vec![receiver, other]),
                    "isStrictSuperset" => (SetOp::StrictSubset, vec![other, receiver]),
                    _ => (SetOp::Disjoint, vec![receiver, other]),
                };
                Ok(TExpr::Call {
                    callee: op.callee().to_string(),
                    type_args: vec![ty],
                    args: arguments,
                    ret: Ty::Bool,
                })
            }
            (true, "contains") => {
                let [arg] = args else {
                    return Err(CheckError::at(line, "contains takes one unlabeled value"));
                };
                if arg.label().is_some() {
                    return Err(CheckError::at(line, "contains takes one unlabeled value"));
                }
                self.expected = Some(key);
                let wanted = self.value(arg.value())?;
                let wanted = self.coerce(line, key, wanted)?;
                Ok(compare(
                    BinOp::Ge,
                    table_op(DictOp::Find, key, value, vec![receiver, wanted], Ty::Int),
                    TExpr::Int(0),
                ))
            }
            (false, "removeValue") => {
                let usage = "removeValue takes the label 'forKey:'";
                let [arg] = args else {
                    return Err(CheckError::at(line, usage));
                };
                if arg.label() != Some("forKey") {
                    return Err(CheckError::at(line, usage));
                }
                let found = self.lookup_entry(line, receiver, key, value, arg.value(), None)?;
                let nil = Expr::new(line, ExprKind::Nil);
                let after = self.update_stmts(
                    line,
                    base,
                    &Update::Entry {
                        key: arg.value(),
                        default: None,
                        op: None,
                        value: &nil,
                    },
                )?;
                Ok(TExpr::Then {
                    value: Box::new(found),
                    after,
                })
            }
            (false, "updateValue") => {
                let usage = "updateValue takes a value and the label 'forKey:'";
                let [new, at] = args else {
                    return Err(CheckError::at(line, usage));
                };
                if new.label().is_some() || at.label() != Some("forKey") {
                    return Err(CheckError::at(line, usage));
                }
                let found = self.lookup_entry(line, receiver, key, value, at.value(), None)?;
                let after = self.update_stmts(
                    line,
                    base,
                    &Update::Entry {
                        key: at.value(),
                        default: None,
                        op: None,
                        value: new.value(),
                    },
                )?;
                Ok(TExpr::Then {
                    value: Box::new(found),
                    after,
                })
            }
            (
                _,
                "insert"
                | "remove"
                | "removeAll"
                | "formUnion"
                | "formIntersection"
                | "subtract"
                | "formSymmetricDifference",
            ) => Err(CheckError::at(
                line,
                format!("'{name}' changes the collection: write it as a statement of its own"),
            )),
            _ => Err(CheckError::at(
                line,
                format!("{} has no method '{name}'", self.name_of(ty)),
            )),
        }
    }

    fn for_elements(
        &mut self,
        line: u32,
        var: &str,
        array: TExpr,
        elem: Ty,
        body: &[Stmt],
    ) -> CResult<TStmt> {
        let array_ty = array.ty();
        let held = self.temp(array_ty);
        let position = self.temp(Ty::Int);
        let count = |held: LocalId| TExpr::Field {
            base: Box::new(TExpr::Local(held, array_ty)),
            index: ARRAY_COUNT,
            ty: Ty::Int,
        };
        self.scopes.push(HashMap::new());
        let result = self.declare(line, var, elem, false).and_then(|local| {
            let mut stmts = vec![
                TStmt::Let {
                    local,
                    init: TExpr::ArrayOp {
                        op: ArrayOp::Get,
                        elem,
                        args: vec![
                            TExpr::Local(held, array_ty),
                            TExpr::Local(position, Ty::Int),
                        ],
                        ty: elem,
                    },
                },
                TStmt::Assign {
                    local: position,
                    value: TExpr::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(TExpr::Local(position, Ty::Int)),
                        rhs: Box::new(TExpr::Int(1)),
                    },
                },
            ];
            stmts.extend(self.in_target(TargetKind::Loop, |cx| cx.block(body))?);
            Ok(TStmt::While {
                cond: TExpr::Binary {
                    op: BinOp::Lt,
                    lhs: Box::new(TExpr::Local(position, Ty::Int)),
                    rhs: Box::new(count(held)),
                },
                body: stmts,
            })
        });
        self.scopes.pop();
        Ok(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: vec![
                TStmt::Let {
                    local: held,
                    init: array,
                },
                TStmt::Let {
                    local: position,
                    init: TExpr::Int(0),
                },
                result?,
            ],
            else_body: Vec::new(),
        })
    }

    fn for_characters(
        &mut self,
        line: u32,
        var: &str,
        text: TExpr,
        body: &[Stmt],
    ) -> CResult<TStmt> {
        let held = self.temp(Ty::Str);
        let offset = self.temp(Ty::Int);
        self.scopes.push(HashMap::new());
        let result = self.declare(line, var, Ty::Char, false).and_then(|local| {
            let mut stmts = vec![
                TStmt::Let {
                    local,
                    init: TExpr::StrOp {
                        op: StrOp::CharAt,
                        args: vec![TExpr::Local(held, Ty::Str), TExpr::Local(offset, Ty::Int)],
                    },
                },
                TStmt::Assign {
                    local: offset,
                    value: TExpr::Binary {
                        op: BinOp::Add,
                        lhs: Box::new(TExpr::Local(offset, Ty::Int)),
                        rhs: Box::new(TExpr::StrOp {
                            op: StrOp::CharWidth,
                            args: vec![TExpr::Local(local, Ty::Char)],
                        }),
                    },
                },
            ];
            stmts.extend(self.in_target(TargetKind::Loop, |cx| cx.block(body))?);
            Ok(TStmt::While {
                cond: TExpr::Binary {
                    op: BinOp::Lt,
                    lhs: Box::new(TExpr::Local(offset, Ty::Int)),
                    rhs: Box::new(TExpr::StrOp {
                        op: StrOp::Count,
                        args: vec![TExpr::Local(held, Ty::Str)],
                    }),
                },
                body: stmts,
            })
        });
        self.scopes.pop();
        Ok(TStmt::If {
            cond: TExpr::Bool(true),
            then_body: vec![
                TStmt::Let {
                    local: held,
                    init: text,
                },
                TStmt::Let {
                    local: offset,
                    init: TExpr::Int(0),
                },
                result?,
            ],
            else_body: Vec::new(),
        })
    }

    fn character_literal(line: u32, text: &str) -> CResult<TExpr> {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(TExpr::Char(u32::from(c))),
            _ => Err(CheckError::at(
                line,
                "a Character literal must hold exactly one character",
            )),
        }
    }

    fn character_conversion(&mut self, line: u32, args: &[Arg]) -> CResult<TExpr> {
        let usage = "Character takes exactly one unlabeled argument";
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label().is_some() {
            return Err(CheckError::at(line, usage));
        }
        self.expected = Some(Ty::Char);
        let value = self.value(arg.value())?;
        match value.ty() {
            Ty::Char => Ok(value),
            Ty::Int => Ok(TExpr::StrOp {
                op: StrOp::CharFromInt,
                args: vec![value],
            }),
            Ty::Str => Ok(TExpr::StrOp {
                op: StrOp::CharFromString,
                args: vec![value],
            }),
            other => Err(CheckError::at(
                line,
                format!(
                    "cannot convert a value of type {} to Character",
                    self.name_of(other)
                ),
            )),
        }
    }

    fn character_property(&mut self, line: u32, base: TExpr, name: &str) -> CResult<TExpr> {
        let value = TExpr::StrOp {
            op: StrOp::CharValue,
            args: vec![base],
        };
        let ranges: &[(i64, i64)] = match name {
            "unicodeValue" => return Ok(value),
            "isLetter" => &[(65, 90), (97, 122)],
            "isNumber" => &[(48, 57)],
            "isUppercase" => &[(65, 90)],
            "isLowercase" => &[(97, 122)],
            "isWhitespace" => &[(9, 13), (32, 32)],
            "isPunctuation" => &[(33, 47), (58, 64), (91, 96), (123, 126)],
            "isASCII" => &[(0, 127)],
            _ => {
                return Err(CheckError::at(
                    line,
                    format!("Character has no member '{name}'"),
                ));
            }
        };
        let held = self.temp(Ty::Int);
        let mut source = Some(value);
        let mut any: Option<TExpr> = None;
        for (low, high) in ranges {
            let operand = match source.take() {
                Some(value) => TExpr::Bind {
                    local: held,
                    init: Box::new(value),
                },
                None => TExpr::Local(held, Ty::Int),
            };
            let inside = TExpr::Binary {
                op: BinOp::And,
                lhs: Box::new(TExpr::Binary {
                    op: BinOp::Ge,
                    lhs: Box::new(operand),
                    rhs: Box::new(TExpr::Int(*low)),
                }),
                rhs: Box::new(TExpr::Binary {
                    op: BinOp::Le,
                    lhs: Box::new(TExpr::Local(held, Ty::Int)),
                    rhs: Box::new(TExpr::Int(*high)),
                }),
            };
            any = Some(match any {
                None => inside,
                Some(before) => TExpr::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(before),
                    rhs: Box::new(inside),
                },
            });
        }
        Ok(any.unwrap_or(TExpr::Bool(false)))
    }

    fn expr_stmt(&mut self, expr: &Expr) -> CResult<TStmt> {
        let member_call = match expr.kind() {
            ExprKind::MemberCall { base, name, args } => Some((base, name, args, None)),
            ExprKind::GenericMemberCall {
                base,
                name,
                type_args,
                args,
            } => Some((base, name, args, Some(type_args.as_slice()))),
            _ => None,
        };
        if let ExprKind::MemberCall { base, name, args } = expr.kind()
            && matches!(
                name.as_str(),
                "append"
                    | "insert"
                    | "remove"
                    | "removeLast"
                    | "removeAll"
                    | "sort"
                    | "formUnion"
                    | "formIntersection"
                    | "subtract"
                    | "formSymmetricDifference"
            )
            && let Ok(subject) = self.value(base)
            && (self.sigs.array_element(subject.ty()).is_some()
                || self.sigs.table_parts(subject.ty()).is_some())
        {
            let set = self.sigs.is_set(subject.ty());
            let table = self.sigs.table_parts(subject.ty()).is_some();
            let update = match name.as_str() {
                "append" => Update::Append(args),
                "insert" if set => Update::SetInsert(args),
                "insert" => Update::Insert(args),
                "remove" if set => Update::SetRemove(args),
                "remove" => Update::Remove(args),
                "removeAll" => Update::RemoveAll(args),
                "sort" => Update::Sort(args),
                "formUnion" => Update::Algebra(SetOp::Union, name, args),
                "formIntersection" => Update::Algebra(SetOp::Intersection, name, args),
                "subtract" => Update::Algebra(SetOp::Subtracting, name, args),
                "formSymmetricDifference" => Update::Algebra(SetOp::Symmetric, name, args),
                _ => Update::RemoveLast(args),
            };
            if !(table && !set && !matches!(update, Update::RemoveAll(_))) {
                return self.update(expr.line(), base, &update);
            }
        }
        if let Some((base, name, args, explicit)) = member_call
            && self.enum_target(base).is_none()
        {
            return Ok(TStmt::Eval(self.method_call(
                expr.line(),
                base,
                name,
                args,
                None,
                explicit,
            )?));
        }
        let ExprKind::Call { callee, args } = expr.kind() else {
            return Err(CheckError::at(
                expr.line(),
                "the value of this expression is unused",
            ));
        };
        if callee == PRINT {
            return self.print(expr.line(), args);
        }
        if callee == FATAL_ERROR {
            return Self::fatal_error(expr.line(), args);
        }
        Ok(TStmt::Eval(self.call(expr.line(), callee, args, None)?))
    }

    fn fatal_error(line: u32, args: &[Arg]) -> CResult<TStmt> {
        let usage = "fatalError takes no argument or one unlabeled string literal";
        let text = match args {
            [] => "Fatal error\n".to_string(),
            [arg] if arg.label().is_none() => match arg.value().kind() {
                ExprKind::Str(text) if !text.contains('\0') => format!("Fatal error: {text}\n"),
                ExprKind::Str(_) => {
                    return Err(CheckError::at(
                        line,
                        "a string literal cannot contain a NUL character",
                    ));
                }
                ExprKind::Int(_) | ExprKind::Bool(_) => {
                    return Err(CheckError::at(line, usage));
                }
                _ => {
                    return Err(CheckError::at(
                        line,
                        "the message of fatalError must be a string literal",
                    ));
                }
            },
            _ => return Err(CheckError::at(line, usage)),
        };
        Ok(TStmt::Fatal(text))
    }

    fn string_conversion(&mut self, line: u32, args: &[Arg]) -> CResult<TExpr> {
        let usage = "String takes exactly one unlabeled argument";
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label().is_some() {
            return Err(CheckError::at(line, usage));
        }
        let value = self.value(arg.value())?;
        match value.ty() {
            Ty::Str => Ok(value),
            Ty::Char => Ok(TExpr::StrOp {
                op: StrOp::FromChar,
                args: vec![value],
            }),
            Ty::Int => Ok(TExpr::StrOp {
                op: StrOp::FromInt,
                args: vec![value],
            }),
            Ty::Bool => Ok(TExpr::Select {
                cond: Box::new(value),
                then: Box::new(TExpr::Str("true".to_string())),
                otherwise: Box::new(TExpr::Str("false".to_string())),
                ty: Ty::Str,
            }),
            Ty::Enum(_) => Ok(self.enum_name(value)),
            other => Err(CheckError::at(
                line,
                format!(
                    "cannot convert a value of type {} to String",
                    self.name_of(other)
                ),
            )),
        }
    }

    fn enum_name(&mut self, value: TExpr) -> TExpr {
        let ty = value.ty();
        let names: Vec<String> = self
            .sigs
            .enum_of(ty)
            .map(|sig| sig.cases().iter().map(|c| c.name().to_string()).collect())
            .unwrap_or_default();
        let held = self.temp(ty);
        let mut subject = Some(value);
        let mut conditions: Vec<(TExpr, String)> = Vec::new();
        for (index, name) in names.iter().enumerate().take(names.len().saturating_sub(1)) {
            let lhs = match subject.take() {
                Some(value) => TExpr::Bind {
                    local: held,
                    init: Box::new(value),
                },
                None => TExpr::Local(held, ty),
            };
            conditions.push((
                TExpr::Binary {
                    op: BinOp::Eq,
                    lhs: Box::new(lhs),
                    rhs: Box::new(TExpr::EnumCase { ty, index }),
                },
                name.clone(),
            ));
        }
        let mut result = TExpr::Str(names.last().cloned().unwrap_or_default());
        for (condition, name) in conditions.into_iter().rev() {
            result = TExpr::Select {
                cond: Box::new(condition),
                then: Box::new(TExpr::Str(name)),
                otherwise: Box::new(result),
                ty: Ty::Str,
            };
        }
        result
    }

    fn string_end(&mut self, text: TExpr, first: bool) -> TExpr {
        let optional = self.sigs.optional_of(Ty::Char).unwrap_or(Ty::Int);
        let held = self.temp(Ty::Str);
        let source = TExpr::Local(held, Ty::Str);
        let character = if first {
            TExpr::StrOp {
                op: StrOp::CharAt,
                args: vec![source, TExpr::Int(0)],
            }
        } else {
            TExpr::StrOp {
                op: StrOp::LastChar,
                args: vec![source],
            }
        };
        TExpr::Select {
            cond: Box::new(TExpr::StrOp {
                op: StrOp::IsEmpty,
                args: vec![TExpr::Bind {
                    local: held,
                    init: Box::new(text),
                }],
            }),
            then: Box::new(self.none_of(optional)),
            otherwise: Box::new(TExpr::StructInit {
                ty: optional,
                args: vec![TExpr::Int(SOME_CASE as i64), character],
            }),
            ty: optional,
        }
    }

    fn character_method(
        &mut self,
        line: u32,
        receiver: TExpr,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let op = match name {
            "uppercased" => StrOp::Uppercased,
            "lowercased" => StrOp::Lowercased,
            _ => {
                return Err(CheckError::at(
                    line,
                    format!("Character has no method '{name}'"),
                ));
            }
        };
        if !args.is_empty() {
            return Err(CheckError::at(line, format!("'{name}' takes no arguments")));
        }
        Ok(TExpr::StrOp {
            op,
            args: vec![TExpr::StrOp {
                op: StrOp::FromChar,
                args: vec![receiver],
            }],
        })
    }

    fn string_method(
        &mut self,
        line: u32,
        receiver: TExpr,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let whole = match name {
            "reversed" => Some(StrOp::Reversed),
            "uppercased" => Some(StrOp::Uppercased),
            "lowercased" => Some(StrOp::Lowercased),
            "trimmed" => Some(StrOp::Trimmed),
            _ => None,
        };
        if let Some(op) = whole {
            if !args.is_empty() {
                return Err(CheckError::at(line, format!("'{name}' takes no arguments")));
            }
            return Ok(TExpr::StrOp {
                op,
                args: vec![receiver],
            });
        }
        if name == "replacingOccurrences" {
            return self.string_replacement(line, receiver, args);
        }
        if name == "firstIndex" {
            return self.string_first_index(line, receiver, args);
        }
        if name == "split" {
            return self.string_split(line, receiver, args);
        }
        let counted = match name {
            "prefix" => Some(StrOp::Prefix),
            "suffix" => Some(StrOp::Suffix),
            "dropFirst" => Some(StrOp::DropFirst),
            "dropLast" => Some(StrOp::DropLast),
            _ => None,
        };
        if let Some(op) = counted {
            let usage = format!("'{name}' takes exactly one unlabeled argument");
            let [arg] = args else {
                return Err(CheckError::at(line, usage));
            };
            if arg.label().is_some() {
                return Err(CheckError::at(line, usage));
            }
            self.expected = Some(Ty::Int);
            let value = self.value(arg.value())?;
            self.expect(line, Ty::Int, value.ty())?;
            return Ok(TExpr::StrOp {
                op,
                args: vec![receiver, value],
            });
        }
        let op = match name {
            "hasPrefix" => StrOp::HasPrefix,
            "hasSuffix" => StrOp::HasSuffix,
            "contains" => StrOp::Contains,
            _ => {
                return Err(CheckError::at(
                    line,
                    format!("String has no method '{name}'"),
                ));
            }
        };
        let usage = format!("'{name}' takes exactly one unlabeled argument");
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label().is_some() {
            return Err(CheckError::at(line, usage));
        }
        self.expected = Some(Ty::Str);
        let mut value = self.value(arg.value())?;
        if op == StrOp::Contains && value.ty() == Ty::Char {
            value = TExpr::StrOp {
                op: StrOp::FromChar,
                args: vec![value],
            };
        }
        self.expect(line, Ty::Str, value.ty())?;
        Ok(TExpr::StrOp {
            op,
            args: vec![receiver, value],
        })
    }

    fn string_split(&mut self, line: u32, receiver: TExpr, args: &[Arg]) -> CResult<TExpr> {
        let usage = "'split' takes the label 'separator:'";
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label() != Some("separator") {
            return Err(CheckError::at(line, usage));
        }
        self.expected = Some(Ty::Str);
        let mut separator = self.value(arg.value())?;
        if separator.ty() == Ty::Char {
            separator = text_op(StrOp::FromChar, vec![separator]);
        }
        self.expect(line, Ty::Str, separator.ty())?;
        let parts = self.sigs.array_of(Ty::Str, line)?;
        Ok(TExpr::Call {
            callee: SPLIT.to_string(),
            type_args: vec![Ty::Str],
            args: vec![receiver, separator],
            ret: parts,
        })
    }

    fn array_joined(&mut self, line: u32, receiver: TExpr, args: &[Arg]) -> CResult<TExpr> {
        let separator = match args {
            [] => TExpr::Str(String::new()),
            [arg] if arg.label() == Some("separator") => {
                self.expected = Some(Ty::Str);
                let value = self.value(arg.value())?;
                self.expect(line, Ty::Str, value.ty())?;
                value
            }
            _ => {
                return Err(CheckError::at(
                    line,
                    "'joined' takes no argument or the label 'separator:'",
                ));
            }
        };
        let strings = self.sigs.array_of(Ty::Str, line)?;
        if receiver.ty() != strings {
            return Err(CheckError::at(
                line,
                format!(
                    "'joined' needs an array of String, but found {}",
                    self.name_of(receiver.ty())
                ),
            ));
        }
        Ok(TExpr::Call {
            callee: JOINED.to_string(),
            type_args: vec![Ty::Str],
            args: vec![receiver, separator],
            ret: Ty::Str,
        })
    }

    fn string_subscript(&mut self, text: TExpr, index: &Expr) -> CResult<TExpr> {
        if let ExprKind::Range { lo, hi, inclusive } = index.kind() {
            let low = self.int_value(lo)?;
            let mut high = self.int_value(hi)?;
            if *inclusive {
                high = compare(BinOp::Add, high, TExpr::Int(1));
            }
            return Ok(text_op(StrOp::ScalarSlice, vec![text, low, high]));
        }
        let position = self.int_value(index)?;
        Ok(text_op(StrOp::ScalarAt, vec![text, position]))
    }

    fn string_replacement(&mut self, line: u32, receiver: TExpr, args: &[Arg]) -> CResult<TExpr> {
        let usage = "'replacingOccurrences' takes the labels 'of:' and 'with:'";
        let [target, replacement] = args else {
            return Err(CheckError::at(line, usage));
        };
        if target.label() != Some("of") || replacement.label() != Some("with") {
            return Err(CheckError::at(line, usage));
        }
        let mut values = vec![receiver];
        for arg in [target, replacement] {
            self.expected = Some(Ty::Str);
            let value = self.value(arg.value())?;
            self.expect(line, Ty::Str, value.ty())?;
            values.push(value);
        }
        Ok(TExpr::StrOp {
            op: StrOp::Replace,
            args: values,
        })
    }

    fn string_first_index(&mut self, line: u32, receiver: TExpr, args: &[Arg]) -> CResult<TExpr> {
        let usage = "'firstIndex' takes the label 'of:'";
        let [arg] = args else {
            return Err(CheckError::at(line, usage));
        };
        if arg.label() != Some("of") {
            return Err(CheckError::at(line, usage));
        }
        self.expected = Some(Ty::Char);
        let needle = self.value(arg.value())?;
        self.expect(line, Ty::Char, needle.ty())?;
        let optional = self.sigs.optional_of(Ty::Int)?;
        let text = self.temp(Ty::Str);
        let at = self.temp(Ty::Int);
        let found = TExpr::Bind {
            local: at,
            init: Box::new(TExpr::StrOp {
                op: StrOp::ByteIndex,
                args: vec![
                    TExpr::Bind {
                        local: text,
                        init: Box::new(receiver),
                    },
                    TExpr::StrOp {
                        op: StrOp::FromChar,
                        args: vec![needle],
                    },
                ],
            }),
        };
        Ok(TExpr::Select {
            cond: Box::new(TExpr::Binary {
                op: BinOp::Lt,
                lhs: Box::new(found),
                rhs: Box::new(TExpr::Int(0)),
            }),
            then: Box::new(self.none_of(optional)),
            otherwise: Box::new(TExpr::StructInit {
                ty: optional,
                args: vec![
                    TExpr::Int(SOME_CASE as i64),
                    TExpr::StrOp {
                        op: StrOp::ScalarCount,
                        args: vec![TExpr::Local(text, Ty::Str), TExpr::Local(at, Ty::Int)],
                    },
                ],
            }),
            ty: optional,
        })
    }

    fn print_enum(&mut self, value: TExpr) -> TStmt {
        let ty = value.ty();
        let names: Vec<String> = self
            .sigs
            .enum_of(ty)
            .map(|sig| sig.cases().iter().map(|c| c.name().to_string()).collect())
            .unwrap_or_default();
        let held = self.temp(ty);
        let shown = |name: &str| TStmt::Print(TExpr::Str(name.to_string()));
        let mut chain = vec![names.last().map_or_else(|| shown(""), |last| shown(last))];
        for (index, name) in names.iter().enumerate().rev().skip(1) {
            chain = vec![TStmt::If {
                cond: TExpr::Binary {
                    op: BinOp::Eq,
                    lhs: Box::new(TExpr::Local(held, ty)),
                    rhs: Box::new(TExpr::EnumCase { ty, index }),
                },
                then_body: vec![shown(name)],
                else_body: chain,
            }];
        }
        let mut body = vec![TStmt::Let {
            local: held,
            init: value,
        }];
        body.extend(chain);
        TStmt::If {
            cond: TExpr::Bool(true),
            then_body: body,
            else_body: Vec::new(),
        }
    }

    fn print(&mut self, line: u32, args: &[Arg]) -> CResult<TStmt> {
        let [arg] = args else {
            return Err(CheckError::at(
                line,
                "print takes exactly one unlabeled argument",
            ));
        };
        if arg.label().is_some() {
            return Err(CheckError::at(
                line,
                "print takes exactly one unlabeled argument",
            ));
        }
        let value = self.value(arg.value())?;
        if matches!(value.ty(), Ty::Enum(_)) {
            return Ok(self.print_enum(value));
        }
        if value.ty() == Ty::Char {
            return Ok(TStmt::Print(TExpr::StrOp {
                op: StrOp::FromChar,
                args: vec![value],
            }));
        }
        if !matches!(value.ty(), Ty::Int | Ty::Bool | Ty::Str) {
            return Err(CheckError::at(
                line,
                format!("cannot print a value of type {}", self.name_of(value.ty())),
            ));
        }
        Ok(TStmt::Print(value))
    }

    fn condition(&mut self, expr: &Expr) -> CResult<TExpr> {
        let cond = self.value(expr)?;
        self.expect(expr.line(), Ty::Bool, cond.ty())?;
        Ok(cond)
    }

    fn int_value(&mut self, expr: &Expr) -> CResult<TExpr> {
        let value = self.value(expr)?;
        self.expect(expr.line(), Ty::Int, value.ty())?;
        Ok(value)
    }

    fn value(&mut self, expr: &Expr) -> CResult<TExpr> {
        let checked = self.expr(expr)?;
        if checked.ty() == Ty::Void {
            return Err(CheckError::at(
                expr.line(),
                "this call does not produce a value",
            ));
        }
        Ok(checked)
    }

    fn expr(&mut self, expr: &Expr) -> CResult<TExpr> {
        let line = expr.line();
        let expected = self.expected.take();
        match expr.kind() {
            ExprKind::Nil => match expected {
                Some(ty) if self.sigs.unwrapped(ty).is_some() => Ok(self.none_of(ty)),
                Some(ty) => Err(CheckError::at(
                    line,
                    format!("nil cannot be a value of type {}", self.name_of(ty)),
                )),
                None => Err(CheckError::at(
                    line,
                    "nil needs a type from its context, such as 'let x: Int? = nil'",
                )),
            },
            ExprKind::Unwrap(inner) => {
                let base = self.value(inner)?;
                let ty = self.sigs.unwrapped(base.ty()).ok_or_else(|| {
                    CheckError::at(
                        line,
                        format!(
                            "'!' needs an optional, but found {}",
                            self.name_of(base.ty())
                        ),
                    )
                })?;
                Ok(TExpr::Unwrap {
                    base: Box::new(base),
                    ty,
                })
            }
            ExprKind::Binary { op, lhs, rhs }
                if matches!(op, BinOp::Eq | BinOp::Ne)
                    && (matches!(lhs.kind(), ExprKind::Nil)
                        || matches!(rhs.kind(), ExprKind::Nil)) =>
            {
                self.nil_comparison(line, *op, lhs, rhs)
            }
            ExprKind::Int(n) => Ok(TExpr::Int(*n)),
            ExprKind::Bool(b) => Ok(TExpr::Bool(*b)),
            ExprKind::Str(text)
                if expected
                    .is_some_and(|t| t == Ty::Char || self.sigs.unwrapped(t) == Some(Ty::Char)) =>
            {
                Self::character_literal(line, text)
            }
            ExprKind::Str(text) if text.contains('\0') => Err(CheckError::at(
                line,
                "a string literal cannot contain a NUL character",
            )),
            ExprKind::Str(text) => Ok(TExpr::Str(text.clone())),
            ExprKind::Ident(name) => match self.lookup(name, line) {
                Ok(binding) => Ok(TExpr::Local(binding.local, binding.ty)),
                Err(error) => match self.implicit_member(line, name) {
                    Some((receiver, index, field)) => Ok(TExpr::Field {
                        base: Box::new(TExpr::Local(receiver.local, receiver.ty)),
                        index,
                        ty: field.ty(),
                    }),
                    None => Err(error),
                },
            },
            ExprKind::Unary { op, expr: operand } => {
                let operand = self.value(operand)?;
                let wanted = match op {
                    UnaryOp::Neg => Ty::Int,
                    UnaryOp::Not => Ty::Bool,
                };
                self.expect(line, wanted, operand.ty())?;
                Ok(TExpr::Unary {
                    op: *op,
                    expr: Box::new(operand),
                })
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let implicit = |e: &Expr| {
                    matches!(e.kind(), ExprKind::ImplicitMember { .. } | ExprKind::Str(_))
                };
                let literal = |e: &Expr| {
                    matches!(
                        e.kind(),
                        ExprKind::ArrayLiteral(_) | ExprKind::DictLiteral(_)
                    )
                };
                let comparing = matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
                );
                let (lhs, rhs) = if comparing && implicit(lhs) && !implicit(rhs) {
                    let rhs = self.value(rhs)?;
                    self.expected = Some(rhs.ty());
                    (self.value(lhs)?, rhs)
                } else {
                    let lhs = self.value(lhs)?;
                    if (comparing && implicit(rhs))
                        || ((comparing || *op == BinOp::Add) && literal(rhs))
                    {
                        self.expected = Some(lhs.ty());
                    }
                    (lhs, self.value(rhs)?)
                };
                if matches!(op, BinOp::Eq | BinOp::Ne)
                    && (self.sigs.unwrapped(lhs.ty()).is_some()
                        || self.sigs.unwrapped(rhs.ty()).is_some())
                {
                    let target = if self.sigs.unwrapped(lhs.ty()).is_some() {
                        lhs.ty()
                    } else {
                        rhs.ty()
                    };
                    let lhs = self.coerce(line, target, lhs)?;
                    let rhs = self.coerce(line, target, rhs)?;
                    return self.optional_equality(line, *op, lhs, rhs);
                }
                if let (Ty::Param(index), true) = (
                    lhs.ty(),
                    matches!(
                        op,
                        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
                    ),
                ) {
                    return self.parameter_operator(line, *op, index, lhs, rhs);
                }
                if *op == BinOp::Add
                    && let Some(elem) = self.sigs.array_element(lhs.ty())
                {
                    self.expect(line, lhs.ty(), rhs.ty())?;
                    let ty = lhs.ty();
                    return Ok(TExpr::ArrayOp {
                        op: ArrayOp::Concat,
                        elem,
                        args: vec![lhs, rhs],
                        ty,
                    });
                }
                if *op == BinOp::Add && lhs.ty() == Ty::Str {
                    self.expect(line, Ty::Str, rhs.ty())?;
                    return Ok(TExpr::StrOp {
                        op: StrOp::Concat,
                        args: vec![lhs, rhs],
                    });
                }
                self.operands(line, *op, lhs.ty(), rhs.ty())?;
                if matches!(op, BinOp::Eq | BinOp::Ne) && lhs.ty().is_struct() {
                    return self.struct_equality(line, *op, lhs, rhs);
                }
                if matches!(op, BinOp::Eq | BinOp::Ne)
                    && (self.sigs.array_element(lhs.ty()).is_some()
                        || self.sigs.table_parts(lhs.ty()).is_some())
                {
                    return self.array_equality(line, *op, lhs, rhs);
                }
                Ok(TExpr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                })
            }
            ExprKind::Call { callee, .. } if callee == PRINT => {
                Err(CheckError::at(line, "print does not produce a value"))
            }
            ExprKind::Call { callee, .. } if callee == FATAL_ERROR => Err(CheckError::at(
                line,
                "fatalError does not produce a value: write it as a statement",
            )),
            ExprKind::Call { callee, args } => self.call(line, callee, args, expected),
            ExprKind::GenericCall {
                callee,
                type_args,
                args,
            } => self.generic_call(line, callee, type_args, args, expected),
            ExprKind::GenericName { name, .. } => Err(CheckError::at(
                line,
                format!("'{name}' with type arguments is not a value"),
            )),
            ExprKind::ImplicitMember { name, args } => {
                self.implicit_member_value(line, name, args.as_deref(), expected)
            }
            ExprKind::Member { base, name } if self.enum_target(base).is_some() => {
                self.enum_case(line, base, name, expected)
            }
            ExprKind::MemberCall { base, name, args } if self.enum_target(base).is_some() => {
                self.enum_construct(line, base, name, args, expected)
            }
            ExprKind::MemberCall { base, name, args } => {
                self.method_call(line, base, name, args, expected, None)
            }
            ExprKind::GenericMemberCall {
                base,
                name,
                type_args,
                args,
            } => self.method_call(line, base, name, args, expected, Some(type_args)),
            ExprKind::Member { base, name } => {
                let base = self.value(base)?;
                self.field_of(line, base, name)
            }
            ExprKind::Coalesce { lhs, rhs } => self.coalesce(line, lhs, rhs),
            ExprKind::Cast {
                expr,
                ty,
                forced,
                checked,
            } => self.cast(line, expr, ty, *forced, *checked),
            ExprKind::OptionalChain { subject, link } => {
                let (held, value, payload) = self.optional_chain(line, subject)?;
                let condition = Self::is_some(held, value);
                self.chain_values.push(payload);
                let access = self.value(link);
                self.chain_values.pop();
                self.chain_result(condition, access?)
            }
            ExprKind::ChainValue => {
                self.chain_values.last().cloned().ok_or_else(|| {
                    CheckError::at(line, "internal error: no optional chain is open")
                })
            }
            ExprKind::Range { .. } => Err(CheckError::at(
                line,
                "a range can only be used in a for loop",
            )),
            ExprKind::ArrayLiteral(items) => self.array_literal(line, items, expected),
            ExprKind::DictLiteral(pairs) => self.dict_literal(line, pairs, expected),
            ExprKind::Apply { func, args } => {
                let target = self.value(func)?;
                self.apply_value(line, "function", target, None, args)
            }
            ExprKind::Closure(closure) => {
                let target = expected.map(|ty| self.sigs.unwrapped(ty).unwrap_or(ty));
                let parts = target.and_then(|ty| self.sigs.func_parts(ty));
                let (params, ret) = match parts {
                    Some((params, ret)) => (Some(params), Some(ret)),
                    None => (None, None),
                };
                self.closure_value(line, closure, params, ret)
            }
            ExprKind::TypeCall { ty, args } => {
                self.collection_init(line, ty.name(), Some(ty.args()), args, expected)
            }
            ExprKind::IndexDefault {
                base,
                index,
                default,
            } => {
                let table = self.value(base)?;
                match self.sigs.table_parts(table.ty()) {
                    Some((key, value)) if !self.sigs.is_set(table.ty()) => {
                        self.lookup_entry(line, table, key, value, index, Some(default))
                    }
                    _ => Err(CheckError::at(
                        line,
                        format!(
                            "{} has no subscript with 'default:'",
                            self.name_of(table.ty())
                        ),
                    )),
                }
            }
            ExprKind::Index { base, index } => self.subscript(line, base, index),
        }
    }

    fn method_call(
        &mut self,
        line: u32,
        base: &Expr,
        name: &str,
        args: &[Arg],
        expected: Option<Ty>,
        explicit: Option<&[TypeRef]>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let (receiver, lookup_ty, dispatching) = if self.is_super(base) {
            let (receiver, parent) = self.super_receiver(line)?;
            (receiver, parent, false)
        } else {
            let receiver = self.value(base)?;
            let ty = receiver.ty();
            if let Ok((_, field)) = self.member_of(line, ty, name)
                && sigs.func_parts(field.ty()).is_some()
            {
                let function = self.field_of(line, receiver, name)?;
                return self.apply_value(line, name, function, None, args);
            }
            if ty == Ty::Char {
                return self.character_method(line, receiver, name, args);
            }
            if ty == Ty::Str {
                if explicit.is_some() {
                    return Err(CheckError::at(
                        line,
                        "a method of String takes no explicit type arguments",
                    ));
                }
                return self.string_method(line, receiver, name, args);
            }
            if let Some(elem) = sigs.array_element(ty) {
                return self.array_method(line, base, receiver, elem, name, args);
            }
            if let Some(parts) = sigs.table_parts(ty) {
                return self.table_method(line, base, receiver, parts, name, args);
            }
            let requirement_call =
                matches!(ty, Ty::Param(_)) || sigs.existential_protocol(ty).is_some();
            if requirement_call && explicit.is_some() {
                return Err(CheckError::at(
                    line,
                    "a protocol requirement takes no explicit type arguments",
                ));
            }
            if let Ty::Param(index) = ty {
                return self.witness_call(line, base, receiver, index, name, args);
            }
            if let Some(protocol) = sigs.existential_protocol(ty) {
                return self.dispatch_call(line, receiver, &protocol, name, args);
            }
            (receiver, ty, true)
        };
        let owner = sigs.owner_name(lookup_ty).ok_or_else(|| {
            CheckError::at(
                line,
                format!("a value of type {} has no methods", self.name_of(lookup_ty)),
            )
        })?;
        let qualified = sigs
            .resolve_method(lookup_ty, name)
            .ok_or_else(|| CheckError::at(line, format!("'{owner}' has no method '{name}'")))?;
        let sig = sigs
            .func(&qualified)
            .ok_or_else(|| CheckError::at(line, format!("'{owner}' has no method '{name}'")))?;
        if sig.is_mutating() && !self.is_mutable_variable(base) {
            return Err(CheckError::at(
                line,
                format!(
                    "cannot call the mutating method '{name}' on a constant or a temporary value"
                ),
            ));
        }
        let cell = sigs
            .cell_enum(sig.params()[0].ty())
            .map(|_| sig.params()[0].ty());
        let receiver = match cell {
            Some(cell_ty) => self.enum_cell_receiver(line, base, receiver, cell_ty)?,
            None => receiver,
        };
        let params = &sig.params()[1..];
        if args.len() != params.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{qualified}' takes {} argument(s), but {} were given",
                    params.len(),
                    args.len()
                ),
            ));
        }
        let checked = self.check_values(line, sig, params, args, vec![receiver])?;
        let explicit = match explicit {
            Some(given) => {
                let mut all = sigs.type_arguments(lookup_ty);
                let own = sig.generics().len().saturating_sub(all.len());
                all.extend(self.explicit_args(line, &qualified, own, given)?);
                Some(all)
            }
            None => None,
        };
        let call = self.finish_call(line, &qualified, sig, checked, expected, explicit)?;
        let slot =
            if dispatching && lookup_ty.is_class() && sigs.is_overridden_below(lookup_ty, name) {
                sigs.vtable_slot(lookup_ty, name)
            } else {
                None
            };
        Ok(match (call, slot) {
            (TExpr::Call { args, ret, .. }, Some(slot)) => TExpr::VCall { slot, args, ret },
            (other, _) => other,
        })
    }

    fn is_super_init(&self, expr: &Expr) -> bool {
        matches!(
            expr.kind(),
            ExprKind::MemberCall { base, name, .. } if name == "init" && self.is_super(base)
        )
    }

    fn super_init(&mut self, line: u32, expr: &Expr, out: &mut Vec<TStmt>) -> CResult<()> {
        let ExprKind::MemberCall { args, .. } = expr.kind() else {
            return Err(CheckError::at(
                line,
                "internal error: not a super.init call",
            ));
        };
        let Some(target) = self.init_self else {
            return Err(CheckError::at(
                line,
                "'super.init' can only be called from an initializer",
            ));
        };
        let (_, parent) = self.super_receiver(line)?;
        if std::mem::replace(&mut self.super_initialized, true) {
            return Err(CheckError::at(line, "'super.init' is called twice"));
        }
        let sigs = self.sigs;
        let parent_name = sigs.owner_name(parent).unwrap_or_default();
        let built = self.call(line, &parent_name, args, Some(parent))?;
        self.expect(line, parent, built.ty())?;
        let Ty::Class(parent_id) = parent else {
            return Err(CheckError::at(
                line,
                "internal error: a superclass is a class",
            ));
        };
        self.adopt_members(line, target, parent_id, built, out)
    }

    fn adopt_members(
        &mut self,
        line: u32,
        target: LocalId,
        source_id: StructId,
        built: TExpr,
        out: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let source = built.ty();
        let layout = self
            .sigs
            .struct_sig(source_id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown class layout"))?;
        let temporary = LocalId::new(self.locals.len());
        self.locals.push(source);
        out.push(TStmt::Let {
            local: temporary,
            init: built,
        });
        for (index, field) in layout.members() {
            out.push(TStmt::AssignField {
                local: target,
                index,
                value: TExpr::Moved(Box::new(TExpr::Field {
                    base: Box::new(TExpr::Local(temporary, source)),
                    index,
                    ty: field.ty(),
                })),
            });
        }
        out.push(TStmt::Free(TExpr::Local(temporary, source)));
        Ok(())
    }

    fn is_self_init(&self, expr: &Expr) -> bool {
        matches!(
            expr.kind(),
            ExprKind::MemberCall { base, name, .. }
                if name == "init"
                    && matches!(base.kind(), ExprKind::Ident(n) if n == "self")
                    && self.lookup("self", base.line()).is_ok()
        )
    }

    fn self_init(&mut self, line: u32, expr: &Expr, out: &mut Vec<TStmt>) -> CResult<()> {
        let ExprKind::MemberCall { args, .. } = expr.kind() else {
            return Err(CheckError::at(line, "internal error: not a self.init call"));
        };
        let Some(target) = self.init_self else {
            return Err(CheckError::at(
                line,
                "'self.init' can only be called from an initializer",
            ));
        };
        let ty = self.locals[target.index()];
        let owner = self
            .sigs
            .owner_name(ty)
            .ok_or_else(|| CheckError::at(line, "internal error: an initializer needs a type"))?;
        let built = self.call(line, &owner, args, None)?;
        if let TExpr::Call { callee, .. } = &built
            && *callee == self.name
        {
            return Err(CheckError::at(
                line,
                "an initializer cannot delegate to itself",
            ));
        }
        self.expect(line, ty, built.ty())?;
        match ty {
            Ty::Class(id) => self.adopt_members(line, target, id, built, out),
            _ => {
                out.push(TStmt::Assign {
                    local: target,
                    value: built,
                });
                Ok(())
            }
        }
    }

    fn is_super(&self, base: &Expr) -> bool {
        matches!(base.kind(), ExprKind::Ident(name) if name == SUPER && self.lookup(name, base.line()).is_err())
    }

    fn super_receiver(&self, line: u32) -> CResult<(TExpr, Ty)> {
        let binding = self.lookup("self", line).map_err(|_| {
            CheckError::at(line, "'super' can only be used inside a method of a class")
        })?;
        let parent = self.sigs.superclass(binding.ty).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'super' needs a superclass, but {} has none",
                    self.name_of(binding.ty)
                ),
            )
        })?;
        Ok((TExpr::Local(binding.local, binding.ty), parent))
    }

    fn enum_cell_receiver(
        &mut self,
        line: u32,
        base: &Expr,
        receiver: TExpr,
        cell_ty: Ty,
    ) -> CResult<TExpr> {
        let ExprKind::Ident(name) = base.kind() else {
            return Err(CheckError::at(
                line,
                "internal error: a cell needs a variable",
            ));
        };
        if !self.cell_call.allowed || self.cell_call.pending.is_some() {
            return Err(CheckError::at(
                line,
                "a mutating method of an enum must be called as a statement of its own, once per statement",
            ));
        }
        let binding = self.lookup(name, line)?;
        let cell = LocalId::new(self.locals.len());
        self.locals.push(cell_ty);
        let before = TStmt::Let {
            local: cell,
            init: TExpr::StructInit {
                ty: cell_ty,
                args: vec![receiver],
            },
        };
        let after = TStmt::Assign {
            local: binding.local,
            value: TExpr::Field {
                base: Box::new(TExpr::Local(cell, cell_ty)),
                index: 0,
                ty: binding.ty,
            },
        };
        self.cell_call.pending = Some((before, after));
        Ok(TExpr::Local(cell, cell_ty))
    }

    fn is_mutable_variable(&self, base: &Expr) -> bool {
        match base.kind() {
            ExprKind::Ident(name) => self
                .lookup(name, base.line())
                .is_ok_and(|binding| binding.mutable),
            _ => false,
        }
    }

    fn enum_target<'e>(&self, base: &'e Expr) -> Option<EnumTarget<'e>> {
        match base.kind() {
            ExprKind::Ident(name) if self.lookup(name, base.line()).is_err() => {
                self.sigs.enum_def(name).map(|def| EnumTarget {
                    def,
                    explicit: None,
                    name: name.clone(),
                })
            }
            ExprKind::GenericName { name, type_args } => {
                self.sigs.enum_def(name).map(|def| EnumTarget {
                    def,
                    explicit: Some(type_args.as_slice()),
                    name: name.clone(),
                })
            }
            _ => None,
        }
    }

    fn explicit_args(
        &self,
        line: u32,
        name: &str,
        expected: usize,
        given: &[TypeRef],
    ) -> CResult<Vec<Ty>> {
        if given.len() != expected {
            return Err(CheckError::at(
                line,
                format!(
                    "'{name}' expects {expected} type argument(s), but {} were given",
                    given.len()
                ),
            ));
        }
        given
            .iter()
            .map(|tr| self.sigs.resolve_type(tr, &self.scope, line))
            .collect()
    }

    fn infer(
        &self,
        line: u32,
        owner: &str,
        generics: &[String],
        patterns: &[Ty],
        values: &[TExpr],
        context: Option<(Ty, Ty)>,
    ) -> CResult<Vec<Ty>> {
        let mut bound = vec![None; generics.len()];
        if let Some((result, expected)) = context {
            let mut attempt = bound.clone();
            if self.sigs.unify(result, expected, &mut attempt) {
                bound = attempt;
            }
        }
        let callee_scope = scope_of(generics);
        for (pattern, value) in patterns.iter().zip(values) {
            if !self.sigs.has_param(*pattern) {
                continue;
            }
            let mut candidate = *pattern;
            let mut unified = false;
            loop {
                let mut attempt = bound.clone();
                if self.sigs.unify(candidate, value.ty(), &mut attempt) {
                    bound = attempt;
                    unified = true;
                    break;
                }
                match self.sigs.unwrapped(candidate) {
                    Some(inner) => candidate = inner,
                    None => break,
                }
            }
            if !unified {
                let wanted = match pattern {
                    Ty::Param(index) => bound.get(*index).copied().flatten().unwrap_or(*pattern),
                    other => *other,
                };
                let shown = if wanted == *pattern {
                    self.sigs.display(wanted, &callee_scope)
                } else {
                    self.name_of(wanted)
                };
                return Err(CheckError::at(
                    line,
                    format!("expected {shown}, but found {}", self.name_of(value.ty())),
                ));
            }
        }
        bound
            .into_iter()
            .zip(generics)
            .map(|(ty, name)| {
                ty.ok_or_else(|| {
                    CheckError::at(
                        line,
                        format!("cannot infer the type parameter '{name}' of '{owner}'"),
                    )
                })
            })
            .collect()
    }

    fn implicit_member_value(
        &mut self,
        line: u32,
        name: &str,
        args: Option<&[Arg]>,
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let Some(context) = expected else {
            return Err(CheckError::at(
                line,
                format!("'.{name}' needs a type from its context"),
            ));
        };
        let inner = sigs.unwrapped(context);
        let enum_ty = inner.unwrap_or(context);
        let own_case = matches!(name, "none" | "some");
        let wrapped_case = sigs
            .enum_of(enum_ty)
            .filter(|sig| inner.is_none() || (!own_case && sig.case_index(name).is_some()));
        let Some(sig) = wrapped_case else {
            return match (inner, name, args) {
                (Some(_), "none", None) => Ok(self.none_of(context)),
                (Some(payload), "some", Some([arg])) if arg.label().is_none() => {
                    self.expected = Some(payload);
                    let value = self.value(arg.value())?;
                    self.coerce(line, context, value)
                }
                _ => Err(CheckError::at(
                    line,
                    format!(
                        "'.{name}' needs an enum type from its context, but found {}",
                        self.name_of(context)
                    ),
                )),
            };
        };
        let target = EnumTarget {
            def: sigs
                .enum_def(sig.base_name())
                .ok_or_else(|| CheckError::at(line, "internal error: an enum has no definition"))?,
            explicit: None,
            name: sig.base_name().to_string(),
        };
        let built = match args {
            Some(args) => self.enum_construct_of(line, target, name, args, Some(enum_ty))?,
            None => self.enum_case_of(line, target, name, Some(enum_ty))?,
        };
        self.coerce(line, context, built)
    }

    fn enum_case(
        &self,
        line: u32,
        base: &Expr,
        name: &str,
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let Some(target) = self.enum_target(base) else {
            return Err(CheckError::at(line, "internal error: not an enum name"));
        };
        self.enum_case_of(line, target, name, expected)
    }

    fn enum_case_of(
        &self,
        line: u32,
        target: EnumTarget<'_>,
        name: &str,
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let EnumTarget {
            def,
            explicit,
            name: base_name,
        } = target;
        let sigs = self.sigs;
        let generics = sigs.enum_generics(def);
        let args = match (generics.is_empty(), explicit) {
            (true, None) => Vec::new(),
            (_, Some(given)) => self.explicit_args(line, &base_name, generics.len(), given)?,
            (false, None) => {
                let identity: Vec<Ty> = (0..generics.len()).map(Ty::Param).collect();
                let pattern = sigs.instantiate_enum(def, identity, &scope_of(generics), line)?;
                let context = expected.map(|e| (sigs.enum_ty(pattern), e));
                self.infer(line, &base_name, generics, &[], &[], context)
                    .map_err(|_| {
                        CheckError::at(
                            line,
                            format!(
                                "cannot infer the type parameters of '{base_name}'; write '{base_name}<...>.{name}'"
                            ),
                        )
                    })?
            }
        };
        let id = sigs.instantiate_enum(def, args, &self.scope, line)?;
        let sig = sigs
            .enum_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown enum"))?;
        let index = sig.case_index(name).ok_or_else(|| {
            CheckError::at(line, format!("'{}' has no case '{name}'", sig.name()))
        })?;
        if !sig.cases()[index].payload().is_empty() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}.{name}' carries values and must be called with them",
                    sig.name()
                ),
            ));
        }
        let ty = sigs.enum_ty(id);
        if sig.has_payload() {
            Ok(TExpr::StructInit {
                ty,
                args: self.pad_payload(ty, vec![TExpr::Int(index as i64)]),
            })
        } else {
            Ok(TExpr::EnumCase { ty, index })
        }
    }

    fn enum_construct(
        &mut self,
        line: u32,
        base: &Expr,
        name: &str,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let Some(target) = self.enum_target(base) else {
            return Err(CheckError::at(line, "internal error: not an enum name"));
        };
        self.enum_construct_of(line, target, name, args, expected)
    }

    fn enum_construct_of(
        &mut self,
        line: u32,
        target: EnumTarget<'_>,
        name: &str,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let EnumTarget {
            def,
            explicit,
            name: base_name,
        } = target;
        let sigs = self.sigs;
        let generics = sigs.enum_generics(def);
        let inferring = !generics.is_empty() && explicit.is_none();
        let pattern_id = if inferring {
            let identity: Vec<Ty> = (0..generics.len()).map(Ty::Param).collect();
            sigs.instantiate_enum(def, identity, &scope_of(generics), line)?
        } else {
            let given = explicit.unwrap_or(&[]);
            let type_args = self.explicit_args(line, &base_name, generics.len(), given)?;
            sigs.instantiate_enum(def, type_args, &self.scope, line)?
        };
        let pattern_sig = sigs
            .enum_sig(pattern_id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown enum"))?;
        let index = pattern_sig
            .case_index(name)
            .ok_or_else(|| CheckError::at(line, format!("'{base_name}' has no case '{name}'")))?;
        let payload = pattern_sig.cases()[index].payload().to_vec();
        if args.len() != payload.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}.{name}' takes {} value(s), but {} were given",
                    pattern_sig.name(),
                    payload.len(),
                    args.len()
                ),
            ));
        }
        let labels = pattern_sig.cases()[index].labels().to_vec();
        let mut values = Vec::with_capacity(args.len());
        for ((arg, wanted), label) in args.iter().zip(&payload).zip(&labels) {
            self.expect_label(line, arg, label.as_deref())?;
            if !inferring {
                self.expected = Some(*wanted);
            }
            values.push(self.value(arg.value())?);
        }
        let id = if inferring {
            let context = expected.map(|e| (sigs.enum_ty(pattern_id), e));
            let type_args = self.infer(line, &base_name, generics, &payload, &values, context)?;
            sigs.instantiate_enum(def, type_args, &self.scope, line)?
        } else {
            let mut coerced = Vec::with_capacity(values.len());
            for (value, expected) in values.into_iter().zip(&payload) {
                coerced.push(self.coerce(line, *expected, value)?);
            }
            values = coerced;
            pattern_id
        };
        let ty = sigs.enum_ty(id);
        let placed = sigs
            .enum_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown enum"))?
            .cases()[index]
            .slots()
            .to_vec();
        let mut args = self.pad_payload(ty, vec![TExpr::Int(index as i64)]);
        for (value, slot) in values.into_iter().zip(placed) {
            args[slot] = value;
        }
        Ok(TExpr::StructInit { ty, args })
    }

    fn nil_comparison(&mut self, line: u32, op: BinOp, lhs: &Expr, rhs: &Expr) -> CResult<TExpr> {
        let subject = if matches!(rhs.kind(), ExprKind::Nil) {
            lhs
        } else {
            rhs
        };
        if matches!(subject.kind(), ExprKind::Nil) {
            return Err(CheckError::at(line, "nil cannot be compared with nil"));
        }
        let value = self.value(subject)?;
        if self.sigs.unwrapped(value.ty()).is_none() {
            return Err(CheckError::at(
                line,
                format!(
                    "a value of type {} is not an optional, so it cannot be compared with nil",
                    self.name_of(value.ty())
                ),
            ));
        }
        Ok(TExpr::Binary {
            op,
            lhs: Box::new(TExpr::Field {
                base: Box::new(value),
                index: 0,
                ty: Ty::Int,
            }),
            rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
        })
    }

    fn dispatch_call(
        &mut self,
        line: u32,
        receiver: TExpr,
        protocol: &str,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let requirement = sigs
            .protocol(protocol)
            .and_then(|p| p.requirement(name))
            .ok_or_else(|| {
                CheckError::at(
                    line,
                    format!("'{name}' is not a requirement of the protocol '{protocol}'"),
                )
            })?;
        if requirement.is_mutating() {
            return Err(CheckError::at(
                line,
                format!(
                    "the mutating requirement '{name}' cannot be called through a value of the protocol '{protocol}'"
                ),
            ));
        }
        let params = requirement.params();
        if args.len() != params.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{name}' takes {} argument(s), but {} were given",
                    params.len(),
                    args.len()
                ),
            ));
        }
        let mut checked = vec![receiver];
        for (arg, param) in args.iter().zip(params) {
            self.expect_label(line, arg, param.label())?;
            self.expected = Some(param.ty());
            let value = self.value(arg.value())?;
            checked.push(self.coerce(line, param.ty(), value)?);
        }
        Ok(TExpr::Dispatch {
            protocol: protocol.to_string(),
            method: name.to_string(),
            args: checked,
            ret: requirement.ret(),
        })
    }

    fn witness_call(
        &mut self,
        line: u32,
        base: &Expr,
        receiver: TExpr,
        index: usize,
        name: &str,
        args: &[Arg],
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let ty = Ty::Param(index);
        let bound = self.bounds.get(index).cloned().unwrap_or_default();
        let (protocol, requirement) = sigs.find_requirement(&bound, name).ok_or_else(|| {
            CheckError::at(
                line,
                format!(
                    "'{name}' is not a requirement of the constraints of {}",
                    self.name_of(ty)
                ),
            )
        })?;
        if requirement.is_mutating() && !self.is_mutable_variable(base) {
            return Err(CheckError::at(
                line,
                format!(
                    "cannot call the mutating method '{name}' on a constant or a temporary value"
                ),
            ));
        }
        let params = requirement.params();
        if args.len() != params.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{name}' takes {} argument(s), but {} were given",
                    params.len(),
                    args.len()
                ),
            ));
        }
        let mut checked = vec![receiver];
        for (arg, param) in args.iter().zip(params) {
            self.expect_label(line, arg, param.label())?;
            let wanted = sigs.subst(param.ty(), &[ty], &self.scope, line)?;
            if !sigs.has_param(wanted) {
                self.expected = Some(wanted);
            }
            let value = self.value(arg.value())?;
            checked.push(self.coerce(line, wanted, value)?);
        }
        let ret = sigs.subst(requirement.ret(), &[ty], &self.scope, line)?;
        Ok(TExpr::Witness {
            protocol: protocol.name().to_string(),
            method: name.to_string(),
            self_ty: ty,
            args: checked,
            ret,
        })
    }

    fn parameter_operator(
        &mut self,
        line: u32,
        op: BinOp,
        index: usize,
        lhs: TExpr,
        rhs: TExpr,
    ) -> CResult<TExpr> {
        let ty = Ty::Param(index);
        let bound = self.bounds.get(index).cloned().unwrap_or_default();
        let (protocol, method) = match op {
            BinOp::Eq | BinOp::Ne => (EQUATABLE, "=="),
            BinOp::Lt => (COMPARABLE, "<"),
            BinOp::Gt => (COMPARABLE, ">"),
            BinOp::Le => (COMPARABLE, "<="),
            BinOp::Ge => (COMPARABLE, ">="),
            _ => {
                return Err(CheckError::at(
                    line,
                    format!("the operator is not supported for {}", self.name_of(ty)),
                ));
            }
        };
        if !bound_includes(&bound, protocol) {
            return Err(CheckError::at(
                line,
                format!(
                    "{} needs the constraint '{protocol}' to be used with this operator",
                    self.name_of(ty)
                ),
            ));
        }
        self.expect(line, ty, rhs.ty())?;
        let witness = TExpr::Witness {
            protocol: protocol.to_string(),
            method: method.to_string(),
            self_ty: ty,
            args: vec![lhs, rhs],
            ret: Ty::Bool,
        };
        Ok(if op == BinOp::Ne {
            TExpr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(witness),
            }
        } else {
            witness
        })
    }

    fn struct_equality(&mut self, line: u32, op: BinOp, lhs: TExpr, rhs: TExpr) -> CResult<TExpr> {
        let ty = lhs.ty();
        let leaves = self.sigs.equality_leaves(line, ty)?;
        let left = self.temp(ty);
        let right = self.temp(ty);
        let mut operands = Some((lhs, rhs));
        let mut all: Option<TExpr> = None;
        for path in &leaves {
            let (left_root, right_root) = match operands.take() {
                Some((lhs, rhs)) => (
                    TExpr::Bind {
                        local: left,
                        init: Box::new(lhs),
                    },
                    TExpr::Bind {
                        local: right,
                        init: Box::new(rhs),
                    },
                ),
                None => (TExpr::Local(left, ty), TExpr::Local(right, ty)),
            };
            let reach = |root: TExpr| {
                path.iter().fold(root, |base, (index, ty)| TExpr::Field {
                    base: Box::new(base),
                    index: *index,
                    ty: *ty,
                })
            };
            let leaf = path.last().map_or(ty, |(_, ty)| *ty);
            let same = self.equality_of(line, leaf, reach(left_root), reach(right_root))?;
            all = Some(match all {
                None => same,
                Some(before) => TExpr::Binary {
                    op: BinOp::And,
                    lhs: Box::new(before),
                    rhs: Box::new(same),
                },
            });
        }
        let all = all.unwrap_or(TExpr::Bool(true));
        Ok(if op == BinOp::Ne {
            TExpr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(all),
            }
        } else {
            all
        })
    }

    fn equality_of(&mut self, line: u32, ty: Ty, lhs: TExpr, rhs: TExpr) -> CResult<TExpr> {
        match ty {
            Ty::Int | Ty::Bool | Ty::Str | Ty::Char | Ty::Enum(_) => Ok(TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            }),
            Ty::Param(index) => self.parameter_operator(line, BinOp::Eq, index, lhs, rhs),
            _ if self.sigs.array_element(ty).is_some() || self.sigs.table_parts(ty).is_some() => {
                self.array_equality(line, BinOp::Eq, lhs, rhs)
            }
            _ if self.sigs.unwrapped(ty).is_some() => {
                self.optional_equality(line, BinOp::Eq, lhs, rhs)
            }
            Ty::Struct(_) if self.sigs.enum_of(ty).is_none() => {
                self.struct_equality(line, BinOp::Eq, lhs, rhs)
            }
            other => Err(CheckError::at(
                line,
                format!("equality is not supported for {}", self.name_of(other)),
            )),
        }
    }

    fn array_equality(&mut self, line: u32, op: BinOp, lhs: TExpr, rhs: TExpr) -> CResult<TExpr> {
        let ty = lhs.ty();
        if let Some(elem) = self.sigs.array_element(ty) {
            self.equality_of(line, elem, TExpr::Zero(elem), TExpr::Zero(elem))?;
        }
        if let Some((_, value)) = self.sigs.table_parts(ty)
            && value != Ty::Void
        {
            self.equality_of(line, value, TExpr::Zero(value), TExpr::Zero(value))?;
        }
        let equal = TExpr::Call {
            callee: EQUAL.to_string(),
            type_args: vec![ty],
            args: vec![lhs, rhs],
            ret: Ty::Bool,
        };
        Ok(if op == BinOp::Ne {
            TExpr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(equal),
            }
        } else {
            equal
        })
    }

    fn optional_equality(
        &mut self,
        line: u32,
        op: BinOp,
        lhs: TExpr,
        rhs: TExpr,
    ) -> CResult<TExpr> {
        let ty = lhs.ty();
        let Some(inner) = self.sigs.unwrapped(ty) else {
            return Err(CheckError::at(line, "internal error: not an optional"));
        };
        let left = self.temp(ty);
        let right = self.temp(ty);
        let tag = |held: LocalId, source: Option<TExpr>| TExpr::Field {
            base: Box::new(match source {
                Some(value) => TExpr::Bind {
                    local: held,
                    init: Box::new(value),
                },
                None => TExpr::Local(held, ty),
            }),
            index: 0,
            ty: Ty::Int,
        };
        let payload = |held: LocalId| TExpr::Field {
            base: Box::new(TExpr::Local(held, ty)),
            index: 1,
            ty: inner,
        };
        let same_tag = TExpr::Binary {
            op: BinOp::Eq,
            lhs: Box::new(tag(left, Some(lhs))),
            rhs: Box::new(tag(right, Some(rhs))),
        };
        let is_none = TExpr::Binary {
            op: BinOp::Eq,
            lhs: Box::new(tag(left, None)),
            rhs: Box::new(TExpr::Int(NONE_CASE as i64)),
        };
        let values = self.equality_of(line, inner, payload(left), payload(right))?;
        let equal = TExpr::Binary {
            op: BinOp::And,
            lhs: Box::new(same_tag),
            rhs: Box::new(TExpr::Binary {
                op: BinOp::Or,
                lhs: Box::new(is_none),
                rhs: Box::new(values),
            }),
        };
        Ok(if op == BinOp::Ne {
            TExpr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(equal),
            }
        } else {
            equal
        })
    }

    fn operands(&self, line: u32, op: BinOp, lhs: Ty, rhs: Ty) -> CResult<()> {
        match op {
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge if lhs == Ty::Str => {
                self.expect(line, Ty::Str, rhs)
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge if lhs == Ty::Char => {
                self.expect(line, Ty::Char, rhs)
            }
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Rem
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Le
            | BinOp::Ge => {
                self.expect(line, Ty::Int, lhs)?;
                self.expect(line, Ty::Int, rhs)
            }
            BinOp::Eq | BinOp::Ne => {
                self.expect(line, lhs, rhs)?;
                let comparable =
                    matches!(lhs, Ty::Int | Ty::Bool | Ty::Str | Ty::Char | Ty::Enum(_))
                        || self.sigs.array_element(lhs).is_some()
                        || self.sigs.table_parts(lhs).is_some()
                        || (lhs.is_struct() && self.sigs.enum_of(lhs).is_none());
                if comparable {
                    Ok(())
                } else {
                    Err(CheckError::at(
                        line,
                        format!("equality is not supported for {} yet", self.name_of(lhs)),
                    ))
                }
            }
            BinOp::Is | BinOp::IsNot => {
                if lhs.is_class()
                    && rhs.is_class()
                    && (self.sigs.is_subclass(lhs, rhs) || self.sigs.is_subclass(rhs, lhs))
                {
                    Ok(())
                } else {
                    Err(CheckError::at(
                        line,
                        "'===' and '!==' compare instances of related classes",
                    ))
                }
            }
            BinOp::And | BinOp::Or => {
                self.expect(line, Ty::Bool, lhs)?;
                self.expect(line, Ty::Bool, rhs)
            }
        }
    }

    fn call(
        &mut self,
        line: u32,
        callee: &str,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        self.call_with(line, callee, args, expected, None)
    }

    fn call_with(
        &mut self,
        line: u32,
        callee: &str,
        args: &[Arg],
        expected: Option<Ty>,
        explicit: Option<&[TypeRef]>,
    ) -> CResult<TExpr> {
        if matches!(callee, ARRAY | DICTIONARY | SET) {
            return self.collection_init(line, callee, explicit, args, expected);
        }
        if let Ok(binding) = self.lookup(callee, line)
            && self.sigs.func_parts(binding.ty).is_some()
        {
            return self.call_value(line, callee, binding, args);
        }
        if callee == "String" {
            return self.string_conversion(line, args);
        }
        if callee == "Character" {
            return self.character_conversion(line, args);
        }
        let sigs = self.sigs;
        if let Some(receiver) = self.lookup("self", line).ok()
            && sigs.resolve_method(receiver.ty, callee).is_some()
        {
            let base = Expr::new(line, ExprKind::Ident("self".to_string()));
            return self.method_call(line, &base, callee, args, expected, explicit);
        }
        let is_type = sigs.struct_def(callee).is_some() || sigs.enum_def(callee).is_some();
        if is_type && sigs.has_init(callee) {
            return self.init_call(line, callee, args, expected, explicit);
        }
        if let Some(def) = sigs.struct_def(callee) {
            return self.struct_init(line, def, None, args, expected);
        }
        let sig = sigs
            .func(callee)
            .ok_or_else(|| CheckError::at(line, format!("cannot find function '{callee}'")))?;
        let checked = self.check_arguments(line, callee, sig, args)?;
        let explicit = match explicit {
            Some(given) => Some(self.explicit_args(line, callee, sig.generics().len(), given)?),
            None => None,
        };
        self.finish_call(line, callee, sig, checked, expected, explicit)
    }

    fn check_arguments(
        &mut self,
        line: u32,
        callee: &str,
        sig: &FuncSig,
        args: &[Arg],
    ) -> CResult<Vec<TExpr>> {
        if args.len() != sig.params().len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{callee}' takes {} argument(s), but {} were given",
                    sig.params().len(),
                    args.len()
                ),
            ));
        }
        self.check_values(line, sig, sig.params(), args, Vec::new())
    }

    fn check_values(
        &mut self,
        line: u32,
        sig: &FuncSig,
        params: &[ParamSig],
        args: &[Arg],
        prefix: Vec<TExpr>,
    ) -> CResult<Vec<TExpr>> {
        let sigs = self.sigs;
        let mut checked: Vec<Option<TExpr>> = prefix.into_iter().map(Some).collect();
        let offset = checked.len();
        let mut deferred = Vec::new();
        for (index, (arg, param)) in args.iter().zip(params).enumerate() {
            self.expect_label(line, arg, param.label())?;
            let closure_param = sigs.func_parts(param.ty()).is_some() && sigs.has_param(param.ty());
            if closure_param && matches!(arg.value().kind(), ExprKind::Closure(_)) {
                deferred.push(index);
                checked.push(None);
                continue;
            }
            if !sigs.has_param(param.ty()) {
                self.expected = Some(param.ty());
            }
            checked.push(Some(self.value(arg.value())?));
        }
        if !deferred.is_empty() {
            let patterns: Vec<Ty> = sig.params().iter().map(ParamSig::ty).collect();
            let mut bound: Vec<Option<Ty>> = vec![None; sig.generics().len()];
            for (pattern, value) in patterns.iter().zip(&checked) {
                if let Some(value) = value {
                    let mut attempt = bound.clone();
                    if sigs.unify(*pattern, value.ty(), &mut attempt) {
                        bound = attempt;
                    }
                }
            }
            for index in deferred {
                let ExprKind::Closure(closure) = args[index].value().kind() else {
                    continue;
                };
                let pattern = params[index].ty();
                let Some((pattern_params, pattern_ret)) = sigs.func_parts(pattern) else {
                    continue;
                };
                let known: Option<Vec<Ty>> = pattern_params
                    .iter()
                    .map(|p| sigs.subst_known(*p, &bound))
                    .collect();
                let ret = sigs.subst_known(pattern_ret, &bound);
                let value = self.closure_value(line, closure, known, ret)?;
                checked[offset + index] = Some(value);
            }
        }
        Ok(checked.into_iter().flatten().collect())
    }

    fn init_call(
        &mut self,
        line: u32,
        owner: &str,
        args: &[Arg],
        expected: Option<Ty>,
        explicit: Option<&[TypeRef]>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let names = sigs.initializers(owner);
        let candidates: Vec<&String> = if names.len() == 1 {
            names.iter().collect()
        } else {
            names
                .iter()
                .filter(|name| {
                    sigs.func(name).is_some_and(|sig| {
                        sig.params().len() == args.len()
                            && sig
                                .params()
                                .iter()
                                .zip(args)
                                .all(|(param, arg)| param.label() == arg.label())
                    })
                })
                .collect()
        };
        let (name, checked) = match candidates.as_slice() {
            [] => return Err(self.no_initializer(line, owner)),
            [only] => {
                let sig = sigs
                    .func(only)
                    .ok_or_else(|| CheckError::at(line, "internal error: unknown initializer"))?;
                (
                    (*only).clone(),
                    self.check_arguments(line, only, sig, args)?,
                )
            }
            several => self.choose_initializer(line, owner, several, args)?,
        };
        let sig = sigs
            .func(&name)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown initializer"))?;
        let explicit = match explicit {
            Some(given) => Some(self.explicit_args(line, owner, sig.generics().len(), given)?),
            None => None,
        };
        self.finish_call(line, &name, sig, checked, expected, explicit)
    }

    fn no_initializer(&self, line: u32, owner: &str) -> CheckError {
        let prefix = format!("{owner}.");
        let choices: Vec<&str> = self
            .sigs
            .initializers(owner)
            .iter()
            .map(|name| name.strip_prefix(&prefix).unwrap_or(name))
            .collect();
        CheckError::at(
            line,
            format!(
                "no initializer of '{owner}' takes these arguments; the choices are {}",
                choices.join(", ")
            ),
        )
    }

    fn choose_initializer(
        &mut self,
        line: u32,
        owner: &str,
        candidates: &[&String],
        args: &[Arg],
    ) -> CResult<(String, Vec<TExpr>)> {
        let sigs = self.sigs;
        let mut checked = Vec::with_capacity(args.len());
        for (position, arg) in args.iter().enumerate() {
            let wanted: Vec<Ty> = candidates
                .iter()
                .filter_map(|name| sigs.func(name))
                .map(|sig| sig.params()[position].ty())
                .collect();
            if let Some(first) = wanted.first()
                && wanted.iter().all(|ty| ty == first)
                && !sigs.has_param(*first)
            {
                self.expected = Some(*first);
            }
            checked.push(self.value(arg.value())?);
        }
        let mut best: Option<(usize, &String)> = None;
        let mut tied = false;
        for name in candidates {
            let Some(score) = sigs
                .func(name)
                .and_then(|sig| self.initializer_score(sig, &checked))
            else {
                continue;
            };
            match best {
                Some((top, _)) if score < top => {}
                Some((top, _)) if score == top => tied = true,
                _ => {
                    best = Some((score, name));
                    tied = false;
                }
            }
        }
        match best {
            None => Err(self.no_initializer(line, owner)),
            Some(_) if tied => Err(CheckError::at(
                line,
                format!("ambiguous use of an initializer of '{owner}'"),
            )),
            Some((_, name)) => Ok((name.clone(), checked)),
        }
    }

    fn initializer_score(&self, sig: &FuncSig, values: &[TExpr]) -> Option<usize> {
        let sigs = self.sigs;
        let mut bound: Vec<Option<Ty>> = vec![None; sig.generics().len()];
        let mut score = 0;
        for (param, value) in sig.params().iter().zip(values) {
            let found = value.ty();
            if param.ty() == found {
                score += 1;
            } else if sigs.has_param(param.ty()) {
                if !sigs.unify(param.ty(), found, &mut bound) {
                    return None;
                }
                score += 1;
            } else if !self.convertible(param.ty(), found) {
                return None;
            }
        }
        Some(score)
    }

    fn expect_label(&self, line: u32, arg: &Arg, label: Option<&str>) -> CResult<()> {
        if arg.label() == label {
            Ok(())
        } else {
            Err(CheckError::at(
                line,
                format!(
                    "argument label {:?} does not match the expected label {:?}",
                    arg.label(),
                    label
                ),
            ))
        }
    }

    fn finish_call(
        &self,
        line: u32,
        callee: &str,
        sig: &FuncSig,
        checked: Vec<TExpr>,
        expected: Option<Ty>,
        explicit: Option<Vec<Ty>>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let patterns: Vec<Ty> = sig.params().iter().map(ParamSig::ty).collect();
        let type_args = if sig.generics().is_empty() {
            Vec::new()
        } else if let Some(given) = explicit {
            given
        } else {
            let context = expected.map(|e| (sig.ret(), e));
            self.infer(line, callee, sig.generics(), &patterns, &checked, context)?
        };
        if let Some((ty, protocol)) = sigs.unsatisfied_bound(sig.bounds(), &type_args, &self.bounds)
        {
            return Err(CheckError::at(
                line,
                format!(
                    "type '{}' does not conform to the protocol '{protocol}' required by '{callee}'",
                    self.name_of(ty)
                ),
            ));
        }
        let mut args = Vec::with_capacity(checked.len());
        for (pattern, value) in patterns.iter().zip(checked) {
            let wanted = sigs.subst(*pattern, &type_args, &self.scope, line)?;
            args.push(self.coerce(line, wanted, value)?);
        }
        let ret = sigs.subst(sig.ret(), &type_args, &self.scope, line)?;
        Ok(TExpr::Call {
            callee: callee.to_string(),
            type_args,
            args,
            ret,
        })
    }

    fn generic_call(
        &mut self,
        line: u32,
        callee: &str,
        type_args: &[TypeRef],
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        if matches!(callee, ARRAY | DICTIONARY | SET) {
            return self.collection_init(line, callee, Some(type_args), args, expected);
        }
        let is_type =
            self.sigs.struct_def(callee).is_some() || self.sigs.enum_def(callee).is_some();
        if is_type && self.sigs.has_init(callee) {
            return self.init_call(line, callee, args, None, Some(type_args));
        }
        match self.sigs.struct_def(callee) {
            Some(def) => self.struct_init(line, def, Some(type_args), args, None),
            None => self.call_with(line, callee, args, expected, Some(type_args)),
        }
    }

    fn struct_init(
        &mut self,
        line: u32,
        def: usize,
        explicit: Option<&[TypeRef]>,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let generics = sigs.struct_generics(def);
        let base_name = sigs.struct_sig_of_def(def).unwrap_or_default();
        let inferring = !generics.is_empty() && explicit.is_none();
        let pattern_id = if inferring {
            let identity: Vec<Ty> = (0..generics.len()).map(Ty::Param).collect();
            sigs.instantiate_struct(def, identity, &scope_of(generics), line)?
        } else {
            let type_args =
                self.explicit_args(line, &base_name, generics.len(), explicit.unwrap_or(&[]))?;
            sigs.instantiate_struct(def, type_args, &self.scope, line)?
        };
        let sig = sigs
            .struct_sig(pattern_id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown struct"))?;
        let members: Vec<&FieldSig> = sig.members().map(|(_, f)| f).collect();
        if let Some(parent) = sigs.inherited_init_owner(sigs.struct_ty(def, pattern_id)) {
            return Err(CheckError::at(
                line,
                format!(
                    "'{base_name}' must declare an initializer because its superclass '{parent}' does"
                ),
            ));
        }
        if args.len() != members.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}' takes {} member(s), but {} were given",
                    sig.name(),
                    members.len(),
                    args.len()
                ),
            ));
        }
        let mut values = Vec::with_capacity(args.len());
        for (arg, field) in args.iter().zip(&members) {
            if arg.label() != Some(field.name()) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "expected the label '{}' here, in the order the members are declared",
                        field.name()
                    ),
                ));
            }
            if !inferring {
                self.expected = Some(field.ty());
            }
            let mut value = self.value(arg.value())?;
            if !inferring {
                value = self.coerce(line, field.ty(), value)?;
            }
            values.push(value);
        }
        let id = if inferring {
            let patterns: Vec<Ty> = members.iter().map(|f| f.ty()).collect();
            let context = expected.map(|e| (sigs.struct_ty(def, pattern_id), e));
            let type_args = self.infer(line, &base_name, generics, &patterns, &values, context)?;
            sigs.instantiate_struct(def, type_args, &self.scope, line)?
        } else {
            pattern_id
        };
        Ok(sigs.construct(sigs.struct_ty(def, id), values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Decl;
    use crate::items::split_items;
    use crate::lexer::lex;
    use crate::parser::parse_item;

    fn check_all(src: &str) -> Result<Vec<TFunc>, CheckError> {
        let tokens = lex(src).unwrap();
        let decls: Vec<Decl> = split_items(&tokens)
            .unwrap()
            .iter()
            .map(|item| parse_item(item.tokens()).unwrap())
            .collect();
        let sigs = Signatures::collect(&decls)?;
        decls
            .iter()
            .filter_map(|d| match d {
                Decl::Func(f) => Some(f),
                _ => None,
            })
            .map(|f| check_func(f, &sigs))
            .collect()
    }

    fn check_main(body: &str) -> Result<TFunc, CheckError> {
        check_all(&format!("func main() {{\n{body}\n}}")).map(|mut funcs| funcs.remove(0))
    }

    fn rejected_at(src: &str) -> (u32, String) {
        let err = check_all(src).unwrap_err();
        (err.line(), err.message().to_string())
    }

    fn main_rejected_at(body: &str) -> (u32, String) {
        rejected_at(&format!("func main() {{\n{body}\n}}"))
    }

    #[test]
    fn well_typed_arithmetic_and_variables_are_accepted() {
        let func = check_main("var a = 1\nlet b: Int = a * 2\na += b\nprint(a - -b)").unwrap();
        assert_eq!(func.name(), "main");
        assert_eq!(func.locals(), [Ty::Int, Ty::Int]);
    }

    #[test]
    fn a_declared_type_must_match_the_initializer() {
        assert_eq!(main_rejected_at("let a: Int = true").0, 2);
    }

    #[test]
    fn using_an_undefined_variable_names_it() {
        let (line, message) = main_rejected_at("print(1)\nprint(missing)");
        assert_eq!(line, 3);
        assert!(message.contains("missing"), "{message}");
    }

    #[test]
    fn a_let_binding_cannot_be_assigned() {
        assert_eq!(main_rejected_at("let a = 1\na = 2").0, 3);
    }

    #[test]
    fn a_loop_variable_cannot_be_assigned() {
        assert!(check_main("for k in 0..<3 { k = 1 }").is_err());
    }

    #[test]
    fn a_variable_must_be_initialized_where_it_is_declared() {
        assert!(check_main("var c: Int").is_err());
    }

    #[test]
    fn conditions_must_be_booleans() {
        assert!(check_main("if 1 { }").is_err());
        assert!(check_main("while 1 { }").is_err());
    }

    #[test]
    fn arithmetic_requires_integer_operands() {
        assert!(check_main("print(1 + true)").is_err());
        assert!(check_main("print(-true)").is_err());
        assert!(check_main("print(!1)").is_err());
    }

    #[test]
    fn equality_requires_operands_of_the_same_type() {
        assert!(check_main("print(1 == true)").is_err());
        assert!(check_main("print(true == false)").is_ok());
    }

    #[test]
    fn logical_operators_require_boolean_operands() {
        assert!(check_main("print(1 && true)").is_err());
        assert!(check_main("print(true || 1)").is_err());
    }

    #[test]
    fn a_function_with_a_result_must_return_on_every_path() {
        assert!(check_all("func f() -> Int { }").is_err());
        assert!(check_all("func f(_ a: Int) -> Int { if a > 0 { return 1 } }").is_err());
        assert!(
            check_all("func f(_ a: Int) -> Int { if a > 0 { return 1 } else { return 2 } }")
                .is_ok()
        );
        assert!(check_all("func f() -> Int { return 1 }").is_ok());
    }

    #[test]
    fn a_missing_return_is_reported_at_the_function() {
        assert_eq!(rejected_at("\nfunc f() -> Int {\n}").0, 2);
    }

    #[test]
    fn returned_values_must_match_the_declared_result() {
        assert!(check_all("func f() -> Int { return true }").is_err());
        assert!(check_all("func f() { return 1 }").is_err());
        assert!(check_all("func f() -> Int { return }").is_err());
    }

    #[test]
    fn call_arguments_must_match_labels_count_and_types() {
        let decl = "func add(_ a: Int, to b: Int) -> Int { return a + b }\n";
        let ok = format!("{decl}func main() {{ print(add(1, to: 2)) }}");
        assert!(check_all(&ok).is_ok());
        for bad in [
            "add(1, 2)",
            "add(1, by: 2)",
            "add(a: 1, to: 2)",
            "add(1)",
            "add(1, to: true)",
        ] {
            let src = format!("{decl}func main() {{ print({bad}) }}");
            assert!(check_all(&src).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn functions_may_be_called_before_they_are_declared() {
        assert!(
            check_all("func main() { print(later()) }\nfunc later() -> Int { return 1 }").is_ok()
        );
    }

    #[test]
    fn duplicate_function_names_are_rejected() {
        assert!(check_all("func f() { }\nfunc f() { }").is_err());
    }

    #[test]
    fn unknown_type_names_are_rejected() {
        assert!(check_all("func f(_ a: Foo) { }").is_err());
        assert!(check_all("func f() -> Foo { }").is_err());
        assert!(check_main("let a: Foo = 1").is_err());
    }

    #[test]
    fn redeclaring_a_name_in_the_same_scope_is_rejected_but_shadowing_is_allowed() {
        assert!(check_main("let a = 1\nlet a = 2").is_err());
        assert!(check_main("let a = 1\nif true { let a = true\nprint(a) }\nprint(a)").is_ok());
    }

    #[test]
    fn a_variable_goes_out_of_scope_at_the_end_of_its_block() {
        assert!(check_main("if true { let a = 1 }\nprint(a)").is_err());
    }

    #[test]
    fn for_requires_a_range_of_integers() {
        assert!(check_main("for k in true { }").is_err());
        assert!(check_main("for k in 0..<true { }").is_err());
        assert!(check_main("for k in 0...3 { print(k) }").is_ok());
    }

    #[test]
    fn string_literals_have_the_type_string() {
        assert!(check_main("let s: String = \"a\"\nprint(s)").is_ok());
        assert!(check_main("let s: String = 1").is_err());
        assert!(check_main("let n: Int = \"a\"").is_err());
    }

    #[test]
    fn strings_can_be_compared_and_concatenated() {
        assert!(check_main("print(\"a\" == \"b\")").is_ok());
        assert!(check_main("print(\"a\" != \"b\")").is_ok());
        assert!(check_main("print(\"a\" < \"b\")").is_ok());
        assert!(check_main("print(\"a\" < 1)").is_err());
        assert!(check_main("print(\"a\" + \"b\")").is_ok());
        assert!(check_main("print(\"a\" * \"b\")").is_err());
    }

    #[test]
    fn string_literals_cannot_contain_a_nul_character() {
        assert!(check_main("print(\"a\\0b\")").is_err());
    }

    const POINT: &str = "struct P { var x: Int; let tag: Int }\n";

    fn check_with_point(body: &str) -> Result<TFunc, CheckError> {
        check_all(&format!("{POINT}func main() {{\n{body}\n}}")).map(|mut funcs| funcs.remove(0))
    }

    #[test]
    fn a_struct_is_built_and_read_through_its_members() {
        assert!(check_with_point("let p = P(x: 1, tag: 2)\nprint(p.x + p.tag)").is_ok());
    }

    #[test]
    fn the_memberwise_initializer_needs_every_label_in_declaration_order() {
        for bad in [
            "P(tag: 2, x: 1)",
            "P(x: 1)",
            "P(1, 2)",
            "P(x: 1, tag: true)",
            "P(x: 1, tag: 2, z: 3)",
        ] {
            assert!(
                check_with_point(&format!("let p = {bad}")).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn reading_an_unknown_member_names_it() {
        let err = check_with_point("let p = P(x: 1, tag: 2)\nprint(p.nope)").unwrap_err();
        assert!(err.message().contains("nope"), "{}", err.message());
    }

    #[test]
    fn a_member_can_only_be_read_from_a_struct() {
        assert!(check_main("let a = 1\nprint(a.x)").is_err());
    }

    #[test]
    fn members_can_be_assigned_only_on_var_structs_and_var_fields() {
        assert!(check_with_point("var p = P(x: 1, tag: 2)\np.x = 3\np.x += 1").is_ok());
        assert!(check_with_point("let p = P(x: 1, tag: 2)\np.x = 3").is_err());
        assert!(check_with_point("var p = P(x: 1, tag: 2)\np.tag = 3").is_err());
        assert!(check_with_point("var p = P(x: 1, tag: 2)\np.x = true").is_err());
    }

    #[test]
    fn a_struct_parameter_cannot_be_mutated() {
        assert!(check_all(&format!("{POINT}func f(_ p: P) {{ p.x = 1 }}")).is_err());
    }

    #[test]
    fn structs_are_valid_parameter_result_and_annotation_types() {
        let src = format!(
            "{POINT}func f(_ p: P) -> P {{ return p }}\nfunc main() {{ let q: P = f(P(x: 1, tag: 2))\nprint(q.x) }}"
        );
        assert!(check_all(&src).is_ok());
    }

    #[test]
    fn structs_can_be_compared_but_not_printed_as_a_whole() {
        assert!(check_with_point("let a = P(x: 1, tag: 2)\nprint(a == a)").is_ok());
        assert!(check_with_point("print(P(x: 1, tag: 2))").is_err());
    }

    #[test]
    fn a_struct_name_cannot_be_reused() {
        assert!(check_all("struct P { var x: Int }\nstruct P { var y: Int }").is_err());
        assert!(check_all("struct P { var x: Int }\nfunc P() { }").is_err());
    }

    #[test]
    fn struct_fields_must_have_a_supported_type() {
        assert!(check_all("struct Q { var a: Int }\nstruct P { var q: Q }").is_ok());
        assert!(check_all("struct P { var a: Foo }").is_err());
        assert!(check_all("struct P { var a: Int; var a: Int }").is_err());
    }

    #[test]
    fn calling_an_unknown_function_is_rejected_at_its_line() {
        let (line, message) = main_rejected_at("nope(1)");
        assert_eq!(line, 2);
        assert!(message.contains("nope"), "{message}");
    }

    #[test]
    fn print_takes_exactly_one_unlabeled_argument() {
        assert!(check_main("print()").is_err());
        assert!(check_main("print(1, 2)").is_err());
        assert!(check_main("print(x: 1)").is_err());
    }

    #[test]
    fn a_call_that_returns_nothing_cannot_be_used_as_a_value() {
        assert!(check_all("func f() { }\nfunc main() { let a = f() }").is_err());
        assert!(check_main("let a = print(1)").is_err());
    }

    #[test]
    fn an_expression_whose_value_is_unused_is_rejected() {
        assert!(check_main("1 + 2").is_err());
    }

    const COLOR: &str = "enum Color { case red, green, blue }\n";

    fn with_color(body: &str) -> String {
        format!("{COLOR}func main() {{\n{body}\n}}")
    }

    #[test]
    fn an_enum_case_has_the_type_of_its_enum() {
        let funcs = check_all(&with_color("let c = Color.green\nlet d: Color = c")).unwrap();
        let [Ty::Enum(first), Ty::Enum(second)] = funcs[0].locals() else {
            panic!("expected two enum locals");
        };
        assert_eq!(first, second);
    }

    #[test]
    fn an_unknown_enum_case_is_rejected_with_its_line() {
        let (line, message) = rejected_at(&with_color("let c = Color.pink"));
        assert_eq!(line, 3);
        assert!(message.contains("no case 'pink'"), "{message}");
    }

    #[test]
    fn enum_values_of_one_enum_can_be_compared() {
        assert!(check_all(&with_color("print(Color.red == Color.blue)")).is_ok());
    }

    #[test]
    fn enum_values_of_different_enums_cannot_be_compared() {
        let source = format!(
            "enum Size {{ case big }}\n{}",
            with_color("print(Color.red == Size.big)")
        );
        assert!(check_all(&source).is_err());
    }

    #[test]
    fn an_enum_without_payloads_can_be_printed() {
        assert!(check_all(&with_color("print(Color.red)")).is_ok());
    }

    #[test]
    fn a_local_variable_shadows_an_enum_of_the_same_name() {
        let source = "struct P { var red: Int }\nenum P2 { case a }\nfunc main() { let P2 = P(red: 1)\nprint(P2.red) }";
        assert!(check_all(source).is_ok());
    }

    #[test]
    fn a_switch_covering_every_case_needs_no_default() {
        let body =
            "let c = Color.red\nswitch c {\ncase .red: print(1)\ncase .green, .blue: print(2)\n}";
        assert!(check_all(&with_color(body)).is_ok());
    }

    #[test]
    fn a_switch_missing_a_case_is_rejected_and_names_it() {
        let body = "let c = Color.red\nswitch c {\ncase .red: print(1)\ncase .green: print(2)\n}";
        let (line, message) = rejected_at(&with_color(body));
        assert_eq!(line, 4);
        assert!(message.contains("missing case 'blue'"), "{message}");
    }

    #[test]
    fn a_default_makes_a_partial_switch_exhaustive() {
        let body = "let c = Color.red\nswitch c {\ncase .red: print(1)\ndefault: print(2)\n}";
        assert!(check_all(&with_color(body)).is_ok());
    }

    #[test]
    fn a_default_before_the_last_case_is_rejected() {
        let body = "let c = Color.red\nswitch c {\ndefault: print(2)\ncase .red: print(1)\n}";
        assert!(check_all(&with_color(body)).is_err());
    }

    #[test]
    fn a_pattern_of_another_enum_or_an_unknown_case_is_rejected() {
        let other = "let c = Color.red\nswitch c {\ncase Size.big: print(1)\ndefault: print(2)\n}";
        assert!(check_all(&format!("enum Size {{ case big }}\n{}", with_color(other))).is_err());
        let unknown = "let c = Color.red\nswitch c {\ncase .pink: print(1)\ndefault: print(2)\n}";
        assert!(check_all(&with_color(unknown)).is_err());
    }

    #[test]
    fn switching_on_a_struct_is_rejected() {
        let src = "struct P { var x: Int }\nfunc main() {\nlet p = P(x: 1)\nswitch p {\ndefault: print(2)\n}\n}";
        assert!(check_all(src).is_err());
    }

    #[test]
    fn a_switch_without_cases_is_rejected() {
        assert!(check_all(&with_color("switch Color.red {\n}")).is_err());
    }

    #[test]
    fn a_function_returning_from_every_case_of_a_switch_needs_no_final_return() {
        let source = format!(
            "{COLOR}func number(_ c: Color) -> Int {{\nswitch c {{\ncase .red: return 1\ncase .green: return 2\ncase .blue: return 3\n}}\n}}"
        );
        assert!(check_all(&source).is_ok());
    }

    #[test]
    fn a_function_whose_switch_leaves_a_case_without_return_is_rejected() {
        let source = format!(
            "{COLOR}func number(_ c: Color) -> Int {{\nswitch c {{\ncase .red: return 1\ndefault: print(0)\n}}\n}}"
        );
        assert!(check_all(&source).is_err());
    }

    #[test]
    fn a_struct_may_have_an_enum_field() {
        let source = format!(
            "{COLOR}struct Pixel {{ var color: Color }}\nfunc main() {{ let p = Pixel(color: Color.blue)\nprint(p.color == Color.blue) }}"
        );
        assert!(check_all(&source).is_ok());
    }

    #[test]
    fn an_enum_cannot_share_a_name_with_another_type_or_a_function() {
        assert!(check_all("enum A { case x }\nstruct A { var y: Int }").is_err());
        assert!(check_all("enum A { case x }\nfunc A() { }").is_err());
        assert!(check_all("enum A { case x }\nenum A { case y }").is_err());
    }

    #[test]
    fn an_enum_cannot_repeat_a_case() {
        assert!(check_all("enum A { case x, x }").is_err());
    }

    const SHAPE: &str =
        "enum Shape { case circle(Int), rect(Int, Int), label(String, Bool), empty }\n";

    fn with_shape(body: &str) -> String {
        format!("{SHAPE}func main() {{\n{body}\n}}")
    }

    #[test]
    fn a_payload_case_is_built_by_calling_it_with_its_values() {
        let funcs = check_all(&with_shape(
            "let a = Shape.circle(1)\nlet b = Shape.rect(2, 3)\nlet c = Shape.label(\"x\", true)\nlet d = Shape.empty",
        ))
        .unwrap();
        let types = funcs[0].locals();
        assert!(types.iter().all(|t| *t == types[0]));
        assert!(types[0].is_struct());
    }

    #[test]
    fn a_payload_case_with_the_wrong_values_is_rejected() {
        assert!(check_all(&with_shape("let a = Shape.circle(true)")).is_err());
        assert!(check_all(&with_shape("let a = Shape.rect(1)")).is_err());
        assert!(check_all(&with_shape("let a = Shape.circle(x: 1)")).is_err());
    }

    #[test]
    fn a_case_that_carries_values_cannot_be_used_without_them() {
        let (_, message) = rejected_at(&with_shape("let a = Shape.circle"));
        assert!(message.contains("must be called"), "{message}");
    }

    #[test]
    fn calling_a_case_that_carries_nothing_is_rejected() {
        assert!(check_all(&with_shape("let a = Shape.empty(1)")).is_err());
    }

    #[test]
    fn a_member_call_on_something_other_than_an_enum_is_rejected() {
        assert!(check_all(&with_shape("let a = 1\nlet b = a.foo(2)")).is_err());
    }

    #[test]
    fn a_payload_enum_value_has_no_members_and_cannot_be_compared_or_printed() {
        assert!(check_all(&with_shape("let a = Shape.circle(1)\nprint(a.tag)")).is_err());
        assert!(check_all(&with_shape("let a = Shape.circle(1)\nprint(a == a)")).is_err());
        assert!(check_all(&with_shape("print(Shape.empty)")).is_err());
    }

    #[test]
    fn a_pattern_binds_the_payload_values_with_their_types() {
        let body = "switch Shape.rect(1, 2) {\ncase .circle(let r): print(r)\ncase .rect(let w, _): print(w)\ncase .label(let t, let b): print(t)\nprint(b)\ncase .empty: print(0)\n}";
        assert!(check_all(&with_shape(body)).is_ok());
    }

    #[test]
    fn a_bound_value_is_not_visible_outside_its_case() {
        let body = "switch Shape.circle(1) {\ncase .circle(let r): print(r)\ndefault: print(r)\n}";
        assert!(check_all(&with_shape(body)).is_err());
    }

    #[test]
    fn a_pattern_must_bind_every_value_or_none() {
        let few = "switch Shape.rect(1, 2) {\ncase .rect(let w): print(w)\ndefault: print(0)\n}";
        assert!(check_all(&with_shape(few)).is_err());
        let none = "switch Shape.rect(1, 2) {\ncase .rect: print(1)\ndefault: print(0)\n}";
        assert!(check_all(&with_shape(none)).is_ok());
    }

    #[test]
    fn a_case_with_bindings_must_have_a_single_pattern() {
        let body = "switch Shape.rect(1, 2) {\ncase .circle(let r), .empty: print(r)\ndefault: print(0)\n}";
        assert!(check_all(&with_shape(body)).is_err());
    }

    #[test]
    fn a_payload_switch_must_still_be_exhaustive() {
        let body = "switch Shape.empty {\ncase .circle: print(1)\ncase .rect: print(2)\ncase .empty: print(3)\n}";
        let (_, message) = rejected_at(&with_shape(body));
        assert!(message.contains("missing case 'label'"), "{message}");
    }

    #[test]
    fn a_payload_can_be_a_struct_or_an_enum_that_carries_values() {
        assert!(check_all("struct P { var x: Int }\nenum E { case a(P) }").is_ok());
        assert!(check_all("enum A { case x(Int) }\nenum B { case y(A) }").is_ok());
        assert!(check_all("enum E { case a(Missing) }").is_err());
    }

    #[test]
    fn payloads_at_the_same_position_must_agree_when_they_are_structs_or_classes() {
        let source = "struct P { var x: Int }\nstruct Q { var y: Int }\nenum E { case a(P)\n case b(Q) }\nfunc f(_ e: E) { }";
        assert!(check_all(source).is_err());
        let same = "struct P { var x: Int }\nenum E { case a(P)\n case b(P) }\nfunc f(_ e: E) { }";
        assert!(check_all(same).is_ok());
    }

    #[test]
    fn a_payload_may_be_an_enum_without_values() {
        assert!(check_all("enum Color { case red }\nenum E { case a(Color) }").is_ok());
    }

    #[test]
    fn a_struct_field_can_be_an_enum_that_carries_values() {
        let source = format!("{SHAPE}struct P {{ var s: Shape }}");
        assert!(check_all(&source).is_ok());
    }

    #[test]
    fn a_payload_enum_can_be_a_parameter_a_result_and_an_annotation() {
        let source = format!(
            "{SHAPE}func make() -> Shape {{ return Shape.circle(1) }}\nfunc use(_ s: Shape) {{ let t: Shape = s }}\nfunc main() {{ use(make()) }}"
        );
        assert!(check_all(&source).is_ok());
    }

    const BOX: &str = "struct Box<T> { var value: T }\n";
    const MAYBE: &str = "enum Maybe<T> { case some(T), none }\n";

    #[test]
    fn a_value_of_a_type_parameter_can_only_be_passed_around() {
        let source = "func keep<T>(_ a: T) -> T { let b = a\nreturn b }\nfunc main() { }";
        assert!(check_all(source).is_ok());
    }

    #[test]
    fn a_type_parameter_supports_no_operators_printing_or_literals() {
        for body in ["return a == b", "print(a)\nreturn true", "return a < b"] {
            let source = format!("func f<T>(_ a: T, _ b: T) -> Bool {{ {body} }}");
            assert!(check_all(&source).is_err(), "{body}");
        }
        assert!(check_all("func f<T>(_ a: T) -> Int { return a }").is_err());
        assert!(check_all("func f<T>(_ a: T) -> T { return 1 }").is_err());
    }

    #[test]
    fn an_uncalled_generic_function_is_still_checked() {
        let (line, _) = rejected_at("func f<T>(_ a: T) -> Int {\nreturn a\n}\nfunc main() { }");
        assert_eq!(line, 2);
    }

    #[test]
    fn calling_a_generic_function_infers_its_type_arguments_from_the_arguments() {
        let source =
            "func id<T>(_ x: T) -> T { return x }\nfunc main() { let a = id(1)\nlet b = id(true) }";
        let funcs = check_all(source).unwrap();
        assert_eq!(funcs[1].locals(), [Ty::Int, Ty::Bool]);
    }

    #[test]
    fn conflicting_arguments_for_one_type_parameter_are_rejected() {
        let source = "func same<T>(_ a: T, _ b: T) { }\nfunc main() { same(1, true) }";
        assert!(check_all(source).is_err());
    }

    #[test]
    fn a_type_parameter_that_no_argument_determines_is_reported() {
        let source = "func make<T>(_ n: Int) -> Int { return n }\nfunc main() { let a = make(1) }";
        let (_, message) = rejected_at(source);
        assert!(
            message.contains("cannot infer the type parameter 'T'"),
            "{message}"
        );
    }

    #[test]
    fn a_generic_function_takes_explicit_type_arguments() {
        let source = "func id<T>(_ x: T) -> T { return x }\nfunc main() { let a = id<Int>(1) }";
        assert!(check_all(source).is_ok());
        let mismatch =
            "func id<T>(_ x: T) -> T { return x }\nfunc main() { let a = id<Int>(true) }";
        assert!(check_all(mismatch).is_err());
    }

    #[test]
    fn a_generic_struct_infers_its_arguments_from_the_member_values() {
        let source = format!("{BOX}func main() {{ let b = Box(value: 1)\nlet c: Box<Int> = b }}");
        assert!(check_all(&source).is_ok());
    }

    #[test]
    fn type_argument_counts_and_targets_are_checked() {
        let too_many = format!("{BOX}func f(_ b: Box<Int, Int>) {{ }}");
        assert!(check_all(&too_many).is_err());
        let missing = format!("{BOX}func f(_ b: Box) {{ }}");
        assert!(check_all(&missing).is_err());
        assert!(check_all("func f(_ x: Int<Int>) { }").is_err());
        assert!(check_all("func f(_ x: Missing<Int>) { }").is_err());
    }

    #[test]
    fn a_generic_struct_can_hold_a_struct_through_its_type_argument() {
        let source = format!("{BOX}struct P {{ var x: Int }}\nfunc f(_ b: Box<P>) {{ }}");
        assert!(check_all(&source).is_ok());
    }

    #[test]
    fn a_generic_struct_cannot_contain_itself() {
        assert!(
            check_all("struct Node<T> { var next: Node<T> }\nfunc f(_ n: Node<Int>) { }").is_err()
        );
    }

    #[test]
    fn a_generic_enum_case_without_values_needs_a_type_from_its_context() {
        let bare = format!("{MAYBE}func main() {{ let m = Maybe.none }}");
        let (_, message) = rejected_at(&bare);
        assert!(message.contains("cannot infer"), "{message}");
        let annotated = format!("{MAYBE}func main() {{ let m: Maybe<Int> = Maybe.none }}");
        assert!(check_all(&annotated).is_ok());
        let returned = format!("{MAYBE}func f() -> Maybe<Int> {{ return Maybe.none }}");
        assert!(check_all(&returned).is_ok());
        let explicit = format!("{MAYBE}func main() {{ let m = Maybe<Int>.none }}");
        assert!(check_all(&explicit).is_ok());
    }

    #[test]
    fn a_generic_enum_infers_its_argument_from_the_payload() {
        let source =
            format!("{MAYBE}func main() {{ let m = Maybe.some(1)\nlet n: Maybe<Int> = m }}");
        assert!(check_all(&source).is_ok());
        let wrong = format!("{MAYBE}func main() {{ let m: Maybe<Bool> = Maybe.some(1) }}");
        assert!(check_all(&wrong).is_err());
    }

    #[test]
    fn instances_of_a_generic_type_with_different_arguments_are_different_types() {
        let source = format!("{BOX}func main() {{ let a: Box<Int> = Box(value: true) }}");
        assert!(check_all(&source).is_err());
    }

    #[test]
    fn a_switch_inside_a_generic_function_binds_values_of_the_type_parameter() {
        let source = format!(
            "{MAYBE}func get<T>(_ m: Maybe<T>, _ d: T) -> T {{\nswitch m {{\ncase .some(let v): return v\ncase .none: return d\n}}\n}}"
        );
        assert!(check_all(&source).is_ok());
    }
}

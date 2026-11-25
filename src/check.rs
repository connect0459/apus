use std::collections::HashMap;

use crate::ast::{
    Arg, BinOp, CaseLabel, Expr, ExprKind, FuncDecl, Pattern, Stmt, StmtKind, SwitchCase, TypeRef,
    UnaryOp,
};
use crate::init_check::{InitScope, check_enum_initialization, check_initialization};
use crate::sigs::{
    COMPARABLE, CResult, EQUATABLE, EnumSig, FieldSig, FuncSig, NONE_CASE, PRINT, ParamSig,
    SOME_CASE, bound_includes, scope_of,
};
use crate::tir::{LocalId, TAG_SLOT, TExpr, TFunc, TStmt};
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
        scope: scope_of(sig.generics()),
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
    Ok(TFunc::new(
        func.name().to_string(),
        func.params().len() + usize::from(receiver),
        sig.ret(),
        cx.locals,
        body,
    ))
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

fn always_returns(stmts: &[Stmt]) -> bool {
    match stmts.last().map(Stmt::kind) {
        Some(StmtKind::Return(_)) => true,
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

pub fn equality_function(sigs: &Signatures, ty: Ty, name: String) -> CResult<TFunc> {
    let mut cx = FnCx {
        sigs,
        scope: Vec::new(),
        expected: None,
        ret: Ty::Bool,
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
    };
    let left = cx.declare(0, "left", ty, false)?;
    let right = cx.declare(0, "right", ty, false)?;
    let same = cx.struct_equality(
        0,
        BinOp::Eq,
        TExpr::Local(left, ty),
        TExpr::Local(right, ty),
    )?;
    Ok(TFunc::new(
        name,
        2,
        Ty::Bool,
        cx.locals,
        vec![TStmt::Return(Some(same))],
    ))
}

fn always_exits(stmts: &[Stmt]) -> bool {
    exits(stmts, false, &[])
}

fn exits(stmts: &[Stmt], in_switch: bool, inner_labels: &[String]) -> bool {
    match stmts.last().map(Stmt::kind) {
        Some(StmtKind::Return(_) | StmtKind::Continue(_)) => true,
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

impl FnCx<'_> {
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

    fn field_of(&self, line: u32, base: TExpr, name: &str) -> CResult<TExpr> {
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
        let tag = self.temp(Ty::Int);
        let tags = self.sigs.class_tags(target);
        let first = tags.first().copied().unwrap_or(-1);
        let mut condition = TExpr::Binary {
            op: BinOp::Eq,
            lhs: Box::new(TExpr::Bind {
                local: tag,
                init: Box::new(TExpr::Field {
                    base: Box::new(TExpr::Bind {
                        local: object,
                        init: Box::new(subject),
                    }),
                    index: TAG_SLOT,
                    ty: Ty::Int,
                }),
            }),
            rhs: Box::new(TExpr::Int(first)),
        };
        for other in tags.iter().skip(1) {
            condition = TExpr::Binary {
                op: BinOp::Or,
                lhs: Box::new(condition),
                rhs: Box::new(TExpr::Binary {
                    op: BinOp::Eq,
                    lhs: Box::new(TExpr::Local(tag, Ty::Int)),
                    rhs: Box::new(TExpr::Int(*other)),
                }),
            };
        }
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

    fn coerce(&self, line: u32, expected: Ty, value: TExpr) -> CResult<TExpr> {
        let found = value.ty();
        if expected == found || self.sigs.is_subclass(found, expected) {
            return Ok(value);
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
        if matches!(subject_ty, Ty::Int | Ty::Bool | Ty::Str) {
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
                    let mut condition: Option<TExpr> = None;
                    let mut prelude = Vec::new();
                    self.scopes.push(HashMap::new());
                    let outcome = self.case_patterns(
                        case,
                        patterns,
                        (&sig, temporary, subject_ty),
                        &mut covered,
                        &mut condition,
                        &mut prelude,
                    );
                    let body = outcome.and_then(|()| self.block(case.body()));
                    self.scopes.pop();
                    let mut body = body?;
                    prelude.append(&mut body);
                    let Some(condition) = condition else {
                        return Err(CheckError::at(case.line(), "a case needs a pattern"));
                    };
                    branches.push((condition, prelude));
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

    fn literal_pattern(&mut self, line: u32, expr: &Expr) -> CResult<TExpr> {
        let value = self.value(expr)?;
        let literal = match &value {
            TExpr::Int(_) | TExpr::Bool(_) | TExpr::Str(_) => true,
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
            if ty != Ty::Int {
                return Err(CheckError::at(
                    line,
                    format!("a range pattern needs an Int, not {}", self.name_of(ty)),
                ));
            }
            let lo = self.literal_pattern(line, lo)?;
            let hi = self.literal_pattern(line, hi)?;
            self.expect(line, Ty::Int, lo.ty())?;
            self.expect(line, Ty::Int, hi.ty())?;
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
        let value = self.literal_pattern(line, pattern)?;
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
        condition: &mut Option<TExpr>,
        prelude: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let (sig, temporary, subject_ty) = subject;
        for pattern in patterns {
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
                if patterns.len() > 1 {
                    return Err(CheckError::at(
                        case.line(),
                        "a case with bindings must have a single pattern",
                    ));
                }
                self.bind_payload(
                    case.line(),
                    sig,
                    index,
                    (temporary, subject_ty),
                    bindings,
                    prelude,
                )?;
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
            *condition = Some(match condition.take() {
                None => test,
                Some(previous) => TExpr::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(previous),
                    rhs: Box::new(test),
                },
            });
        }
        Ok(())
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
        bindings: &[Option<String>],
        prelude: &mut Vec<TStmt>,
    ) -> CResult<()> {
        let case = &sig.cases()[index];
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
        for (slot, (binding, ty)) in bindings.iter().zip(case.payload()).enumerate() {
            let Some(name) = binding else { continue };
            let local = self.declare(line, name, *ty, false)?;
            prelude.push(TStmt::Let {
                local,
                init: TExpr::Field {
                    base: Box::new(TExpr::Local(temporary, subject_ty)),
                    index: 1 + slot,
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
            ExprKind::Ident(name) => self.assign_variable(line, name, op, value),
            ExprKind::Member { base, name } => self.assign_member(line, base, name, op, value),
            _ => Err(CheckError::at(
                line,
                "the left side of an assignment must be assignable",
            )),
        }
    }

    fn assign_variable(
        &mut self,
        line: u32,
        name: &str,
        op: Option<BinOp>,
        value: &Expr,
    ) -> CResult<TStmt> {
        let binding = match self.lookup(name, line) {
            Ok(binding) => binding,
            Err(error) => {
                return match self.implicit_member(line, name) {
                    Some((receiver, _, _)) => {
                        self.assign_through(line, "self", receiver, name, op, value)
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
        self.expected = Some(binding.ty);
        let rhs = self.value(value)?;
        let current = TExpr::Local(binding.local, binding.ty);
        let new_value = self.assigned_value(line, binding.ty, current, op, rhs)?;
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
        op: Option<BinOp>,
        value: &Expr,
    ) -> CResult<TStmt> {
        let ExprKind::Ident(var) = base.kind() else {
            return Err(CheckError::at(
                line,
                "only a member of a variable can be assigned",
            ));
        };
        let binding = self.lookup(var, line)?;
        self.assign_through(line, var, binding, name, op, value)
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
        op: Option<BinOp>,
        value: &Expr,
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
        self.expected = Some(field_ty);
        let rhs = self.value(value)?;
        let current = TExpr::Field {
            base: Box::new(TExpr::Local(binding.local, binding.ty)),
            index,
            ty: field_ty,
        };
        let new_value = self.assigned_value(line, field_ty, current, op, rhs)?;
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
            return Err(CheckError::at(line, "a for loop iterates over a range"));
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

    fn expr_stmt(&mut self, expr: &Expr) -> CResult<TStmt> {
        if let ExprKind::MemberCall { base, name, args } = expr.kind()
            && self.enum_target(base).is_none()
        {
            return Ok(TStmt::Eval(self.method_call(
                expr.line(),
                base,
                name,
                args,
                None,
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
        Ok(TStmt::Eval(self.call(expr.line(), callee, args, None)?))
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
                let lhs = self.value(lhs)?;
                let rhs = self.value(rhs)?;
                if let (Ty::Param(index), true) = (
                    lhs.ty(),
                    matches!(
                        op,
                        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
                    ),
                ) {
                    return self.parameter_operator(line, *op, index, lhs, rhs);
                }
                self.operands(line, *op, lhs.ty(), rhs.ty())?;
                if matches!(op, BinOp::Eq | BinOp::Ne) && lhs.ty().is_struct() {
                    return self.struct_equality(line, *op, lhs, rhs);
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
            ExprKind::Call { callee, args } => self.call(line, callee, args, expected),
            ExprKind::GenericCall {
                callee,
                type_args,
                args,
            } => self.generic_call(line, callee, type_args, args),
            ExprKind::GenericName { name, .. } => Err(CheckError::at(
                line,
                format!("'{name}' with type arguments is not a value"),
            )),
            ExprKind::Member { base, name } if self.enum_target(base).is_some() => {
                self.enum_case(line, base, name, expected)
            }
            ExprKind::MemberCall { base, name, args } if self.enum_target(base).is_some() => {
                self.enum_construct(line, base, name, args, expected)
            }
            ExprKind::MemberCall { base, name, args } => {
                self.method_call(line, base, name, args, expected)
            }
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
        }
    }

    fn method_call(
        &mut self,
        line: u32,
        base: &Expr,
        name: &str,
        args: &[Arg],
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let (receiver, lookup_ty, dispatching) = if self.is_super(base) {
            let (receiver, parent) = self.super_receiver(line)?;
            (receiver, parent, false)
        } else {
            let receiver = self.value(base)?;
            let ty = receiver.ty();
            if let Ty::Param(index) = ty {
                return self.witness_call(line, base, receiver, index, name, args);
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
        let mut checked = vec![receiver];
        for (arg, param) in args.iter().zip(params) {
            self.expect_label(line, arg, param.label())?;
            if !sigs.has_param(param.ty()) {
                self.expected = Some(param.ty());
            }
            checked.push(self.value(arg.value())?);
        }
        let call = self.finish_call(line, &qualified, sig, checked, expected, None)?;
        let cases = if dispatching {
            sigs.dispatch_cases(lookup_ty, name)
        } else {
            Vec::new()
        };
        Ok(match call {
            TExpr::Call {
                callee, args, ret, ..
            } if !cases.is_empty() => TExpr::Virtual {
                cases,
                default: callee,
                args,
                ret,
            },
            other => other,
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
        let built = self.call(line, &parent_name, args, None)?;
        self.expect(line, parent, built.ty())?;
        let Ty::Class(parent_id) = parent else {
            return Err(CheckError::at(
                line,
                "internal error: a superclass is a class",
            ));
        };
        let layout = sigs
            .struct_sig(parent_id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown superclass layout"))?;
        let temporary = LocalId::new(self.locals.len());
        self.locals.push(parent);
        out.push(TStmt::Let {
            local: temporary,
            init: built,
        });
        for (index, field) in layout.members() {
            out.push(TStmt::AssignField {
                local: target,
                index,
                value: TExpr::Moved(Box::new(TExpr::Field {
                    base: Box::new(TExpr::Local(temporary, parent)),
                    index,
                    ty: field.ty(),
                })),
            });
        }
        out.push(TStmt::Free(TExpr::Local(temporary, parent)));
        Ok(())
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

    fn enum_target<'e>(&self, base: &'e Expr) -> Option<(usize, Option<&'e [TypeRef]>)> {
        match base.kind() {
            ExprKind::Ident(name) if self.lookup(name, base.line()).is_err() => {
                self.sigs.enum_def(name).map(|def| (def, None))
            }
            ExprKind::GenericName { name, type_args } => self
                .sigs
                .enum_def(name)
                .map(|def| (def, Some(type_args.as_slice()))),
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

    fn enum_case(
        &self,
        line: u32,
        base: &Expr,
        name: &str,
        expected: Option<Ty>,
    ) -> CResult<TExpr> {
        let Some((def, explicit)) = self.enum_target(base) else {
            return Err(CheckError::at(line, "internal error: not an enum name"));
        };
        let sigs = self.sigs;
        let generics = sigs.enum_generics(def);
        let base_name = match base.kind() {
            ExprKind::Ident(n) | ExprKind::GenericName { name: n, .. } => n.clone(),
            _ => String::new(),
        };
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
        let Some((def, explicit)) = self.enum_target(base) else {
            return Err(CheckError::at(line, "internal error: not an enum name"));
        };
        let sigs = self.sigs;
        let generics = sigs.enum_generics(def);
        let base_name = match base.kind() {
            ExprKind::Ident(n) | ExprKind::GenericName { name: n, .. } => n.clone(),
            _ => String::new(),
        };
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
        let mut values = Vec::with_capacity(args.len());
        for (arg, wanted) in args.iter().zip(&payload) {
            if arg.label().is_some() {
                return Err(CheckError::at(line, "a payload value cannot be labeled"));
            }
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
        let mut slots = vec![TExpr::Int(index as i64)];
        slots.extend(values);
        let ty = sigs.enum_ty(id);
        Ok(TExpr::StructInit {
            ty,
            args: self.pad_payload(ty, slots),
        })
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
            let same = TExpr::Binary {
                op: BinOp::Eq,
                lhs: Box::new(reach(left_root)),
                rhs: Box::new(reach(right_root)),
            };
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

    fn operands(&self, line: u32, op: BinOp, lhs: Ty, rhs: Ty) -> CResult<()> {
        match op {
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
                let comparable = matches!(lhs, Ty::Int | Ty::Bool | Ty::Str | Ty::Enum(_))
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
        let sigs = self.sigs;
        if explicit.is_none()
            && let Some(receiver) = self.lookup("self", line).ok()
            && sigs.resolve_method(receiver.ty, callee).is_some()
        {
            let base = Expr::new(line, ExprKind::Ident("self".to_string()));
            return self.method_call(line, &base, callee, args, expected);
        }
        let init_name = format!("{callee}.init");
        let is_type = sigs.struct_def(callee).is_some() || sigs.enum_def(callee).is_some();
        let callee = if is_type && sigs.func(&init_name).is_some() {
            init_name.as_str()
        } else {
            callee
        };
        if let Some(def) = sigs.struct_def(callee) {
            return self.struct_init(line, def, None, args, expected);
        }
        let sig = sigs
            .func(callee)
            .ok_or_else(|| CheckError::at(line, format!("cannot find function '{callee}'")))?;
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
        let mut checked = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(sig.params()) {
            self.expect_label(line, arg, param.label())?;
            if !sigs.has_param(param.ty()) {
                self.expected = Some(param.ty());
            }
            checked.push(self.value(arg.value())?);
        }
        let explicit = match explicit {
            Some(given) => Some(self.explicit_args(
                line,
                callee.strip_suffix(".init").unwrap_or(callee),
                sig.generics().len(),
                given,
            )?),
            None => None,
        };
        self.finish_call(line, callee, sig, checked, expected, explicit)
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
    ) -> CResult<TExpr> {
        let init_name = format!("{callee}.init");
        let has_init = self.sigs.func(&init_name).is_some();
        match self.sigs.struct_def(callee) {
            _ if has_init => self.call_with(line, &init_name, args, None, Some(type_args)),
            Some(def) => self.struct_init(line, def, Some(type_args), args, None),
            None => Err(CheckError::at(
                line,
                format!("'{callee}' does not take explicit type arguments"),
            )),
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
    fn strings_can_be_compared_but_not_combined_yet() {
        assert!(check_main("print(\"a\" == \"b\")").is_ok());
        assert!(check_main("print(\"a\" != \"b\")").is_ok());
        assert!(check_main("print(\"a\" < \"b\")").is_err());
        assert!(check_main("print(\"a\" + \"b\")").is_err());
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
    fn an_enum_value_cannot_be_printed() {
        assert!(check_all(&with_color("print(Color.red)")).is_err());
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
    fn a_generic_function_does_not_take_explicit_type_arguments() {
        let source = "func id<T>(_ x: T) -> T { return x }\nfunc main() { let a = id<Int>(1) }";
        assert!(check_all(source).is_err());
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

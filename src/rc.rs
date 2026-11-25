use crate::ast::BinOp;
use crate::sigs::Signatures;
use crate::tir::{LocalId, RC_SLOT, TAG_SLOT, TExpr, TFunc, TStmt, UNOWNED_SLOT};
use crate::types::{StructId, Ty};

const DESTROYING: i64 = 1 << 40;

pub fn insert(func: &TFunc, sigs: &Signatures) -> (TFunc, Vec<StructId>) {
    let mut pass = Pass {
        sigs,
        locals: func.locals().to_vec(),
        scopes: Vec::new(),
        jumps: Vec::new(),
        temps: Vec::new(),
        created: Vec::new(),
    };
    let body = pass.block(func.body());
    let rewritten = TFunc::new(
        func.name().to_string(),
        func.param_types().len(),
        func.ret(),
        pass.locals,
        body,
    );
    (rewritten, pass.created)
}

fn needs_rc(sigs: &Signatures, ty: Ty) -> bool {
    match ty {
        Ty::Class(_) | Ty::Str => true,
        Ty::Struct(id) => sigs
            .struct_sig(id)
            .is_some_and(|sig| sig.fields().iter().any(|f| needs_rc(sigs, f.ty()))),
        _ => false,
    }
}

pub fn drop_function(sigs: &Signatures, id: StructId) -> TFunc {
    let class = Ty::Class(id);
    let this = LocalId::new(0);
    let receiver = || TExpr::Local(this, class);
    let mut body = vec![TStmt::AssignField {
        local: this,
        index: RC_SLOT,
        value: TExpr::Int(DESTROYING),
    }];
    let mut chain = Some(class);
    while let Some(ty) = chain {
        if let Some(owner) = sigs.owner_name(ty) {
            let deinit = format!("{owner}.deinit");
            if sigs.func(&deinit).is_some() {
                body.push(TStmt::Eval(TExpr::Call {
                    callee: deinit,
                    type_args: Vec::new(),
                    args: vec![receiver()],
                    ret: Ty::Void,
                }));
            }
        }
        chain = sigs.superclass(ty);
    }
    if let Some(layout) = sigs.struct_sig(id) {
        for (index, field) in layout.members() {
            if field.is_weak() {
                if let Some(Ty::Class(target)) = sigs.unwrapped(field.ty()) {
                    body.push(TStmt::ReleaseUnowned(TExpr::Field {
                        base: Box::new(TExpr::Field {
                            base: Box::new(receiver()),
                            index,
                            ty: field.ty(),
                        }),
                        index: 1,
                        ty: Ty::Class(target),
                    }));
                }
                continue;
            }
            if !needs_rc(sigs, field.ty()) {
                continue;
            }
            let member = TExpr::Field {
                base: Box::new(receiver()),
                index,
                ty: field.ty(),
            };
            body.push(if field.is_unowned() {
                TStmt::ReleaseUnowned(member)
            } else {
                TStmt::Release(member)
            });
        }
    }
    body.push(TStmt::AssignField {
        local: this,
        index: RC_SLOT,
        value: TExpr::Int(0),
    });
    body.push(TStmt::If {
        cond: TExpr::Binary {
            op: BinOp::Eq,
            lhs: Box::new(TExpr::Field {
                base: Box::new(receiver()),
                index: UNOWNED_SLOT,
                ty: Ty::Int,
            }),
            rhs: Box::new(TExpr::Int(0)),
        },
        then_body: vec![TStmt::Free(receiver())],
        else_body: Vec::new(),
    });
    TFunc::new(sigs.drop_name(id), 1, Ty::Void, vec![class], body)
}

struct JumpTarget {
    height: usize,
    resume: Vec<TStmt>,
}

struct Pass<'a> {
    sigs: &'a Signatures,
    locals: Vec<Ty>,
    scopes: Vec<Vec<LocalId>>,
    jumps: Vec<JumpTarget>,
    temps: Vec<LocalId>,
    created: Vec<StructId>,
}

enum Holder {
    Local(LocalId),
    Path(TExpr),
}

impl Pass<'_> {
    fn holder_ty(&self, holder: &Holder) -> Ty {
        match holder {
            Holder::Local(local) => self.locals[local.index()],
            Holder::Path(base) => base.ty(),
        }
    }

    fn holder_expr(&self, holder: &Holder) -> TExpr {
        match holder {
            Holder::Local(local) => TExpr::Local(*local, self.locals[local.index()]),
            Holder::Path(base) => base.clone(),
        }
    }

    fn store(&self, holder: &Holder, index: usize, value: TExpr) -> TStmt {
        match holder {
            Holder::Local(local) => TStmt::AssignField {
                local: *local,
                index,
                value,
            },
            Holder::Path(base) => TStmt::AssignPath {
                base: base.clone(),
                index,
                value,
            },
        }
    }

    fn needs_rc(&self, ty: Ty) -> bool {
        needs_rc(self.sigs, ty)
    }

    fn is_unowned_field(&self, base: Ty, index: usize) -> bool {
        let (Ty::Struct(id) | Ty::Class(id)) = base else {
            return false;
        };
        self.sigs
            .struct_sig(id)
            .and_then(|sig| sig.fields().get(index).map(|f| f.is_unowned()))
            .unwrap_or(false)
    }

    fn is_weak_field(&self, base: Ty, index: usize) -> bool {
        let (Ty::Struct(id) | Ty::Class(id)) = base else {
            return false;
        };
        self.sigs
            .struct_sig(id)
            .and_then(|sig| sig.fields().get(index).map(|f| f.is_weak()))
            .unwrap_or(false)
    }

    fn new_local(&mut self, ty: Ty) -> LocalId {
        let local = LocalId::new(self.locals.len());
        self.locals.push(ty);
        local
    }

    fn scoped<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (R, Vec<LocalId>) {
        let saved = std::mem::take(&mut self.temps);
        let result = f(self);
        let temps = std::mem::replace(&mut self.temps, saved);
        (result, temps)
    }

    fn release_local(&self, local: LocalId) -> [TStmt; 2] {
        let ty = self.locals[local.index()];
        [TStmt::Release(TExpr::Local(local, ty)), TStmt::Clear(local)]
    }

    fn release_all(&self, locals: impl Iterator<Item = LocalId>) -> Vec<TStmt> {
        locals.flat_map(|l| self.release_local(l)).collect()
    }

    fn release_temps(&self, temps: &[LocalId]) -> Vec<TStmt> {
        self.release_all(temps.iter().rev().copied())
    }

    fn jump_scope(
        &mut self,
        resume: Vec<TStmt>,
        body: impl FnOnce(&mut Self) -> Vec<TStmt>,
    ) -> Vec<TStmt> {
        self.jumps.push(JumpTarget {
            height: self.scopes.len(),
            resume,
        });
        let result = body(self);
        self.jumps.pop();
        result
    }

    fn jump(&self, depth: usize, with_resume: bool) -> Vec<TStmt> {
        let Some(target) = self
            .jumps
            .len()
            .checked_sub(depth + 1)
            .map(|i| &self.jumps[i])
        else {
            return Vec::new();
        };
        let leaving = self.scopes[target.height..].iter().flatten().rev().copied();
        let mut out = self.release_all(leaving);
        if with_resume {
            out.extend(target.resume.iter().cloned());
        }
        out
    }

    fn live_locals(&self) -> Vec<LocalId> {
        self.scopes.iter().flatten().copied().collect()
    }

    fn block(&mut self, stmts: &[TStmt]) -> Vec<TStmt> {
        self.scopes.push(Vec::new());
        let mut out = Vec::new();
        for stmt in stmts {
            self.stmt(stmt, &mut out);
        }
        let scope = self.scopes.pop().unwrap_or_default();
        if !matches!(
            out.last(),
            Some(TStmt::Return(_) | TStmt::Break(_) | TStmt::Continue(_) | TStmt::Fatal(_))
        ) {
            out.extend(self.release_all(scope.into_iter().rev()));
        }
        out
    }

    fn stmt(&mut self, stmt: &TStmt, out: &mut Vec<TStmt>) {
        match stmt {
            TStmt::Let { local, init } => self.let_stmt(*local, init, out),
            TStmt::Assign { local, value } => self.assign(*local, value, out),
            TStmt::AssignField {
                local,
                index,
                value,
            } => self.assign_field(&Holder::Local(*local), *index, value, out),
            TStmt::AssignPath { base, index, value } => {
                let (base, _) = self.scoped(|p| p.borrowed(base));
                self.assign_field(&Holder::Path(base), *index, value, out);
            }
            TStmt::Return(value) => self.return_stmt(value.as_ref(), out),
            TStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let (cond, temps) = self.scoped(|p| p.expr(cond));
                let cond = self.hold(cond, Ty::Bool, &temps, out);
                let then_body = self.block(then_body);
                let else_body = self.block(else_body);
                out.push(TStmt::If {
                    cond,
                    then_body,
                    else_body,
                });
            }
            TStmt::Break(depth) => {
                out.extend(self.jump(*depth, false));
                out.push(TStmt::Break(*depth));
            }
            TStmt::Continue(depth) => {
                out.extend(self.jump(*depth, true));
                out.push(TStmt::Continue(*depth));
            }
            TStmt::Block(body) => {
                let body = self.jump_scope(Vec::new(), |p| p.block(body));
                out.push(TStmt::Block(body));
            }
            TStmt::While { cond, body } => {
                let (cond, temps) = self.scoped(|p| p.expr(cond));
                if temps.is_empty() {
                    let body = self.jump_scope(Vec::new(), |p| p.block(body));
                    out.push(TStmt::While { cond, body });
                    return;
                }
                let flag = self.new_local(Ty::Bool);
                let mut resume = vec![TStmt::Assign {
                    local: flag,
                    value: cond.clone(),
                }];
                resume.extend(self.release_temps(&temps));
                let mut body = self.jump_scope(resume.clone(), |p| p.block(body));
                out.push(TStmt::Let {
                    local: flag,
                    init: cond,
                });
                out.extend(self.release_temps(&temps));
                body.extend(resume);
                out.push(TStmt::While {
                    cond: TExpr::Local(flag, Ty::Bool),
                    body,
                });
            }
            TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => {
                let ((lo, hi), temps) = self.scoped(|p| (p.expr(lo), p.expr(hi)));
                let (lo, hi) = if temps.is_empty() {
                    (lo, hi)
                } else {
                    let low = self.new_local(Ty::Int);
                    let high = self.new_local(Ty::Int);
                    out.push(TStmt::Let {
                        local: low,
                        init: lo,
                    });
                    out.push(TStmt::Let {
                        local: high,
                        init: hi,
                    });
                    out.extend(self.release_temps(&temps));
                    (TExpr::Local(low, Ty::Int), TExpr::Local(high, Ty::Int))
                };
                let body = self.jump_scope(Vec::new(), |p| p.block(body));
                out.push(TStmt::ForRange {
                    local: *local,
                    lo,
                    hi,
                    inclusive: *inclusive,
                    body,
                });
            }
            TStmt::Fatal(text) => out.push(TStmt::Fatal(text.clone())),
            TStmt::Print(expr) => {
                let (expr, temps) = self.scoped(|p| p.borrowed(expr));
                out.push(TStmt::Print(expr));
                out.extend(self.release_temps(&temps));
            }
            TStmt::Eval(expr) => {
                let (expr, temps) = self.scoped(|p| p.borrowed(expr));
                out.push(TStmt::Eval(expr));
                out.extend(self.release_temps(&temps));
            }
            TStmt::Release(expr) => {
                let (expr, temps) = self.scoped(|p| p.expr(expr));
                out.push(TStmt::Release(expr));
                out.extend(self.release_temps(&temps));
            }
            TStmt::ReleaseUnowned(expr) => {
                let (expr, temps) = self.scoped(|p| p.expr(expr));
                out.push(TStmt::ReleaseUnowned(expr));
                out.extend(self.release_temps(&temps));
            }
            TStmt::Clear(local) => out.push(TStmt::Clear(*local)),
            TStmt::Free(expr) => {
                let (value, temps) = self.scoped(|p| p.expr(expr));
                out.push(TStmt::Free(value));
                if let TExpr::Local(local, _) = expr {
                    out.push(TStmt::Clear(*local));
                }
                out.extend(self.release_temps(&temps));
            }
        }
    }

    fn hold(&mut self, value: TExpr, ty: Ty, temps: &[LocalId], out: &mut Vec<TStmt>) -> TExpr {
        if temps.is_empty() {
            return value;
        }
        let local = self.new_local(ty);
        out.push(TStmt::Let { local, init: value });
        out.extend(self.release_temps(temps));
        TExpr::Local(local, ty)
    }

    fn let_stmt(&mut self, local: LocalId, init: &TExpr, out: &mut Vec<TStmt>) {
        let ty = self.locals[local.index()];
        let counted = self.needs_rc(ty);
        let (init, temps) = self.scoped(|p| {
            if counted {
                p.owned_value(init)
            } else {
                p.expr(init)
            }
        });
        out.push(TStmt::Let { local, init });
        out.extend(self.release_temps(&temps));
        if counted && let Some(scope) = self.scopes.last_mut() {
            scope.push(local);
        }
    }

    fn assign(&mut self, local: LocalId, value: &TExpr, out: &mut Vec<TStmt>) {
        let ty = self.locals[local.index()];
        if !self.needs_rc(ty) {
            let (value, temps) = self.scoped(|p| p.expr(value));
            out.push(TStmt::Assign { local, value });
            out.extend(self.release_temps(&temps));
            return;
        }
        let (value, temps) = self.scoped(|p| p.owned_value(value));
        let fresh = self.new_local(ty);
        out.push(TStmt::Let {
            local: fresh,
            init: value,
        });
        out.extend(self.release_temps(&temps));
        out.push(TStmt::Release(TExpr::Local(local, ty)));
        out.push(TStmt::Assign {
            local,
            value: TExpr::Local(fresh, ty),
        });
    }

    fn assign_field(&mut self, holder: &Holder, index: usize, value: &TExpr, out: &mut Vec<TStmt>) {
        let base = self.holder_ty(holder);
        let field_ty = match base {
            Ty::Struct(id) | Ty::Class(id) => self
                .sigs
                .struct_sig(id)
                .and_then(|sig| sig.fields().get(index).map(|f| f.ty())),
            _ => None,
        }
        .unwrap_or(Ty::Int);
        if self.is_unowned_field(base, index) {
            self.assign_unowned_field(holder, index, value, out);
            return;
        }
        if self.is_weak_field(base, index) {
            self.assign_weak_field(holder, index, value, out);
            return;
        }
        if !self.needs_rc(field_ty) {
            let (value, temps) = self.scoped(|p| p.expr(value));
            out.push(self.store(holder, index, value));
            out.extend(self.release_temps(&temps));
            return;
        }
        let (value, temps) = self.scoped(|p| p.owned_value(value));
        let fresh = self.new_local(field_ty);
        out.push(TStmt::Let {
            local: fresh,
            init: value,
        });
        out.extend(self.release_temps(&temps));
        out.push(TStmt::Release(TExpr::Field {
            base: Box::new(self.holder_expr(holder)),
            index,
            ty: field_ty,
        }));
        out.push(self.store(holder, index, TExpr::Local(fresh, field_ty)));
    }

    fn assign_weak_field(
        &mut self,
        holder: &Holder,
        index: usize,
        value: &TExpr,
        out: &mut Vec<TStmt>,
    ) {
        let field_ty = value.ty();
        let target = self.sigs.unwrapped(field_ty).unwrap_or(Ty::Int);
        let moved = matches!(value, TExpr::Moved(_));
        let (value, temps) = self.scoped(|p| {
            let value = p.borrowed(value);
            if moved {
                value
            } else {
                TExpr::RetainWeak(Box::new(value))
            }
        });
        let fresh = self.new_local(field_ty);
        out.push(TStmt::Let {
            local: fresh,
            init: value,
        });
        out.extend(self.release_temps(&temps));
        out.push(TStmt::ReleaseUnowned(TExpr::Field {
            base: Box::new(TExpr::Field {
                base: Box::new(self.holder_expr(holder)),
                index,
                ty: field_ty,
            }),
            index: 1,
            ty: target,
        }));
        out.push(self.store(holder, index, TExpr::Local(fresh, field_ty)));
    }

    fn assign_unowned_field(
        &mut self,
        holder: &Holder,
        index: usize,
        value: &TExpr,
        out: &mut Vec<TStmt>,
    ) {
        let field_ty = value.ty();
        let moved = matches!(value, TExpr::Moved(_));
        let (value, temps) = self.scoped(|p| {
            let value = p.borrowed(value);
            if moved {
                value
            } else {
                TExpr::RetainUnowned(Box::new(value))
            }
        });
        let fresh = self.new_local(field_ty);
        out.push(TStmt::Let {
            local: fresh,
            init: value,
        });
        out.extend(self.release_temps(&temps));
        out.push(TStmt::ReleaseUnowned(TExpr::Field {
            base: Box::new(self.holder_expr(holder)),
            index,
            ty: field_ty,
        }));
        out.push(self.store(holder, index, TExpr::Local(fresh, field_ty)));
    }

    fn return_stmt(&mut self, value: Option<&TExpr>, out: &mut Vec<TStmt>) {
        let live = self.live_locals();
        let Some(value) = value else {
            out.extend(self.release_all(live.into_iter().rev()));
            out.push(TStmt::Return(None));
            return;
        };
        let ty = value.ty();
        let counted = self.needs_rc(ty);
        let (result, temps) = self.scoped(|p| {
            if counted {
                p.owned_value(value)
            } else {
                p.expr(value)
            }
        });
        if temps.is_empty() && live.is_empty() && !counted {
            out.push(TStmt::Return(Some(result)));
            return;
        }
        let held = self.new_local(ty);
        out.push(TStmt::Let {
            local: held,
            init: result,
        });
        out.extend(self.release_temps(&temps));
        out.extend(self.release_all(live.into_iter().rev()));
        out.push(TStmt::Return(Some(TExpr::Local(held, ty))));
    }

    fn is_owned(&self, expr: &TExpr) -> bool {
        if let TExpr::Field { base, index, .. } = expr {
            return self.is_unowned_field(base.ty(), *index)
                || self.is_weak_field(base.ty(), *index);
        }
        matches!(
            expr,
            TExpr::New { .. }
                | TExpr::Call { .. }
                | TExpr::Virtual { .. }
                | TExpr::VCall { .. }
                | TExpr::StructInit { .. }
                | TExpr::Zero(_)
                | TExpr::Retain(_)
                | TExpr::Moved(_)
                | TExpr::LoadUnowned(_)
                | TExpr::RetainUnowned(_)
                | TExpr::RetainWeak(_)
                | TExpr::LoadWeak(_)
                | TExpr::Select { .. }
        ) || matches!(expr, TExpr::StrOp { op, .. } if op.allocates())
    }

    fn owned_value(&mut self, expr: &TExpr) -> TExpr {
        let ty = expr.ty();
        let value = self.expr(expr);
        if self.needs_rc(ty) && !self.is_owned(expr) {
            TExpr::Retain(Box::new(value))
        } else {
            value
        }
    }

    fn borrowed(&mut self, expr: &TExpr) -> TExpr {
        let ty = expr.ty();
        let value = self.expr(expr);
        if self.needs_rc(ty) && self.is_owned(expr) {
            let local = self.new_local(ty);
            self.temps.push(local);
            TExpr::Bind {
                local,
                init: Box::new(value),
            }
        } else {
            value
        }
    }

    fn borrowed_all(&mut self, exprs: &[TExpr]) -> Vec<TExpr> {
        exprs.iter().map(|e| self.borrowed(e)).collect()
    }

    fn owned_all(&mut self, exprs: &[TExpr]) -> Vec<TExpr> {
        exprs.iter().map(|e| self.owned_value(e)).collect()
    }

    fn expr(&mut self, expr: &TExpr) -> TExpr {
        match expr {
            TExpr::Call {
                callee,
                type_args,
                args,
                ret,
            } => TExpr::Call {
                callee: callee.clone(),
                type_args: type_args.clone(),
                args: self.borrowed_all(args),
                ret: *ret,
            },
            TExpr::StrOp { op, args } => TExpr::StrOp {
                op: *op,
                args: self.borrowed_all(args),
            },
            TExpr::Dispatch {
                protocol,
                method,
                args,
                ret,
            } => TExpr::Dispatch {
                protocol: protocol.clone(),
                method: method.clone(),
                args: self.borrowed_all(args),
                ret: *ret,
            },
            TExpr::Witness {
                protocol,
                method,
                self_ty,
                args,
                ret,
            } => TExpr::Witness {
                protocol: protocol.clone(),
                method: method.clone(),
                self_ty: *self_ty,
                args: self.borrowed_all(args),
                ret: *ret,
            },
            TExpr::VCall { slot, args, ret } => TExpr::VCall {
                slot: *slot,
                args: self.borrowed_all(args),
                ret: *ret,
            },
            TExpr::InstanceOf { object, class } => TExpr::InstanceOf {
                object: Box::new(self.borrowed(object)),
                class: Box::new(self.expr(class)),
            },
            TExpr::ClassDesc(ty) => TExpr::ClassDesc(*ty),
            TExpr::Descriptor(descriptor) => {
                for link in descriptor.chain() {
                    if !self.created.contains(&link.class()) {
                        self.created.push(link.class());
                    }
                }
                TExpr::Descriptor(descriptor.clone())
            }
            TExpr::Virtual {
                cases,
                default,
                args,
                ret,
            } => TExpr::Virtual {
                cases: cases.clone(),
                default: default.clone(),
                args: self.borrowed_all(args),
                ret: *ret,
            },
            TExpr::StructInit { ty, args } => TExpr::StructInit {
                ty: *ty,
                args: self.owned_all(args),
            },
            TExpr::New { ty, args } => {
                let mut args: Vec<TExpr> = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        let moved = matches!(arg, TExpr::Moved(_));
                        if self.is_unowned_field(*ty, index) && !moved {
                            TExpr::RetainUnowned(Box::new(self.borrowed(arg)))
                        } else if self.is_weak_field(*ty, index) && !moved {
                            TExpr::RetainWeak(Box::new(self.borrowed(arg)))
                        } else {
                            self.owned_value(arg)
                        }
                    })
                    .collect();
                if let Ty::Class(id) = ty {
                    if !self.created.contains(id) {
                        self.created.push(*id);
                    }
                    let tag = i64::try_from(id.index()).unwrap_or(i64::MAX);
                    args[TAG_SLOT] = TExpr::Int(tag);
                }
                TExpr::New { ty: *ty, args }
            }
            TExpr::Field { base, index, ty } => {
                let field = TExpr::Field {
                    base: Box::new(self.borrowed(base)),
                    index: *index,
                    ty: *ty,
                };
                if self.is_unowned_field(base.ty(), *index) {
                    TExpr::LoadUnowned(Box::new(field))
                } else if self.is_weak_field(base.ty(), *index) {
                    TExpr::LoadWeak(Box::new(field))
                } else {
                    field
                }
            }
            TExpr::Unary { op, expr } => TExpr::Unary {
                op: *op,
                expr: Box::new(self.borrowed(expr)),
            },
            TExpr::Binary { op, lhs, rhs } => TExpr::Binary {
                op: *op,
                lhs: Box::new(self.borrowed(lhs)),
                rhs: Box::new(self.borrowed(rhs)),
            },
            TExpr::Select {
                cond,
                then,
                otherwise,
                ty,
            } => TExpr::Select {
                cond: Box::new(self.expr(cond)),
                then: Box::new(self.owned_value(then)),
                otherwise: Box::new(self.owned_value(otherwise)),
                ty: *ty,
            },
            TExpr::Unwrap { base, ty } => TExpr::Unwrap {
                base: Box::new(self.borrowed(base)),
                ty: *ty,
            },
            TExpr::Moved(inner) => TExpr::Moved(Box::new(match &**inner {
                TExpr::Field { base, index, ty } => TExpr::Field {
                    base: Box::new(self.borrowed(base)),
                    index: *index,
                    ty: *ty,
                },
                other => self.expr(other),
            })),
            TExpr::RetainUnowned(inner) => TExpr::RetainUnowned(Box::new(self.expr(inner))),
            TExpr::LoadUnowned(inner) => TExpr::LoadUnowned(Box::new(self.expr(inner))),
            TExpr::RetainWeak(inner) => TExpr::RetainWeak(Box::new(self.expr(inner))),
            TExpr::LoadWeak(inner) => TExpr::LoadWeak(Box::new(self.expr(inner))),
            TExpr::Retain(inner) => TExpr::Retain(Box::new(self.expr(inner))),
            TExpr::Bind { local, init } => {
                if self.needs_rc(init.ty()) && self.is_owned(init) {
                    self.temps.push(*local);
                }
                TExpr::Bind {
                    local: *local,
                    init: Box::new(self.expr(init)),
                }
            }
            TExpr::Int(_)
            | TExpr::Bool(_)
            | TExpr::Str(_)
            | TExpr::Local(..)
            | TExpr::EnumCase { .. }
            | TExpr::Zero(_)
            | TExpr::FuncAddr(_) => expr.clone(),
        }
    }
}

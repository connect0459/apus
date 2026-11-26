use std::collections::HashMap;

use crate::sigs::Signatures;
use crate::tir::{LocalHook, LocalId, TExpr, TFunc, TStmt};
use crate::types::Ty;

struct Cell {
    holder: LocalId,
    value: Ty,
    cell: Ty,
}

struct Environment {
    index: usize,
    value: Ty,
    cell: Ty,
}

struct Lowering {
    cells: HashMap<LocalId, Cell>,
    environment: HashMap<LocalId, Environment>,
    locals: Vec<Ty>,
}

impl Lowering {
    fn cell_address(&self, local: LocalId) -> Option<(TExpr, Ty)> {
        if let Some(cell) = self.cells.get(&local) {
            return Some((TExpr::Local(cell.holder, cell.cell), cell.value));
        }
        let env = self.environment.get(&local)?;
        Some((
            TExpr::EnvCell {
                env: Box::new(TExpr::Local(LocalId::new(0), Ty::Int)),
                index: env.index,
                ty: env.cell,
            },
            env.value,
        ))
    }
}

impl LocalHook for Lowering {
    fn expr(&mut self, expr: &TExpr) -> Option<TExpr> {
        match expr {
            TExpr::Local(local, ty) => {
                let (address, _) = self.cell_address(*local)?;
                Some(TExpr::Field {
                    base: Box::new(address),
                    index: 0,
                    ty: *ty,
                })
            }
            TExpr::Capture(local, _) => self.cell_address(*local).map(|(address, _)| address),
            _ => None,
        }
    }

    fn stmt(&mut self, stmt: &TStmt) -> Option<TStmt> {
        match stmt {
            TStmt::Let { local, init } => {
                let cell = self.cells.get(local)?;
                if cell.holder != *local {
                    return None;
                }
                Some(TStmt::Let {
                    local: *local,
                    init: TExpr::StructInit {
                        ty: cell.cell,
                        args: vec![init.clone()],
                    },
                })
            }
            TStmt::Assign { local, value } => {
                let (address, _) = self.cell_address(*local)?;
                Some(TStmt::AssignPath {
                    base: address,
                    index: 0,
                    value: value.clone(),
                })
            }
            TStmt::AssignField {
                local,
                index,
                value,
            } => {
                let (address, ty) = self.cell_address(*local)?;
                Some(TStmt::AssignPath {
                    base: TExpr::Field {
                        base: Box::new(address),
                        index: 0,
                        ty,
                    },
                    index: *index,
                    value: value.clone(),
                })
            }
            TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => {
                let cell = self.cells.get(local)?;
                let (cell_ty, value) = (cell.cell, cell.value);
                let fresh = LocalId::new(self.locals.len());
                self.locals.push(Ty::Int);
                let mut inner = vec![TStmt::Let {
                    local: *local,
                    init: TExpr::StructInit {
                        ty: cell_ty,
                        args: vec![TExpr::Local(fresh, value)],
                    },
                }];
                inner.extend(body.iter().cloned());
                Some(TStmt::ForRange {
                    local: fresh,
                    lo: lo.clone(),
                    hi: hi.clone(),
                    inclusive: *inclusive,
                    body: inner,
                })
            }
            _ => None,
        }
    }
}

pub fn lower(
    func: &TFunc,
    celled: &[LocalId],
    environment: &[(LocalId, usize)],
    sigs: &Signatures,
) -> TFunc {
    if celled.is_empty() && environment.is_empty() {
        return func.clone();
    }
    let mut locals = func.locals().to_vec();
    let mut cells = HashMap::new();
    let mut prologue = Vec::new();
    for local in celled {
        let value = locals[local.index()];
        let cell = sigs.cell_of(value);
        if local.index() < func.param_count() {
            let holder = LocalId::new(locals.len());
            locals.push(cell);
            prologue.push(TStmt::Let {
                local: holder,
                init: TExpr::StructInit {
                    ty: cell,
                    args: vec![TExpr::Local(*local, value)],
                },
            });
            cells.insert(
                *local,
                Cell {
                    holder,
                    value,
                    cell,
                },
            );
        } else {
            locals[local.index()] = cell;
            cells.insert(
                *local,
                Cell {
                    holder: *local,
                    value,
                    cell,
                },
            );
        }
    }
    let mut captured = HashMap::new();
    for (local, index) in environment {
        let value = locals[local.index()];
        captured.insert(
            *local,
            Environment {
                index: *index,
                value,
                cell: sigs.cell_of(value),
            },
        );
    }
    let mut lowering = Lowering {
        cells,
        environment: captured,
        locals: Vec::new(),
    };
    lowering.locals = locals;
    let snapshot = lowering.locals.clone();
    let result = func.rewrite_locals(&mut lowering, snapshot, prologue.clone());
    let final_locals = lowering.locals.clone();
    TFunc::new(
        result.name().to_string(),
        result.param_count(),
        result.ret(),
        final_locals,
        result.body().to_vec(),
    )
}

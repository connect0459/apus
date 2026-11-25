use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::BinOp;
use crate::check::{boxed_requirement, equality_function};
use crate::sigs::{COMPARABLE, Conformer, EQUATABLE, Signatures};
use crate::tir::{Descriptor, TFunc, Witnessed};
use crate::types::Ty;

const INSTANCE_LIMIT: usize = 2000;
const EQUAL: &str = "$equal";

fn note_dependency(direct: &RefCell<Vec<usize>>, id: usize) {
    let mut direct = direct.borrow_mut();
    if !direct.contains(&id) {
        direct.push(id);
    }
}

fn resolve_witness(
    sigs: &Signatures,
    protocol: &str,
    method: &str,
    self_ty: Ty,
) -> Result<Witnessed, String> {
    let name = sigs.type_name(self_ty);
    let missing = || format!("type '{name}' does not conform to the protocol '{protocol}'");
    if !sigs.conforms(self_ty, protocol) {
        return Err(missing());
    }
    match (protocol, method) {
        (EQUATABLE, "==") if matches!(self_ty, Ty::Int | Ty::Bool | Ty::Str | Ty::Enum(_)) => {
            Ok(Witnessed::Operator(BinOp::Eq))
        }
        (EQUATABLE, "==") => Ok(Witnessed::Call {
            callee: EQUAL.to_string(),
            type_args: vec![self_ty],
        }),
        (COMPARABLE, op) => {
            let op = match op {
                "<" => BinOp::Lt,
                ">" => BinOp::Gt,
                "<=" => BinOp::Le,
                _ => BinOp::Ge,
            };
            Ok(Witnessed::Operator(op))
        }
        _ => {
            let qualified = sigs
                .resolve_method(self_ty, method)
                .ok_or_else(|| format!("type '{name}' has no method '{method}'"))?;
            let slot = if self_ty.is_class() && sigs.is_overridden_below(self_ty, method) {
                sigs.vtable_slot(self_ty, method)
            } else {
                None
            };
            Ok(match slot {
                Some(slot) => Witnessed::Slot(slot),
                None => Witnessed::Call {
                    callee: qualified,
                    type_args: sigs.type_arguments(self_ty),
                },
            })
        }
    }
}

pub type GenericProvider<'a> = &'a mut dyn FnMut(&str) -> Result<Rc<TFunc>, String>;

pub struct Instantiator<'a> {
    sigs: &'a Signatures,
    provider: GenericProvider<'a>,
    ids: HashMap<String, usize>,
    funcs: Vec<TFunc>,
    deps: Vec<Vec<usize>>,
}

impl<'a> Instantiator<'a> {
    pub fn new(sigs: &'a Signatures, provider: GenericProvider<'a>) -> Self {
        Self {
            sigs,
            provider,
            ids: HashMap::new(),
            funcs: Vec::new(),
            deps: Vec::new(),
        }
    }

    pub fn instances(&self) -> &[TFunc] {
        &self.funcs
    }

    pub fn closure(&self, direct: &[usize]) -> Vec<usize> {
        let mut seen = vec![false; self.funcs.len()];
        let mut order = Vec::new();
        let mut stack: Vec<usize> = direct.to_vec();
        while let Some(id) = stack.pop() {
            if std::mem::replace(&mut seen[id], true) {
                continue;
            }
            order.push(id);
            stack.extend(self.deps[id].iter().copied());
        }
        order.sort_unstable();
        order
    }

    pub fn resolve(&mut self, func: &TFunc) -> Result<(TFunc, Vec<usize>), String> {
        let direct: RefCell<Vec<usize>> = RefCell::new(Vec::new());
        let sigs = self.sigs;
        let me = RefCell::new(&mut *self);
        let rewritten = {
            let mut ty = |t: Ty| Ok::<Ty, String>(t);
            let mut call = |callee: String, type_args: Vec<Ty>| {
                if type_args.is_empty() {
                    return Ok((callee, type_args));
                }
                let mut me = me.borrow_mut();
                let id = me.instance_for(&callee, &type_args)?;
                note_dependency(&direct, id);
                Ok((me.funcs[id].name().to_string(), Vec::new()))
            };
            let mut witness = |protocol: &str, method: &str, self_ty: Ty| {
                resolve_witness(sigs, protocol, method, self_ty)
            };
            let mut dispatch = |protocol: &str, method: &str| {
                me.borrow_mut()
                    .dispatch_cases(protocol, method, &direct)
                    .map(Some)
            };
            let mut describe = |class: Ty| me.borrow_mut().describe(class, &direct);
            func.rewrite(
                func.name().to_string(),
                &mut ty,
                &mut call,
                &mut witness,
                &mut dispatch,
                &mut describe,
            )?
        };
        Ok((rewritten, direct.into_inner()))
    }

    fn describe(
        &mut self,
        class: Ty,
        direct: &RefCell<Vec<usize>>,
    ) -> Result<Option<Box<Descriptor>>, String> {
        let sigs = self.sigs;
        let Ty::Class(id) = class else {
            return Ok(None);
        };
        if sigs.has_param(class) || sigs.existential_protocol(class).is_some() {
            return Ok(None);
        }
        let parent = match sigs.superclass(class) {
            Some(parent) => self.describe(parent, direct)?,
            None => None,
        };
        if let Some((qualified, arguments)) = sigs.deinit_of(class)
            && !arguments.is_empty()
        {
            let instance = self.instance_for(&qualified, &arguments)?;
            note_dependency(direct, instance);
        }
        let mut methods = Vec::new();
        for name in sigs.vtable_methods(class) {
            let (qualified, owner) = sigs
                .resolve_method_owner(class, &name)
                .ok_or_else(|| format!("'{}' has no method '{name}'", sigs.type_name(class)))?;
            let arguments = sigs.type_arguments(owner);
            if arguments.is_empty() {
                methods.push(qualified);
            } else {
                let instance = self.instance_for(&qualified, &arguments)?;
                note_dependency(direct, instance);
                methods.push(self.funcs[instance].name().to_string());
            }
        }
        Ok(Some(Box::new(Descriptor::new(
            sigs.descriptor_name(class),
            id,
            sigs.drop_name(id),
            parent,
            methods,
        ))))
    }

    fn dispatch_cases(
        &mut self,
        protocol: &str,
        method: &str,
        direct: &RefCell<Vec<usize>>,
    ) -> Result<Vec<(i64, String)>, String> {
        let sigs = self.sigs;
        let conformers = sigs
            .existential_conformers(protocol)
            .map_err(|e| e.message().to_string())?;
        if conformers.is_empty() {
            return Err(format!("no type conforms to the protocol '{protocol}'"));
        }
        let tag = |ty: Ty| match ty {
            Ty::Class(id) => i64::try_from(id.index()).unwrap_or(i64::MAX),
            _ => -1,
        };
        let mut cases = Vec::with_capacity(conformers.len());
        for conformer in conformers {
            match conformer {
                Conformer::Class(ty) => {
                    let callee = sigs.resolve_method(ty, method).ok_or_else(|| {
                        format!("'{}' has no method '{method}'", sigs.type_name(ty))
                    })?;
                    cases.push((tag(ty), callee));
                }
                Conformer::Boxed { value, boxed } => {
                    let id = self.boxed_instance(value, boxed, method)?;
                    note_dependency(direct, id);
                    cases.push((tag(boxed), self.funcs[id].name().to_string()));
                }
            }
        }
        Ok(cases)
    }

    fn boxed_instance(&mut self, value: Ty, boxed: Ty, method: &str) -> Result<usize, String> {
        let sigs = self.sigs;
        let name = format!("$box<{}>.{method}", sigs.type_name(value));
        if let Some(id) = self.ids.get(&name) {
            return Ok(*id);
        }
        if self.funcs.len() >= INSTANCE_LIMIT {
            return Err(format!(
                "too many generic instances while creating '{name}'"
            ));
        }
        let qualified = sigs
            .resolve_method(value, method)
            .ok_or_else(|| format!("'{}' has no method '{method}'", sigs.type_name(value)))?;
        let func = boxed_requirement(sigs, name.clone(), boxed, value, &qualified)
            .map_err(|e| e.message().to_string())?;
        let id = self.funcs.len();
        self.ids.insert(name, id);
        self.funcs.push(func.clone());
        self.deps.push(Vec::new());
        let (rewritten, direct) = self.resolve(&func)?;
        self.funcs[id] = rewritten;
        self.deps[id] = direct;
        Ok(id)
    }

    fn instance_for(&mut self, callee: &str, args: &[Ty]) -> Result<usize, String> {
        let mangled = self.sigs.mangled_call(callee, args);
        if let Some(id) = self.ids.get(&mangled) {
            return Ok(*id);
        }
        if self.funcs.len() >= INSTANCE_LIMIT {
            return Err(format!(
                "too many generic instances while instantiating '{mangled}'"
            ));
        }
        let sigs = self.sigs;
        let generic = if callee == EQUAL {
            Rc::new(
                args.first()
                    .map(|ty| equality_function(sigs, *ty, mangled.clone()))
                    .ok_or_else(|| "internal error: no type for equality".to_string())?
                    .map_err(|e| e.message().to_string())?,
            )
        } else {
            (self.provider)(callee)?
        };
        let mut ty = |t: Ty| {
            sigs.subst(t, args, &[], 0)
                .map_err(|e| e.message().to_string())
        };
        let mut keep = |c: String, a: Vec<Ty>| Ok::<_, String>((c, a));
        let mut keep_witness = |_: &str, _: &str, _: Ty| Ok::<_, String>(Witnessed::Keep);
        let mut keep_dispatch = |_: &str, _: &str| Ok::<_, String>(None);
        let mut keep_describe = |_: Ty| Ok::<_, String>(None);
        let substituted = generic.rewrite(
            mangled.clone(),
            &mut ty,
            &mut keep,
            &mut keep_witness,
            &mut keep_dispatch,
            &mut keep_describe,
        )?;
        let id = self.funcs.len();
        self.ids.insert(mangled, id);
        self.funcs.push(substituted.clone());
        self.deps.push(Vec::new());
        let (rewritten, direct) = self.resolve(&substituted)?;
        self.funcs[id] = rewritten;
        self.deps[id] = direct;
        Ok(id)
    }
}

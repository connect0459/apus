use std::collections::HashMap;
use std::rc::Rc;

use crate::sigs::Signatures;
use crate::tir::TFunc;
use crate::types::Ty;

const INSTANCE_LIMIT: usize = 2000;

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
        let mut direct: Vec<usize> = Vec::new();
        let rewritten = {
            let mut ty = |t: Ty| Ok::<Ty, String>(t);
            let mut call = |callee: String, type_args: Vec<Ty>| {
                if type_args.is_empty() {
                    return Ok((callee, type_args));
                }
                let id = self.instance_for(&callee, &type_args)?;
                if !direct.contains(&id) {
                    direct.push(id);
                }
                Ok((self.funcs[id].name().to_string(), Vec::new()))
            };
            func.rewrite(func.name().to_string(), &mut ty, &mut call)?
        };
        Ok((rewritten, direct))
    }

    fn instance_for(&mut self, callee: &str, args: &[Ty]) -> Result<usize, String> {
        let names: Vec<String> = args.iter().map(|a| self.sigs.type_name(*a)).collect();
        let mangled = format!("{callee}<{}>", names.join(","));
        if let Some(id) = self.ids.get(&mangled) {
            return Ok(*id);
        }
        if self.funcs.len() >= INSTANCE_LIMIT {
            return Err(format!(
                "too many generic instances while instantiating '{mangled}'"
            ));
        }
        let generic = (self.provider)(callee)?;
        let sigs = self.sigs;
        let mut ty = |t: Ty| {
            sigs.subst(t, args, &[], 0)
                .map_err(|e| e.message().to_string())
        };
        let mut keep = |c: String, a: Vec<Ty>| Ok::<_, String>((c, a));
        let substituted = generic.rewrite(mangled.clone(), &mut ty, &mut keep)?;
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

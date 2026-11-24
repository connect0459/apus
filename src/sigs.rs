use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{Decl, EnumDecl, FuncDecl, StructDecl, TypeRef};
use crate::tir::TStruct;
use crate::types::{EnumId, StructId, Ty};

pub const PRINT: &str = "print";

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    line: u32,
    message: String,
}

impl CheckError {
    pub(crate) fn at(line: u32, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

fn generic_owner(line: u32, owner: &str) -> CheckError {
    CheckError::at(
        line,
        format!("methods of the generic type '{owner}' are not supported yet"),
    )
}

pub type CResult<T> = Result<T, CheckError>;

pub type Scope = [(String, Ty)];

pub fn scope_of(generics: &[String]) -> Vec<(String, Ty)> {
    generics
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), Ty::Param(index)))
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamSig {
    label: Option<String>,
    ty: Ty,
}

impl ParamSig {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn ty(&self) -> Ty {
        self.ty
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncSig {
    generics: Vec<String>,
    params: Vec<ParamSig>,
    ret: Ty,
    receiver: bool,
    mutating: bool,
}

impl FuncSig {
    pub fn has_receiver(&self) -> bool {
        self.receiver
    }

    pub fn is_mutating(&self) -> bool {
        self.mutating
    }

    pub fn generics(&self) -> &[String] {
        &self.generics
    }

    pub fn params(&self) -> &[ParamSig] {
        &self.params
    }

    pub fn ret(&self) -> Ty {
        self.ret
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldSig {
    name: String,
    ty: Ty,
    mutable: bool,
}

impl FieldSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> Ty {
        self.ty
    }

    pub fn is_mutable(&self) -> bool {
        self.mutable
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructSig {
    name: String,
    fields: Vec<FieldSig>,
}

impl StructSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[FieldSig] {
        &self.fields
    }

    pub fn field(&self, name: &str) -> Option<(usize, &FieldSig)> {
        self.fields.iter().enumerate().find(|(_, f)| f.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseSig {
    name: String,
    payload: Vec<Ty>,
}

impl CaseSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn payload(&self) -> &[Ty] {
        &self.payload
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumSig {
    name: String,
    base_name: String,
    cases: Vec<CaseSig>,
    has_payload: bool,
    layout: Option<StructId>,
}

impl EnumSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn base_name(&self) -> &str {
        &self.base_name
    }

    pub fn cases(&self) -> &[CaseSig] {
        &self.cases
    }

    pub fn has_payload(&self) -> bool {
        self.has_payload
    }

    pub fn case_index(&self, name: &str) -> Option<usize> {
        self.cases.iter().position(|c| c.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Origin {
    Struct { def: usize, args: Vec<Ty> },
    EnumLayout(EnumId),
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Instances {
    structs: Vec<Rc<StructSig>>,
    struct_origin: Vec<Origin>,
    struct_index: HashMap<(usize, Vec<Ty>), StructId>,
    struct_pending: HashSet<(usize, Vec<Ty>)>,
    enums: Vec<Rc<EnumSig>>,
    enum_origin: Vec<(usize, Vec<Ty>)>,
    enum_index: HashMap<(usize, Vec<Ty>), EnumId>,
    enum_pending: HashSet<(usize, Vec<Ty>)>,
    layout_owner: HashMap<StructId, EnumId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signatures {
    funcs: HashMap<String, FuncSig>,
    struct_defs: Vec<StructDecl>,
    struct_def_ids: HashMap<String, usize>,
    enum_defs: Vec<EnumDecl>,
    enum_def_ids: HashMap<String, usize>,
    instances: RefCell<Instances>,
}

impl Signatures {
    pub fn collect(decls: &[Decl]) -> CResult<Self> {
        let mut sigs = Self {
            funcs: HashMap::new(),
            struct_defs: Vec::new(),
            struct_def_ids: HashMap::new(),
            enum_defs: Vec::new(),
            enum_def_ids: HashMap::new(),
            instances: RefCell::new(Instances::default()),
        };
        for decl in decls {
            match decl {
                Decl::Struct(s) => sigs.register_struct_def(s)?,
                Decl::Enum(e) => sigs.register_enum_def(e)?,
                Decl::Func(_) => {}
            }
        }
        for (def, decl) in sigs.enum_defs.clone().iter().enumerate() {
            if decl.generics().is_empty() {
                sigs.instantiate_enum(def, Vec::new(), &[], decl.line())?;
            }
        }
        for (def, decl) in sigs.struct_defs.clone().iter().enumerate() {
            if decl.generics().is_empty() {
                sigs.instantiate_struct(def, Vec::new(), &[], decl.line())?;
            }
        }
        for decl in decls {
            if let Decl::Func(func) = decl {
                sigs.register_func(func)?;
            }
        }
        Ok(sigs)
    }

    fn register_struct_def(&mut self, decl: &StructDecl) -> CResult<()> {
        if self.struct_def_ids.contains_key(decl.name())
            || self.enum_def_ids.contains_key(decl.name())
        {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate type '{}'", decl.name()),
            ));
        }
        self.struct_def_ids
            .insert(decl.name().to_string(), self.struct_defs.len());
        self.struct_defs.push(decl.clone());
        Ok(())
    }

    fn register_enum_def(&mut self, decl: &EnumDecl) -> CResult<()> {
        if self.struct_def_ids.contains_key(decl.name())
            || self.enum_def_ids.contains_key(decl.name())
        {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate type '{}'", decl.name()),
            ));
        }
        self.enum_def_ids
            .insert(decl.name().to_string(), self.enum_defs.len());
        self.enum_defs.push(decl.clone());
        Ok(())
    }

    fn register_func(&mut self, func: &FuncDecl) -> CResult<()> {
        if func.name() == PRINT {
            return Err(CheckError::at(
                func.line(),
                "'print' is a reserved function name",
            ));
        }
        if self.struct_def_ids.contains_key(func.name())
            || self.enum_def_ids.contains_key(func.name())
        {
            return Err(CheckError::at(
                func.line(),
                format!("'{}' is already the name of a type", func.name()),
            ));
        }
        let scope = scope_of(func.generics());
        let owner_ty = match func.owner() {
            Some(owner) => Some(self.member_owner(func, owner)?),
            None if func.is_mutating() || func.is_init() => {
                return Err(CheckError::at(
                    func.line(),
                    "'mutating' and 'init' belong inside a type",
                ));
            }
            None => None,
        };
        let receiver = owner_ty.filter(|_| !func.is_init());
        let mut params: Vec<ParamSig> = receiver
            .map(|ty| ParamSig { label: None, ty })
            .into_iter()
            .collect();
        for p in func.params() {
            params.push(ParamSig {
                label: p.label().map(str::to_string),
                ty: self.resolve_type(p.ty(), &scope, func.line())?,
            });
        }
        let ret = match (func.ret(), owner_ty) {
            (_, Some(ty)) if func.is_init() => ty,
            (Some(ty), _) => self.resolve_type(ty, &scope, func.line())?,
            (None, _) => Ty::Void,
        };
        let sig = FuncSig {
            generics: func.generics().to_vec(),
            params,
            ret,
            receiver: receiver.is_some(),
            mutating: func.is_mutating(),
        };
        if self.funcs.insert(func.name().to_string(), sig).is_some() {
            return Err(CheckError::at(
                func.line(),
                format!("duplicate function '{}'", func.name()),
            ));
        }
        Ok(())
    }

    fn member_owner(&self, func: &FuncDecl, owner: &str) -> CResult<Ty> {
        let line = func.line();
        if !func.generics().is_empty() {
            return Err(CheckError::at(
                line,
                format!("generic methods are not supported yet: '{}'", func.name()),
            ));
        }
        let ty = if let Some(def) = self.struct_def_ids.get(owner).copied() {
            if !self.struct_defs[def].generics().is_empty() {
                return Err(generic_owner(line, owner));
            }
            let id = self.instantiate_struct(def, Vec::new(), &[], line)?;
            self.struct_ty(def, id)
        } else if let Some(def) = self.enum_def_ids.get(owner).copied() {
            if !self.enum_defs[def].generics().is_empty() {
                return Err(generic_owner(line, owner));
            }
            let id = self.instantiate_enum(def, Vec::new(), &[], line)?;
            self.enum_ty(id)
        } else {
            return Err(CheckError::at(line, format!("unknown type '{owner}'")));
        };
        if func.is_mutating() && ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("a method of the class '{owner}' cannot be mutating"),
            ));
        }
        if func.is_mutating() && matches!(ty, Ty::Enum(_)) {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}' cannot be mutating: only an enum with payloads can have mutating methods yet",
                    func.name()
                ),
            ));
        }
        if func.is_init() && (self.is_enum_ty(ty)) {
            return Err(CheckError::at(
                line,
                format!("the enum '{owner}' cannot have an initializer yet"),
            ));
        }
        Ok(ty)
    }

    pub fn owner_name(&self, ty: Ty) -> Option<String> {
        if let Some(sig) = self.enum_of(ty) {
            return Some(sig.base_name().to_string());
        }
        match ty {
            Ty::Struct(id) | Ty::Class(id) => self.struct_sig(id).map(|s| s.name.clone()),
            _ => None,
        }
    }

    pub fn resolve_type(&self, tr: &TypeRef, scope: &Scope, line: u32) -> CResult<Ty> {
        let name = tr.name();
        let args_given = !tr.args().is_empty();
        if let Some((_, ty)) = scope.iter().find(|(n, _)| n == name) {
            return self.reject_args(args_given, name, line).map(|()| *ty);
        }
        if let Some(ty) = Ty::from_builtin_name(name) {
            return self.reject_args(args_given, name, line).map(|()| ty);
        }
        if let Some(def) = self.struct_def_ids.get(name).copied() {
            let args =
                self.resolve_args(tr, self.struct_defs[def].generics().len(), scope, line)?;
            let id = self.instantiate_struct(def, args, scope, line)?;
            return Ok(self.struct_ty(def, id));
        }
        if let Some(def) = self.enum_def_ids.get(name).copied() {
            let args = self.resolve_args(tr, self.enum_defs[def].generics().len(), scope, line)?;
            let id = self.instantiate_enum(def, args, scope, line)?;
            return Ok(self.enum_ty(id));
        }
        Err(CheckError::at(line, format!("unknown type '{name}'")))
    }

    fn reject_args(&self, args_given: bool, name: &str, line: u32) -> CResult<()> {
        if args_given {
            Err(CheckError::at(
                line,
                format!("'{name}' does not take type arguments"),
            ))
        } else {
            Ok(())
        }
    }

    fn resolve_args(
        &self,
        tr: &TypeRef,
        expected: usize,
        scope: &Scope,
        line: u32,
    ) -> CResult<Vec<Ty>> {
        if tr.args().len() != expected {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}' expects {expected} type argument(s), but {} were given",
                    tr.name(),
                    tr.args().len()
                ),
            ));
        }
        tr.args()
            .iter()
            .map(|arg| self.resolve_type(arg, scope, line))
            .collect()
    }

    pub fn instance_name(&self, base: &str, args: &[Ty], scope: &Scope) -> String {
        if args.is_empty() {
            return base.to_string();
        }
        let names: Vec<String> = args.iter().map(|a| self.display(*a, scope)).collect();
        format!("{base}<{}>", names.join(","))
    }

    pub fn instantiate_struct(
        &self,
        def: usize,
        args: Vec<Ty>,
        scope: &Scope,
        line: u32,
    ) -> CResult<StructId> {
        let key = (def, args);
        if let Some(id) = self.instances.borrow().struct_index.get(&key) {
            return Ok(*id);
        }
        let decl = &self.struct_defs[def];
        let name = self.instance_name(decl.name(), &key.1, scope);
        if !self
            .instances
            .borrow_mut()
            .struct_pending
            .insert(key.clone())
        {
            return Err(CheckError::at(
                line,
                format!("a struct cannot contain itself: '{name}'"),
            ));
        }
        let fields = self.struct_fields(decl, &key.1, &name, scope, line);
        self.instances.borrow_mut().struct_pending.remove(&key);
        let fields = fields?;
        let mut instances = self.instances.borrow_mut();
        let id = StructId::new(instances.structs.len());
        instances.structs.push(Rc::new(StructSig { name, fields }));
        instances.struct_origin.push(Origin::Struct {
            def,
            args: key.1.clone(),
        });
        instances.struct_index.insert(key, id);
        Ok(id)
    }

    fn struct_fields(
        &self,
        decl: &StructDecl,
        args: &[Ty],
        name: &str,
        scope: &Scope,
        line: u32,
    ) -> CResult<Vec<FieldSig>> {
        let own_scope: Vec<(String, Ty)> = decl
            .generics()
            .iter()
            .cloned()
            .zip(args.iter().copied())
            .collect();
        let at = if line == 0 { decl.line() } else { line };
        let mut fields: Vec<FieldSig> = Vec::new();
        for field in decl.fields() {
            let ty = self.resolve_type(field.ty(), &own_scope, at)?;
            if let Ty::Struct(_) = ty {
                return Err(CheckError::at(
                    at,
                    format!(
                        "a struct field cannot have a struct type yet: '{}' in '{name}'",
                        self.display(ty, scope)
                    ),
                ));
            }
            if fields.iter().any(|f| f.name == field.name()) {
                return Err(CheckError::at(
                    at,
                    format!("duplicate field '{}' in '{name}'", field.name()),
                ));
            }
            fields.push(FieldSig {
                name: field.name().to_string(),
                ty,
                mutable: field.is_mutable(),
            });
        }
        Ok(fields)
    }

    pub fn instantiate_enum(
        &self,
        def: usize,
        args: Vec<Ty>,
        scope: &Scope,
        line: u32,
    ) -> CResult<EnumId> {
        let key = (def, args);
        if let Some(id) = self.instances.borrow().enum_index.get(&key) {
            return Ok(*id);
        }
        let decl = &self.enum_defs[def];
        let name = self.instance_name(decl.name(), &key.1, scope);
        if !self.instances.borrow_mut().enum_pending.insert(key.clone()) {
            return Err(CheckError::at(
                line,
                format!("an enum cannot contain itself: '{name}'"),
            ));
        }
        let cases = self.enum_cases(decl, &key.1, &name, scope, line);
        self.instances.borrow_mut().enum_pending.remove(&key);
        let cases = cases?;
        let has_payload = decl.has_payload();
        let mut instances = self.instances.borrow_mut();
        let id = EnumId::new(instances.enums.len());
        let layout = if has_payload {
            let widest = cases.iter().map(|c| c.payload.len()).max().unwrap_or(0);
            let mut fields = vec![FieldSig {
                name: "tag".to_string(),
                ty: Ty::Int,
                mutable: false,
            }];
            for slot in 0..widest {
                fields.push(FieldSig {
                    name: format!("payload{slot}"),
                    ty: Ty::Int,
                    mutable: false,
                });
            }
            let layout = StructId::new(instances.structs.len());
            instances.structs.push(Rc::new(StructSig {
                name: name.clone(),
                fields,
            }));
            instances.struct_origin.push(Origin::EnumLayout(id));
            instances.layout_owner.insert(layout, id);
            Some(layout)
        } else {
            None
        };
        instances.enums.push(Rc::new(EnumSig {
            name,
            base_name: decl.name().to_string(),
            cases,
            has_payload,
            layout,
        }));
        instances.enum_origin.push((def, key.1.clone()));
        instances.enum_index.insert(key, id);
        Ok(id)
    }

    fn enum_cases(
        &self,
        decl: &EnumDecl,
        args: &[Ty],
        name: &str,
        scope: &Scope,
        line: u32,
    ) -> CResult<Vec<CaseSig>> {
        let own_scope: Vec<(String, Ty)> = decl
            .generics()
            .iter()
            .cloned()
            .zip(args.iter().copied())
            .collect();
        let at = if line == 0 { decl.line() } else { line };
        let mut cases: Vec<CaseSig> = Vec::new();
        for case in decl.cases() {
            if cases.iter().any(|c| c.name == case.name()) {
                return Err(CheckError::at(
                    at,
                    format!("duplicate case '{}' in '{name}'", case.name()),
                ));
            }
            let mut payload = Vec::new();
            for tr in case.payload() {
                let ty = self.resolve_type(tr, &own_scope, at)?;
                if let Ty::Struct(_) = ty {
                    return Err(CheckError::at(
                        at,
                        format!(
                            "a payload cannot have a struct type yet: '{}' in '{name}'",
                            self.display(ty, scope)
                        ),
                    ));
                }
                payload.push(ty);
            }
            cases.push(CaseSig {
                name: case.name().to_string(),
                payload,
            });
        }
        Ok(cases)
    }

    pub fn struct_ty(&self, def: usize, id: StructId) -> Ty {
        if self.struct_defs[def].is_class() {
            Ty::Class(id)
        } else {
            Ty::Struct(id)
        }
    }

    pub fn enum_ty(&self, id: EnumId) -> Ty {
        match self.instances.borrow().enums[id.index()].layout {
            Some(layout) => Ty::Struct(layout),
            None => Ty::Enum(id),
        }
    }

    pub fn enum_of(&self, ty: Ty) -> Option<Rc<EnumSig>> {
        let instances = self.instances.borrow();
        match ty {
            Ty::Enum(id) => instances.enums.get(id.index()).cloned(),
            Ty::Struct(layout) => {
                let id = instances.layout_owner.get(&layout)?;
                instances.enums.get(id.index()).cloned()
            }
            _ => None,
        }
    }

    pub fn struct_sig(&self, id: StructId) -> Option<Rc<StructSig>> {
        self.instances.borrow().structs.get(id.index()).cloned()
    }

    pub fn enum_sig(&self, id: EnumId) -> Option<Rc<EnumSig>> {
        self.instances.borrow().enums.get(id.index()).cloned()
    }

    pub fn func(&self, name: &str) -> Option<&FuncSig> {
        self.funcs.get(name)
    }

    pub fn struct_def(&self, name: &str) -> Option<usize> {
        self.struct_def_ids.get(name).copied()
    }

    pub fn enum_def(&self, name: &str) -> Option<usize> {
        self.enum_def_ids.get(name).copied()
    }

    pub fn struct_sig_of_def(&self, def: usize) -> Option<String> {
        self.struct_defs.get(def).map(|d| d.name().to_string())
    }

    pub fn struct_generics(&self, def: usize) -> &[String] {
        self.struct_defs[def].generics()
    }

    pub fn enum_generics(&self, def: usize) -> &[String] {
        self.enum_defs[def].generics()
    }

    pub fn is_enum_ty(&self, ty: Ty) -> bool {
        self.enum_of(ty).is_some()
    }

    pub fn type_name(&self, ty: Ty) -> String {
        self.display(ty, &[])
    }

    pub fn display(&self, ty: Ty, scope: &Scope) -> String {
        match ty {
            Ty::Int => "Int".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Str => "String".to_string(),
            Ty::Void => "Void".to_string(),
            Ty::Param(index) => scope
                .iter()
                .find(|(_, t)| *t == Ty::Param(index))
                .map_or_else(|| format!("T{index}"), |(name, _)| name.clone()),
            Ty::Struct(id) | Ty::Class(id) => self
                .struct_sig(id)
                .map_or_else(String::new, |s| s.name.clone()),
            Ty::Enum(id) => self
                .enum_sig(id)
                .map_or_else(String::new, |e| e.name.clone()),
        }
    }

    fn origin_of(&self, ty: Ty) -> Option<(Kind, usize, Vec<Ty>)> {
        let instances = self.instances.borrow();
        match ty {
            Ty::Struct(id) | Ty::Class(id) => match instances.struct_origin.get(id.index())? {
                Origin::Struct { def, args } => Some((Kind::Struct, *def, args.clone())),
                Origin::EnumLayout(owner) => {
                    let (def, args) = instances.enum_origin.get(owner.index())?;
                    Some((Kind::Enum, *def, args.clone()))
                }
            },
            Ty::Enum(id) => {
                let (def, args) = instances.enum_origin.get(id.index())?;
                Some((Kind::Enum, *def, args.clone()))
            }
            _ => None,
        }
    }

    pub fn has_param(&self, ty: Ty) -> bool {
        match ty {
            Ty::Param(_) => true,
            _ => self
                .origin_of(ty)
                .is_some_and(|(_, _, args)| args.iter().any(|a| self.has_param(*a))),
        }
    }

    pub fn unify(&self, pattern: Ty, actual: Ty, bound: &mut [Option<Ty>]) -> bool {
        if let Ty::Param(index) = pattern {
            return match bound.get_mut(index) {
                Some(slot @ None) => {
                    *slot = Some(actual);
                    true
                }
                Some(Some(existing)) => *existing == actual,
                None => false,
            };
        }
        match (self.origin_of(pattern), self.origin_of(actual)) {
            (Some((pk, pd, pargs)), Some((ak, ad, aargs))) if pk == ak && pd == ad => pargs
                .iter()
                .zip(aargs.iter())
                .all(|(p, a)| self.unify(*p, *a, bound)),
            _ => pattern == actual,
        }
    }

    pub fn subst(&self, ty: Ty, args: &[Ty], scope: &Scope, line: u32) -> CResult<Ty> {
        if let Ty::Param(index) = ty {
            return args
                .get(index)
                .copied()
                .ok_or_else(|| CheckError::at(line, "internal error: unbound type parameter"));
        }
        let Some((kind, def, inner)) = self.origin_of(ty) else {
            return Ok(ty);
        };
        if !inner.iter().any(|a| self.has_param(*a)) {
            return Ok(ty);
        }
        let substituted = inner
            .iter()
            .map(|a| self.subst(*a, args, scope, line))
            .collect::<CResult<Vec<_>>>()?;
        match kind {
            Kind::Struct => {
                let id = self.instantiate_struct(def, substituted, scope, line)?;
                Ok(self.struct_ty(def, id))
            }
            Kind::Enum => {
                let id = self.instantiate_enum(def, substituted, scope, line)?;
                Ok(self.enum_ty(id))
            }
        }
    }

    pub fn layouts(&self) -> Vec<TStruct> {
        self.instances
            .borrow()
            .structs
            .iter()
            .map(|s| TStruct::new(s.name.clone(), s.fields.iter().map(|f| f.ty).collect()))
            .collect()
    }
}

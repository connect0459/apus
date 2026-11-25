use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{Decl, EnumCaseDecl, EnumDecl, FuncDecl, StructDecl, TypeRef};
use crate::tir::{TExpr, TStruct};
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

fn identity(generics: &[String]) -> Vec<Ty> {
    (0..generics.len()).map(Ty::Param).collect()
}

const HEADER_FIELDS: [&str; 4] = ["$class", "$rc", "$drop", "$unowned"];

pub const OPTIONAL: &str = "Optional";
pub const NONE_CASE: usize = 0;
pub const SOME_CASE: usize = 1;

fn optional_decl() -> EnumDecl {
    EnumDecl::new(
        0,
        OPTIONAL.to_string(),
        vec![
            EnumCaseDecl::new("none".to_string(), Vec::new()),
            EnumCaseDecl::new("some".to_string(), vec![TypeRef::from("Wrapped")]),
        ],
    )
    .with_generics(vec!["Wrapped".to_string()])
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
    unowned: bool,
    weak: bool,
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

    pub fn is_unowned(&self) -> bool {
        self.unowned
    }

    pub fn is_weak(&self) -> bool {
        self.weak
    }

    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('$')
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

    pub fn members(&self) -> impl Iterator<Item = (usize, &FieldSig)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.is_hidden())
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
    EnumCell(EnumId),
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
    superclass: HashMap<StructId, StructId>,
    cell_owner: HashMap<StructId, EnumId>,
    cell_index: HashMap<EnumId, StructId>,
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
        sigs.register_enum_def(&optional_decl())?;
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
        for decl in decls {
            if let Decl::Func(func) = decl {
                sigs.check_override(func)?;
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
        if decl.superclass().is_some() && !decl.generics().is_empty() {
            return Err(CheckError::at(
                decl.line(),
                "generic classes cannot take part in inheritance yet",
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
        let owner = match func.owner() {
            Some(owner) => Some(self.member_owner(func, owner)?),
            None if func.is_mutating() || func.is_init() => {
                return Err(CheckError::at(
                    func.line(),
                    "'mutating' and 'init' belong inside a type",
                ));
            }
            None => None,
        };
        let owner_ty = owner.as_ref().map(|(ty, _)| *ty);
        let generics = match &owner {
            Some((_, generics)) => {
                if func.is_init() && !func.generics().is_empty() {
                    return Err(CheckError::at(
                        func.line(),
                        "an initializer cannot declare type parameters",
                    ));
                }
                if let Some(name) = func.generics().iter().find(|g| generics.contains(g)) {
                    return Err(CheckError::at(
                        func.line(),
                        format!("'{name}' is already a type parameter of the enclosing type"),
                    ));
                }
                generics.iter().chain(func.generics()).cloned().collect()
            }
            None => func.generics().to_vec(),
        };
        let scope = scope_of(&generics);
        let receiver = owner_ty.filter(|_| !func.is_init());
        let receiver_param = receiver.map(|ty| match ty {
            Ty::Enum(id) if func.is_mutating() => self.enum_cell(id),
            other => other,
        });
        let mut params: Vec<ParamSig> = receiver_param
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
            generics,
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

    fn check_override(&self, func: &FuncDecl) -> CResult<()> {
        let Some(owner) = func
            .owner()
            .filter(|_| !func.is_init() && !func.is_deinit())
        else {
            return Ok(());
        };
        let (ty, _) = self.member_owner(func, owner)?;
        let name = func.name().strip_prefix(owner).map_or("", |n| &n[1..]);
        let inherited = self
            .superclass(ty)
            .and_then(|parent| self.resolve_method(parent, name));
        let line = func.line();
        match (func.is_override(), inherited) {
            (true, None) => Err(CheckError::at(
                line,
                format!("'{name}' is marked 'override', but no superclass of '{owner}' has it"),
            )),
            (false, Some(_)) => Err(CheckError::at(
                line,
                format!("'{name}' overrides an inherited method: write 'override'"),
            )),
            (true, Some(inherited)) => {
                let (Some(mine), Some(theirs)) =
                    (self.funcs.get(func.name()), self.funcs.get(&inherited))
                else {
                    return Ok(());
                };
                let same = mine.ret == theirs.ret
                    && mine.params.len() == theirs.params.len()
                    && mine
                        .params
                        .iter()
                        .zip(&theirs.params)
                        .skip(1)
                        .all(|(a, b)| a.ty == b.ty && a.label == b.label);
                if same {
                    Ok(())
                } else {
                    Err(CheckError::at(
                        line,
                        format!("'{name}' must keep the signature of '{inherited}' to override it"),
                    ))
                }
            }
            (false, None) => Ok(()),
        }
    }

    pub fn inherited_init_owner(&self, ty: Ty) -> Option<String> {
        let mut current = self.superclass(ty);
        while let Some(t) = current {
            let owner = self.owner_name(t)?;
            if self.funcs.contains_key(&format!("{owner}.init")) {
                return Some(owner);
            }
            current = self.superclass(t);
        }
        None
    }

    pub fn resolve_method(&self, ty: Ty, name: &str) -> Option<String> {
        let mut current = Some(ty);
        while let Some(t) = current {
            let qualified = format!("{}.{name}", self.owner_name(t)?);
            if self
                .funcs
                .get(&qualified)
                .is_some_and(FuncSig::has_receiver)
            {
                return Some(qualified);
            }
            current = self.superclass(t);
        }
        None
    }

    pub fn dispatch_cases(&self, ty: Ty, name: &str) -> Vec<(i64, String)> {
        let Some(default) = self.resolve_method(ty, name) else {
            return Vec::new();
        };
        let mut ids: Vec<StructId> = self.instances.borrow().superclass.keys().copied().collect();
        ids.sort_by_key(|id| id.index());
        ids.into_iter()
            .map(Ty::Class)
            .filter(|class| *class != ty && self.is_subclass(*class, ty))
            .filter_map(|class| {
                let Ty::Class(id) = class else { return None };
                let target = self.resolve_method(class, name)?;
                (target != default).then(|| (i64::try_from(id.index()).unwrap_or(i64::MAX), target))
            })
            .collect()
    }

    fn member_owner(&self, func: &FuncDecl, owner: &str) -> CResult<(Ty, Vec<String>)> {
        let line = func.line();
        let (ty, generics) = if let Some(def) = self.struct_def_ids.get(owner).copied() {
            let generics = self.struct_defs[def].generics().to_vec();
            let id =
                self.instantiate_struct(def, identity(&generics), &scope_of(&generics), line)?;
            (self.struct_ty(def, id), generics)
        } else if let Some(def) = self.enum_def_ids.get(owner).copied() {
            let generics = self.enum_defs[def].generics().to_vec();
            let id = self.instantiate_enum(def, identity(&generics), &scope_of(&generics), line)?;
            (self.enum_ty(id), generics)
        } else {
            return Err(CheckError::at(line, format!("unknown type '{owner}'")));
        };
        if func.is_deinit() && !ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("'deinit' applies only to a class, not to '{owner}'"),
            ));
        }
        if func.is_deinit() && !generics.is_empty() {
            return Err(CheckError::at(
                line,
                format!("the generic class '{owner}' cannot declare a deinit yet"),
            ));
        }
        if func.is_override() && !ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("'override' applies only to methods of a class, not of '{owner}'"),
            ));
        }
        if func.is_mutating() && ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("a method of the class '{owner}' cannot be mutating"),
            ));
        }
        Ok((ty, generics))
    }

    pub fn method_names(&self, ty: Ty) -> Vec<String> {
        let mut names = Vec::new();
        let mut current = Some(ty);
        while let Some(t) = current {
            if let Some(owner) = self.owner_name(t) {
                let prefix = format!("{owner}.");
                names.extend(
                    self.funcs
                        .iter()
                        .filter(|(_, sig)| sig.has_receiver())
                        .filter_map(|(name, _)| name.strip_prefix(&prefix).map(str::to_string)),
                );
            }
            current = self.superclass(t);
        }
        names
    }

    pub fn owner_name(&self, ty: Ty) -> Option<String> {
        let (kind, def, _) = self.origin_of(ty)?;
        Some(match kind {
            Kind::Struct => self.struct_defs[def].name().to_string(),
            Kind::Enum => self.enum_defs[def].name().to_string(),
        })
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
        let reserved = decl.is_class().then(|| {
            let mut instances = self.instances.borrow_mut();
            let id = StructId::new(instances.structs.len());
            instances.structs.push(Rc::new(StructSig {
                name: name.clone(),
                fields: Vec::new(),
            }));
            instances.struct_origin.push(Origin::Struct {
                def,
                args: key.1.clone(),
            });
            instances.struct_index.insert(key.clone(), id);
            id
        });
        let fields = self.struct_fields(decl, &key.1, &name, scope, line);
        self.instances.borrow_mut().struct_pending.remove(&key);
        let (fields, superclass) = fields?;
        let mut instances = self.instances.borrow_mut();
        let id = match reserved {
            Some(id) => {
                instances.structs[id.index()] = Rc::new(StructSig { name, fields });
                id
            }
            None => {
                let id = StructId::new(instances.structs.len());
                instances.structs.push(Rc::new(StructSig { name, fields }));
                instances.struct_origin.push(Origin::Struct {
                    def,
                    args: key.1.clone(),
                });
                instances.struct_index.insert(key, id);
                id
            }
        };
        if let Some(parent) = superclass {
            instances.superclass.insert(id, parent);
        }
        Ok(id)
    }

    fn struct_fields(
        &self,
        decl: &StructDecl,
        args: &[Ty],
        name: &str,
        scope: &Scope,
        line: u32,
    ) -> CResult<(Vec<FieldSig>, Option<StructId>)> {
        let own_scope: Vec<(String, Ty)> = decl
            .generics()
            .iter()
            .cloned()
            .zip(args.iter().copied())
            .collect();
        let at = if line == 0 { decl.line() } else { line };
        let mut fields: Vec<FieldSig> = Vec::new();
        let mut parent = None;
        if decl.is_class() {
            fields.extend(HEADER_FIELDS.iter().map(|name| FieldSig {
                name: (*name).to_string(),
                ty: Ty::Int,
                mutable: true,
                unowned: false,
                weak: false,
            }));
            if let Some(superclass) = decl.superclass() {
                let id = self.superclass_id(decl, superclass, at)?;
                let inherited = self.struct_sig(id).ok_or_else(|| {
                    CheckError::at(at, "internal error: unknown superclass layout")
                })?;
                fields.extend(inherited.members().map(|(_, f)| f.clone()));
                parent = Some(id);
            }
        }
        for field in decl.fields() {
            let ty = self.resolve_type(field.ty(), &own_scope, at)?;
            if fields.iter().any(|f| f.name == field.name()) {
                return Err(CheckError::at(
                    at,
                    format!("duplicate field '{}' in '{name}'", field.name()),
                ));
            }
            if field.is_weak() {
                let holds_class = self
                    .unwrapped(ty)
                    .is_some_and(|inner| matches!(inner, Ty::Class(_)));
                if !holds_class || !field.is_mutable() {
                    return Err(CheckError::at(
                        at,
                        format!(
                            "'weak' needs a 'var' member of an optional class type, but '{}' in '{name}' is {}",
                            field.name(),
                            self.display(ty, scope)
                        ),
                    ));
                }
            }
            if field.is_unowned() && !ty.is_class() {
                return Err(CheckError::at(
                    at,
                    format!(
                        "'unowned' needs a class type, but '{}' in '{name}' is {}",
                        field.name(),
                        self.display(ty, scope)
                    ),
                ));
            }
            fields.push(FieldSig {
                name: field.name().to_string(),
                ty,
                mutable: field.is_mutable(),
                unowned: field.is_unowned(),
                weak: field.is_weak(),
            });
        }
        Ok((fields, parent))
    }

    fn superclass_id(&self, decl: &StructDecl, superclass: &str, at: u32) -> CResult<StructId> {
        let Some(def) = self.struct_def_ids.get(superclass).copied() else {
            return Err(CheckError::at(at, format!("unknown type '{superclass}'")));
        };
        let parent = &self.struct_defs[def];
        if !parent.is_class() {
            return Err(CheckError::at(
                at,
                format!(
                    "'{superclass}' is not a class, so '{}' cannot inherit from it",
                    decl.name()
                ),
            ));
        }
        if !decl.generics().is_empty() || !parent.generics().is_empty() {
            return Err(CheckError::at(
                at,
                "generic classes cannot take part in inheritance yet",
            ));
        }
        if superclass == decl.name()
            || self
                .instances
                .borrow()
                .struct_pending
                .contains(&(def, Vec::new()))
        {
            return Err(CheckError::at(
                at,
                format!(
                    "'{}' cannot inherit from itself, directly or not",
                    decl.name()
                ),
            ));
        }
        self.instantiate_struct(def, Vec::new(), &[], at)
    }

    pub fn drop_name(&self, id: StructId) -> String {
        let name = self
            .struct_sig(id)
            .map_or_else(String::new, |s| s.name.clone());
        format!("{name}.$drop")
    }

    pub fn construct(&self, ty: Ty, members: Vec<TExpr>) -> TExpr {
        match ty {
            Ty::Class(id) => {
                let tag = i64::try_from(id.index()).unwrap_or(i64::MAX);
                let header = [
                    TExpr::Int(tag),
                    TExpr::Int(1),
                    TExpr::FuncAddr(self.drop_name(id)),
                    TExpr::Int(0),
                ];
                let args = header.into_iter().chain(members).collect();
                TExpr::New { ty, args }
            }
            _ => TExpr::StructInit { ty, args: members },
        }
    }

    pub fn superclass(&self, ty: Ty) -> Option<Ty> {
        let Ty::Class(id) = ty else { return None };
        let parent = self.instances.borrow().superclass.get(&id).copied()?;
        Some(Ty::Class(parent))
    }

    pub fn is_subclass(&self, sub: Ty, sup: Ty) -> bool {
        let mut current = Some(sub);
        while let Some(ty) = current {
            if ty == sup {
                return true;
            }
            current = self.superclass(ty);
        }
        false
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
        let cases = self.enum_cases(decl, &key.1, &name, line);
        self.instances.borrow_mut().enum_pending.remove(&key);
        let cases = cases?;
        let has_payload = decl.has_payload();
        let at = if line == 0 { decl.line() } else { line };
        let mut instances = self.instances.borrow_mut();
        let id = EnumId::new(instances.enums.len());
        let layout = if has_payload {
            let widest = cases.iter().map(|c| c.payload.len()).max().unwrap_or(0);
            let mut fields = vec![FieldSig {
                name: "tag".to_string(),
                ty: Ty::Int,
                mutable: false,
                unowned: false,
                weak: false,
            }];
            for slot in 0..widest {
                let ty = Self::slot_type(&cases, slot).ok_or_else(|| {
                    CheckError::at(
                        at,
                        format!(
                            "the payloads at position {slot} of '{name}' must have the same type when it is a class or a struct"
                        ),
                    )
                })?;
                fields.push(FieldSig {
                    name: format!("payload{slot}"),
                    ty,
                    mutable: false,
                    unowned: false,
                    weak: false,
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

    fn slot_type(cases: &[CaseSig], slot: usize) -> Option<Ty> {
        let types: Vec<Ty> = cases
            .iter()
            .filter_map(|c| c.payload.get(slot).copied())
            .collect();
        let first = *types.first()?;
        if types.iter().all(|t| *t == first) {
            Some(first)
        } else if types
            .iter()
            .any(|t| matches!(t, Ty::Class(_) | Ty::Struct(_)))
        {
            None
        } else {
            Some(Ty::Int)
        }
    }

    fn enum_cases(
        &self,
        decl: &EnumDecl,
        args: &[Ty],
        name: &str,
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
                payload.push(self.resolve_type(tr, &own_scope, at)?);
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

    fn enum_cell(&mut self, id: EnumId) -> Ty {
        let mut instances = self.instances.borrow_mut();
        if let Some(cell) = instances.cell_index.get(&id) {
            return Ty::Struct(*cell);
        }
        let name = format!("{}.self", instances.enums[id.index()].name);
        let cell = StructId::new(instances.structs.len());
        instances.structs.push(Rc::new(StructSig {
            name,
            fields: vec![FieldSig {
                name: "value".to_string(),
                ty: Ty::Int,
                mutable: true,
                unowned: false,
                weak: false,
            }],
        }));
        instances.struct_origin.push(Origin::EnumCell(id));
        instances.cell_owner.insert(cell, id);
        instances.cell_index.insert(id, cell);
        Ty::Struct(cell)
    }

    pub fn cell_enum(&self, ty: Ty) -> Option<Ty> {
        let Ty::Struct(cell) = ty else { return None };
        let id = self.instances.borrow().cell_owner.get(&cell).copied()?;
        Some(Ty::Enum(id))
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

    pub fn optional_of(&self, ty: Ty) -> CResult<Ty> {
        let id = self.instantiate_enum(0, vec![ty], &[], 0)?;
        Ok(self.enum_ty(id))
    }

    pub fn unwrapped(&self, ty: Ty) -> Option<Ty> {
        match self.origin_of(ty)? {
            (Kind::Enum, 0, args) => args.first().copied(),
            _ => None,
        }
    }

    pub fn display(&self, ty: Ty, scope: &Scope) -> String {
        if let Some(inner) = self.unwrapped(ty) {
            return format!("{}?", self.display(inner, scope));
        }
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
                Origin::EnumLayout(owner) | Origin::EnumCell(owner) => {
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
        let instances = self.instances.borrow();
        instances
            .structs
            .iter()
            .map(|s| {
                let mut slots = Vec::new();
                let mut starts = Vec::new();
                for field in &s.fields {
                    starts.push(slots.len());
                    Self::flatten(&instances, field.ty, &mut slots);
                }
                TStruct::new(s.name.clone(), slots, starts)
            })
            .collect()
    }

    fn flatten(instances: &Instances, ty: Ty, slots: &mut Vec<Ty>) {
        match ty {
            Ty::Struct(id) => match instances.structs.get(id.index()) {
                Some(sig) => {
                    for field in &sig.fields {
                        Self::flatten(instances, field.ty, slots);
                    }
                }
                None => slots.push(ty),
            },
            other => slots.push(other),
        }
    }
}

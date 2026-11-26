use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    Decl, EnumCaseDecl, EnumDecl, Field, FuncDecl, ProtocolDecl, StructDecl, TypeRef,
};
use crate::tir::{TExpr, TStruct};
use crate::types::{EnumId, StructId, Ty};

pub const PRINT: &str = "print";
pub const FATAL_ERROR: &str = "fatalError";

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

pub const BOX: &str = "$Box";
pub const ARRAY: &str = "Array";
pub const EQUAL: &str = "$equal";
pub const CONTAINS: &str = "$contains";
pub const ARRAY_COUNT: usize = 4;
pub const ARRAY_CAPACITY: usize = 5;
pub const ARRAY_BUFFER: usize = 6;
pub const OPTIONAL: &str = "Optional";
pub const NONE_CASE: usize = 0;
pub const SOME_CASE: usize = 1;

fn box_decl() -> StructDecl {
    StructDecl::new(
        0,
        BOX.to_string(),
        vec![Field::new("value".to_string(), false, TypeRef::from("T"))],
    )
    .with_generics(vec!["T".to_string()])
    .as_class()
}

fn array_decl() -> StructDecl {
    StructDecl::new(
        0,
        ARRAY.to_string(),
        vec![
            Field::new("count".to_string(), false, TypeRef::from("Int")),
            Field::new("$capacity".to_string(), true, TypeRef::from("Int")),
            Field::new("$buffer".to_string(), true, TypeRef::from("Int")),
        ],
    )
    .with_generics(vec!["Element".to_string()])
    .as_class()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Conformer {
    Class(Ty),
    Boxed { value: Ty, boxed: Ty },
}

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

pub const EQUATABLE: &str = "Equatable";
pub const COMPARABLE: &str = "Comparable";
pub const SELF_TYPE: &str = "Self";

pub type CResult<T> = Result<T, CheckError>;

#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    name: String,
    params: Vec<ParamSig>,
    ret: Ty,
    mutating: bool,
}

impl Requirement {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &[ParamSig] {
        &self.params
    }

    pub fn ret(&self) -> Ty {
        self.ret
    }

    pub fn is_mutating(&self) -> bool {
        self.mutating
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolSig {
    name: String,
    requirements: Vec<Requirement>,
    associated: Vec<String>,
}

impl ProtocolSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }

    pub fn associated(&self) -> &[String] {
        &self.associated
    }

    pub fn requirement(&self, name: &str) -> Option<&Requirement> {
        self.requirements.iter().find(|r| r.name == name)
    }
}

fn operator_requirement(name: &str) -> Requirement {
    let operand = ParamSig {
        label: None,
        ty: Ty::Param(0),
    };
    Requirement {
        name: name.to_string(),
        params: vec![operand.clone(), operand],
        ret: Ty::Bool,
        mutating: false,
    }
}

fn standard_protocols() -> HashMap<String, ProtocolSig> {
    let equatable = ProtocolSig {
        name: EQUATABLE.to_string(),
        requirements: vec![operator_requirement("==")],
        associated: Vec::new(),
    };
    let comparable = ProtocolSig {
        name: COMPARABLE.to_string(),
        requirements: ["<", ">", "<=", ">="]
            .iter()
            .map(|op| operator_requirement(op))
            .collect(),
        associated: Vec::new(),
    };
    [equatable, comparable]
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect()
}

pub fn bound_includes(bound: &[String], protocol: &str) -> bool {
    bound
        .iter()
        .any(|have| have == protocol || (protocol == EQUATABLE && have == COMPARABLE))
}

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
    bounds: Vec<Vec<String>>,
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

    pub fn bounds(&self) -> &[Vec<String>] {
        &self.bounds
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
    slots: Vec<usize>,
    labels: Vec<Option<String>>,
}

impl CaseSig {
    pub fn labels(&self) -> &[Option<String>] {
        &self.labels
    }

    pub fn slots(&self) -> &[usize] {
        &self.slots
    }

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
    Existential(String),
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
    existentials: HashMap<String, StructId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signatures {
    funcs: HashMap<String, FuncSig>,
    struct_defs: Vec<StructDecl>,
    struct_def_ids: HashMap<String, usize>,
    enum_defs: Vec<EnumDecl>,
    enum_def_ids: HashMap<String, usize>,
    protocols: HashMap<String, ProtocolSig>,
    assoc_types: Vec<(String, String)>,
    inits: HashMap<String, Vec<String>>,
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
            protocols: standard_protocols(),
            assoc_types: Vec::new(),
            inits: HashMap::new(),
            instances: RefCell::new(Instances::default()),
        };
        let mut protocol_decls: Vec<&ProtocolDecl> = Vec::new();
        for decl in decls {
            if let Decl::Protocol(p) = decl {
                sigs.register_protocol_name(p)?;
                protocol_decls.push(p);
            }
        }
        let normalized: Vec<Decl> = decls
            .iter()
            .map(|decl| match decl {
                Decl::Struct(s)
                    if s.superclass().is_some_and(|parent| {
                        parent.args().is_empty() && sigs.protocols.contains_key(parent.name())
                    }) =>
                {
                    let mut names = vec![
                        s.superclass()
                            .map_or_else(String::new, |p| p.name().to_string()),
                    ];
                    names.extend(s.conformances().iter().cloned());
                    Decl::Struct(s.clone().without_superclass().with_conformances(names))
                }
                other => other.clone(),
            })
            .collect();
        let decls = &normalized[..];
        sigs.register_enum_def(&optional_decl())?;
        sigs.register_struct_def(&box_decl())?;
        sigs.register_struct_def(&array_decl())?;
        for decl in decls {
            match decl {
                Decl::Struct(s) => sigs.register_struct_def(s)?,
                Decl::Enum(e) => sigs.register_enum_def(e)?,
                Decl::Func(_) | Decl::Protocol(_) => {}
            }
        }
        sigs.validate_protocol_names()?;
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
        sigs.instantiate_boxes()?;
        for decl in protocol_decls {
            sigs.resolve_protocol(decl)?;
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
        sigs.check_conformances()?;
        Ok(sigs)
    }

    fn declares_user_protocol(&self, conformances: &[String]) -> bool {
        conformances
            .iter()
            .any(|name| name != EQUATABLE && name != COMPARABLE)
    }

    fn instantiate_boxes(&self) -> CResult<()> {
        for def in 0..self.struct_defs.len() {
            let decl = &self.struct_defs[def];
            if decl.name() == BOX
                || decl.is_class()
                || !decl.generics().is_empty()
                || !self.declares_user_protocol(decl.conformances())
            {
                continue;
            }
            let id = self.instantiate_struct(def, Vec::new(), &[], decl.line())?;
            self.box_of(self.struct_ty(def, id), decl.line())?;
        }
        for def in 0..self.enum_defs.len() {
            let decl = &self.enum_defs[def];
            if decl.name() == OPTIONAL
                || !decl.generics().is_empty()
                || !self.declares_user_protocol(decl.conformances())
            {
                continue;
            }
            let id = self.instantiate_enum(def, Vec::new(), &[], decl.line())?;
            self.box_of(self.enum_ty(id), decl.line())?;
        }
        Ok(())
    }

    fn register_protocol_name(&mut self, decl: &ProtocolDecl) -> CResult<()> {
        if decl.name() == EQUATABLE || decl.name() == COMPARABLE {
            return Err(CheckError::at(
                decl.line(),
                format!(
                    "'{}' is a standard protocol and cannot be declared again",
                    decl.name()
                ),
            ));
        }
        let placeholder = ProtocolSig {
            name: decl.name().to_string(),
            requirements: Vec::new(),
            associated: decl.associated().to_vec(),
        };
        if self
            .protocols
            .insert(decl.name().to_string(), placeholder)
            .is_some()
        {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate protocol '{}'", decl.name()),
            ));
        }
        for name in decl.associated() {
            self.assoc_types
                .push((decl.name().to_string(), name.clone()));
        }
        Ok(())
    }

    fn require_protocol(&self, name: &str, line: u32) -> CResult<()> {
        if self.protocols.contains_key(name) {
            Ok(())
        } else if self.struct_def_ids.contains_key(name) || self.enum_def_ids.contains_key(name) {
            Err(CheckError::at(line, format!("'{name}' is not a protocol")))
        } else {
            Err(CheckError::at(line, format!("unknown protocol '{name}'")))
        }
    }

    fn validate_protocol_names(&self) -> CResult<()> {
        for decl in &self.struct_defs {
            for name in decl.conformances() {
                self.require_protocol(name, decl.line())?;
            }
            for index in 0..decl.generics().len() {
                for name in decl.bound(index) {
                    self.require_protocol(name, decl.line())?;
                }
            }
        }
        for decl in &self.enum_defs {
            for name in decl.conformances() {
                self.require_protocol(name, decl.line())?;
            }
            for index in 0..decl.generics().len() {
                for name in decl.bound(index) {
                    self.require_protocol(name, decl.line())?;
                }
            }
        }
        Ok(())
    }

    fn resolve_protocol(&mut self, decl: &ProtocolDecl) -> CResult<()> {
        let mut scope = vec![(SELF_TYPE.to_string(), Ty::Param(0))];
        for name in decl.associated() {
            if let Some(id) = self.assoc_id(decl.name(), name) {
                scope.push((name.clone(), Ty::assoc(0, id)));
            }
        }
        let mut requirements: Vec<Requirement> = Vec::new();
        for requirement in decl.requirements() {
            if requirements.iter().any(|r| r.name == requirement.name()) {
                return Err(CheckError::at(
                    requirement.line(),
                    format!("duplicate requirement '{}'", requirement.name()),
                ));
            }
            if !requirement.generics().is_empty() {
                return Err(CheckError::at(
                    requirement.line(),
                    "a protocol requirement cannot declare type parameters yet",
                ));
            }
            let mut params = Vec::new();
            for param in requirement.params() {
                params.push(ParamSig {
                    label: param.label().map(str::to_string),
                    ty: self.resolve_type(param.ty(), &scope, requirement.line())?,
                });
            }
            let ret = match requirement.ret() {
                Some(ty) => self.resolve_type(ty, &scope, requirement.line())?,
                None => Ty::Void,
            };
            requirements.push(Requirement {
                name: requirement.name().to_string(),
                params,
                ret,
                mutating: requirement.is_mutating(),
            });
        }
        self.protocols.insert(
            decl.name().to_string(),
            ProtocolSig {
                name: decl.name().to_string(),
                requirements,
                associated: decl.associated().to_vec(),
            },
        );
        Ok(())
    }

    fn assoc_id(&self, protocol: &str, name: &str) -> Option<usize> {
        self.assoc_types
            .iter()
            .position(|(p, n)| p == protocol && n == name)
    }

    pub fn scope_for(&self, generics: &[String], bounds: &[Vec<String>]) -> Vec<(String, Ty)> {
        let mut scope = scope_of(generics);
        for (index, name) in generics.iter().enumerate() {
            for protocol in bounds.get(index).map_or(&[][..], Vec::as_slice) {
                let Some(sig) = self.protocols.get(protocol) else {
                    continue;
                };
                for item in sig.associated() {
                    if let Some(id) = self.assoc_id(protocol, item) {
                        scope.push((format!("{name}.{item}"), Ty::assoc(index, id)));
                    }
                }
            }
        }
        scope
    }

    fn type_scope(
        &self,
        decl_generics: &[String],
        bounds: &[Vec<String>],
        args: &[Ty],
    ) -> Vec<(String, Ty)> {
        let mut scope: Vec<(String, Ty)> = decl_generics
            .iter()
            .cloned()
            .zip(args.iter().copied())
            .collect();
        for (index, name) in decl_generics.iter().enumerate() {
            let Some(arg) = args.get(index) else { continue };
            for protocol in bounds.get(index).map_or(&[][..], Vec::as_slice) {
                let Some(sig) = self.protocols.get(protocol) else {
                    continue;
                };
                for item in sig.associated() {
                    let Some(id) = self.assoc_id(protocol, item) else {
                        continue;
                    };
                    let projected = match arg {
                        Ty::Param(p) => Some(Ty::assoc(*p, id)),
                        other => self.assoc_binding(*other, id),
                    };
                    if let Some(projected) = projected {
                        scope.push((format!("{name}.{item}"), projected));
                    }
                }
            }
        }
        scope
    }

    pub fn assoc_binding(&self, ty: Ty, id: usize) -> Option<Ty> {
        let (_, name) = self.assoc_types.get(id)?;
        let mut current = Some(ty);
        while let Some(t) = current {
            if let Some((kind, def, args)) = self.origin_of(t) {
                let (generics, aliases) = match kind {
                    Kind::Struct => (
                        self.struct_defs[def].generics(),
                        self.struct_defs[def].aliases(),
                    ),
                    Kind::Enum => (
                        self.enum_defs[def].generics(),
                        self.enum_defs[def].aliases(),
                    ),
                };
                if let Some((_, alias)) = aliases.iter().find(|(n, _)| n == name) {
                    let scope: Vec<(String, Ty)> =
                        generics.iter().cloned().zip(args.iter().copied()).collect();
                    return self.resolve_type(alias, &scope, 0).ok();
                }
            }
            current = self.superclass(t);
        }
        None
    }

    pub fn member_scope(
        &self,
        owner: Option<&str>,
        generics: &[String],
        bounds: &[Vec<String>],
        line: u32,
    ) -> CResult<Vec<(String, Ty)>> {
        let mut scope = self.scope_for(generics, bounds);
        let aliases = owner.map_or(&[][..], |owner| self.owner_aliases(owner));
        for (name, alias) in aliases {
            let ty = self.resolve_type(alias, &scope, line)?;
            scope.push((name.clone(), ty));
        }
        Ok(scope)
    }

    fn owner_aliases(&self, owner: &str) -> &[(String, TypeRef)] {
        if let Some(def) = self.struct_def_ids.get(owner) {
            self.struct_defs[*def].aliases()
        } else if let Some(def) = self.enum_def_ids.get(owner) {
            self.enum_defs[*def].aliases()
        } else {
            &[]
        }
    }

    pub fn protocol(&self, name: &str) -> Option<&ProtocolSig> {
        self.protocols.get(name)
    }

    fn declared_conformances(&self, ty: Ty) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        let mut current = Some(ty);
        while let Some(t) = current {
            if let Some((kind, def, _)) = self.origin_of(t) {
                let declared = match kind {
                    Kind::Struct => self.struct_defs[def].conformances(),
                    Kind::Enum => self.enum_defs[def].conformances(),
                };
                names.extend(declared.iter().map(String::as_str));
            }
            current = self.superclass(t);
        }
        names
    }

    pub fn type_arguments(&self, ty: Ty) -> Vec<Ty> {
        self.origin_of(ty)
            .map(|(_, _, args)| args)
            .unwrap_or_default()
    }

    pub fn existential_protocol(&self, ty: Ty) -> Option<String> {
        let Ty::Class(id) = ty else { return None };
        match self.instances.borrow().struct_origin.get(id.index())? {
            Origin::Existential(name) => Some(name.clone()),
            _ => None,
        }
    }

    pub fn existential(&self, protocol: &str, line: u32) -> CResult<Ty> {
        let sig = self
            .protocols
            .get(protocol)
            .ok_or_else(|| CheckError::at(line, format!("unknown protocol '{protocol}'")))?;
        let standard = protocol == EQUATABLE || protocol == COMPARABLE;
        let mentions_self = sig
            .requirements()
            .iter()
            .any(|r| self.has_param(r.ret()) || r.params().iter().any(|p| self.has_param(p.ty())));
        if standard || mentions_self || !sig.associated().is_empty() {
            return Err(CheckError::at(
                line,
                format!(
                    "the protocol '{protocol}' can only be used as a constraint: it has associated types or requirements that mention Self"
                ),
            ));
        }
        let mut instances = self.instances.borrow_mut();
        if let Some(id) = instances.existentials.get(protocol) {
            return Ok(Ty::Class(*id));
        }
        let id = StructId::new(instances.structs.len());
        let fields = HEADER_FIELDS
            .iter()
            .map(|name| FieldSig {
                name: (*name).to_string(),
                ty: Ty::Int,
                mutable: true,
                unowned: false,
                weak: false,
            })
            .collect();
        instances.structs.push(Rc::new(StructSig {
            name: protocol.to_string(),
            fields,
        }));
        instances
            .struct_origin
            .push(Origin::Existential(protocol.to_string()));
        instances.existentials.insert(protocol.to_string(), id);
        Ok(Ty::Class(id))
    }

    pub fn box_of(&self, value: Ty, line: u32) -> CResult<Ty> {
        let def = self
            .struct_def_ids
            .get(BOX)
            .copied()
            .ok_or_else(|| CheckError::at(line, "internal error: no box class"))?;
        let id = self.instantiate_struct(def, vec![value], &[], line)?;
        Ok(self.struct_ty(def, id))
    }

    pub fn existential_conformers(&self, protocol: &str) -> CResult<Vec<Conformer>> {
        let mut found = Vec::new();
        for def in 0..self.struct_defs.len() {
            let decl = &self.struct_defs[def];
            if decl.name() == BOX || !decl.generics().is_empty() {
                continue;
            }
            let id = self.instantiate_struct(def, Vec::new(), &[], decl.line())?;
            let ty = self.struct_ty(def, id);
            if !self.conforms(ty, protocol) {
                continue;
            }
            found.push(if ty.is_class() {
                Conformer::Class(ty)
            } else {
                Conformer::Boxed {
                    value: ty,
                    boxed: self.box_of(ty, decl.line())?,
                }
            });
        }
        for def in 0..self.enum_defs.len() {
            let decl = &self.enum_defs[def];
            if decl.name() == OPTIONAL || !decl.generics().is_empty() {
                continue;
            }
            let id = self.instantiate_enum(def, Vec::new(), &[], decl.line())?;
            let ty = self.enum_ty(id);
            if self.conforms(ty, protocol) {
                found.push(Conformer::Boxed {
                    value: ty,
                    boxed: self.box_of(ty, decl.line())?,
                });
            }
        }
        Ok(found)
    }

    pub fn conforms(&self, ty: Ty, protocol: &str) -> bool {
        match protocol {
            EQUATABLE => {
                matches!(ty, Ty::Int | Ty::Bool | Ty::Str | Ty::Char | Ty::Enum(_))
                    || self.declared_conformances(ty).contains(&protocol)
                    || self
                        .unwrapped(ty)
                        .is_some_and(|inner| self.conforms(inner, EQUATABLE))
                    || self
                        .array_element(ty)
                        .is_some_and(|inner| self.conforms(inner, EQUATABLE))
            }
            COMPARABLE => matches!(ty, Ty::Int | Ty::Str | Ty::Char),
            _ => self.declared_conformances(ty).contains(&protocol),
        }
    }

    pub fn find_requirement(
        &self,
        bound: &[String],
        method: &str,
    ) -> Option<(&ProtocolSig, &Requirement)> {
        bound.iter().find_map(|name| {
            let protocol = self.protocols.get(name)?;
            Some((protocol, protocol.requirement(method)?))
        })
    }

    pub fn unsatisfied_bound(
        &self,
        bounds: &[Vec<String>],
        type_args: &[Ty],
        caller_bounds: &[Vec<String>],
    ) -> Option<(Ty, String)> {
        for (bound, ty) in bounds.iter().zip(type_args) {
            for protocol in bound {
                let holds = match ty {
                    Ty::Param(index) => caller_bounds
                        .get(*index)
                        .is_some_and(|have| bound_includes(have, protocol)),
                    other => self.conforms(*other, protocol),
                };
                if !holds {
                    return Some((*ty, protocol.clone()));
                }
            }
        }
        None
    }

    fn check_type_bounds(
        &self,
        bounds: &[Vec<String>],
        args: &[Ty],
        type_name: &str,
        line: u32,
    ) -> CResult<()> {
        if args.iter().any(|a| self.has_param(*a)) {
            return Ok(());
        }
        match self.unsatisfied_bound(bounds, args, &[]) {
            Some((ty, protocol)) => Err(CheckError::at(
                line,
                format!(
                    "type '{}' does not conform to the protocol '{protocol}' required by '{type_name}'",
                    self.type_name(ty)
                ),
            )),
            None => Ok(()),
        }
    }

    fn equatable_value(&self, ty: Ty) -> bool {
        match ty {
            Ty::Int | Ty::Bool | Ty::Str | Ty::Char | Ty::Enum(_) => true,
            Ty::Class(_) => self
                .array_element(ty)
                .is_some_and(|inner| self.equatable_value(inner)),
            Ty::Struct(_) => match self.unwrapped(ty) {
                Some(inner) => self.equatable_value(inner),
                None => self.enum_of(ty).is_none() && self.equality_leaves(0, ty).is_ok(),
            },
            _ => false,
        }
    }

    pub fn equality_leaves(&self, line: u32, ty: Ty) -> CResult<Vec<Vec<(usize, Ty)>>> {
        let mut leaves = Vec::new();
        self.collect_equality_leaves(line, ty, &mut Vec::new(), &mut leaves)?;
        Ok(leaves)
    }

    fn collect_equality_leaves(
        &self,
        line: u32,
        ty: Ty,
        path: &mut Vec<(usize, Ty)>,
        leaves: &mut Vec<Vec<(usize, Ty)>>,
    ) -> CResult<()> {
        let Ty::Struct(id) = ty else {
            return Ok(());
        };
        let sig = self
            .struct_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown struct"))?;
        for (index, field) in sig.members() {
            let member = field.ty();
            path.push((index, member));
            match member {
                Ty::Int | Ty::Bool | Ty::Str | Ty::Char | Ty::Enum(_) => leaves.push(path.clone()),
                Ty::Struct(_) if self.enum_of(member).is_none() => {
                    self.collect_equality_leaves(line, member, path, leaves)?;
                }
                Ty::Struct(_)
                    if self
                        .unwrapped(member)
                        .is_some_and(|inner| self.equatable_value(inner)) =>
                {
                    leaves.push(path.clone());
                }
                _ => {
                    return Err(CheckError::at(
                        line,
                        format!(
                            "equality is not supported for {}: member '{}' has type {}",
                            self.type_name(ty),
                            field.name(),
                            self.type_name(member)
                        ),
                    ));
                }
            }
            path.pop();
        }
        Ok(())
    }

    fn check_conformances(&self) -> CResult<()> {
        for def in 0..self.struct_defs.len() {
            let decl = &self.struct_defs[def];
            let generics = decl.generics().to_vec();
            let id = self.instantiate_struct(
                def,
                identity(&generics),
                &scope_of(&generics),
                decl.line(),
            )?;
            let ty = self.struct_ty(def, id);
            self.check_type_conformances(
                ty,
                decl.name(),
                decl.conformances(),
                !generics.is_empty(),
                decl.line(),
            )?;
        }
        for def in 0..self.enum_defs.len() {
            let decl = &self.enum_defs[def];
            let generics = decl.generics().to_vec();
            let id =
                self.instantiate_enum(def, identity(&generics), &scope_of(&generics), decl.line())?;
            let ty = self.enum_ty(id);
            self.check_type_conformances(
                ty,
                decl.name(),
                decl.conformances(),
                !generics.is_empty(),
                decl.line(),
            )?;
        }
        Ok(())
    }

    fn check_type_conformances(
        &self,
        ty: Ty,
        name: &str,
        conformances: &[String],
        generic: bool,
        line: u32,
    ) -> CResult<()> {
        for protocol in conformances {
            match protocol.as_str() {
                COMPARABLE => {
                    return Err(CheckError::at(
                        line,
                        format!(
                            "'{name}' cannot conform to 'Comparable': only Int and String are Comparable yet"
                        ),
                    ));
                }
                EQUATABLE => self.check_equatable(ty, name, generic, line)?,
                _ => self.check_requirements(ty, name, protocol, line)?,
            }
        }
        Ok(())
    }

    fn check_equatable(&self, ty: Ty, name: &str, generic: bool, line: u32) -> CResult<()> {
        if generic {
            return Err(CheckError::at(
                line,
                format!("the generic type '{name}' cannot conform to 'Equatable' yet"),
            ));
        }
        if ty.is_class() {
            return Err(CheckError::at(
                line,
                format!("the class '{name}' cannot conform to 'Equatable' yet"),
            ));
        }
        if matches!(ty, Ty::Enum(_)) {
            return Ok(());
        }
        if self.enum_of(ty).is_some() {
            return Err(CheckError::at(
                line,
                format!(
                    "equality is not supported for '{name}': an enum with payloads cannot conform to 'Equatable' yet"
                ),
            ));
        }
        self.equality_leaves(line, ty).map(|_| ())
    }

    fn check_requirements(&self, ty: Ty, name: &str, protocol: &str, line: u32) -> CResult<()> {
        let Some(sig) = self.protocols.get(protocol) else {
            return Ok(());
        };
        let owner_generics = self.owner_generics(name);
        for item in sig.associated() {
            let bound = self
                .assoc_id(protocol, item)
                .and_then(|id| self.assoc_binding(ty, id));
            if bound.is_none() {
                return Err(CheckError::at(
                    line,
                    format!(
                        "'{name}' does not define the associated type '{item}' of '{protocol}': add 'typealias {item} = ...'"
                    ),
                ));
            }
        }
        for requirement in sig.requirements() {
            let mismatch = |why: &str| {
                CheckError::at(
                    line,
                    format!(
                        "'{name}' does not implement the requirement '{}' of '{protocol}': {why}",
                        requirement.name()
                    ),
                )
            };
            let qualified = self
                .resolve_method(ty, requirement.name())
                .ok_or_else(|| mismatch("the method is missing"))?;
            let method = self
                .funcs
                .get(&qualified)
                .ok_or_else(|| mismatch("the method is missing"))?;
            if method.generics().len() != owner_generics {
                return Err(mismatch("a generic method cannot satisfy a requirement"));
            }
            let given = &method.params()[1..];
            if given.len() != requirement.params().len() {
                return Err(mismatch("the number of parameters differs"));
            }
            for (have, want) in given.iter().zip(requirement.params()) {
                let wanted = self.subst(want.ty(), &[ty], &[], line)?;
                if have.label() != want.label() || have.ty() != wanted {
                    return Err(mismatch("a parameter label or type differs"));
                }
            }
            if method.ret() != self.subst(requirement.ret(), &[ty], &[], line)? {
                return Err(mismatch("the result type differs"));
            }
            if method.is_mutating() && !requirement.is_mutating() {
                return Err(mismatch(
                    "the method is mutating but the requirement is not",
                ));
            }
            if requirement.is_mutating() && !method.is_mutating() && !ty.is_class() {
                return Err(mismatch(
                    "the requirement is mutating but the method is not",
                ));
            }
            if requirement.is_mutating() && matches!(ty, Ty::Enum(_)) {
                return Err(mismatch(
                    "an enum without payloads cannot satisfy a mutating requirement yet",
                ));
            }
        }
        Ok(())
    }

    fn owner_generics(&self, owner: &str) -> usize {
        if let Some(def) = self.struct_def_ids.get(owner) {
            self.struct_defs[*def].generics().len()
        } else if let Some(def) = self.enum_def_ids.get(owner) {
            self.enum_defs[*def].generics().len()
        } else {
            0
        }
    }

    fn owner_bounds(&self, owner: &str) -> Vec<Vec<String>> {
        if let Some(def) = self.struct_def_ids.get(owner) {
            let decl = &self.struct_defs[*def];
            (0..decl.generics().len())
                .map(|i| decl.bound(i).to_vec())
                .collect()
        } else if let Some(def) = self.enum_def_ids.get(owner) {
            let decl = &self.enum_defs[*def];
            (0..decl.generics().len())
                .map(|i| decl.bound(i).to_vec())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn register_struct_def(&mut self, decl: &StructDecl) -> CResult<()> {
        if self.struct_def_ids.contains_key(decl.name())
            || self.enum_def_ids.contains_key(decl.name())
            || self.protocols.contains_key(decl.name())
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
            || self.protocols.contains_key(decl.name())
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
        if func.name() == PRINT || func.name() == FATAL_ERROR {
            return Err(CheckError::at(
                func.line(),
                format!("'{}' is a reserved function name", func.name()),
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
        let mut bounds = func
            .owner()
            .map(|owner| self.owner_bounds(owner))
            .unwrap_or_default();
        for index in 0..func.generics().len() {
            for name in func.bound(index) {
                self.require_protocol(name, func.line())?;
            }
            bounds.push(func.bound(index).to_vec());
        }
        let scope = self.member_scope(func.owner(), &generics, &bounds, func.line())?;
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
            bounds,
            params,
            ret,
            receiver: receiver.is_some(),
            mutating: func.is_mutating(),
        };
        if self.funcs.insert(func.name().to_string(), sig).is_some() {
            let kind = if func.is_init() {
                "initializer"
            } else {
                "function"
            };
            return Err(CheckError::at(
                func.line(),
                format!("duplicate {kind} '{}'", func.name()),
            ));
        }
        if let (true, Some(owner)) = (func.is_init(), func.owner()) {
            self.inits
                .entry(owner.to_string())
                .or_default()
                .push(func.name().to_string());
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
        let own_generics = self.owner_generics(owner);
        let has_own_parameters = self
            .funcs
            .get(func.name())
            .is_some_and(|sig| sig.generics().len() > own_generics);
        if has_own_parameters && (func.is_override() || inherited.is_some()) {
            return Err(CheckError::at(
                line,
                format!("'{name}' has type parameters of its own and cannot be overridden"),
            ));
        }
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
                let owner_args = self
                    .superclass(ty)
                    .and_then(|parent| self.resolve_method_owner(parent, name))
                    .map(|(_, owner_ty)| self.type_arguments(owner_ty))
                    .unwrap_or_default();
                let seen = |ty: Ty| self.subst(ty, &owner_args, &[], line).unwrap_or(ty);
                let same = mine.ret == seen(theirs.ret)
                    && mine.params.len() == theirs.params.len()
                    && mine
                        .params
                        .iter()
                        .zip(&theirs.params)
                        .skip(1)
                        .all(|(a, b)| a.ty == seen(b.ty) && a.label == b.label);
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

    pub fn initializers(&self, owner: &str) -> &[String] {
        self.inits.get(owner).map_or(&[], Vec::as_slice)
    }

    pub fn has_init(&self, owner: &str) -> bool {
        !self.initializers(owner).is_empty()
    }

    pub fn inherited_init_owner(&self, ty: Ty) -> Option<String> {
        let mut current = self.superclass(ty);
        while let Some(t) = current {
            let owner = self.owner_name(t)?;
            if self.has_init(&owner) {
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

    pub fn mangled_call(&self, callee: &str, args: &[Ty]) -> String {
        let names: Vec<String> = args.iter().map(|a| self.type_name(*a)).collect();
        format!("{callee}<{}>", names.join(","))
    }

    pub fn deinit_of(&self, class: Ty) -> Option<(String, Vec<Ty>)> {
        let qualified = format!("{}.deinit", self.owner_name(class)?);
        self.funcs.get(&qualified)?;
        Some((qualified, self.type_arguments(class)))
    }

    pub fn deinit_name(&self, class: Ty) -> Option<String> {
        let (qualified, args) = self.deinit_of(class)?;
        Some(if args.is_empty() {
            qualified
        } else {
            self.mangled_call(&qualified, &args)
        })
    }

    pub fn descriptor_name(&self, class: Ty) -> String {
        format!("apus_class_{}", self.type_name(class))
    }

    pub fn resolve_method_owner(&self, ty: Ty, name: &str) -> Option<(String, Ty)> {
        let mut current = Some(ty);
        while let Some(t) = current {
            let qualified = format!("{}.{name}", self.owner_name(t)?);
            if self
                .funcs
                .get(&qualified)
                .is_some_and(FuncSig::has_receiver)
            {
                return Some((qualified, t));
            }
            current = self.superclass(t);
        }
        None
    }

    fn virtual_candidates(&self, class: Ty) -> Vec<String> {
        let Some(owner) = self.owner_name(class) else {
            return Vec::new();
        };
        let own_generics = self.owner_generics(&owner);
        let prefix = format!("{owner}.");
        let mut names: Vec<String> = self
            .funcs
            .iter()
            .filter(|(_, sig)| {
                sig.has_receiver() && !sig.is_mutating() && sig.generics().len() == own_generics
            })
            .filter_map(|(name, _)| name.strip_prefix(&prefix))
            .filter(|name| !name.contains('.') && *name != "deinit")
            .map(str::to_string)
            .collect();
        names.sort();
        names
    }

    pub fn vtable_methods(&self, class: Ty) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current = Some(class);
        while let Some(t) = current {
            chain.push(t);
            current = self.superclass(t);
        }
        let mut methods: Vec<String> = Vec::new();
        for t in chain.into_iter().rev() {
            for name in self.virtual_candidates(t) {
                if !methods.contains(&name) {
                    methods.push(name);
                }
            }
        }
        methods
    }

    pub fn vtable_slot(&self, class: Ty, name: &str) -> Option<usize> {
        self.vtable_methods(class)
            .iter()
            .position(|method| method == name)
    }

    pub fn is_overridden_below(&self, class: Ty, name: &str) -> bool {
        let Some((Kind::Struct, def, _)) = self.origin_of(class) else {
            return false;
        };
        (0..self.struct_defs.len()).any(|candidate| {
            if candidate == def || !self.struct_defs[candidate].is_class() {
                return false;
            }
            let mut current = self.struct_defs[candidate].superclass();
            let mut steps = 0;
            while let Some(parent) = current {
                let Some(parent_def) = self.struct_def_ids.get(parent.name()).copied() else {
                    return false;
                };
                if parent_def == def {
                    let method = format!("{}.{name}", self.struct_defs[candidate].name());
                    return self.funcs.get(&method).is_some_and(FuncSig::has_receiver);
                }
                current = self.struct_defs[parent_def].superclass();
                steps += 1;
                if steps > self.struct_defs.len() {
                    return false;
                }
            }
            false
        })
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
        if self.protocols.contains_key(name) {
            self.reject_args(args_given, name, line)?;
            return self.existential(name, line);
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
        if decl.name() == ARRAY && key.1.first() == Some(&Ty::Void) {
            return Err(CheckError::at(line, "an array cannot hold Void"));
        }
        let bounds: Vec<Vec<String>> = (0..decl.generics().len())
            .map(|i| decl.bound(i).to_vec())
            .collect();
        self.check_type_bounds(&bounds, &key.1, decl.name(), line)?;
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
        let bounds: Vec<Vec<String>> = (0..decl.generics().len())
            .map(|i| decl.bound(i).to_vec())
            .collect();
        let own_scope = self.type_scope(decl.generics(), &bounds, args);
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
                let id = self.superclass_id(decl, superclass, &own_scope, at)?;
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

    fn superclass_id(
        &self,
        decl: &StructDecl,
        superclass: &TypeRef,
        scope: &Scope,
        at: u32,
    ) -> CResult<StructId> {
        let name = superclass.name();
        let Some(def) = self.struct_def_ids.get(name).copied() else {
            return Err(CheckError::at(at, format!("unknown type '{name}'")));
        };
        let parent = &self.struct_defs[def];
        if !parent.is_class() {
            return Err(CheckError::at(
                at,
                format!(
                    "'{name}' is not a class, so '{}' cannot inherit from it",
                    decl.name()
                ),
            ));
        }
        if name == decl.name()
            || self
                .instances
                .borrow()
                .struct_pending
                .iter()
                .any(|(pending, _)| *pending == def)
        {
            return Err(CheckError::at(
                at,
                format!(
                    "'{}' cannot inherit from itself, directly or not",
                    decl.name()
                ),
            ));
        }
        match self.resolve_type(superclass, scope, at)? {
            Ty::Class(id) => Ok(id),
            _ => Err(CheckError::at(
                at,
                format!(
                    "'{name}' is not a class, so '{}' cannot inherit from it",
                    decl.name()
                ),
            )),
        }
    }

    pub fn hierarchy_has_generic(&self, ty: Ty) -> bool {
        let mut current = Some(ty).filter(|t| t.is_class());
        while let Some(t) = current {
            if let Some((Kind::Struct, def, _)) = self.origin_of(t)
                && !self.struct_defs[def].generics().is_empty()
            {
                return true;
            }
            current = self.superclass(t);
        }
        false
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
                    TExpr::ClassDesc(ty),
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
        if sub == sup {
            return true;
        }
        if let Some(protocol) = self.existential_protocol(sup) {
            return sub.is_class()
                && self.existential_protocol(sub).is_none()
                && self.type_arguments(sub).is_empty()
                && self.conforms(sub, &protocol);
        }
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
        let bounds: Vec<Vec<String>> = (0..decl.generics().len())
            .map(|i| decl.bound(i).to_vec())
            .collect();
        self.check_type_bounds(&bounds, &key.1, decl.name(), line)?;
        if !self.instances.borrow_mut().enum_pending.insert(key.clone()) {
            return Err(CheckError::at(
                line,
                format!("an enum cannot contain itself: '{name}'"),
            ));
        }
        let cases = self.enum_cases(decl, &key.1, &name, line);
        self.instances.borrow_mut().enum_pending.remove(&key);
        let mut cases = cases?;
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
            let mut string_slots: Vec<Option<usize>> = vec![None; widest];
            let mut extra = 0;
            for (slot, string_slot) in string_slots.iter_mut().enumerate() {
                let (ty, mixed) = Self::slot_plan(&cases, slot).ok_or_else(|| {
                    CheckError::at(
                        at,
                        format!(
                            "the payloads at position {slot} of '{name}' must have the same type when it is a class or a struct"
                        ),
                    )
                })?;
                if mixed {
                    *string_slot = Some(1 + widest + extra);
                    extra += 1;
                }
                fields.push(FieldSig {
                    name: format!("payload{slot}"),
                    ty,
                    mutable: false,
                    unowned: false,
                    weak: false,
                });
            }
            for (slot, string_slot) in string_slots.iter().enumerate() {
                if string_slot.is_some() {
                    fields.push(FieldSig {
                        name: format!("payload_string{slot}"),
                        ty: Ty::Str,
                        mutable: false,
                        unowned: false,
                        weak: false,
                    });
                }
            }
            for case in &mut cases {
                case.slots = case
                    .payload
                    .iter()
                    .enumerate()
                    .map(|(position, ty)| match (ty, string_slots[position]) {
                        (Ty::Str, Some(slot)) => slot,
                        _ => 1 + position,
                    })
                    .collect();
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

    fn slot_plan(cases: &[CaseSig], slot: usize) -> Option<(Ty, bool)> {
        let types: Vec<Ty> = cases
            .iter()
            .filter_map(|c| c.payload.get(slot).copied())
            .collect();
        let first = *types.first()?;
        if types.iter().all(|t| *t == first) {
            return Some((first, false));
        }
        let mixed = types.contains(&Ty::Str);
        let others: Vec<Ty> = types.into_iter().filter(|t| *t != Ty::Str).collect();
        let head = *others.first()?;
        if others.iter().all(|t| *t == head) {
            Some((head, mixed))
        } else if others
            .iter()
            .any(|t| matches!(t, Ty::Class(_) | Ty::Struct(_)))
        {
            None
        } else {
            Some((Ty::Int, mixed))
        }
    }

    fn enum_cases(
        &self,
        decl: &EnumDecl,
        args: &[Ty],
        name: &str,
        line: u32,
    ) -> CResult<Vec<CaseSig>> {
        let bounds: Vec<Vec<String>> = (0..decl.generics().len())
            .map(|i| decl.bound(i).to_vec())
            .collect();
        let own_scope = self.type_scope(decl.generics(), &bounds, args);
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
            let slots = (0..payload.len()).map(|position| 1 + position).collect();
            cases.push(CaseSig {
                name: case.name().to_string(),
                payload,
                slots,
                labels: case.labels().to_vec(),
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

    pub fn array_element(&self, ty: Ty) -> Option<Ty> {
        match self.origin_of(ty)? {
            (Kind::Struct, 1, args) => args.first().copied(),
            _ => None,
        }
    }

    pub fn array_of(&self, element: Ty, line: u32) -> CResult<Ty> {
        let id = self.instantiate_struct(1, vec![element], &[], line)?;
        Ok(self.struct_ty(1, id))
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
        if let Some(element) = self.array_element(ty) {
            return format!("[{}]", self.display(element, scope));
        }
        match ty {
            Ty::Int => "Int".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Str => "String".to_string(),
            Ty::Char => "Character".to_string(),
            Ty::Void => "Void".to_string(),
            Ty::Param(index) => scope
                .iter()
                .find(|(_, t)| *t == Ty::Param(index))
                .map_or_else(|| format!("T{index}"), |(name, _)| name.clone()),
            Ty::Assoc(packed) => {
                let (index, id) = Ty::split_assoc(packed);
                scope.iter().find(|(_, t)| *t == ty).map_or_else(
                    || {
                        let name = self.assoc_types.get(id).map_or("?", |(_, n)| n.as_str());
                        format!("T{index}.{name}")
                    },
                    |(name, _)| name.clone(),
                )
            }
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
                Origin::Existential(_) => None,
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
            Ty::Param(_) | Ty::Assoc(_) => true,
            _ => self
                .origin_of(ty)
                .is_some_and(|(_, _, args)| args.iter().any(|a| self.has_param(*a))),
        }
    }

    pub fn unify(&self, pattern: Ty, actual: Ty, bound: &mut [Option<Ty>]) -> bool {
        if matches!(pattern, Ty::Assoc(_)) {
            return true;
        }
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
        if pattern.is_class()
            && let Some((_, pattern_def, _)) = self.origin_of(pattern)
        {
            let mut current = Some(actual);
            while let Some(candidate) = current {
                if self
                    .origin_of(candidate)
                    .is_some_and(|(_, d, _)| d == pattern_def)
                {
                    return self.unify_instances(pattern, candidate, bound);
                }
                current = self.superclass(candidate);
            }
        }
        self.unify_instances(pattern, actual, bound)
    }

    fn unify_instances(&self, pattern: Ty, actual: Ty, bound: &mut [Option<Ty>]) -> bool {
        match (self.origin_of(pattern), self.origin_of(actual)) {
            (Some((pk, pd, pargs)), Some((ak, ad, aargs))) if pk == ak && pd == ad => pargs
                .iter()
                .zip(aargs.iter())
                .all(|(p, a)| self.unify(*p, *a, bound)),
            _ => pattern == actual,
        }
    }

    pub fn subst(&self, ty: Ty, args: &[Ty], scope: &Scope, line: u32) -> CResult<Ty> {
        if let Ty::Assoc(packed) = ty {
            let (index, id) = Ty::split_assoc(packed);
            let base = args
                .get(index)
                .copied()
                .ok_or_else(|| CheckError::at(line, "internal error: unbound type parameter"))?;
            return match base {
                Ty::Param(p) => Ok(Ty::assoc(p, id)),
                Ty::Assoc(_) => Err(CheckError::at(
                    line,
                    "an associated type of an associated type is not supported yet",
                )),
                concrete => self.assoc_binding(concrete, id).ok_or_else(|| {
                    let (protocol, name) = self
                        .assoc_types
                        .get(id)
                        .map_or(("?", "?"), |(p, n)| (p.as_str(), n.as_str()));
                    CheckError::at(
                        line,
                        format!(
                            "'{}' does not define the associated type '{name}' of '{protocol}'",
                            self.type_name(concrete)
                        ),
                    )
                }),
            };
        }
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

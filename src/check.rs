use std::collections::HashMap;

use crate::ast::{
    Arg, BinOp, CaseLabel, Decl, EnumDecl, Expr, ExprKind, FuncDecl, Pattern, Stmt, StmtKind,
    StructDecl, SwitchCase, UnaryOp,
};
use crate::tir::{LocalId, TExpr, TFunc, TStmt, TStruct};
use crate::types::{EnumId, StructId, Ty};

const PRINT: &str = "print";

#[derive(Debug, Clone, PartialEq)]
pub struct CheckError {
    line: u32,
    message: String,
}

impl CheckError {
    fn at(line: u32, message: impl Into<String>) -> Self {
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

type CResult<T> = Result<T, CheckError>;

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
    params: Vec<ParamSig>,
    ret: Ty,
}

impl FuncSig {
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

    fn field(&self, name: &str) -> Option<(usize, &FieldSig)> {
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
    cases: Vec<CaseSig>,
    has_payload: bool,
    layout: Option<StructId>,
}

impl EnumSig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cases(&self) -> &[CaseSig] {
        &self.cases
    }

    fn case_index(&self, name: &str) -> Option<usize> {
        self.cases.iter().position(|c| c.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signatures {
    funcs: HashMap<String, FuncSig>,
    structs: Vec<StructSig>,
    struct_ids: HashMap<String, StructId>,
    enums: Vec<EnumSig>,
    enum_ids: HashMap<String, EnumId>,
}

impl Signatures {
    pub fn collect(decls: &[Decl]) -> CResult<Self> {
        let mut sigs = Self {
            funcs: HashMap::new(),
            structs: Vec::new(),
            struct_ids: HashMap::new(),
            enums: Vec::new(),
            enum_ids: HashMap::new(),
        };
        let enum_decls: Vec<&EnumDecl> = decls
            .iter()
            .filter_map(|d| match d {
                Decl::Enum(e) => Some(e),
                _ => None,
            })
            .collect();
        for decl in &enum_decls {
            sigs.register_enum_name(decl)?;
        }
        let struct_decls: Vec<&StructDecl> = decls
            .iter()
            .filter_map(|d| match d {
                Decl::Struct(s) => Some(s),
                Decl::Func(_) | Decl::Enum(_) => None,
            })
            .collect();
        for decl in &struct_decls {
            sigs.register_struct_name(decl)?;
        }
        for (index, decl) in enum_decls.iter().enumerate() {
            sigs.register_enum_cases(EnumId::new(index), decl)?;
        }
        for decl in &struct_decls {
            let fields = sigs.struct_fields(decl)?;
            sigs.structs.push(StructSig {
                name: decl.name().to_string(),
                fields,
            });
        }
        for index in 0..enum_decls.len() {
            sigs.register_enum_layout(EnumId::new(index));
        }
        for decl in decls {
            if let Decl::Func(func) = decl {
                sigs.register_func(func)?;
            }
        }
        Ok(sigs)
    }

    fn register_enum_name(&mut self, decl: &EnumDecl) -> CResult<()> {
        let id = EnumId::new(self.enums.len());
        if self.enum_ids.insert(decl.name().to_string(), id).is_some() {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate type '{}'", decl.name()),
            ));
        }
        self.enums.push(EnumSig {
            name: decl.name().to_string(),
            cases: Vec::new(),
            has_payload: decl.has_payload(),
            layout: None,
        });
        Ok(())
    }

    fn register_enum_cases(&mut self, id: EnumId, decl: &EnumDecl) -> CResult<()> {
        let mut cases: Vec<CaseSig> = Vec::new();
        for case in decl.cases() {
            if cases.iter().any(|c| c.name == case.name()) {
                return Err(CheckError::at(
                    decl.line(),
                    format!("duplicate case '{}' in '{}'", case.name(), decl.name()),
                ));
            }
            let payload = case
                .payload()
                .iter()
                .map(|name| self.payload_type(decl, name))
                .collect::<CResult<Vec<_>>>()?;
            cases.push(CaseSig {
                name: case.name().to_string(),
                payload,
            });
        }
        self.enums[id.index()].cases = cases;
        Ok(())
    }

    fn payload_type(&self, decl: &EnumDecl, name: &str) -> CResult<Ty> {
        if let Some(ty) = Ty::from_builtin_name(name) {
            return Ok(ty);
        }
        match self.enum_ids.get(name).map(|id| &self.enums[id.index()]) {
            Some(sig) if !sig.has_payload => self.resolve_type(name, decl.line()),
            Some(_) => Err(CheckError::at(
                decl.line(),
                format!("a payload cannot have the type '{name}' yet: it carries values"),
            )),
            None if self.struct_ids.contains_key(name) => Err(CheckError::at(
                decl.line(),
                format!(
                    "a payload cannot have a struct type yet: '{name}' in '{}'",
                    decl.name()
                ),
            )),
            None => Err(CheckError::at(
                decl.line(),
                format!("unknown payload type: '{name}' in '{}'", decl.name()),
            )),
        }
    }

    fn register_enum_layout(&mut self, id: EnumId) {
        let sig = &self.enums[id.index()];
        if !sig.has_payload {
            return;
        }
        let widest = sig.cases.iter().map(|c| c.payload.len()).max().unwrap_or(0);
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
        let layout = StructId::new(self.structs.len());
        let name = sig.name.clone();
        self.structs.push(StructSig { name, fields });
        self.enums[id.index()].layout = Some(layout);
    }

    pub fn enum_ty(&self, id: EnumId) -> Ty {
        match self.enums[id.index()].layout {
            Some(layout) => Ty::Struct(layout),
            None => Ty::Enum(id),
        }
    }

    pub fn enum_of(&self, ty: Ty) -> Option<&EnumSig> {
        match ty {
            Ty::Enum(id) => self.enum_sig(id),
            Ty::Struct(layout) => self.enums.iter().find(|e| e.layout == Some(layout)),
            _ => None,
        }
    }

    fn register_struct_name(&mut self, decl: &StructDecl) -> CResult<()> {
        if self.enum_ids.contains_key(decl.name()) {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate type '{}'", decl.name()),
            ));
        }
        let id = StructId::new(self.struct_ids.len());
        if self
            .struct_ids
            .insert(decl.name().to_string(), id)
            .is_some()
        {
            return Err(CheckError::at(
                decl.line(),
                format!("duplicate type '{}'", decl.name()),
            ));
        }
        Ok(())
    }

    fn struct_fields(&self, decl: &StructDecl) -> CResult<Vec<FieldSig>> {
        let mut fields: Vec<FieldSig> = Vec::new();
        for field in decl.fields() {
            let payload_free_enum = self
                .enum_ids
                .get(field.ty())
                .filter(|id| !self.enums[id.index()].has_payload)
                .map(|id| Ty::Enum(*id));
            let Some(ty) = Ty::from_builtin_name(field.ty()).or(payload_free_enum) else {
                let reason = if self.struct_ids.contains_key(field.ty())
                    || self.enum_ids.contains_key(field.ty())
                {
                    "a struct field cannot have a struct type yet"
                } else {
                    "unknown field type"
                };
                return Err(CheckError::at(
                    decl.line(),
                    format!("{reason}: '{}' in '{}'", field.ty(), decl.name()),
                ));
            };
            if fields.iter().any(|f| f.name == field.name()) {
                return Err(CheckError::at(
                    decl.line(),
                    format!("duplicate field '{}' in '{}'", field.name(), decl.name()),
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

    fn register_func(&mut self, func: &FuncDecl) -> CResult<()> {
        if func.name() == PRINT {
            return Err(CheckError::at(
                func.line(),
                "'print' is a reserved function name",
            ));
        }
        if self.struct_ids.contains_key(func.name()) || self.enum_ids.contains_key(func.name()) {
            return Err(CheckError::at(
                func.line(),
                format!("'{}' is already the name of a type", func.name()),
            ));
        }
        let sig = self.signature_of(func)?;
        if self.funcs.insert(func.name().to_string(), sig).is_some() {
            return Err(CheckError::at(
                func.line(),
                format!("duplicate function '{}'", func.name()),
            ));
        }
        Ok(())
    }

    fn signature_of(&self, func: &FuncDecl) -> CResult<FuncSig> {
        let params = func
            .params()
            .iter()
            .map(|p| {
                Ok(ParamSig {
                    label: p.label().map(str::to_string),
                    ty: self.resolve_type(p.ty(), func.line())?,
                })
            })
            .collect::<CResult<Vec<_>>>()?;
        let ret = match func.ret() {
            Some(name) => self.resolve_type(name, func.line())?,
            None => Ty::Void,
        };
        Ok(FuncSig { params, ret })
    }

    pub fn resolve_type(&self, name: &str, line: u32) -> CResult<Ty> {
        Ty::from_builtin_name(name)
            .or_else(|| self.struct_ids.get(name).map(|id| Ty::Struct(*id)))
            .or_else(|| self.enum_ids.get(name).map(|id| self.enum_ty(*id)))
            .ok_or_else(|| CheckError::at(line, format!("unknown type '{name}'")))
    }

    pub fn func(&self, name: &str) -> Option<&FuncSig> {
        self.funcs.get(name)
    }

    pub fn enum_sig(&self, id: EnumId) -> Option<&EnumSig> {
        self.enums.get(id.index())
    }

    pub fn struct_sig(&self, id: StructId) -> Option<&StructSig> {
        self.structs.get(id.index())
    }

    pub fn type_name(&self, ty: Ty) -> String {
        match ty {
            Ty::Int => "Int".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Str => "String".to_string(),
            Ty::Void => "Void".to_string(),
            Ty::Struct(id) => self
                .struct_sig(id)
                .map_or_else(String::new, |s| s.name.clone()),
            Ty::Enum(id) => self
                .enum_sig(id)
                .map_or_else(String::new, |e| e.name.clone()),
        }
    }

    pub fn layouts(&self) -> Vec<TStruct> {
        self.structs
            .iter()
            .map(|s| TStruct::new(s.name.clone(), s.fields.iter().map(|f| f.ty).collect()))
            .collect()
    }
}

pub fn check_func(func: &FuncDecl, sigs: &Signatures) -> CResult<TFunc> {
    let sig = sigs.func(func.name()).ok_or_else(|| {
        CheckError::at(
            func.line(),
            format!("function '{}' has no signature", func.name()),
        )
    })?;
    let mut cx = FnCx {
        sigs,
        ret: sig.ret,
        locals: Vec::new(),
        scopes: vec![HashMap::new()],
    };
    for (param, param_sig) in func.params().iter().zip(sig.params()) {
        cx.declare(func.line(), param.name(), param_sig.ty, false)?;
    }
    let body = cx.block(func.body())?;
    if sig.ret != Ty::Void && !always_returns(func.body()) {
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
        func.params().len(),
        sig.ret,
        cx.locals,
        body,
    ))
}

fn always_returns(stmts: &[Stmt]) -> bool {
    match stmts.last().map(Stmt::kind) {
        Some(StmtKind::Return(_)) => true,
        Some(StmtKind::If {
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

#[derive(Clone, Copy)]
struct Binding {
    local: LocalId,
    ty: Ty,
    mutable: bool,
}

struct FnCx<'a> {
    sigs: &'a Signatures,
    ret: Ty,
    locals: Vec<Ty>,
    scopes: Vec<HashMap<String, Binding>>,
}

impl FnCx<'_> {
    fn expect(&self, line: u32, expected: Ty, found: Ty) -> CResult<()> {
        if expected == found {
            Ok(())
        } else {
            Err(CheckError::at(
                line,
                format!(
                    "expected {}, but found {}",
                    self.sigs.type_name(expected),
                    self.sigs.type_name(found)
                ),
            ))
        }
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

    fn lower_stmt(&mut self, stmt: &Stmt, out: &mut Vec<TStmt>) -> CResult<()> {
        match stmt.kind() {
            StmtKind::Switch { subject, cases } => {
                self.switch_stmt(stmt.line(), subject, cases, out)
            }
            _ => {
                out.push(self.stmt(stmt)?);
                Ok(())
            }
        }
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
        let sigs = self.sigs;
        let Some(sig) = sigs.enum_of(subject_ty) else {
            return Err(CheckError::at(
                line,
                format!(
                    "switch is supported only on enums, not {}",
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
                CaseLabel::Patterns(patterns) => {
                    let mut condition: Option<TExpr> = None;
                    let mut prelude = Vec::new();
                    self.scopes.push(HashMap::new());
                    let outcome = self.case_patterns(
                        case,
                        patterns,
                        (sig, temporary, subject_ty),
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
        out.extend(tail.unwrap_or_default());
        Ok(())
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
            if pattern.type_name().is_some_and(|n| n != sig.name()) {
                return Err(CheckError::at(
                    case.line(),
                    format!("expected a case of '{}'", sig.name()),
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
            let tag = if sig.has_payload {
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
        if sig.has_payload {
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
                let value = self.value(init)?;
                if let Some(annotation) = ty {
                    self.expect(line, self.sigs.resolve_type(annotation, line)?, value.ty())?;
                }
                let local = self.declare(line, name, value.ty(), *mutable)?;
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
            StmtKind::While { cond, body } => {
                let cond = self.condition(cond)?;
                Ok(TStmt::While {
                    cond,
                    body: self.block(body)?,
                })
            }
            StmtKind::For { var, iter, body } => self.for_stmt(line, var, iter, body),
            StmtKind::Expr(expr) => self.expr_stmt(expr),
            StmtKind::Switch { .. } => Err(CheckError::at(
                line,
                "internal error: a switch must be lowered into a block",
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
        let binding = self.lookup(name, line)?;
        if !binding.mutable {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
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
        if !binding.mutable {
            return Err(CheckError::at(
                line,
                format!("cannot assign to a member of '{var}': it is a constant"),
            ));
        }
        let (index, field) = self.member_of(line, binding.ty, name)?;
        if !field.mutable {
            return Err(CheckError::at(
                line,
                format!("cannot assign to '{name}': it is a constant"),
            ));
        }
        let field_ty = field.ty;
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
            None => {
                self.expect(line, target_ty, rhs.ty())?;
                Ok(rhs)
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

    fn member_of(&self, line: u32, ty: Ty, name: &str) -> CResult<(usize, &FieldSig)> {
        let Ty::Struct(id) = ty else {
            return Err(CheckError::at(
                line,
                format!("a value of type {} has no members", self.sigs.type_name(ty)),
            ));
        };
        if self.sigs.enum_of(ty).is_some() {
            return Err(CheckError::at(
                line,
                format!("a value of type {} has no members", self.sigs.type_name(ty)),
            ));
        }
        let sig = self
            .sigs
            .struct_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown struct"))?;
        sig.field(name)
            .ok_or_else(|| CheckError::at(line, format!("'{}' has no member '{name}'", sig.name())))
    }

    fn return_stmt(&mut self, line: u32, value: Option<&Expr>) -> CResult<TStmt> {
        match (value, self.ret) {
            (None, Ty::Void) => Ok(TStmt::Return(None)),
            (None, _) => Err(CheckError::at(line, "this function must return a value")),
            (Some(_), Ty::Void) => Err(CheckError::at(
                line,
                "this function does not return a value",
            )),
            (Some(expr), ret) => {
                let value = self.value(expr)?;
                self.expect(line, ret, value.ty())?;
                Ok(TStmt::Return(Some(value)))
            }
        }
    }

    fn for_stmt(&mut self, line: u32, var: &str, iter: &Expr, body: &[Stmt]) -> CResult<TStmt> {
        let ExprKind::Range { lo, hi, inclusive } = iter.kind() else {
            return Err(CheckError::at(line, "a for loop iterates over a range"));
        };
        let lo = self.int_value(lo)?;
        let hi = self.int_value(hi)?;
        self.scopes.push(HashMap::new());
        let result = self.declare(line, var, Ty::Int, false).and_then(|local| {
            Ok(TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive: *inclusive,
                body: self.block(body)?,
            })
        });
        self.scopes.pop();
        result
    }

    fn expr_stmt(&mut self, expr: &Expr) -> CResult<TStmt> {
        let ExprKind::Call { callee, args } = expr.kind() else {
            return Err(CheckError::at(
                expr.line(),
                "the value of this expression is unused",
            ));
        };
        if callee == PRINT {
            return self.print(expr.line(), args);
        }
        Ok(TStmt::Eval(self.call(expr.line(), callee, args)?))
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
                format!(
                    "cannot print a value of type {}",
                    self.sigs.type_name(value.ty())
                ),
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
        match expr.kind() {
            ExprKind::Int(n) => Ok(TExpr::Int(*n)),
            ExprKind::Bool(b) => Ok(TExpr::Bool(*b)),
            ExprKind::Str(text) if text.contains('\0') => Err(CheckError::at(
                line,
                "a string literal cannot contain a NUL character",
            )),
            ExprKind::Str(text) => Ok(TExpr::Str(text.clone())),
            ExprKind::Ident(name) => {
                let binding = self.lookup(name, line)?;
                Ok(TExpr::Local(binding.local, binding.ty))
            }
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
                self.operands(line, *op, lhs.ty(), rhs.ty())?;
                Ok(TExpr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                })
            }
            ExprKind::Call { callee, .. } if callee == PRINT => {
                Err(CheckError::at(line, "print does not produce a value"))
            }
            ExprKind::Call { callee, args } => self.call(line, callee, args),
            ExprKind::Member { base, name } if self.names_enum(base) => {
                self.enum_case(line, base, name)
            }
            ExprKind::MemberCall { base, name, args } if self.names_enum(base) => {
                self.enum_construct(line, base, name, args)
            }
            ExprKind::MemberCall { .. } => Err(CheckError::at(
                line,
                "only an enum case can be called through a type name",
            )),
            ExprKind::Member { base, name } => {
                let base = self.value(base)?;
                let (index, field) = self.member_of(line, base.ty(), name)?;
                let ty = field.ty;
                Ok(TExpr::Field {
                    base: Box::new(base),
                    index,
                    ty,
                })
            }
            ExprKind::Range { .. } => Err(CheckError::at(
                line,
                "a range can only be used in a for loop",
            )),
        }
    }

    fn names_enum(&self, base: &Expr) -> bool {
        match base.kind() {
            ExprKind::Ident(name) => {
                self.lookup(name, base.line()).is_err() && self.sigs.enum_ids.contains_key(name)
            }
            _ => false,
        }
    }

    fn enum_named<'s>(
        sigs: &'s Signatures,
        line: u32,
        base: &Expr,
    ) -> CResult<(EnumId, &'s EnumSig)> {
        let ExprKind::Ident(type_name) = base.kind() else {
            return Err(CheckError::at(line, "internal error: not an enum name"));
        };
        let id = sigs
            .enum_ids
            .get(type_name)
            .copied()
            .ok_or_else(|| CheckError::at(line, "internal error: unknown enum"))?;
        let sig = sigs
            .enum_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown enum"))?;
        Ok((id, sig))
    }

    fn enum_case(&self, line: u32, base: &Expr, name: &str) -> CResult<TExpr> {
        let (id, sig) = Self::enum_named(self.sigs, line, base)?;
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
        let ty = self.sigs.enum_ty(id);
        if sig.has_payload {
            Ok(TExpr::StructInit {
                ty,
                args: vec![TExpr::Int(index as i64)],
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
    ) -> CResult<TExpr> {
        let sigs = self.sigs;
        let (id, sig) = Self::enum_named(sigs, line, base)?;
        let index = sig.case_index(name).ok_or_else(|| {
            CheckError::at(line, format!("'{}' has no case '{name}'", sig.name()))
        })?;
        let payload = sig.cases()[index].payload();
        if args.len() != payload.len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}.{name}' takes {} value(s), but {} were given",
                    sig.name(),
                    payload.len(),
                    args.len()
                ),
            ));
        }
        let mut values = vec![TExpr::Int(index as i64)];
        for (arg, expected) in args.iter().zip(payload) {
            if arg.label().is_some() {
                return Err(CheckError::at(line, "a payload value cannot be labeled"));
            }
            let value = self.value(arg.value())?;
            self.expect(line, *expected, value.ty())?;
            values.push(value);
        }
        Ok(TExpr::StructInit {
            ty: sigs.enum_ty(id),
            args: values,
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
                if matches!(lhs, Ty::Int | Ty::Bool | Ty::Enum(_)) {
                    Ok(())
                } else {
                    Err(CheckError::at(
                        line,
                        format!(
                            "equality is not supported for {} yet",
                            self.sigs.type_name(lhs)
                        ),
                    ))
                }
            }
            BinOp::And | BinOp::Or => {
                self.expect(line, Ty::Bool, lhs)?;
                self.expect(line, Ty::Bool, rhs)
            }
        }
    }

    fn call(&mut self, line: u32, callee: &str, args: &[Arg]) -> CResult<TExpr> {
        let sigs = self.sigs;
        if let Some(id) = sigs.struct_ids.get(callee) {
            return self.struct_init(line, *id, args);
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
            if arg.label() != param.label() {
                return Err(CheckError::at(
                    line,
                    format!(
                        "argument label {:?} does not match the expected label {:?}",
                        arg.label(),
                        param.label()
                    ),
                ));
            }
            let value = self.value(arg.value())?;
            self.expect(line, param.ty(), value.ty())?;
            checked.push(value);
        }
        Ok(TExpr::Call {
            callee: callee.to_string(),
            args: checked,
            ret: sig.ret(),
        })
    }

    fn struct_init(&mut self, line: u32, id: StructId, args: &[Arg]) -> CResult<TExpr> {
        let sigs = self.sigs;
        let sig = sigs
            .struct_sig(id)
            .ok_or_else(|| CheckError::at(line, "internal error: unknown struct"))?;
        if args.len() != sig.fields().len() {
            return Err(CheckError::at(
                line,
                format!(
                    "'{}' takes {} member(s), but {} were given",
                    sig.name(),
                    sig.fields().len(),
                    args.len()
                ),
            ));
        }
        let mut checked = Vec::with_capacity(args.len());
        for (arg, field) in args.iter().zip(sig.fields()) {
            if arg.label() != Some(field.name()) {
                return Err(CheckError::at(
                    line,
                    format!(
                        "expected the label '{}' here, in the order the members are declared",
                        field.name()
                    ),
                ));
            }
            let value = self.value(arg.value())?;
            self.expect(line, field.ty(), value.ty())?;
            checked.push(value);
        }
        Ok(TExpr::StructInit {
            ty: Ty::Struct(id),
            args: checked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn strings_cannot_be_compared_or_combined_yet() {
        assert!(check_main("print(\"a\" == \"b\")").is_err());
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
    fn structs_cannot_be_compared_or_printed_as_a_whole() {
        assert!(check_with_point("let a = P(x: 1, tag: 2)\nprint(a == a)").is_err());
        assert!(check_with_point("print(P(x: 1, tag: 2))").is_err());
    }

    #[test]
    fn a_struct_name_cannot_be_reused() {
        assert!(check_all("struct P { var x: Int }\nstruct P { var y: Int }").is_err());
        assert!(check_all("struct P { var x: Int }\nfunc P() { }").is_err());
    }

    #[test]
    fn struct_fields_must_have_a_supported_type() {
        assert!(check_all("struct Q { var a: Int }\nstruct P { var q: Q }").is_err());
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
    fn switching_on_a_value_that_is_not_an_enum_is_rejected() {
        assert!(check_all(&with_color("switch 1 {\ndefault: print(2)\n}")).is_err());
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
    fn a_payload_cannot_be_a_struct_or_an_enum_that_carries_values() {
        assert!(check_all("struct P { var x: Int }\nenum E { case a(P) }").is_err());
        assert!(check_all("enum A { case x(Int) }\nenum B { case y(A) }").is_err());
        assert!(check_all("enum E { case a(Missing) }").is_err());
    }

    #[test]
    fn a_payload_may_be_an_enum_without_values() {
        assert!(check_all("enum Color { case red }\nenum E { case a(Color) }").is_ok());
    }

    #[test]
    fn a_struct_field_cannot_be_an_enum_that_carries_values() {
        let source = format!("{SHAPE}struct P {{ var s: Shape }}");
        assert!(check_all(&source).is_err());
    }

    #[test]
    fn a_payload_enum_can_be_a_parameter_a_result_and_an_annotation() {
        let source = format!(
            "{SHAPE}func make() -> Shape {{ return Shape.circle(1) }}\nfunc use(_ s: Shape) {{ let t: Shape = s }}\nfunc main() {{ use(make()) }}"
        );
        assert!(check_all(&source).is_ok());
    }
}

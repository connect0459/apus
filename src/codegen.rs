use std::collections::HashMap;
use std::sync::Arc;

use cranelift_codegen::Context;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::Imm64;
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, ExtFuncData, ExternalName, FuncRef, GlobalValue, GlobalValueData,
    InstBuilder, MemFlagsData, Signature, StackSlotData, StackSlotKind, TrapCode, Type,
    UserExternalName, Value, types,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{
    DataDescription, DataId, FuncId, Linkage, Module, ModuleReloc, ModuleRelocTarget,
    default_libcall_names,
};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ast::{BinOp, UnaryOp};
use crate::entry::{EntryReloc, FuncEntry, Literal, SymbolRef};
use crate::hash::Fnv1a;
use crate::tir::{DROP_SLOT, RC_SLOT, TExpr, TFunc, TStmt, TStruct, UNOWNED_SLOT};
use crate::types::Ty;

const VARIADIC_PADDING: usize = 7;
const FIELD_SIZE: u32 = 8;
const UNREACHABLE: TrapCode = TrapCode::unwrap_user(1);
const UNOWNED_DEAD: TrapCode = TrapCode::unwrap_user(2);
const UNWRAPPED_NIL: TrapCode = TrapCode::unwrap_user(3);
const NAMESPACE_FUNC: u32 = 0;
const NAMESPACE_DATA: u32 = 1;

pub const ENTRY_SYMBOL: &str = "apus_main";
const RUNTIME_FORMAT_INT: &str = "apus_format_int";
const RUNTIME_TRUE: &str = "apus_true";
const RUNTIME_FALSE: &str = "apus_false";
const RUNTIME_DATA: [(&str, &[u8]); 3] = [
    (RUNTIME_FORMAT_INT, b"%ld\n\0"),
    (RUNTIME_TRUE, b"true\0"),
    (RUNTIME_FALSE, b"false\0"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct CodegenError {
    message: String,
}

impl CodegenError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl<E: std::fmt::Display> From<E> for CodegenError {
    fn from(error: E) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

type GResult<T> = Result<T, CodegenError>;

pub fn symbol(name: &str) -> String {
    format!("apus_{name}")
}

pub fn compile(funcs: &[TFunc], structs: &[TStruct]) -> GResult<Vec<u8>> {
    let backend = Backend::new()?;
    let entries = funcs
        .iter()
        .map(|func| backend.compile_function(func, structs))
        .collect::<GResult<Vec<_>>>()?;
    backend.assemble(&entries)
}

pub struct Backend {
    isa: Arc<dyn TargetIsa>,
}

impl Backend {
    pub fn new() -> GResult<Self> {
        let mut flags = settings::builder();
        flags.set("opt_level", "none")?;
        flags.set("is_pic", "true")?;
        let isa = cranelift_native::builder()?.finish(settings::Flags::new(flags))?;
        Ok(Self { isa })
    }

    pub fn compile_function(&self, func: &TFunc, structs: &[TStruct]) -> GResult<FuncEntry> {
        let mut ctx = Context::new();
        let mut builder_ctx = FunctionBuilderContext::new();
        self.compile_with(&mut ctx, &mut builder_ctx, func, structs)
    }

    pub fn compile_many(
        &self,
        funcs: &[&TFunc],
        structs: &[TStruct],
        threads: usize,
    ) -> GResult<Vec<FuncEntry>> {
        let threads = threads.clamp(1, funcs.len().max(1));
        if threads == 1 {
            return self.compile_chunk(funcs, structs);
        }
        let chunk_size = funcs.len().div_ceil(threads);
        std::thread::scope(|scope| {
            let handles: Vec<_> = funcs
                .chunks(chunk_size)
                .map(|chunk| scope.spawn(move || self.compile_chunk(chunk, structs)))
                .collect();
            let mut entries = Vec::with_capacity(funcs.len());
            for handle in handles {
                let chunk = handle
                    .join()
                    .map_err(|_| CodegenError::from("a compilation thread panicked"))??;
                entries.extend(chunk);
            }
            Ok(entries)
        })
    }

    fn compile_chunk(&self, funcs: &[&TFunc], structs: &[TStruct]) -> GResult<Vec<FuncEntry>> {
        let mut ctx = Context::new();
        let mut builder_ctx = FunctionBuilderContext::new();
        funcs
            .iter()
            .map(|func| self.compile_with(&mut ctx, &mut builder_ctx, func, structs))
            .collect()
    }

    fn compile_with(
        &self,
        ctx: &mut Context,
        builder_ctx: &mut FunctionBuilderContext,
        func: &TFunc,
        structs: &[TStruct],
    ) -> GResult<FuncEntry> {
        ctx.clear();
        let call_conv = self.isa.default_call_conv();
        ctx.func.signature = signature(call_conv, func.param_types(), func.ret());
        let (symbols, literals) = emit_function(&*self.isa, ctx, builder_ctx, structs, func)?;
        ctx.compile(&*self.isa, &mut ControlPlane::default())
            .map_err(|e| CodegenError::from(format!("{:?}", e.inner)))?;
        let compiled = ctx
            .compiled_code()
            .ok_or_else(|| CodegenError::from("compilation produced no code"))?;
        let alignment = u64::from(compiled.buffer.min_alignment);
        let code = compiled.buffer.data().to_vec();
        let relocs = compiled
            .buffer
            .relocs()
            .iter()
            .map(|reloc| entry_reloc(reloc, &ctx.func, &symbols))
            .collect::<GResult<Vec<_>>>()?;
        Ok(FuncEntry::new(
            symbol(func.name()),
            alignment,
            code,
            relocs,
            literals,
        ))
    }

    pub fn assemble(&self, entries: &[FuncEntry]) -> GResult<Vec<u8>> {
        let builder = ObjectBuilder::new(self.isa.clone(), "apus", default_libcall_names())?;
        let mut module = ObjectModule::new(builder);
        let mut funcs: HashMap<String, FuncId> = HashMap::new();
        let mut data: HashMap<String, DataId> = HashMap::new();
        let placeholder = module.make_signature();
        for name in ["printf", "puts", "malloc", "free"] {
            let id = module.declare_function(name, Linkage::Import, &placeholder)?;
            funcs.insert(name.to_string(), id);
        }
        for (name, bytes) in RUNTIME_DATA {
            data.insert(name.to_string(), define_text(&mut module, name, bytes)?);
        }
        for entry in entries {
            let id = module.declare_function(entry.symbol(), Linkage::Export, &placeholder)?;
            funcs.insert(entry.symbol().to_string(), id);
            for literal in entry.literals() {
                if !data.contains_key(literal.name()) {
                    let id = define_text(&mut module, literal.name(), literal.bytes())?;
                    data.insert(literal.name().to_string(), id);
                }
            }
        }
        for entry in entries {
            let relocs = entry
                .relocs()
                .iter()
                .map(|r| module_reloc(r, &funcs, &data))
                .collect::<GResult<Vec<_>>>()?;
            module.define_function_bytes(
                funcs[entry.symbol()],
                entry.alignment(),
                entry.code(),
                &relocs,
            )?;
        }
        define_entry(&mut module, &self.isa, &funcs)?;
        Ok(module.finish().emit()?)
    }
}

fn module_reloc(
    reloc: &EntryReloc,
    funcs: &HashMap<String, FuncId>,
    data: &HashMap<String, DataId>,
) -> GResult<ModuleReloc> {
    let name = match reloc.target() {
        SymbolRef::Func(name) => {
            let id = funcs
                .get(name)
                .ok_or_else(|| CodegenError::from(format!("unresolved function '{name}'")))?;
            ModuleRelocTarget::user(NAMESPACE_FUNC, id.as_u32())
        }
        SymbolRef::Data(name) => {
            let id = data
                .get(name)
                .ok_or_else(|| CodegenError::from(format!("unresolved data '{name}'")))?;
            ModuleRelocTarget::user(NAMESPACE_DATA, id.as_u32())
        }
    };
    Ok(ModuleReloc {
        offset: reloc.offset(),
        kind: reloc.kind(),
        name,
        addend: reloc.addend(),
    })
}

fn entry_reloc(
    reloc: &cranelift_codegen::MachReloc,
    func: &cranelift_codegen::ir::Function,
    symbols: &[SymbolRef],
) -> GResult<EntryReloc> {
    let module_reloc = ModuleReloc::from_mach_reloc(reloc, func, FuncId::from_u32(0));
    let ModuleRelocTarget::User { index, .. } = module_reloc.name else {
        return Err(CodegenError::from("unsupported relocation target"));
    };
    let target = symbols
        .get(index as usize)
        .cloned()
        .ok_or_else(|| CodegenError::from("relocation refers to an unknown symbol"))?;
    Ok(EntryReloc::new(
        module_reloc.offset,
        module_reloc.kind,
        target,
        module_reloc.addend,
    ))
}

fn clif_type(ty: Ty) -> Option<Type> {
    match ty {
        Ty::Int | Ty::Str | Ty::Struct(_) | Ty::Class(_) | Ty::Enum(_) => Some(types::I64),
        Ty::Bool => Some(types::I8),
        Ty::Void | Ty::Param(_) => None,
    }
}

fn return_type(ret: Ty) -> Option<Type> {
    if ret.is_struct() {
        None
    } else {
        clif_type(ret)
    }
}

fn signature(call_conv: cranelift_codegen::isa::CallConv, params: &[Ty], ret: Ty) -> Signature {
    let mut sig = Signature::new(call_conv);
    if ret.is_struct() {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.params.extend(
        params
            .iter()
            .filter_map(|t| clif_type(*t))
            .map(AbiParam::new),
    );
    sig.returns.extend(return_type(ret).map(AbiParam::new));
    sig
}

fn define_text(module: &mut ObjectModule, name: &str, bytes: &[u8]) -> GResult<DataId> {
    let id = module.declare_data(name, Linkage::Local, false, false)?;
    let mut data = DataDescription::new();
    data.define(bytes.to_vec().into_boxed_slice());
    module.define_data(id, &data)?;
    Ok(id)
}

#[derive(Default)]
struct Symbols {
    table: Vec<SymbolRef>,
    funcs: HashMap<String, FuncRef>,
    data: HashMap<String, GlobalValue>,
}

fn emit_function(
    isa: &dyn TargetIsa,
    ctx: &mut Context,
    builder_ctx: &mut FunctionBuilderContext,
    structs: &[TStruct],
    func: &TFunc,
) -> GResult<(Vec<SymbolRef>, Vec<Literal>)> {
    let target = isa.frontend_config();
    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    let vars: Vec<Variable> = func
        .locals()
        .iter()
        .map(|ty| clif_type(*ty).map(|t| builder.declare_var(t)))
        .collect::<Option<_>>()
        .ok_or_else(|| CodegenError::from("a local variable has no representable type"))?;
    let mut params = builder.block_params(entry).to_vec();
    let sret = if func.ret().is_struct() {
        Some(params.remove(0))
    } else {
        None
    };
    let param_count = params.len();
    for (var, value) in vars.iter().zip(params) {
        builder.def_var(*var, value);
    }
    for (var, ty) in vars.iter().zip(func.locals()).skip(param_count) {
        if matches!(ty, Ty::Class(_) | Ty::Struct(_)) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(*var, zero);
        }
    }
    let mut emitter = Emitter {
        call_conv: isa.default_call_conv(),
        structs,
        builder,
        vars,
        local_types: func.locals().to_vec(),
        ret: func.ret(),
        sret,
        symbols: Symbols::default(),
        literals: Vec::new(),
    };
    let terminated = emitter.stmts(func.body())?;
    if !terminated {
        if func.ret() == Ty::Void {
            emitter.builder.ins().return_(&[]);
        } else {
            emitter.builder.ins().trap(UNREACHABLE);
        }
    }
    emitter.builder.seal_all_blocks();
    let Emitter {
        builder,
        symbols,
        literals,
        ..
    } = emitter;
    builder.finalize(target);
    Ok((symbols.table, literals))
}

struct Emitter<'a, 'f> {
    call_conv: cranelift_codegen::isa::CallConv,
    structs: &'a [TStruct],
    builder: FunctionBuilder<'f>,
    vars: Vec<Variable>,
    local_types: Vec<Ty>,
    ret: Ty,
    sret: Option<Value>,
    symbols: Symbols,
    literals: Vec<Literal>,
}

fn field_offset(index: usize) -> i32 {
    i32::try_from(index).map_or(i32::MAX, |i| i.saturating_mul(FIELD_SIZE as i32))
}

impl Emitter<'_, '_> {
    fn import_func(&mut self, name: &str, sig: Signature, colocated: bool) -> FuncRef {
        if let Some(existing) = self.symbols.funcs.get(name) {
            return *existing;
        }
        let index = u32::try_from(self.symbols.table.len()).unwrap_or(u32::MAX);
        self.symbols.table.push(SymbolRef::Func(name.to_string()));
        let signature = self.builder.func.import_signature(sig);
        let user = self
            .builder
            .func
            .declare_imported_user_function(UserExternalName {
                namespace: NAMESPACE_FUNC,
                index,
            });
        let func_ref = self.builder.func.import_function(ExtFuncData {
            name: ExternalName::user(user),
            signature,
            colocated,
            patchable: false,
        });
        self.symbols.funcs.insert(name.to_string(), func_ref);
        func_ref
    }

    fn import_data(&mut self, name: &str) -> GlobalValue {
        if let Some(existing) = self.symbols.data.get(name) {
            return *existing;
        }
        let index = u32::try_from(self.symbols.table.len()).unwrap_or(u32::MAX);
        self.symbols.table.push(SymbolRef::Data(name.to_string()));
        let user = self
            .builder
            .func
            .declare_imported_user_function(UserExternalName {
                namespace: NAMESPACE_DATA,
                index,
            });
        let global = self
            .builder
            .func
            .create_global_value(GlobalValueData::Symbol {
                name: ExternalName::user(user),
                offset: Imm64::new(0),
                colocated: true,
                tls: false,
            });
        self.symbols.data.insert(name.to_string(), global);
        global
    }

    fn data_address(&mut self, name: &str) -> Value {
        let global = self.import_data(name);
        self.builder.ins().symbol_value(types::I64, global)
    }

    fn string_address(&mut self, text: &str) -> Value {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let mut hasher = Fnv1a::new();
        hasher.write(&bytes);
        let name = format!("apus_str_{:016x}", hasher.finish());
        if !self.literals.iter().any(|l| l.name() == name) {
            self.literals.push(Literal::new(name.clone(), bytes));
        }
        self.data_address(&name)
    }

    fn struct_fields(&self, ty: Ty) -> GResult<&[Ty]> {
        match ty {
            Ty::Struct(id) | Ty::Class(id) => self
                .structs
                .get(id.index())
                .map(TStruct::fields)
                .ok_or_else(|| CodegenError::from("unknown struct")),
            other => Err(CodegenError::from(format!("{other:?} is not a struct"))),
        }
    }

    fn slot_start(&self, ty: Ty, index: usize) -> usize {
        match ty {
            Ty::Struct(id) | Ty::Class(id) => self
                .structs
                .get(id.index())
                .and_then(|s| s.starts().get(index).copied())
                .unwrap_or(index),
            _ => index,
        }
    }

    fn store_field(
        &mut self,
        dest: Value,
        owner: Ty,
        index: usize,
        value: Value,
        value_ty: Ty,
    ) -> GResult<()> {
        let offset = field_offset(self.slot_start(owner, index));
        if value_ty.is_struct() {
            let target = self.builder.ins().iadd_imm_s(dest, i64::from(offset));
            return self.copy_struct(target, value, value_ty);
        }
        self.builder
            .ins()
            .store(MemFlagsData::trusted(), value, dest, offset);
        Ok(())
    }

    fn alloc_struct(&mut self, ty: Ty) -> GResult<Value> {
        let count = u32::try_from(self.struct_fields(ty)?.len())?;
        let size = count.max(1) * FIELD_SIZE;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        Ok(self.builder.ins().stack_addr(types::I64, slot, 0))
    }

    fn alloc_object(&mut self, ty: Ty) -> GResult<Value> {
        let count = u32::try_from(self.struct_fields(ty)?.len())?;
        let size = i64::from(count.max(1) * FIELD_SIZE);
        let size = self.builder.ins().iconst(types::I64, size);
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let malloc = self.import_func("malloc", sig, false);
        let call = self.builder.ins().call(malloc, &[size]);
        self.builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| CodegenError::from("malloc returned no value"))
    }

    fn copy_struct(&mut self, dest: Value, src: Value, ty: Ty) -> GResult<()> {
        let fields = self.struct_fields(ty)?.to_vec();
        let flags = MemFlagsData::trusted();
        for (index, field) in fields.iter().enumerate() {
            let clif = clif_type(*field)
                .ok_or_else(|| CodegenError::from("a field has no representable type"))?;
            let offset = field_offset(index);
            let value = self.builder.ins().load(clif, flags, src, offset);
            self.builder.ins().store(flags, value, dest, offset);
        }
        Ok(())
    }

    fn stmts(&mut self, stmts: &[TStmt]) -> GResult<bool> {
        for stmt in stmts {
            if self.stmt(stmt)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn stmt(&mut self, stmt: &TStmt) -> GResult<bool> {
        match stmt {
            TStmt::Let { local, init } => {
                let value = self.value(init)?;
                let ty = self.local_types[local.index()];
                let stored = if ty.is_struct() {
                    let dest = self.alloc_struct(ty)?;
                    self.copy_struct(dest, value, ty)?;
                    dest
                } else {
                    value
                };
                self.builder.def_var(self.vars[local.index()], stored);
                Ok(false)
            }
            TStmt::Assign { local, value } => {
                let value = self.value(value)?;
                let ty = self.local_types[local.index()];
                let var = self.vars[local.index()];
                if ty.is_struct() {
                    let dest = self.builder.use_var(var);
                    self.copy_struct(dest, value, ty)?;
                } else {
                    self.builder.def_var(var, value);
                }
                Ok(false)
            }
            TStmt::AssignField {
                local,
                index,
                value,
            } => {
                let value_ty = value.ty();
                let value = self.value(value)?;
                let base = self.builder.use_var(self.vars[local.index()]);
                let owner = self.local_types[local.index()];
                self.store_field(base, owner, *index, value, value_ty)?;
                Ok(false)
            }
            TStmt::Return(value) => {
                self.return_stmt(value.as_ref())?;
                Ok(true)
            }
            TStmt::If {
                cond,
                then_body,
                else_body,
            } => self.if_stmt(cond, then_body, else_body),
            TStmt::While { cond, body } => self.while_stmt(cond, body),
            TStmt::ForRange {
                local,
                lo,
                hi,
                inclusive,
                body,
            } => self.for_stmt(self.vars[local.index()], lo, hi, *inclusive, body),
            TStmt::Print(expr) => {
                self.print(expr)?;
                Ok(false)
            }
            TStmt::Eval(expr) => {
                self.expr(expr)?;
                Ok(false)
            }
            TStmt::Release(expr) => {
                let value = self.value(expr)?;
                self.release(value, expr.ty())?;
                Ok(false)
            }
            TStmt::Clear(local) => {
                let zero = self.builder.ins().iconst(types::I64, 0);
                self.builder.def_var(self.vars[local.index()], zero);
                Ok(false)
            }
            TStmt::Free(expr) => {
                let value = self.value(expr)?;
                self.free_pointer(value);
                Ok(false)
            }
            TStmt::ReleaseUnowned(expr) => {
                let value = self.value(expr)?;
                self.release_unowned(value)?;
                Ok(false)
            }
        }
    }

    fn class_slots(&self, ty: Ty) -> GResult<Vec<usize>> {
        Ok(self
            .struct_fields(ty)?
            .iter()
            .enumerate()
            .filter(|(_, f)| matches!(f, Ty::Class(_)))
            .map(|(index, _)| index)
            .collect())
    }

    fn guarded(
        &mut self,
        pointer: Value,
        body: impl FnOnce(&mut Self) -> GResult<()>,
    ) -> GResult<()> {
        let live = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.ins().brif(pointer, live, &[], done, &[]);
        self.builder.switch_to_block(live);
        body(self)?;
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(())
    }

    fn free_pointer(&mut self, value: Value) {
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        let free = self.import_func("free", sig, false);
        self.builder.ins().call(free, &[value]);
    }

    fn retain_unowned(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let offset = field_offset(UNOWNED_SLOT);
            let count = this.builder.ins().load(types::I64, flags, value, offset);
            let next = this.builder.ins().iadd_imm_s(count, 1);
            this.builder.ins().store(flags, next, value, offset);
            Ok(())
        })
    }

    fn release_unowned(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let offset = field_offset(UNOWNED_SLOT);
            let count = this.builder.ins().load(types::I64, flags, value, offset);
            let next = this.builder.ins().iadd_imm_s(count, -1);
            this.builder.ins().store(flags, next, value, offset);
            let none_left = this.builder.ins().icmp_imm_s(IntCC::Equal, next, 0);
            let check = this.builder.create_block();
            let release = this.builder.create_block();
            let done = this.builder.create_block();
            this.builder.ins().brif(none_left, check, &[], done, &[]);
            this.builder.switch_to_block(check);
            let strong = this
                .builder
                .ins()
                .load(types::I64, flags, value, field_offset(RC_SLOT));
            let dead = this.builder.ins().icmp_imm_s(IntCC::Equal, strong, 0);
            this.builder.ins().brif(dead, release, &[], done, &[]);
            this.builder.switch_to_block(release);
            this.free_pointer(value);
            this.builder.ins().jump(done, &[]);
            this.builder.switch_to_block(done);
            Ok(())
        })
    }

    fn load_unowned(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let offset = field_offset(RC_SLOT);
            let count = this.builder.ins().load(types::I64, flags, value, offset);
            this.builder.ins().trapz(count, UNOWNED_DEAD);
            let next = this.builder.ins().iadd_imm_s(count, 1);
            this.builder.ins().store(flags, next, value, offset);
            Ok(())
        })
    }

    fn retain(&mut self, value: Value, ty: Ty) -> GResult<()> {
        match ty {
            Ty::Class(_) => self.retain_reference(value),
            Ty::Struct(_) => {
                for field in self.load_class_slots(value, ty)? {
                    self.retain_reference(field)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn release(&mut self, value: Value, ty: Ty) -> GResult<()> {
        match ty {
            Ty::Class(_) => self.release_reference(value),
            Ty::Struct(_) => {
                if self.class_slots(ty)?.is_empty() {
                    return Ok(());
                }
                self.guarded(value, |this| {
                    for field in this.load_class_slots(value, ty)? {
                        this.release_reference(field)?;
                    }
                    Ok(())
                })
            }
            _ => Ok(()),
        }
    }

    fn load_class_slots(&mut self, value: Value, ty: Ty) -> GResult<Vec<Value>> {
        let flags = MemFlagsData::trusted();
        Ok(self
            .class_slots(ty)?
            .into_iter()
            .map(|slot| {
                self.builder
                    .ins()
                    .load(types::I64, flags, value, field_offset(slot))
            })
            .collect())
    }

    fn retain_reference(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let offset = field_offset(RC_SLOT);
            let count = this.builder.ins().load(types::I64, flags, value, offset);
            let next = this.builder.ins().iadd_imm_s(count, 1);
            this.builder.ins().store(flags, next, value, offset);
            Ok(())
        })
    }

    fn release_reference(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let offset = field_offset(RC_SLOT);
            let count = this.builder.ins().load(types::I64, flags, value, offset);
            let last = this.builder.ins().icmp_imm_s(IntCC::Equal, count, 1);
            let destroy = this.builder.create_block();
            let decrement = this.builder.create_block();
            let merge = this.builder.create_block();
            this.builder.ins().brif(last, destroy, &[], decrement, &[]);
            this.builder.switch_to_block(destroy);
            let drop_fn =
                this.builder
                    .ins()
                    .load(types::I64, flags, value, field_offset(DROP_SLOT));
            let mut sig = Signature::new(this.call_conv);
            sig.params.push(AbiParam::new(types::I64));
            let sig = this.builder.import_signature(sig);
            this.builder.ins().call_indirect(sig, drop_fn, &[value]);
            this.builder.ins().jump(merge, &[]);
            this.builder.switch_to_block(decrement);
            let next = this.builder.ins().iadd_imm_s(count, -1);
            this.builder.ins().store(flags, next, value, offset);
            this.builder.ins().jump(merge, &[]);
            this.builder.switch_to_block(merge);
            Ok(())
        })
    }

    fn return_stmt(&mut self, value: Option<&TExpr>) -> GResult<()> {
        match (value, self.sret) {
            (Some(expr), Some(sret)) => {
                let src = self.value(expr)?;
                self.copy_struct(sret, src, self.ret)?;
                self.builder.ins().return_(&[]);
            }
            (Some(expr), None) => {
                let value = self.value(expr)?;
                self.builder.ins().return_(&[value]);
            }
            (None, _) => {
                self.builder.ins().return_(&[]);
            }
        }
        Ok(())
    }

    fn if_stmt(&mut self, cond: &TExpr, then_body: &[TStmt], else_body: &[TStmt]) -> GResult<bool> {
        let cond = self.value(cond)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder
            .ins()
            .brif(cond, then_block, &[], else_block, &[]);
        let then_done = self.branch(then_block, then_body, merge)?;
        let else_done = self.branch(else_block, else_body, merge)?;
        self.builder.switch_to_block(merge);
        if then_done && else_done {
            self.builder.ins().trap(UNREACHABLE);
        }
        Ok(then_done && else_done)
    }

    fn branch(&mut self, block: Block, body: &[TStmt], next: Block) -> GResult<bool> {
        self.builder.switch_to_block(block);
        let terminated = self.stmts(body)?;
        if !terminated {
            self.builder.ins().jump(next, &[]);
        }
        Ok(terminated)
    }

    fn while_stmt(&mut self, cond: &TExpr, body: &[TStmt]) -> GResult<bool> {
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let cond = self.value(cond)?;
        self.builder.ins().brif(cond, body_block, &[], exit, &[]);
        self.branch(body_block, body, header)?;
        self.builder.switch_to_block(exit);
        Ok(false)
    }

    fn for_stmt(
        &mut self,
        var: Variable,
        lo: &TExpr,
        hi: &TExpr,
        inclusive: bool,
        body: &[TStmt],
    ) -> GResult<bool> {
        let lo = self.value(lo)?;
        let hi = self.value(hi)?;
        self.builder.def_var(var, lo);
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let current = self.builder.use_var(var);
        let cc = if inclusive {
            IntCC::SignedLessThanOrEqual
        } else {
            IntCC::SignedLessThan
        };
        let more = self.builder.ins().icmp(cc, current, hi);
        self.builder.ins().brif(more, body_block, &[], exit, &[]);
        self.builder.switch_to_block(body_block);
        if !self.stmts(body)? {
            let current = self.builder.use_var(var);
            if inclusive {
                let advance = self.builder.create_block();
                let at_end = self.builder.ins().icmp(IntCC::Equal, current, hi);
                self.builder.ins().brif(at_end, exit, &[], advance, &[]);
                self.builder.switch_to_block(advance);
            }
            let current = self.builder.use_var(var);
            let next = self.builder.ins().iadd_imm_s(current, 1);
            self.builder.def_var(var, next);
            self.builder.ins().jump(header, &[]);
        }
        self.builder.switch_to_block(exit);
        Ok(false)
    }

    fn print(&mut self, expr: &TExpr) -> GResult<()> {
        let value = self.value(expr)?;
        match expr.ty() {
            Ty::Int => self.print_int(value),
            Ty::Bool => self.print_bool(value),
            Ty::Str => self.puts(value),
            other => return Err(CodegenError::from(format!("cannot print a {other:?}"))),
        }
        Ok(())
    }

    fn print_int(&mut self, value: Value) {
        let format = self.data_address(RUNTIME_FORMAT_INT);
        let mut sig = Signature::new(self.call_conv);
        for _ in 0..(2 + VARIADIC_PADDING) {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I32));
        let printf = self.import_func("printf", sig, false);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut args = vec![format];
        args.extend(std::iter::repeat_n(zero, VARIADIC_PADDING));
        args.push(value);
        self.builder.ins().call(printf, &args);
    }

    fn print_bool(&mut self, value: Value) {
        let yes = self.data_address(RUNTIME_TRUE);
        let no = self.data_address(RUNTIME_FALSE);
        let text = self.builder.ins().select(value, yes, no);
        self.puts(text);
    }

    fn puts(&mut self, text: Value) {
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I32));
        let puts = self.import_func("puts", sig, false);
        self.builder.ins().call(puts, &[text]);
    }

    fn value(&mut self, expr: &TExpr) -> GResult<Value> {
        self.expr(expr)?
            .ok_or_else(|| CodegenError::from("an expression without a value was used as a value"))
    }

    fn expr(&mut self, expr: &TExpr) -> GResult<Option<Value>> {
        let value = match expr {
            TExpr::Int(n) => self.builder.ins().iconst(types::I64, *n),
            TExpr::EnumCase { index, .. } => self
                .builder
                .ins()
                .iconst(types::I64, i64::try_from(*index)?),
            TExpr::Zero(ty) if ty.is_struct() => {
                let dest = self.alloc_struct(*ty)?;
                let zero = self.builder.ins().iconst(types::I64, 0);
                for slot in 0..self.struct_fields(*ty)?.len() {
                    self.builder.ins().store(
                        MemFlagsData::trusted(),
                        zero,
                        dest,
                        field_offset(slot),
                    );
                }
                dest
            }
            TExpr::Zero(ty) => {
                let clif = clif_type(*ty)
                    .ok_or_else(|| CodegenError::from("a zero value has no representable type"))?;
                self.builder.ins().iconst(clif, 0)
            }
            TExpr::Bool(b) => self.builder.ins().iconst(types::I8, i64::from(*b)),
            TExpr::Str(text) => self.string_address(text),
            TExpr::Local(local, _) => self.builder.use_var(self.vars[local.index()]),
            TExpr::Unary { op, expr } => {
                let operand = self.value(expr)?;
                match op {
                    UnaryOp::Neg => {
                        let zero = self.builder.ins().iconst(types::I64, 0);
                        let (result, overflow) = self.builder.ins().ssub_overflow(zero, operand);
                        self.builder
                            .ins()
                            .trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
                        result
                    }
                    UnaryOp::Not => self.builder.ins().bxor_imm_u(operand, 1),
                }
            }
            TExpr::Binary {
                op: BinOp::And,
                lhs,
                rhs,
            } => return self.short_circuit(true, lhs, rhs),
            TExpr::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
            } => return self.short_circuit(false, lhs, rhs),
            TExpr::Binary { op, lhs, rhs } => {
                let lhs = self.value(lhs)?;
                let rhs = self.value(rhs)?;
                self.binary(*op, lhs, rhs)
            }
            TExpr::Call {
                callee, args, ret, ..
            } => return self.call(callee, args, *ret),
            TExpr::Virtual {
                cases,
                default,
                args,
                ret,
            } => return self.virtual_call(cases, default, args, *ret),
            TExpr::Retain(inner) => {
                let value = self.value(inner)?;
                self.retain(value, inner.ty())?;
                value
            }
            TExpr::Moved(inner) => return self.expr(inner),
            TExpr::Select {
                cond,
                then,
                otherwise,
                ty,
            } => {
                let clif = clif_type(*ty).ok_or_else(|| {
                    CodegenError::from("a selected value has no representable type")
                })?;
                let condition = self.value(cond)?;
                let then_block = self.builder.create_block();
                let else_block = self.builder.create_block();
                let merge = self.builder.create_block();
                self.builder.append_block_param(merge, clif);
                self.builder
                    .ins()
                    .brif(condition, then_block, &[], else_block, &[]);
                self.builder.switch_to_block(then_block);
                let chosen = self.value(then)?;
                self.builder.ins().jump(merge, &[BlockArg::Value(chosen)]);
                self.builder.switch_to_block(else_block);
                let other = self.value(otherwise)?;
                self.builder.ins().jump(merge, &[BlockArg::Value(other)]);
                self.builder.switch_to_block(merge);
                self.builder.block_params(merge)[0]
            }
            TExpr::Unwrap { base, ty } => {
                let owner = base.ty();
                let base = self.value(base)?;
                let flags = MemFlagsData::trusted();
                let tag = self.builder.ins().load(types::I64, flags, base, 0);
                self.builder.ins().trapz(tag, UNWRAPPED_NIL);
                let offset = field_offset(self.slot_start(owner, 1));
                if ty.is_struct() {
                    self.builder.ins().iadd_imm_s(base, i64::from(offset))
                } else {
                    let clif = clif_type(*ty)
                        .ok_or_else(|| CodegenError::from("a payload has no representable type"))?;
                    self.builder.ins().load(clif, flags, base, offset)
                }
            }
            TExpr::RetainUnowned(inner) => {
                let value = self.value(inner)?;
                self.retain_unowned(value)?;
                value
            }
            TExpr::LoadUnowned(inner) => {
                let value = self.value(inner)?;
                self.load_unowned(value)?;
                value
            }
            TExpr::Bind { local, init } => {
                let mut value = self.value(init)?;
                if init.ty().is_struct() {
                    let copy = self.alloc_struct(init.ty())?;
                    self.copy_struct(copy, value, init.ty())?;
                    value = copy;
                }
                self.builder.def_var(self.vars[local.index()], value);
                value
            }
            TExpr::FuncAddr(name) => {
                let sig = signature(self.call_conv, &[Ty::Int], Ty::Void);
                let func_ref = self.import_func(&symbol(name), sig, true);
                self.builder.ins().func_addr(types::I64, func_ref)
            }
            TExpr::StructInit { ty, args } => {
                let values = args
                    .iter()
                    .map(|a| self.value(a))
                    .collect::<GResult<Vec<_>>>()?;
                let dest = self.alloc_struct(*ty)?;
                for (index, (value, arg)) in values.into_iter().zip(args).enumerate() {
                    self.store_field(dest, *ty, index, value, arg.ty())?;
                }
                dest
            }
            TExpr::New { ty, args } => {
                let values = args
                    .iter()
                    .map(|a| self.value(a))
                    .collect::<GResult<Vec<_>>>()?;
                let dest = self.alloc_object(*ty)?;
                for (index, (value, arg)) in values.into_iter().zip(args).enumerate() {
                    self.store_field(dest, *ty, index, value, arg.ty())?;
                }
                dest
            }
            TExpr::Field { base, index, ty } => {
                let owner = base.ty();
                let base = self.value(base)?;
                let offset = field_offset(self.slot_start(owner, *index));
                if ty.is_struct() {
                    self.builder.ins().iadd_imm_s(base, i64::from(offset))
                } else {
                    let clif = clif_type(*ty)
                        .ok_or_else(|| CodegenError::from("a field has no representable type"))?;
                    let flags = MemFlagsData::trusted();
                    self.builder.ins().load(clif, flags, base, offset)
                }
            }
        };
        Ok(Some(value))
    }

    fn binary(&mut self, op: BinOp, lhs: Value, rhs: Value) -> Value {
        if let Some(cc) = comparison(op) {
            return self.builder.ins().icmp(cc, lhs, rhs);
        }
        let (result, overflow) = match op {
            BinOp::Add => self.builder.ins().sadd_overflow(lhs, rhs),
            BinOp::Sub => self.builder.ins().ssub_overflow(lhs, rhs),
            BinOp::Mul => self.builder.ins().smul_overflow(lhs, rhs),
            BinOp::Div => return self.builder.ins().sdiv(lhs, rhs),
            _ => return self.builder.ins().srem(lhs, rhs),
        };
        self.builder
            .ins()
            .trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
        result
    }

    fn short_circuit(&mut self, is_and: bool, lhs: &TExpr, rhs: &TExpr) -> GResult<Option<Value>> {
        let lhs = self.value(lhs)?;
        let rhs_block = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I8);
        let decided = self.builder.ins().iconst(types::I8, i64::from(!is_and));
        let decided = [BlockArg::Value(decided)];
        if is_and {
            self.builder
                .ins()
                .brif(lhs, rhs_block, &[], merge, &decided);
        } else {
            self.builder
                .ins()
                .brif(lhs, merge, &decided, rhs_block, &[]);
        }
        self.builder.switch_to_block(rhs_block);
        let rhs = self.value(rhs)?;
        self.builder.ins().jump(merge, &[BlockArg::Value(rhs)]);
        self.builder.switch_to_block(merge);
        Ok(Some(self.builder.block_params(merge)[0]))
    }

    fn virtual_call(
        &mut self,
        cases: &[(i64, String)],
        default: &str,
        args: &[TExpr],
        ret: Ty,
    ) -> GResult<Option<Value>> {
        let param_types: Vec<Ty> = args.iter().map(TExpr::ty).collect();
        let sig = signature(self.call_conv, &param_types, ret);
        let result_slot = if ret.is_struct() {
            Some(self.alloc_struct(ret)?)
        } else {
            None
        };
        let mut values: Vec<Value> = result_slot.into_iter().collect();
        for arg in args {
            values.push(self.value(arg)?);
        }
        let receiver = *values
            .get(usize::from(result_slot.is_some()))
            .ok_or_else(|| CodegenError::from("a virtual call has no receiver"))?;
        let tag = self
            .builder
            .ins()
            .load(types::I64, MemFlagsData::trusted(), receiver, 0);
        let merge = self.builder.create_block();
        let result_type = return_type(ret);
        if let Some(clif) = result_type {
            self.builder.append_block_param(merge, clif);
        }
        let targets = cases
            .iter()
            .map(|(id, callee)| (Some(*id), callee.as_str()))
            .chain(std::iter::once((None, default)));
        for (id, callee) in targets {
            let func_ref = self.import_func(&symbol(callee), sig.clone(), true);
            let next = match id {
                Some(id) => {
                    let hit = self.builder.create_block();
                    let next = self.builder.create_block();
                    let expected = self.builder.ins().iconst(types::I64, id);
                    let matches = self.builder.ins().icmp(IntCC::Equal, tag, expected);
                    self.builder.ins().brif(matches, hit, &[], next, &[]);
                    self.builder.switch_to_block(hit);
                    Some(next)
                }
                None => None,
            };
            let call = self.builder.ins().call(func_ref, &values);
            let result = result_type
                .and_then(|_| self.builder.inst_results(call).first().copied())
                .map(BlockArg::Value);
            self.builder.ins().jump(merge, result.as_slice());
            if let Some(next) = next {
                self.builder.switch_to_block(next);
            }
        }
        self.builder.switch_to_block(merge);
        Ok(result_slot.or_else(|| self.builder.block_params(merge).first().copied()))
    }

    fn call(&mut self, callee: &str, args: &[TExpr], ret: Ty) -> GResult<Option<Value>> {
        let param_types: Vec<Ty> = args.iter().map(TExpr::ty).collect();
        let sig = signature(self.call_conv, &param_types, ret);
        let func_ref = self.import_func(&symbol(callee), sig, true);
        let result_slot = if ret.is_struct() {
            Some(self.alloc_struct(ret)?)
        } else {
            None
        };
        let mut values: Vec<Value> = result_slot.into_iter().collect();
        for arg in args {
            values.push(self.value(arg)?);
        }
        let call = self.builder.ins().call(func_ref, &values);
        Ok(result_slot.or_else(|| self.builder.inst_results(call).first().copied()))
    }
}

fn comparison(op: BinOp) -> Option<IntCC> {
    match op {
        BinOp::Eq | BinOp::Is => Some(IntCC::Equal),
        BinOp::Ne | BinOp::IsNot => Some(IntCC::NotEqual),
        BinOp::Lt => Some(IntCC::SignedLessThan),
        BinOp::Gt => Some(IntCC::SignedGreaterThan),
        BinOp::Le => Some(IntCC::SignedLessThanOrEqual),
        BinOp::Ge => Some(IntCC::SignedGreaterThanOrEqual),
        _ => None,
    }
}

fn define_entry(
    module: &mut ObjectModule,
    isa: &Arc<dyn TargetIsa>,
    funcs: &HashMap<String, FuncId>,
) -> GResult<()> {
    let main_id = *funcs
        .get(ENTRY_SYMBOL)
        .ok_or_else(|| CodegenError::from("no 'main' function"))?;
    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::I32));
    let entry_id = module.declare_function("main", Linkage::Export, &sig)?;
    let mut ctx = Context::new();
    ctx.func.signature = sig;
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let callee = module.declare_func_in_func(main_id, builder.func);
    builder.ins().call(callee, &[]);
    let status = builder.ins().iconst(types::I32, 0);
    builder.ins().return_(&[status]);
    builder.seal_all_blocks();
    builder.finalize(isa.frontend_config());
    module.define_function(entry_id, &mut ctx)?;
    Ok(())
}

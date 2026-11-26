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
use crate::sigs::{ARRAY_BUFFER, ARRAY_CAPACITY, ARRAY_COUNT};
use crate::tir::{
    ArrayOp, DESC_METHODS, DESC_PARENT, DESC_SLOT, Descriptor, RC_SLOT, StrOp, TExpr, TFunc, TStmt,
    TStruct, UNOWNED_SLOT,
};
use crate::types::Ty;

const VARIADIC_PADDING: usize = 7;
const FIELD_SIZE: u32 = 8;
const ELEMENT_SIZE: i64 = 8;
const UNREACHABLE: TrapCode = TrapCode::unwrap_user(1);
const UNOWNED_DEAD: TrapCode = TrapCode::unwrap_user(2);
const UNWRAPPED_NIL: TrapCode = TrapCode::unwrap_user(3);
const INDEX_OUT_OF_RANGE: TrapCode = TrapCode::unwrap_user(4);
const MIN_ARRAY_CAPACITY: i64 = 4;
const NAMESPACE_FUNC: u32 = 0;
const NAMESPACE_DATA: u32 = 1;

pub const ENTRY_SYMBOL: &str = "apus_main";
const STRING_HEADER: usize = 16;
const STRING_RC: i32 = -16;
const STRING_LEN: i32 = -8;
const IMMORTAL: i64 = i64::MIN;
const RUNTIME_FORMAT_INT: &str = "apus_format_int";
const RUNTIME_FORMAT_PLAIN: &str = "apus_format_plain";
const INT_BUFFER: i64 = 32;
const SNPRINTF_PADDING: usize = 5;
const RUNTIME_TRUE: &str = "apus_true";
const RUNTIME_FALSE: &str = "apus_false";
const RUNTIME_DATA: [(&str, &[u8]); 4] = [
    (RUNTIME_FORMAT_INT, b"%ld\n\0"),
    (RUNTIME_FORMAT_PLAIN, b"%ld\0"),
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
                .map(|chunk| {
                    std::thread::Builder::new()
                        .stack_size(crate::limits::STACK_SIZE)
                        .spawn_scoped(scope, move || self.compile_chunk(chunk, structs))
                        .map_err(|e| CodegenError::from(format!("cannot start a thread: {e}")))
                })
                .collect::<GResult<Vec<_>>>()?;
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
        for name in [
            "printf", "puts", "malloc", "realloc", "memmove", "free", "fflush", "write", "memcpy",
            "memcmp", "memmem", "snprintf",
        ] {
            let id = module.declare_function(name, Linkage::Import, &placeholder)?;
            funcs.insert(name.to_string(), id);
        }
        for (name, bytes) in RUNTIME_DATA {
            data.insert(name.to_string(), define_text(&mut module, name, bytes)?);
        }
        for entry in entries {
            let id = module.declare_function(entry.symbol(), Linkage::Export, &placeholder)?;
            funcs.insert(entry.symbol().to_string(), id);
        }
        let mut pending: Vec<(DataId, &Literal)> = Vec::new();
        for entry in entries {
            for literal in entry.literals() {
                if !data.contains_key(literal.name()) {
                    let writable = !literal.relocs().is_empty();
                    let id =
                        module.declare_data(literal.name(), Linkage::Local, writable, false)?;
                    data.insert(literal.name().to_string(), id);
                    pending.push((id, literal));
                }
            }
        }
        for (id, literal) in pending {
            let mut description = DataDescription::new();
            description.define(literal.bytes().to_vec().into_boxed_slice());
            description.set_align(8);
            for (offset, target) in literal.relocs() {
                match target {
                    SymbolRef::Func(name) => {
                        let func = *funcs.get(name).ok_or_else(|| {
                            CodegenError::from(format!("unresolved function '{name}'"))
                        })?;
                        let func_ref = module.declare_func_in_data(func, &mut description);
                        description.write_function_addr(*offset, func_ref);
                    }
                    SymbolRef::Data(name) => {
                        let target = *data.get(name).ok_or_else(|| {
                            CodegenError::from(format!("unresolved data '{name}'"))
                        })?;
                        let value = module.declare_data_in_data(target, &mut description);
                        description.write_data_addr(*offset, value, 0);
                    }
                }
            }
            module.define_data(id, &description)?;
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
        Ty::Int | Ty::Str | Ty::Char | Ty::Struct(_) | Ty::Class(_) | Ty::Enum(_) => {
            Some(types::I64)
        }
        Ty::Bool => Some(types::I8),
        Ty::Void | Ty::Param(_) | Ty::Assoc(_) => None,
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
    data.set_align(8);
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
        if matches!(ty, Ty::Class(_) | Ty::Struct(_) | Ty::Str) {
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
        jumps: Vec::new(),
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
    jumps: Vec<JumpBlocks>,
}

struct JumpBlocks {
    exit: Block,
    next: Option<Block>,
    exit_used: bool,
    next_used: bool,
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
        let mut bytes = Vec::with_capacity(STRING_HEADER + text.len() + 1);
        bytes.extend_from_slice(&IMMORTAL.to_le_bytes());
        bytes.extend_from_slice(&i64::try_from(text.len()).unwrap_or(i64::MAX).to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
        let mut hasher = Fnv1a::new();
        hasher.write(&bytes);
        let name = format!("apus_str_{:016x}", hasher.finish());
        if !self.literals.iter().any(|l| l.name() == name) {
            self.literals.push(Literal::new(name.clone(), bytes));
        }
        let base = self.data_address(&name);
        self.builder.ins().iadd_imm_s(base, STRING_HEADER as i64)
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
            TStmt::AssignPath { base, index, value } => {
                let value_ty = value.ty();
                let value = self.value(value)?;
                let owner = base.ty();
                let address = self.value(base)?;
                self.store_field(address, owner, *index, value, value_ty)?;
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
            TStmt::Break(depth) => {
                let target = self.jump_target(*depth)?;
                target.exit_used = true;
                let exit = target.exit;
                self.builder.ins().jump(exit, &[]);
                Ok(true)
            }
            TStmt::Continue(depth) => {
                let target = self.jump_target(*depth)?;
                target.next_used = true;
                let next = target
                    .next
                    .ok_or_else(|| CodegenError::from("continue outside a loop".to_string()))?;
                self.builder.ins().jump(next, &[]);
                Ok(true)
            }
            TStmt::Block(body) => self.block_stmt(body),
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
            TStmt::Fatal(text) => {
                self.fatal(text);
                Ok(true)
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
            TStmt::DropArray { array, elem } => {
                let array = self.value(array)?;
                self.drop_array(array, *elem)?;
                Ok(false)
            }
            TStmt::ReleaseUnowned(expr) => {
                let value = self.value(expr)?;
                self.release_unowned(value)?;
                Ok(false)
            }
        }
    }

    fn array_op(&mut self, op: ArrayOp, elem: Ty, args: &[TExpr]) -> GResult<Value> {
        let values = args
            .iter()
            .map(|a| self.value(a))
            .collect::<GResult<Vec<_>>>()?;
        let flags = MemFlagsData::trusted();
        let stride = self.stride(elem)?;
        let inline = elem.is_struct();
        match op {
            ArrayOp::Buffer => {
                let slots = i64::try_from(values.len().max(1))?;
                let size = self.builder.ins().iconst(types::I64, slots * stride);
                let buffer = self.allocate(size)?;
                for (index, value) in values.into_iter().enumerate() {
                    let offset = i64::try_from(index)? * stride;
                    self.put_element(buffer, offset, value, elem)?;
                }
                Ok(buffer)
            }
            ArrayOp::Get => {
                let address = self.element_address(values[0], values[1], stride);
                if inline {
                    Ok(address)
                } else {
                    let clif = self.element_type(elem)?;
                    Ok(self.builder.ins().load(clif, flags, address, 0))
                }
            }
            ArrayOp::Append => {
                let value = self.detach(values[1], elem)?;
                let array = self.unique_array(values[0], elem, 1)?;
                let count =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
                let buffer =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
                let at = self.builder.ins().imul_imm_s(count, stride);
                let address = self.builder.ins().iadd(buffer, at);
                self.put_element(address, 0, value, elem)?;
                let next = self.builder.ins().iadd_imm_s(count, 1);
                self.builder
                    .ins()
                    .store(flags, next, array, field_offset(ARRAY_COUNT));
                Ok(array)
            }
            ArrayOp::Insert => {
                let count0 = self.builder.ins().load(
                    types::I64,
                    flags,
                    values[0],
                    field_offset(ARRAY_COUNT),
                );
                let outside =
                    self.builder
                        .ins()
                        .icmp(IntCC::UnsignedGreaterThan, values[1], count0);
                self.builder.ins().trapnz(outside, INDEX_OUT_OF_RANGE);
                let value = self.detach(values[2], elem)?;
                let array = self.unique_array(values[0], elem, 1)?;
                let count =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
                let buffer =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
                let from = self.builder.ins().imul_imm_s(values[1], stride);
                let source = self.builder.ins().iadd(buffer, from);
                let target = self.builder.ins().iadd_imm_s(source, stride);
                let moved = self.builder.ins().isub(count, values[1]);
                let bytes = self.builder.ins().imul_imm_s(moved, stride);
                let mut sig = Signature::new(self.call_conv);
                for _ in 0..3 {
                    sig.params.push(AbiParam::new(types::I64));
                }
                let memmove = self.import_func("memmove", sig, false);
                self.builder.ins().call(memmove, &[target, source, bytes]);
                self.put_element(source, 0, value, elem)?;
                let next = self.builder.ins().iadd_imm_s(count, 1);
                self.builder
                    .ins()
                    .store(flags, next, array, field_offset(ARRAY_COUNT));
                Ok(array)
            }
            ArrayOp::RemoveLast => {
                let count0 = self.builder.ins().load(
                    types::I64,
                    flags,
                    values[0],
                    field_offset(ARRAY_COUNT),
                );
                self.builder.ins().trapz(count0, INDEX_OUT_OF_RANGE);
                let array = self.unique_array(values[0], elem, 0)?;
                let count =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
                let last = self.builder.ins().iadd_imm_s(count, -1);
                self.builder
                    .ins()
                    .store(flags, last, array, field_offset(ARRAY_COUNT));
                let buffer =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
                let at = self.builder.ins().imul_imm_s(last, stride);
                let address = self.builder.ins().iadd(buffer, at);
                if inline {
                    self.release(address, elem)?;
                } else {
                    let clif = self.element_type(elem)?;
                    let old = self.builder.ins().load(clif, flags, address, 0);
                    self.release(old, elem)?;
                }
                Ok(array)
            }
            ArrayOp::Unique => self.unique_array(values[0], elem, 0),
            ArrayOp::Remove => {
                self.element_address(values[0], values[1], stride);
                let array = self.unique_array(values[0], elem, 0)?;
                let count =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
                let buffer =
                    self.builder
                        .ins()
                        .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
                let at = self.builder.ins().imul_imm_s(values[1], stride);
                let address = self.builder.ins().iadd(buffer, at);
                if inline {
                    self.release(address, elem)?;
                } else {
                    let clif = self.element_type(elem)?;
                    let old = self.builder.ins().load(clif, flags, address, 0);
                    self.release(old, elem)?;
                }
                let tail = self.builder.ins().iadd_imm_s(address, stride);
                let after = self.builder.ins().isub(count, values[1]);
                let remaining = self.builder.ins().iadd_imm_s(after, -1);
                let bytes = self.builder.ins().imul_imm_s(remaining, stride);
                let mut sig = Signature::new(self.call_conv);
                for _ in 0..3 {
                    sig.params.push(AbiParam::new(types::I64));
                }
                let memmove = self.import_func("memmove", sig, false);
                self.builder.ins().call(memmove, &[address, tail, bytes]);
                let last = self.builder.ins().iadd_imm_s(count, -1);
                self.builder
                    .ins()
                    .store(flags, last, array, field_offset(ARRAY_COUNT));
                Ok(array)
            }
            ArrayOp::Concat => self.concat_arrays(values[0], values[1], elem, stride),
            ArrayOp::Set => {
                self.element_address(values[0], values[1], stride);
                let value = self.detach(values[2], elem)?;
                let array = self.unique_array(values[0], elem, 0)?;
                let address = self.element_address(array, values[1], stride);
                if inline {
                    self.release(address, elem)?;
                } else {
                    let clif = self.element_type(elem)?;
                    let old = self.builder.ins().load(clif, flags, address, 0);
                    self.release(old, elem)?;
                }
                self.put_element(address, 0, value, elem)?;
                Ok(array)
            }
        }
    }

    fn element_type(&self, elem: Ty) -> GResult<Type> {
        clif_type(elem)
            .ok_or_else(|| CodegenError::from("an array element has no representable type"))
    }

    fn stride(&self, elem: Ty) -> GResult<i64> {
        if elem.is_struct() {
            Ok(i64::try_from(self.struct_fields(elem)?.len().max(1))? * ELEMENT_SIZE)
        } else {
            Ok(ELEMENT_SIZE)
        }
    }

    fn put_element(&mut self, base: Value, offset: i64, value: Value, elem: Ty) -> GResult<()> {
        if elem.is_struct() {
            let target = self.builder.ins().iadd_imm_s(base, offset);
            return self.copy_struct(target, value, elem);
        }
        self.builder
            .ins()
            .store(MemFlagsData::trusted(), value, base, i32::try_from(offset)?);
        Ok(())
    }

    fn detach(&mut self, value: Value, elem: Ty) -> GResult<Value> {
        if !elem.is_struct() {
            return Ok(value);
        }
        let copy = self.alloc_struct(elem)?;
        self.copy_struct(copy, value, elem)?;
        Ok(copy)
    }

    fn allocate(&mut self, size: Value) -> GResult<Value> {
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

    fn element_address(&mut self, array: Value, index: Value, stride: i64) -> Value {
        let flags = MemFlagsData::trusted();
        let count = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
        let outside = self
            .builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, index, count);
        self.builder.ins().trapnz(outside, INDEX_OUT_OF_RANGE);
        let buffer = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
        let at = self.builder.ins().imul_imm_s(index, stride);
        self.builder.ins().iadd(buffer, at)
    }

    fn unique_array(&mut self, array: Value, elem: Ty, extra: i64) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let stride = self.stride(elem)?;
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let reuse = self.builder.create_block();
        let copy = self.builder.create_block();
        let strong = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(RC_SLOT));
        let sole = self.builder.ins().icmp_imm_s(IntCC::Equal, strong, 1);
        self.builder.ins().brif(sole, reuse, &[], copy, &[]);

        self.builder.switch_to_block(reuse);
        let count = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
        let capacity =
            self.builder
                .ins()
                .load(types::I64, flags, array, field_offset(ARRAY_CAPACITY));
        let needed = self.builder.ins().iadd_imm_s(count, extra);
        let fits = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThanOrEqual, needed, capacity);
        let grow = self.builder.create_block();
        let keep = self.builder.create_block();
        self.builder.ins().brif(fits, keep, &[], grow, &[]);
        self.builder.switch_to_block(grow);
        let wanted = self.grown_capacity(needed, capacity);
        let bytes = self.builder.ins().imul_imm_s(wanted, stride);
        let buffer = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let realloc = self.import_func("realloc", sig, false);
        let call = self.builder.ins().call(realloc, &[buffer, bytes]);
        let moved = self
            .builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| CodegenError::from("realloc returned no value"))?;
        self.builder
            .ins()
            .store(flags, moved, array, field_offset(ARRAY_BUFFER));
        self.builder
            .ins()
            .store(flags, wanted, array, field_offset(ARRAY_CAPACITY));
        self.builder.ins().jump(keep, &[]);
        self.builder.switch_to_block(keep);
        self.retain_reference(array)?;
        self.builder.ins().jump(merge, &[BlockArg::Value(array)]);

        self.builder.switch_to_block(copy);
        let count = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
        let needed = self.builder.ins().iadd_imm_s(count, extra);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let wanted = self.grown_capacity(needed, zero);
        let object_size = self
            .builder
            .ins()
            .iconst(types::I64, i64::try_from(ARRAY_BUFFER + 1)? * ELEMENT_SIZE);
        let fresh = self.allocate(object_size)?;
        for slot in 0..ARRAY_COUNT {
            let value = self
                .builder
                .ins()
                .load(types::I64, flags, array, field_offset(slot));
            self.builder
                .ins()
                .store(flags, value, fresh, field_offset(slot));
        }
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder
            .ins()
            .store(flags, one, fresh, field_offset(RC_SLOT));
        self.builder
            .ins()
            .store(flags, zero, fresh, field_offset(UNOWNED_SLOT));
        self.builder
            .ins()
            .store(flags, count, fresh, field_offset(ARRAY_COUNT));
        self.builder
            .ins()
            .store(flags, wanted, fresh, field_offset(ARRAY_CAPACITY));
        let bytes = self.builder.ins().imul_imm_s(wanted, stride);
        let target = self.allocate(bytes)?;
        self.builder
            .ins()
            .store(flags, target, fresh, field_offset(ARRAY_BUFFER));
        let source = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
        self.copy_elements(source, target, count, elem, stride)?;
        self.builder.ins().jump(merge, &[BlockArg::Value(fresh)]);

        self.builder.switch_to_block(merge);
        Ok(self.builder.block_params(merge)[0])
    }

    fn concat_arrays(
        &mut self,
        left: Value,
        right: Value,
        elem: Ty,
        stride: i64,
    ) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let left_count =
            self.builder
                .ins()
                .load(types::I64, flags, left, field_offset(ARRAY_COUNT));
        let right_count =
            self.builder
                .ins()
                .load(types::I64, flags, right, field_offset(ARRAY_COUNT));
        let total = self.builder.ins().iadd(left_count, right_count);
        let one = self.builder.ins().iconst(types::I64, 1);
        let capacity = self.builder.ins().smax(total, one);
        let object_size = self
            .builder
            .ins()
            .iconst(types::I64, i64::try_from(ARRAY_BUFFER + 1)? * ELEMENT_SIZE);
        let fresh = self.allocate(object_size)?;
        for slot in 0..ARRAY_COUNT {
            let value = self
                .builder
                .ins()
                .load(types::I64, flags, left, field_offset(slot));
            self.builder
                .ins()
                .store(flags, value, fresh, field_offset(slot));
        }
        let zero = self.builder.ins().iconst(types::I64, 0);
        self.builder
            .ins()
            .store(flags, one, fresh, field_offset(RC_SLOT));
        self.builder
            .ins()
            .store(flags, zero, fresh, field_offset(UNOWNED_SLOT));
        self.builder
            .ins()
            .store(flags, total, fresh, field_offset(ARRAY_COUNT));
        self.builder
            .ins()
            .store(flags, capacity, fresh, field_offset(ARRAY_CAPACITY));
        let bytes = self.builder.ins().imul_imm_s(capacity, stride);
        let target = self.allocate(bytes)?;
        self.builder
            .ins()
            .store(flags, target, fresh, field_offset(ARRAY_BUFFER));
        let left_buffer =
            self.builder
                .ins()
                .load(types::I64, flags, left, field_offset(ARRAY_BUFFER));
        let right_buffer =
            self.builder
                .ins()
                .load(types::I64, flags, right, field_offset(ARRAY_BUFFER));
        let shifted = self.builder.ins().imul_imm_s(left_count, stride);
        let after = self.builder.ins().iadd(target, shifted);
        self.copy_elements(left_buffer, target, left_count, elem, stride)?;
        self.copy_elements(right_buffer, after, right_count, elem, stride)?;
        Ok(fresh)
    }

    fn copy_elements(
        &mut self,
        source: Value,
        target: Value,
        count: Value,
        elem: Ty,
        stride: i64,
    ) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.each_element(count, |this, index| {
            let at = this.builder.ins().imul_imm_s(index, stride);
            let from = this.builder.ins().iadd(source, at);
            let to = this.builder.ins().iadd(target, at);
            if elem.is_struct() {
                this.copy_struct(to, from, elem)?;
                return this.retain(to, elem);
            }
            let value = this.builder.ins().load(types::I64, flags, from, 0);
            this.retain(value, elem)?;
            this.builder.ins().store(flags, value, to, 0);
            Ok(())
        })
    }

    fn grown_capacity(&mut self, needed: Value, current: Value) -> Value {
        let doubled = self.builder.ins().imul_imm_s(current, 2);
        let wanted = self.builder.ins().smax(needed, doubled);
        let floor = self.builder.ins().iconst(types::I64, MIN_ARRAY_CAPACITY);
        self.builder.ins().smax(wanted, floor)
    }

    fn each_element(
        &mut self,
        count: Value,
        mut body: impl FnMut(&mut Self, Value) -> GResult<()>,
    ) -> GResult<()> {
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let step = self.builder.create_block();
        let exit = self.builder.create_block();
        let start = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(header, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(header);
        let index = self.builder.block_params(header)[0];
        let more = self.builder.ins().icmp(IntCC::SignedLessThan, index, count);
        self.builder.ins().brif(more, step, &[], exit, &[]);
        self.builder.switch_to_block(step);
        body(self, index)?;
        let next = self.builder.ins().iadd_imm_s(index, 1);
        self.builder.ins().jump(header, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(exit);
        Ok(())
    }

    fn drop_array(&mut self, array: Value, elem: Ty) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        let buffer = self
            .builder
            .ins()
            .load(types::I64, flags, array, field_offset(ARRAY_BUFFER));
        let counted = match elem {
            Ty::Str | Ty::Class(_) => true,
            Ty::Struct(_) => !self.counted_slots(elem)?.is_empty(),
            _ => false,
        };
        if counted {
            let stride = self.stride(elem)?;
            let count =
                self.builder
                    .ins()
                    .load(types::I64, flags, array, field_offset(ARRAY_COUNT));
            self.each_element(count, |this, index| {
                let at = this.builder.ins().imul_imm_s(index, stride);
                let address = this.builder.ins().iadd(buffer, at);
                if elem.is_struct() {
                    return this.release(address, elem);
                }
                let value = this.builder.ins().load(types::I64, flags, address, 0);
                this.release(value, elem)
            })?;
        }
        self.free_pointer(buffer);
        Ok(())
    }

    fn counted_slots(&self, ty: Ty) -> GResult<Vec<(usize, Ty)>> {
        Ok(self
            .struct_fields(ty)?
            .iter()
            .enumerate()
            .filter(|(_, f)| matches!(f, Ty::Class(_) | Ty::Str))
            .map(|(index, f)| (index, *f))
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
            Ty::Str => self.retain_string(value),
            Ty::Struct(_) => {
                for (field, slot_ty) in self.load_counted_slots(value, ty)? {
                    self.retain(field, slot_ty)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn release(&mut self, value: Value, ty: Ty) -> GResult<()> {
        match ty {
            Ty::Class(_) => self.release_reference(value),
            Ty::Str => self.release_string(value),
            Ty::Struct(_) => {
                if self.counted_slots(ty)?.is_empty() {
                    return Ok(());
                }
                self.guarded(value, |this| {
                    for (field, slot_ty) in this.load_counted_slots(value, ty)? {
                        this.release(field, slot_ty)?;
                    }
                    Ok(())
                })
            }
            _ => Ok(()),
        }
    }

    fn load_counted_slots(&mut self, value: Value, ty: Ty) -> GResult<Vec<(Value, Ty)>> {
        let flags = MemFlagsData::trusted();
        Ok(self
            .counted_slots(ty)?
            .into_iter()
            .map(|(slot, slot_ty)| {
                let loaded = self
                    .builder
                    .ins()
                    .load(types::I64, flags, value, field_offset(slot));
                (loaded, slot_ty)
            })
            .collect())
    }

    fn string_op(&mut self, op: StrOp, args: &[TExpr]) -> GResult<Value> {
        let values = args
            .iter()
            .map(|arg| self.value(arg))
            .collect::<GResult<Vec<_>>>()?;
        let flags = MemFlagsData::trusted();
        match (op, values.as_slice()) {
            (StrOp::Concat, [left, right]) => {
                let left_len = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *left, STRING_LEN);
                let right_len = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *right, STRING_LEN);
                let total = self.builder.ins().iadd(left_len, right_len);
                let result = self.alloc_string(total);
                self.copy_bytes(result, *left, left_len);
                let tail = self.builder.ins().iadd(result, left_len);
                self.copy_bytes(tail, *right, right_len);
                Ok(result)
            }
            (StrOp::Count, [text]) => {
                Ok(self
                    .builder
                    .ins()
                    .load(types::I64, flags, *text, STRING_LEN))
            }
            (StrOp::IsEmpty, [text]) => {
                let len = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *text, STRING_LEN);
                Ok(self.builder.ins().icmp_imm_s(IntCC::Equal, len, 0))
            }
            (StrOp::FromInt, [number]) => Ok(self.int_to_string(*number)),
            (StrOp::CharValue, [scalar]) => Ok(*scalar),
            (StrOp::CharWidth, [scalar]) => Ok(self.char_width(*scalar)),
            (StrOp::FromChar, [scalar]) => Ok(self.char_to_string(*scalar)),
            (StrOp::CharAt, [text, offset]) => Ok(self.char_at(*text, *offset)),
            (StrOp::LastChar, [text]) => Ok(self.last_char(*text)),
            (StrOp::Trimmed, [text]) => Ok(self.trimmed(*text)),
            (StrOp::Replace, [text, target, replacement]) => {
                Ok(self.replaced(*text, *target, *replacement))
            }
            (StrOp::ByteIndex, [text, needle]) => Ok(self.byte_index(*text, *needle)),
            (StrOp::ScalarCount, [text, bytes]) => Ok(self.scalar_count(*text, *bytes)),
            (StrOp::Prefix | StrOp::Suffix | StrOp::DropFirst | StrOp::DropLast, [text, count]) => {
                let non_negative =
                    self.builder
                        .ins()
                        .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, *count, 0);
                self.builder.ins().trapz(non_negative, UNREACHABLE);
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *text, STRING_LEN);
                let zero = self.zero();
                Ok(match op {
                    StrOp::Prefix => {
                        let end = self.forward_offset(*text, *count);
                        self.slice(*text, zero, end)
                    }
                    StrOp::DropFirst => {
                        let start = self.forward_offset(*text, *count);
                        self.slice(*text, start, length)
                    }
                    StrOp::Suffix => {
                        let start = self.backward_offset(*text, *count);
                        self.slice(*text, start, length)
                    }
                    _ => {
                        let end = self.backward_offset(*text, *count);
                        self.slice(*text, zero, end)
                    }
                })
            }
            (StrOp::Reversed, [text]) => Ok(self.reversed_string(*text)),
            (StrOp::Uppercased, [text]) => Ok(self.map_ascii(*text, 97, 122, -32)),
            (StrOp::Lowercased, [text]) => Ok(self.map_ascii(*text, 65, 90, 32)),
            (StrOp::CharFromInt, [number]) => {
                self.require_scalar(*number);
                Ok(*number)
            }
            (StrOp::CharFromString, [text]) => {
                let zero = self.builder.ins().iconst(types::I64, 0);
                let scalar = self.char_at(*text, zero);
                let width = self.char_width(scalar);
                let length = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *text, STRING_LEN);
                let exact = self.builder.ins().icmp(IntCC::Equal, length, width);
                self.builder.ins().trapz(exact, UNREACHABLE);
                Ok(scalar)
            }
            (StrOp::HasPrefix, [text, prefix]) => Ok(self.string_edge(*text, *prefix, false)),
            (StrOp::HasSuffix, [text, suffix]) => Ok(self.string_edge(*text, *suffix, true)),
            (StrOp::Contains, [text, needle]) => {
                let text_len = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *text, STRING_LEN);
                let needle_len = self
                    .builder
                    .ins()
                    .load(types::I64, flags, *needle, STRING_LEN);
                let mut sig = Signature::new(self.call_conv);
                for _ in 0..4 {
                    sig.params.push(AbiParam::new(types::I64));
                }
                sig.returns.push(AbiParam::new(types::I64));
                let memmem = self.import_func("memmem", sig, false);
                let call = self
                    .builder
                    .ins()
                    .call(memmem, &[*text, text_len, *needle, needle_len]);
                let found = self.builder.inst_results(call)[0];
                let present = self.builder.ins().icmp_imm_s(IntCC::NotEqual, found, 0);
                let empty = self.builder.ins().icmp_imm_s(IntCC::Equal, needle_len, 0);
                Ok(self.builder.ins().bor(present, empty))
            }
            _ => Err(CodegenError::from(
                "a string operation has the wrong operands",
            )),
        }
    }

    fn char_width(&mut self, scalar: Value) -> Value {
        let below = |this: &mut Self, limit: i64| {
            this.builder
                .ins()
                .icmp_imm_s(IntCC::UnsignedLessThan, scalar, limit)
        };
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let three = self.builder.ins().iconst(types::I64, 3);
        let four = self.builder.ins().iconst(types::I64, 4);
        let wide = below(self, 0x10000);
        let upper = self.builder.ins().select(wide, three, four);
        let medium = below(self, 0x800);
        let middle = self.builder.ins().select(medium, two, upper);
        let narrow = below(self, 0x80);
        self.builder.ins().select(narrow, one, middle)
    }

    fn char_to_string(&mut self, scalar: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let width = self.char_width(scalar);
        let result = self.alloc_string(width);
        let buffer = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        let bytes = self.builder.ins().stack_addr(types::I64, buffer, 0);
        let is =
            |this: &mut Self, count: i64| this.builder.ins().icmp_imm_s(IntCC::Equal, width, count);
        let shifted = |this: &mut Self, amount: i64| this.builder.ins().ushr_imm_u(scalar, amount);
        let tail = |this: &mut Self, value: Value| {
            let low = this.builder.ins().band_imm_s(value, 0x3f);
            this.builder.ins().bor_imm_s(low, 0x80)
        };
        let marked =
            |this: &mut Self, value: Value, mark: i64| this.builder.ins().bor_imm_s(value, mark);
        let two = is(self, 2);
        let three = is(self, 3);
        let one = is(self, 1);
        let s6 = shifted(self, 6);
        let s12 = shifted(self, 12);
        let s18 = shifted(self, 18);
        let lead2 = marked(self, s6, 0xC0);
        let lead3 = marked(self, s12, 0xE0);
        let lead4 = marked(self, s18, 0xF0);
        let pick34 = self.builder.ins().select(three, lead3, lead4);
        let pick234 = self.builder.ins().select(two, lead2, pick34);
        let first = self.builder.ins().select(one, scalar, pick234);
        let t0 = tail(self, scalar);
        let t6 = tail(self, s6);
        let t12 = tail(self, s12);
        let second_wide = self.builder.ins().select(three, t6, t12);
        let second = self.builder.ins().select(two, t0, second_wide);
        let third = self.builder.ins().select(three, t0, t6);
        for (offset, value) in [(0, first), (1, second), (2, third), (3, t0)] {
            let byte = self.builder.ins().ireduce(types::I8, value);
            self.builder.ins().store(flags, byte, bytes, offset);
        }
        self.copy_bytes(result, bytes, width);
        result
    }

    fn char_at(&mut self, text: Value, offset: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let at = self.builder.ins().iadd(text, offset);
        let load = |this: &mut Self, delta: i32| {
            let byte = this.builder.ins().load(types::I8, flags, at, delta);
            this.builder.ins().uextend(types::I64, byte)
        };
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let multi = self.builder.create_block();
        let two = self.builder.create_block();
        let wide = self.builder.create_block();
        let three = self.builder.create_block();
        let four = self.builder.create_block();
        let b0 = load(self, 0);
        let ascii = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, b0, 0x80);
        self.builder
            .ins()
            .brif(ascii, done, &[BlockArg::Value(b0)], multi, &[]);
        self.builder.switch_to_block(multi);
        let short = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, b0, 0xE0);
        self.builder.ins().brif(short, two, &[], wide, &[]);
        self.builder.switch_to_block(two);
        let b1 = load(self, 1);
        let high = self.builder.ins().band_imm_s(b0, 0x1f);
        let high = self.builder.ins().ishl_imm_u(high, 6);
        let low = self.builder.ins().band_imm_s(b1, 0x3f);
        let value = self.builder.ins().bor(high, low);
        self.builder.ins().jump(done, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(wide);
        let medium = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, b0, 0xF0);
        self.builder.ins().brif(medium, three, &[], four, &[]);
        self.builder.switch_to_block(three);
        let b1 = load(self, 1);
        let b2 = load(self, 2);
        let high = self.builder.ins().band_imm_s(b0, 0x0f);
        let high = self.builder.ins().ishl_imm_u(high, 12);
        let mid = self.builder.ins().band_imm_s(b1, 0x3f);
        let mid = self.builder.ins().ishl_imm_u(mid, 6);
        let low = self.builder.ins().band_imm_s(b2, 0x3f);
        let value = self.builder.ins().bor(high, mid);
        let value = self.builder.ins().bor(value, low);
        self.builder.ins().jump(done, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(four);
        let b1 = load(self, 1);
        let b2 = load(self, 2);
        let b3 = load(self, 3);
        let high = self.builder.ins().band_imm_s(b0, 0x07);
        let high = self.builder.ins().ishl_imm_u(high, 18);
        let a = self.builder.ins().band_imm_s(b1, 0x3f);
        let a = self.builder.ins().ishl_imm_u(a, 12);
        let b = self.builder.ins().band_imm_s(b2, 0x3f);
        let b = self.builder.ins().ishl_imm_u(b, 6);
        let c = self.builder.ins().band_imm_s(b3, 0x3f);
        let value = self.builder.ins().bor(high, a);
        let value = self.builder.ins().bor(value, b);
        let value = self.builder.ins().bor(value, c);
        self.builder.ins().jump(done, &[BlockArg::Value(value)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn find(&mut self, hay: Value, hay_len: Value, needle: Value, needle_len: Value) -> Value {
        let mut sig = Signature::new(self.call_conv);
        for _ in 0..4 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let memmem = self.import_func("memmem", sig, false);
        let call = self
            .builder
            .ins()
            .call(memmem, &[hay, hay_len, needle, needle_len]);
        let found = self.builder.inst_results(call)[0];
        let empty = self.builder.ins().icmp_imm_s(IntCC::Equal, needle_len, 0);
        let nothing = self.zero();
        self.builder.ins().select(empty, nothing, found)
    }

    fn byte_index(&mut self, text: Value, needle: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let needle_len = self
            .builder
            .ins()
            .load(types::I64, flags, needle, STRING_LEN);
        let found = self.find(text, length, needle, needle_len);
        let missing = self.builder.ins().icmp_imm_s(IntCC::Equal, found, 0);
        let offset = self.builder.ins().isub(found, text);
        let none = self.builder.ins().iconst(types::I64, -1);
        let empty = self.builder.ins().icmp_imm_s(IntCC::Equal, needle_len, 0);
        let zero = self.zero();
        let located = self.builder.ins().select(missing, none, offset);
        self.builder.ins().select(empty, zero, located)
    }

    fn scalar_count(&mut self, text: Value, bytes: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.zero();
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(zero), BlockArg::Value(zero)]);
        self.builder.switch_to_block(header);
        let index = self.builder.block_params(header)[0];
        let total = self.builder.block_params(header)[1];
        let more = self.builder.ins().icmp(IntCC::SignedLessThan, index, bytes);
        self.builder
            .ins()
            .brif(more, body, &[], done, &[BlockArg::Value(total)]);
        self.builder.switch_to_block(body);
        let address = self.builder.ins().iadd(text, index);
        let byte = self.builder.ins().load(types::I8, flags, address, 0);
        let byte = self.builder.ins().uextend(types::I64, byte);
        let marker = self.builder.ins().band_imm_s(byte, 0xC0);
        let lead = self.builder.ins().icmp_imm_s(IntCC::NotEqual, marker, 0x80);
        let lead = self.builder.ins().uextend(types::I64, lead);
        let next_total = self.builder.ins().iadd(total, lead);
        let next_index = self.builder.ins().iadd_imm_s(index, 1);
        self.builder.ins().jump(
            header,
            &[BlockArg::Value(next_index), BlockArg::Value(next_total)],
        );
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn is_space(&mut self, byte: Value) -> Value {
        let shifted = self.builder.ins().iadd_imm_s(byte, -9);
        let control = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, shifted, 5);
        let blank = self.builder.ins().icmp_imm_s(IntCC::Equal, byte, 32);
        self.builder.ins().bor(control, blank)
    }

    fn trimmed(&mut self, text: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let zero = self.zero();
        let start = self.skip_spaces(text, zero, length, true);
        let end = self.skip_spaces(text, length, start, false);
        self.slice(text, start, end)
    }

    fn skip_spaces(&mut self, text: Value, from: Value, limit: Value, forward: bool) -> Value {
        let flags = MemFlagsData::trusted();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let check = self.builder.create_block();
        let step = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        self.builder.ins().jump(header, &[BlockArg::Value(from)]);
        self.builder.switch_to_block(header);
        let position = self.builder.block_params(header)[0];
        let inside = if forward {
            self.builder
                .ins()
                .icmp(IntCC::SignedLessThan, position, limit)
        } else {
            self.builder
                .ins()
                .icmp(IntCC::SignedGreaterThan, position, limit)
        };
        self.builder
            .ins()
            .brif(inside, check, &[], done, &[BlockArg::Value(position)]);
        self.builder.switch_to_block(check);
        let at = if forward {
            position
        } else {
            self.builder.ins().iadd_imm_s(position, -1)
        };
        let address = self.builder.ins().iadd(text, at);
        let byte = self.builder.ins().load(types::I8, flags, address, 0);
        let byte = self.builder.ins().uextend(types::I64, byte);
        let space = self.is_space(byte);
        self.builder
            .ins()
            .brif(space, step, &[], done, &[BlockArg::Value(position)]);
        self.builder.switch_to_block(step);
        let next = if forward {
            self.builder.ins().iadd_imm_s(position, 1)
        } else {
            self.builder.ins().iadd_imm_s(position, -1)
        };
        self.builder.ins().jump(header, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn replaced(&mut self, text: Value, target: Value, replacement: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let target_len = self
            .builder
            .ins()
            .load(types::I64, flags, target, STRING_LEN);
        let new_len = self
            .builder
            .ins()
            .load(types::I64, flags, replacement, STRING_LEN);
        let zero = self.zero();
        let count_header = self.builder.create_block();
        self.builder.append_block_param(count_header, types::I64);
        self.builder.append_block_param(count_header, types::I64);
        let count_done = self.builder.create_block();
        self.builder.append_block_param(count_done, types::I64);
        let count_next = self.builder.create_block();
        self.builder.append_block_param(count_next, types::I64);
        self.builder.append_block_param(count_next, types::I64);
        self.builder.ins().jump(
            count_header,
            &[BlockArg::Value(zero), BlockArg::Value(zero)],
        );
        self.builder.switch_to_block(count_header);
        let at = self.builder.block_params(count_header)[0];
        let seen = self.builder.block_params(count_header)[1];
        let rest = self.builder.ins().isub(length, at);
        let from = self.builder.ins().iadd(text, at);
        let hit = self.find(from, rest, target, target_len);
        let missing = self.builder.ins().icmp_imm_s(IntCC::Equal, hit, 0);
        let after_hit = self.builder.ins().isub(hit, text);
        let after_hit = self.builder.ins().iadd(after_hit, target_len);
        let next_seen = self.builder.ins().iadd_imm_s(seen, 1);
        self.builder.ins().brif(
            missing,
            count_done,
            &[BlockArg::Value(seen)],
            count_next,
            &[BlockArg::Value(after_hit), BlockArg::Value(next_seen)],
        );
        self.builder.switch_to_block(count_next);
        let next_at = self.builder.block_params(count_next)[0];
        let next_count = self.builder.block_params(count_next)[1];
        self.builder.ins().jump(
            count_header,
            &[BlockArg::Value(next_at), BlockArg::Value(next_count)],
        );
        self.builder.switch_to_block(count_done);
        let matches = self.builder.block_params(count_done)[0];
        let delta = self.builder.ins().isub(new_len, target_len);
        let growth = self.builder.ins().imul(matches, delta);
        let total = self.builder.ins().iadd(length, growth);
        let result = self.alloc_string(total);
        let fill_header = self.builder.create_block();
        self.builder.append_block_param(fill_header, types::I64);
        self.builder.append_block_param(fill_header, types::I64);
        let fill_hit = self.builder.create_block();
        let fill_tail = self.builder.create_block();
        let fill_done = self.builder.create_block();
        let start = self.zero();
        self.builder.ins().jump(
            fill_header,
            &[BlockArg::Value(start), BlockArg::Value(start)],
        );
        self.builder.switch_to_block(fill_header);
        let at = self.builder.block_params(fill_header)[0];
        let out = self.builder.block_params(fill_header)[1];
        let rest = self.builder.ins().isub(length, at);
        let from = self.builder.ins().iadd(text, at);
        let hit = self.find(from, rest, target, target_len);
        let missing = self.builder.ins().icmp_imm_s(IntCC::Equal, hit, 0);
        self.builder
            .ins()
            .brif(missing, fill_tail, &[], fill_hit, &[]);
        self.builder.switch_to_block(fill_tail);
        let to = self.builder.ins().iadd(result, out);
        self.copy_bytes(to, from, rest);
        self.builder.ins().jump(fill_done, &[]);
        self.builder.switch_to_block(fill_hit);
        let segment = self.builder.ins().isub(hit, from);
        let to = self.builder.ins().iadd(result, out);
        self.copy_bytes(to, from, segment);
        let out = self.builder.ins().iadd(out, segment);
        let to = self.builder.ins().iadd(result, out);
        self.copy_bytes(to, replacement, new_len);
        let out = self.builder.ins().iadd(out, new_len);
        let after = self.builder.ins().isub(hit, text);
        let after = self.builder.ins().iadd(after, target_len);
        self.builder
            .ins()
            .jump(fill_header, &[BlockArg::Value(after), BlockArg::Value(out)]);
        self.builder.switch_to_block(fill_done);
        result
    }

    fn slice(&mut self, text: Value, start: Value, end: Value) -> Value {
        let length = self.builder.ins().isub(end, start);
        let result = self.alloc_string(length);
        let from = self.builder.ins().iadd(text, start);
        self.copy_bytes(result, from, length);
        result
    }

    fn forward_offset(&mut self, text: Value, count: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let check = self.builder.create_block();
        let step = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.zero();
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(zero), BlockArg::Value(zero)]);
        self.builder.switch_to_block(header);
        let offset = self.builder.block_params(header)[0];
        let seen = self.builder.block_params(header)[1];
        let wanted = self.builder.ins().icmp(IntCC::SignedLessThan, seen, count);
        self.builder
            .ins()
            .brif(wanted, check, &[], done, &[BlockArg::Value(offset)]);
        self.builder.switch_to_block(check);
        let inside = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, offset, length);
        self.builder
            .ins()
            .brif(inside, step, &[], done, &[BlockArg::Value(offset)]);
        self.builder.switch_to_block(step);
        let address = self.builder.ins().iadd(text, offset);
        let lead = self.builder.ins().load(types::I8, flags, address, 0);
        let lead = self.builder.ins().uextend(types::I64, lead);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let three = self.builder.ins().iconst(types::I64, 3);
        let four = self.builder.ins().iconst(types::I64, 4);
        let wide = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, lead, 0xF0);
        let upper = self.builder.ins().select(wide, three, four);
        let medium = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, lead, 0xE0);
        let middle = self.builder.ins().select(medium, two, upper);
        let narrow = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, lead, 0x80);
        let width = self.builder.ins().select(narrow, one, middle);
        let next_offset = self.builder.ins().iadd(offset, width);
        let next_seen = self.builder.ins().iadd_imm_s(seen, 1);
        self.builder.ins().jump(
            header,
            &[BlockArg::Value(next_offset), BlockArg::Value(next_seen)],
        );
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn backward_offset(&mut self, text: Value, count: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let check = self.builder.create_block();
        let step = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.zero();
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(length), BlockArg::Value(zero)]);
        self.builder.switch_to_block(header);
        let offset = self.builder.block_params(header)[0];
        let seen = self.builder.block_params(header)[1];
        let wanted = self.builder.ins().icmp(IntCC::SignedLessThan, seen, count);
        self.builder
            .ins()
            .brif(wanted, check, &[], done, &[BlockArg::Value(offset)]);
        self.builder.switch_to_block(check);
        let inside = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThan, offset, 0);
        self.builder
            .ins()
            .brif(inside, step, &[], done, &[BlockArg::Value(offset)]);
        self.builder.switch_to_block(step);
        let earlier = self.scalar_start_before(text, offset);
        let next_seen = self.builder.ins().iadd_imm_s(seen, 1);
        self.builder.ins().jump(
            header,
            &[BlockArg::Value(earlier), BlockArg::Value(next_seen)],
        );
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn last_char(&mut self, text: Value) -> Value {
        let start = self.last_start(text);
        self.char_at(text, start)
    }

    fn last_start(&mut self, text: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        self.scalar_start_before(text, length)
    }

    fn scalar_start_before(&mut self, text: Value, end: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let back = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let first = self.builder.ins().iadd_imm_s(end, -1);
        self.builder.ins().jump(header, &[BlockArg::Value(first)]);
        self.builder.switch_to_block(header);
        let start = self.builder.block_params(header)[0];
        let address = self.builder.ins().iadd(text, start);
        let byte = self.builder.ins().load(types::I8, flags, address, 0);
        let byte = self.builder.ins().uextend(types::I64, byte);
        let marker = self.builder.ins().band_imm_s(byte, 0xC0);
        let continuation = self.builder.ins().icmp_imm_s(IntCC::Equal, marker, 0x80);
        self.builder
            .ins()
            .brif(continuation, back, &[], done, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(back);
        let earlier = self.builder.ins().iadd_imm_s(start, -1);
        self.builder.ins().jump(header, &[BlockArg::Value(earlier)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn reversed_string(&mut self, text: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let result = self.alloc_string(length);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        let nothing = self.zero();
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(length), BlockArg::Value(nothing)]);
        self.builder.switch_to_block(header);
        let end = self.builder.block_params(header)[0];
        let written = self.builder.block_params(header)[1];
        let more = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThan, end, 0);
        self.builder.ins().brif(more, body, &[], exit, &[]);
        self.builder.switch_to_block(body);
        let start = self.scalar_start_before(text, end);
        let width = self.builder.ins().isub(end, start);
        let from = self.builder.ins().iadd(text, start);
        let to = self.builder.ins().iadd(result, written);
        self.copy_bytes(to, from, width);
        let next_written = self.builder.ins().iadd(written, width);
        self.builder.ins().jump(
            header,
            &[BlockArg::Value(start), BlockArg::Value(next_written)],
        );
        self.builder.switch_to_block(exit);
        result
    }

    fn map_ascii(&mut self, text: Value, low: i64, high: i64, delta: i64) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let result = self.alloc_string(length);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        let start = self.zero();
        self.builder.ins().jump(header, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(header);
        let index = self.builder.block_params(header)[0];
        let more = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, index, length);
        self.builder.ins().brif(more, body, &[], exit, &[]);
        self.builder.switch_to_block(body);
        let source = self.builder.ins().iadd(text, index);
        let byte = self.builder.ins().load(types::I8, flags, source, 0);
        let wide = self.builder.ins().uextend(types::I64, byte);
        let shifted = self.builder.ins().iadd_imm_s(wide, -low);
        let inside =
            self.builder
                .ins()
                .icmp_imm_s(IntCC::UnsignedLessThan, shifted, high - low + 1);
        let changed = self.builder.ins().iadd_imm_s(wide, delta);
        let chosen = self.builder.ins().select(inside, changed, wide);
        let narrow = self.builder.ins().ireduce(types::I8, chosen);
        let target = self.builder.ins().iadd(result, index);
        self.builder.ins().store(flags, narrow, target, 0);
        let next = self.builder.ins().iadd_imm_s(index, 1);
        self.builder.ins().jump(header, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(exit);
        result
    }

    fn zero(&mut self) -> Value {
        self.builder.ins().iconst(types::I64, 0)
    }

    fn require_scalar(&mut self, number: Value) {
        let in_range = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, number, 0x0011_0000);
        let shifted = self.builder.ins().iadd_imm_s(number, -0xD800);
        let surrogate = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::UnsignedLessThan, shifted, 0x800);
        let allowed = self.builder.ins().bxor_imm_u(surrogate, 1);
        let valid = self.builder.ins().band(in_range, allowed);
        self.builder.ins().trapz(valid, UNREACHABLE);
    }

    fn memcmp(&mut self, left: Value, right: Value, len: Value) -> Value {
        let mut sig = Signature::new(self.call_conv);
        for _ in 0..3 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I32));
        let memcmp = self.import_func("memcmp", sig, false);
        let call = self.builder.ins().call(memcmp, &[left, right, len]);
        let result = self.builder.inst_results(call)[0];
        self.builder.ins().sextend(types::I64, result)
    }

    fn string_edge(&mut self, text: Value, edge: Value, at_end: bool) -> Value {
        let flags = MemFlagsData::trusted();
        let text_len = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let edge_len = self.builder.ins().load(types::I64, flags, edge, STRING_LEN);
        let fits = self
            .builder
            .ins()
            .icmp(IntCC::SignedGreaterThanOrEqual, text_len, edge_len);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let compared = self.builder.ins().select(fits, edge_len, zero);
        let start = if at_end {
            let offset = self.builder.ins().isub(text_len, edge_len);
            let safe = self.builder.ins().select(fits, offset, zero);
            self.builder.ins().iadd(text, safe)
        } else {
            text
        };
        let order = self.memcmp(start, edge, compared);
        let same = self.builder.ins().icmp_imm_s(IntCC::Equal, order, 0);
        self.builder.ins().band(fits, same)
    }

    fn int_to_string(&mut self, number: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let size = self
            .builder
            .ins()
            .iconst(types::I64, STRING_HEADER as i64 + INT_BUFFER);
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let malloc = self.import_func("malloc", sig, false);
        let call = self.builder.ins().call(malloc, &[size]);
        let block = self.builder.inst_results(call)[0];
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().store(flags, one, block, 0);
        let bytes = self.builder.ins().iadd_imm_s(block, STRING_HEADER as i64);
        let mut sig = Signature::new(self.call_conv);
        for _ in 0..(3 + SNPRINTF_PADDING + 1) {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I32));
        let snprintf = self.import_func("snprintf", sig, false);
        let capacity = self.builder.ins().iconst(types::I64, INT_BUFFER);
        let format = self.data_address(RUNTIME_FORMAT_PLAIN);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut args = vec![bytes, capacity, format];
        args.extend(std::iter::repeat_n(zero, SNPRINTF_PADDING));
        args.push(number);
        let call = self.builder.ins().call(snprintf, &args);
        let written = self.builder.inst_results(call)[0];
        let written = self.builder.ins().sextend(types::I64, written);
        self.builder.ins().store(flags, written, block, 8);
        bytes
    }

    fn string_order(&mut self, left: Value, right: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let left_len = self.builder.ins().load(types::I64, flags, left, STRING_LEN);
        let right_len = self
            .builder
            .ins()
            .load(types::I64, flags, right, STRING_LEN);
        let shortest = self.builder.ins().smin(left_len, right_len);
        let bytes = self.memcmp(left, right, shortest);
        let lengths = self.builder.ins().isub(left_len, right_len);
        let differ = self.builder.ins().icmp_imm_s(IntCC::NotEqual, bytes, 0);
        self.builder.ins().select(differ, bytes, lengths)
    }

    fn alloc_string(&mut self, len: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let size = self
            .builder
            .ins()
            .iadd_imm_s(len, (STRING_HEADER + 1) as i64);
        let mut sig = Signature::new(self.call_conv);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let malloc = self.import_func("malloc", sig, false);
        let call = self.builder.ins().call(malloc, &[size]);
        let block = self.builder.inst_results(call)[0];
        let one = self.builder.ins().iconst(types::I64, 1);
        self.builder.ins().store(flags, one, block, 0);
        self.builder.ins().store(flags, len, block, 8);
        let bytes = self.builder.ins().iadd_imm_s(block, STRING_HEADER as i64);
        let end = self.builder.ins().iadd(bytes, len);
        let zero = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().store(flags, zero, end, 0);
        bytes
    }

    fn copy_bytes(&mut self, destination: Value, source: Value, len: Value) {
        let mut sig = Signature::new(self.call_conv);
        for _ in 0..3 {
            sig.params.push(AbiParam::new(types::I64));
        }
        sig.returns.push(AbiParam::new(types::I64));
        let memcpy = self.import_func("memcpy", sig, false);
        self.builder.ins().call(memcpy, &[destination, source, len]);
    }

    fn retain_string(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let count = this.builder.ins().load(types::I64, flags, value, STRING_RC);
            let immortal = this
                .builder
                .ins()
                .icmp_imm_s(IntCC::SignedLessThan, count, 0);
            let bump = this.builder.create_block();
            let merge = this.builder.create_block();
            this.builder.ins().brif(immortal, merge, &[], bump, &[]);
            this.builder.switch_to_block(bump);
            let next = this.builder.ins().iadd_imm_s(count, 1);
            this.builder.ins().store(flags, next, value, STRING_RC);
            this.builder.ins().jump(merge, &[]);
            this.builder.switch_to_block(merge);
            Ok(())
        })
    }

    fn release_string(&mut self, value: Value) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        self.guarded(value, |this| {
            let count = this.builder.ins().load(types::I64, flags, value, STRING_RC);
            let immortal = this
                .builder
                .ins()
                .icmp_imm_s(IntCC::SignedLessThan, count, 0);
            let counted = this.builder.create_block();
            let merge = this.builder.create_block();
            this.builder.ins().brif(immortal, merge, &[], counted, &[]);
            this.builder.switch_to_block(counted);
            let last = this.builder.ins().icmp_imm_s(IntCC::Equal, count, 1);
            let destroy = this.builder.create_block();
            let decrement = this.builder.create_block();
            this.builder.ins().brif(last, destroy, &[], decrement, &[]);
            this.builder.switch_to_block(destroy);
            let block = this.builder.ins().iadd_imm_s(value, i64::from(STRING_RC));
            this.free_pointer(block);
            this.builder.ins().jump(merge, &[]);
            this.builder.switch_to_block(decrement);
            let next = this.builder.ins().iadd_imm_s(count, -1);
            this.builder.ins().store(flags, next, value, STRING_RC);
            this.builder.ins().jump(merge, &[]);
            this.builder.switch_to_block(merge);
            Ok(())
        })
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
            let descriptor =
                this.builder
                    .ins()
                    .load(types::I64, flags, value, field_offset(DESC_SLOT));
            let drop_fn = this.builder.ins().load(types::I64, flags, descriptor, 0);
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

    fn jump_target(&mut self, depth: usize) -> GResult<&mut JumpBlocks> {
        let index = self
            .jumps
            .len()
            .checked_sub(depth + 1)
            .ok_or_else(|| CodegenError::from("jump outside a loop or a switch".to_string()))?;
        Ok(&mut self.jumps[index])
    }

    fn open_jump_scope(&mut self, exit: Block, next: Option<Block>) {
        self.jumps.push(JumpBlocks {
            exit,
            next,
            exit_used: false,
            next_used: false,
        });
    }

    fn close_jump_scope(&mut self) -> GResult<JumpBlocks> {
        self.jumps
            .pop()
            .ok_or_else(|| CodegenError::from("unbalanced jump scope".to_string()))
    }

    fn block_stmt(&mut self, body: &[TStmt]) -> GResult<bool> {
        let exit = self.builder.create_block();
        self.open_jump_scope(exit, None);
        let terminated = self.stmts(body)?;
        let scope = self.close_jump_scope()?;
        if !terminated {
            self.builder.ins().jump(exit, &[]);
        }
        self.builder.switch_to_block(exit);
        if terminated && !scope.exit_used {
            self.builder.ins().trap(UNREACHABLE);
            return Ok(true);
        }
        Ok(false)
    }

    fn while_stmt(&mut self, cond: &TExpr, body: &[TStmt]) -> GResult<bool> {
        let header = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let cond = self.value(cond)?;
        self.builder.ins().brif(cond, body_block, &[], exit, &[]);
        self.open_jump_scope(exit, Some(header));
        self.branch(body_block, body, header)?;
        self.close_jump_scope()?;
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
        let latch = self.builder.create_block();
        self.builder.switch_to_block(body_block);
        self.open_jump_scope(exit, Some(latch));
        let terminated = self.stmts(body)?;
        let scope = self.close_jump_scope()?;
        if !terminated {
            self.builder.ins().jump(latch, &[]);
        }
        if !terminated || scope.next_used {
            self.builder.switch_to_block(latch);
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

    fn fatal(&mut self, text: &str) {
        let mut flush = Signature::new(self.call_conv);
        flush.params.push(AbiParam::new(types::I64));
        flush.returns.push(AbiParam::new(types::I32));
        let flush = self.import_func("fflush", flush, false);
        let everything = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().call(flush, &[everything]);
        let mut write = Signature::new(self.call_conv);
        write.params.push(AbiParam::new(types::I32));
        write.params.push(AbiParam::new(types::I64));
        write.params.push(AbiParam::new(types::I64));
        write.returns.push(AbiParam::new(types::I64));
        let write = self.import_func("write", write, false);
        let standard_error = self.builder.ins().iconst(types::I32, 2);
        let address = self.string_address(text);
        let length = self
            .builder
            .ins()
            .iconst(types::I64, i64::try_from(text.len()).unwrap_or(0));
        self.builder
            .ins()
            .call(write, &[standard_error, address, length]);
        self.builder.ins().trap(UNREACHABLE);
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
            TExpr::StrOp { op, args } => self.string_op(*op, args)?,
            TExpr::ArrayOp { op, elem, args, .. } => self.array_op(*op, *elem, args)?,
            TExpr::Witness { .. } | TExpr::Dispatch { .. } => {
                return Err(CodegenError::from(
                    "a protocol requirement was not resolved",
                ));
            }
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
            TExpr::Char(value) => self.builder.ins().iconst(types::I64, i64::from(*value)),
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
            TExpr::Binary { op, lhs, rhs }
                if lhs.ty() == Ty::Str && matches!(op, BinOp::Eq | BinOp::Ne) =>
            {
                let lhs = self.value(lhs)?;
                let rhs = self.value(rhs)?;
                let same = self.string_equals(lhs, rhs);
                if *op == BinOp::Ne {
                    self.builder.ins().bxor_imm_u(same, 1)
                } else {
                    same
                }
            }
            TExpr::Binary { op, lhs, rhs } if lhs.ty() == Ty::Str => {
                let lhs = self.value(lhs)?;
                let rhs = self.value(rhs)?;
                let order = self.string_order(lhs, rhs);
                let condition = comparison(*op)
                    .ok_or_else(|| CodegenError::from("a string supports only comparisons"))?;
                self.builder.ins().icmp_imm_s(condition, order, 0)
            }
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
            TExpr::RetainWeak(inner) => {
                let value = self.value(inner)?;
                let flags = MemFlagsData::trusted();
                let tag = self.builder.ins().load(types::I64, flags, value, 0);
                self.guarded(tag, |this| {
                    let payload =
                        this.builder
                            .ins()
                            .load(types::I64, flags, value, field_offset(1));
                    this.retain_unowned(payload)
                })?;
                value
            }
            TExpr::LoadWeak(inner) => {
                let ty = inner.ty();
                let source = self.value(inner)?;
                let flags = MemFlagsData::trusted();
                let dest = self.alloc_struct(ty)?;
                let zero = self.builder.ins().iconst(types::I64, 0);
                self.builder.ins().store(flags, zero, dest, 0);
                self.builder.ins().store(flags, zero, dest, field_offset(1));
                let tag = self.builder.ins().load(types::I64, flags, source, 0);
                self.guarded(tag, |this| {
                    let payload =
                        this.builder
                            .ins()
                            .load(types::I64, flags, source, field_offset(1));
                    let strong =
                        this.builder
                            .ins()
                            .load(types::I64, flags, payload, field_offset(RC_SLOT));
                    this.guarded(strong, |this| {
                        let next = this.builder.ins().iadd_imm_s(strong, 1);
                        this.builder
                            .ins()
                            .store(flags, next, payload, field_offset(RC_SLOT));
                        this.builder.ins().store(flags, tag, dest, 0);
                        this.builder
                            .ins()
                            .store(flags, payload, dest, field_offset(1));
                        Ok(())
                    })
                })?;
                dest
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
            TExpr::ClassDesc(_) => {
                return Err(CodegenError::from("a class descriptor was not resolved"));
            }
            TExpr::Descriptor(descriptor) => self.descriptor_address(descriptor),
            TExpr::VCall { slot, args, ret } => return self.table_call(*slot, args, *ret),
            TExpr::InstanceOf { object, class } => {
                let object = self.value(object)?;
                let target = self.value(class)?;
                self.instance_of(object, target)
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
            _ => {
                let smallest = self.builder.ins().icmp_imm_s(IntCC::Equal, lhs, i64::MIN);
                let minus_one = self.builder.ins().icmp_imm_s(IntCC::Equal, rhs, -1);
                let overflow = self.builder.ins().band(smallest, minus_one);
                self.builder
                    .ins()
                    .trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
                return self.builder.ins().srem(lhs, rhs);
            }
        };
        self.builder
            .ins()
            .trapnz(overflow, TrapCode::INTEGER_OVERFLOW);
        result
    }

    fn string_equals(&mut self, lhs: Value, rhs: Value) -> Value {
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let advance = self.builder.create_block();
        let compare_end = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I8);
        let start = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(header, &[BlockArg::Value(start)]);
        self.builder.switch_to_block(header);
        let offset = self.builder.block_params(header)[0];
        let flags = MemFlagsData::trusted();
        let left_address = self.builder.ins().iadd(lhs, offset);
        let right_address = self.builder.ins().iadd(rhs, offset);
        let left = self.builder.ins().load(types::I8, flags, left_address, 0);
        let right = self.builder.ins().load(types::I8, flags, right_address, 0);
        let differ = self.builder.ins().icmp(IntCC::NotEqual, left, right);
        let no = self.builder.ins().iconst(types::I8, 0);
        self.builder
            .ins()
            .brif(differ, done, &[BlockArg::Value(no)], compare_end, &[]);
        self.builder.switch_to_block(compare_end);
        let at_end = self.builder.ins().icmp_imm_s(IntCC::Equal, left, 0);
        let yes = self.builder.ins().iconst(types::I8, 1);
        self.builder
            .ins()
            .brif(at_end, done, &[BlockArg::Value(yes)], advance, &[]);
        self.builder.switch_to_block(advance);
        let next = self.builder.ins().iadd_imm_s(offset, 1);
        self.builder.ins().jump(header, &[BlockArg::Value(next)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
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

    fn descriptor_address(&mut self, descriptor: &Descriptor) -> Value {
        for link in descriptor.chain() {
            if self.literals.iter().any(|l| l.name() == link.name()) {
                continue;
            }
            let slots = DESC_METHODS + link.methods().len();
            let mut relocs = vec![(0, SymbolRef::Func(symbol(link.drop())))];
            if let Some(parent) = link.parent() {
                let offset = u32::try_from(DESC_PARENT * 8).unwrap_or(0);
                relocs.push((offset, SymbolRef::Data(parent.name().to_string())));
            }
            for (index, method) in link.methods().iter().enumerate() {
                let offset = u32::try_from((DESC_METHODS + index) * 8).unwrap_or(0);
                relocs.push((offset, SymbolRef::Func(symbol(method))));
            }
            self.literals.push(
                Literal::new(link.name().to_string(), vec![0; slots * 8]).with_relocs(relocs),
            );
        }
        self.data_address(descriptor.name())
    }

    fn table_call(&mut self, slot: usize, args: &[TExpr], ret: Ty) -> GResult<Option<Value>> {
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
        let flags = MemFlagsData::trusted();
        let descriptor =
            self.builder
                .ins()
                .load(types::I64, flags, receiver, field_offset(DESC_SLOT));
        let entry = self.builder.ins().load(
            types::I64,
            flags,
            descriptor,
            field_offset(DESC_METHODS + slot),
        );
        let signature = self.builder.import_signature(sig);
        let call = self.builder.ins().call_indirect(signature, entry, &values);
        Ok(result_slot.or_else(|| self.builder.inst_results(call).first().copied()))
    }

    fn instance_of(&mut self, object: Value, target: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let walk = self.builder.create_block();
        self.builder.append_block_param(walk, types::I64);
        let compare = self.builder.create_block();
        let step = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I8);
        let first = self
            .builder
            .ins()
            .load(types::I64, flags, object, field_offset(DESC_SLOT));
        self.builder.ins().jump(walk, &[BlockArg::Value(first)]);
        self.builder.switch_to_block(walk);
        let current = self.builder.block_params(walk)[0];
        let no = self.builder.ins().iconst(types::I8, 0);
        self.builder
            .ins()
            .brif(current, compare, &[], done, &[BlockArg::Value(no)]);
        self.builder.switch_to_block(compare);
        let same = self.builder.ins().icmp(IntCC::Equal, current, target);
        let yes = self.builder.ins().iconst(types::I8, 1);
        self.builder
            .ins()
            .brif(same, done, &[BlockArg::Value(yes)], step, &[]);
        self.builder.switch_to_block(step);
        let parent = self
            .builder
            .ins()
            .load(types::I64, flags, current, field_offset(DESC_PARENT));
        self.builder.ins().jump(walk, &[BlockArg::Value(parent)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
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

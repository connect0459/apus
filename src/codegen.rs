use std::collections::HashMap;

use cranelift_codegen::Context;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    AbiParam, Block, BlockArg, InstBuilder, MemFlagsData, Signature, StackSlotData, StackSlotKind,
    TrapCode, Type, Value, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ast::{BinOp, UnaryOp};
use crate::hash::Fnv1a;
use crate::tir::{TExpr, TFunc, TStmt, TStruct};
use crate::types::Ty;

const VARIADIC_PADDING: usize = 7;
const FIELD_SIZE: u32 = 8;
const UNREACHABLE: TrapCode = TrapCode::unwrap_user(1);

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

pub fn compile(funcs: &[TFunc], structs: &[TStruct]) -> GResult<Vec<u8>> {
    let mut module = new_module()?;
    let runtime = Runtime::declare(&mut module)?;
    let mut pool = StringPool::default();
    let mut ids = HashMap::new();
    for func in funcs {
        let sig = signature(&module, func.param_types(), func.ret());
        let id = module.declare_function(&symbol(func.name()), Linkage::Export, &sig)?;
        ids.insert(func.name().to_string(), id);
    }
    let mut ctx = module.make_context();
    let mut builder_ctx = FunctionBuilderContext::new();
    for func in funcs {
        ctx.func.signature = signature(&module, func.param_types(), func.ret());
        let env = Env {
            runtime: &runtime,
            ids: &ids,
            structs,
        };
        emit_function(
            &mut module,
            &mut ctx,
            &mut builder_ctx,
            &mut pool,
            env,
            func,
        )?;
        let id = ids[func.name()];
        module.define_function(id, &mut ctx)?;
        module.clear_context(&mut ctx);
    }
    define_entry(&mut module, &mut ctx, &mut builder_ctx, &ids)?;
    Ok(module.finish().emit()?)
}

fn symbol(name: &str) -> String {
    format!("apus_{name}")
}

fn clif_type(ty: Ty) -> Option<Type> {
    match ty {
        Ty::Int | Ty::Str | Ty::Struct(_) => Some(types::I64),
        Ty::Bool => Some(types::I8),
        Ty::Void => None,
    }
}

fn return_type(ret: Ty) -> Option<Type> {
    if ret.is_struct() {
        None
    } else {
        clif_type(ret)
    }
}

fn signature(module: &ObjectModule, params: &[Ty], ret: Ty) -> Signature {
    let mut sig = module.make_signature();
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

fn new_module() -> GResult<ObjectModule> {
    let mut flags = settings::builder();
    flags.set("opt_level", "none")?;
    flags.set("is_pic", "true")?;
    let isa = cranelift_native::builder()?.finish(settings::Flags::new(flags))?;
    let builder = ObjectBuilder::new(isa, "apus", default_libcall_names())?;
    Ok(ObjectModule::new(builder))
}

#[derive(Default)]
struct StringPool {
    literals: HashMap<u64, DataId>,
}

impl StringPool {
    fn intern(&mut self, module: &mut ObjectModule, text: &str) -> GResult<DataId> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let mut hasher = Fnv1a::new();
        hasher.write(&bytes);
        let key = hasher.finish();
        if let Some(id) = self.literals.get(&key) {
            return Ok(*id);
        }
        let id = define_text(module, &format!("apus_str_{key:016x}"), &bytes)?;
        self.literals.insert(key, id);
        Ok(id)
    }
}

struct Runtime {
    printf: FuncId,
    puts: FuncId,
    format_int: DataId,
    text_true: DataId,
    text_false: DataId,
}

impl Runtime {
    fn declare(module: &mut ObjectModule) -> GResult<Self> {
        let mut printf_sig = module.make_signature();
        for _ in 0..(2 + VARIADIC_PADDING) {
            printf_sig.params.push(AbiParam::new(types::I64));
        }
        printf_sig.returns.push(AbiParam::new(types::I32));
        let printf = module.declare_function("printf", Linkage::Import, &printf_sig)?;
        let mut puts_sig = module.make_signature();
        puts_sig.params.push(AbiParam::new(types::I64));
        puts_sig.returns.push(AbiParam::new(types::I32));
        let puts = module.declare_function("puts", Linkage::Import, &puts_sig)?;
        Ok(Self {
            printf,
            puts,
            format_int: define_text(module, "apus_format_int", b"%ld\n\0")?,
            text_true: define_text(module, "apus_true", b"true\0")?,
            text_false: define_text(module, "apus_false", b"false\0")?,
        })
    }
}

fn define_text(module: &mut ObjectModule, name: &str, bytes: &[u8]) -> GResult<DataId> {
    let id = module.declare_data(name, Linkage::Local, false, false)?;
    let mut data = DataDescription::new();
    data.define(bytes.to_vec().into_boxed_slice());
    module.define_data(id, &data)?;
    Ok(id)
}

#[derive(Clone, Copy)]
struct Env<'e> {
    runtime: &'e Runtime,
    ids: &'e HashMap<String, FuncId>,
    structs: &'e [TStruct],
}

fn emit_function(
    module: &mut ObjectModule,
    ctx: &mut Context,
    builder_ctx: &mut FunctionBuilderContext,
    pool: &mut StringPool,
    env: Env,
    func: &TFunc,
) -> GResult<()> {
    let target = module.target_config();
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
    for (var, value) in vars.iter().zip(params) {
        builder.def_var(*var, value);
    }
    let mut emitter = Emitter {
        module,
        pool,
        env,
        builder,
        vars,
        local_types: func.locals().to_vec(),
        ret: func.ret(),
        sret,
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
    emitter.builder.finalize(target);
    Ok(())
}

struct Emitter<'m, 'f> {
    module: &'m mut ObjectModule,
    pool: &'m mut StringPool,
    env: Env<'m>,
    builder: FunctionBuilder<'f>,
    vars: Vec<Variable>,
    local_types: Vec<Ty>,
    ret: Ty,
    sret: Option<Value>,
}

fn field_offset(index: usize) -> i32 {
    i32::try_from(index).map_or(i32::MAX, |i| i.saturating_mul(FIELD_SIZE as i32))
}

impl Emitter<'_, '_> {
    fn struct_fields(&self, ty: Ty) -> GResult<&[Ty]> {
        match ty {
            Ty::Struct(id) => self
                .env
                .structs
                .get(id.index())
                .map(TStruct::fields)
                .ok_or_else(|| CodegenError::from("unknown struct")),
            other => Err(CodegenError::from(format!("{other:?} is not a struct"))),
        }
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
                let value = self.value(value)?;
                let base = self.builder.use_var(self.vars[local.index()]);
                let flags = MemFlagsData::trusted();
                self.builder
                    .ins()
                    .store(flags, value, base, field_offset(*index));
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
        }
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

    fn data_address(&mut self, id: DataId) -> Value {
        let global = self.module.declare_data_in_func(id, self.builder.func);
        self.builder.ins().symbol_value(types::I64, global)
    }

    fn print_int(&mut self, value: Value) {
        let format = self.data_address(self.env.runtime.format_int);
        let printf = self
            .module
            .declare_func_in_func(self.env.runtime.printf, self.builder.func);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let mut args = vec![format];
        args.extend(std::iter::repeat_n(zero, VARIADIC_PADDING));
        args.push(value);
        self.builder.ins().call(printf, &args);
    }

    fn print_bool(&mut self, value: Value) {
        let yes = self.data_address(self.env.runtime.text_true);
        let no = self.data_address(self.env.runtime.text_false);
        let text = self.builder.ins().select(value, yes, no);
        self.puts(text);
    }

    fn puts(&mut self, text: Value) {
        let puts = self
            .module
            .declare_func_in_func(self.env.runtime.puts, self.builder.func);
        self.builder.ins().call(puts, &[text]);
    }

    fn value(&mut self, expr: &TExpr) -> GResult<Value> {
        self.expr(expr)?
            .ok_or_else(|| CodegenError::from("an expression without a value was used as a value"))
    }

    fn expr(&mut self, expr: &TExpr) -> GResult<Option<Value>> {
        let value = match expr {
            TExpr::Int(n) => self.builder.ins().iconst(types::I64, *n),
            TExpr::Bool(b) => self.builder.ins().iconst(types::I8, i64::from(*b)),
            TExpr::Str(text) => {
                let id = self.pool.intern(self.module, text)?;
                self.data_address(id)
            }
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
            TExpr::Call { callee, args, ret } => return self.call(callee, args, *ret),
            TExpr::StructInit { ty, args } => {
                let values = args
                    .iter()
                    .map(|a| self.value(a))
                    .collect::<GResult<Vec<_>>>()?;
                let dest = self.alloc_struct(*ty)?;
                let flags = MemFlagsData::trusted();
                for (index, value) in values.into_iter().enumerate() {
                    self.builder
                        .ins()
                        .store(flags, value, dest, field_offset(index));
                }
                dest
            }
            TExpr::Field { base, index, ty } => {
                let base = self.value(base)?;
                let clif = clif_type(*ty)
                    .ok_or_else(|| CodegenError::from("a field has no representable type"))?;
                let flags = MemFlagsData::trusted();
                self.builder
                    .ins()
                    .load(clif, flags, base, field_offset(*index))
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

    fn call(&mut self, callee: &str, args: &[TExpr], ret: Ty) -> GResult<Option<Value>> {
        let id = *self
            .env
            .ids
            .get(callee)
            .ok_or_else(|| CodegenError::from(format!("unknown function '{callee}'")))?;
        let result_slot = if ret.is_struct() {
            Some(self.alloc_struct(ret)?)
        } else {
            None
        };
        let mut values: Vec<Value> = result_slot.into_iter().collect();
        for arg in args {
            values.push(self.value(arg)?);
        }
        let func_ref = self.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(func_ref, &values);
        Ok(result_slot.or_else(|| self.builder.inst_results(call).first().copied()))
    }
}

fn comparison(op: BinOp) -> Option<IntCC> {
    match op {
        BinOp::Eq => Some(IntCC::Equal),
        BinOp::Ne => Some(IntCC::NotEqual),
        BinOp::Lt => Some(IntCC::SignedLessThan),
        BinOp::Gt => Some(IntCC::SignedGreaterThan),
        BinOp::Le => Some(IntCC::SignedLessThanOrEqual),
        BinOp::Ge => Some(IntCC::SignedGreaterThanOrEqual),
        _ => None,
    }
}

fn define_entry(
    module: &mut ObjectModule,
    ctx: &mut Context,
    builder_ctx: &mut FunctionBuilderContext,
    ids: &HashMap<String, FuncId>,
) -> GResult<()> {
    let main_id = *ids
        .get("main")
        .ok_or_else(|| CodegenError::from("no 'main' function"))?;
    let mut sig = module.make_signature();
    sig.returns.push(AbiParam::new(types::I32));
    let entry_id = module.declare_function("main", Linkage::Export, &sig)?;
    ctx.func.signature = sig;
    let target = module.target_config();
    let mut builder = FunctionBuilder::new(&mut ctx.func, builder_ctx);
    let block = builder.create_block();
    builder.switch_to_block(block);
    let callee = module.declare_func_in_func(main_id, builder.func);
    builder.ins().call(callee, &[]);
    let status = builder.ins().iconst(types::I32, 0);
    builder.ins().return_(&[status]);
    builder.seal_all_blocks();
    builder.finalize(target);
    module.define_function(entry_id, ctx)?;
    module.clear_context(ctx);
    Ok(())
}

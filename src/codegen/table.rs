use super::*;

const HASH_PRIME: i64 = 0x0000_0100_0000_01b3;
const HASH_OFFSET: i64 = 0xcbf2_9ce4_8422_2325_u64 as i64;
const HASH_GOLDEN: i64 = 0x9E37_79B9_7F4A_7C15_u64 as i64;
const EMPTY_SLOT: i64 = -1;
const REMOVED: i64 = -1;
const MIN_TABLE_CAPACITY: i64 = 4;
const TABLE_OBJECT_SLOTS: i64 = 9;

pub(super) struct Shape {
    key: Ty,
    value: Ty,
    key_slots: i64,
    stride: i64,
    hash: String,
    equal: String,
}

impl Emitter<'_, '_> {
    pub(super) fn string_hash(&mut self, text: Value) -> Value {
        let flags = MemFlagsData::trusted();
        let length = self.builder.ins().load(types::I64, flags, text, STRING_LEN);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.zero();
        let seed = self.builder.ins().iconst(types::I64, HASH_OFFSET);
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(zero), BlockArg::Value(seed)]);
        self.builder.switch_to_block(header);
        let index = self.builder.block_params(header)[0];
        let acc = self.builder.block_params(header)[1];
        let more = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, index, length);
        self.builder
            .ins()
            .brif(more, body, &[], done, &[BlockArg::Value(acc)]);
        self.builder.switch_to_block(body);
        let address = self.builder.ins().iadd(text, index);
        let byte = self.builder.ins().load(types::I8, flags, address, 0);
        let byte = self.builder.ins().uextend(types::I64, byte);
        let mixed = self.builder.ins().bxor(acc, byte);
        let next_acc = self.builder.ins().imul_imm_s(mixed, HASH_PRIME);
        let next = self.builder.ins().iadd_imm_s(index, 1);
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(next), BlockArg::Value(next_acc)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    pub(super) fn hash_mix(&mut self, acc: Value, value: Value) -> Value {
        let mixed = self.builder.ins().bxor(acc, value);
        self.builder.ins().imul_imm_s(mixed, HASH_PRIME)
    }

    fn shape(&self, key: Ty, value: Ty, hash: &str, equal: &str) -> GResult<Shape> {
        let slots = |ty: Ty| -> GResult<i64> {
            Ok(match ty {
                Ty::Void => 0,
                t if t.is_struct() => i64::try_from(self.struct_fields(t)?.len())?,
                _ => 1,
            })
        };
        let key_slots = slots(key)?;
        let value_slots = slots(value)?;
        Ok(Shape {
            key,
            value,
            key_slots,
            stride: (1 + key_slots + value_slots) * ELEMENT_SIZE,
            hash: hash.to_string(),
            equal: equal.to_string(),
        })
    }

    fn tload(&mut self, table: Value, slot: usize) -> Value {
        self.builder.ins().load(
            types::I64,
            MemFlagsData::trusted(),
            table,
            field_offset(slot),
        )
    }

    fn tstore(&mut self, table: Value, slot: usize, value: Value) {
        self.builder
            .ins()
            .store(MemFlagsData::trusted(), value, table, field_offset(slot));
    }

    fn entry_address(&mut self, shape: &Shape, entries: Value, index: Value) -> Value {
        let at = self.builder.ins().imul_imm_s(index, shape.stride);
        self.builder.ins().iadd(entries, at)
    }

    fn part(&mut self, entry: Value, offset: i64, ty: Ty) -> GResult<Value> {
        if ty.is_struct() {
            return Ok(self.builder.ins().iadd_imm_s(entry, offset));
        }
        let clif = self.element_type(ty)?;
        Ok(self
            .builder
            .ins()
            .load(clif, MemFlagsData::trusted(), entry, i32::try_from(offset)?))
    }

    fn entry_key(&mut self, shape: &Shape, entry: Value) -> GResult<Value> {
        self.part(entry, ELEMENT_SIZE, shape.key)
    }

    fn value_offset(shape: &Shape) -> i64 {
        ELEMENT_SIZE + shape.key_slots * ELEMENT_SIZE
    }

    fn entry_value(&mut self, shape: &Shape, entry: Value) -> GResult<Value> {
        self.part(entry, Self::value_offset(shape), shape.value)
    }

    fn counted(&self, ty: Ty) -> GResult<bool> {
        Ok(match ty {
            Ty::Str | Ty::Class(_) => true,
            Ty::Struct(_) => !self.counted_slots(ty)?.is_empty(),
            _ => false,
        })
    }

    fn retain_part(&mut self, entry: Value, offset: i64, ty: Ty) -> GResult<()> {
        if ty == Ty::Void || !self.counted(ty)? {
            return Ok(());
        }
        let value = self.part(entry, offset, ty)?;
        self.retain(value, ty)
    }

    fn release_part(&mut self, entry: Value, offset: i64, ty: Ty) -> GResult<()> {
        if ty == Ty::Void || !self.counted(ty)? {
            return Ok(());
        }
        let value = self.part(entry, offset, ty)?;
        self.release(value, ty)
    }

    fn call_values(
        &mut self,
        callee: &str,
        params: &[Ty],
        ret: Ty,
        arguments: &[Value],
    ) -> GResult<Value> {
        let sig = signature(self.call_conv, params, ret);
        let func_ref = self.import_func(&symbol(callee), sig, true);
        let call = self.builder.ins().call(func_ref, arguments);
        self.builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or_else(|| CodegenError::from("a key function returned no value"))
    }

    fn table_hash(&mut self, shape: &Shape, key: Value) -> GResult<Value> {
        let raw = match shape.key {
            Ty::Str => self.string_hash(key),
            Ty::Bool => {
                let wide = self.builder.ins().uextend(types::I64, key);
                self.scramble(wide)
            }
            Ty::Int | Ty::Char | Ty::Enum(_) => self.scramble(key),
            other => {
                if shape.hash.is_empty() {
                    return Err(CodegenError::from("a key type has no hash function"));
                }
                let hash = shape.hash.clone();
                self.call_values(&hash, &[other], Ty::Int, &[key])?
            }
        };
        Ok(self.builder.ins().band_imm_s(raw, i64::MAX))
    }

    fn scramble(&mut self, value: Value) -> Value {
        let product = self.builder.ins().imul_imm_s(value, HASH_GOLDEN);
        let high = self.builder.ins().ushr_imm_s(product, 32);
        self.builder.ins().bxor(product, high)
    }

    fn keys_equal(&mut self, shape: &Shape, left: Value, right: Value) -> GResult<Value> {
        Ok(match shape.key {
            Ty::Str => self.string_equals(left, right),
            Ty::Int | Ty::Char | Ty::Enum(_) | Ty::Bool => {
                self.builder.ins().icmp(IntCC::Equal, left, right)
            }
            other => {
                if shape.equal.is_empty() {
                    return Err(CodegenError::from("a key type has no equality function"));
                }
                let equal = shape.equal.clone();
                self.call_values(&equal, &[other, other], Ty::Bool, &[left, right])?
            }
        })
    }

    fn table_find(
        &mut self,
        shape: &Shape,
        dict: Value,
        key: Value,
        hash: Value,
    ) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let probe = self.builder.create_block();
        let used = self.tload(dict, DICT_USED);
        let missing = self.builder.ins().iconst(types::I64, EMPTY_SLOT);
        let none = self.builder.ins().icmp_imm_s(IntCC::Equal, used, 0);
        self.builder
            .ins()
            .brif(none, done, &[BlockArg::Value(missing)], probe, &[]);
        self.builder.switch_to_block(probe);
        let entries = self.tload(dict, DICT_ENTRIES);
        let table = self.tload(dict, DICT_TABLE);
        let capacity = self.tload(dict, DICT_CAPACITY);
        let slots = self.builder.ins().imul_imm_s(capacity, 2);
        let mask = self.builder.ins().iadd_imm_s(slots, -1);
        let first = self.builder.ins().band(hash, mask);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.ins().jump(header, &[BlockArg::Value(first)]);
        self.builder.switch_to_block(header);
        let position = self.builder.block_params(header)[0];
        let at = self.builder.ins().imul_imm_s(position, ELEMENT_SIZE);
        let cell = self.builder.ins().iadd(table, at);
        let slot = self.builder.ins().load(types::I64, flags, cell, 0);
        let vacant = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::Equal, slot, EMPTY_SLOT);
        let check = self.builder.create_block();
        self.builder
            .ins()
            .brif(vacant, done, &[BlockArg::Value(missing)], check, &[]);
        self.builder.switch_to_block(check);
        let entry = self.entry_address(shape, entries, slot);
        let stored = self.builder.ins().load(types::I64, flags, entry, 0);
        let same_hash = self.builder.ins().icmp(IntCC::Equal, stored, hash);
        let compare = self.builder.create_block();
        let next = self.builder.create_block();
        self.builder.ins().brif(same_hash, compare, &[], next, &[]);
        self.builder.switch_to_block(compare);
        let stored_key = self.entry_key(shape, entry)?;
        let equal = self.keys_equal(shape, stored_key, key)?;
        self.builder
            .ins()
            .brif(equal, done, &[BlockArg::Value(slot)], next, &[]);
        self.builder.switch_to_block(next);
        let stepped = self.builder.ins().iadd_imm_s(position, 1);
        let wrapped = self.builder.ins().band(stepped, mask);
        self.builder.ins().jump(header, &[BlockArg::Value(wrapped)]);
        self.builder.switch_to_block(done);
        Ok(self.builder.block_params(done)[0])
    }

    fn probe_insert(&mut self, table: Value, mask: Value, hash: Value, index: Value) {
        let flags = MemFlagsData::trusted();
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let advance = self.builder.create_block();
        let stored = self.builder.create_block();
        let first = self.builder.ins().band(hash, mask);
        self.builder.ins().jump(header, &[BlockArg::Value(first)]);
        self.builder.switch_to_block(header);
        let position = self.builder.block_params(header)[0];
        let at = self.builder.ins().imul_imm_s(position, ELEMENT_SIZE);
        let cell = self.builder.ins().iadd(table, at);
        let slot = self.builder.ins().load(types::I64, flags, cell, 0);
        let vacant = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::Equal, slot, EMPTY_SLOT);
        self.builder.ins().brif(vacant, stored, &[], advance, &[]);
        self.builder.switch_to_block(advance);
        let stepped = self.builder.ins().iadd_imm_s(position, 1);
        let wrapped = self.builder.ins().band(stepped, mask);
        self.builder.ins().jump(header, &[BlockArg::Value(wrapped)]);
        self.builder.switch_to_block(stored);
        self.builder.ins().store(flags, index, cell, 0);
    }

    fn table_capacity_for(&mut self, entries: Value) -> Value {
        let doubled = self.builder.ins().imul_imm_s(entries, 2);
        let floor = self.builder.ins().iconst(types::I64, MIN_TABLE_CAPACITY);
        let wanted = self.builder.ins().smax(doubled, floor);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        let grow = self.builder.ins().iconst(types::I64, MIN_TABLE_CAPACITY);
        let step = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        self.builder.ins().jump(header, &[BlockArg::Value(grow)]);
        self.builder.switch_to_block(header);
        let capacity = self.builder.block_params(header)[0];
        let short = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThan, capacity, wanted);
        self.builder
            .ins()
            .brif(short, step, &[], done, &[BlockArg::Value(capacity)]);
        self.builder.switch_to_block(step);
        let twice = self.builder.ins().imul_imm_s(capacity, 2);
        self.builder.ins().jump(header, &[BlockArg::Value(twice)]);
        self.builder.switch_to_block(done);
        self.builder.block_params(done)[0]
    }

    fn rebuild(
        &mut self,
        shape: &Shape,
        source: Value,
        capacity: Value,
        retain: bool,
    ) -> GResult<(Value, Value, Value)> {
        let flags = MemFlagsData::trusted();
        let bytes = self.builder.ins().imul_imm_s(capacity, shape.stride);
        let entries = self.allocate(bytes)?;
        let slots = self.builder.ins().imul_imm_s(capacity, 2);
        let table_bytes = self.builder.ins().imul_imm_s(slots, ELEMENT_SIZE);
        let table = self.allocate(table_bytes)?;
        let vacant = self.builder.ins().iconst(types::I64, EMPTY_SLOT);
        self.each_element(slots, |this, index| {
            let at = this.builder.ins().imul_imm_s(index, ELEMENT_SIZE);
            let cell = this.builder.ins().iadd(table, at);
            this.builder.ins().store(flags, vacant, cell, 0);
            Ok(())
        })?;
        let mask = self.builder.ins().iadd_imm_s(slots, -1);
        let used = self.tload(source, DICT_USED);
        let from = self.tload(source, DICT_ENTRIES);
        let header = self.builder.create_block();
        self.builder.append_block_param(header, types::I64);
        self.builder.append_block_param(header, types::I64);
        let body = self.builder.create_block();
        let copy = self.builder.create_block();
        let latch = self.builder.create_block();
        self.builder.append_block_param(latch, types::I64);
        self.builder.append_block_param(latch, types::I64);
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I64);
        let zero = self.zero();
        self.builder
            .ins()
            .jump(header, &[BlockArg::Value(zero), BlockArg::Value(zero)]);
        self.builder.switch_to_block(header);
        let index = self.builder.block_params(header)[0];
        let kept = self.builder.block_params(header)[1];
        let more = self.builder.ins().icmp(IntCC::SignedLessThan, index, used);
        self.builder
            .ins()
            .brif(more, body, &[], done, &[BlockArg::Value(kept)]);
        self.builder.switch_to_block(body);
        let source_entry = self.entry_address(shape, from, index);
        let hash = self.builder.ins().load(types::I64, flags, source_entry, 0);
        let live = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, hash, 0);
        let next = self.builder.ins().iadd_imm_s(index, 1);
        self.builder.ins().brif(
            live,
            copy,
            &[],
            latch,
            &[BlockArg::Value(next), BlockArg::Value(kept)],
        );
        self.builder.switch_to_block(copy);
        let target_entry = self.entry_address(shape, entries, kept);
        let size = self.builder.ins().iconst(types::I64, shape.stride);
        self.copy_bytes(target_entry, source_entry, size);
        if retain {
            self.retain_part(target_entry, ELEMENT_SIZE, shape.key)?;
            self.retain_part(target_entry, Self::value_offset(shape), shape.value)?;
        }
        self.probe_insert(table, mask, hash, kept);
        let kept_next = self.builder.ins().iadd_imm_s(kept, 1);
        self.builder
            .ins()
            .jump(latch, &[BlockArg::Value(next), BlockArg::Value(kept_next)]);
        self.builder.switch_to_block(latch);
        let index_next = self.builder.block_params(latch)[0];
        let kept_now = self.builder.block_params(latch)[1];
        self.builder.ins().jump(
            header,
            &[BlockArg::Value(index_next), BlockArg::Value(kept_now)],
        );
        self.builder.switch_to_block(done);
        let live_count = self.builder.block_params(done)[0];
        Ok((entries, table, live_count))
    }

    fn unique_table(&mut self, shape: &Shape, dict: Value, extra: i64) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let reuse = self.builder.create_block();
        let copy = self.builder.create_block();
        let strong = self
            .builder
            .ins()
            .load(types::I64, flags, dict, field_offset(RC_SLOT));
        let sole = self.builder.ins().icmp_imm_s(IntCC::Equal, strong, 1);
        self.builder.ins().brif(sole, reuse, &[], copy, &[]);

        self.builder.switch_to_block(reuse);
        let used = self.tload(dict, DICT_USED);
        let capacity = self.tload(dict, DICT_CAPACITY);
        let needed = self.builder.ins().iadd_imm_s(used, extra);
        let fits = self
            .builder
            .ins()
            .icmp(IntCC::SignedLessThanOrEqual, needed, capacity);
        let grow = self.builder.create_block();
        let keep = self.builder.create_block();
        self.builder.ins().brif(fits, keep, &[], grow, &[]);
        self.builder.switch_to_block(grow);
        let live = self.tload(dict, DICT_COUNT);
        let wanted_entries = self.builder.ins().iadd_imm_s(live, extra);
        let wanted = self.table_capacity_for(wanted_entries);
        let (entries, table, kept) = self.rebuild(shape, dict, wanted, false)?;
        let old_entries = self.tload(dict, DICT_ENTRIES);
        let old_table = self.tload(dict, DICT_TABLE);
        self.free_pointer(old_entries);
        self.free_pointer(old_table);
        self.tstore(dict, DICT_ENTRIES, entries);
        self.tstore(dict, DICT_TABLE, table);
        self.tstore(dict, DICT_CAPACITY, wanted);
        self.tstore(dict, DICT_USED, kept);
        self.builder.ins().jump(keep, &[]);
        self.builder.switch_to_block(keep);
        self.retain_reference(dict)?;
        self.builder.ins().jump(merge, &[BlockArg::Value(dict)]);

        self.builder.switch_to_block(copy);
        let live = self.tload(dict, DICT_COUNT);
        let wanted_entries = self.builder.ins().iadd_imm_s(live, extra);
        let wanted = self.table_capacity_for(wanted_entries);
        let (entries, table, kept) = self.rebuild(shape, dict, wanted, true)?;
        let size = self
            .builder
            .ins()
            .iconst(types::I64, TABLE_OBJECT_SLOTS * ELEMENT_SIZE);
        let fresh = self.allocate(size)?;
        for slot in 0..DICT_COUNT {
            let value = self.tload(dict, slot);
            self.tstore(fresh, slot, value);
        }
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.zero();
        self.tstore(fresh, RC_SLOT, one);
        self.tstore(fresh, UNOWNED_SLOT, zero);
        self.tstore(fresh, DICT_COUNT, live);
        self.tstore(fresh, DICT_USED, kept);
        self.tstore(fresh, DICT_CAPACITY, wanted);
        self.tstore(fresh, DICT_ENTRIES, entries);
        self.tstore(fresh, DICT_TABLE, table);
        self.builder.ins().jump(merge, &[BlockArg::Value(fresh)]);

        self.builder.switch_to_block(merge);
        Ok(self.builder.block_params(merge)[0])
    }

    fn table_set(
        &mut self,
        shape: &Shape,
        dict: Value,
        key: Value,
        value: Option<Value>,
        overwrite: bool,
    ) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let key = self.detach(key, shape.key)?;
        let value = match value {
            Some(value) => Some(self.detach(value, shape.value)?),
            None => None,
        };
        let hash = self.table_hash(shape, key)?;
        let table = self.unique_table(shape, dict, 1)?;
        let found = self.table_find(shape, table, key, hash)?;
        let replace = self.builder.create_block();
        let insert = self.builder.create_block();
        let done = self.builder.create_block();
        let present = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, found, 0);
        self.builder.ins().brif(present, replace, &[], insert, &[]);

        self.builder.switch_to_block(replace);
        let entries = self.tload(table, DICT_ENTRIES);
        let entry = self.entry_address(shape, entries, found);
        if overwrite {
            self.release_part(entry, Self::value_offset(shape), shape.value)?;
            if let Some(value) = value {
                self.put_element(entry, Self::value_offset(shape), value, shape.value)?;
            }
        } else if let Some(value) = value
            && self.counted(shape.value)?
        {
            self.release(value, shape.value)?;
        }
        self.release(key, shape.key)?;
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(insert);
        let used = self.tload(table, DICT_USED);
        let entries = self.tload(table, DICT_ENTRIES);
        let entry = self.entry_address(shape, entries, used);
        self.builder.ins().store(flags, hash, entry, 0);
        self.put_element(entry, ELEMENT_SIZE, key, shape.key)?;
        if let Some(value) = value {
            self.put_element(entry, Self::value_offset(shape), value, shape.value)?;
        }
        let capacity = self.tload(table, DICT_CAPACITY);
        let slots = self.builder.ins().imul_imm_s(capacity, 2);
        let mask = self.builder.ins().iadd_imm_s(slots, -1);
        let cells = self.tload(table, DICT_TABLE);
        self.probe_insert(cells, mask, hash, used);
        let next_used = self.builder.ins().iadd_imm_s(used, 1);
        self.tstore(table, DICT_USED, next_used);
        let count = self.tload(table, DICT_COUNT);
        let next_count = self.builder.ins().iadd_imm_s(count, 1);
        self.tstore(table, DICT_COUNT, next_count);
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(done);
        Ok(table)
    }

    fn table_remove(&mut self, shape: &Shape, dict: Value, key: Value) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let key = self.detach(key, shape.key)?;
        let hash = self.table_hash(shape, key)?;
        let table = self.unique_table(shape, dict, 0)?;
        let found = self.table_find(shape, table, key, hash)?;
        let remove = self.builder.create_block();
        let done = self.builder.create_block();
        let present = self
            .builder
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, found, 0);
        self.builder.ins().brif(present, remove, &[], done, &[]);
        self.builder.switch_to_block(remove);
        let entries = self.tload(table, DICT_ENTRIES);
        let entry = self.entry_address(shape, entries, found);
        self.release_part(entry, ELEMENT_SIZE, shape.key)?;
        self.release_part(entry, Self::value_offset(shape), shape.value)?;
        let removed = self.builder.ins().iconst(types::I64, REMOVED);
        self.builder.ins().store(flags, removed, entry, 0);
        let count = self.tload(table, DICT_COUNT);
        let next = self.builder.ins().iadd_imm_s(count, -1);
        self.tstore(table, DICT_COUNT, next);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        Ok(table)
    }

    pub(super) fn drop_table(&mut self, table: Value, key: Ty, value: Ty) -> GResult<()> {
        let flags = MemFlagsData::trusted();
        let shape = self.shape(key, value, "", "")?;
        let entries = self.tload(table, DICT_ENTRIES);
        if self.counted(key)? || self.counted(value)? {
            let used = self.tload(table, DICT_USED);
            self.each_element(used, |this, index| {
                let entry = this.entry_address(&shape, entries, index);
                let hash = this.builder.ins().load(types::I64, flags, entry, 0);
                let live = this
                    .builder
                    .ins()
                    .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, hash, 0);
                let release = this.builder.create_block();
                let skip = this.builder.create_block();
                this.builder.ins().brif(live, release, &[], skip, &[]);
                this.builder.switch_to_block(release);
                this.release_part(entry, ELEMENT_SIZE, key)?;
                this.release_part(entry, Self::value_offset(&shape), value)?;
                this.builder.ins().jump(skip, &[]);
                this.builder.switch_to_block(skip);
                Ok(())
            })?;
        }
        let cells = self.tload(table, DICT_TABLE);
        self.free_pointer(entries);
        self.free_pointer(cells);
        Ok(())
    }

    pub(super) fn dict_op(
        &mut self,
        op: DictOp,
        key: Ty,
        value: Ty,
        hash: &str,
        equal: &str,
        args: &[TExpr],
    ) -> GResult<Value> {
        let flags = MemFlagsData::trusted();
        let shape = self.shape(key, value, hash, equal)?;
        let values = args
            .iter()
            .map(|a| self.value(a))
            .collect::<GResult<Vec<_>>>()?;
        match op {
            DictOp::Find => {
                let digest = self.table_hash(&shape, values[1])?;
                self.table_find(&shape, values[0], values[1], digest)
            }
            DictOp::Set | DictOp::Ensure => {
                let stored = values.get(2).copied();
                self.table_set(&shape, values[0], values[1], stored, op == DictOp::Set)
            }
            DictOp::Store => {
                let stored = self.detach(values[2], shape.value)?;
                let entries = self.tload(values[0], DICT_ENTRIES);
                let entry = self.entry_address(&shape, entries, values[1]);
                self.release_part(entry, Self::value_offset(&shape), shape.value)?;
                self.put_element(entry, Self::value_offset(&shape), stored, shape.value)?;
                self.retain_reference(values[0])?;
                Ok(values[0])
            }
            DictOp::Remove => self.table_remove(&shape, values[0], values[1]),
            DictOp::Key | DictOp::Value | DictOp::Live => {
                let entries = self.tload(values[0], DICT_ENTRIES);
                let entry = self.entry_address(&shape, entries, values[1]);
                match op {
                    DictOp::Key => self.entry_key(&shape, entry),
                    DictOp::Value => self.entry_value(&shape, entry),
                    _ => {
                        let hash = self.builder.ins().load(types::I64, flags, entry, 0);
                        Ok(self
                            .builder
                            .ins()
                            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, hash, 0))
                    }
                }
            }
        }
    }
}

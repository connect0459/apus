use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use std::time::UNIX_EPOCH;

use crate::ast::{Decl, FuncDecl};
use crate::cache::Cache;
use crate::check::{CheckError, Signatures, check_func};
use crate::codegen::{Backend, CodegenError};
use crate::entry::FuncEntry;
use crate::hash::{Fnv1a, hash_bytes};
use crate::items::{Item, ItemKind, SplitError, signature_digest, split_items};
use crate::key::function_key;
use crate::lexer::{LexError, lex};
use crate::link::{LinkError, link};
use crate::memo::ItemMemo;
use crate::mono::Instantiator;
use crate::parser::{ParseError, parse_item, parse_signature};
use crate::rc;
use crate::stamp::BuildStamp;
use crate::tir::TFunc;
use crate::types::StructId;

pub use crate::link::Linker;

#[derive(Debug, Clone, PartialEq)]
pub struct BuildError {
    line: Option<u32>,
    message: String,
}

impl BuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            line: None,
            message: message.into(),
        }
    }

    fn at(line: u32, message: impl Into<String>) -> Self {
        Self {
            line: Some(line),
            message: message.into(),
        }
    }

    pub fn line(&self) -> Option<u32> {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl From<LexError> for BuildError {
    fn from(e: LexError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<SplitError> for BuildError {
    fn from(e: SplitError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<ParseError> for BuildError {
    fn from(e: ParseError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<CheckError> for BuildError {
    fn from(e: CheckError) -> Self {
        Self::at(e.line(), e.message())
    }
}

impl From<CodegenError> for BuildError {
    fn from(e: CodegenError) -> Self {
        Self::new(e.message())
    }
}

impl From<LinkError> for BuildError {
    fn from(e: LinkError) -> Self {
        Self::new(e.message())
    }
}

pub fn build(source: &str, output: &Path) -> Result<BuildReport, BuildError> {
    build_with(source, output, &BuildOptions::default())
}

pub fn build_with(
    source: &str,
    output: &Path,
    options: &BuildOptions,
) -> Result<BuildReport, BuildError> {
    let cache_dir = options
        .cache_dir
        .clone()
        .unwrap_or_else(|| default_cache_dir(output));
    let compiler_stamp = options
        .compiler_stamp
        .unwrap_or_else(default_compiler_stamp);
    let source_hash = hash_bytes(source.as_bytes());
    let previous = BuildStamp::load(&cache_dir).filter(|s| s.compiler_stamp() == compiler_stamp);
    if previous
        .as_ref()
        .is_some_and(|p| p.source_hash() == source_hash && p.output_is_intact(output))
    {
        return Ok(BuildReport {
            skipped: true,
            ..BuildReport::default()
        });
    }

    let tokens = lex(source)?;
    let items = split_items(&tokens)?;
    let digest = signature_digest(&items);
    let memo = ItemMemo::load(&cache_dir).filter(|m| m.is_valid_for(compiler_stamp, digest));
    let memo_valid = memo.is_some();
    let mut cache = Cache::open(&cache_dir);

    let mut slots: Vec<Slot> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !is_function(item) || item.is_generic() {
            continue;
        }
        let known = memo.as_ref().and_then(|m| {
            m.function_keys(item.hash())?
                .iter()
                .map(|key| Some((*key, cache.get(*key)?.clone())))
                .collect::<Option<Vec<_>>>()
        });
        slots.push(Slot {
            item_index: index,
            item_hash: item.hash(),
            known,
            unit: None,
        });
    }
    if !items
        .iter()
        .any(|i| i.kind() == ItemKind::Func && !i.is_generic() && i.name() == "main")
    {
        return Err(BuildError::new("no 'main' function"));
    }

    let dirty: Vec<usize> = (0..slots.len())
        .filter(|slot| slots[*slot].known.is_none())
        .collect();
    let mut layouts = Vec::new();
    let mut instance_units: Vec<Unit> = Vec::new();
    if !dirty.is_empty() {
        let mut needs_body = vec![false; items.len()];
        for (index, item) in items.iter().enumerate() {
            needs_body[index] = !is_function(item) || item.is_generic();
        }
        for slot in &dirty {
            needs_body[slots[*slot].item_index] = true;
        }
        let mut decls = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let parse = if needs_body[index] {
                parse_item
            } else {
                parse_signature
            };
            decls.push(match (parse(item.tokens())?, item.owner()) {
                (Decl::Func(func), Some(owner)) => Decl::Func(func.with_owner(owner)),
                (decl, _) => decl,
            });
        }
        let sigs = Signatures::collect(&decls)?;
        let mut generic_decls: HashMap<&str, &FuncDecl> = HashMap::new();
        for (index, item) in items.iter().enumerate() {
            if let (true, Decl::Func(func)) = (item.is_generic(), &decls[index]) {
                generic_decls.insert(func.name(), func);
            }
        }
        let mut generic_checked: HashMap<String, Rc<TFunc>> = HashMap::new();
        if !memo_valid {
            for func in generic_decls.values() {
                generic_checked.insert(func.name().to_string(), Rc::new(check_func(func, &sigs)?));
            }
        }
        let mut provider = |name: &str| -> Result<Rc<TFunc>, String> {
            if let Some(found) = generic_checked.get(name) {
                return Ok(Rc::clone(found));
            }
            let func = generic_decls
                .get(name)
                .ok_or_else(|| format!("cannot find generic function '{name}'"))?;
            let typed = Rc::new(check_func(func, &sigs).map_err(|e| e.message().to_string())?);
            generic_checked.insert(name.to_string(), Rc::clone(&typed));
            Ok(typed)
        };
        let mut instantiator = Instantiator::new(&sigs, &mut provider);
        let mut resolved: Vec<(usize, TFunc, Vec<usize>, Vec<StructId>)> = Vec::new();
        for slot in &dirty {
            let Decl::Func(func) = &decls[slots[*slot].item_index] else {
                continue;
            };
            let typed = check_func(func, &sigs)?;
            let (typed, direct) = instantiator.resolve(&typed).map_err(BuildError::new)?;
            let (typed, created) = rc::insert(&typed, &sigs);
            resolved.push((*slot, typed, direct, created));
        }
        layouts = sigs.layouts();
        let counted_instances: Vec<(TFunc, Vec<StructId>)> = instantiator
            .instances()
            .iter()
            .map(|f| rc::insert(f, &sigs))
            .collect();
        let instance_keys: Vec<u64> = counted_instances
            .iter()
            .map(|(f, _)| function_key(f, &layouts, compiler_stamp))
            .collect();
        let mut drops: HashMap<StructId, Unit> = HashMap::new();
        for (slot, typed, direct, created) in resolved {
            let key = function_key(&typed, &layouts, compiler_stamp);
            let mut instances: Vec<usize> = instantiator.closure(&direct);
            instances.sort_unstable();
            let mut keys: Vec<u64> = instances.iter().map(|id| instance_keys[*id]).collect();
            let mut classes = created;
            for id in &instances {
                classes.extend(counted_instances[*id].1.iter().copied());
            }
            for class in classes {
                let unit = drops.entry(class).or_insert_with(|| {
                    let func = rc::drop_function(&sigs, class);
                    Unit {
                        key: function_key(&func, &layouts, compiler_stamp),
                        func,
                    }
                });
                if !keys.contains(&unit.key) {
                    keys.push(unit.key);
                }
                if !instance_units.iter().any(|u| u.key == unit.key) {
                    instance_units.push(Unit {
                        key: unit.key,
                        func: unit.func.clone(),
                    });
                }
            }
            slots[slot].unit = Some((Unit { key, func: typed }, keys));
            for id in instances {
                if !instance_units.iter().any(|u| u.key == instance_keys[id]) {
                    instance_units.push(Unit {
                        key: instance_keys[id],
                        func: counted_instances[id].0.clone(),
                    });
                }
            }
        }
    }

    let backend = Backend::new()?;
    let mut compiled_entries: HashMap<u64, FuncEntry> = HashMap::new();
    let mut missing: Vec<&Unit> = Vec::new();
    let own_units = slots
        .iter()
        .filter_map(|s| s.unit.as_ref().map(|(unit, _)| unit));
    for unit in own_units.chain(instance_units.iter()) {
        if compiled_entries.contains_key(&unit.key) || missing.iter().any(|m| m.key == unit.key) {
            continue;
        }
        match cache.get(unit.key) {
            Some(entry) => {
                compiled_entries.insert(unit.key, entry.clone());
            }
            None => missing.push(unit),
        }
    }
    let to_compile: Vec<&TFunc> = missing.iter().map(|u| &u.func).collect();
    let threads = options
        .threads
        .unwrap_or_else(|| automatic_threads(to_compile.len()));
    let compiled = backend.compile_many(&to_compile, &layouts, threads)?;
    let compiled_count = missing.len();
    for (unit, entry) in missing.iter().zip(compiled) {
        cache.put(unit.key, entry.clone());
        compiled_entries.insert(unit.key, entry);
    }

    let mut entries: Vec<FuncEntry> = Vec::new();
    let mut emitted: HashSet<u64> = HashSet::new();
    let mut memo_pairs: Vec<(u64, Vec<u64>)> = Vec::new();
    for slot in slots {
        let keyed: Vec<(u64, FuncEntry)> = match (slot.known, slot.unit) {
            (Some(known), _) => known,
            (None, Some((unit, instance_keys))) => std::iter::once(unit.key)
                .chain(instance_keys)
                .filter_map(|key| compiled_entries.get(&key).map(|e| (key, e.clone())))
                .collect(),
            (None, None) => Vec::new(),
        };
        memo_pairs.push((slot.item_hash, keyed.iter().map(|(key, _)| *key).collect()));
        for (key, entry) in keyed {
            if emitted.insert(key) {
                entries.push(entry);
            }
        }
    }
    let object = backend.assemble(&entries)?;
    let _ = cache.save();
    let _ = ItemMemo::new(compiler_stamp, digest, memo_pairs).save(&cache_dir);

    let object_hash = hash_bytes(&object);
    let link_needed = !previous
        .as_ref()
        .is_some_and(|p| p.object_hash() == object_hash && p.output_is_intact(output));
    if link_needed {
        link(&object, output, options.linker, &cache_dir)?;
        if let Some(stamp) = BuildStamp::new(source_hash, compiler_stamp, object_hash, output) {
            let _ = stamp.save(&cache_dir);
        }
    } else if let Some(previous) = previous {
        let _ = previous.with_source_hash(source_hash).save(&cache_dir);
    }
    Ok(BuildReport {
        skipped: false,
        checked: dirty.len(),
        compiled: compiled_count,
        reused: entries.len().saturating_sub(compiled_count),
        linked: link_needed,
    })
}

fn is_function(item: &Item) -> bool {
    matches!(item.kind(), ItemKind::Func | ItemKind::Method)
}

struct Unit {
    key: u64,
    func: TFunc,
}

struct Slot {
    item_index: usize,
    item_hash: u64,
    known: Option<Vec<(u64, FuncEntry)>>,
    unit: Option<(Unit, Vec<u64>)>,
}

const FUNCTIONS_PER_THREAD: usize = 16;

fn automatic_threads(functions: usize) -> usize {
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    cores.min(functions / FUNCTIONS_PER_THREAD).max(1)
}

fn default_cache_dir(output: &Path) -> PathBuf {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    parent.join(".apus-cache")
}

fn default_compiler_stamp() -> u64 {
    let mut hasher = Fnv1a::new();
    hasher.write(env!("CARGO_PKG_VERSION").as_bytes());
    if let Some(metadata) = std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::metadata(exe).ok())
    {
        hasher.write(&metadata.len().to_le_bytes());
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok());
        hasher.write(&modified.map_or(0, |d| d.as_nanos()).to_le_bytes());
    }
    hasher.finish()
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    cache_dir: Option<PathBuf>,
    linker: Linker,
    compiler_stamp: Option<u64>,
    threads: Option<usize>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            cache_dir: None,
            linker: Linker::Ld,
            compiler_stamp: None,
            threads: None,
        }
    }
}

impl BuildOptions {
    pub fn with_cache_dir(mut self, dir: PathBuf) -> Self {
        self.cache_dir = Some(dir);
        self
    }

    pub fn with_linker(mut self, linker: Linker) -> Self {
        self.linker = linker;
        self
    }

    pub fn with_compiler_stamp(mut self, stamp: u64) -> Self {
        self.compiler_stamp = Some(stamp);
        self
    }

    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildReport {
    skipped: bool,
    checked: usize,
    compiled: usize,
    reused: usize,
    linked: bool,
}

impl BuildReport {
    pub fn was_skipped(&self) -> bool {
        self.skipped
    }

    pub fn checked(&self) -> usize {
        self.checked
    }

    pub fn compiled(&self) -> usize {
        self.compiled
    }

    pub fn reused(&self) -> usize {
        self.reused
    }

    pub fn linked(&self) -> bool {
        self.linked
    }
}

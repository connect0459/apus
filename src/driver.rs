use std::path::{Path, PathBuf};

use std::time::UNIX_EPOCH;

use crate::ast::Decl;
use crate::cache::Cache;
use crate::check::{CheckError, Signatures, check_func};
use crate::codegen::{Backend, CodegenError};
use crate::entry::FuncEntry;
use crate::hash::{Fnv1a, hash_bytes};
use crate::items::{ItemKind, SplitError, signature_digest, split_items};
use crate::key::function_key;
use crate::lexer::{LexError, lex};
use crate::link::{LinkError, link};
use crate::memo::ItemMemo;
use crate::parser::{ParseError, parse_item, parse_signature};
use crate::stamp::BuildStamp;
use crate::tir::TFunc;

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
    let mut cache = Cache::open(&cache_dir);

    let mut slots: Vec<Slot> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if item.kind() != ItemKind::Func {
            continue;
        }
        let known = memo.as_ref().and_then(|m| {
            let key = m.function_key(item.hash())?;
            let entry = cache.get(key)?.clone();
            Some((key, entry))
        });
        slots.push(Slot {
            item_index: index,
            item_hash: item.hash(),
            known,
            checked: None,
        });
    }

    let dirty: Vec<usize> = (0..slots.len())
        .filter(|slot| slots[*slot].known.is_none())
        .collect();
    let mut layouts = Vec::new();
    if !dirty.is_empty() {
        let mut needs_body = vec![false; items.len()];
        for slot in &dirty {
            needs_body[slots[*slot].item_index] = true;
        }
        let mut decls = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let parse = if item.kind() == ItemKind::Struct || needs_body[index] {
                parse_item
            } else {
                parse_signature
            };
            decls.push(parse(item.tokens())?);
        }
        let sigs = Signatures::collect(&decls)?;
        layouts = sigs.layouts();
        for slot in &dirty {
            if let Decl::Func(func) = &decls[slots[*slot].item_index] {
                let typed = check_func(func, &sigs)?;
                let key = function_key(&typed, &layouts, compiler_stamp);
                let entry = cache.get(key).cloned();
                slots[*slot].known = entry.map(|e| (key, e));
                slots[*slot].checked = Some((key, typed));
            }
        }
    }
    if !items
        .iter()
        .any(|i| i.kind() == ItemKind::Func && i.name() == "main")
    {
        return Err(BuildError::new("no 'main' function"));
    }

    let backend = Backend::new()?;
    let missing: Vec<usize> = (0..slots.len())
        .filter(|slot| slots[*slot].known.is_none())
        .collect();
    let to_compile: Vec<&TFunc> = missing
        .iter()
        .filter_map(|slot| slots[*slot].checked.as_ref().map(|(_, typed)| typed))
        .collect();
    let threads = options
        .threads
        .unwrap_or_else(|| automatic_threads(to_compile.len()));
    let compiled = backend.compile_many(&to_compile, &layouts, threads)?;
    for (slot, entry) in missing.iter().zip(compiled) {
        if let Some((key, _)) = &slots[*slot].checked {
            let key = *key;
            cache.put(key, entry.clone());
            slots[*slot].known = Some((key, entry));
        }
    }
    let memo_pairs: Vec<(u64, u64)> = slots
        .iter()
        .filter_map(|s| s.known.as_ref().map(|(key, _)| (s.item_hash, *key)))
        .collect();
    let entries: Vec<FuncEntry> = slots
        .into_iter()
        .filter_map(|s| s.known.map(|(_, entry)| entry))
        .collect();
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
        compiled: missing.len(),
        reused: entries.len() - missing.len(),
        linked: link_needed,
    })
}

struct Slot {
    item_index: usize,
    item_hash: u64,
    known: Option<(u64, FuncEntry)>,
    checked: Option<(u64, TFunc)>,
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::entry::FuncEntry;

pub struct Cache {
    dir: PathBuf,
    entries: HashMap<u64, FuncEntry>,
    used: HashSet<u64>,
}

const PACK_FILE: &str = "functions.pack";
const PACK_MAGIC: [u8; 4] = *b"APCK";
const PACK_VERSION: u8 = 1;

impl Cache {
    pub fn open(dir: &Path) -> Cache {
        let entries = std::fs::read(dir.join(PACK_FILE))
            .ok()
            .and_then(|bytes| decode_pack(&bytes))
            .unwrap_or_default();
        Cache {
            dir: dir.to_path_buf(),
            entries,
            used: HashSet::new(),
        }
    }

    pub fn get(&mut self, key: u64) -> Option<&FuncEntry> {
        let entry = self.entries.get(&key)?;
        self.used.insert(key);
        Some(entry)
    }

    pub fn put(&mut self, key: u64, entry: FuncEntry) {
        self.entries.insert(key, entry);
        self.used.insert(key);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn save(&self) -> std::io::Result<()> {
        let mut keys: Vec<u64> = self.used.iter().copied().collect();
        keys.sort_unstable();
        let mut out = Vec::new();
        out.extend_from_slice(&PACK_MAGIC);
        out.push(PACK_VERSION);
        let mut blobs = Vec::new();
        for key in keys {
            let Some(blob) = self.entries.get(&key).and_then(FuncEntry::encode) else {
                continue;
            };
            blobs.push((key, blob));
        }
        out.extend_from_slice(&(blobs.len() as u64).to_le_bytes());
        for (key, blob) in blobs {
            out.extend_from_slice(&key.to_le_bytes());
            out.extend_from_slice(&(blob.len() as u64).to_le_bytes());
            out.extend_from_slice(&blob);
        }
        let path = self.dir.join(PACK_FILE);
        if std::fs::read(&path).is_ok_and(|existing| existing == out) {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dir)?;
        let temporary = self.dir.join(format!("{PACK_FILE}.tmp"));
        std::fs::write(&temporary, &out)?;
        std::fs::rename(&temporary, &path)
    }
}

fn decode_pack(bytes: &[u8]) -> Option<HashMap<u64, FuncEntry>> {
    let header = PACK_MAGIC.len() + 1;
    if bytes.len() < header + 8 || bytes[..PACK_MAGIC.len()] != PACK_MAGIC {
        return None;
    }
    if bytes[PACK_MAGIC.len()] != PACK_VERSION {
        return None;
    }
    let mut rest = &bytes[header..];
    let count = read_u64(&mut rest)?;
    let mut entries = HashMap::new();
    for _ in 0..count {
        let key = read_u64(&mut rest)?;
        let length = usize::try_from(read_u64(&mut rest)?).ok()?;
        if length > rest.len() {
            return None;
        }
        let (blob, tail) = rest.split_at(length);
        entries.insert(key, FuncEntry::decode(blob)?);
        rest = tail;
    }
    rest.is_empty().then_some(entries)
}

fn read_u64(rest: &mut &[u8]) -> Option<u64> {
    if rest.len() < 8 {
        return None;
    }
    let (head, tail) = rest.split_at(8);
    *rest = tail;
    Some(u64::from_le_bytes(head.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryReloc, SymbolRef};
    use cranelift_codegen::binemit::Reloc;

    fn entry(name: &str) -> FuncEntry {
        FuncEntry::new(name.to_string(), 4, vec![1, 2, 3, 4], vec![], vec![])
    }

    fn reopened(dir: &Path) -> Cache {
        Cache::open(dir)
    }

    #[test]
    fn a_cache_over_an_empty_directory_has_no_entries() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Cache::open(dir.path()).is_empty());
    }

    #[test]
    fn saved_entries_are_found_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::open(dir.path());
        cache.put(1, entry("apus_a"));
        cache.put(2, entry("apus_b"));
        cache.save().unwrap();
        let mut again = reopened(dir.path());
        assert_eq!(again.get(1), Some(&entry("apus_a")));
        assert_eq!(again.get(2), Some(&entry("apus_b")));
        assert_eq!(again.get(3), None);
    }

    #[test]
    fn only_the_entries_used_in_a_session_survive_saving() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = Cache::open(dir.path());
        first.put(1, entry("apus_a"));
        first.put(2, entry("apus_b"));
        first.save().unwrap();
        let mut second = reopened(dir.path());
        assert!(second.get(1).is_some());
        second.save().unwrap();
        let mut third = reopened(dir.path());
        assert!(third.get(1).is_some());
        assert!(third.get(2).is_none());
        assert_eq!(third.len(), 1);
    }

    #[test]
    fn saving_creates_the_cache_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let mut cache = Cache::open(&nested);
        cache.put(1, entry("apus_a"));
        cache.save().unwrap();
        assert!(reopened(&nested).len() == 1);
    }

    #[test]
    fn a_corrupt_cache_file_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::open(dir.path());
        cache.put(1, entry("apus_a"));
        cache.save().unwrap();
        let file = dir.path().join("functions.pack");
        std::fs::write(&file, b"this is not a cache").unwrap();
        assert!(reopened(dir.path()).is_empty());
    }

    #[test]
    fn a_truncated_cache_file_yields_no_partial_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = Cache::open(dir.path());
        cache.put(1, entry("apus_a"));
        cache.put(2, entry("apus_b"));
        cache.save().unwrap();
        let file = dir.path().join("functions.pack");
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, &bytes[..bytes.len() - 3]).unwrap();
        assert!(reopened(dir.path()).is_empty());
    }

    #[test]
    fn an_entry_that_cannot_be_encoded_is_not_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let odd = FuncEntry::new(
            "apus_a".to_string(),
            4,
            vec![0; 4],
            vec![EntryReloc::new(
                0,
                Reloc::X86PCRel4,
                SymbolRef::Func("g".to_string()),
                0,
            )],
            vec![],
        );
        let mut cache = Cache::open(dir.path());
        cache.put(1, odd);
        cache.put(2, entry("apus_b"));
        cache.save().unwrap();
        let mut again = reopened(dir.path());
        assert!(again.get(1).is_none());
        assert!(again.get(2).is_some());
    }
}

use std::collections::HashMap;
use std::path::Path;

const MEMO_FILE: &str = "items";
const MEMO_MAGIC: [u8; 4] = *b"APIM";
const MEMO_VERSION: u8 = 2;
const HEADER_LEN: usize = 4 + 1 + 8 + 8 + 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMemo {
    compiler_stamp: u64,
    signature_digest: u64,
    keys: HashMap<u64, Vec<u64>>,
}

impl ItemMemo {
    pub fn new(
        compiler_stamp: u64,
        signature_digest: u64,
        pairs: impl IntoIterator<Item = (u64, Vec<u64>)>,
    ) -> Self {
        Self {
            compiler_stamp,
            signature_digest,
            keys: pairs.into_iter().collect(),
        }
    }

    pub fn is_valid_for(&self, compiler_stamp: u64, signature_digest: u64) -> bool {
        self.compiler_stamp == compiler_stamp && self.signature_digest == signature_digest
    }

    pub fn function_keys(&self, item_hash: u64) -> Option<&[u64]> {
        self.keys.get(&item_hash).map(Vec::as_slice)
    }

    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(MEMO_FILE)).ok()?;
        if bytes.len() < HEADER_LEN || bytes[..4] != MEMO_MAGIC || bytes[4] != MEMO_VERSION {
            return None;
        }
        let word = |start: usize| -> Option<u64> {
            Some(u64::from_le_bytes(
                bytes.get(start..start + 8)?.try_into().ok()?,
            ))
        };
        let count = usize::try_from(word(21)?).ok()?;
        let mut keys = HashMap::new();
        let mut at = HEADER_LEN;
        for _ in 0..count {
            let item_hash = word(at)?;
            let length = usize::try_from(word(at + 8)?).ok()?;
            let end = at.checked_add(16)?.checked_add(length.checked_mul(8)?)?;
            if end > bytes.len() {
                return None;
            }
            let list = (0..length)
                .map(|i| word(at + 16 + i * 8))
                .collect::<Option<Vec<_>>>()?;
            keys.insert(item_hash, list);
            at = end;
        }
        if at != bytes.len() {
            return None;
        }
        Some(Self {
            compiler_stamp: word(5)?,
            signature_digest: word(13)?,
            keys,
        })
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut pairs: Vec<(&u64, &Vec<u64>)> = self.keys.iter().collect();
        pairs.sort_unstable();
        let mut out = Vec::new();
        out.extend_from_slice(&MEMO_MAGIC);
        out.push(MEMO_VERSION);
        out.extend_from_slice(&self.compiler_stamp.to_le_bytes());
        out.extend_from_slice(&self.signature_digest.to_le_bytes());
        out.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
        for (item_hash, list) in pairs {
            out.extend_from_slice(&item_hash.to_le_bytes());
            out.extend_from_slice(&(list.len() as u64).to_le_bytes());
            for key in list {
                out.extend_from_slice(&key.to_le_bytes());
            }
        }
        let temporary = dir.join(format!("{MEMO_FILE}.tmp"));
        std::fs::write(&temporary, &out)?;
        std::fs::rename(&temporary, dir.join(MEMO_FILE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ItemMemo {
        ItemMemo::new(7, 9, [(1, vec![100]), (2, vec![200, 201, 202])])
    }

    #[test]
    fn a_saved_memo_loads_back_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        sample().save(dir.path()).unwrap();
        assert_eq!(ItemMemo::load(dir.path()), Some(sample()));
    }

    #[test]
    fn a_memo_is_valid_only_for_its_compiler_and_signature_digest() {
        let memo = sample();
        assert!(memo.is_valid_for(7, 9));
        assert!(!memo.is_valid_for(8, 9));
        assert!(!memo.is_valid_for(7, 10));
    }

    #[test]
    fn a_memo_answers_the_key_of_a_known_item_only() {
        let memo = sample();
        assert_eq!(memo.function_keys(2), Some(&[200, 201, 202][..]));
        assert_eq!(memo.function_keys(3), None);
    }

    #[test]
    fn a_missing_or_corrupt_memo_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ItemMemo::load(dir.path()), None);
        std::fs::write(dir.path().join(MEMO_FILE), b"corrupt").unwrap();
        assert_eq!(ItemMemo::load(dir.path()), None);
    }

    #[test]
    fn a_truncated_memo_is_ignored_as_a_whole() {
        let dir = tempfile::tempdir().unwrap();
        sample().save(dir.path()).unwrap();
        let path = dir.path().join(MEMO_FILE);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(ItemMemo::load(dir.path()), None);
    }
}

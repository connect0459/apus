use std::path::Path;
use std::time::UNIX_EPOCH;

const STAMP_FILE: &str = "stamp";
const STAMP_MAGIC: [u8; 4] = *b"APST";
const STAMP_VERSION: u8 = 1;
const STAMP_LEN: usize = 4 + 1 + 5 * 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildStamp {
    source_hash: u64,
    compiler_stamp: u64,
    object_hash: u64,
    output_len: u64,
    output_modified_nanos: u64,
}

impl BuildStamp {
    pub fn new(
        source_hash: u64,
        compiler_stamp: u64,
        object_hash: u64,
        output: &Path,
    ) -> Option<Self> {
        let (output_len, output_modified_nanos) = output_fingerprint(output)?;
        Some(Self {
            source_hash,
            compiler_stamp,
            object_hash,
            output_len,
            output_modified_nanos,
        })
    }

    pub fn source_hash(&self) -> u64 {
        self.source_hash
    }

    pub fn compiler_stamp(&self) -> u64 {
        self.compiler_stamp
    }

    pub fn object_hash(&self) -> u64 {
        self.object_hash
    }

    pub fn output_is_intact(&self, output: &Path) -> bool {
        output_fingerprint(output) == Some((self.output_len, self.output_modified_nanos))
    }

    pub fn with_source_hash(self, source_hash: u64) -> Self {
        Self {
            source_hash,
            ..self
        }
    }

    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(STAMP_FILE)).ok()?;
        if bytes.len() != STAMP_LEN || bytes[..4] != STAMP_MAGIC || bytes[4] != STAMP_VERSION {
            return None;
        }
        let field = |index: usize| -> Option<u64> {
            let start = 5 + index * 8;
            Some(u64::from_le_bytes(
                bytes.get(start..start + 8)?.try_into().ok()?,
            ))
        };
        Some(Self {
            source_hash: field(0)?,
            compiler_stamp: field(1)?,
            object_hash: field(2)?,
            output_len: field(3)?,
            output_modified_nanos: field(4)?,
        })
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut out = Vec::with_capacity(STAMP_LEN);
        out.extend_from_slice(&STAMP_MAGIC);
        out.push(STAMP_VERSION);
        for value in [
            self.source_hash,
            self.compiler_stamp,
            self.object_hash,
            self.output_len,
            self.output_modified_nanos,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        let temporary = dir.join(format!("{STAMP_FILE}.tmp"));
        std::fs::write(&temporary, &out)?;
        std::fs::rename(&temporary, dir.join(STAMP_FILE))
    }
}

fn output_fingerprint(path: &Path) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let nanos = u64::try_from(modified.as_nanos()).ok()?;
    Some((metadata.len(), nanos))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp_for(output: &Path) -> BuildStamp {
        BuildStamp::new(1, 2, 3, output).unwrap()
    }

    #[test]
    fn a_saved_stamp_loads_back_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("prog");
        std::fs::write(&output, b"binary").unwrap();
        let stamp = stamp_for(&output);
        stamp.save(&dir.path().join("cache")).unwrap();
        assert_eq!(BuildStamp::load(&dir.path().join("cache")), Some(stamp));
    }

    #[test]
    fn a_missing_or_corrupt_stamp_file_loads_as_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(BuildStamp::load(dir.path()), None);
        std::fs::write(dir.path().join("stamp"), b"garbage").unwrap();
        assert_eq!(BuildStamp::load(dir.path()), None);
    }

    #[test]
    fn a_stamp_cannot_be_made_for_a_missing_output() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(BuildStamp::new(1, 2, 3, &dir.path().join("missing")), None);
    }

    #[test]
    fn the_output_is_intact_until_it_is_removed_or_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("prog");
        std::fs::write(&output, b"binary").unwrap();
        let stamp = stamp_for(&output);
        assert!(stamp.output_is_intact(&output));
        std::fs::write(&output, b"a different and longer binary").unwrap();
        assert!(!stamp.output_is_intact(&output));
        std::fs::remove_file(&output).unwrap();
        assert!(!stamp.output_is_intact(&output));
    }

    #[test]
    fn replacing_the_source_hash_keeps_the_other_fields() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("prog");
        std::fs::write(&output, b"binary").unwrap();
        let updated = stamp_for(&output).with_source_hash(99);
        assert_eq!(updated.source_hash(), 99);
        assert_eq!(updated.compiler_stamp(), 2);
        assert_eq!(updated.object_hash(), 3);
        assert!(updated.output_is_intact(&output));
    }
}

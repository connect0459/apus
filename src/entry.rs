use cranelift_codegen::binemit::Reloc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolRef {
    Func(String),
    Data(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryReloc {
    offset: u32,
    kind: Reloc,
    target: SymbolRef,
    addend: i64,
}

impl EntryReloc {
    pub fn new(offset: u32, kind: Reloc, target: SymbolRef, addend: i64) -> Self {
        Self {
            offset,
            kind,
            target,
            addend,
        }
    }

    pub fn offset(&self) -> u32 {
        self.offset
    }

    pub fn kind(&self) -> Reloc {
        self.kind
    }

    pub fn target(&self) -> &SymbolRef {
        &self.target
    }

    pub fn addend(&self) -> i64 {
        self.addend
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Literal {
    name: String,
    bytes: Vec<u8>,
    relocs: Vec<(u32, SymbolRef)>,
}

impl Literal {
    pub fn new(name: String, bytes: Vec<u8>) -> Self {
        Self {
            name,
            bytes,
            relocs: Vec::new(),
        }
    }

    pub fn with_relocs(mut self, relocs: Vec<(u32, SymbolRef)>) -> Self {
        self.relocs = relocs;
        self
    }

    pub fn relocs(&self) -> &[(u32, SymbolRef)] {
        &self.relocs
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncEntry {
    symbol: String,
    alignment: u64,
    code: Vec<u8>,
    relocs: Vec<EntryReloc>,
    literals: Vec<Literal>,
}

impl FuncEntry {
    pub fn new(
        symbol: String,
        alignment: u64,
        code: Vec<u8>,
        relocs: Vec<EntryReloc>,
        literals: Vec<Literal>,
    ) -> Self {
        Self {
            symbol,
            alignment,
            code,
            relocs,
            literals,
        }
    }

    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    pub fn alignment(&self) -> u64 {
        self.alignment
    }

    pub fn code(&self) -> &[u8] {
        &self.code
    }

    pub fn relocs(&self) -> &[EntryReloc] {
        &self.relocs
    }

    pub fn literals(&self) -> &[Literal] {
        &self.literals
    }
}

const MAGIC: [u8; 4] = *b"APFE";
const FORMAT_VERSION: u8 = 2;

const TARGET_FUNC: u8 = 0;
const TARGET_DATA: u8 = 1;

fn reloc_code(kind: Reloc) -> Option<u8> {
    match kind {
        Reloc::Abs4 => Some(0),
        Reloc::Abs8 => Some(1),
        Reloc::Arm64Call => Some(2),
        Reloc::Aarch64AdrGotPage21 => Some(3),
        Reloc::Aarch64Ld64GotLo12Nc => Some(4),
        Reloc::Aarch64AdrPrelPgHi21 => Some(5),
        Reloc::Aarch64AddAbsLo12Nc => Some(6),
        _ => None,
    }
}

fn reloc_from_code(code: u8) -> Option<Reloc> {
    match code {
        0 => Some(Reloc::Abs4),
        1 => Some(Reloc::Abs8),
        2 => Some(Reloc::Arm64Call),
        3 => Some(Reloc::Aarch64AdrGotPage21),
        4 => Some(Reloc::Aarch64Ld64GotLo12Nc),
        5 => Some(Reloc::Aarch64AdrPrelPgHi21),
        6 => Some(Reloc::Aarch64AddAbsLo12Nc),
        _ => None,
    }
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    put_u64(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if count > self.bytes.len() {
            return None;
        }
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn length(&mut self) -> Option<usize> {
        let length = usize::try_from(self.u64()?).ok()?;
        (length <= self.bytes.len()).then_some(length)
    }

    fn blob(&mut self) -> Option<Vec<u8>> {
        let length = self.length()?;
        self.take(length).map(<[u8]>::to_vec)
    }

    fn string(&mut self) -> Option<String> {
        String::from_utf8(self.blob()?).ok()
    }
}

impl FuncEntry {
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(self.code.len() + 64);
        out.extend_from_slice(&MAGIC);
        out.push(FORMAT_VERSION);
        put_bytes(&mut out, self.symbol.as_bytes());
        put_u64(&mut out, self.alignment);
        put_bytes(&mut out, &self.code);
        put_u64(&mut out, self.relocs.len() as u64);
        for reloc in &self.relocs {
            out.extend_from_slice(&reloc.offset.to_le_bytes());
            out.push(reloc_code(reloc.kind)?);
            match &reloc.target {
                SymbolRef::Func(name) => {
                    out.push(TARGET_FUNC);
                    put_bytes(&mut out, name.as_bytes());
                }
                SymbolRef::Data(name) => {
                    out.push(TARGET_DATA);
                    put_bytes(&mut out, name.as_bytes());
                }
            }
            out.extend_from_slice(&reloc.addend.to_le_bytes());
        }
        put_u64(&mut out, self.literals.len() as u64);
        for literal in &self.literals {
            put_bytes(&mut out, literal.name.as_bytes());
            put_bytes(&mut out, &literal.bytes);
            put_u64(&mut out, literal.relocs.len() as u64);
            for (offset, target) in &literal.relocs {
                out.extend_from_slice(&offset.to_le_bytes());
                match target {
                    SymbolRef::Func(name) => {
                        out.push(TARGET_FUNC);
                        put_bytes(&mut out, name.as_bytes());
                    }
                    SymbolRef::Data(name) => {
                        out.push(TARGET_DATA);
                        put_bytes(&mut out, name.as_bytes());
                    }
                }
            }
        }
        Some(out)
    }

    pub fn decode(bytes: &[u8]) -> Option<FuncEntry> {
        let mut reader = Reader { bytes };
        if reader.take(MAGIC.len())? != MAGIC || reader.u8()? != FORMAT_VERSION {
            return None;
        }
        let symbol = reader.string()?;
        let alignment = reader.u64()?;
        let code = reader.blob()?;
        let reloc_count = reader.length()?;
        let mut relocs = Vec::with_capacity(reloc_count);
        for _ in 0..reloc_count {
            let offset = reader.u32()?;
            let kind = reloc_from_code(reader.u8()?)?;
            let target = match reader.u8()? {
                TARGET_FUNC => SymbolRef::Func(reader.string()?),
                TARGET_DATA => SymbolRef::Data(reader.string()?),
                _ => return None,
            };
            let addend = reader.i64()?;
            relocs.push(EntryReloc::new(offset, kind, target, addend));
        }
        let literal_count = reader.length()?;
        let mut literals = Vec::with_capacity(literal_count);
        for _ in 0..literal_count {
            let name = reader.string()?;
            let bytes = reader.blob()?;
            let reloc_count = reader.length()?;
            let mut relocs = Vec::with_capacity(reloc_count);
            for _ in 0..reloc_count {
                let offset = reader.u32()?;
                let target = match reader.u8()? {
                    TARGET_FUNC => SymbolRef::Func(reader.string()?),
                    TARGET_DATA => SymbolRef::Data(reader.string()?),
                    _ => return None,
                };
                relocs.push((offset, target));
            }
            literals.push(Literal::new(name, bytes).with_relocs(relocs));
        }
        reader
            .bytes
            .is_empty()
            .then(|| FuncEntry::new(symbol, alignment, code, relocs, literals))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FuncEntry {
        FuncEntry::new(
            "apus_f".to_string(),
            4,
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            vec![
                EntryReloc::new(
                    0,
                    Reloc::Arm64Call,
                    SymbolRef::Func("apus_g".to_string()),
                    0,
                ),
                EntryReloc::new(
                    4,
                    Reloc::Aarch64AdrGotPage21,
                    SymbolRef::Data("apus_str_1".to_string()),
                    -8,
                ),
            ],
            vec![
                Literal::new("apus_str_1".to_string(), b"hi\0".to_vec()),
                Literal::new("apus_class_C".to_string(), vec![0; 16]).with_relocs(vec![
                    (0, SymbolRef::Func("apus_drop".to_string())),
                    (8, SymbolRef::Data("apus_class_P".to_string())),
                ]),
            ],
        )
    }

    #[test]
    fn an_encoded_entry_decodes_to_the_same_entry() {
        let bytes = sample().encode().unwrap();
        assert_eq!(FuncEntry::decode(&bytes), Some(sample()));
    }

    #[test]
    fn an_entry_without_relocations_or_literals_round_trips() {
        let entry = FuncEntry::new("apus_f".to_string(), 4, vec![9; 16], vec![], vec![]);
        assert_eq!(FuncEntry::decode(&entry.encode().unwrap()), Some(entry));
    }

    #[test]
    fn truncated_input_is_rejected_at_every_length() {
        let bytes = sample().encode().unwrap();
        for length in 0..bytes.len() {
            assert_eq!(FuncEntry::decode(&bytes[..length]), None, "length {length}");
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = sample().encode().unwrap();
        bytes.push(0);
        assert_eq!(FuncEntry::decode(&bytes), None);
    }

    #[test]
    fn input_with_another_format_version_is_rejected() {
        let mut bytes = sample().encode().unwrap();
        bytes[4] ^= 0xff;
        assert_eq!(FuncEntry::decode(&bytes), None);
        let mut bytes = sample().encode().unwrap();
        bytes[0] ^= 0xff;
        assert_eq!(FuncEntry::decode(&bytes), None);
    }

    #[test]
    fn a_length_larger_than_the_input_is_rejected_without_allocating() {
        let mut bytes = sample().encode().unwrap();
        let code_length_offset = 5 + 8 + "apus_f".len() + 8;
        bytes[code_length_offset..code_length_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(FuncEntry::decode(&bytes), None);
    }

    #[test]
    fn a_relocation_kind_of_another_architecture_cannot_be_encoded() {
        let entry = FuncEntry::new(
            "apus_f".to_string(),
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
        assert_eq!(entry.encode(), None);
    }
}

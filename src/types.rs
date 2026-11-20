#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructId(usize);

impl StructId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnumId(usize);

impl EnumId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Int,
    Bool,
    Str,
    Void,
    Struct(StructId),
    Enum(EnumId),
}

impl Ty {
    pub fn from_builtin_name(name: &str) -> Option<Ty> {
        match name {
            "Int" => Some(Ty::Int),
            "Bool" => Some(Ty::Bool),
            "String" => Some(Ty::Str),
            _ => None,
        }
    }

    pub fn is_struct(self) -> bool {
        matches!(self, Ty::Struct(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(usize);

impl StructId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(usize);

impl EnumId {
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Bool,
    Str,
    Char,
    Void,
    Struct(StructId),
    Class(StructId),
    Enum(EnumId),
    Param(usize),
    Assoc(usize),
}

impl Ty {
    pub fn assoc(index: usize, id: usize) -> Ty {
        Ty::Assoc((index << 32) | (id & 0xffff_ffff))
    }

    pub fn split_assoc(packed: usize) -> (usize, usize) {
        (packed >> 32, packed & 0xffff_ffff)
    }

    pub fn from_builtin_name(name: &str) -> Option<Ty> {
        match name {
            "Int" => Some(Ty::Int),
            "Bool" => Some(Ty::Bool),
            "String" => Some(Ty::Str),
            "Character" => Some(Ty::Char),
            _ => None,
        }
    }

    pub fn is_param_like(self) -> bool {
        matches!(self, Ty::Param(_) | Ty::Assoc(_))
    }

    pub fn is_struct(self) -> bool {
        matches!(self, Ty::Struct(_))
    }

    pub fn is_class(self) -> bool {
        matches!(self, Ty::Class(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_handle_stays_two_words_so_that_deep_expressions_fit_the_default_stack() {
        assert!(std::mem::size_of::<Ty>() <= 16);
    }

    #[test]
    fn an_associated_type_keeps_its_parameter_and_its_identifier() {
        let Ty::Assoc(packed) = Ty::assoc(3, 9) else {
            panic!("expected an associated type");
        };
        assert_eq!(Ty::split_assoc(packed), (3, 9));
    }
}

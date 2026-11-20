use crate::hash::Fnv1a;
use crate::lexer::{Spanned, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Func,
    Struct,
    Enum,
}

#[derive(Debug, Clone)]
pub struct Item {
    kind: ItemKind,
    name: String,
    tokens: Vec<Spanned>,
    hash: u64,
    signature_hash: u64,
    generic: bool,
}

impl Item {
    pub fn is_generic(&self) -> bool {
        self.generic
    }

    pub fn kind(&self) -> ItemKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tokens(&self) -> &[Spanned] {
        &self.tokens
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn signature_hash(&self) -> u64 {
        self.signature_hash
    }
}

pub fn signature_digest(items: &[Item]) -> u64 {
    let mut hasher = Fnv1a::new();
    for item in items {
        hasher.write(&item.signature_hash().to_le_bytes());
    }
    hasher.finish()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplitError {
    line: u32,
    message: String,
}

impl SplitError {
    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn split_items(tokens: &[Spanned]) -> Result<Vec<Item>, SplitError> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let kind = match tokens[i].token() {
            Token::Ident(k) if k == "func" => ItemKind::Func,
            Token::Ident(k) if k == "struct" => ItemKind::Struct,
            Token::Ident(k) if k == "enum" => ItemKind::Enum,
            _ => {
                return Err(SplitError {
                    line: tokens[i].line(),
                    message: "expected 'func', 'struct' or 'enum' at top level".to_string(),
                });
            }
        };
        let start = i;
        let name = match tokens.get(i + 1).map(|t| t.token()) {
            Some(Token::Ident(n)) => n.clone(),
            _ => {
                return Err(SplitError {
                    line: tokens[i].line(),
                    message: "expected a name".to_string(),
                });
            }
        };
        let mut depth = 0usize;
        let mut opened = false;
        let mut end = None;
        for (j, t) in tokens.iter().enumerate().skip(i + 2) {
            match t.token() {
                Token::Punct(p) if p == "{" => {
                    depth += 1;
                    opened = true;
                }
                Token::Punct(p) if p == "}" => {
                    if depth == 0 {
                        return Err(SplitError {
                            line: t.line(),
                            message: "unbalanced '}'".to_string(),
                        });
                    }
                    depth -= 1;
                    if depth == 0 {
                        end = Some(j + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = match (end, opened) {
            (Some(e), _) => e,
            _ => {
                return Err(SplitError {
                    line: tokens[start].line(),
                    message: "unbalanced '{'".to_string(),
                });
            }
        };
        let slice = tokens[start..end].to_vec();
        let hash = hash_tokens(&slice);
        let generic = kind == ItemKind::Func
            && matches!(slice.get(2).map(|t| t.token()), Some(Token::Punct(p)) if p == "<");
        let signature_hash = match kind {
            ItemKind::Struct | ItemKind::Enum => hash,
            ItemKind::Func if generic => hash,
            ItemKind::Func => {
                let header_end = slice
                    .iter()
                    .position(|t| matches!(t.token(), Token::Punct(p) if p == "{"))
                    .unwrap_or(slice.len());
                hash_tokens(&slice[..header_end])
            }
        };
        items.push(Item {
            kind,
            name,
            tokens: slice,
            hash,
            signature_hash,
            generic,
        });
        i = end;
    }
    Ok(items)
}

pub fn hash_tokens(tokens: &[Spanned]) -> u64 {
    let mut hasher = Fnv1a::new();
    for t in tokens {
        match t.token() {
            Token::Ident(s) => {
                hasher.write(&[1]);
                hasher.write(s.as_bytes());
            }
            Token::Int(v) => {
                hasher.write(&[2]);
                hasher.write(&v.to_le_bytes());
            }
            Token::Str(s) => {
                hasher.write(&[3]);
                hasher.write(s.as_bytes());
            }
            Token::Punct(s) => {
                hasher.write(&[4]);
                hasher.write(s.as_bytes());
            }
        }
        hasher.write(&[0xff]);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn items(src: &str) -> Vec<Item> {
        split_items(&lex(src).unwrap()).unwrap()
    }

    fn hash_of(src: &str, name: &str) -> u64 {
        items(src)
            .into_iter()
            .find(|i| i.name() == name)
            .unwrap()
            .hash
    }

    #[test]
    fn top_level_funcs_and_structs_become_items_in_source_order() {
        let found = items("struct P { var x: Int }\nfunc f() { }\nfunc g() -> Int { return 1 }");
        let summary: Vec<(ItemKind, &str)> = found.iter().map(|i| (i.kind(), i.name())).collect();
        assert_eq!(
            summary,
            vec![
                (ItemKind::Struct, "P"),
                (ItemKind::Func, "f"),
                (ItemKind::Func, "g"),
            ]
        );
    }

    #[test]
    fn an_enum_is_an_item_of_its_own_kind() {
        let found = items("enum Color { case red, green }\nfunc f() { }");
        assert_eq!(found[0].kind(), ItemKind::Enum);
        assert_eq!(found[0].name(), "Color");
    }

    #[test]
    fn changing_an_enum_case_changes_the_signature_digest() {
        assert_ne!(
            digest_of("enum Color { case red }"),
            digest_of("enum Color { case blue }")
        );
    }

    #[test]
    fn nested_braces_stay_inside_one_item() {
        let found = items("func f() { if true { while false { } } }\nfunc g() { }");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn whitespace_comments_and_line_shifts_do_not_change_the_hash() {
        let before = hash_of("func f() { return 1 }", "f");
        let after = hash_of("\n\n// note\nfunc  f( ) {\n  return 1 /* c */\n}", "f");
        assert_eq!(before, after);
    }

    #[test]
    fn editing_a_body_changes_only_that_items_hash() {
        let before = "func f() { return 1 }\nfunc g() { return 2 }";
        let after = "func f() { return 1 }\nfunc g() { return 3 }";
        assert_eq!(hash_of(before, "f"), hash_of(after, "f"));
        assert_ne!(hash_of(before, "g"), hash_of(after, "g"));
    }

    #[test]
    fn items_with_different_kinds_and_same_text_hash_differently() {
        assert_ne!(hash_of("func a() { }", "a"), hash_of("struct a { }", "a"));
    }

    #[test]
    fn unbalanced_braces_are_an_error() {
        assert!(split_items(&lex("func f() { ").unwrap()).is_err());
    }

    #[test]
    fn a_stray_top_level_token_is_an_error_with_its_line() {
        let err = split_items(&lex("func f() { }\n\nx = 1").unwrap()).unwrap_err();
        assert_eq!(err.line(), 3);
    }

    #[test]
    fn a_func_without_a_name_is_an_error() {
        assert!(split_items(&lex("func { }").unwrap()).is_err());
    }

    fn digest_of(src: &str) -> u64 {
        signature_digest(&items(src))
    }

    #[test]
    fn editing_a_function_body_keeps_the_signature_digest() {
        assert_eq!(
            digest_of("func f() -> Int { return 1 }\nstruct P { var x: Int }"),
            digest_of("func f() -> Int { return 2 }\nstruct P { var x: Int }")
        );
    }

    #[test]
    fn changing_a_function_header_changes_the_signature_digest() {
        assert_ne!(
            digest_of("func f() -> Int { return 1 }"),
            digest_of("func f() -> Bool { return true }")
        );
    }

    #[test]
    fn changing_a_struct_body_changes_the_signature_digest() {
        assert_ne!(
            digest_of("struct P { var x: Int }"),
            digest_of("struct P { var y: Int }")
        );
    }

    #[test]
    fn adding_a_function_changes_the_signature_digest() {
        assert_ne!(
            digest_of("func f() { }"),
            digest_of("func f() { }\nfunc g() { }")
        );
    }

    #[test]
    fn a_function_with_type_parameters_is_generic() {
        let found = items("func id<T>(_ x: T) -> T { return x }\nfunc f() { }");
        assert!(found[0].is_generic());
        assert!(!found[1].is_generic());
    }

    #[test]
    fn editing_a_generic_function_body_changes_the_signature_digest() {
        assert_ne!(
            digest_of("func id<T>(_ x: T) -> T { return x }"),
            digest_of("func id<T>(_ x: T) -> T { let y = x\nreturn y }")
        );
    }
}

use crate::hash::Fnv1a;
use crate::lexer::{Spanned, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Func,
    Struct,
    Enum,
    Protocol,
    Method,
}

#[derive(Debug, Clone)]
pub struct Item {
    kind: ItemKind,
    name: String,
    tokens: Vec<Spanned>,
    hash: u64,
    signature_hash: u64,
    generic: bool,
    owner: Option<String>,
}

impl Item {
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

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
            Token::Ident(k) if k == "struct" || k == "class" => ItemKind::Struct,
            Token::Ident(k) if k == "enum" => ItemKind::Enum,
            Token::Ident(k) if k == "protocol" => ItemKind::Protocol,
            _ => {
                return Err(SplitError {
                    line: tokens[i].line(),
                    message:
                        "expected 'func', 'struct', 'class', 'enum' or 'protocol' at top level"
                            .to_string(),
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
        let mut slice = tokens[start..end].to_vec();
        let owner_generic =
            matches!(slice.get(2).map(|t| t.token()), Some(Token::Punct(p)) if p == "<");
        let members = match kind {
            ItemKind::Func | ItemKind::Method => Vec::new(),
            ItemKind::Struct | ItemKind::Enum => extract_members(&mut slice)?,
            ItemKind::Protocol => Vec::new(),
        };
        let hash = hash_tokens(&slice);
        let generic = kind == ItemKind::Func
            && matches!(slice.get(2).map(|t| t.token()), Some(Token::Punct(p)) if p == "<");
        let signature_hash = match kind {
            ItemKind::Struct | ItemKind::Enum | ItemKind::Protocol => hash,
            ItemKind::Func if generic => hash,
            ItemKind::Method => hash,
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
            name: name.clone(),
            tokens: slice,
            hash,
            signature_hash,
            generic,
            owner: None,
        });
        for member in members {
            items.push(member_item(&name, member, owner_generic)?);
        }
        i = end;
    }
    Ok(items)
}

fn is_member_start(token: &Token) -> bool {
    matches!(token, Token::Ident(k) if k == "func" || k == "init" || k == "deinit" || k == "mutating" || k == "override")
}

fn extract_members(slice: &mut Vec<Spanned>) -> Result<Vec<Vec<Spanned>>, SplitError> {
    let mut kept = Vec::with_capacity(slice.len());
    let mut members = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < slice.len() {
        let token = slice[i].token();
        if depth == 1 && is_member_start(token) {
            let open = slice[i..]
                .iter()
                .position(|t| matches!(t.token(), Token::Punct(p) if p == "{"))
                .map(|offset| i + offset);
            let Some(open) = open else {
                return Err(SplitError {
                    line: slice[i].line(),
                    message: "expected a body for this member".to_string(),
                });
            };
            let mut inner = 0usize;
            let mut close = None;
            for (j, t) in slice.iter().enumerate().skip(open) {
                match t.token() {
                    Token::Punct(p) if p == "{" => inner += 1,
                    Token::Punct(p) if p == "}" => {
                        inner -= 1;
                        if inner == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close) = close else {
                return Err(SplitError {
                    line: slice[i].line(),
                    message: "unbalanced '{'".to_string(),
                });
            };
            members.push(slice[i..=close].to_vec());
            i = close + 1;
            continue;
        }
        match token {
            Token::Punct(p) if p == "{" => depth += 1,
            Token::Punct(p) if p == "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        kept.push(slice[i].clone());
        i += 1;
    }
    *slice = kept;
    Ok(members)
}

fn modifier_count(member: &[Spanned]) -> usize {
    member
        .iter()
        .take_while(|t| matches!(t.token(), Token::Ident(k) if k == "mutating" || k == "override"))
        .count()
}

fn has_type_parameters(member: &[Spanned]) -> bool {
    let at = modifier_count(member);
    if matches!(member.get(at).map(Spanned::token), Some(Token::Ident(k)) if k == "deinit") {
        return false;
    }
    let name_at = at + 1;
    matches!(member.get(name_at + 1).map(Spanned::token), Some(Token::Punct(p)) if p == "<")
}

fn member_name(tokens: &[Spanned]) -> Result<String, SplitError> {
    let at = modifier_count(tokens);
    match tokens.get(at).map(Spanned::token) {
        Some(Token::Ident(k)) if k == "init" => Ok("init".to_string()),
        Some(Token::Ident(k)) if k == "deinit" => Ok("deinit".to_string()),
        Some(Token::Ident(k)) if k == "func" => match tokens.get(at + 1).map(Spanned::token) {
            Some(Token::Ident(name)) => Ok(name.clone()),
            _ => Err(SplitError {
                line: tokens[at].line(),
                message: "expected a name".to_string(),
            }),
        },
        _ => Err(SplitError {
            line: tokens[at.min(tokens.len() - 1)].line(),
            message: "expected 'func' after a modifier".to_string(),
        }),
    }
}

fn member_item(owner: &str, tokens: Vec<Spanned>, generic: bool) -> Result<Item, SplitError> {
    let name = format!("{owner}.{}", member_name(&tokens)?);
    let mut hasher = Fnv1a::new();
    hasher.write(owner.as_bytes());
    hasher.write(&hash_tokens(&tokens).to_le_bytes());
    let header_end = tokens
        .iter()
        .position(|t| matches!(t.token(), Token::Punct(p) if p == "{"))
        .unwrap_or(tokens.len());
    let generic = generic || has_type_parameters(&tokens);
    let signed = if generic { tokens.len() } else { header_end };
    let mut signature = Fnv1a::new();
    signature.write(owner.as_bytes());
    signature.write(&hash_tokens(&tokens[..signed]).to_le_bytes());
    Ok(Item {
        kind: ItemKind::Method,
        name,
        hash: hasher.finish(),
        signature_hash: signature.finish(),
        generic,
        owner: Some(owner.to_string()),
        tokens,
    })
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
    fn a_class_is_an_item_named_after_the_class() {
        let found = items("class C { var x: Int }\nfunc f() { }");
        assert_eq!(found[0].kind(), ItemKind::Struct);
        assert_eq!(found[0].name(), "C");
    }

    #[test]
    fn an_enum_is_an_item_of_its_own_kind() {
        let found = items("enum Color { case red, green }\nfunc f() { }");
        assert_eq!(found[0].kind(), ItemKind::Enum);
        assert_eq!(found[0].name(), "Color");
    }

    #[test]
    fn a_protocol_is_an_item_whose_requirements_belong_to_the_signature_digest() {
        let found = items("protocol P {\n  func f() -> Int\n}\nfunc g() { }");
        assert_eq!(found[0].kind(), ItemKind::Protocol);
        assert_eq!(found[0].name(), "P");
        assert_eq!(found.len(), 2);
        assert_ne!(
            signature_digest(&items("protocol P {\n  func f() -> Int\n}")),
            signature_digest(&items("protocol P {\n  func f() -> Bool\n}"))
        );
    }

    #[test]
    fn a_protocol_requirement_is_not_split_into_a_method_item() {
        let found = items("protocol P {\n  func f()\n  mutating func g()\n}");
        assert_eq!(found.len(), 1);
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

    const COUNTER: &str = "class Counter {\n  var n: Int\n  func bump() { self.n += 1 }\n  mutating func reset() { }\n  init(n: Int) { self.n = n }\n}\nfunc main() { }";

    #[test]
    fn the_members_of_a_type_become_items_of_their_own() {
        let found = items(COUNTER);
        let summary: Vec<(ItemKind, &str)> = found.iter().map(|i| (i.kind(), i.name())).collect();
        assert_eq!(
            summary,
            vec![
                (ItemKind::Struct, "Counter"),
                (ItemKind::Method, "Counter.bump"),
                (ItemKind::Method, "Counter.reset"),
                (ItemKind::Method, "Counter.init"),
                (ItemKind::Func, "main"),
            ]
        );
        assert_eq!(found[1].owner(), Some("Counter"));
        assert_eq!(found[4].owner(), None);
    }

    #[test]
    fn a_member_item_keeps_its_modifier_and_its_whole_body() {
        let found = items(COUNTER);
        let reset = &found[2];
        assert_eq!(
            reset.tokens().first().map(|t| t.token().clone()),
            Some(Token::Ident("mutating".to_string()))
        );
        assert_eq!(
            reset.tokens().last().map(|t| t.token().clone()),
            Some(Token::Punct("}".to_string()))
        );
    }

    #[test]
    fn editing_a_method_body_changes_neither_the_type_item_nor_the_digest() {
        let before = "struct P { var x: Int\n func f() -> Int { return 1 } }";
        let after = "struct P { var x: Int\n func f() -> Int { return 2 } }";
        assert_eq!(hash_of(before, "P"), hash_of(after, "P"));
        assert_eq!(digest_of(before), digest_of(after));
        assert_ne!(hash_of(before, "P.f"), hash_of(after, "P.f"));
    }

    #[test]
    fn changing_a_method_header_changes_the_digest() {
        assert_ne!(
            digest_of("struct P { var x: Int\n func f() -> Int { return 1 } }"),
            digest_of("struct P { var x: Int\n func f() -> Bool { return true } }")
        );
    }

    #[test]
    fn a_deinit_is_split_out_as_a_member_of_its_class() {
        let found = items("class A {\n  var x: Int\n  deinit { print(1) }\n}\nfunc main() { }");
        assert!(
            found.iter().any(|item| item.name() == "A.deinit"),
            "{found:?}"
        );
        assert!(!found.iter().any(|item| item.is_generic()));
    }

    #[test]
    fn an_overriding_method_is_split_out_with_its_name() {
        let found =
            items("class B: A {\n  var y: Int\n  override func f() { }\n}\nfunc main() { }");
        assert!(found.iter().any(|item| item.name() == "B.f"), "{found:?}");
    }

    #[test]
    fn marking_a_method_mutating_changes_the_digest() {
        assert_ne!(
            digest_of("struct P { var x: Int\n func f() { } }"),
            digest_of("struct P { var x: Int\n mutating func f() { } }")
        );
    }

    #[test]
    fn identical_method_text_in_different_types_hashes_differently() {
        let src = "struct A { var x: Int\n func f() { } }\nstruct B { var x: Int\n func f() { } }";
        assert_ne!(hash_of(src, "A.f"), hash_of(src, "B.f"));
    }

    #[test]
    fn an_enum_may_have_methods() {
        let found = items("enum E { case a, b\n func f() { } }");
        assert_eq!(found[0].kind(), ItemKind::Enum);
        assert_eq!(found[1].name(), "E.f");
    }

    #[test]
    fn a_method_without_a_name_is_an_error() {
        assert!(split_items(&lex("struct P { func { } }").unwrap()).is_err());
    }

    #[test]
    fn a_member_without_a_body_is_an_error() {
        assert!(split_items(&lex("struct P { func f() }").unwrap()).is_err());
    }

    #[test]
    fn mutating_without_func_is_an_error() {
        assert!(split_items(&lex("struct P { mutating x() { } }").unwrap()).is_err());
    }

    #[test]
    fn a_member_body_that_never_closes_is_an_error() {
        assert!(split_items(&lex("struct P { func f() { ").unwrap()).is_err());
    }

    #[test]
    fn the_members_of_a_generic_type_are_generic_items() {
        let found = items(
            "struct Box<T> { var v: T\n func get() -> T { return self.v } }\nstruct P { var x: Int\n func f() { } }",
        );
        assert!(found[1].is_generic());
        assert!(!found[3].is_generic());
    }

    #[test]
    fn editing_a_method_body_of_a_generic_type_changes_the_signature_digest() {
        assert_ne!(
            digest_of("struct Box<T> { var v: T\n func get() -> T { return self.v } }"),
            digest_of("struct Box<T> { var v: T\n func get() -> T { let x = self.v\n return x } }")
        );
    }

    #[test]
    fn a_method_with_type_parameters_is_a_generic_item_even_in_a_plain_type() {
        let found = items(
            "struct P { var x: Int\n func id<T>(_ v: T) -> T { return v }\n mutating func f() { }\n mutating func g<T>(_ v: T) { } }",
        );
        assert!(found[1].is_generic());
        assert!(!found[2].is_generic());
        assert!(found[3].is_generic());
    }

    #[test]
    fn editing_a_generic_method_body_of_a_plain_type_changes_the_signature_digest() {
        assert_ne!(
            digest_of("struct P { var x: Int\n func id<T>(_ v: T) -> T { return v } }"),
            digest_of("struct P { var x: Int\n func id<T>(_ v: T) -> T { let w = v\n return w } }")
        );
    }
}

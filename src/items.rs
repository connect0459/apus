use crate::hash::Fnv1a;
use crate::lexer::{Piece, Spanned, Token};

pub const TOP_LEVEL: &str = "$top-level";

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
    top_level: bool,
}

impl Item {
    pub fn is_top_level(&self) -> bool {
        self.top_level
    }

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

const ACCESS_MODIFIERS: [&str; 9] = [
    "public",
    "private",
    "internal",
    "fileprivate",
    "open",
    "package",
    "final",
    "convenience",
    "required",
];

const IGNORED_ATTRIBUTES: [&str; 13] = [
    "inline",
    "inlinable",
    "usableFromInline",
    "discardableResult",
    "escaping",
    "frozen",
    "_eagerMove",
    "_optimize",
    "_transparent",
    "_alwaysEmitIntoClient",
    "_semantics",
    "_effects",
    "_noAllocation",
];

const ATTRIBUTES_WITH_ARGUMENTS: [&str; 4] = ["inline", "_optimize", "_semantics", "_effects"];

const DECLARATION_WORDS: [&str; 20] = [
    "func",
    "var",
    "let",
    "struct",
    "class",
    "enum",
    "protocol",
    "init",
    "deinit",
    "typealias",
    "extension",
    "mutating",
    "override",
    "static",
    "subscript",
    "lazy",
    "weak",
    "unowned",
    "indirect",
    "associatedtype",
];

fn is_word(token: &Token, words: &[&str]) -> bool {
    matches!(token, Token::Ident(w) if words.contains(&w.as_str()))
}

fn starts_declaration(tokens: &[Spanned], at: usize) -> bool {
    at == 0
        || tokens[at - 1].line() != tokens[at].line()
        || matches!(tokens[at - 1].token(), Token::Punct(p) if p == "{" || p == "}" || p == ";")
}

fn skip_access_modifiers(tokens: &[Spanned], mut at: usize) -> usize {
    while at < tokens.len() && is_word(tokens[at].token(), &ACCESS_MODIFIERS) {
        at += 1;
        let setter = [
            Token::Punct("(".into()),
            Token::Ident("set".into()),
            Token::Punct(")".into()),
        ];
        if tokens.len() >= at + 3
            && tokens[at..at + 3]
                .iter()
                .zip(&setter)
                .all(|(t, expected)| t.token() == expected)
        {
            at += 3;
        }
    }
    at
}

fn skip_attribute(tokens: &[Spanned], at: usize) -> Option<usize> {
    let Token::Ident(word) = tokens[at].token() else {
        return None;
    };
    let name = word.strip_prefix('@')?;
    if !IGNORED_ATTRIBUTES.contains(&name) {
        return None;
    }
    let mut end = at + 1;
    let same_line = |end: usize| {
        tokens
            .get(end)
            .is_some_and(|t| t.line() == tokens[at].line())
    };
    if ATTRIBUTES_WITH_ARGUMENTS.contains(&name)
        && same_line(end)
        && matches!(tokens[end].token(), Token::Punct(p) if p == "(")
    {
        let mut depth = 0usize;
        while end < tokens.len() {
            match tokens[end].token() {
                Token::Punct(p) if p == "(" => depth += 1,
                Token::Punct(p) if p == ")" => {
                    depth -= 1;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
    }
    Some(end)
}

fn without_modifiers(tokens: &[Spanned]) -> Vec<Spanned> {
    let mut kept = Vec::with_capacity(tokens.len());
    let mut at = 0;
    let mut after_attribute = false;
    while at < tokens.len() {
        if let Some(end) = skip_attribute(tokens, at) {
            at = end;
            after_attribute = true;
            continue;
        }
        let starts = after_attribute || starts_declaration(tokens, at);
        after_attribute = false;
        if starts && is_word(tokens[at].token(), &ACCESS_MODIFIERS) {
            let mut after = skip_access_modifiers(tokens, at);
            while let Some(end) = skip_attribute(tokens, after) {
                after = end;
            }
            if tokens
                .get(after)
                .is_some_and(|t| is_word(t.token(), &DECLARATION_WORDS))
            {
                at = after;
                continue;
            }
        }
        kept.push(tokens[at].clone());
        at += 1;
    }
    kept
}

pub fn split_items(tokens: &[Spanned]) -> Result<Vec<Item>, SplitError> {
    let tokens = &without_modifiers(tokens)[..];
    let mut items = Vec::new();
    let mut top_level: Vec<Spanned> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let kind = match tokens[i].token() {
            Token::Ident(k) if k == "func" => ItemKind::Func,
            Token::Ident(k) if k == "struct" || k == "class" => ItemKind::Struct,
            Token::Ident(k) if k == "enum" => ItemKind::Enum,
            Token::Ident(k) if k == "protocol" => ItemKind::Protocol,
            _ => {
                i = take_statements(tokens, i, &mut top_level)?;
                continue;
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
            top_level: false,
        });
        for member in members {
            items.push(member_item(&name, member, owner_generic)?);
        }
        i = end;
    }
    if !top_level.is_empty() {
        items.push(top_level_item(top_level));
    }
    Ok(items)
}

fn is_declaration_start(token: &Token) -> bool {
    matches!(token, Token::Ident(k) if matches!(k.as_str(), "func" | "struct" | "class" | "enum" | "protocol"))
}

fn take_statements(
    tokens: &[Spanned],
    start: usize,
    out: &mut Vec<Spanned>,
) -> Result<usize, SplitError> {
    let mut depth = 0usize;
    let mut i = start;
    while i < tokens.len() {
        let token = &tokens[i];
        let first_on_line = i == 0 || tokens[i - 1].line() != token.line();
        if depth == 0 && i > start && first_on_line && is_declaration_start(token.token()) {
            break;
        }
        if depth == 0
            && first_on_line
            && let Token::Ident(word) = token.token()
            && matches!(word.as_str(), "mutating" | "override" | "init" | "deinit")
        {
            return Err(SplitError {
                line: token.line(),
                message: format!("'{word}' belongs inside a type, not at the top level"),
            });
        }
        if depth == 0
            && first_on_line
            && matches!(token.token(), Token::Ident(k) if k == "import")
            && matches!(tokens.get(i + 1).map(Spanned::token), Some(Token::Ident(_)))
        {
            i += 2;
            continue;
        }
        match token.token() {
            Token::Punct(p) if p == "{" || p == "(" || p == "[" => depth += 1,
            Token::Punct(p) if p == "}" || p == ")" || p == "]" => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        out.push(token.clone());
        i += 1;
    }
    Ok(i)
}

fn top_level_item(statements: Vec<Spanned>) -> Item {
    let line = statements.first().map_or(1, Spanned::line);
    let last = statements.last().map_or(line, Spanned::line);
    let punct = |text: &str, line: u32| Spanned::new(Token::Punct(text.to_string()), line);
    let mut tokens = vec![
        Spanned::new(Token::Ident("func".to_string()), line),
        Spanned::new(Token::Ident(TOP_LEVEL.to_string()), line),
        punct("(", line),
        punct(")", line),
        punct("{", line),
    ];
    tokens.extend(statements);
    tokens.push(punct("}", last + 1));
    let hash = hash_tokens(&tokens);
    Item {
        kind: ItemKind::Func,
        name: TOP_LEVEL.to_string(),
        tokens,
        hash,
        signature_hash: hash,
        generic: false,
        owner: None,
        top_level: true,
    }
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
        top_level: false,
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
            Token::Interp(pieces) => {
                hasher.write(&[5]);
                for piece in pieces {
                    match piece {
                        Piece::Text(text) => {
                            hasher.write(&[1]);
                            hasher.write(text.as_bytes());
                        }
                        Piece::Code(code) => {
                            hasher.write(&[2]);
                            hasher.write(&hash_tokens(code).to_le_bytes());
                        }
                    }
                    hasher.write(&[0xfe]);
                }
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
    fn a_top_level_statement_keeps_the_line_it_was_written_on() {
        let found = items("func f() { }\n\nx = 1");
        let main = found.iter().find(|i| i.is_top_level()).unwrap();
        let x = main
            .tokens()
            .iter()
            .find(|t| matches!(t.token(), Token::Ident(w) if w == "x"))
            .unwrap();
        assert_eq!(x.line(), 3);
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

    #[test]
    fn top_level_statements_become_one_main_item_after_the_declarations() {
        let found = items("print(1)\nfunc f() { }\nlet a = 2\nstruct P { var x: Int }\nprint(a)");
        let summary: Vec<(ItemKind, &str, bool)> = found
            .iter()
            .map(|i| (i.kind(), i.name(), i.is_top_level()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (ItemKind::Func, "f", false),
                (ItemKind::Struct, "P", false),
                (ItemKind::Func, TOP_LEVEL, true),
            ]
        );
    }

    #[test]
    fn the_statements_keep_their_order_and_blocks_stay_together() {
        let found = items("let a = 1\nif a > 0 {\n  func inner() { }\n}\nprint(a)");
        let main = found.iter().find(|i| i.is_top_level()).unwrap();
        let words: Vec<String> = main
            .tokens()
            .iter()
            .filter_map(|t| match t.token() {
                Token::Ident(w) => Some(w.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            words,
            [
                "func", TOP_LEVEL, "let", "a", "if", "a", "func", "inner", "print", "a"
            ]
        );
    }

    #[test]
    fn an_import_line_is_dropped() {
        let found = items("import Swift\nprint(1)");
        let main = found.iter().find(|i| i.is_top_level()).unwrap();
        assert!(
            !main
                .tokens()
                .iter()
                .any(|t| matches!(t.token(), Token::Ident(w) if w == "import" || w == "Swift"))
        );
    }

    #[test]
    fn a_program_without_statements_has_no_top_level_item() {
        assert!(items("func main() { }").iter().all(|i| !i.is_top_level()));
    }

    #[test]
    fn a_main_function_may_be_declared_next_to_top_level_statements() {
        let found = items("func main() { }\nprint(1)");
        let names: Vec<&str> = found.iter().map(|i| i.name()).collect();
        assert_eq!(names, ["main", TOP_LEVEL]);
    }

    #[test]
    fn editing_a_top_level_statement_changes_the_main_item_and_the_digest() {
        let one = items("func f() { }\nprint(1)");
        let two = items("func f() { }\nprint(2)");
        assert_ne!(signature_digest(&one), signature_digest(&two));
        assert_eq!(one[0].hash(), two[0].hash());
    }

    #[test]
    fn access_modifiers_and_ignorable_attributes_before_a_declaration_are_dropped() {
        let plain = items("struct S { var x: Int\n func f() { } }\nfunc g() { }");
        let noisy = items(
            "@inline(never)\npublic struct S {\n  private(set) var x: Int\n  public final func f() { }\n}\n@_eagerMove internal func g() { }",
        );
        let hashes = |found: &[Item]| found.iter().map(Item::hash).collect::<Vec<_>>();
        assert_eq!(hashes(&plain), hashes(&noisy));
    }

    #[test]
    fn a_modifier_word_used_as_a_name_is_kept() {
        let found = items("var open = 1\nopen = 2");
        let main = found.iter().find(|i| i.is_top_level()).unwrap();
        let words = main
            .tokens()
            .iter()
            .filter(|t| matches!(t.token(), Token::Ident(w) if w == "open"))
            .count();
        assert_eq!(words, 2);
    }

    #[test]
    fn an_attribute_with_a_meaning_is_not_dropped() {
        let found = items("func f(_ g: @autoclosure () -> Int) { }");
        assert!(
            found[0]
                .tokens()
                .iter()
                .any(|t| matches!(t.token(), Token::Ident(w) if w == "@autoclosure"))
        );
    }
}

use crate::lexer::{Spanned, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Func,
    Struct,
}

#[derive(Debug, Clone)]
pub struct Item {
    kind: ItemKind,
    name: String,
    tokens: Vec<Spanned>,
    hash: u64,
}

impl Item {
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
            _ => {
                return Err(SplitError {
                    line: tokens[i].line(),
                    message: "expected 'func' or 'struct' at top level".to_string(),
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
        items.push(Item {
            kind,
            name,
            tokens: slice,
            hash,
        });
        i = end;
    }
    Ok(items)
}

pub fn hash_tokens(tokens: &[Spanned]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for t in tokens {
        match t.token() {
            Token::Ident(s) => {
                feed(&[1]);
                feed(s.as_bytes());
            }
            Token::Int(v) => {
                feed(&[2]);
                feed(&v.to_le_bytes());
            }
            Token::Str(s) => {
                feed(&[3]);
                feed(s.as_bytes());
            }
            Token::Punct(s) => {
                feed(&[4]);
                feed(s.as_bytes());
            }
        }
        feed(&[0xff]);
    }
    h
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
}

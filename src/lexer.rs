#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Str(String),
    Punct(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub token: Token,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub line: u32,
    pub message: String,
}

const THREE_CHAR_PUNCTS: [&str; 2] = ["..<", "..."];
const TWO_CHAR_PUNCTS: [&str; 10] = ["->", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "??"];

pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start_line = line;
            let mut depth = 0;
            loop {
                if i >= chars.len() {
                    return Err(LexError {
                        line: start_line,
                        message: "unterminated block comment".to_string(),
                    });
                }
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"' {
            let start_line = line;
            i += 1;
            let mut text = String::new();
            loop {
                match chars.get(i) {
                    None | Some('\n') => {
                        return Err(LexError {
                            line: start_line,
                            message: "unterminated string literal".to_string(),
                        });
                    }
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some('\\') => {
                        let escaped = match chars.get(i + 1) {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('\\') => '\\',
                            Some('"') => '"',
                            Some('0') => '\0',
                            _ => {
                                return Err(LexError {
                                    line,
                                    message: "invalid escape sequence".to_string(),
                                });
                            }
                        };
                        text.push(escaped);
                        i += 2;
                    }
                    Some(&ch) => {
                        text.push(ch);
                        i += 1;
                    }
                }
            }
            out.push(Spanned {
                token: Token::Str(text),
                line: start_line,
            });
        } else if c.is_ascii_digit() {
            let mut value: i64 = 0;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                if let Some(d) = chars[i].to_digit(10) {
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(d as i64))
                        .ok_or(LexError {
                            line,
                            message: "integer literal overflows".to_string(),
                        })?;
                }
                i += 1;
            }
            out.push(Spanned {
                token: Token::Int(value),
                line,
            });
        } else if c.is_alphabetic() || c == '_' || c == '@' || c == '#' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Spanned {
                token: Token::Ident(text),
                line,
            });
        } else {
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let matched = THREE_CHAR_PUNCTS
                .iter()
                .chain(TWO_CHAR_PUNCTS.iter())
                .find(|p| rest.starts_with(**p));
            let text = match matched {
                Some(p) => p.to_string(),
                None => c.to_string(),
            };
            i += text.chars().count();
            out.push(Spanned {
                token: Token::Punct(text),
                line,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<Token> {
        lex(src).unwrap().into_iter().map(|s| s.token).collect()
    }

    fn ident(s: &str) -> Token {
        Token::Ident(s.to_string())
    }

    fn punct(s: &str) -> Token {
        Token::Punct(s.to_string())
    }

    #[test]
    fn identifiers_and_integers_are_separated_by_whitespace() {
        assert_eq!(
            tokens("let x1 = 42"),
            vec![ident("let"), ident("x1"), punct("="), Token::Int(42)]
        );
    }

    #[test]
    fn line_comments_and_block_comments_produce_no_tokens() {
        assert_eq!(
            tokens("a // c\n/* b /* nested */ */ c"),
            vec![ident("a"), ident("c")]
        );
    }

    #[test]
    fn multi_character_operators_are_single_tokens() {
        assert_eq!(
            tokens("-> ..< ... == != <= >= && || += -="),
            vec![
                punct("->"),
                punct("..<"),
                punct("..."),
                punct("=="),
                punct("!="),
                punct("<="),
                punct(">="),
                punct("&&"),
                punct("||"),
                punct("+="),
                punct("-="),
            ]
        );
    }

    #[test]
    fn string_literals_keep_their_contents_and_escapes() {
        assert_eq!(tokens(r#""a\nb""#), vec![Token::Str("a\nb".to_string())]);
    }

    #[test]
    fn underscores_in_integer_literals_are_ignored() {
        assert_eq!(tokens("1_000"), vec![Token::Int(1000)]);
    }

    #[test]
    fn tokens_remember_their_line() {
        let lines: Vec<u32> = lex("a\n\nb").unwrap().iter().map(|s| s.line).collect();
        assert_eq!(lines, vec![1, 3]);
    }

    #[test]
    fn unterminated_string_is_an_error_with_its_line() {
        let err = lex("a\n\"abc").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        assert!(lex("/* abc").is_err());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Str(String),
    Interp(Vec<Piece>),
    Punct(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Text(String),
    Code(Vec<Spanned>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    token: Token,
    line: u32,
}

impl Spanned {
    pub fn new(token: Token, line: u32) -> Self {
        Self { token, line }
    }

    pub fn token(&self) -> &Token {
        &self.token
    }

    pub fn line(&self) -> u32 {
        self.line
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    line: u32,
    message: String,
}

impl LexError {
    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub const TERNARY: &str = "? ";

const THREE_CHAR_PUNCTS: [&str; 4] = ["..<", "...", "===", "!=="];
const TWO_CHAR_PUNCTS: [&str; 13] = [
    "->", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "??",
];

fn unicode_escape(chars: &[char], at: usize, line: u32) -> Result<(char, usize), LexError> {
    let bad = |message: &str| LexError {
        line,
        message: message.to_string(),
    };
    if chars.get(at) != Some(&'{') {
        return Err(bad("a unicode escape is written \\u{...}"));
    }
    let close = chars[at..]
        .iter()
        .position(|c| *c == '}')
        .map(|offset| at + offset)
        .ok_or_else(|| bad("unterminated unicode escape"))?;
    let digits: String = chars[at + 1..close].iter().collect();
    u32::from_str_radix(&digits, 16)
        .ok()
        .filter(|_| !digits.is_empty() && digits.len() <= 8)
        .and_then(char::from_u32)
        .map(|scalar| (scalar, close + 1))
        .ok_or_else(|| bad("invalid unicode escape"))
}

fn skip_code(chars: &[char], mut at: usize) -> Option<usize> {
    let mut depth = 1usize;
    while let Some(&c) = chars.get(at) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            '"' => {
                at = skip_string(chars, at + 1)?;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    None
}

fn skip_string(chars: &[char], mut at: usize) -> Option<usize> {
    while let Some(&c) = chars.get(at) {
        match c {
            '"' => return Some(at + 1),
            '\\' if chars.get(at + 1) == Some(&'(') => {
                at = skip_code(chars, at + 2)? + 1;
            }
            '\\' => at += 2,
            _ => at += 1,
        }
    }
    None
}

pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    lex_from(src, 1)
}

fn lex_from(src: &str, first_line: u32) -> Result<Vec<Spanned>, LexError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = first_line;
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
            let mut pieces: Vec<Piece> = Vec::new();
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
                    Some('\\') if chars.get(i + 1) == Some(&'(') => {
                        let Some(close) = skip_code(&chars, i + 2) else {
                            return Err(LexError {
                                line: start_line,
                                message: "unterminated string interpolation".to_string(),
                            });
                        };
                        let inner: String = chars[i + 2..close].iter().collect();
                        let code = lex_from(&inner, line)?;
                        line += u32::try_from(inner.matches('\n').count()).unwrap_or(0);
                        if !text.is_empty() {
                            pieces.push(Piece::Text(std::mem::take(&mut text)));
                        }
                        pieces.push(Piece::Code(code));
                        i = close + 1;
                    }
                    Some('\\') => {
                        if chars.get(i + 1) == Some(&'u') {
                            let (scalar, next) = unicode_escape(&chars, i + 2, line)?;
                            text.push(scalar);
                            i = next;
                            continue;
                        }
                        let escaped = match chars.get(i + 1) {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('r') => '\r',
                            Some('\\') => '\\',
                            Some('"') => '"',
                            Some('\'') => '\'',
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
            let token = if pieces.is_empty() {
                Token::Str(text)
            } else {
                if !text.is_empty() {
                    pieces.push(Piece::Text(text));
                }
                Token::Interp(pieces)
            };
            out.push(Spanned {
                token,
                line: start_line,
            });
        } else if c.is_ascii_digit() {
            let radix = match (c, chars.get(i + 1)) {
                ('0', Some('x')) => 16,
                ('0', Some('b')) => 2,
                ('0', Some('o')) => 8,
                _ => 10,
            };
            if radix != 10 {
                i += 2;
            }
            let mut value: i64 = 0;
            let mut digits = 0;
            while i < chars.len() && (chars[i].is_digit(radix) || chars[i] == '_') {
                if let Some(d) = chars[i].to_digit(radix) {
                    digits += 1;
                    value = value
                        .checked_mul(i64::from(radix))
                        .and_then(|v| v.checked_add(i64::from(d)))
                        .ok_or(LexError {
                            line,
                            message: "integer literal overflows".to_string(),
                        })?;
                }
                i += 1;
            }
            if digits == 0 {
                return Err(LexError {
                    line,
                    message: "a number prefix needs digits".to_string(),
                });
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
            let mut text = match matched {
                Some(p) => p.to_string(),
                None => c.to_string(),
            };
            i += text.chars().count();
            let spaced = |at: usize| chars.get(at).is_some_and(|c| c.is_whitespace());
            if text == "?" && i >= 2 && spaced(i - 2) && spaced(i) {
                text = TERNARY.to_string();
            }
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
        lex(src)
            .unwrap()
            .into_iter()
            .map(|s| s.token().clone())
            .collect()
    }

    fn ident(s: &str) -> Token {
        Token::Ident(s.to_string())
    }

    fn punct(s: &str) -> Token {
        Token::Punct(s.to_string())
    }

    #[test]
    fn a_string_without_an_interpolation_is_one_string_token() {
        assert_eq!(tokens("\"a\\nb\""), vec![Token::Str("a\nb".to_string())]);
    }

    #[test]
    fn a_string_with_an_interpolation_is_split_into_text_and_code() {
        let found = tokens("\"a\\(x + 1)b\"");
        let [Token::Interp(pieces)] = found.as_slice() else {
            panic!("expected an interpolated string");
        };
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0], Piece::Text("a".to_string()));
        assert_eq!(pieces[2], Piece::Text("b".to_string()));
        let Piece::Code(code) = &pieces[1] else {
            panic!("expected code");
        };
        let inner: Vec<Token> = code.iter().map(|t| t.token().clone()).collect();
        assert_eq!(inner, vec![ident("x"), punct("+"), Token::Int(1)]);
    }

    #[test]
    fn an_interpolation_may_contain_strings_that_contain_interpolations() {
        let found = tokens("\"a\\(\"b\\(c)\")d\"");
        assert!(matches!(found.as_slice(), [Token::Interp(pieces)] if pieces.len() == 3));
    }

    #[test]
    fn an_unterminated_interpolation_is_an_error() {
        let error = lex("\"a\\(1").unwrap_err();
        assert!(error.message().contains("interpolation"));
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
    fn identity_operators_are_single_tokens() {
        assert_eq!(
            tokens("a === b !== c"),
            vec![
                ident("a"),
                punct("==="),
                ident("b"),
                punct("!=="),
                ident("c")
            ]
        );
    }

    #[test]
    fn multi_character_operators_are_single_tokens() {
        assert_eq!(
            tokens("-> ..< ... == != <= >= && || += -= *= /= %="),
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
                punct("*="),
                punct("/="),
                punct("%="),
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
        let lines: Vec<u32> = lex("a\n\nb").unwrap().iter().map(|s| s.line()).collect();
        assert_eq!(lines, vec![1, 3]);
    }

    #[test]
    fn unterminated_string_is_an_error_with_its_line() {
        let err = lex("a\n\"abc").unwrap_err();
        assert_eq!(err.line(), 2);
    }

    #[test]
    fn unterminated_block_comment_is_an_error() {
        assert!(lex("/* abc").is_err());
    }

    #[test]
    fn a_question_mark_with_spaces_on_both_sides_is_the_ternary_mark() {
        assert_eq!(
            tokens("a ? b : c"),
            vec![
                ident("a"),
                punct(TERNARY),
                ident("b"),
                punct(":"),
                ident("c")
            ]
        );
        assert_eq!(
            tokens("a?.b"),
            vec![ident("a"), punct("?"), punct("."), ident("b")]
        );
        assert_eq!(tokens("Int? x"), vec![ident("Int"), punct("?"), ident("x")]);
        assert_eq!(tokens("a ?? b"), vec![ident("a"), punct("??"), ident("b")]);
    }

    #[test]
    fn a_unicode_escape_names_a_scalar_by_its_hexadecimal_value() {
        assert_eq!(
            tokens("\"a\\u{41}\\u{1F600}\""),
            vec![Token::Str("aA\u{1F600}".to_string())]
        );
    }

    #[test]
    fn the_carriage_return_and_single_quote_escapes_are_read() {
        assert_eq!(tokens("\"\\r\\'\""), vec![Token::Str("\r'".to_string())]);
    }

    #[test]
    fn a_malformed_unicode_escape_is_an_error() {
        for bad in ["\"\\u41\"", "\"\\u{}\"", "\"\\u{110000}\"", "\"\\u{zz}\""] {
            assert!(lex(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn integer_literals_may_be_hexadecimal_binary_or_octal_and_use_underscores() {
        assert_eq!(
            tokens("0xFF 0b101 0o17 1_000 0x_f_f"),
            vec![
                Token::Int(255),
                Token::Int(5),
                Token::Int(15),
                Token::Int(1000),
                Token::Int(255)
            ]
        );
    }

    #[test]
    fn a_prefix_without_digits_or_a_value_too_large_is_an_error() {
        for bad in ["0x", "0b2", "0xFFFFFFFFFFFFFFFFF"] {
            assert!(lex(bad).is_err(), "{bad}");
        }
    }
}

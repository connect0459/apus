use crate::lexer::{Spanned, Token};

const ARRAY: &str = include_str!("array_prelude.swift");
const LAZY: &str = include_str!("lazy_prelude.swift");

const ARRAY_WORDS: [&str; 6] = [
    "prefix",
    "suffix",
    "dropFirst",
    "dropLast",
    "reversed",
    "drop",
];

pub fn extras(tokens: &[Spanned]) -> String {
    let mut array = false;
    let mut lazy = false;
    for spanned in tokens {
        if let Token::Ident(word) = spanned.token() {
            array |= ARRAY_WORDS.contains(&word.as_str());
            lazy |= word == "lazy";
        }
    }
    let mut text = String::new();
    if array {
        text.push_str(ARRAY);
    }
    if lazy {
        text.push_str(LAZY);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    fn extras_of(source: &str) -> String {
        extras(&lex(source).unwrap())
    }

    #[test]
    fn a_program_that_uses_none_of_the_library_words_gets_no_extra_source() {
        assert_eq!(extras_of("func main() { print(1) }"), "");
    }

    #[test]
    fn a_word_of_the_array_library_adds_its_source_only() {
        let text = extras_of("func main() { let a = [1].reversed() }");
        assert!(text.contains("func _arrayReversed"));
        assert!(!text.contains("LazySequence"));
    }

    #[test]
    fn the_lazy_word_adds_the_lazy_library() {
        let text = extras_of("func main() { let a = [1].lazy }");
        assert!(text.contains("struct LazySequence"));
        assert!(!text.contains("func _arrayReversed"));
    }
}

use apus::{items::split_items, lexer::lex, parser::parse_item};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let src = std::fs::read_to_string(path).unwrap();
    let t = Instant::now();
    let tokens = lex(&src).unwrap();
    let lexed = t.elapsed();
    let items = split_items(&tokens).unwrap();
    let split = t.elapsed();
    for item in &items {
        if let Err(e) = parse_item(item.tokens()) {
            panic!("{}: line {}: {}", item.name(), e.line(), e.message());
        }
    }
    println!(
        "bytes={} tokens={} items={} lex={:?} lex+split={:?} lex+split+parse={:?}",
        src.len(),
        tokens.len(),
        items.len(),
        lexed,
        split,
        t.elapsed()
    );
}

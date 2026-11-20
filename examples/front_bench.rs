use apus::{items::split_items, lexer::lex};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let src = std::fs::read_to_string(path).unwrap();
    let t = Instant::now();
    let tokens = lex(&src).unwrap();
    let lexed = t.elapsed();
    let items = split_items(&tokens).unwrap();
    println!(
        "bytes={} tokens={} items={} lex={:?} lex+split={:?}",
        src.len(),
        tokens.len(),
        items.len(),
        lexed,
        t.elapsed()
    );
}

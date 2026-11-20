use apus::ast::Decl;
use apus::check::{Signatures, check_func};
use apus::codegen::compile;
use apus::items::split_items;
use apus::lexer::lex;
use apus::link::link;
use apus::parser::parse_item;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("source path");
    let output = std::env::args().nth(2).expect("output path");
    let source = std::fs::read_to_string(path).unwrap();
    let t = Instant::now();
    let tokens = lex(&source).unwrap();
    let items = split_items(&tokens).unwrap();
    let decls: Vec<Decl> = items
        .iter()
        .map(|i| parse_item(i.tokens()).unwrap())
        .collect();
    let front = t.elapsed();
    let t = Instant::now();
    let sigs = Signatures::collect(&decls).unwrap();
    let funcs: Vec<_> = decls
        .iter()
        .filter_map(|d| match d {
            Decl::Func(f) => Some(check_func(f, &sigs).unwrap()),
            _ => None,
        })
        .collect();
    let checked = t.elapsed();
    let t = Instant::now();
    let object = compile(&funcs, &sigs.layouts()).unwrap();
    let generated = t.elapsed();
    let t = Instant::now();
    link(
        &object,
        std::path::Path::new(&output),
        apus::link::Linker::Cc,
        std::path::Path::new(&format!("{output}.cache")),
    )
    .unwrap();
    let linked = t.elapsed();
    println!(
        "front(lex+split+parse)={front:?} check={checked:?} codegen={generated:?} link={linked:?} object={}KiB",
        object.len() / 1024
    );
}

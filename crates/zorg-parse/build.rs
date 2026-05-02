use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"),
    );
    let grammar_src = manifest_dir.join("../../..").join("zorg-treesitter/src");
    let parser = grammar_src.join("parser.c");

    println!("cargo:rerun-if-changed={}", parser.display());
    println!(
        "cargo:rerun-if-changed={}",
        grammar_src.join("tree_sitter/parser.h").display()
    );

    if !parser.exists() {
        panic!(
            "missing generated Zorg Tree-sitter parser at {}; run `npm run generate` in ../zorg-treesitter",
            parser.display()
        );
    }

    cc::Build::new()
        .include(&grammar_src)
        .file(parser)
        .warnings(false)
        .compile("tree-sitter-zorg");
}

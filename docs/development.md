# Zorg Rust Development

This repository uses a Rust 2024 workspace. The local stable toolchain used for
the foundation phase supports Rust 2024, `rustfmt`, and `clippy`, so no edition
fallback is required.

## Workspace Layout

- `crates/zorg-core`: shared semantic types, diagnostics, and source spans.
- `crates/zorg-parse`: parser entry points and Tree-sitter integration boundary.
- `crates/zorg-store`: indexing and persistence boundary.
- `crates/zorg-query`: SWOG LIST query boundary.
- `crates/zorg-fix`: strict check and autofix boundary.
- `crates/zorg-capture`: capture/template boundary.
- `crates/zorg-cli`: `zorg` command-line binary.
- `crates/zorg-ls`: `zorg-ls` language-server binary.

The crates are intentionally skeletal during Epic 1. They expose stable places
for later implementation work without claiming parser, store, query, LSP,
capture, or formatter behavior is complete.

## Tree-sitter Grammar

`crates/zorg-parse` links the local generated Zorg grammar from
`../zorg-treesitter/src/parser.c` through its build script. If that file is
missing or stale, run `npm run generate` in the sibling `../zorg-treesitter`
repository before building or testing this workspace.

This is a local integration boundary for parser development. Release packaging
can replace it later without changing downstream Rust code that relies on the
public node names documented in `../zorg-treesitter/docs/grammar.md`.

## Validation

Run these commands from the repository root:

```sh
python3 tools/check_fixture_manifest.py
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
```

The fixture manifest command verifies that `fixtures/corpus/**/*.z`, the
machine-readable manifest, and the recorded Tree-sitter/Neovim fixture
derivations have not drifted.

For the LSP MVP specifically, `cargo test -p zorg-ls` starts `zorg-ls` over
stdio, builds temporary store indexes, and exercises diagnostics, navigation,
symbols, completion, rename, code actions, degraded store states, non-`.z`
documents, and single-root multi-folder initialization.

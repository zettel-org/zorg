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

## Validation

Run these commands from the repository root:

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
```

The smoke tests prove both binaries exist and respond to their foundation-phase
surfaces. Later phases should extend those tests as behavior moves out of stubs.

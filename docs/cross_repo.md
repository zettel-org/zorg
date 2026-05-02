# Zorg Cross-Repo Contract

Epic 1 leaves three repositories with separate ownership and one shared v1
contract. This document records how the repos fit together, what must stay
aligned, and the validation results from the Phase 5 handoff on 2026-05-02.

## Repository Roles

- `../zorg` owns the Rust workspace, CLI binaries, language server binary,
  shared documentation, and canonical fixtures.
- `../zorg-treesitter` owns the Tree-sitter grammar skeleton, corpus tests, and
  editor query files for the `zorg` parser.
- `../zorg-nvim` owns Neovim filetype detection, setup, command wrappers, LSP
  startup, Tree-sitter registration, help docs, and health checks.

Runtime semantics belong in the Rust crates. Tree-sitter should expose syntax
structure for parsing and editor features, and Neovim should delegate to
`zorg`, `zorg-ls`, and the `zorg` Tree-sitter parser instead of reimplementing
Zorg model behavior in Lua.

## Shared Decisions

All three repos agree on these Epic 1 decisions:

- Canonical source files use `.z`.
- The default corpus root is `~/zorg`.
- Directory zettel use `init.z`.
- File zettel headers use `%%% @foo ... %%%`.
- IDs use `@foo`; local IDs use `^bar`.
- Links use `#foo/bar`, `+child`, and `~sibling`.
- Type tags use `#z/...`.
- Properties use `key::value`.
- Todos use `[ ]`, `[N]`, `[X]`, and `[?]`.
- Query and template definitions are ordinary zettel tagged `#z/query` and
  `#z/tmpl`.
- Legacy Python-era syntax is not a v1 compatibility surface.

Unsupported legacy forms include `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`,
`LID::`, `tick::`, old folgezettel IDs, tag sugar, and Python-era link
behavior.

## Naming Alignment

- Rust CLI package: `zorg-cli`.
- Rust CLI binary: `zorg`.
- Rust language-server package and binary: `zorg-ls`.
- Tree-sitter grammar name: `zorg`.
- Tree-sitter language scope: `source.zorg`.
- Neovim filetype: `zorg`.
- Neovim Tree-sitter parser registration name: `zorg`.

Tree-sitter Epic 1 placeholder nodes are intentionally conservative:
`document`, `file_header`, `identifier`, `local_identifier`, `hash_reference`,
`child_link`, `sibling_link`, `property`, `todo_marker`, `code_fence`, and
`text`. Later grammar work should extend those nodes only when the Rust model
and editor query contracts are updated together.

## Fixture Synchronization

`../zorg/fixtures/corpus` is the canonical fixture source. Downstream repos may
copy small examples into local test formats when their tooling requires it, but
the copied examples must preserve the contract in `../zorg/docs`.

When adding shared fixture coverage:

1. Add or update the canonical `.z` fixture in `../zorg/fixtures/corpus`.
2. Document the new fixture intent in `../zorg/fixtures/README.md`.
3. Port only the minimum needed example into `../zorg-treesitter/test/corpus`
   or Neovim tests.
4. Keep accepted fixtures `.z`-only. Legacy-looking negative examples should be
   explicit invalid cases, not compatibility fixtures.

## Validation Results

Commands run locally on 2026-05-02:

### `../zorg`

- `cargo fmt --check`: passed.
- `cargo test --workspace`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo run -p zorg-cli -- --help`: passed.
- `cargo run -p zorg-ls -- --version`: passed.

### `../zorg-treesitter`

- `npm run generate`: passed.
- `npm test`: passed.

### `../zorg-nvim`

- `nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"`:
  passed.
- `stylua --check .`: passed.
- `luacheck lua tests filetype.lua plugin ftplugin`: passed.

No validation command was skipped during the Phase 5 handoff.

## Handoff Notes

Recommended next implementation work:

- Epic 2 grammar work should expand `../zorg-treesitter/grammar.js` and corpus
  coverage from the shared fixtures before adding speculative syntax.
- Epic 3 Rust model work should implement parsing, source spans, ID resolution,
  tag/property/todo modeling, and diagnostics in `zorg-core` and `zorg-parse`.
- LSP, query, capture, fix, and Neovim behavior should remain stubbed until the
  Rust model contract can provide source-backed semantics.
- Any future rename or autofix behavior must reject edits when source spans are
  missing or ambiguous.

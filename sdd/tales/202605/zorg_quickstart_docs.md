---
create_time: 2026-05-04 11:37:16
status: done
prompt: sdd/prompts/202605/zorg_quickstart_docs.md
---
# Plan: Zorg Quickstart Documentation

## Goal

Create `docs/quickstart.md` as a first-use guide for installing Zorg from this repository and starting a small canonical
`.z` corpus. The guide should be practical for a new user, accurate to the current Rust workspace, and shorter than the
broad README CLI quick start.

## Context

- Zorg is a Rust 2024 workspace. The user-facing binaries are `zorg` from `crates/zorg-cli` and `zorg-ls` from
  `crates/zorg-ls`.
- Local source installation is currently documented with:
  - `cargo install --path crates/zorg-cli`
  - `cargo install --path crates/zorg-ls`
- Source builds depend on the sibling `../zorg-treesitter` generated parser (`src/parser.c`) when parser artifacts are
  missing or stale.
- The default corpus root is `~/zorg`.
- The default database is `<root>/.zorg/zorg.sqlite3`, with path resolution controlled by CLI flags, `ZORG_ROOT` /
  `ZORG_DATABASE_PATH`, config files, and then defaults.
- Canonical source files use `.z`; legacy `.zo`, `.zoq`, `.zot`, and `.zoc` files are not normal v1 source.
- Core first-use commands are:
  - `zorg --version`
  - `zorg-ls --version`
  - `zorg check --root ~/zorg`
  - `zorg db status --root ~/zorg`
  - `zorg db reindex --root ~/zorg`
  - `zorg query '<swog>' --root ~/zorg`
  - `zorg path @id --root ~/zorg`
  - `zorg watch --root ~/zorg`
  - `zorg dash --root ~/zorg`

## Documentation Shape

`docs/quickstart.md` should use these sections:

1. Title and short purpose statement.
2. Prerequisites:
   - Rust toolchain supporting Rust 2024 / workspace rust-version `1.85`.
   - This repository checkout.
   - Sibling `../zorg-treesitter` generation note for source builds.
3. Install from source:
   - Build workspace sanity check.
   - Install `zorg` and `zorg-ls` binaries from local crate paths.
   - Verify versions.
4. Create a first corpus:
   - Create `~/zorg`.
   - Add a minimal `start.z` with a file zettel, todo, query zettel, and template zettel using canonical v1 syntax.
5. Validate and index:
   - Run `zorg check`.
   - Inspect resolved root/database with `zorg db status`.
   - Run `zorg db reindex`.
6. Start using Zorg:
   - Run inline queries.
   - Run saved query by ID.
   - Resolve an ID to source with `zorg path`.
   - Start `zorg watch` in a separate terminal while editing.
   - Launch `zorg dash` after indexing.
   - Optionally create a note through `zorg capture`.
7. Editor integration:
   - Install `zorg-ls`.
   - Build/refresh the index before graph-backed editor features.
   - Point editor LSP configuration at `zorg-ls` and the selected root/database.
8. Configuration and next reading:
   - Mention `--root`, `--db`, `ZORG_ROOT`, `ZORG_DATABASE_PATH`, and config files at a high level.
   - Link to existing docs for syntax, query, capture, LSP, import/export, and development.

## Style And Constraints

- Keep the guide user-facing and command-oriented.
- Prefer installed binary commands (`zorg ...`) rather than `cargo run ...` after installation.
- Use temporary or explicit root/database examples only when needed; default to `~/zorg` because that is the product
  contract.
- Do not present legacy files as normal input. Mention import only as a later migration path.
- Do not over-document every command from README. The quickstart should be the path from zero to a working corpus.
- Keep examples copy-pasteable and use LF/UTF-8 `.z` syntax.

## Verification

- Run `cargo run -p zorg-cli -- --help` or rely on the already verified output to confirm command names.
- Inspect the new Markdown for broken obvious links and formatting.
- Optionally run a Markdown-adjacent smoke check by creating the sample corpus in a temporary directory and executing
  the documented check/reindex/query/path commands against it.

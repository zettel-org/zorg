# Zorg

Zorg is an editor-agnostic plaintext zettelkasten built around one primitive:
the zettel. The v1 MVP uses `.z` files under `~/zorg`, a Rust CLI and library
workspace, a Tree-sitter grammar, SQLite indexing, SWOG LIST queries, `zorg-ls`,
capture/fix commands, and a Neovim frontend.

This repository owns the Rust implementation and the shared product contract.
It contains the Rust parser/model, SQLite store, SWOG query engine, CLI,
capture/fix workflow, LSP server, shared documentation, and canonical fixtures
that Tree-sitter and Neovim integrations consume.

## Non-Negotiable MVP Contract

- Canonical source files use `.z`.
- The default corpus root is `~/zorg`.
- File zettel headers use `%%% @foo ... %%%`.
- Directory zettel are `init.z`.
- IDs are `@foo`; local IDs are `^bar`, resolved under the nearest ancestor ID
  as `@ancestor/bar`.
- Links are absolute `#foo/bar`, child-relative `+child`, and sibling-relative
  `~sibling`.
- Type tags use `#z/...`.
- Properties use `key::value`; slash-separated list values are the v1 list form.
- Lifecycle properties are `do::`, `due::`, and `did::`.
- Timebox properties are `p::`, `start::`, and `end::`.
- Todos are `[ ]`, `[N]`, `[X]`, and `[?]`.
- Code blocks are ordinary Markdown fences.
- Query output is LIST only.
- Query and template definitions are ordinary zettel tagged `#z/query` and
  `#z/tmpl`.

Legacy Python-era syntax is not supported as readable or writable v1 syntax.
That includes `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, `tick::`, generated
`.zoc`, custom `@@@` fences, old folgezettel IDs, tag sugar, and Python-era link
behavior.

## Documentation Map

- `docs/syntax.md`: `.z` syntax and explicit no-legacy policy.
- `docs/model.md`: semantic zettel model, IDs, hierarchy, links, tags,
  properties, todos, diagnostics, and source spans.
- `docs/query.md`: SWOG LIST MVP, filters, ordering, query zettel execution,
  and deferred query behavior.
- `docs/lsp.md`: `zorg-ls` MVP boundaries, diagnostics, source-span
  expectations, rename safety, and root handling.
- `docs/capture.md`: `#z/tmpl` template discovery, capture destinations,
  source-file recording, and noninteractive CLI expectations.
- `docs/fix.md`: strict check mode, allowed autofixes, idempotency, and
  unsupported legacy diagnostics.
- `docs/development.md`: Rust workspace layout, crate boundaries, and
  validation commands.
- `docs/cross_repo.md`: cross-repo ownership, naming alignment, fixture
  synchronization, validation results, and handoff notes.

## Fixtures

`fixtures/corpus` is the canonical cross-repo fixture corpus. Keep it small and
representative. The Rust parser/model, Tree-sitter grammar, Neovim plugin, LSP,
query, capture, and fix tests should reuse these files before adding local
copies.

See `fixtures/README.md` for the fixture inventory and policy.

## Query Quick Start

SWOG LIST queries run against the SQLite index for a corpus root:

```bash
cargo run -p zorg-cli -- db reindex --root fixtures/corpus
cargo run -p zorg-cli -- query '#z/todo -did:*' --root fixtures/corpus
cargo run -p zorg-cli -- query --id @query-fixture/queries/daily --root fixtures/corpus
```

The CLI rejects deferred TABLE, aggregation, OR, and parenthesized query forms
with explicit parser errors. See `docs/query.md` for the full MVP query
contract.

## Rust Validation

Run the Rust validation commands from this repository root:

```bash
python3 tools/check_fixture_manifest.py
cargo fmt --check
cargo test --workspace
cargo test --workspace mvp_e2e
cargo clippy --workspace --all-targets -- -D warnings
```

`cargo test --workspace mvp_e2e` is the named Rust MVP end-to-end harness. It
uses temporary `~/zorg`-like roots and explicit database paths to cover parse,
strict legacy rejection, reindex, inline and query-zettel SWOG queries, JSON
capture, fix, `fix --check`, and `zorg-ls` diagnostics/navigation actions
without requiring or modifying a developer's real `~/zorg`.

## Cross-Repo Validation

Run the full local MVP gate from this repository root when validating Rust,
Tree-sitter, and Neovim together:

```bash
tools/validate_cross_repo.sh
```

The command expects sibling `../zorg-treesitter` and `../zorg-nvim` checkouts,
checks required local tools, then runs the Rust workspace checks, Tree-sitter
generation/query/shared-fixture checks, and Neovim headless tests. See
`docs/cross_repo.md` for troubleshooting and path overrides.

## Related Repositories

- `../zorg`: this repo; Rust CLI, libraries, shared docs, and fixtures.
- `../zorg-treesitter`: Tree-sitter grammar and editor query files.
- `../zorg-nvim`: Neovim plugin and user-facing editor integration.

## Current Status

The Rust workspace now has the parser/model foundation, SQLite store indexing,
SWOG LIST query evaluation, inline `zorg query`, and query-by-`#z/query` ID
execution. `zorg-ls` now exposes the MVP language-server surface over stdio,
including live diagnostics, indexed graph navigation, symbols, completion,
safe rename planning, and deterministic quick fixes when the store index is
ready. Capture and strict fix/check behavior are implemented for the MVP
workflow and covered by CLI integration tests.

# Zorg

Zorg is an editor-agnostic plaintext zettelkasten built around one primitive:
the zettel. The v1 MVP uses `.z` files under `~/zorg`, a Rust CLI and library
workspace, a Tree-sitter grammar, SQLite indexing, SWOG LIST queries, `zorg-ls`,
capture/fix commands, and a Neovim frontend.

This repository owns the Rust implementation and the shared product contract.
The implementation is intentionally skeletal during Epic 1; the current work is
documentation and fixtures that later parser, model, query, LSP, capture, fix,
Tree-sitter, and Neovim agents consume.

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

## Fixtures

`fixtures/corpus` is the canonical cross-repo fixture corpus. Keep it small and
representative. The Rust parser/model, Tree-sitter grammar, Neovim plugin, LSP,
query, capture, and fix tests should reuse these files before adding local
copies.

See `fixtures/README.md` for the fixture inventory and policy.

## Related Repositories

- `../zorg`: this repo; Rust CLI, libraries, shared docs, and fixtures.
- `../zorg-treesitter`: Tree-sitter grammar and editor query files.
- `../zorg-nvim`: Neovim plugin and user-facing editor integration.

## Current Status

Epic 1 is repository foundation work. The Rust workspace now provides executable
crate and binary scaffolding, but the parser, SQLite store, query engine, LSP
protocol behavior, capture writer, and formatter remain intentionally stubbed.
Later phases will implement those behaviors while preserving the docs and
fixtures as the shared contract.

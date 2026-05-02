# Zorg Fixture Corpus

`fixtures/corpus` is the canonical cross-repo fixture contract for the Zorg v1
MVP. The Rust parser/model, Tree-sitter grammar, Neovim plugin tests, query
tests, LSP diagnostics, capture examples, and fix/check tests should reuse these
files before adding local variants.

The corpus is intentionally small. Add fixtures only when they clarify a shared
syntax or semantic contract that more than one repo should honor.

## Files

- `minimal.z`: one file zettel with an ID, property, tag, and paragraph.
- `nested.z`: nested notes, local ID, absolute/child/sibling links, and todos.
- `query_and_template.z`: `#z/query` and `#z/tmpl` examples in ordinary zettel.
- `query_focus.z`: query-focused todos, properties, links, text, and stored
  query definitions.
- `legacy_invalid.z`: legacy-looking syntax that strict checks should reject.
- `dir/init.z`: directory zettel example.

## Policy

Fixtures use `.z` only. Do not add `.zo`, `.zoq`, `.zot`, or `.zoc` as accepted
input fixtures. Invalid legacy-looking examples belong in `legacy_invalid.z` or
clearly named negative fixtures.

# Zorg Fixture Corpus

`fixtures/corpus` is the canonical cross-repo fixture contract for the Zorg v1
MVP. The Rust parser/model, Tree-sitter grammar, Neovim plugin tests, query
tests, LSP diagnostics, capture examples, and fix/check tests should reuse these
files before adding local variants.

The corpus is intentionally small. Add fixtures only when they clarify a shared
syntax or semantic contract that more than one repo should honor.

## Files

- `autofix_fixed.z`: expected fixed output for safe autofix behavior.
- `autofix_unfixed.z`: fixable strict-check input for autofix planning.
- `minimal.z`: one file zettel with an ID, property, tag, and paragraph.
- `nested.z`: nested notes, local ID, absolute/child/sibling links, and todos.
- `query_and_template.z`: `#z/query` and `#z/tmpl` examples in ordinary zettel.
- `query_focus.z`: query-focused todos, properties, links, text, and stored
  query definitions.
- `legacy_invalid.z`: legacy-looking syntax that strict checks should reject.
- `dir/init.z`: directory zettel example.

## Manifest

`fixtures/manifest.json` is the machine-readable fixture contract. It lists
every canonical fixture, its content hash, whether the fixture is valid or
negative, its role, and the Rust, Tree-sitter, or Neovim surfaces expected to
exercise it.

The manifest also tracks import/export bridge fixtures under
`fixtures/import_export`. Those files are not part of the canonical parser/store
corpus. Legacy `.zo`, `.zoq`, `.zot`, and `.zoc` files in that tree are
import-only inputs, while `.z`, `.md`, and `.json` files there are expected
bridge outputs or plans for future bridge tests.

Run the synchronization check from the Rust repo root:

```sh
python3 tools/check_fixture_manifest.py
```

The check fails when `fixtures/corpus/**/*.z` and the manifest disagree, when a
canonical fixture hash changes without a manifest update, when a declared
downstream fixture is missing, or when a tracked downstream copy/derivation
drifts. It also fails when `fixtures/import_export` contains an untracked
bridge fixture, when a bridge fixture hash drifts, or when an import/export
fixture is declared with the wrong kind or extension.
Tree-sitter corpus files are recorded as derived fixtures because
`test/corpus/*.txt` must include expected parse trees. Local-only Neovim test
fixtures must carry an explicit reason in the manifest.

## Policy

Fixtures use `.z` only. Do not add `.zo`, `.zoq`, `.zot`, or `.zoc` as accepted
input fixtures. Invalid legacy-looking examples belong in `legacy_invalid.z` or
clearly named negative fixtures.

Import/export bridge fixtures are the one exception to the extension rule, and
only under `fixtures/import_export`. They exist to test explicit bridge
conversion contracts; they must not be copied into `fixtures/corpus` or treated
as accepted normal Zorg input.

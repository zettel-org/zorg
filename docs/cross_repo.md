# Zorg Cross-Repo Contract

Epic 1 leaves three repositories with separate ownership and one shared v1
contract. This document records how the repos fit together, what must stay
aligned, and how to run the local validation gate across all three sibling
repositories.

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
`source_file`, `file_header`, `id`, `local_id`, `hash_reference`, `child_link`,
`sibling_link`, `property`, `todo_marker`, `code_fence`, and `text`. Later
grammar work should extend those nodes only when the Rust model and editor query
contracts are updated together.

## Fixture Synchronization

`../zorg/fixtures/corpus` is the canonical fixture source. Downstream repos may
copy small examples into local test formats when their tooling requires it, but
the copied examples must preserve the contract in `../zorg/docs`.

`../zorg/fixtures/manifest.json` is the machine-readable fixture contract. It
lists every canonical fixture, its content hash, whether it is valid or
negative, the role it serves, and the Rust, Tree-sitter, or Neovim surfaces
expected to exercise it. The same manifest records downstream fixture
provenance:

- Tree-sitter `test/corpus/*.txt` entries are derived fixtures because the
  corpus format mixes source snippets with expected parse trees.
- Tree-sitter highlight smoke input is a derived editor-query fixture that
  combines representative canonical syntax.
- Neovim `.tests/root/captured.z` is local-only because it is a generated target
  opened by command tests, not a copied canonical fixture.

Run the deterministic sync check from the Rust repo root:

```sh
python3 tools/check_fixture_manifest.py
```

The command fails when a canonical `.z` file is missing from the manifest, when
a canonical fixture hash changes without a manifest update, when a declared
downstream fixture is missing, or when a tracked downstream fixture hash changes
without updating its recorded provenance.

When adding shared fixture coverage:

1. Add or update the canonical `.z` fixture in `../zorg/fixtures/corpus`.
2. Update `../zorg/fixtures/manifest.json` with its role, validity, content
   hash, expected surfaces, and current source hash references from any
   downstream fixtures.
3. Document the fixture intent in `../zorg/fixtures/README.md`.
4. Port only the minimum needed example into `../zorg-treesitter/test/corpus`,
   `../zorg-treesitter/test/highlight`, or Neovim tests.
5. Run `python3 tools/check_fixture_manifest.py`, then the local validation
   commands for each repo whose fixtures changed.
6. Keep accepted fixtures `.z`-only. Legacy-looking negative examples should be
   explicit invalid cases, not compatibility fixtures.

## Cross-Repo Validation Gate

Run the full local MVP gate from the Rust repo root:

```sh
tools/validate_cross_repo.sh
```

The command validates the sibling repositories in this order:

1. Rust workspace: fixture manifest sync, formatting, full workspace tests, the
   named MVP E2E harness, clippy, `zorg --help`, and `zorg-ls --version`.
2. Tree-sitter grammar: npm dependency install, parser generation, corpus
   tests, editor query compilation, highlight smoke, and parsing all valid
   shared fixtures from `fixtures/manifest.json`.
3. Neovim plugin: headless `smoke`, `commands`, `helpers`, and `lsp` tests,
   including runtime query loading checks.

The script fails fast. Each step prints a short label before it runs, and a
failure reports the active step so the broken repo or command is visible
without reading a long transcript. On a normal development machine, expect the
gate to take several minutes because it runs the full Rust workspace tests and
clippy.

Required local tools are checked before validation starts:

- `cargo`
- `npm` and `npx`
- `nvim`
- `python3`

The Tree-sitter CLI is resolved through `../zorg-treesitter` npm dependencies
with `npx --no-install tree-sitter`; a global Tree-sitter install is not
required. If `../zorg-treesitter` or `../zorg-nvim` are not adjacent to the
Rust checkout, set `ZORG_TREESITTER_DIR` or `ZORG_NVIM_DIR`:

```sh
ZORG_TREESITTER_DIR=/path/to/zorg-treesitter \
ZORG_NVIM_DIR=/path/to/zorg-nvim \
tools/validate_cross_repo.sh
```

The gate uses repository fixtures, temporary test roots, explicit test database
paths, and Neovim test binaries. It must not depend on or mutate a developer's
real `~/zorg` corpus.

Troubleshooting:

- If parser generation changes files under `../zorg-treesitter/src`, inspect
  the generated diff and confirm it matches the generated-artifact policy
  before committing anything.
- If `tree-sitter parse` fails, confirm that the failing path is a manifest
  fixture with `"validity": "valid"` and that the grammar was generated from
  the current checkout.
- If a Neovim test cannot find query files, confirm the command is running from
  `../zorg-nvim` or set `ZORG_NVIM_DIR`; the tests prepend that repo to
  `runtimepath`.
- If the fixture manifest check fails, update the canonical fixture hash or
  downstream provenance only after confirming the source change is intentional.

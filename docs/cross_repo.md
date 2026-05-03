# Zorg Cross-Repo Contract

Zorg v1 uses three repositories with separate ownership and one shared source,
runtime, and editor contract. This document records how the repos fit together,
what must stay aligned, and how to run the local validation gate across all
three sibling repositories.

## Repository Roles

- `../zorg` owns the Rust workspace, CLI binaries, language server binary,
  shared documentation, and canonical fixtures.
- `../zorg-treesitter` owns the Tree-sitter grammar, corpus tests, generated
  parser boundary, and editor query files for the `zorg` parser.
- `../zorg-nvim` owns Neovim filetype detection, setup, command wrappers, LSP
  startup, Tree-sitter registration, help docs, and health checks.

Runtime semantics belong in the Rust crates. Tree-sitter should expose syntax
structure for parsing and editor features, and Neovim should delegate to
`zorg`, `zorg-ls`, and the `zorg` Tree-sitter parser instead of reimplementing
Zorg model behavior in Lua.

## Shared Decisions

All three repos agree on these v1 decisions:

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

Tree-sitter public nodes are intentionally conservative and documented in
`../zorg-treesitter/docs/grammar.md`. Current public block nodes include
`source_file`, `file_header`, `file_header_open`, `file_header_close`,
`zettel_item`, `zettel_opening`, `paragraph`, `blank_line`, and
`fenced_code_block`. Current public inline nodes include `id`, `local_id`,
`absolute_link`, `child_link`, `sibling_link`, `tag`, `type_tag`, `property`,
`todo_marker`, and `title_text`. Rename or extend public nodes only when the
Rust model and editor query contracts are updated together.

## Live Indexing Contract

Epic 11 adds two Rust-owned freshness surfaces that future Neovim work should
wrap instead of reimplementing in Lua.

`zorg watch` is the long-running indexer process. Start one process for each
unique corpus root and database pair:

```sh
zorg watch --root <root> --db <database> --format json
```

The stable argv surface is `watch`, `--root PATH`, `--db PATH`, `--debounce
MS`, `--format text|json`, and the `--json` shortcut. `--exit-after-ready`,
`--once`, and `--exit-after-events N` are bounded modes for smoke tests and
health checks. They are not the normal editor watch mode.

JSON output is line-delimited, one object per lifecycle event. Every object has
these fields:

- `schema_version`: currently `1`.
- `state`: one of `starting`, `ready`, `indexing`, `indexed`, `degraded`,
  `error`, `stopping`, or `stopped`.
- `root`: resolved corpus root path.
- `database`: resolved SQLite database path.

`indexed` events also include a `summary` object with
`discovered_files`, `indexed_files`, `unchanged_files`, `new_files`,
`changed_files`, `deleted_files`, `zettel_count`, `diagnostic_count`,
`effective_tag_count`, and `last_indexed_at_unix_ms`. `degraded` and `error`
events include `message`.

Neovim should treat watcher events as status and freshness signals, not as
source-of-truth model data. The Rust watcher filters editor scratch paths,
legacy extensions, `.zorg`, the configured database and its SQLite sidecar
files, and unrelated non-`.z` files before scheduling a reindex. Accepted
events are debounced into `Store::reindex()` passes, so branch checkouts and
atomic-save bursts may produce one later `indexed` event instead of one event
per changed file.

Only one watcher should be running for a root/database pair. A second watcher
for the same pair duplicates work and can contend on SQLite writes. Separate
roots or separate database paths may use separate watcher jobs.

`zorg-ls` has a separate freshness path. It advertises
`textDocumentSync.save` and refreshes the store snapshot after
`textDocument/didSave` by opening the configured store, running
`Store::reindex()`, reloading graph data, and republishing diagnostics for known
indexed files and open buffers. It does not own a filesystem watcher. Current
freshness is surfaced through server log messages:

- `zorg-ls loaded store ...` means initialization found a usable snapshot.
- `zorg-ls running with degraded store status: ...` means graph-backed features
  are temporarily unavailable.
- `zorg-ls refreshed store index ...` means a save-triggered refresh succeeded.
- `zorg-ls store refresh recovered ...` means a degraded session became ready.
- `zorg-ls store refresh degraded: ...` means the save path still cannot load
  the configured root/database.

Recommended Epic 15 health wording: report watcher state separately from LSP
graph state. For example, show `watcher ready`, `watcher indexing`, or `watcher
stopped`; show `LSP graph ready`, `LSP graph degraded`, or `LSP graph refreshed
after save`. Do not present Neovim as owning the index, and do not promise that
a watcher event alone updates an already-running LSP snapshot before the save
refresh or client-triggered LSP lifecycle catches up.

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

The command accepts only `--help`/`-h` as an option. Unknown arguments exit
before validation starts so scripted invocations do not accidentally run a
different gate than intended.

The command validates the sibling repositories in this order:

1. Rust workspace: fixture manifest sync, formatting, serialized full
   workspace tests, the named MVP E2E harness, clippy, `zorg --help`, and
   `zorg-ls --version`. The gate runs the full workspace test step as
   `cargo test --workspace -- --test-threads=1` because several stdio LSP smoke
   tests spawn `zorg-ls` processes and are easier to diagnose when they cannot
   interfere with each other.
2. Tree-sitter grammar: npm dependency install, parser generation, corpus
   tests, editor query compilation, highlight smoke, and parsing all valid
   shared fixtures from `fixtures/manifest.json`. The parse step captures the
   Tree-sitter output and fails if any valid shared fixture emits `ERROR` or
   `MISSING` nodes, even when the CLI process itself exits successfully.
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

The script also checks that the Tree-sitter path has `package.json` and
`grammar.js`, and that the Neovim path has the Zorg Lua module and smoke test.
This catches common path mixups before longer Rust or npm work begins.

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

- If `zorg watch --format json` emits `error` before `ready`, confirm the root
  exists, the parent directory of the database is writable, and no other
  long-running writer owns the same SQLite database.
- If the watcher stays quiet after a large branch checkout, wait for the
  debounced reindex pass and then inspect the next `indexed.summary` counts.
  Overflow or imprecise backend notifications are treated as full incremental
  reindex hints.
- If source edits do not appear in graph-backed editor features, check both
  processes: the watcher may have refreshed the SQLite index while `zorg-ls`
  is still degraded until a save-triggered refresh reloads the graph snapshot.
- If `.z` changes are ignored, confirm the path is under the configured root
  and is not under `.zorg`, not the configured database or a SQLite sidecar,
  not an editor swap/temp file, and not a legacy extension such as `.zoq`.
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

## Release Handoff

`docs/release.md` is the release packaging contract for the MVP. It keeps the
cross-repo validation gate above as the required pre-release check and defines
the coordinated version, changelog, binary archive, checksum, generated parser,
and rollback policies.

Run the non-publishing release dry run from the Rust repo root:

```sh
tools/release_dry_run.sh
```

The dry run requires clean Rust, Tree-sitter, and Neovim worktrees, runs this
cross-repo gate, builds local release binaries, creates a temporary host
archive, verifies its checksum, and inspects package/archive outputs without
tagging, pushing, uploading, or publishing.

Generated Tree-sitter artifacts remain untracked for the MVP release. The
release dry run expects `npm run generate` to create `../zorg-treesitter/src`
locally before Rust release binaries are built, and it fails if the generated
parser outputs are missing.

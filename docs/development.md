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

The crates own the Rust MVP boundaries: parsing/model lowering, store indexing,
SWOG query evaluation, strict check/fix behavior, capture/template expansion,
the `zorg` CLI, and `zorg-ls`.

## Build and Install

Build the full workspace from the repository root:

```sh
cargo build --workspace
```

Install local binaries from source when you want `zorg` and `zorg-ls` on your
`PATH`:

```sh
cargo install --path crates/zorg-cli
cargo install --path crates/zorg-ls
```

The binaries report their versions without starting a database or LSP session:

```sh
zorg --version
zorg-ls --version
```

## Store Configuration

`StoreOptions` remains the canonical path object consumed by store APIs. The
CLI resolves those paths from the typed Zorg config contract before opening the
store.

Precedence is:

1. CLI flags: `--root PATH` and `--db PATH`.
2. Environment: `ZORG_ROOT`, `ZORG_DATABASE_PATH` or `ZORG_DB`,
   `ZORG_WATCHER_DEBOUNCE_MS`, and `ZORG_WATCHER_LOG_PATH`.
3. Root-local config: `<resolved-root>/.zorg/config.toml`.
4. User config: `$XDG_CONFIG_HOME/zorg/config.toml`, or
   `~/.config/zorg/config.toml` when `XDG_CONFIG_HOME` is unset.
5. Defaults: root `~/zorg`, database `<root>/.zorg/zorg.sqlite3`, watcher
   debounce `250`.

Supported TOML keys are `root`, `database_path`, `watcher_debounce_ms`,
`watcher_log_path`, and `[named_roots]`. Path values may start with `~/`, which
is expanded from the resolved home directory. The named-roots map is reserved
for later multi-root workflows; names currently accept ASCII letters, numbers,
`-`, and `_`.

`zorg db status` prints the resolved `root:` and `database:` lines without
changing its script-friendly output shape:

```sh
zorg db status
zorg db status --root fixtures/corpus --db /tmp/zorg.sqlite3
```

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
cargo test --workspace mvp_e2e
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
```

These commands cover the current v1 baseline surfaces: `db reindex`, `db
status`, inline and `#z/query`-backed `query`, strict `check`, deterministic
`fix`, JSON `capture`, and the stdio `zorg-ls` server.

The fixture manifest command verifies that `fixtures/corpus/**/*.z`, the
machine-readable manifest, and the recorded Tree-sitter/Neovim fixture
derivations have not drifted.

The named MVP E2E harness is `cargo test --workspace mvp_e2e`. It copies
canonical fixtures into temporary `~/zorg`-like roots, runs the CLI parse,
strict legacy rejection, explicit database reindex, inline and query-zettel
SWOG queries, JSON capture, reindex, captured-zettel query, fix, and
`fix --check` loop, then starts `zorg-ls` over stdio against an indexed temp
root for diagnostics, navigation, references, symbols, completion, and
quick-fix actions. The tests set an isolated process `HOME` and use explicit
roots/databases so they do not depend on or mutate a developer's real
`~/zorg`.

For the LSP MVP specifically, `cargo test -p zorg-ls` starts `zorg-ls` over
stdio, builds temporary store indexes, and exercises diagnostics, navigation,
symbols, completion, rename, code actions, degraded store states, non-`.z`
documents, and single-root multi-folder initialization.

## Large-Corpus Baseline

`tools/generate_large_corpus.py` creates deterministic synthetic `.z` corpora
for store, query, watcher, and future FTS regression work. The generated files
exercise file and nested zettel IDs, tags, properties, todo markers, absolute
links, `#z/query` definitions, and repeated queryable body text.

Generate into an empty temporary directory:

```sh
tmp_parent="$(mktemp -d)"
python3 tools/generate_large_corpus.py \
  --output "$tmp_parent/corpus" \
  --files 1000 \
  --zettels-per-file 4 \
  --seed 10
```

The generator refuses the filesystem root, the home directory, the repository
root, the current working directory, paths inside this repository, and existing
non-empty directories. It is designed to be cheap to run under `/tmp` and
deterministic from `--files`, `--zettels-per-file`, and `--seed`.

Build the CLI once, then record a benchmark-like baseline. The numbers are
informational; exact wall-clock timing must not be used as a brittle CI
requirement.

```sh
cargo build -p zorg-cli
python3 tools/perf_large_corpus.py \
  --root "$tmp_parent/corpus" \
  --db "$tmp_parent/zorg-perf.sqlite3" \
  --zorg-bin target/debug/zorg
```

The baseline command runs `zorg check`, `zorg db reindex`, `zorg db status`,
mutates one generated `.z` file, runs an incremental `zorg db reindex`, checks
freshness again, and then runs a store-backed query. It prints line-oriented
counts and timings including `reindex_files_per_second`,
`incremental_changed_seconds`, `incremental_changed_files`, status freshness
fields, and `query_rows`. A fresh post-reindex status has `new_files: 0`,
`changed_files: 0`, and `deleted_files: 0`; the post-incremental status fields
must also remain zero.

For a quick validation run while developing this tooling, use a smaller corpus:

```sh
tmp_parent="$(mktemp -d)"
python3 tools/generate_large_corpus.py --output "$tmp_parent/corpus" --files 12 --zettels-per-file 2 --seed 3
cargo build -p zorg-cli
python3 tools/perf_large_corpus.py --root "$tmp_parent/corpus" --db "$tmp_parent/zorg-perf.sqlite3" --zorg-bin target/debug/zorg
```

Later watcher and FTS phases should reuse this generated corpus as a regression
target by pinning explicit inputs in logs or test setup, not by asserting exact
elapsed times.

## Epic 11 Handoff

The Epic 10 foundation pieces are intentionally narrow and stable for the live
workspace indexing work that follows:

- Config keys: `root`, `database_path`, `watcher_debounce_ms`,
  `watcher_log_path`, and `[named_roots]`. `StoreOptions` remains the canonical
  resolved path object for store APIs; watcher code should consume
  `ResolvedConfig` and pass `StoreOptions` into the store.
- Config precedence: CLI flags override environment, then root-local
  `.zorg/config.toml`, user config, and defaults. `ZORG_ROOT`,
  `ZORG_DATABASE_PATH`/`ZORG_DB`, `ZORG_WATCHER_DEBOUNCE_MS`, and
  `ZORG_WATCHER_LOG_PATH` are the environment surface.
- Migration convention: append new entries to the embedded migration list,
  keep each migration idempotent, update `SCHEMA_VERSION`, add a fixture-style
  migration test from the previous schema, and keep future-version refusal
  tests passing.
- Performance baseline: use `tools/generate_large_corpus.py` with explicit
  `--files`, `--zettels-per-file`, and `--seed` values, then run
  `tools/perf_large_corpus.py` against an explicit temp database. Treat batch
  throughput and changed-file incremental timings as regression signals in
  logs, not CI thresholds.
- Validation gate: run `tools/validate_cross_repo.sh` before handoff when
  local Rust, npm/Tree-sitter, and Neovim tools are available. The gate uses
  fixtures and temp roots rather than a developer's real `~/zorg`.

## Cross-Repo Validation

Run the full local MVP validation gate from the Rust repo root:

```sh
tools/validate_cross_repo.sh
```

The gate expects sibling `../zorg-treesitter` and `../zorg-nvim` checkouts by
default. Override those paths with `ZORG_TREESITTER_DIR` or `ZORG_NVIM_DIR` when
working from a different layout.

Use `tools/validate_cross_repo.sh --help` to print the supported invocation,
environment overrides, required tool summary, and fixture isolation policy.

The command checks required tools and sibling repo shape up front, runs the
Rust validation set above, then validates Tree-sitter
generation/tests/query compilation/shared-fixture parsing and Neovim headless
smoke/commands/helpers/LSP tests. Inside the gate, the full Rust workspace test
uses `cargo test --workspace -- --test-threads=1` so stdio LSP smoke tests do
not interfere with one another. The Tree-sitter shared-fixture step parses only
fixtures marked valid in `fixtures/manifest.json` and fails on recovered `ERROR`
or `MISSING` nodes. See `docs/cross_repo.md` for the exact step list and
troubleshooting notes.

## Release Dry Run

The release process lives in `docs/release.md`. Run the non-publishing dry run
from this repository root:

```sh
tools/release_dry_run.sh
```

The command requires clean Rust, Tree-sitter, and Neovim worktrees; verifies the
coordinated Rust and Tree-sitter version fields; runs the cross-repo validation
gate; confirms generated parser outputs exist; builds release binaries; creates
and checks a temporary host archive; and inspects package/archive outputs
without tagging, pushing, uploading, or publishing.

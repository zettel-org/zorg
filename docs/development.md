# Zorg Rust Development

This repository uses a Rust 2024 workspace. The local stable toolchain used for
the foundation phase supports Rust 2024, `rustfmt`, and `clippy`, so no edition
fallback is required.

## Workspace Layout

- `crates/zorg-core`: shared semantic types, diagnostics, and source spans.
- `crates/zorg-parse`: parser entry points and Tree-sitter integration boundary.
- `crates/zorg-store`: indexing and persistence boundary.
- `crates/zorg-query`: SWOG LIST query boundary.
- `crates/zorg-refactor`: shared structural refactor planning boundary.
- `crates/zorg-fix`: strict check and autofix boundary.
- `crates/zorg-capture`: capture/template boundary.
- `crates/zorg-cli`: `zorg` command-line binary.
- `crates/zorg-ls`: `zorg-ls` language-server binary.
- `crates/zorg-watch`: live workspace watcher contracts and service boundary.

The crates own the Rust MVP boundaries: parsing/model lowering, store indexing,
SWOG query evaluation, structural refactor planning, strict check/fix behavior,
capture/template expansion, the `zorg` CLI, `zorg-ls`, and the live indexing
watcher.

Import/export bridge behavior should live outside `zorg-parse`. Legacy parsing
is a conversion boundary only: `.zo`, `.zoq`, `.zot`, `.zoc`, and legacy inline
markers must not be added to normal parser, store, Tree-sitter, LSP, or editor
source acceptance. The bridge contract and fixtures are documented in
`docs/import_export.md` and `fixtures/import_export`.

## Refactor Planning Contract

`crates/zorg-refactor` is the shared Rust boundary for structural source
rewrites. User-facing CLI commands and editor integrations should build
`RefactorPlan` values there, serialize `RefactorPreview` for dry runs, and call
the shared application helpers for write mode.

The read-only lookup half of this boundary is `zorg_refactor::locate_zettel`,
which powers `zorg path @id` and the `zorg open @id` alias. These commands use
an existing current store snapshot, return the absolute source path,
root-relative path, zettel opening span, title, and kind, and never refresh or
write the index. Their JSON output is the stable editor jump contract documented
in `docs/query.md`.

The initial preview JSON envelope is:

```json
{
  "schema_version": 1,
  "plan": {
    "operation": "promote",
    "mode": "preview",
    "root": "/absolute/corpus/root",
    "target_id": "project/task",
    "warnings": [],
    "rejections": [],
    "files": [
      {
        "absolute_path": "/absolute/corpus/root/project.z",
        "root_relative_path": "project.z",
        "original_guard": {
          "content_hash": "0000000000000000",
          "mtime_unix_ms": 1770000000000,
          "byte_len": 128
        },
        "edits": [
          {
            "span": {
              "start_byte": 0,
              "end_byte": 8,
              "start_line": 1,
              "start_column": 1,
              "end_line": 1,
              "end_column": 9
            },
            "replacement": "@project/task",
            "label": "rewrite declaration"
          }
        ]
      }
    ]
  }
}
```

Safety invariants:

- Refactor commands must reparse source files from disk before planning writes;
  indexed rows are lookup hints, not rewrite authority.
- Every file edit must be in bounds, UTF-8 boundary aligned, and non-overlapping.
  Edits are sorted by byte span before application.
- Write mode must be explicit. Plans in `check` or `preview` mode are never
  applied by the shared write helper.
- Write mode refuses plans with `rejections` and refuses files whose content
  hash, modification time, or byte length no longer matches the captured
  `original_guard`.
- Multi-file application writes temporary files first, then renames them over
  the guarded sources. Later command-specific planners should validate syntax
  and semantic diagnostics before constructing a write plan.
- Rename-like link rewrites should use the shared declaration/reference
  replacement helpers. Relative references are rewritten only when the helper
  can prove the new target remains a direct child in the required context.

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
`-`, and `_`. A named root may only be declared once across the merged user and
root-local config files.

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
`fix`, JSON `capture`, explicit legacy import, Markdown export, and the stdio
`zorg-ls` server.

The fixture manifest command verifies that `fixtures/corpus/**/*.z`, the
machine-readable manifest, and the recorded Tree-sitter/Neovim fixture
derivations have not drifted. It also verifies the separate
`fixtures/import_export` bridge fixture inventory without treating those files
as canonical parser/store fixtures.

The named MVP E2E harness is `cargo test --workspace mvp_e2e`. It copies
canonical fixtures into temporary `~/zorg`-like roots, runs the CLI parse,
strict legacy rejection, explicit database reindex, inline and query-zettel
SWOG queries, `zorg path` JSON, refactor preview/write flows for promote, move,
and extract, post-refactor check/reindex/query validation, legacy import
plan/apply, Markdown export selectors, JSON capture, reindex,
captured-zettel query, fix, and `fix --check` loop, then starts `zorg-ls` over
stdio against an indexed temp root for diagnostics, navigation, references,
symbols, completion, and quick-fix actions. The tests set an isolated process
`HOME` and use explicit roots/databases so they do not depend on or mutate a
developer's real `~/zorg`.

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

## Live Watcher Contract

`crates/zorg-watch` isolates long-running live indexing from the synchronous
CLI and store crates. Its public boundary is deliberately small:

- `WatchOptions` carries the watched root, SQLite database path, debounce
  duration, and bounded-run controls for tests and future smoke checks.
- `WatchEvent` represents create, write, remove, rename, rescan-needed, and
  ignored path events.
- `WatchState` emits lifecycle records: `starting`, `ready`, `indexing`,
  `indexed`, `degraded`, `error`, `stopping`, and `stopped`.
- `WatchEventSink` is the callback surface that later CLI text/JSON output and
  service tests consume.

The implementation boundary for live indexing is:

- use `notify` as the filesystem event source;
- run the watcher service on Tokio because it is long-running and must support
  deterministic shutdown;
- treat filesystem notifications as hints and use `Store::reindex()` as the
  only database mutation path.

The running service owns one `Store` handle for its root/database pair. Event
bursts are coalesced into a single debounced reindex pass; if more accepted
events arrive while indexing is in progress, they queue through the watcher
channel and schedule one later debounced pass. Bounded runs stop accepting new
events when their limit is reached, finish the pending pass, then emit
`stopping` and `stopped`.

The watcher filters paths before any indexing work is scheduled. Accepted source
files are canonical `.z` files under the configured root. Directory events are
accepted only as traversal/container hints that can trigger a full incremental
reindex. The watcher ignores `.zorg`, the configured database path, legacy
non-canonical extensions (`.zo`, `.zoq`, `.zot`, `.zoc`), editor swap/backup/temp
names, hidden scratch names, and unrelated non-source files. Overflow or backend
rescan indications are debounced into a normal full incremental
`Store::reindex()` pass rather than a separate indexing path.

`zorg watch` is the explicit long-running CLI entry point for this service:

```sh
zorg watch --root ~/zorg --db ~/zorg/.zorg/zorg.sqlite3
zorg watch --root ~/zorg --format json
```

The command supports `--root PATH`, `--db PATH`, `--debounce MS`,
`--format text|json`, and the `--json` shortcut. Text output prints readable
`watch: starting`, `watch: ready`, `watch: indexing`, `watch: indexed`,
`watch: degraded`, `watch: error`, `watch: stopping`, and `watch: stopped`
lines. JSON output is line-delimited so editor clients can follow a running
process; each event has `schema_version`, `state`, `root`, and `database`
fields. `indexed` events include a `summary` object with
`discovered_files`, `indexed_files`, `unchanged_files`, `new_files`,
`changed_files`, `deleted_files`, `zettel_count`, `diagnostic_count`,
`effective_tag_count`, and `last_indexed_at_unix_ms`. `degraded` and `error`
events include `message`.

`zorg db reindex` remains the batch and CI path. The bounded watcher flags
`--exit-after-ready`, `--once`, and `--exit-after-events N` are intended for
smoke tests and health checks, not daily interactive use.

Future Neovim integrations should run at most one watcher process for a given
root/database pair. Duplicate watcher jobs can contend on SQLite writes without
making the index fresher. Separate roots or separate database paths may use
separate watcher jobs.

## Dashboard Foundation

`zorg dash` launches the terminal dashboard behind the default-on `dash` Cargo
feature in `zorg-cli`. Slim CLI builds can exclude terminal UI dependencies with:

```sh
cargo build -p zorg-cli --no-default-features
```

The dashboard opens the SQLite store read-only and degrades visibly when the
index is missing or incompatible. `zorg dash --once` renders one deterministic
Ratatui frame to stdout for smoke tests, and `zorg dash --exit-after MS` gives
bounded interactive runs a CI-safe shutdown path.

`zorg dash` supports `--root PATH`, `--db PATH`, `--panel
today|inbox|search|diagnostics|index`, `--query @id|SWOG`, `--once`,
`--exit-after MS`, `--no-alt-screen`, `--mouse`, `--no-mouse`, and
`--no-color`. Mouse capture is disabled by default until dashboard mouse
gestures exist; `--mouse` opts in and `--no-mouse` keeps capture disabled. Its
primary keys are `tab`/`backtab` for panels, arrows or `j`/`k` for rows, `/`
for Search editing, `r` for refresh, `R` for confirmed reindex, `enter` for
`$EDITOR`, `c` for capture through `zorg-capture`, `L` for the recent status
log, `?` for help, and `q`/`Esc` for exit or cancel. The dashboard does not
silently start `zorg watch`; first-run and empty-index states show the resolved
root/database and the `zorg db reindex --root ... --db ...` command users
should run when they need a fresh index. Dashboard write actions remain
explicit and route through existing crate boundaries.

`zorg-ls` refreshes the store snapshot on `textDocument/didSave` by using the
same store mutation boundary and then reloading graph data. The language server
does not host a separate filesystem watcher for the first live-indexing
integration. It reports readiness and degradation through LSP log messages and
republishes diagnostics after refresh; editor health UI should keep watcher
process status separate from LSP graph status.

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
smoke/commands/helpers/LSP tests. The Rust-local portion also checks import
plan/apply JSON, imported output check/reindex/query behavior, Markdown export
JSON, and strict rejection of legacy-looking canonical `.z` input. Inside the
gate, the full Rust workspace test uses
`cargo test --workspace -- --test-threads=1` so stdio LSP smoke tests do not
interfere with one another. The Tree-sitter shared-fixture step parses only
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

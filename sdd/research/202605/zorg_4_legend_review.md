---
research_date: 2026-05-04
bead_id: zorg-4
title: Zorg v1.1 non-dashboard legend review
source_legend: sdd/legends/202605/zorg_next_features_without_dashboard.md
source_research: sdd/research/202605/zorg_next_feature_recommendations.md
reviewed_repos:
  - ../zorg
  - ../zorg-nvim
verification:
  - sase bead show zorg-4
  - git log --oneline --grep='zorg-4' --all
  - cargo run -q -p zorg-cli -- --help
  - cargo run -q -p zorg-cli -- query --help
  - cargo run -q -p zorg-cli -- import --help
  - cargo run -q -p zorg-cli -- export --help
  - README.md
  - docs/query.md
  - docs/refactor.md
  - docs/import_export.md
  - docs/lsp.md
  - docs/cross_repo.md
  - docs/development.md
  - ../zorg-nvim/README.md
  - ../zorg-nvim/doc/zorg.txt
---

# Zorg v1.1 Non-Dashboard Legend Review

## Scope

This research reviews the work completed under the `zorg-4` legend bead, "Zorg next feature implementation plan without
dashboard." `sase bead show zorg-4` reports the parent legend as open, but all six child epics are closed:

1. `zorg-4.1`: foundations, configuration, migration, validation, and performance baselines.
2. `zorg-4.2`: live workspace indexing.
3. `zorg-4.3`: Search and Query v1.1.
4. `zorg-4.4`: zettel refactoring commands.
5. `zorg-4.5`: import and export bridges.
6. `zorg-4.6`: `zorg.nvim` v1.1 integration.

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory convention used by generated SDD artifacts.

This review intentionally excludes the later dashboard work. The current checkout has `zorg dash` and `zorg-5` dashboard
commits, but those are outside the `zorg-4` non-dashboard legend.

## One-Screen Summary

The `zorg-4` work turns the v1 MVP from a manually reindexed, LIST-only query system into a live, searchable,
refactorable, importable/exportable Zorg workspace with first-class Neovim wrappers.

The practical user loop is now:

```sh
zorg db reindex --root ~/zorg
zorg watch --root ~/zorg --format json
zorg query '#z/todo OR #z/query' --root ~/zorg
zorg query 'TABLE #z/todo' --root ~/zorg
zorg query 'count(#z/todo)' --root ~/zorg
zorg path @some/id --format json --root ~/zorg
zorg promote @some/nested --root ~/zorg
zorg move @some/id --to archive/some-id.z --root ~/zorg
zorg extract --file notes.z --range 12:1-14:1 --id @notes/extracted --root ~/zorg
zorg import legacy plan old-notes --root ~/zorg --dest imported --format json
zorg export markdown --query '#z/todo' --root ~/zorg --out /tmp/zorg-md
```

The implementation keeps the core product rule intact: Rust owns parsing, indexing, querying, refactoring, import, export,
and graph semantics. `zorg.nvim` shells out to `zorg`, starts/configures `zorg-ls`, renders JSON output, and manages
editor jobs without reimplementing Zorg semantics in Lua.

## What Changed By Epic

### Epic 10: Foundations

The foundation epic made later v1.1 work safer to land:

- Added store path and config resolution across CLI flags, environment variables, root-local `.zorg/config.toml`, user
  config, then defaults.
- Kept `StoreOptions` as the canonical resolved root/database object.
- Added configuration keys for root, database path, watcher debounce, watcher log path, and named roots.
- Added schema migration harness coverage for fresh databases, reopening, older schemas, idempotent migration, and
  unknown future schemas.
- Added or hardened `tools/validate_cross_repo.sh` so Rust, Tree-sitter, and Neovim contracts can be checked together.
- Added deterministic large-corpus tooling through `tools/generate_large_corpus.py` and
  `tools/perf_large_corpus.py`.
- Refreshed README and development docs around current command names, validation, and fixture policy.

Why it matters: users can rely on predictable root/database resolution, and maintainers now have migration and
performance harnesses before changing store-heavy behavior.

### Epic 11: Live Workspace Indexing

Live indexing added a Rust watcher layer and LSP freshness behavior:

- Added the `crates/zorg-watch` crate for long-running filesystem watching.
- Defined watcher options, path filtering, event/state types, JSON-friendly lifecycle events, debounce behavior, and
  shutdown behavior.
- Implemented debounced single-writer reindex jobs over `Store::reindex()`.
- Filtered out `.zorg`, configured database files, SQLite sidecars, hidden editor scratch files, swap/temp files,
  legacy non-canonical extensions, and non-source files.
- Added `zorg watch [--root PATH] [--db PATH] [--debounce MS] [--format text|json]`.
- Added bounded test/smoke flags: `--exit-after-ready`, `--once`, and `--exit-after-events N`.
- Added line-delimited JSON watcher events for editor jobs. Events include `schema_version`, `state`, `root`, and
  `database`; indexed events also include store summary counts.
- Updated `zorg-ls` to advertise save notifications and refresh its store snapshot after `textDocument/didSave`.

Why it matters: users no longer need to manually reindex after every edit when a watcher is running, and `zorg-ls` can
recover graph features after saves without a server restart.

### Epic 12: Search And Query v1.1

Query v1.1 made search structured and editor-friendly:

- Added `zorg query --json` and `zorg query --format json`.
- Added versioned JSON envelopes for `list`, `table`, and `aggregate` results.
- Bumped the SQLite store schema to include FTS-backed title/body/raw text search.
- Routed quoted text filters and `text:` filters through FTS for store-backed queries.
- Replaced flat query filters with a boolean expression model.
- Added explicit `OR`, unary negation, and parenthesized grouping with clear precedence.
- Added minimal `TABLE <query expression>` output with fixed columns: todo, id, file, title.
- Added `count(<query expression>)` as the only aggregate function.
- Preserved LIST as the default human renderer.

Why it matters: users can search text efficiently, run more realistic saved queries, feed structured query data to
editors, and get simple tabular or count views without introducing dashboard-specific behavior.

### Epic 13: Zettel Refactoring Commands

The refactor epic added safe structural editing commands:

- Added the `crates/zorg-refactor` crate as the shared Rust rewrite boundary.
- Defined `RefactorPlan`, file plans, edit spans, source guards, preview JSON, and preview/check/write modes.
- Added helpers for locating zettels, reparsing source before writes, validating non-overlapping UTF-8 byte edits, and
  applying guarded multi-file edits.
- Added `zorg path @id` and `zorg open @id` as read-only editor jump contracts with human and JSON output.
- Added `zorg promote @id` to promote nested zettels into file zettels.
- Added `zorg move @id --to PATH_OR_PARENT` for safe file/nested moves.
- Added `zorg extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id` and byte-range variants.
- Added LSP code actions for safe promote/extract contexts using shared Rust planners or CLI-style preview commands.
- Documented the safety model in `docs/refactor.md`.

Why it matters: users can grow notes organically and later restructure them without manually moving source blocks and
repairing graph metadata. The default is preview, and `--write` is required for source changes.

### Epic 14: Import And Export Bridges

The bridge epic added explicit adoption and exit paths while preserving the no-legacy normal-source rule:

- Added `docs/import_export.md` and bridge fixtures under `fixtures/import_export`.
- Extended `fixtures/manifest.json` and `tools/check_fixture_manifest.py` so import-only legacy fixtures are tracked
  without becoming accepted corpus syntax.
- Added the `crates/zorg-bridge` crate.
- Added deterministic legacy import planning for `.zo`, `.zoq`, and `.zot` inputs.
- Converted selected legacy markers, including `ID::`, `LID::`, and `tick::`, into canonical `.z` output or explicit
  diagnostics.
- Added bridge diagnostics for lossy conversion, unsupported input, collisions, invalid generated output, and IO errors.
- Added `zorg import legacy plan PATH...` as a read-only preview command.
- Added `zorg import legacy apply PATH...` as explicit write mode for canonical `.z` files only.
- Added Markdown export rendering for canonical `.z` zettels.
- Added `zorg export markdown` selectors for `--id`, `--subtree`, `--query`, and `--query-id`, with stdout or output
  directory modes and JSON metadata.

Why it matters: existing legacy users can inspect migration output before writing anything, and current users can export
subsets of a Zorg corpus to Markdown without making Markdown an import/source format.

### Epic 15: zorg.nvim v1.1 Integration

The Neovim epic wrapped the new Rust contracts without moving semantics into Lua:

- Added shared headless test helpers for fake `zorg` binaries, argv assertions, notification capture, scratch buffers,
  and schema-shaped watcher/query fixtures.
- Expanded configuration and health reporting for root/database settings, watcher settings, binary availability,
  `zorg watch`, and query JSON support.
- Added watcher lifecycle UI: `:ZorgWatchStart`, `:ZorgWatchStop`, and `:ZorgWatchStatus`.
- Made `:ZorgQuery` prefer `zorg query --json` and render navigable LIST/TABLE result buffers.
- Added `:ZorgPath` and `:ZorgOpen` location jumps from Rust JSON.
- Added `:ZorgPromote`, `:ZorgMove`, and range-based `:ZorgExtract` wrappers that render Rust preview JSON and require
  confirmation before rerunning with `--write`.
- Added `:ZorgImportPlan`, `:ZorgImportApply`, and Markdown export wrappers.
- Added optional mapping helpers while keeping global mappings disabled by default.
- Updated `README.md`, `doc/zorg.txt`, and cross-repo validation notes.

Why it matters: Neovim users get live index status, structured query buffers, safe refactor previews, import review
buffers, and export commands from the editor while Rust remains authoritative.

## Current Command Surface To Teach

### Freshness

Use batch reindex in scripts and CI:

```sh
zorg db reindex --root ~/zorg
zorg db status --root ~/zorg
```

Use live indexing while editing:

```sh
zorg watch --root ~/zorg --format json
```

`zorg-ls` refreshes on save. It does not host its own watcher.

### Query

```sh
zorg query '#z/todo -did:*' --root ~/zorg
zorg query '#z/todo OR #z/query' --root ~/zorg
zorg query 'TABLE #z/todo' --root ~/zorg
zorg query 'count(#z/todo)' --root ~/zorg
zorg query --id @system/queries/today --root ~/zorg
zorg query '#z/todo' --format json --root ~/zorg
```

LIST is for humans. JSON is for editors and scripts.

### Location And Refactor

```sh
zorg path @project/plan --root ~/zorg --format json
zorg open @project/plan --root ~/zorg --format json
zorg promote @project/plan --root ~/zorg
zorg promote @project/plan --write --root ~/zorg
zorg move @project/plan --to archive/project-plan.z --root ~/zorg
zorg extract --file notes.z --range 12:1-14:1 --id @notes/extracted --root ~/zorg
```

`promote`, `move`, and `extract` default to preview. Use `--write` only after reviewing the plan.

### Import And Export

```sh
zorg import legacy plan legacy-notes --root ~/zorg --dest imported --format json
zorg import legacy apply legacy-notes --root ~/zorg --dest imported --format json
zorg export markdown --id @project/plan --root ~/zorg --stdout
zorg export markdown --subtree @project --root ~/zorg --out /tmp/zorg-md
zorg export markdown --query '#z/todo' --root ~/zorg --out /tmp/zorg-md --format json
```

Legacy syntax is import input only. Markdown is export output only. Neither becomes accepted normal Zorg source syntax.

### Neovim

Important commands:

- `:ZorgIndex`
- `:ZorgStatus`
- `:ZorgWatchStart`
- `:ZorgWatchStop`
- `:ZorgWatchStatus`
- `:ZorgQuery`
- `:ZorgPath`
- `:ZorgOpen`
- `:ZorgPromote`
- `:ZorgMove`
- `:ZorgExtract`
- `:ZorgImportPlan`
- `:ZorgImportApply`
- `:ZorgExportMarkdown`
- `:ZorgExportCurrent`
- `:ZorgExportSubtree`
- `:ZorgExportQuery`

No global mappings are installed by default. Optional mappings can be enabled with:

```lua
require("zorg").setup({ mappings = { enabled = true } })
```

## Architecture Map For Contributors

New contributors should start with these boundaries:

| Area | Primary files |
| --- | --- |
| Config and store | `crates/zorg-store/src/lib.rs`, `docs/development.md` |
| Live indexing | `crates/zorg-watch/src/lib.rs`, `crates/zorg-cli/src/main.rs`, `docs/cross_repo.md` |
| Query v1.1 | `crates/zorg-query/src/lib.rs`, `crates/zorg-store/src/lib.rs`, `docs/query.md` |
| Refactors | `crates/zorg-refactor/src/lib.rs`, `crates/zorg-refactor/src/promote.rs`, `crates/zorg-refactor/src/move_zettel.rs`, `crates/zorg-refactor/src/extract.rs`, `docs/refactor.md` |
| Import/export | `crates/zorg-bridge/src/lib.rs`, `crates/zorg-bridge/src/markdown.rs`, `docs/import_export.md` |
| CLI | `crates/zorg-cli/src/main.rs`, `crates/zorg-cli/tests/smoke.rs`, `crates/zorg-cli/tests/mvp_e2e.rs` |
| LSP refresh/actions | `crates/zorg-ls/src/state.rs`, `crates/zorg-ls/src/actions.rs`, `docs/lsp.md` |
| Neovim wrappers | `../zorg-nvim/lua/zorg/commands.lua`, `../zorg-nvim/lua/zorg/watcher.lua`, `../zorg-nvim/lua/zorg/refactor.lua`, `../zorg-nvim/lua/zorg/health.lua` |

## Safety And Product Rules That Survived The Work

- `.z` remains the only normal source format.
- Legacy `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`, `LID::`, and `tick::` are not accepted by normal parse/check/store/LSP
  paths.
- Import and export are explicit bridge commands, not broad compatibility modes.
- Rust remains the source of truth for semantic behavior.
- Neovim wrappers use JSON contracts and do not edit `.z` source directly for structural refactors.
- Refactor write mode is explicit and guarded by source hashes, byte lengths, reparsing, UTF-8 span validation, and
  collision checks.
- Live indexing is an explicit long-running process. Batch workflows still use `zorg db reindex`.
- Query JSON envelopes are schema-versioned so editor clients do not scrape LIST output.

## Validation Surface

The docs now point maintainers at these high-value checks:

```sh
python3 tools/check_fixture_manifest.py
cargo fmt --check
cargo test --workspace
cargo test --workspace mvp_e2e
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
tools/validate_cross_repo.sh
```

For Neovim, the Epic 15 notes record headless tests around commands, config, health, watcher, query buffers, refactors,
import/export, helpers, LSP, and cross-repo validation. Local notes in the bead ledger mention that `stylua` and
`luacheck` were unavailable in at least one implementation environment, so those remain optional local checks unless
installed.

## Fast Onboarding Narrative

Explain Zorg v1.1 to a new user this way:

1. Zorg source is still just `.z` files under a root, usually `~/zorg`.
2. Run `zorg db reindex` once, then keep `zorg watch` running while editing.
3. Use `zorg query` for retrieval. LIST is readable, JSON is for tools, TABLE and `count()` cover quick reports.
4. Use `zorg path` or `zorg open` to jump from an ID to source.
5. Use `promote`, `move`, and `extract` when your note structure changes. Preview first; write only after review.
6. Use `import legacy plan/apply` to migrate old Zorg files deliberately.
7. Use `export markdown` to publish or hand off selected notes.
8. In Neovim, the same operations are available as `:Zorg*` commands backed by the Rust CLI and LSP.

That is the shortest route from the v1 mental model to the new v1.1 functionality.

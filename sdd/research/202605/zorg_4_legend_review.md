---
research_date: 2026-05-04
bead_id: zorg-4
title: Zorg v1.1 non-dashboard legend review
source_legend: sdd/legends/202605/zorg_next_features_without_dashboard.md
source_research: sdd/research/202605/zorg_next_feature_recommendations.md
reviewed_repos:
  - ../zorg_100  # current implementation tree (Epics 10-15 plus dashboard work)
  - ../zorg      # historical clone (Epics 10-12 only at time of review)
  - ../zorg-nvim # Neovim plugin (Epic 15)
verification:
  - sase bead show zorg-4
  - grep '"id":"zorg-4' sdd/beads/issues.jsonl
  - cd ../zorg_100 && git log --oneline | grep 'zorg-4'
  - cd ../zorg_100 && cargo run -q -p zorg-cli -- --help
  - ../zorg_100/README.md
  - ../zorg_100/docs/query.md
  - ../zorg_100/docs/refactor.md
  - ../zorg_100/docs/import_export.md
  - ../zorg_100/docs/development.md
  - ../zorg_100/docs/cross_repo.md
  - ../zorg_100/crates/zorg-watch/src/lib.rs
  - ../zorg_100/crates/zorg-store/src/lib.rs (SCHEMA_VERSION = 2)
  - ../zorg-nvim/README.md
  - ../zorg-nvim/lua/zorg/commands.lua
---

# Zorg v1.1 Non-Dashboard Legend Review

## Scope And Bead State

This research reviews the `zorg-4` legend bead, "Zorg next feature
implementation plan without dashboard." `sase bead show zorg-4` reports the
parent legend as **OPEN** while all six child epics are **CLOSED**:

1. `zorg-4.1` Epic 10: foundations, configuration, migration, validation,
   performance baselines.
2. `zorg-4.2` Epic 11: live workspace indexing.
3. `zorg-4.3` Epic 12: Search and Query v1.1.
4. `zorg-4.4` Epic 13: zettel refactoring commands.
5. `zorg-4.5` Epic 14: import and export bridges.
6. `zorg-4.6` Epic 15: `zorg.nvim` v1.1 integration.

The legend bead is the only open node in the tree; closing it should be a
near-term reconciliation. The earlier `zorg-1` (v1 MVP) legend has the same
"open legend with closed epics" pattern, so a single reconciliation pass can
close both.

This review intentionally excludes Epic 16 (`zorg dash` terminal dashboard,
tracked under `zorg-5`). The dashboard sits on top of v1.1 contracts but is not
part of the `zorg-4` non-dashboard legend.

There is no `sdd/research/README.md` in this checkout. The file is placed
under `sdd/research/202605/` to match the month-directory convention used by
the SDD `prompts/`, `tales/`, `epics/`, and `legends/` trees.

## Where The Code Actually Lives

Sibling repositories under `../`:

| Path | What is in it now |
| --- | --- |
| `../zorg_100` | Current implementation tree. Contains Epics 10–15 commits plus subsequent dashboard work. Treat this as the canonical Rust source for v1.1. |
| `../zorg` | Older clone tracking origin/master. At review time it includes Epic 10 (foundations), 11 (watcher/`zorg watch`), and 12 (query JSON, FTS, OR, TABLE), but not 13 (refactor) or 14 (import/export). |
| `../zorg_102`, `../zorg_103` | Additional development worktrees that share the same git object database. |
| `../zorg-nvim` | Neovim plugin. Contains all Epic 15 commits (watcher UI, query JSON buffers, refactor wrappers, import/export wrappers, helper polish). |

Bead-recorded commits map to `../zorg_100`. Examples: `zorg-4.4.1` → `0881fbd`,
`zorg-4.5.1` → `dd65c49`, `zorg-4.6.7` → `5474e4c` (the last is in
`../zorg-nvim`).

When reading code referenced below as `crates/zorg-...` or `docs/...`, look in
`../zorg_100`. When reading Lua references, look in `../zorg-nvim`.

## Glossary

- **SWOG**: the inline query language used by `zorg query`. Whitespace-AND with
  explicit OR/parentheses, negation, tag, link, file, todo, text, property, and
  modified-age filters.
- **FTS**: SQLite FTS5 virtual table backing title/body/raw text search.
  Populated during reindex.
- **LSP**: `zorg-ls` over stdio. Diagnostics, completion, navigation, rename,
  code actions, refactor previews.
- **Refactor preview envelope**: schema-versioned JSON describing planned
  source edits before any write.
- **Watcher**: `zorg watch` long-running service that debounces filesystem
  events into incremental reindex passes.
- **Bridge**: explicit one-shot conversion command (`zorg import legacy`,
  `zorg export markdown`). Bridge formats are not normal v1 source syntax.

## One-Screen Summary

`zorg-4` turns the v1 MVP from a manually-reindexed, LIST-only query system
into a live, searchable, refactorable, importable/exportable Zorg workspace
with first-class Neovim wrappers, while keeping `.z` as the only normal source
format and Rust as the only place semantics live.

The practical loop:

```sh
zorg db reindex --root ~/zorg                                      # one-time / CI
zorg watch --root ~/zorg --format json                             # live indexing
zorg query '#z/todo OR #z/query' --root ~/zorg                     # boolean OR
zorg query 'TABLE #z/todo' --root ~/zorg                           # TABLE form
zorg query 'count(#z/todo)' --root ~/zorg                          # aggregate
zorg query '#z/todo' --format json --root ~/zorg                   # editor-ready
zorg path @some/id --format json --root ~/zorg                     # jump JSON
zorg promote @some/nested --root ~/zorg                            # preview
zorg promote @some/nested --write --root ~/zorg                    # apply
zorg move @some/id --to archive/some-id.z --root ~/zorg
zorg extract --file notes.z --range 12:1-14:1 --id @notes/extracted --root ~/zorg
zorg import legacy plan legacy-notes --root ~/zorg --dest imported --format json
zorg import legacy apply legacy-notes --root ~/zorg --dest imported
zorg export markdown --query '#z/todo' --root ~/zorg --out /tmp/zorg-md
```

## What's New For An Existing v1 User

If you already use v1, the v1.1 deltas worth memorizing:

- `zorg watch` exists and replaces "remember to run `zorg db reindex`" during
  active editing. The default debounce is **250 ms**.
- The SQLite schema is now **version 2** (was 1). FTS is added during the
  v1→v2 migration; reindex repopulates it. Older databases migrate forward
  idempotently.
- `zorg query` accepts `--json` / `--format json`, `OR` / `|` / `||`,
  parenthesized groups, `TABLE <expr>`, and `count(<expr>)`. LIST stays the
  default human renderer.
- New CLI verbs: `path`, `open`, `promote`, `move`, `extract`, `import legacy
  plan|apply`, `export markdown`. All structural writes default to preview;
  `--write` is required to touch source.
- `zorg-ls` refreshes its store snapshot on `textDocument/didSave` and exposes
  `refactor.rewrite` (promote) and `refactor.extract` code actions. No server
  restart needed after saves.
- `zorg.nvim` adds `:ZorgWatchStart|Stop|Status`, `:ZorgPath`, `:ZorgOpen`,
  `:ZorgPromote`, `:ZorgMove`, `:ZorgExtract`, `:ZorgImportPlan|Apply`,
  `:ZorgExportMarkdown|Current|Subtree|Query`. Global mappings remain off by
  default behind `mappings.enabled`.
- Configuration order is now documented: CLI flags > environment >
  root-local `.zorg/config.toml` > user config > defaults. Named roots can be
  configured (`named_roots = { ... }`) for tooling that wants short aliases.

What did **not** change:

- `.z` is still the only accepted normal source extension.
- Default corpus root is still `~/zorg`.
- Legacy `.zo`/`.zoq`/`.zot`/`.zoc`/`ID::`/`LID::`/`tick::` are still rejected
  by `zorg parse`, `zorg check`, the store, and `zorg-ls`. The bridge accepts
  them only as explicit import inputs.
- LIST is still the default human renderer with the same column layout.

## What Changed By Epic

### Epic 10 (zorg-4.1): Foundations

Made later work safer to land:

- Documented config resolution order across CLI flags, environment, root-local
  `.zorg/config.toml`, user config, then defaults; `StoreOptions` remains the
  canonical resolved root/database object.
- Added config keys for root, database path, watcher debounce, watcher log
  path, and **named roots** (`BTreeMap<String, PathBuf>` merged from user and
  root config; duplicate aliases are rejected).
- Added a schema migration harness with coverage for fresh databases,
  reopening, v1→current migration, idempotent migration, and refusal of
  unknown future schema versions.
- Added or hardened `tools/validate_cross_repo.sh` so Rust, Tree-sitter, and
  Neovim contracts can be checked together (the `tools/` directory now ships
  the script that v1 docs referenced but did not include).
- Added deterministic large-corpus tooling via `tools/generate_large_corpus.py`
  and `tools/perf_large_corpus.py`, plus a documented baseline command.
- Refreshed README and `docs/development.md` for current command names,
  fixture policy, and validation flow.

Why it matters: predictable root/database resolution; a real migration
harness before the FTS schema bump; a real cross-repo gate before the v1.1
features ship.

### Epic 11 (zorg-4.2): Live Workspace Indexing

Added a Rust watcher layer and LSP freshness behavior:

- New `crates/zorg-watch` crate. Public types include `WatchOptions`,
  `WatchEvent`, `WatchState { root, database_path, kind }`,
  `WatchStateKind::{Starting, Ready, Indexing, Indexed{summary}, Degraded,
  Error, Stopping, Stopped}`, `WatchIndexSummary` (mirrors `ReindexSummary`
  fields including `discovered_files`, `indexed_files`, `unchanged_files`,
  `new_files`, `changed_files`, `deleted_files`, `zettel_count`,
  `diagnostic_count`, `effective_tag_count`, `last_indexed_at_unix_ms`), and
  `WatchEventSink`.
- `tokio` runtime with `notify::recommended_watcher` recursive over the
  corpus root. Single-writer reindex via `Store::reindex()`; events are
  coalesced through a `DebounceScheduler` (default 250 ms).
- Path filtering ignores `.zorg/`, the configured database file and SQLite
  sidecars (`-journal`, `-wal`, `-shm`), hidden editor scratch and swap files,
  legacy non-`.z` extensions, and any non-source file.
- `zorg watch [--root PATH] [--db PATH] [--debounce MS] [--format text|json]`.
  Bounded test/smoke flags: `--exit-after-ready`, `--once`,
  `--exit-after-events N`.
- Line-delimited JSON events use the `WatchState` shape. Sample events:

  ```json
  {"root":"/abs/root","database_path":"/abs/.zorg/zorg.sqlite3","state":"ready"}
  {"root":"/abs/root","database_path":"/abs/.zorg/zorg.sqlite3","state":"indexing"}
  {"root":"/abs/root","database_path":"/abs/.zorg/zorg.sqlite3","state":"indexed","summary":{"indexed_files":8,"zettel_count":25,"...":"..."}}
  ```

- `zorg-ls` advertises save notifications and refreshes its store snapshot
  after `textDocument/didSave`. It does **not** host its own watcher.

Why it matters: editor sessions stay current without per-edit reindex calls;
graph features in `zorg-ls` recover after saves without a server restart.

### Epic 12 (zorg-4.3): Search And Query v1.1

Made search structured and editor-friendly:

- Added `zorg query --json` / `--format json` with versioned envelopes.
- Bumped store schema **1 → 2** to add an FTS5 virtual table for
  `title|body|raw`. Reindex repopulates it; rows stay in sync on
  create/update/delete.
- Routed quoted phrases and `text:` filters through FTS for store-backed
  queries. Non-text filters and ordering remain deterministic.
- Replaced flat AND-only filters with a boolean expression model. Operator
  precedence (tightest to loosest): **parens → unary `-` → implicit AND
  whitespace → explicit OR**. `OR`, `|`, and `||` are equivalent.
- `TABLE <expr>` renders fixed columns `todo, id, file, title`. Custom
  columns and custom functions are explicitly rejected.
- `count(<expr>)` is the only aggregate. `sum/avg/min/max` produce
  unsupported-feature errors instead of silent omission.
- LIST remains the default human renderer with the same row format
  `<todo> <identity>  <path>  <title>` and stable column padding.

JSON envelopes (compact):

```jsonc
// list
{"schema_version":1,"kind":"list","query_source":"inline","query":"#z/todo",
 "query_zettel":null,"rows":[{"store_row_id":12,"canonical_id":"project/plan",
 "path":"nested.z","title":"...","todo_marker":"[ ]","source_order":1,
 "span":{...},"tags":["z/todo"],"properties":[{"key":"area","value":"work/zorg"}]}],
 "diagnostics":[]}

// table
{"schema_version":1,"kind":"table","query":"TABLE #z/todo",
 "columns":[{"key":"todo","label":"Todo"},{"key":"id","label":"ID"},
 {"key":"file","label":"File"},{"key":"title","label":"Title"}],
 "rows":[{"todo":"[ ]","id":"@project/plan","file":"nested.z","title":"..."}],
 "diagnostics":[]}

// aggregate
{"schema_version":1,"kind":"aggregate","query":"count(#z/todo)",
 "values":{"count":3},"diagnostics":[]}
```

Why it matters: real search at scale via FTS, real boolean queries for saved
`#z/query` zettel, structured output for editors, and quick reports without
introducing dashboard-specific renderers.

### Epic 13 (zorg-4.4): Zettel Refactoring Commands

Added safe structural editing:

- New `crates/zorg-refactor` crate as the shared rewrite boundary.
  `RefactorPlan`, file plans, edit spans, source guards (content hash, mtime,
  byte length), preview JSON, and preview/check/write modes.
- Helpers locate zettels via the index, reread and reparse source before
  planning, validate non-overlapping UTF-8-aligned byte edits, and apply
  guarded multi-file edits via temp-file-then-rename.
- `zorg path @id` and `zorg open @id`: read-only editor jump contract.
  Stable JSON includes `canonical_id`, `absolute_path`,
  `root_relative_path`, `source_span`, `title`, and `kind` (`file` or
  `nested`).
- `zorg promote @id [--to PATH] [--check|--write]`: nested → file zettel,
  preserving canonical IDs. Default destination derives from the canonical
  ID (`@foo/bar` → `foo/bar.z`).
- `zorg move @id --to PATH_OR_PARENT [--check|--write]`: file/nested moves
  preserving IDs. Path destinations move file zettels or convert nested →
  file using the same conservative source rules as promote. Parent
  destinations (`@parent`) move nested zettels under another zettel.
- `zorg extract --file PATH --range L:C-L:C --id @new/id [--byte-range
  S..E] [--to PATH] [--replace-with-link] [--write]`: extracts a body range
  to a new file and replaces it with an absolute `#new/id` link.
  Paragraph-or-fenced selections by default; sub-paragraph ranges require
  `--replace-with-link`.
- LSP code actions: `refactor.rewrite` returns a `WorkspaceEdit` with a
  destination `createFile` op for safe promote contexts; `refactor.extract`
  returns a `zorg.extract.preview` command with a placeholder `@new/id` for
  the editor to resolve.
- `docs/refactor.md` documents the safety model and exit codes:
  `0` success/preview, `1` refused/failed plan, `2` invalid CLI usage.

Refactor preview envelope (compact):

```jsonc
{"schema_version":1,"plan":{"operation":"promote","mode":"preview",
 "root":"/abs/root","target_id":"project/plan","warnings":[],"rejections":[],
 "files":[{"absolute_path":"...","root_relative_path":"project.z",
   "original_guard":{"content_hash":"...","mtime_unix_ms":...,"byte_len":128},
   "edits":[{"span":{"start_byte":0,"end_byte":8,"start_line":1,
     "start_column":1,"end_line":1,"end_column":9},
     "replacement":"@project/plan","label":"rewrite declaration"}]}]}}
```

Why it matters: notes can grow organically and be restructured later without
hand-editing source blocks or repairing graph metadata. Preview-by-default
plus reparse-then-write makes structural edits auditable.

### Epic 14 (zorg-4.5): Import And Export Bridges

Added explicit adoption and exit paths while preserving the no-legacy rule:

- `docs/import_export.md` plus bridge fixtures under
  `fixtures/import_export/`. `fixtures/manifest.json` and
  `tools/check_fixture_manifest.py` track import-only legacy fixtures
  separately so legacy syntax never enters `fixtures/corpus/`.
- New `crates/zorg-bridge` crate. Deterministic legacy import planning for
  `.zo`, `.zoq`, `.zot`. Generated `.zoc` cache files are explicitly rejected.
- Inline marker conversions: `ID::` → `@`, `LID::` → `^`, `tick::YYYY-MM-DD` →
  `modified::YYYY-MM-DD` (lossy: tick history collapses). Legacy `[[some/id]]`
  → `#some/id` when the target is a valid canonical ID; otherwise preserved
  as body text with a diagnostic.
- `zorg import legacy plan PATH... [--root R] [--dest D] [--json]`: read-only
  preview. Exit codes: `0` plan ok, `1` fatal diagnostics, `2` CLI usage.
- `zorg import legacy apply PATH...`: explicit write mode. Writes only `.z`,
  refuses overwrites (no force/replace in this version), refuses fatal plans.
  Apply JSON adds `write_results` per-file outcomes.
- `zorg export markdown` selectors: `--id`, `--subtree`, `--query '<swog>'`,
  `--query-id @id`. Output modes: stdout (default; multi-item separated) or
  `--out DIR` (one `.md` per zettel; existing files not overwritten).
- Markdown mapping: IDs render as headings and root front-matter `id`; tags
  and properties render as front matter; todos render as bracket markers in
  headings/list items; Zorg links render as `[#target](zorg:#target)` when
  the target is in the export set, otherwise preserved as text with a lossy
  diagnostic.

Bridge JSON envelope (compact):

```jsonc
{"schema_version":1,"command":"import legacy plan","mode":"plan",
 "inputs":[{"path":"...zo","kind":"legacy_note"}],
 "outputs":[{"input_path":"...","root_relative_path":"legacy/project.z",
   "canonical_id":"legacy/project","status":"planned",
   "lossiness":["tick_history_collapsed"]}],
 "diagnostics":[{"severity":"warning","kind":"lossy",
   "code":"legacy.tick_history_collapsed","path":"...","line":5,"message":"..."}],
 "collisions":[],
 "summary":{"planned":1,"lossy":1,"unsupported":0,"fatal":0}}
```

Diagnostic kinds: `lossy`, `unsupported`, `collision`, `invalid_output`,
`io`. Severities: `info`, `warning`, `error`.

Why it matters: existing legacy users can audit migration output before
writing anything; current users can publish or hand off subsets without
making Markdown a normal source format.

### Epic 15 (zorg-4.6): zorg.nvim v1.1 Integration

Wrapped the new Rust contracts without moving semantics into Lua:

- Shared headless test helpers (`tests/testlib.lua`) for fake `zorg`
  binaries, argv assertions, notification capture, scratch buffers, and
  schema-shaped watcher/query JSON fixtures.
- Expanded config and `:checkhealth zorg`: resolved root and database,
  watcher availability and settings, `zorg watch` and query JSON support,
  CLI version, parser/query status. Health degrades clearly when
  binaries/parser are missing.
- Watcher lifecycle UI: `:ZorgWatchStart`, `:ZorgWatchStop`,
  `:ZorgWatchStatus`. One job per root. Optional auto-start behind
  `watcher.autostart` (default `false`).
- Query JSON result buffers: `:ZorgQuery` prefers `zorg query --json` and
  renders navigable LIST/TABLE buffers backed by JSON rows, not LIST scrape.
- Location jumps: `:ZorgPath` and `:ZorgOpen` consume the `path` JSON.
- Refactor wrappers: `:ZorgPromote`, `:ZorgMove`, range-aware
  `:ZorgExtract`. They render Rust preview JSON and require explicit
  confirmation before rerunning with `--write`.
- Bridge wrappers: `:ZorgImportPlan`, `:ZorgImportApply`,
  `:ZorgExportMarkdown`, `:ZorgExportCurrent`, `:ZorgExportSubtree`,
  `:ZorgExportQuery`.
- Optional `<leader>z*` mapping helpers; global mappings stay disabled
  unless `mappings.enabled = true`.
- `README.md`, `doc/zorg.txt`, and `docs/cross_repo.md` (in `../zorg`)
  document the cross-repo validation gate.

Why it matters: editor users get live index status, structured query
buffers, safe refactor previews, import review buffers, and export
commands without Lua reimplementing Zorg semantics.

## Current Command Surface

`zorg --help` (from `../zorg_100`):

```
parse FILE
check [--root PATH] FILE...
db status   [--root PATH] [--db PATH]
db reindex  [--root PATH] [--db PATH]
watch       [--root PATH] [--db PATH] [--debounce MS] [--format text|json]
dash        [--root PATH] [--db PATH] [--panel ...]   # Epic 16, outside zorg-4
index       (deferred-alias notice → db reindex)
query '<swog>'                  [--root PATH] [--db PATH] [--json|--format json]
query --id @some/query          [--root PATH] [--db PATH] [--json|--format json]
path  @id                       [--root PATH] [--db PATH] [--json|--format json]
open  @id                       (alias of path)
promote @id [--to PATH] [--check|--write]
move    @id --to PATH_OR_PARENT [--check|--write]
extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id
        [--byte-range S..E] [--to PATH] [--replace-with-link] [--check|--write]
import legacy plan  PATH... [--root R] [--dest D] [--json]
import legacy apply PATH... [--root R] [--dest D] [--json]
export markdown (--id @id|--subtree @id|--query '<swog>'|--query-id @id)
        [--root R] [--db DB] [--out DIR|--stdout] [--json]
fix      [--check] [--json] [--root PATH] FILE...
capture  [--template @id|TITLE] [--json] [--title T] [--dest P] [--root PATH]
```

Common flags: every store-aware command takes `--root PATH` (default `~/zorg`)
and `--db PATH` (default `<root>/.zorg/zorg.sqlite3`). `--json` and `--format
json` are equivalent for commands that support it.

## Common Pitfalls / FAQ

- **"Why does my query find nothing?"** The store can be stale or missing.
  Run `zorg db reindex` once, or start `zorg watch`. `zorg-ls` does not own a
  watcher; without `zorg watch`, save events still trigger an LSP-side store
  refresh, but the on-disk database itself only updates when `zorg db
  reindex` or `zorg watch` runs.
- **"FTS rows are out of sync."** They repopulate on reindex. After a v1→v2
  migration, run `zorg db reindex` to backfill the new FTS table.
- **"Why does `zorg promote` say it can't write?"** Default mode is preview.
  Use `--write` after reviewing the JSON preview. If the source has changed
  since the index was built, the source guard refuses; rerun `zorg db
  reindex` and try again.
- **"Why is `extract` rejecting my range?"** Default extract requires a
  paragraph-like body block. For sub-paragraph spans, pass
  `--replace-with-link`.
- **"Where do I put a saved query?"** Any `.z` zettel tagged `#z/query`.
  Use `query::short` or a fenced `swog` block. Run via `zorg query --id
  @path/to/query`.
- **"What are the OR aliases?"** `OR`, `|`, and `||` are equivalent.
  `()` for grouping. `-` for unary negation.
- **"Can I import legacy `.zoc` cache files?"** No. Only `.zo`, `.zoq`,
  `.zot` are accepted by the bridge. `.zoc` is explicitly rejected.

## Architecture Map For Contributors

Start with these boundaries (paths relative to `../zorg_100`):

| Area | Primary files |
| --- | --- |
| Config and store | `crates/zorg-store/src/lib.rs` (resolution order, named roots, `SCHEMA_VERSION = 2`, FTS migration), `docs/development.md` |
| Live indexing | `crates/zorg-watch/src/lib.rs` (states, debounce scheduler, notify loop), `crates/zorg-cli/src/main.rs` (`watch` subcommand) |
| Query v1.1 | `crates/zorg-query/src/lib.rs` (boolean AST, FTS routing, TABLE/count), `crates/zorg-store/src/lib.rs` (FTS table), `docs/query.md` |
| Refactors | `crates/zorg-refactor/src/{lib.rs,promote.rs,move_zettel.rs,extract.rs}`, `docs/refactor.md` |
| Import/export | `crates/zorg-bridge/src/{lib.rs,markdown.rs}`, `fixtures/import_export/`, `docs/import_export.md` |
| CLI | `crates/zorg-cli/src/main.rs`, `crates/zorg-cli/tests/{smoke.rs,mvp_e2e.rs}` |
| LSP refresh and actions | `crates/zorg-ls/src/{state.rs,actions.rs}`, `docs/lsp.md` |
| Neovim wrappers | `../zorg-nvim/lua/zorg/{commands.lua,watcher.lua,refactor.lua,health.lua,config.lua}` |

## Safety And Product Rules That Survived The Work

- `.z` is the only normal source format.
- Legacy `.zo`/`.zoq`/`.zot`/`.zoc`/`ID::`/`LID::`/`tick::` are not accepted
  by parser, store, query, or LSP.
- Import and export are explicit one-shot bridge commands; never broad
  compatibility modes; never write hidden compat metadata.
- Rust is the source of truth for parse, query, refactor, import, export, and
  graph semantics. Neovim shells out for everything semantic.
- Refactor write mode is explicit and guarded by content hash, byte length,
  reparse, UTF-8 span validation, non-overlap, and post-write reparse.
- Live indexing is an explicit long-running process. CI and scripts use
  `zorg db reindex`.
- All editor-facing JSON envelopes are schema-versioned so editors do not
  scrape LIST/text output.

## Explicit Non-Scope (As Of zorg-4)

The legend documented these as out of scope and they remain out of scope:

- `zorg dash`, terminal dashboards, dashboard panels/actions, plugin-only
  dashboard views. (`zorg dash` exists in `../zorg_100` because Epic 16
  landed afterward, but it is not part of `zorg-4`.)
- Legacy syntax as normal parser input.
- Editor clients other than Neovim.
- A general plugin system.
- Heavyweight cross-root union queries. Lightweight named roots are
  configured but query evaluation is still single-root.

## Validation Surface

Rust checks (run in `../zorg_100`):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --workspace mvp_e2e
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
python3 tools/check_fixture_manifest.py
tools/validate_cross_repo.sh
```

Tree-sitter checks when touched (in `../zorg-treesitter`):

```sh
npm run generate
npm test
```

Neovim checks (in `../zorg-nvim`):

```sh
nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"
# plus headless tests for commands, helpers, lsp, watcher, query, refactor,
# import/export, contracts. stylua/luacheck are optional and may be absent.
```

Phase notes in the bead ledger record `stylua` and `luacheck` were not
installed during Epic 15 work, so those remain optional local checks.

## Recommended Onboarding Path

For a brand-new user, teach v1.1 in this order:

1. `.z` is the only normal source. Default root is `~/zorg`.
2. Run `zorg db reindex` once, then leave `zorg watch` running while editing.
3. Use `zorg query` for retrieval. Start with LIST. Switch to JSON, TABLE,
   `count()` only when the use case asks for them.
4. Use `zorg path @id` (or `zorg open @id`) to jump from an ID to source.
5. When notes outgrow their original shape, use `promote`, `move`, and
   `extract`. Always preview first; `--write` only after reviewing the plan.
6. Migrate legacy notes deliberately with `zorg import legacy plan ...`,
   then `... apply` once the plan looks right.
7. Hand off subsets via `zorg export markdown`.
8. In Neovim, the same surfaces appear as `:Zorg*` commands backed by the
   Rust CLI and `zorg-ls`. Enable optional mappings with
   `require("zorg").setup({ mappings = { enabled = true } })`.
9. Architecture comes after the loop is visible: Tree-sitter parses → Rust
   models, indexes (SQLite + FTS), queries, refactors, imports, exports →
   `zorg-ls` edits safely → Neovim wraps all of it.

That path moves a user from "plain `.z` files" to "live, queryable,
refactorable, importable/exportable knowledge graph" without ever exposing
Lua-side semantic logic — which is the core reason `zorg-4` was worth doing.

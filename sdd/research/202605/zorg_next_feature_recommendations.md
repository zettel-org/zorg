---
research_date: 2026-05-03
last_revised: 2026-05-02
bead_id: zorg-1
title: Zorg next feature recommendations after v1 MVP
source_research:
  - ../zorg/research/202605/v1_mvp_curation.md
  - sdd/research/zorg_1_legend_review.md
  - sdd/legends/202605/zorg_v1_mvp.md
verification:
  - ../zorg/crates/zorg-cli/src/main.rs
  - ../zorg/crates/zorg-store/src/lib.rs
  - ../zorg/crates/zorg-query/src/lib.rs
  - ../zorg/crates/zorg-capture/src/lib.rs
  - ../zorg/crates/zorg-cli/tests/smoke.rs
  - ../zorg/crates/zorg-ls/tests/smoke.rs
  - ../zorg/Cargo.toml
  - ../zorg/README.md
  - ../zorg/tools/
  - ../zorg/fixtures/corpus/
---

# Zorg Next Feature Recommendations After v1 MVP

## Scope

This research reviews the recent `zorg-1` legend work, the legend review note, and the original MVP curation file that
inspired the Rust v1 implementation. `sase bead show zorg-1` reports the parent legend as open, but all nine child epics
are closed:

1. Foundations.
2. Tree-sitter grammar and highlighting.
3. Rust parse and semantic model.
4. SQLite store and incremental indexing.
5. SWOG query MVP.
6. LSP MVP.
7. Capture and fix.
8. Neovim integration.
9. Documentation, release, and cross-repo validation.

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory layout used by generated SDD docs.

## Current Product Baseline

The v1 work successfully built the spine described in the curation research:

- `.z` files parse through Tree-sitter and lower into a Rust semantic model.
- Files, directory `init.z` files, nested notes, todos, refs, queries, and templates share the same zettel primitive.
- IDs, local IDs, absolute links, child links, sibling links, tags, properties, todos, source spans, and diagnostics are
  represented in typed Rust structures.
- SQLite indexing persists files, zettel, IDs, links, tags, effective tags, properties, todos, text payloads, diagnostics,
  and metadata.
- `zorg query` supports SWOG LIST queries and stored `#z/query` zettel.
- `zorg-ls` exposes diagnostics, completion, navigation, references, symbols, rename, and code actions.
- `zorg capture` and `zorg fix` cover the first write-side workflow.
- `zorg.nvim` delegates semantics to the CLI/LSP and provides filetype, Tree-sitter, commands, LSP startup, mappings, and
  health checks.

The practical MVP loop is now: write `.z` notes, run check/fix/capture, reindex, query, and use editor graph features.

## Observed Gaps That Matter Most

The gaps below are not defects in the MVP. They are the highest leverage places to turn a coherent first implementation
into a daily system. Each is grounded in a concrete observation from the v1 source tree:

- **Manual reindex.** `zorg query` and graph-backed LSP features degrade when the SQLite store is stale. There is no
  `notify`, `tokio`, or async runtime in `../zorg/Cargo.toml`, and no `watch`/`serve`/`daemon` subcommand in
  `../zorg/crates/zorg-cli/src/main.rs`. The watcher path is true greenfield, not an extension.
- **LIST-only query output.** `text_index` is provisioned in the schema (legend review, persistence model section), but
  full-text search is not enabled, and JSON output is not exposed for `zorg query`. Editor and TUI integrations would
  have to scrape the LIST renderer today.
- **No structural rewrites beyond rename.** `zorg-fix` and `zorg-ls` rename are span-safe, but `move`, `promote`,
  `extract`, and `open/path` commands are absent. Refactoring is what keeps a large note graph healthy.
- **No daily entry point.** `zorg dash` is still deferred. The fixture corpus has only 8 `.z` files in
  `../zorg/fixtures/corpus/`, so daily-loop ergonomics have not been pressure-tested on a real working set.
- **No configuration file.** The CLI hardcodes `$HOME/zorg` (`../zorg/crates/zorg-store/src/lib.rs`) and parses `--root`
  on every invocation (`../zorg/crates/zorg-cli/src/main.rs`). There is no `.zorg/config.toml` reader, so users repeat
  flags or wrap the CLI in shell aliases. The `.zorg/` directory exists only to hold `zorg.sqlite3`.
- **No release or distribution path.** No `.github/workflows/`, no Homebrew formula, no AUR PKGBUILD, no `cargo install`
  instructions in `../zorg/README.md`, and no documented release process. The `cross_repo.md` flow still references a
  `tools/validate_cross_repo.sh` that does not exist; only `tools/zorg_sibling_commit_stop_hook` is present.
- **Single-editor integration.** Only `../zorg-nvim` exists. The differentiation thesis from the original curation
  promises "editor-agnostic via LSP + Tree-sitter," but no VSCode extension, Helix language config, or Emacs client is
  documented.
- **Single-root design.** `Store` and `QueryContext` accept exactly one root path; queries cannot span multiple corpora
  in one invocation.
- **Static capture templates.** `../zorg/crates/zorg-capture/src/lib.rs` defines a fixed `TemplateValues` set (`{{id}}`,
  `{{title}}`, `{{date}}`, `{{source}}`, `{{body}}`) with no prompted fields, conditionals, or computed expressions.
- **Stale onboarding doc.** `../zorg/README.md` still describes capture and broader fix behavior as "later
  implementation phases" even though Epic 7 is closed and 32 CLI smoke tests exercise capture/fix in
  `../zorg/crates/zorg-cli/tests/smoke.rs`.
- **No performance baseline.** No `criterion` or benchmark crates anywhere in the workspace; no >1000-file fixture; no
  documented incremental reindex throughput. The original curation called for a "performance pass on db reindex" in M6
  and that pass has not landed visibly.

## Recommended Next Features

### 1. Live Workspace Indexing

Build a live indexing path that keeps the SQLite graph current without making users remember `zorg db reindex`.

Recommended shape:

- Add `zorg watch` or `zorg serve` as a long-running process over one root and database.
- Use file notifications to trigger debounced incremental reindex.
- Let `zorg-ls` request or host the same refresh path so editor graph features recover after saves.
- Teach `zorg.nvim` to surface index state and start the watcher when configured.
- Keep `zorg db reindex` as the deterministic batch command for scripts and CI.

Why this is highest impact:

- It removes the largest day-to-day friction in the current product.
- It makes completion, go-to-definition, references, rename, code actions, and query results feel trustworthy while
  writing.
- It reuses the existing incremental store architecture instead of inventing new semantics.

Acceptance bar:

- Saving a `.z` file updates `db status` and query results without manual reindex.
- `zorg-ls` transitions from degraded to ready after the watcher refreshes the store.
- Deleted and renamed files are reflected correctly.
- Watch mode has deterministic shutdown, logging, and test coverage for debounce behavior.

Effort and risk:

- Largest single epic in this list. Introduces an async runtime (likely `tokio`) and a file-watch crate (`notify`) into a
  workspace that is currently pure-sync — that is a load-bearing dependency choice that should be made with the rest of
  the roadmap in mind.
- The reindex code path is already incremental, so the watch glue is a small surface, but coordinating LSP server, CLI
  watcher, and on-disk database under concurrent writes will need explicit locking or single-writer routing.
- Risk of partial reads on slow filesystems and editor "save via temp file then rename" sequences. Plan tests around
  Vim-style swap files, JetBrains safe-write, and rapid `git checkout` switches.

### 2. Search And Query v1.1

Upgrade SWOG from a useful filter language into a practical retrieval layer.

Recommended shape:

- Enable SQLite FTS for title/body/raw text and route text filters through it.
- Add `--json` or `--format json` for query results so editor/TUI integrations do not scrape LIST output.
- Add OR and parenthesized grouping before TABLE output. These unlock common saved-query use cases without requiring an
  analytics renderer.
- Add TABLE and `count()` only after the expression model is stable.
- Keep query definitions as ordinary `#z/query` zettel.

Why this is high impact:

- Queries are the main payoff for indexing a plaintext corpus.
- Better text search makes Zorg useful even before a user has carefully tagged every note.
- Structured query output becomes the foundation for dashboards, virtual documents, and Neovim result buffers.

Acceptance bar:

- Existing LIST queries remain stable.
- Text filters use FTS when available and fall back clearly when not.
- `zorg query --json` returns source paths, IDs, titles, todo markers, source spans, and matched metadata.
- Unsupported query forms continue to fail clearly until implemented.

Effort and risk:

- Lowest-risk of the five. FTS5 is a SQLite extension shipped with `rusqlite` and only needs an additional virtual table
  plus a populate path during reindex. JSON output is a renderer change in `zorg-query` plus schema versioning.
- OR/grouping requires a real boolean expression layer, which is the only non-trivial parser work. Land FTS and JSON
  first; OR/grouping can ship as a follow-up without breaking SWOG users.
- Schema migration risk: the FTS table needs to be repopulated on store-version bumps. Handle it inside the existing
  migrations infrastructure rather than as a one-off.

### 3. Zettel Refactoring Commands

Add safe structural editing commands that exercise the "everything is a zettel" model after creation.

Recommended shape:

- `zorg move @id --to PATH_OR_PARENT` to move a zettel while preserving references.
- `zorg promote @id` to turn a nested zettel into a file zettel.
- `zorg extract RANGE --id @id` for turning selected text into a child or sibling zettel.
- `zorg open @id` or `zorg path @id` for scripts and editor integrations.
- Share the same source-span and rewrite safety rules as LSP rename.

Why this is high impact:

- Capture creates new material, but refactoring is what keeps a large note graph healthy.
- Promotion was called strategically important in the original curation because it proves the subnote-to-note-to-page
  continuum.
- This turns Zorg from an indexer into a structural authoring tool.

Acceptance bar:

- Commands reject ambiguous, missing-span, or collision-prone edits.
- Links and IDs are rewritten only when every affected source span is known.
- Neovim can call the commands without reimplementing rewrite logic in Lua.
- Golden tests cover nested-to-file promotion, move across directories, and rejected unsafe rewrites.

Effort and risk:

- Medium effort, high subtle-bug risk. Each rewrite touches multiple files, and the worst failure mode (silent link
  rot) is hard to detect after the fact. Treat preview/dry-run output and a `--check` mode as required, not optional.
- `promote` is the strategically-loaded command from the original curation because it proves the
  subnote→note→page continuum. If only one ships in the first cut, ship `promote` and let `move`/`extract` follow.
- Should reuse the LSP rename plan machinery rather than inventing parallel rewrite logic; otherwise we will end up with
  two source-of-truth implementations.

### 4. Daily Dashboard

Build the deferred `zorg dash` around query zettel and the existing store.

Recommended shape:

- Start with a terminal dashboard, not a plugin-only UI.
- Show inbox, due/overdue, next actions, recently modified notes, unresolved diagnostics, and saved query panels.
- Allow basic actions: open path, mark done, run capture, run fix, refresh index.
- Back every panel with a normal SWOG query or built-in query definition.
- Let `zorg.nvim` call the dashboard or expose equivalent query result buffers later.

Why this is high impact:

- It gives users a default daily entry point instead of making them memorize queries.
- It demonstrates the value of unified zettel types: todos, refs, inbox notes, queries, diagnostics, and templates can
  appear in one operational view.
- It was already identified as a high-value v1.1 candidate in the original curation.

Acceptance bar:

- Dashboard starts quickly against an existing index and reports stale/missing index state clearly.
- Default panels are useful on the fixture corpus and a real root.
- Actions call existing safe commands instead of mutating files directly.
- The dashboard remains optional; CLI, LSP, and Neovim keep working independently.

Effort and risk:

- Effort scales with how much the dashboard does on its own versus how much it delegates. Keep panels as thin renderers
  over `zorg query --json` (feature 2) so the dashboard does not become a parallel query implementation.
- Strongly benefits from feature 1 landing first; without live indexing, every panel either lies or forces a refresh
  before it is useful.
- Risk: TUI work tends to grow. Pick a small TUI dependency, scope to read-mostly panels in v1, and resist building a
  bespoke widget kit.

### 5. Import And Export Bridges

Add one-way bridges that make Zorg easier to adopt and easier to leave.

Recommended shape:

- `zorg import legacy` or `zorg migrate plan` for old `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, and `tick::` material,
  producing a reviewable plan before writing `.z`.
- `zorg export markdown` for selected zettel, query results, or a subtree.
- Preserve the v1 rule that canonical source is `.z` and legacy forms are not accepted as normal input.
- Prefer explicit, auditable transforms over silent compatibility.

Why this is high impact:

- The original MVP intentionally avoided migration compatibility to keep v1 small, but adoption still depends on getting
  real notes into the new format.
- Export lowers risk for users who want plaintext durability and publication options.
- A plan-first importer fits the existing strict-check and safe-fix philosophy.

Acceptance bar:

- Import planning reports every unsupported or lossy transform.
- Writing imported output creates `.z` files only and never leaves hidden compatibility state.
- Export can render a single zettel, a subtree, and a query result set to Markdown.
- Import/export tests use small fixtures and do not expand the core parser into a legacy compatibility layer.

Effort and risk:

- Lowest user-pull of the five today, but the highest unlock for adoption. Users will not switch from a working
  zettelkasten without a credible "get my notes in" and "get my notes out" story.
- Risk: legacy import quietly becomes a permanent dialect. Keep the importer plan-first and offline; reject the
  temptation to make `zorg parse` accept legacy syntax silently.
- Markdown export must decide what to do with `+child`, `~sibling`, and `^local` links. Pick a documented lossy mapping
  and surface a diagnostic when a zettel cannot round-trip cleanly.

## Recommended Build Order

1. Live Workspace Indexing.
2. Search And Query v1.1.
3. Zettel Refactoring Commands.
4. Daily Dashboard.
5. Import And Export Bridges.

Before starting feature epics, do two small housekeeping tasks:

- Reconcile the parent `zorg-1` bead if the legend is considered complete.
- Refresh stale status and validation documentation, especially the `../zorg` README and the missing cross-repo validation
  script reference.

## Adjacent Work That Almost Made The Top Five

These items are smaller than feature epics but each one is a real day-to-day friction. Several are good candidates to
ride along inside the top-five epics rather than wait for their own bead. They were not promoted to the top five only
because each is narrower than the five chosen, but two or three of them probably should ship before or alongside the
top five.

### A. Configuration File Support

`../zorg/crates/zorg-cli/src/main.rs` parses `--root`/`--db` per-invocation and `../zorg/crates/zorg-store/src/lib.rs`
hardcodes `$HOME/zorg`. There is no `.zorg/config.toml` reader. A small TOML or INI loader resolved in this order would
remove the most common ergonomic complaint:

1. CLI flags.
2. `$ZORG_ROOT` and `$ZORG_DB`.
3. `<root>/.zorg/config.toml`.
4. `$HOME/.config/zorg/config.toml`.
5. Defaults.

This pairs naturally with feature 1 (the watcher needs a place to record its socket/log path) and feature 4 (dashboard
panels need a saved root and default query set).

### B. Distribution And Release Pipeline

There are no `.github/workflows/` files in any of the three sibling repos, no Homebrew formula, no AUR PKGBUILD, and no
`cargo install` instructions in `../zorg/README.md`. The `cross_repo.md` flow points at a
`tools/validate_cross_repo.sh` that does not exist; the only present script is `tools/zorg_sibling_commit_stop_hook`.

Minimum viable release surface:

- A CI workflow that runs `cargo test` for `../zorg`, the Tree-sitter corpus tests for `../zorg-treesitter`, and a
  Neovim headless test for `../zorg-nvim`.
- The missing `tools/validate_cross_repo.sh` so cross-repo handoff is automated, not a manual checklist.
- Tagged releases and prebuilt binaries (or at minimum a documented `cargo install --git` recipe) so users do not have
  to clone three repos and figure out the build order.

This is infrastructure, not a feature, but adoption is currently capped by the install story.

### C. Editor Integration Beyond Neovim

The differentiation thesis says Zorg is editor-agnostic, but only `../zorg-nvim` exists today. Concrete adjacent work:

- A short `docs/editor_setup.md` explaining how to wire `zorg-ls` into VSCode, Helix, and Emacs (`lsp-mode` /
  `eglot`), what filetype/extension association to use, and how to load the Tree-sitter parser.
- A minimal VSCode extension that registers the `.z` filetype, the Tree-sitter grammar (via WASM), and starts
  `zorg-ls`. This is small but meaningfully expands the user base.
- A Helix language entry and an Emacs `tree-sitter-zorg` recipe.

This is mostly documentation and packaging, not new core surface, but it converts the "editor-agnostic" claim into
something a new user can verify in their editor of choice.

### D. Multi-Root Workspace Support

`Store` and `QueryContext` accept exactly one `--root`. Many users keep separate corpora for work, personal, and
archive. Two reasonable shapes:

- Lightweight: a `roots = ["~/zorg", "~/work-zorg"]` list in `config.toml` plus `--root @work` named-root flags so the
  CLI can target one of several known roots without long paths.
- Heavyweight: cross-root queries that union results across stores. This is appealing but introduces ID-collision and
  ranking questions that are out of scope for v1.1.

Recommend shipping the lightweight form alongside item A, and deferring cross-root union queries until after feature 2
ships.

### E. Capture Template Power

`../zorg/crates/zorg-capture/src/lib.rs` defines a fixed `TemplateValues` set of `{{id}}`, `{{title}}`, `{{date}}`,
`{{source}}`, `{{body}}`. A small expansion would unlock common workflows without inventing a new templating language:

- Prompted fields (`{{prompt:project_id}}`) that ask the user when running interactively and accept `--field
  project_id=...` non-interactively.
- Computed fields for `{{today}}`, `{{now}}`, `{{week}}`, and `{{git_branch}}`.
- Default-value fields (`{{date|today}}`).

This is independent of the top five and could ship as its own small bead.

### F. README And Cross-Repo Doc Refresh

`../zorg/README.md:92` still describes capture and broader fix as "later implementation phases." 32 CLI smoke tests in
`../zorg/crates/zorg-cli/tests/smoke.rs` already exercise capture and fix end to end. The README, the cross-repo doc,
and the missing validation script should be reconciled before the next external announcement. This is hours of work,
not days.

## Cross-Cutting Concerns

These do not belong to a single feature epic but should shape the next few sprints.

- **Performance baseline.** No `criterion` benches exist and no >1000-file fixture is checked in. Before feature 1 lands,
  it is worth adding a synthetic large-corpus generator and a baseline benchmark for `db reindex` so the watcher's
  debounce window can be tuned against real numbers, and so the FTS work in feature 2 has a regression target.
- **Async runtime decision.** Feature 1 is the first epic that needs an async runtime. Pick `tokio` once and use it
  consistently for the watcher, future serve mode, and dashboard refresh. Keep the CLI commands synchronous so the
  startup-time properties of `zorg query`, `zorg fix`, and `zorg capture` are unchanged.
- **Schema migrations.** Features 1, 2, and 3 each plausibly bump the SQLite schema version. Land a migrations test
  harness now; doing it later means rewriting three migrations under deadline.
- **Concurrency model.** Live indexing implies an opinion about concurrent writers. Make it explicit: the watcher is the
  single writer, the CLI defers to the watcher when one is running, and `zorg-ls` reads through the watcher rather than
  contending for the database.
- **Observability.** Today there is no structured logging story. Adding `tracing` and a `--log-level` flag costs little
  and pays for itself the first time a user reports a stale-index bug.

## Features Not Recommended As The Next Five

These remain valuable but are not the right next move.

- **Plugin system.** Wait until the core command/API surface stabilizes after watch, query JSON, and refactoring
  commands. Locking a plugin contract too early forces backwards-compat costs on every later epic.
- **Habit tracking.** Useful vertical workflow, but not central to the zettel graph. The original curation explicitly
  flagged it as standalone.
- **Recutils integration or alternate graph database.** SQLite is sufficient for the next two layers; switching backends
  adds risk without clear user payoff.
- **Folgezettel tree jumpers.** Valuable later, but they should build on refactoring, search, and dashboard primitives.
- **Embedded notes (`((foo))` / `%(foo)`).** Useful, but higher risk because it introduces rendering and source-rewrite
  semantics before move/promote are proven.
- **Saved query dot-snippets and named URL shorthand `!foo`.** Both depend on a stable query/expression model and are
  cheap to add later.
- **Datalog/graph-database backend.** Premature; SQLite plus FTS plus OR/grouping is more than enough for v1.x.

## Research Methodology

This revision is grounded in the artifacts below. Future revisions should re-validate the citations rather than trust
them, since several depend on file paths and line numbers that drift.

- `sase bead show zorg-1` and the `zorg-1` ledger in `sdd/beads/issues.jsonl`.
- The legend at `sdd/legends/202605/zorg_v1_mvp.md` and the legend review at `sdd/research/zorg_1_legend_review.md`.
- The original curation at `../zorg/research/202605/v1_mvp_curation.md`.
- A direct read of `../zorg/Cargo.toml`, `../zorg/crates/zorg-cli/src/main.rs`,
  `../zorg/crates/zorg-store/src/lib.rs`, `../zorg/crates/zorg-query/src/lib.rs`, and
  `../zorg/crates/zorg-capture/src/lib.rs` to confirm the absence of an async runtime, file watcher, FTS, refactoring
  commands, multi-root support, and config-file parsing.
- A scan of `../zorg/crates/zorg-cli/tests/smoke.rs` (32 functions) and `../zorg/crates/zorg-ls/tests/smoke.rs` (26
  functions) for current test coverage; capture and fix are well covered, multi-root and performance are not.
- A scan of `../zorg/tools/` confirming `zorg_sibling_commit_stop_hook` exists and `validate_cross_repo.sh` does not.
- A scan of the three sibling repos for `.github/workflows/` (none present) and packaging artifacts (none present).
- Inspection of `../zorg/fixtures/corpus/` (8 `.z` files) as the largest checked-in corpus.

No code or docs were modified by this research; this is a planning artifact.

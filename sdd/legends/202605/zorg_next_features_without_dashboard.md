---
title: Zorg next feature implementation plan without dashboard
legend_bead_id: zorg-4
tier: legend
created: 2026-05-02
source_research: sdd/research/202605/zorg_next_feature_recommendations.md
scope:
- live workspace indexing
- search and query v1.1
- zettel refactoring commands
- import and export bridges
- strong zorg.nvim support
excluded:
- daily dashboard
create_time: 2026-05-02 23:41:15
status: wip
prompt: sdd/prompts/202605/zorg_next_features_without_dashboard.md
---

# Zorg Next Features Without Dashboard

## Objective

Implement the recommended next Zorg features from `sdd/research/202605/zorg_next_feature_recommendations.md`, explicitly
excluding the Daily Dashboard. The work spans the sibling repositories:

- `../zorg`: Rust CLI, store, query, LSP, fix/capture, refactoring, import/export, docs, tools, and tests.
- `../zorg-nvim`: Neovim command/UI integration over the new Rust CLI/LSP surfaces.
- `../zorg-treesitter`: only touched if query/refactoring/import/export examples require syntax-query updates or
  cross-repo validation fixtures.

The end state should give users a live, searchable, structurally refactorable Zorg corpus with auditable import/export
paths, and it should make `zorg.nvim` feel like a first-class frontend without moving Zorg semantics into Lua.

## Explicit Non-Scope

- Do not implement `zorg dash`, terminal dashboards, saved dashboard panels, dashboard actions, or plugin-only dashboard
  views in this round.
- Do not make legacy `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`, `LID::`, or `tick::` syntax normal parser input.
- Do not implement VSCode, Helix, or Emacs clients in this plan. Editor-agnostic docs may mention the LSP contract, but
  implementation focus is `../zorg-nvim`.
- Do not introduce a general plugin system.
- Do not attempt heavyweight cross-root union queries. Lightweight named-root config may be introduced if it directly
  supports watcher and Neovim ergonomics.

## Current Baseline

The checked implementation already has the v1 spine:

- Sync Rust workspace with `zorg db reindex`, `zorg db status`, `zorg query`, `zorg check`, `zorg fix`, `zorg capture`,
  and `zorg-ls`.
- SQLite schema version `1` with normalized files, zettel, links, tags, effective tags, properties, todos, diagnostics,
  and a non-FTS `text_index` table.
- SWOG MVP uses implicit AND, LIST rendering, query zettel definitions, and in-memory lowercase substring text matching.
- `zorg-ls` loads one store snapshot, reports degraded state for stale/missing indexes, and serves graph features from
  that snapshot.
- `../zorg-nvim` already wraps index/status/query/fix/capture, starts `zorg-ls`, passes root/db initialization options,
  loads Tree-sitter queries, exposes health checks, and has headless tests.

The next work should build on these surfaces rather than replacing them.

## Cross-Cutting Product Rules

- Rust remains the source of truth for parse, query, refactor, import/export, and graph semantics.
- Lua in `../zorg-nvim` shells out to `zorg`, starts/configures `zorg-ls`, renders structured output, and exposes
  ergonomic commands and health state.
- Long-running live indexing should be the only new async-heavy path. Keep one-shot commands fast and sync at their
  outer CLI boundary where practical.
- Any source rewrite must be span-backed, previewable, deterministic, and refused when spans are incomplete or
  ambiguous.
- Any schema migration must be versioned, tested from older databases, and safe to run repeatedly.
- Every phase must leave its touched repo in a passing state with the relevant local validation commands.

## Epic Dependency Order

1. Epic 10: shared foundations, config, validation, and performance baselines.
2. Epic 11: live workspace indexing.
3. Epic 12: search and query v1.1.
4. Epic 13: zettel refactoring commands.
5. Epic 14: import and export bridges.
6. Epic 15: full `zorg.nvim` support for the new feature set.

Epics 12 and 13 can begin after Epic 10 if scheduling pressure is high, but Epic 15 should wait until stable JSON/CLI
contracts exist for the relevant core features. Epic 14 can run after Epic 10 and does not depend on live indexing.

## Epic 10: Foundations, Configuration, Validation, And Baselines

Purpose: prepare the repos for large v1.1 work before adding feature behavior.

### Phase 10.1: Config Contract And Store Path Resolution

Owner: one Rust CLI/store agent.

Primary files:

- `../zorg/crates/zorg-store/src/lib.rs`
- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/docs/development.md`
- `../zorg/README.md`

Scope:

- Add a documented config resolution order: CLI flags, environment, root-local `.zorg/config.toml`, user config, then
  defaults.
- Keep `StoreOptions` as the canonical resolved path type.
- Add config keys needed by later work: root, database path, watcher debounce, watcher log path, named roots if cheap,
  and Neovim-friendly defaults.
- Preserve current `--root` and `--db` behavior exactly when flags are supplied.

Acceptance:

- Existing CLI smoke tests pass unchanged.
- New tests prove precedence, invalid config errors, and no dependency on the user's real `~/zorg`.
- `zorg db status` prints the resolved root/database and remains script-friendly.

### Phase 10.2: Schema Migration Harness

Owner: one store-focused Rust agent.

Primary files:

- `../zorg/crates/zorg-store/src/lib.rs`
- `../zorg/crates/zorg-store/Cargo.toml`

Scope:

- Introduce explicit migration tests that can create a schema-version-1 database and migrate it forward.
- Add helper APIs for future migrations used by watcher metadata, FTS, and refactoring/import metadata if needed.
- Keep migrations deterministic and idempotent.

Acceptance:

- Tests cover fresh database creation, reopen, v1-to-current migration, and failed/unknown future schema handling.
- No feature migration is added here unless required to make the harness real.

### Phase 10.3: Cross-Repo Validation Script And Docs Refresh

Owner: one cross-repo docs/tools agent.

Primary files:

- `../zorg/tools/validate_cross_repo.sh`
- `../zorg/docs/cross_repo.md`
- `../zorg/README.md`
- `../zorg-nvim/README.md`
- `../zorg-treesitter/README.md`

Scope:

- Add the missing cross-repo validation script referenced by docs.
- Refresh stale README language around capture/fix and current command names.
- Document validation commands for Rust, Tree-sitter, and Neovim.

Acceptance:

- The script runs the Rust workspace tests, Tree-sitter generation/corpus tests, and Neovim headless tests using fixture
  roots, not the user's real corpus.
- Docs describe the current v1 baseline and point to the upcoming v1.1 feature work without promising dashboard support.

### Phase 10.4: Large-Corpus Generator And Performance Baseline

Owner: one Rust tooling/performance agent.

Primary files:

- `../zorg/tools/`
- `../zorg/crates/zorg-store/`
- `../zorg/docs/development.md`

Scope:

- Add a deterministic synthetic corpus generator for hundreds/thousands of `.z` files.
- Add benchmarks or benchmark-like tests for `db reindex` and status freshness checks.
- Capture baseline numbers in docs without making CI depend on exact wall-clock timing.

Acceptance:

- Developers can generate a large corpus under a temp root.
- A documented command reports reindex throughput and changed-file incremental cost.
- Watcher and FTS phases have a repeatable regression target.

## Epic 11: Live Workspace Indexing

Purpose: keep SQLite current after file saves, deletes, renames, and branch switches, and expose index state to LSP and
Neovim.

### Phase 11.1: Watcher Architecture And Dependency Decision

Owner: one Rust architecture agent.

Primary files:

- `../zorg/Cargo.toml`
- `../zorg/crates/zorg-store/src/lib.rs`
- new watcher module or crate under `../zorg/crates/`
- `../zorg/docs/development.md`

Scope:

- Decide and document the async/runtime boundary, likely `tokio` plus `notify`.
- Define watcher events, debounce policy, ignored paths, shutdown behavior, and logging.
- Define the concurrency model: single writer when watcher is active; one-shot CLI commands remain deterministic.

Acceptance:

- A compile-tested skeleton exposes a watcher service API without starting it from the CLI yet.
- Tests cover path filtering and debounce scheduling using a fake clock/event source if possible.

### Phase 11.2: Incremental Watch Reindex Engine

Owner: one Rust store/watcher agent.

Primary files:

- watcher module/crate
- `../zorg/crates/zorg-store/src/lib.rs`
- watcher tests

Scope:

- Convert file events into debounced incremental reindex calls.
- Correctly handle write-via-temp-rename, deletes, renames, rapid repeated saves, ignored temporary files, and
  `git checkout` style changes.
- Add structured logs and deterministic shutdown.

Acceptance:

- Saving a `.z` file updates `db status` and query results without manual `db reindex`.
- Deleted and renamed files are reflected correctly.
- Tests cover debounce behavior and representative editor save patterns.

### Phase 11.3: `zorg watch` CLI

Owner: one Rust CLI agent.

Primary files:

- `../zorg/crates/zorg-cli/src/main.rs`
- watcher module/crate
- `../zorg/crates/zorg-cli/tests/smoke.rs`
- `../zorg/docs/development.md`

Scope:

- Add `zorg watch [--root PATH] [--db PATH] [--debounce MS] [--log-level LEVEL]`.
- Keep `zorg db reindex` as the batch/CI command.
- Provide machine-readable status output if useful for `zorg.nvim`.

Acceptance:

- CLI starts, logs ready state, indexes on changes, and exits cleanly on signal or test-controlled shutdown.
- Smoke tests avoid indefinite processes by using bounded run/test flags or a service harness.

### Phase 11.4: LSP Refresh Integration

Owner: one Rust LSP agent.

Primary files:

- `../zorg/crates/zorg-ls/src/state.rs`
- `../zorg/crates/zorg-ls/src/main.rs`
- `../zorg/crates/zorg-ls/tests/smoke.rs`

Scope:

- Teach `zorg-ls` to refresh its store snapshot after saves or after watcher completion signals.
- Decide whether `zorg-ls` hosts an internal watcher, talks to an external watcher, or performs bounded refreshes on
  document save.
- Surface degraded/ready transitions through diagnostics, log messages, or LSP notifications.

Acceptance:

- `zorg-ls` transitions from stale/degraded to ready after refresh.
- Completion, references, navigation, and rename use the refreshed graph without restarting the server.
- Tests cover stale-to-ready behavior.

### Phase 11.5: Neovim Watch Controls

Owner: deferred to Epic 15 implementation agent, but contract defined here.

Required Rust-facing contract:

- `zorg watch` must have stable flags, clear ready/error output, and a bounded test mode.
- `zorg-ls` must expose enough state for health/status commands to say whether graph data is fresh.

## Epic 12: Search And Query v1.1

Purpose: make SWOG a practical retrieval layer with FTS-backed text search, structured output, and a real boolean
expression model.

### Phase 12.1: Query JSON Contract

Owner: one Rust query/CLI agent.

Primary files:

- `../zorg/crates/zorg-query/src/lib.rs`
- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/docs/query.md`
- `../zorg/crates/zorg-cli/tests/smoke.rs`

Scope:

- Add `zorg query --json` and/or `--format json`.
- Define stable result rows containing source paths, IDs, titles, todo markers, source spans, tags/properties needed by
  editor clients, and diagnostics/errors.
- Keep current LIST output byte-for-byte stable unless a test intentionally updates it.

Acceptance:

- JSON output works for inline queries and `--id @query`.
- Empty results return valid JSON.
- Errors remain clear and script-detectable.

### Phase 12.2: SQLite FTS Migration And Store APIs

Owner: one Rust store agent.

Primary files:

- `../zorg/crates/zorg-store/src/lib.rs`
- store migration tests

Scope:

- Add an FTS5-backed virtual table for title/body/raw text.
- Populate it during reindex and repopulate on migration.
- Expose a store query API for text search with a clear fallback/error path if FTS is unavailable.

Acceptance:

- Existing text index data migrates correctly.
- FTS rows stay in sync with zettel rows after create/update/delete.
- Tests prove title, body, and raw searches.

### Phase 12.3: Query Evaluation Uses FTS

Owner: one Rust query agent.

Primary files:

- `../zorg/crates/zorg-query/src/lib.rs`
- `../zorg/crates/zorg-query/Cargo.toml`

Scope:

- Route text filters and quoted phrases through the FTS store API when available.
- Preserve deterministic ordering and non-text filters.
- Keep a deliberate, tested fallback or clear unsupported error.

Acceptance:

- Existing SWOG tests pass.
- Text search is case-insensitive and uses FTS ranking or deterministic tie-breakers without destabilizing LIST output.
- Query JSON can report matched text metadata if feasible.

### Phase 12.4: Boolean OR And Parentheses

Owner: one Rust query-parser agent.

Primary files:

- `../zorg/crates/zorg-query/src/lib.rs`
- `../zorg/docs/query.md`

Scope:

- Replace flat implicit-AND-only query representation with a boolean expression AST.
- Support OR and parenthesized grouping.
- Keep old simple queries as the simplest expression form.

Acceptance:

- Existing simple queries parse and evaluate identically.
- OR/grouping tests cover precedence, negation, errors, and query zettel definitions.
- Unsupported TABLE/aggregation syntax continues to fail clearly.

### Phase 12.5: TABLE And `count()` MVP

Owner: one Rust query renderer agent.

Primary files:

- `../zorg/crates/zorg-query/src/lib.rs`
- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/docs/query.md`

Scope:

- Add TABLE output only after the expression model is stable.
- Add minimal `count()` support if it can be represented cleanly without creating a dashboard-specific renderer.
- Ensure JSON covers both list rows and table/aggregate outputs.

Acceptance:

- LIST remains stable.
- TABLE and count have focused tests, documented limitations, and useful errors for unsupported forms.

## Epic 13: Zettel Refactoring Commands

Purpose: add safe structural editing over the existing "everything is a zettel" model.

### Phase 13.1: Shared Rewrite Planning Foundation

Owner: one Rust fix/LSP agent.

Primary files:

- `../zorg/crates/zorg-fix/src/`
- `../zorg/crates/zorg-ls/src/rename.rs`
- new refactor module/crate if warranted

Scope:

- Extract or reuse the LSP rename planning machinery for CLI refactors.
- Define common edit-plan, preview, conflict, and span-safety types.
- Add dry-run/check output that Neovim can consume.

Acceptance:

- Existing rename and fix tests pass.
- Shared plan types can represent multi-file edits, rejected edits, and source ranges.

### Phase 13.2: `zorg path` / `zorg open`

Owner: one Rust CLI/query agent.

Primary files:

- `../zorg/crates/zorg-cli/src/main.rs`
- store/query APIs
- CLI smoke tests

Scope:

- Add `zorg path @id` or `zorg open @id` to print the source file and optional line/column for a zettel.
- Support JSON output for editor integrations.

Acceptance:

- Missing, ambiguous, or anonymous zettel produce clear errors.
- Neovim can jump to results without parsing LIST output.

### Phase 13.3: `zorg promote @id`

Owner: one Rust refactor agent.

Primary files:

- refactor module/crate
- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/crates/zorg-cli/tests/smoke.rs`

Scope:

- Promote a nested zettel into a file zettel while preserving IDs, body, tags, properties, and known references.
- Require preview/check mode before write semantics are trusted.
- Refuse unsafe rewrites when source spans or destination paths are ambiguous.

Acceptance:

- Golden tests cover nested-to-file promotion, destination collisions, and rejected unsafe cases.
- Links are updated only when every affected source span is known.

### Phase 13.4: `zorg move @id --to PATH_OR_PARENT`

Owner: one Rust refactor agent.

Primary files:

- refactor module/crate
- CLI and smoke tests

Scope:

- Move file or nested zettel to another file/parent while preserving references.
- Support dry-run/check and JSON preview.

Acceptance:

- Tests cover moves across directories, parent changes, collisions, and link preservation.
- The command is idempotent or refuses to run when the target state already conflicts.

### Phase 13.5: `zorg extract RANGE --id @id`

Owner: one Rust refactor agent.

Primary files:

- refactor module/crate
- CLI and smoke tests

Scope:

- Extract a byte/line range into a child or sibling zettel with a chosen ID.
- Preserve selected text exactly unless formatting is explicitly documented.
- Provide editor-friendly arguments for current buffer/range use.

Acceptance:

- Tests cover range boundaries, multiline extraction, invalid ranges, and collision refusal.
- JSON preview includes edits and destination details.

### Phase 13.6: LSP Code Actions For Refactors

Owner: one Rust LSP agent.

Primary files:

- `../zorg/crates/zorg-ls/src/actions.rs`
- `../zorg/crates/zorg-ls/tests/smoke.rs`

Scope:

- Expose safe promote/move/extract opportunities as LSP code actions where practical.
- Keep the CLI as the authoritative execution path if an editor command must shell out.

Acceptance:

- LSP tests cover advertised actions and rejected contexts.
- No Lua-side semantic rewrite logic is required.

## Epic 14: Import And Export Bridges

Purpose: make adoption and exit credible while keeping canonical `.z` as the only normal source format.

### Phase 14.1: Bridge Specs And Fixture Corpus

Owner: one docs/fixtures agent.

Primary files:

- `../zorg/docs/import_export.md`
- `../zorg/fixtures/`
- `../zorg/README.md`

Scope:

- Specify legacy import planning, lossy transform reporting, write behavior, and Markdown export mapping.
- Add small legacy input fixtures and expected `.z`/Markdown outputs.

Acceptance:

- Docs explicitly state that legacy syntax is not accepted by `zorg parse` or normal indexing.
- Fixtures cover `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, and `tick::` examples as import inputs only.

### Phase 14.2: Import Plan Engine

Owner: one Rust import agent.

Primary files:

- new import/export module/crate or scoped module under `../zorg`
- CLI tests

Scope:

- Implement `zorg import legacy plan PATH...` or equivalent.
- Produce JSON and text plans listing output files, transforms, lossy cases, unsupported forms, and collisions.
- Do not write files in this phase.

Acceptance:

- Plan output is deterministic.
- Unsupported/lossy forms are surfaced, not silently rewritten.

### Phase 14.3: Import Write Mode

Owner: one Rust import agent.

Primary files:

- import module/crate
- `../zorg/crates/zorg-cli/src/main.rs`
- CLI smoke tests

Scope:

- Add explicit write/apply mode that creates `.z` files only.
- Never leave hidden compatibility state.
- Refuse overwrites unless a documented force/replace behavior is supplied.

Acceptance:

- Writing imported output passes `zorg check` and indexes under a temp root.
- Failed imports leave no partial hidden state, or report exactly what was written.

### Phase 14.4: Markdown Export Engine

Owner: one Rust export agent.

Primary files:

- import/export module/crate
- query/store APIs
- CLI smoke tests

Scope:

- Implement `zorg export markdown` for a single zettel, subtree, and query result set.
- Document and implement the lossy mapping for `+child`, `~sibling`, and `^local` links.
- Support stdout and output-directory modes.

Acceptance:

- Export tests cover single zettel, subtree, query result set, unresolved links, and lossy diagnostics.
- Export never mutates source `.z` files.

## Epic 15: Great `zorg.nvim` Support

Purpose: make `../zorg-nvim` expose the new capabilities cleanly, with async UI, result buffers, health, docs, and
tests. This epic consumes stable Rust CLI/LSP contracts from Epics 11-14.

### Phase 15.1: Config And Health Upgrade

Owner: one Neovim agent.

Primary files:

- `../zorg-nvim/lua/zorg/config.lua`
- `../zorg-nvim/lua/zorg/health.lua`
- `../zorg-nvim/README.md`
- `../zorg-nvim/doc/zorg.txt`

Scope:

- Mirror the Rust config model without duplicating resolution semantics.
- Show resolved root/database, watcher availability, LSP freshness support, CLI version, and parser/query status.
- Keep defaults conservative and no forced global mappings.

Acceptance:

- Headless health/config tests pass.
- Health messages explain missing watcher/CLI/LSP states clearly.

### Phase 15.2: Watcher Lifecycle UI

Owner: one Neovim agent.

Primary files:

- `../zorg-nvim/lua/zorg/commands.lua`
- `../zorg-nvim/lua/zorg/init.lua`
- tests

Scope:

- Add commands such as `:ZorgWatchStart`, `:ZorgWatchStop`, and `:ZorgWatchStatus` once the Rust contract is stable.
- Manage one watcher job per root, surface logs/errors, and avoid duplicate watchers.
- Optionally auto-start only when configured.

Acceptance:

- Tests use a fake `zorg` binary to verify argv, job lifecycle, and status rendering.
- Users can see whether index state is fresh without leaving Neovim.

### Phase 15.3: Query JSON Result Buffers

Owner: one Neovim agent.

Primary files:

- `../zorg-nvim/lua/zorg/commands.lua`
- possible new result-buffer module
- tests

Scope:

- Call `zorg query --json` for query buffers instead of scraping LIST output.
- Render navigable result buffers with IDs, titles, paths, todo markers, and source positions.
- Add mappings/actions to open result locations.

Acceptance:

- Query result rendering handles empty results, errors, and query-by-id.
- Tests decode fixture JSON and verify buffer contents and navigation targets.

### Phase 15.4: Refactor Commands And Range Integration

Owner: one Neovim agent.

Primary files:

- `../zorg-nvim/lua/zorg/commands.lua`
- `../zorg-nvim/lua/zorg/mappings.lua`
- tests/docs

Scope:

- Add `:ZorgPath`, `:ZorgPromote`, `:ZorgMove`, and visual-range `:ZorgExtract` wrappers.
- Use Rust dry-run/JSON previews and ask for explicit confirmation before writes if the CLI requires it.
- Refresh buffers and LSP/index state after successful writes.

Acceptance:

- No Lua-side source rewriting.
- Tests verify argv construction, visual range conversion, preview handling, and successful edit refresh behavior.

### Phase 15.5: Import/Export Commands

Owner: one Neovim agent.

Primary files:

- `../zorg-nvim/lua/zorg/commands.lua`
- docs/tests

Scope:

- Add import-plan command that renders a reviewable plan buffer.
- Add import-apply wrapper only when the Rust CLI provides explicit safe write semantics.
- Add export-markdown wrappers for current zettel, subtree, and query result set.

Acceptance:

- Plan buffers clearly separate warnings, errors, and writes.
- Export commands open generated output or display stdout without mutating `.z` buffers.

### Phase 15.6: End-To-End Neovim Validation

Owner: one cross-repo Neovim validation agent.

Primary files:

- `../zorg-nvim/tests/`
- `../zorg/tools/validate_cross_repo.sh`
- docs

Scope:

- Extend headless tests with fake and real CLI paths where practical.
- Add cross-repo validation for watcher command contracts, query JSON, refactor wrappers, import/export wrappers, and
  LSP initialization options.

Acceptance:

- Cross-repo validation proves Neovim can exercise every new non-dashboard feature without scraping unstable text.
- README/help docs contain a concise recommended setup and command map.

## Recommended Bead/Epic Naming

Use sequential v1.1 epic beads after the closed v1 epics:

- `zorg-1.10`: Foundations/config/validation/performance.
- `zorg-1.11`: Live workspace indexing.
- `zorg-1.12`: Search and query v1.1.
- `zorg-1.13`: Zettel refactoring commands.
- `zorg-1.14`: Import and export bridges.
- `zorg-1.15`: `zorg.nvim` v1.1 integration.

Each phase should become its own implementation bead or child task. If the parent `zorg-1` legend is now considered
complete, reconcile it before opening the v1.1 beads.

## Validation Matrix

Rust validation:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- feature-specific CLI smoke tests using temp roots and temp databases

Tree-sitter validation when touched:

- `npm run generate`
- `npm test`
- highlight/query smoke checks against shared fixtures

Neovim validation:

- `nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"`
- feature-specific headless tests for commands, helpers, LSP setup, watcher jobs, and result buffers

Cross-repo validation:

- `../zorg/tools/validate_cross_repo.sh`
- test roots must be fixtures or temporary directories, never the user's real `~/zorg`

## Handoff Guidance For Future Agents

- Start every phase with `git status --short` in each touched repo.
- Read the previous phase's public types and tests before changing behavior.
- Do not revert unrelated changes in sibling repos.
- Keep command output contracts documented in the same phase that introduces them.
- Prefer JSON for editor integration surfaces; LIST/text output remains for humans.
- When a phase introduces a new CLI command, add Neovim contract notes even if the Lua wrapper lands in Epic 15.
- When a phase changes store/query semantics, update both CLI smoke tests and LSP/query integration tests where
  relevant.

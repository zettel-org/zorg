---
create_time: 2026-05-03 18:36:56
bead_id: zorg-4.6
tier: epic
legend_bead_id: zorg-4
status: wip
prompt: sdd/prompts/202605/zorg_nvim_epic_15.md
---
# Plan: Complete Epic 15, `zorg.nvim` v1.1 Integration

## Context

Epic #6 in `sdd/legends/202605/zorg_next_features_without_dashboard.md` is the sixth dependency-order item, Epic 15:
Great `zorg.nvim` Support. The target repository is `../zorg-nvim`, with cross-repo validation in `../zorg` and no
normal semantic implementation in Lua.

Current baseline:

- `../zorg-nvim` already exposes thin wrappers for `zorg db reindex`, `zorg db status`, `zorg query`, `zorg fix`, and
  `zorg capture`.
- `../zorg-nvim` already starts `zorg-ls`, passes root/database/trace initialization options, ships Tree-sitter runtime
  queries, has health checks, optional mappings, and headless tests.
- `../zorg` currently exposes stable `zorg watch` JSON output and `zorg query --json` / `--format json`.
- `../zorg` does not currently expose `zorg path`/`zorg open`, `zorg promote`, `zorg move`, `zorg extract`,
  `zorg import`, or `zorg export` commands. The corresponding Neovim phases must wait for those Rust contracts or land
  only explicit unavailable-state handling.

Product rules:

- Rust remains the source of truth for parsing, querying, indexing, refactoring, import/export, and graph semantics.
- Lua only shells out to stable CLI commands, configures `zorg-ls`, renders structured output, manages jobs/buffers, and
  exposes ergonomic commands.
- Use JSON contracts for editor integration. Existing human-readable LIST/text output remains available but must not be
  scraped for new features.
- Every phase must leave touched repos passing their relevant local validation commands.
- Do not add dashboard features, global mappings by default, or Lua-side source rewriting.

## Phase Breakdown

### Phase 15.0: Contract Audit And Shared Neovim Test Harness

Owner: one Neovim/contracts agent.

Purpose: create the shared implementation footing before feature agents edit independently.

Scope:

- Audit and document the Rust CLI/LSP contracts that Epic 15 is allowed to consume: `zorg watch --format json`,
  `zorg query --json`, current LSP initialization/freshness behavior, and unavailable refactor/import/export contracts.
- Add or refactor reusable Lua test helpers for fake `zorg` binaries, fake async runners, scratch buffer assertions,
  notification capture, and command argv assertions.
- Add small fixture JSON payloads or inline builders for watcher and query outputs so later tests are not duplicated.
- Add a short contract note in `../zorg-nvim` docs that marks refactor/import/export wrappers as blocked until matching
  Rust commands exist.

Acceptance:

- Existing headless tests still pass.
- Later agents can test new commands without copying fake-runner boilerplate.
- The plan of record clearly identifies which Epic 15 features are implementable now and which are gated by upstream
  Rust work.

Validation:

- From `../zorg-nvim`: run all existing headless tests.

### Phase 15.1: Config And Health Upgrade

Owner: one Neovim config/health agent.

Dependencies: Phase 15.0.

Purpose: mirror the Rust config model at the editor boundary without duplicating Rust resolution semantics.

Scope:

- Extend `lua/zorg/config.lua` with watcher options needed by later phases: enabled/autostart flag, debounce override,
  log display preference if useful, and per-root job policy.
- Keep `root`, `database_path`/`db_path`, CLI command, and LSP command behavior backward compatible.
- Improve `lua/zorg/health.lua` to report configured root/database, effective Neovim-side watcher settings,
  availability/version of `zorg`, availability/version of `zorg-ls`, Tree-sitter parser/query status, and whether the
  CLI appears to support `watch` and `query --json`.
- Do not run expensive indexing or long-lived watcher processes from health.

Acceptance:

- Health output is clear when binaries are missing, when watcher support is missing, and when query JSON support is
  missing.
- Default setup remains conservative: no forced global mappings and no watcher autostart by default.
- Tests cover config expansion and health behavior with fake binaries.

Validation:

- From `../zorg-nvim`: headless tests for config, health, smoke, helpers, commands, and LSP.

### Phase 15.2: Watcher Lifecycle UI

Owner: one Neovim async/job agent.

Dependencies: Phases 15.0 and 15.1; Rust `zorg watch --format json` is present.

Purpose: expose live indexing from Neovim with one managed watcher job per root.

Scope:

- Add a watcher module or well-contained command helpers for `:ZorgWatchStart`, `:ZorgWatchStop`, and
  `:ZorgWatchStatus`.
- Start `zorg watch --root {root} [--db {db}] --format json`, plus configured debounce when set.
- Track watcher jobs by resolved root and prevent duplicate jobs for the same root.
- Parse line-delimited watcher JSON states: `starting`, `ready`, `indexing`, `indexed`, `degraded`, `error`, `stopping`,
  and `stopped`.
- Surface status through notifications and a lightweight status/scratch buffer without forcing users into a dashboard.
- Ensure shutdown is deterministic for tests and normal Neovim exit.

Acceptance:

- Fake-binary tests verify argv construction, duplicate-start refusal, stop behavior, JSON state parsing, and status
  rendering.
- Users can see whether live indexing is unavailable, starting, ready, indexing, degraded, errored, or stopped.
- Watcher failures do not crash Neovim or leave stale job state.

Validation:

- From `../zorg-nvim`: watcher-specific headless test plus existing headless suite.
- From `../zorg`: targeted watcher CLI tests if cross-repo contract concerns arise.

### Phase 15.3: Query JSON Result Buffers

Owner: one Neovim result-buffer agent.

Dependencies: Phase 15.0; Rust `zorg query --json` is present.

Purpose: stop scraping LIST output and render navigable query results from structured JSON.

Scope:

- Make `:ZorgQuery` prefer `zorg query --json` unless the user explicitly requests text/list output.
- Support inline queries and `:ZorgQuery --id @query/id`.
- Decode schema-versioned JSON envelopes for `kind = "list"` and `kind = "table"`.
- Render result buffers containing IDs, todo markers, titles, root-relative paths, and source locations.
- Add buffer-local mappings/actions to open the source location using JSON `path` and `span` fields.
- Handle empty results, diagnostics arrays, command errors, invalid JSON, unsupported schema versions, and table output.

Acceptance:

- Query result buffers are navigable without parsing human LIST output.
- Tests cover inline query, query-by-id, empty rows, invalid JSON, table rows, diagnostics, and opening source
  locations.
- Existing query helper behavior remains backward compatible for users who pass normal SWOG text.

Validation:

- From `../zorg-nvim`: query result-buffer tests plus existing headless suite.
- Optional cross-repo smoke using a real `cargo run -p zorg-cli -- query --json` fixture.

### Phase 15.4: Refactor Command Wrappers And Range Integration

Owner: one Neovim refactor UX agent.

Dependencies: Phase 15.0 plus Rust Epic 13 contracts for `zorg path`/`zorg open`, `zorg promote`, `zorg move`, and
`zorg extract`. Do not implement semantic rewrites in Lua if these commands are still absent.

Purpose: expose safe structural refactors from Neovim while keeping Rust authoritative.

Scope:

- Add `:ZorgPath` or `:ZorgOpen` to jump from an ID to a file/span using Rust JSON output.
- Add `:ZorgPromote`, `:ZorgMove`, and visual-range `:ZorgExtract` wrappers once the corresponding Rust commands expose
  JSON dry-run/preview and write/apply modes.
- Convert visual selections to the Rust-supported range form without changing source text in Lua.
- Render JSON previews in a reviewable buffer and require explicit confirmation before write/apply when the CLI contract
  requires it.
- Refresh changed buffers and LSP/index state after successful writes.
- If Rust commands are not yet available, provide explicit health/command unavailable messages rather than hidden stubs.

Acceptance:

- Tests verify argv construction, current-buffer ID/range handling, preview rendering, confirmation flow, cancellation,
  successful write refresh behavior, and unavailable-command behavior.
- No Lua code edits `.z` source directly for promote/move/extract.
- Existing `:ZorgFix` behavior remains unchanged.

Validation:

- From `../zorg-nvim`: refactor wrapper tests plus existing headless suite.
- From `../zorg`: targeted CLI smoke tests for the Rust refactor command contracts when available.

### Phase 15.5: Import And Export Commands

Owner: one Neovim import/export agent.

Dependencies: Phase 15.0 plus Rust Epic 14 contracts for legacy import planning/apply and Markdown export. Do not
implement legacy parsing or Markdown conversion in Lua if these commands are still absent.

Purpose: make adoption and exit paths available from Neovim through auditable Rust plans.

Scope:

- Add `:ZorgImportPlan` wrapper that calls the Rust legacy import plan command with JSON output and renders a review
  buffer separating writes, warnings, lossy transforms, unsupported forms, and errors.
- Add `:ZorgImportApply` only when Rust provides explicit safe write semantics and overwrite/collision behavior.
- Add Markdown export wrappers for current zettel, subtree, and query result set using Rust JSON/text contracts.
- Open generated outputs or display stdout as appropriate without mutating `.z` buffers from Lua.
- If Rust commands are not yet available, provide explicit unavailable messages and health notes.

Acceptance:

- Tests cover import plan rendering, lossy/error sections, apply confirmation, export stdout, export output-directory
  reporting, and unavailable-command behavior.
- Lua never accepts legacy syntax as normal Zorg source and never performs import/export transforms itself.

Validation:

- From `../zorg-nvim`: import/export wrapper tests plus existing headless suite.
- From `../zorg`: targeted CLI smoke tests for import/export contracts when available.

### Phase 15.6: Command Surface, Helpers, Mappings, And Documentation Polish

Owner: one Neovim docs/API agent.

Dependencies: Phases 15.1 through 15.5, with absent Rust-gated features documented as unavailable rather than silently
omitted.

Purpose: make the completed command set coherent for users and stable for future maintenance.

Scope:

- Update `README.md` and `doc/zorg.txt` with the final command map, setup recommendations, watcher options, query-result
  navigation, refactor wrappers, import/export wrappers, and known Rust-contract gates.
- Add helper functions and optional mappings only where they are ergonomic and remain disabled by default.
- Ensure completions include new stable flags without shelling out.
- Keep docs concise and accurate: no dashboard language, no plugin-only semantics, and no promises for commands absent
  from Rust.

Acceptance:

- User docs match implemented command names and defaults.
- Optional mappings remain opt-in and do not conflict with existing defaults.
- Help tags can be generated.

Validation:

- From `../zorg-nvim`: full headless suite plus optional `stylua --check .` and
  `luacheck lua tests filetype.lua plugin ftplugin` when installed.

### Phase 15.7: Cross-Repo End-To-End Validation

Owner: one cross-repo validation agent.

Dependencies: All implementable Epic 15 phases. Rust-gated phases must either be implemented upstream or represented by
clear unavailable-state tests.

Purpose: prove `zorg.nvim` can exercise every implemented non-dashboard v1.1 feature through stable Rust contracts.

Scope:

- Extend `../zorg/tools/validate_cross_repo.sh` to run new Neovim headless tests and targeted contract checks.
- Add real-CLI cross-repo checks for watcher JSON, query JSON result buffers, LSP initialization/save-refresh state, and
  any available refactor/import/export wrappers.
- Ensure all test roots and databases are temporary fixtures, never the developer's real corpus.
- Keep cross-repo validation useful when optional local tools are missing by failing with clear prerequisite messages.

Acceptance:

- Cross-repo validation covers all implemented Epic 15 surfaces without scraping unstable human text.
- Rust, Tree-sitter, and Neovim repos remain independently testable.
- The validation script remains the documented final gate for this epic.

Validation:

- From `../zorg`: `tools/validate_cross_repo.sh`.

## Recommended Agent Sequencing

1. Run Phase 15.0 first. It defines contracts and shared tests.
2. Run Phases 15.1, 15.2, and 15.3 next. These are implementable with current Rust contracts.
3. Run Phase 15.4 only after Rust Epic 13 CLI JSON contracts exist, or limit it to unavailable-state handling.
4. Run Phase 15.5 only after Rust Epic 14 CLI JSON/text contracts exist, or limit it to unavailable-state handling.
5. Run Phase 15.6 after the command surface stabilizes.
6. Run Phase 15.7 last as the cross-repo gate.

## Final Definition Of Done

- `../zorg-nvim` exposes config/health, watcher lifecycle controls, JSON query result buffers, refactor wrappers,
  import/export wrappers, docs, and tests for every Rust contract that exists.
- Missing Rust-gated features fail clearly and are documented instead of being partially reimplemented in Lua.
- `zorg.nvim` does not implement Zorg semantics; it shells out to Rust or configures `zorg-ls`.
- All relevant headless Neovim tests pass.
- `../zorg/tools/validate_cross_repo.sh` passes once upstream Rust-gated contracts are available.

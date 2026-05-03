---
create_time: 2026-05-03 01:03:03
bead_id: zorg-4.2
tier: epic
legend_bead_id: zorg-4
status: wip
prompt: sdd/prompts/202605/epic_11_live_workspace_indexing.md
---
# Epic 11 Live Workspace Indexing Implementation Plan

## Context

Epic #2 in `sdd/legends/202605/zorg_next_features_without_dashboard.md` is Epic 11, Live Workspace Indexing. The
implementation target is primarily `../zorg`, with `../zorg-nvim` work deferred to Epic 15 except for contracts that
must be stable before Neovim wraps them.

Current baseline from inspection:

- `../zorg/crates/zorg-store` already has `Store::reindex()` with incremental behavior and tests for unchanged files,
  changed files, deletes, and renames as delete-plus-add.
- `zorg db status` and `zorg db reindex` already expose root/database/status fields through stable text output.
- `zorg-ls` currently loads a single store snapshot during initialization. If the snapshot is stale or missing, graph
  features degrade until the server is restarted after manual reindexing.
- `zorg-ls` already runs on Tokio, while `zorg-cli` is currently a synchronous CLI binary.
- There is no watcher crate/module, no `zorg watch` command, and no LSP snapshot refresh after saves.

The plan below keeps each phase independently handoffable to a distinct future agent instance. Every phase should start
with `git status --short` in every touched repo, preserve unrelated changes, and leave the touched repo passing the
phase validation commands.

## Product Goal

Users should be able to keep a Zorg SQLite index current while editing a corpus, and `zorg-ls` should pick up refreshed
graph data without a server restart. Batch workflows keep using `zorg db reindex`; live indexing is added as an explicit
long-running path with stable CLI output for future `zorg.nvim` integration.

## Non-Goals

- Do not implement Epic 12 query JSON, Epic 13 refactors, Epic 14 import/export, or Epic 15 Neovim UI.
- Do not move parsing, query, graph, or rewrite semantics into Lua.
- Do not add a dashboard, saved dashboard panels, or dashboard-specific status output.
- Do not add a broad config system as part of this epic. Use the existing `StoreOptions`/`--root`/`--db` surfaces unless
  a tiny option is required for watcher behavior.
- Do not replace the existing incremental store implementation. The watcher should feed it.

## Phase 11A: Watcher Library Contract And Event Model

Owner: one Rust watcher architecture agent.

Touched repo: `../zorg`.

Likely files:

- `../zorg/Cargo.toml`
- new `../zorg/crates/zorg-watch/Cargo.toml`
- new `../zorg/crates/zorg-watch/src/lib.rs`
- `../zorg/docs/development.md`

Scope:

- Add a small `zorg-watch` library crate to isolate long-running file watching from the synchronous CLI/store crates.
- Define public types for:
  - watcher options: root, database path, debounce duration, optional bounded run/test controls;
  - watch events: create/write/remove/rename/rescan-needed/ignored;
  - run state: starting, ready, indexing, indexed summary, degraded/error, stopping/stopped;
  - structured event sink callbacks that CLI and tests can consume.
- Select and document the implementation boundary: `notify` for filesystem events, a Tokio runtime for the long-running
  watch service, and `Store::reindex()` as the only database mutation path.
- Implement path filtering rules before any database writes:
  - include canonical `.z` files;
  - include directories only as traversal/event containers;
  - ignore `.zorg`, the configured database path, legacy non-canonical extensions, swap/temp files, hidden editor
    scratch names, and non-source files;
  - treat overflow/rescan events as a debounced full incremental `Store::reindex()` call, not a separate indexing path.
- Provide compile-tested skeleton APIs plus unit tests for path filtering and debounce scheduling with fake events.

Acceptance:

- `cargo fmt --check`
- `cargo test -p zorg-watch`
- The new crate compiles in the workspace without starting a real filesystem watcher in unit tests.
- Docs name the CLI/LSP contracts planned in later phases without promising Neovim commands yet.

## Phase 11B: Watcher Reindex Engine

Owner: one Rust store/watcher implementation agent.

Touched repo: `../zorg`.

Likely files:

- `../zorg/crates/zorg-watch/src/lib.rs`
- `../zorg/crates/zorg-watch/Cargo.toml`
- focused tests under the watcher crate

Scope:

- Implement the real watcher loop over the Phase 11A API.
- Convert filesystem events into one debounced reindex job per root/database pair.
- Keep a single writer model:
  - the watcher owns its `Store` handle while running;
  - overlapping event bursts coalesce into one queued reindex;
  - if events arrive during indexing, run one more debounced pass after the current pass completes.
- Handle common editor and VCS patterns:
  - atomic save via temp file plus rename;
  - rapid repeated writes to the same `.z` file;
  - source deletes;
  - source renames;
  - directory creates/deletes;
  - large branch-checkout bursts and watcher overflow/rescan indications.
- Emit deterministic state events suitable for CLI printing and future Neovim job parsing. At minimum: `ready`,
  `indexing`, `indexed`, `error`, and `stopped`, with root/database and summary fields where available.
- Keep shutdown deterministic: stop accepting events, finish or cancel according to documented policy, flush final
  state, and return a clear result.

Acceptance:

- `cargo fmt --check`
- `cargo test -p zorg-watch`
- Integration-style tests using temp roots prove that saves, deletes, and renames make subsequent store/query reads see
  the updated index without manual `zorg db reindex`.
- Tests use bounded watcher runs or direct service harnesses, never indefinite background processes.

## Phase 11C: `zorg watch` CLI

Owner: one Rust CLI agent.

Touched repo: `../zorg`.

Likely files:

- `../zorg/crates/zorg-cli/Cargo.toml`
- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/crates/zorg-cli/tests/smoke.rs`
- `../zorg/docs/development.md`
- `../zorg/README.md` if command summary needs refresh

Scope:

- Add `zorg watch` as the explicit long-running live-indexing entry point.
- Support stable flags:
  - `--root PATH`
  - `--db PATH`
  - `--debounce MS`
  - `--format text|json` or `--json` if the repo prefers existing JSON flag patterns
  - test-only or bounded-run control suitable for smoke tests, such as `--once`, `--exit-after-ready`, or an internal
    harness exposed only under test cfg if preferred.
- Preserve existing `zorg db reindex` behavior and output.
- Print clear ready/error/indexed states. JSON mode should be line-delimited event objects so editor clients can follow
  a running process without scraping human text.
- Exit codes:
  - `0` for clean bounded completion or signal-triggered graceful shutdown;
  - nonzero for invalid args, store open failures, watcher setup failures, or unrecoverable loop failures;
  - recoverable per-event indexing errors should be emitted as error events while the watcher keeps running if safe.
- Update help text and docs with examples and with guidance that `zorg db reindex` remains the batch/CI path.

Acceptance:

- `cargo fmt --check`
- `cargo test -p zorg-cli`
- Smoke tests verify help text, invalid args, ready output, bounded run behavior, and that changing a `.z` file during a
  bounded/harnessed watch updates `zorg db status`/query-visible index state.
- Human text output remains readable; JSON output is stable enough for Epic 15 Neovim wrappers.

## Phase 11D: LSP Snapshot Refresh On Saves

Owner: one Rust LSP agent.

Touched repo: `../zorg`.

Likely files:

- `../zorg/crates/zorg-ls/src/state.rs`
- `../zorg/crates/zorg-ls/src/main.rs`
- `../zorg/crates/zorg-ls/tests/smoke.rs`
- `../zorg/docs/lsp.md`

Scope:

- Add a conservative LSP refresh mechanism that does not require `zorg-ls` to host its own filesystem watcher.
- On `textDocument/didSave`, or on a configurable refresh trigger if save notifications are not available, run a bounded
  refresh against the configured store:
  - call `Store::reindex()` for the configured root/database;
  - reload the `StoreSnapshot`;
  - republish indexed diagnostics for affected or known files;
  - log ready/degraded transitions.
- Advertise save notifications in `textDocumentSync`.
- Keep live parse diagnostics on open/change as they are today.
- Do not block unrelated LSP requests on long indexing work longer than necessary. A simple serialized async task is
  acceptable for v1.1 as long as state updates are guarded by the existing lock and tests prove no deadlock.
- Preserve degraded behavior for missing root/database. If the database does not exist, the refresh path may create it
  by opening the store and indexing, but the behavior must be documented and tested.

Acceptance:

- `cargo fmt --check`
- `cargo test -p zorg-ls`
- Tests cover:
  - stale-at-initialize transitions to ready after save;
  - completion/navigation/references use the refreshed graph without restarting the server;
  - diagnostics are republished after refresh;
  - missing/invalid roots still degrade clearly and do not panic.

## Phase 11E: Cross-Repo Contract Validation And Handoff Docs

Owner: one cross-repo validation/docs agent.

Touched repos: primarily `../zorg`; `../zorg-nvim` only docs/contract notes if needed.

Likely files:

- `../zorg/tools/validate_cross_repo.sh`
- `../zorg/docs/development.md`
- `../zorg/docs/lsp.md`
- `../zorg/docs/cross_repo.md`
- `../zorg-nvim/README.md` or `../zorg-nvim/doc/zorg.txt` only for forward-looking command contract notes

Scope:

- Extend cross-repo validation to exercise the stable Rust surfaces that Epic 15 will consume:
  - `zorg watch --help`;
  - bounded `zorg watch` ready/indexed JSON events;
  - `zorg-ls` save-triggered refresh behavior with a temp root;
  - existing Neovim tests to ensure no regression in current setup.
- Document the final contract for future Neovim agents:
  - command argv and required flags;
  - JSON event schema and lifecycle states;
  - duplicate watcher expectations: one watcher per root/database pair;
  - how `zorg-ls` freshness is surfaced through logs/diagnostics today;
  - recommended health/status wording for Epic 15 without implementing the UI.
- Add troubleshooting notes for ignored paths, stale indexes, watcher setup errors, and branch checkout bursts.

Acceptance:

- `cargo fmt --check`
- `cargo test --workspace`
- `../zorg/tools/validate_cross_repo.sh`
- If `../zorg-nvim` docs are touched, run the existing headless Neovim tests documented in the roadmap.
- No tests or scripts use the user's real `~/zorg`; all watcher tests operate on temp roots.

## Sequencing And Agent Handoffs

The phases should run in order. Phase 11A defines the public types and prevents later agents from inventing incompatible
event/state names. Phase 11B makes the service real. Phase 11C exposes the service to users and editor clients. Phase
11D refreshes `zorg-ls` without waiting for Neovim UI work. Phase 11E locks down the cross-repo contract for Epic 15.

Each agent should hand off:

- files changed;
- commands run and their result;
- any output contract changes;
- any known limitations intentionally left for a later phase.

## Risk Controls

- Database contention: keep watcher ownership to one store handle and document that concurrent one-shot CLI writes may
  fail with ordinary SQLite locking if run simultaneously. Do not add a daemon or lock manager in this epic.
- Event flakiness: treat watcher events as hints and always run `Store::reindex()`, which already compares the full
  discovered snapshot against indexed state.
- Test stability: unit-test filtering/debounce with fake events; use bounded real watcher integration tests sparingly.
- LSP responsiveness: serialize refreshes and publish transitions, but avoid introducing a permanently running internal
  watcher in `zorg-ls` during this epic.
- Future Neovim support: stabilize JSON event names in Phase 11C and document them in Phase 11E so Epic 15 does not need
  to parse human output.

## Final Validation Matrix

At the end of Epic 11, the final agent should run:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `../zorg/tools/validate_cross_repo.sh`

If `../zorg-nvim` is touched:

- `nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"`
- any feature-specific headless tests added by that phase

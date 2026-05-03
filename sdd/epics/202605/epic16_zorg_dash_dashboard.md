---
plan_name: epic16_zorg_dash_dashboard
bead_id: zorg-5
tier: epic
recommended_epic_bead_id: zorg-4.7
source:
- sdd/research/202605/zorg_dash_dashboard_research.md
- crates/zorg-cli/src/main.rs
- crates/zorg-store/src/lib.rs
- crates/zorg-query/src/lib.rs
- crates/zorg-watch/src/lib.rs
create_time: 2026-05-03 18:28:27
status: done
prompt: sdd/prompts/202605/epic16_zorg_dash_dashboard.md
---

# Epic 16 Zorg Dash Dashboard MVP Implementation Plan

## Scope Decision

This plan covers the MVP for a new first-class `zorg dash` command that launches a terminal dashboard for an indexed
Zorg corpus.

The prior dashboard research recommends a native Rust implementation in this workspace using Ratatui with Crossterm,
rather than a sibling Python/Textual application. This plan accepts that recommendation. The dashboard should ship as
part of the existing `zorg` binary behind a default-on Cargo feature, with all parsing, querying, indexing, capture,
fix, and refactor behavior delegated to existing Rust crates.

The phase breakdown below is intentionally sequential. Each phase should be handled by a distinct agent instance, and
each phase should leave the repo in a passing, shippable state for the behavior it introduces.

## Current Baseline

Inspection of the current checkout shows:

- `crates/zorg-cli/src/main.rs` has no `dash` command. CLI dispatch is currently a single-file command router.
- `crates/zorg-cli/Cargo.toml` depends directly on the functional crates. It has no Cargo feature gate and no
  Ratatui/Crossterm dependency.
- The workspace has no `crates/zorg-dash` crate.
- `zorg-store` has useful dashboard data APIs: `Store::index_status`, `Store::discover_sources`, `Store::list_zettel`,
  `Store::list_diagnostics`, `Store::search_text`, `Store::lookup_zettel_by_canonical_id`, and the
  `zorg-query::QueryStore` adapter.
- `Store::open_with_options` currently creates the database directory, opens SQLite in read/write mode, and runs
  migrations. A high-standard dashboard should add a read-only store open path before promising read-only launch
  semantics.
- `zorg-query` already exposes structured `QueryExecutionResult` rows with IDs, paths, titles, todo markers, source
  spans, lifecycle dates, tags, and properties.
- `zorg-watch` already owns long-running watcher semantics. The dashboard MVP should not embed the watcher.
- `Cargo.lock` does not currently include Ratatui, Crossterm, or tui-input.
- Existing CLI smoke tests use temp corpora and `env!("CARGO_BIN_EXE_zorg")`, which is suitable for `zorg dash --once`
  and bounded-run tests.

## Product Goal

The MVP should make `zorg dash` useful as a daily terminal cockpit:

- launch from a shell as `zorg dash [--root PATH] [--db PATH]`;
- show corpus root, database path, index freshness, diagnostics count, and current panel at all times;
- provide dense keyboard-first panels for Today, Inbox, Search, Diagnostics, and Index;
- let users refresh, reindex with confirmation, open selected rows in `$EDITOR`, and capture a new item;
- remain responsive on large corpora by keeping disk and SQLite work off the render/event path;
- preserve terminal state on normal exit, error exit, panic, `--exit-after`, and `--once`.

## Non-Goals

- Do not build a web app, Tauri app, Textual app, Neovim-only surface, or sibling repository.
- Do not embed `zorg-watch` or create a second long-lived writer in the MVP.
- Do not edit `.z` source directly from the dashboard except through existing safe library/CLI boundaries.
- Do not introduce persistent dashboard layout configuration, plugins, dashboards-as-zettel, or theme config in the MVP.
- Do not add a machine-oriented `zorg dash --json` contract. `--once` is for deterministic rendering and smoke tests,
  not for downstream automation.
- Do not replace `$EDITOR` or host long-form editing inside the TUI.

## MVP Architecture

Add a new `crates/zorg-dash` library crate with these boundaries:

- `args`: parse `zorg dash` options independently from the CLI dispatcher while matching existing `--root`/`--db`
  precedence through `ResolvedConfig`.
- `data`: read-only store opening, query execution, diagnostics loading, index status loading, and preview text loading.
- `model`: pure dashboard state and view models: panels, rows, selection, inspector, footer bindings, index health,
  error/log messages, and pending actions.
- `actions`: refresh, reindex, open selected row in `$EDITOR`, and capture. Write-like actions must delegate to existing
  Rust crates or one-shot command paths and report failures visibly.
- `ui`: Ratatui rendering only. Rendering should be deterministic enough for `TestBackend` tests.
- `app`: event loop, async tasks, debouncing, generation counters, terminal lifecycle, and shutdown.

Wire `crates/zorg-cli` with:

```toml
[features]
default = ["dash"]
dash = ["dep:zorg-dash"]
```

and a dispatch arm:

```rust
Some("dash") => {
    #[cfg(feature = "dash")]
    { zorg_dash::run(args.collect()) }
    #[cfg(not(feature = "dash"))]
    {
        eprintln!("zorg dash: not built with the `dash` feature");
        std::process::exit(2);
    }
}
```

The MVP command surface should be:

```text
zorg dash [--root PATH] [--db PATH]
          [--panel today|inbox|search|diagnostics|index]
          [--query @id|SWOG]
          [--once]
          [--exit-after MS]
          [--no-alt-screen]
          [--no-mouse]
```

Initial panel semantics:

- Today: built from existing SWOG queries for due/do/todo work plus diagnostic attention items.
- Inbox: `#z/inbox` rows.
- Search: user-entered SWOG query, with `--query` preload and debounce while typing.
- Diagnostics: `Store::list_diagnostics` rows grouped/sorted for triage.
- Index: `Store::index_status`, schema version, discovered/indexed/stale counts, and last indexed timestamp.

## Phase 16A: Store Read-Only Foundation And Dashboard Data Contract

Owner: one Rust store/data agent.

Likely files:

- `crates/zorg-store/src/lib.rs`
- `crates/zorg-store/Cargo.toml` only if a small dependency is unavoidable
- `crates/zorg-store` tests
- optional `docs/development.md` note for store open modes

Scope:

- Add a read-only open path for the store, for example `Store::open_read_only_with_options`.
- The read-only path must not create database directories, create a missing database, mutate schema metadata, or run
  migrations. It should return a clear typed/displayable error for missing database, unreadable database, or
  incompatible schema.
- Keep existing `Store::open_with_options` behavior unchanged for command paths that need migrations or writes.
- Confirm that dashboard-needed read APIs work from a read-only connection: `schema_version`, `index_status`,
  `discover_sources`, `list_zettel`, `list_diagnostics`, `search_text`, and query adapter methods.
- Add small helper tests around missing DB, existing DB, and no-write behavior.

Acceptance:

- Existing store, query, watch, and CLI tests keep passing.
- Read-only open can inspect a previously reindexed store.
- Read-only open fails without creating files when the database does not exist.
- Dashboard agents can rely on a non-mutating launch path.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-store
cargo test --workspace
```

## Phase 16B: `zorg-dash` Crate Skeleton, CLI Feature Gate, And Deterministic Frame Harness

Owner: one Rust CLI/TUI foundation agent.

Likely files:

- `Cargo.toml`
- `Cargo.lock`
- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-cli/Cargo.toml`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `README.md` or `docs/development.md` for minimal command mention

Scope:

- Add the `zorg-dash` crate with Ratatui/Crossterm dependencies kept out of non-dashboard crates.
- Add the default-on `dash` feature to `zorg-cli` and wire the `zorg dash` dispatch arm.
- Implement strict argument parsing for the MVP command surface, including help text and usage-error exit code `2`.
- Implement a minimal dashboard frame with status bar, left nav, main panel placeholder, inspector placeholder, and
  footer. It should use real root/database/index metadata when available and a clear degraded state when the read-only
  index cannot be opened.
- Implement `--once` so tests can render one deterministic frame to stdout and exit `0`.
- Implement `--exit-after MS` so bounded interactive runs can be tested without hung CI.
- Add terminal lifecycle scaffolding: raw mode, optional alt-screen, optional mouse capture, cleanup guard, and panic
  cleanup hook.

Acceptance:

- `zorg --help` lists `dash`.
- `cargo build -p zorg-cli --no-default-features` still succeeds and prints the feature-disabled error for `zorg dash`.
- `zorg dash --once --root TMP --db TMP/zorg.sqlite3` renders a stable textual frame and exits.
- A smoke test covers `--once`; another covers a short `--exit-after` run.
- Terminal setup/teardown is contained in `zorg-dash`, not spread through `zorg-cli`.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke
cargo test --workspace
```

## Phase 16C: Index, Diagnostics, And Today Panels

Owner: one Rust dashboard model/render agent.

Likely files:

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash` tests
- `crates/zorg-cli/tests/smoke.rs`

Scope:

- Define pure view models for panels, rows, inspector content, status bar data, and footer bindings.
- Load dashboard snapshots from the read-only store:
  - Index panel from `Store::index_status` and `schema_version`.
  - Diagnostics panel from `Store::list_diagnostics`.
  - Today panel from built-in SWOG queries plus diagnostics requiring attention.
- Decide and document the exact built-in Today query strings in code tests. Keep them ordinary `zorg-query` queries, not
  dashboard-specific SQL.
- Implement inspector details for zettel rows and diagnostic rows: title, ID, path, line/column when available, tags,
  properties, todo marker, and message/preview text.
- Render useful empty states and degraded-index states.
- Add model tests for deterministic row ordering and health classification.
- Add Ratatui `TestBackend` render tests for stable status/nav/panel output.

Acceptance:

- `zorg dash --once --panel index` shows real index counts.
- `zorg dash --once --panel diagnostics` shows indexed diagnostics with paths and messages.
- `zorg dash --once --panel today` shows due/do/todo rows when fixtures contain them and does not block on missing rows.
- Rendering tests do not assert fragile full-screen snapshots unless necessary; prefer stable frame summaries and key
  lines.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
```

## Phase 16D: Interactive Navigation, Refresh, Reindex, And Open-In-Editor

Owner: one Rust TUI interaction/actions agent.

Likely files:

- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash` tests
- `crates/zorg-cli/tests/smoke.rs`

Scope:

- Implement keyboard navigation: `q`, `r`, `R`, `enter`, `tab`/`backtab`, arrows, `j`/`k`, `g`/`G`, `?`.
- Implement panel switching and selection state without layout shifts.
- Implement async/background refresh. SQLite and filesystem reads must run outside the render/event path using
  `spawn_blocking` or an equivalent bounded worker path with generation counters.
- Implement reindex with confirmation. Reindex may use a normal read/write `Store` or an existing one-shot command path,
  then refresh the read-only snapshot.
- Implement open-in-`$EDITOR` for rows with source locations. The dashboard should leave raw/alt-screen state before
  launching the editor and restore it afterward. Fallback behavior should be explicit when `$EDITOR` is unset.
- Add a help overlay and an in-app log/error overlay for failed refresh, reindex, and editor actions.

Acceptance:

- Interactive runs remain responsive while refresh/reindex is in progress.
- `--exit-after` reliably exits after the requested duration.
- Reindex requires confirmation and shows success/failure.
- Editor launch does not leave the terminal in raw mode after return or failure.
- Tests cover state transitions for navigation, confirmation, refresh completion, and failed actions.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
```

## Phase 16E: Search And Inbox Panels

Owner: one Rust query UX agent.

Likely files:

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash` tests
- optional `docs/query.md` note if dashboard search behavior needs documenting

Scope:

- Implement Inbox as a built-in query-backed panel for `#z/inbox`.
- Implement Search as a focused SWOG query input:
  - preload from `--query`;
  - support stored query IDs through `zorg_query::execute_query_by_id`;
  - debounce inline query execution while typing;
  - show parse/evaluation errors inline without tearing down the dashboard;
  - preserve selection sensibly across result refreshes.
- Add footer/help bindings for `/`, `esc`, query editing, and running a stored query.
- Keep query execution asynchronous and generation-checked so stale search results cannot overwrite newer input.
- Reuse `zorg-query` structured rows for result rendering and inspector details.

Acceptance:

- `zorg dash --once --panel inbox` shows `#z/inbox` rows from an indexed fixture corpus.
- `zorg dash --once --panel search --query '#z/inbox'` renders matching query rows.
- Invalid queries render a dashboard error state, not a process panic.
- Search results remain deterministic and consistent with `zorg query` for the same indexed corpus.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-query
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
```

## Phase 16F: Capture Action, Polish, Docs, And MVP Hardening

Owner: one Rust product-hardening agent.

Likely files:

- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/README.md`
- `docs/capture.md`
- `README.md`
- `crates/zorg-cli/tests/smoke.rs`

Scope:

- Implement a minimal capture action that delegates to `zorg-capture` instead of writing source directly.
- The MVP capture flow may be intentionally small: title/body/template/destination fields are enough. It must report the
  created path/ID and refresh the dashboard after success.
- Respect terminal color conventions: `NO_COLOR`, 256-color fallback, and a sane default palette. Avoid hard-coded
  one-note color themes.
- Finish resize behavior, degraded-index explanations, loading/error states, and empty states.
- Add documentation for `zorg dash`, including command flags, index expectations, watcher relationship, key bindings,
  and MVP non-goals.
- Audit panic/error paths for terminal cleanup.
- Keep the dashboard read-mostly by default and ensure all write actions are explicit.

Acceptance:

- `c` opens a capture flow, successful capture creates a zettel through `zorg-capture`, and the dashboard refreshes.
- Docs explain that users should run `zorg db reindex` or `zorg watch` to keep the dashboard current; the MVP does not
  silently start a watcher.
- The dashboard is usable at common terminal sizes, including narrow widths, without text overlap or layout panics.
- Final MVP supports Today, Inbox, Search, Diagnostics, and Index panels plus refresh, reindex, open, and capture.

Validation:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Definition Of Done For The MVP

The MVP is complete after Phase 16F when:

- `zorg dash` launches by default from the existing `zorg` binary.
- `zorg dash --once` and `zorg dash --exit-after` make dashboard behavior testable in CI.
- Slim builds can exclude the dashboard with `cargo build -p zorg-cli --no-default-features`.
- All MVP panels render real corpus data or clear degraded/empty states.
- The UI remains keyboard-first, dense, and operational rather than decorative.
- The dashboard does not duplicate parser/query/store semantics or mutate `.z` files through ad hoc source rewrites.
- The full workspace passes format, clippy, and test validation.

## Key Risks And Mitigations

- Terminal lifecycle bugs: isolate terminal setup in a guard type, install panic cleanup, and keep `--once` out of raw
  mode where possible.
- Store mutation on launch: land the read-only store open path before the TUI depends on live data.
- UI scope creep: keep panels query-backed, keep writes delegated, and defer dashboards-as-zettel, graph, saved layouts,
  embedded watcher, and themes.
- Slow queries on large corpora: use background tasks, debounce search, and drop stale generations.
- Snapshot fragility: test pure models heavily and assert stable render fragments rather than whole-screen buffers when
  styling is not the behavior under test.
- Two-writer behavior with `zorg watch`: do not embed the watcher in MVP; show stale/index health and route writes
  through existing one-shot code paths.
- CLI coupling: keep `zorg-cli` as a thin dispatch layer and put dashboard parsing/runtime in `zorg-dash`.

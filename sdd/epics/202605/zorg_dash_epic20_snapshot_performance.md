---
plan_name: zorg_dash_epic20_snapshot_performance_startup_tests
create_time: 2026-05-04 00:58:14
status: proposed
source:
- sdd/legends/202605/zorg_dash_next_improvements.md
- crates/zorg-dash/README.md
- crates/zorg-dash/src/lib.rs
- crates/zorg-dash/src/app.rs
- crates/zorg-dash/src/actions.rs
- crates/zorg-dash/src/data.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/ui.rs
- crates/zorg-query/src/lib.rs
- crates/zorg-store/src/lib.rs
- crates/zorg-cli/tests/smoke.rs
prompt: sdd/prompts/202605/zorg_dash_epic20_snapshot_performance.md
---

# Zorg Dash Epic 20 Implementation Plan

## Objective

Implement Epic 20 from `sdd/legends/202605/zorg_dash_next_improvements.md`: make `zorg dash` responsive at startup,
reduce snapshot refresh cost on larger corpora, expose useful lightweight telemetry, and deepen regression coverage
around realistic render and interaction cases.

This plan intentionally stops at planning. Implementation should happen phase by phase, with each phase owned by a
distinct agent instance.

## Current Baseline

The current checkout already includes substantial Epic 17-19 work:

- `crates/zorg-dash` has stable `PanelRowId`, per-panel `PanelViewport`, list rendering, status events, a log overlay,
  no-color mode, default-off mouse capture, diagnostic filters, fix preview/apply, Today todo actions, yanking, and
  pending activity spinners.
- `crates/zorg-dash/src/lib.rs` still calls `load_frame()` before entering the interactive terminal loop, and
  `load_frame()` calls `data::load_snapshot()` synchronously. Interactive startup therefore cannot draw until the full
  snapshot has loaded.
- `--once` and non-interactive stdout fallback are intentionally synchronous and deterministic today. They should stay
  that way unless a later epic adds a separate streaming/API mode.
- `data::query_zettel()` calls `Store::list_zettel()` every time it executes a query in order to build preview text.
  `load_today()` runs three built-in queries, then `load_ready_snapshot()` separately runs Inbox and optional Search, so
  a single snapshot can load full-corpus preview text four or five times.
- `zorg-query` already has `list_zettel_for_query(include_body_text)` behavior through `QueryStore`, but
  `QueryResultRow` does not include preview/body text. A dashboard-local preview cache is likely the smallest safe
  change for Epic 20.
- The app tracks elapsed durations in status details, but it does not keep a first-class telemetry model with refresh
  count, last refresh/search/action durations, row counts, or index inspector display.
- Render coverage is broad at the unit level, but there is no dedicated large overflow fixture/golden suite and no
  explicit before/after measurement harness for snapshot loading cost.

## Phase Count Decision

Use six phases. The legend suggested four, but Epic 20 mixes architecture, performance, telemetry, and test-depth work.
Separating the measurement harness from behavior changes gives later agents a baseline before optimizing, and separating
golden/edge coverage from the performance changes keeps test failures easier to diagnose.

The phases should land in order:

1. Measurement and overflow fixture foundation.
2. Interactive async initial loading frame.
3. Snapshot preview memoization and query-load optimization.
4. Telemetry model and Index/status display.
5. Golden frame and interaction edge coverage.
6. Large-corpus performance validation, docs, and final smoke hardening.

## Cross-Phase Design

Keep the dashboard architecture boundaries from the legend:

- `data.rs` owns snapshot loading and preview-cache construction. It should expose small testable units for snapshot
  load metrics instead of letting app tests infer performance through wall-clock sleeps.
- `app.rs` owns worker scheduling, pending-operation guards, generation checks, and telemetry updates. Startup loading
  should use the same worker/result path as refresh, search, reindex, capture, fix, and todo apply.
- `model.rs` owns any new loading or telemetry state. Prefer explicit model types such as `DashboardSnapshot::Loading`
  or `DashboardTelemetry` over status strings that the renderer has to parse.
- `ui.rs` remains render-only and should render loading/degraded/ready states from explicit model data.
- `lib.rs` owns CLI behavior. Keep `--once` synchronous and deterministic; make only true interactive terminal startup
  draw before the initial snapshot completes.

Avoid adding a heavyweight benchmarking framework unless there is already a local pattern. Test helpers that generate a
temporary large corpus and count load operations are enough for Epic 20. Performance tests with wall-clock thresholds
should be conservative and preferably isolated from normal unit tests when they would be flaky.

## Phase 20A: Measurement And Overflow Fixture Foundation

### Scope

Create the test and measurement scaffolding needed to prove later phases improved the dashboard without changing user
behavior yet.

### Likely Files

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/tests` if integration tests are split out
- `crates/zorg-cli/tests/smoke.rs` only for helper reuse if needed
- new dashboard test fixture helpers, preferably generated temporary corpora rather than a huge checked-in corpus

### Technical Shape

- Add a generated overflow corpus helper for dashboard tests: many zettel rows, due/do/open todos, inbox tags,
  diagnostics-triggering rows, and enough rows to overflow the default `--once` main area.
- Add a lightweight snapshot-load instrumentation hook that can count expensive full-corpus preview collection calls
  during tests. Keep production behavior unchanged.
- Add test helpers for rendering specific panels at `100x28` and narrow sizes, normalizing the frame text where needed.
- Capture current baseline expectations in tests that are intentionally scoped to scaffolding: large corpus can be
  indexed and rendered, and instrumentation can observe existing preview-load multiplication.

### Acceptance

- The generated overflow corpus is deterministic and does not rely on system date except where tests inject or derive
  the current query date explicitly.
- Tests can render Today, Inbox, Search, Diagnostics, and Index against the overflow corpus.
- Tests or helper assertions can count how often preview collection is requested during a snapshot load.
- No user-visible dashboard behavior changes in this phase.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 20B: Interactive Async Initial Loading Frame

### Scope

Make true interactive startup draw immediately with a loading/degraded frame while the initial snapshot loads on the
worker channel.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`

### Technical Shape

- Split config resolution from snapshot loading in `lib.rs`.
- Add an explicit initial frame constructor that knows root, database, panel, and query but starts with loading state. A
  `DashboardSnapshot::Loading` variant is acceptable if it keeps degraded/ready handling clear.
- Enter the terminal and draw the loading frame before starting or while starting the initial snapshot worker.
- Add `PendingOperationKind::InitialLoad` or reuse refresh only if labels, telemetry, and generation handling remain
  unambiguous.
- Add `AsyncResult::InitialLoad` so the first result can replace loading state, sync all viewports, record elapsed time,
  and preserve initial panel/query.
- Keep `--once` and non-terminal stdout fallback synchronous, because scripts expect one complete frame.
- Decide how keys behave while loading: quit/help/log should work; row actions should report that data is still loading;
  refresh should either be ignored politely or coalesced behind the initial load.

### Acceptance

- Interactive `zorg dash` can render a loading frame before expensive snapshot work completes.
- `--once` still renders a complete ready/degraded frame and exits.
- `--exit-after --no-alt-screen` bounded runs still terminate cleanly while an initial load is pending or after it
  completes.
- Initial load success and failure both produce status/log events with elapsed timing.
- Terminal cleanup still runs on normal exit and panic.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3 --exit-after 50 --no-alt-screen --no-mouse
```

## Phase 20C: Snapshot Preview Memoization And Query Load Optimization

### Scope

Eliminate repeated full-corpus preview collection within one snapshot load.

### Likely Files

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs` only if rows need minor field adjustments
- `crates/zorg-store/src/lib.rs` or `crates/zorg-query/src/lib.rs` only if a narrow shared API proves cleaner than a
  dashboard-local cache

### Technical Shape

- Introduce a dashboard-local `PreviewCache` or `SnapshotLoadContext` built once per `load_ready_snapshot()`.
- Pass that cache into Today's three built-in queries, Inbox, and optional Search instead of calling
  `Store::list_zettel()` inside every `query_zettel()`.
- Keep `load_search_panel()` efficient by building at most one preview cache for that standalone search.
- Prefer using existing store/query APIs. If adding a shared API, keep it narrow, documented, and covered by
  `zorg-store` or `zorg-query` tests.
- Preserve existing `ZettelRow.preview` behavior and ordering.
- Use Phase 20A instrumentation to assert that Today snapshot loading does not collect full-corpus previews once per
  built-in query.

### Acceptance

- A ready snapshot with Today, Inbox, and optional Search performs one full-corpus preview collection, not one per
  query.
- Standalone search performs at most one preview collection.
- Query results, previews, row ordering, badges, and search errors remain behaviorally unchanged.
- Degraded snapshot behavior remains fail-closed and clear when the store cannot be opened or queried.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-query
cargo test -p zorg-store
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3 --once --panel today
```

## Phase 20D: Telemetry Model And Index/Status Display

### Scope

Make performance and operational feedback first-class instead of transient status text.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/data.rs` if snapshot load metrics become part of telemetry
- `crates/zorg-dash/README.md`

### Technical Shape

- Add a `DashboardTelemetry` model with fields such as:
  - refresh count
  - last initial-load duration
  - last refresh duration
  - last search duration
  - last action kind and duration
  - current row counts by panel
  - optional preview-cache/load counters in test/debug builds if useful
- Update telemetry on initial load, refresh, search, reindex, capture, fix apply, todo apply, and failed operations
  where elapsed time is known.
- Render compact telemetry in the Index inspector and possibly a one-line status summary without crowding key help.
- Keep status events as human-readable history; telemetry should be structured data used by render tests and future JSON
  export work.

### Acceptance

- Index inspector shows refresh/search/action timing and row counts when available.
- Status/log messages continue to include useful elapsed details.
- Telemetry remains deterministic enough for render tests by formatting durations through existing helper logic.
- Failed operations record the relevant last action duration without pretending row counts changed.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3 --once --panel index
```

## Phase 20E: Golden Frame And Interaction Edge Coverage

### Scope

Add realistic regression tests for frame layout and interactive state-machine gaps called out by Epic 20.

### Likely Files

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/tests` if integration tests are split out
- `crates/zorg-cli/tests/smoke.rs`

### Technical Shape

- Add snapshot/golden-style render tests for each panel against the overflow corpus. Prefer stable text snapshots or
  focused normalized frame assertions over brittle full-buffer comparisons when dynamic values are present.
- Cover narrow capture, fix, todo, yank, diagnostic-filter, help, and log overlays.
- Cover rapid refresh/pending guards, including behavior while initial load is pending.
- Expand CLI smoke coverage where missing for `--no-mouse`, `--no-alt-screen`, degraded index, empty corpus, and bounded
  interactive runs.
- Add feasible panic cleanup coverage around the cleanup hook or terminal guard behavior without requiring an actual
  terminal panic in CI.

### Acceptance

- Golden/snapshot-style tests would fail on meaningful layout regressions in all five current panels.
- Overlay tests cover narrow terminal widths without panics or obvious section overlap.
- Pending refresh/initial-load guards are explicit and covered.
- CLI flag behavior remains documented by smoke tests.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 20F: Large-Corpus Performance Validation, Docs, And Final Hardening

### Scope

Run and document before/after performance expectations, then close remaining Epic 20 acceptance gaps.

### Likely Files

- `crates/zorg-dash/README.md`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/tests` or focused ignored performance test/module
- `crates/zorg-cli/tests/smoke.rs`

### Technical Shape

- Add a conservative large-corpus validation path: either an ignored test, a documented cargo command, or a small helper
  used by normal tests at a modest size and by manual profiling at a larger size.
- Record expected performance envelopes for `--once` and bounded interactive startup on the generated corpus. Avoid
  tight CI wall-clock thresholds unless the repo already has a stable precedent.
- Update `crates/zorg-dash/README.md` with the new loading behavior, telemetry surface, and any performance validation
  command.
- Run the full workspace test suite after all Epic 20 phases have landed.

### Acceptance

- A materially larger generated corpus completes `--once` and bounded interactive runs within documented expectations.
- The plan's preview-load instrumentation demonstrates the intended before/after shape.
- README behavior matches the implemented startup, telemetry, and testing surfaces.
- `cargo test --workspace` passes at the end of the epic.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic20.sqlite3 --once --panel today
```

## Dependency Notes And Risks

- Phase 20B should not make `--once` async. Keeping one-shot rendering complete and synchronous avoids breaking scripts
  and existing smoke tests.
- Phase 20C should start with a dashboard-local cache. Adding preview/body fields to `zorg-query::QueryResultRow` would
  widen a shared API and should only happen if it clearly simplifies the code.
- Telemetry should not be serialized as a public API in this epic. Epic 22 owns `--once --json`; Epic 20 should create
  structured in-memory data that Epic 22 can later export.
- Full-screen golden files can become noisy. Normalize dynamic paths/durations or assert selected stable frame regions.
- Large-corpus performance tests can be flaky on shared CI. Keep strict operation-count assertions in normal tests and
  use conservative/manual wall-clock profiling for larger corpora.

## Done Criteria For Epic 20

- Interactive startup can show a loading frame before full snapshot work completes.
- Today snapshot loading no longer gathers full-corpus previews once per built-in query.
- Refresh/search/action timing and row counts are visible through structured telemetry and dashboard UI.
- Overflow corpus frame tests cover every current panel.
- Edge coverage includes pending guards, narrow overlays, degraded/empty states, mouse/alt-screen flags, bounded runs,
  and feasible cleanup behavior.
- Final validation commands pass, including `cargo test --workspace`.

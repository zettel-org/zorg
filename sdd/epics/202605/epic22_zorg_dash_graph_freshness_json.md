---
plan_name: epic22_zorg_dash_graph_freshness_json
tier: epic
legend_bead_id: zorg-6
legend: sdd/legends/202605/zorg_dash_next_improvements.md
epic: 22
created: 2026-05-04
status: proposed
source:
- sdd/legends/202605/zorg_dash_next_improvements.md
- crates/zorg-dash/README.md
- crates/zorg-dash/Cargo.toml
- crates/zorg-dash/src/lib.rs
- crates/zorg-dash/src/app.rs
- crates/zorg-dash/src/actions.rs
- crates/zorg-dash/src/data.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/ui.rs
- crates/zorg-store/src/lib.rs
- crates/zorg-watch/src/lib.rs
- crates/zorg-cli/tests/smoke.rs
create_time: 2026-05-04 03:11:34
prompt: sdd/prompts/202605/epic22_zorg_dash_graph_freshness_json.md
---

# Epic 22: Graph Context, Freshness Awareness, And JSON Frame Export

## Goal

Implement Epic 22 from `sdd/legends/202605/zorg_dash_next_improvements.md`: enrich `zorg dash` with bounded graph
context, make snapshot freshness visible without embedding a watcher, and add a one-shot structured JSON frame export
for scripts and regression tests.

The epic should preserve the existing product boundary: `zorg dash` remains a native read-mostly Ratatui dashboard.
Interactive browsing stays human-oriented; `--once --json` is a bounded frame export, not a long-lived automation API.

## Current Baseline

The current checkout already includes the dashboard improvements from prior epics:

- `crates/zorg-dash` has Today, Inbox, Queries, Search, Diagnostics, and Index panels.
- `DashboardFrame` carries root, database path, active panel, query, diagnostic filters, telemetry, marked diagnostic
  state, and a `DashboardSnapshot`.
- `DashboardSnapshot::Ready` carries `IndexPanel`, diagnostics, Today rows, Inbox rows, `QueryPanel`, and `SearchPanel`.
- `PanelRowId`, per-panel `PanelViewport`, scroll preservation, status events, log overlay, no-color handling, default
  off mouse capture, and narrow render tests are already present.
- Interactive startup already renders `DashboardSnapshot::Loading` and completes snapshot work asynchronously.
- Snapshot loading uses `SnapshotLoadContext` to collect preview text once per snapshot.
- `DashboardTelemetry` already tracks refresh count, last initial load, refresh/search/action durations, and panel row
  counts; the Index inspector already includes telemetry.
- `zorg-store` already exposes the core graph APIs needed by Epic 22: `list_outgoing_links`, `list_incoming_links`,
  `list_zettel_ancestors`, and `list_zettel_descendants`.
- `StoredLink` includes unresolved outgoing link information through `resolved`, `target_zettel_id`,
  `target_canonical_id`, and `target_text`.
- `IndexStatus` includes `last_indexed_at_unix_ms`, plus changed/new/deleted counts that can be used for local health.
- `zorg-watch` exposes in-process `WatchState` and `WatchStateKind` types and `zorg watch --format json` streams those
  states, but there is no durable watcher-status file, socket, lock, or shared read-only status contract that
  `zorg dash` can inspect safely.
- `zorg-dash` currently has no JSON serialization dependency and `run_inner` always renders `--once` through Ratatui
  text output.

## Scope Decisions

Use six phases. The legend suggested four, but splitting graph data from graph rendering, freshness checks from
auto-refresh scheduling, and JSON model from CLI export gives each future agent a clean ownership boundary.

The final phase split is:

1. Graph-neighborhood data model and loading.
2. Graph inspector rendering and documentation.
3. Freshness detection and health model.
4. Idle auto-refresh scheduling.
5. JSON frame view model and serializer.
6. CLI `--once --json`, smoke coverage, and final docs.

Watcher status integration is intentionally deferred unless a stable status surface is added before or during this epic.
Do not spawn `zorg watch`, tail watcher logs, parse streaming CLI output, or infer process liveness from lock files in
Epic 22. A later epic can add watcher status after `zorg-watch` or `zorg-store` exposes a read-only, durable contract.

## Cross-Phase Design

Keep responsibilities aligned with the existing dashboard architecture:

- `model.rs` owns graph context view models, freshness state, auto-refresh configuration/state, JSON frame structs if
  they are dashboard-specific, and any new row/inspector metadata.
- `data.rs` owns read-only store access for graph neighborhoods and index freshness checks. It should return explicit
  partial/error states rather than degrading the entire dashboard for a single selected row graph failure.
- `app.rs` owns idle timing, pending-operation guards, generation checks, freshness polling, auto-refresh scheduling,
  and status/log events.
- `ui.rs` owns Ratatui rendering only: bounded graph sections, freshness hints, auto-refresh status, and any new Index
  rows.
- `lib.rs` owns CLI flag parsing and output selection. JSON output should be available only for one-shot frames, not
  interactive mode.
- `crates/zorg-store/src/lib.rs` should only change if a narrow helper materially reduces dashboard query cost or
  removes duplicated endpoint lookup code.

Use deterministic and bounded behavior:

- Bound graph sections with constants such as 8 rows per section in inspectors and separate counts for total rows.
- Preserve unresolved outgoing links instead of filtering them out.
- Avoid loading graph neighborhoods for every row during snapshot load if that would scale poorly. Prefer lazy loading
  for the selected row in interactive mode, plus synchronous selected-panel enrichment for `--once` if needed for stable
  output.
- Auto-refresh should never run while search/capture/todo prompts are editing, while an overlay that requires user
  confirmation is open, or while any worker operation is pending.
- JSON frame output should expose stable field names and plain data. It should not serialize terminal layout, colors, or
  Ratatui buffers.

## Phase 22A: Graph-Neighborhood Data Model And Loading

### Objective

Create the read-only graph context model and loader for a selected zettel row without changing the TUI layout yet.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-store/src/lib.rs` only if endpoint lookup needs a focused public helper
- `crates/zorg-dash/Cargo.toml` only if test helpers require no new runtime dependencies

### Main Work

- Add dashboard graph model types such as:
  - `GraphNeighborhood`.
  - `GraphSection<T>` with `total_count`, bounded `rows`, and `truncated_count`.
  - `GraphLinkRow` for outgoing and incoming links.
  - `GraphZettelRow` for ancestors and descendants.
  - `GraphLoadState` for unavailable, loading, ready, and failed states if interactive lazy loading needs explicit
    state.
- Represent unresolved outgoing links explicitly with target text, link kind, source span, and a clear unresolved flag.
- For resolved links, include enough endpoint information for display: target/source canonical ID when available, title
  if cheaply available, relative path, and source position.
- Load neighborhoods through existing `Store` APIs:
  - `list_outgoing_links(selected.store_id)`.
  - `list_incoming_links(selected.store_id)`.
  - `list_zettel_ancestors(selected.store_id)`.
  - `list_zettel_descendants(selected.store_id)`.
- Add any narrow `zorg-store` helper only if the dashboard otherwise has to do repeated full-index scans to resolve
  incoming link source titles or outgoing target titles.
- Ensure graph loading is row-scoped and bounded at the model boundary. The loader may count full sections but should
  keep only the display limit in the dashboard model.
- Add data/model tests for:
  - resolved outgoing links.
  - unresolved outgoing links.
  - incoming backlinks.
  - ancestor ordering from root-most to direct parent.
  - descendant ordering in file/source order.
  - high-degree truncation counts.
  - per-row failure isolation.

### Acceptance

- A selected `ZettelRow` can be mapped to a bounded `GraphNeighborhood`.
- Unresolved outgoing links survive in the model with explicit unresolved status.
- Graph loading does not require loading graph details for every row in the snapshot.
- Existing snapshot, query, and panel tests still pass.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-store
cargo test -p zorg-dash
```

## Phase 22B: Graph Inspector Rendering And Documentation

### Objective

Show graph context in the zettel inspector for Today, Inbox, and Search rows with bounded, readable sections.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`

### Main Work

- Decide how the inspector receives graph data:
  - Interactive mode should lazily request graph context for the selected zettel row and cache it by stable row identity
    and snapshot generation.
  - `--once` can either synchronously load graph context for the selected row or omit it with a documented "not loaded"
    state; prefer synchronous selected-row loading if it stays cheap and deterministic.
- Extend zettel inspector lines with sections:
  - Outgoing links.
  - Incoming backlinks.
  - Ancestors.
  - Descendants.
- Render each section with total counts, bounded example rows, and truncation markers.
- Mark unresolved outgoing links plainly, for example `unresolved @missing-id`.
- Keep the inspector compact. Do not wrap graph rows into multi-line blocks unless the layout already supports it.
- Ensure graph failures show a local inspector message and status/log event rather than degrading the whole dashboard.
- Add render tests for graph sections at normal and narrow widths.
- Update README/help text only where users need to know that the inspector includes bounded graph context.

### Acceptance

- Selecting a zettel row can show outgoing links, incoming backlinks, ancestors, and descendants.
- Unresolved outgoing links are visible as unresolved.
- High-degree graph sections show counts and truncation markers without freezing or overflowing the UI.
- Non-zettel rows keep their existing inspector behavior.
- Narrow render tests show no overlap between graph context, status, and footer.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3 --once --panel inbox
```

## Phase 22C: Freshness Detection And Health Model

### Objective

Detect when the current dashboard snapshot is older than the index database and surface that state without refreshing
automatically yet.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`

### Main Work

- Add explicit freshness model types, for example:
  - `SnapshotFreshness::Unknown | Current | NewerIndexAvailable | StaleSources | CheckFailed`.
  - `IndexGeneration` or equivalent based on `IndexStatus.last_indexed_at_unix_ms` plus stable status counts.
- Capture an index generation when a snapshot is loaded.
- Add a read-only freshness check in `data.rs` that opens the store and compares current index status to the frame's
  captured generation.
- Run freshness checks from `app.rs` on a conservative idle interval, separately from refresh.
- Show a "newer index available" hint when the SQLite index changed after the current snapshot was loaded.
- Keep existing Index health semantics for source staleness (`new_files`, `changed_files`, `deleted_files`) and add
  freshness as a separate concept so users can distinguish "index has newer data than this frame" from "index is stale
  relative to source files".
- Include freshness details in the Index inspector and status/log events.
- Ensure check failures are non-fatal and do not replace the current snapshot.

### Acceptance

- If an external `zorg db reindex` updates the database after dashboard snapshot load, the dashboard can report that a
  newer index is available.
- Freshness checks are read-only and do not start reindexing.
- Freshness failure leaves the current dashboard usable and records a warning.
- Index inspector distinguishes snapshot freshness from source/index health.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-store
cargo test -p zorg-cli --test smoke dash
```

## Phase 22D: Idle Auto-Refresh Scheduling

### Objective

Optionally refresh from a newer index while the dashboard is idle, using conservative generation and pending-operation
guards.

### Likely Files

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`

### Main Work

- Add CLI options for auto-refresh with conservative defaults. A reasonable shape is:
  - `--auto-refresh MS` to opt in with a minimum interval guard.
  - `--no-auto-refresh` if the implementation chooses a default-on mode. Prefer default-off unless product direction
    changes during implementation.
- Store auto-refresh configuration in `DashOptions` and app state.
- Trigger auto-refresh only when all are true:
  - freshness says a newer index is available or the interval check says a refresh is due.
  - no initial load, refresh, reindex, capture, search, fix, todo, or graph load operation is pending.
  - the user is not editing search/capture/todo/filter prompts.
  - no confirmation overlay is open.
- Use existing `start_refresh` machinery so generation checks, telemetry, row count deltas, and selection preservation
  remain centralized.
- Add status/log events that explain skipped auto-refresh when useful, but avoid noisy repeated logs.
- Render auto-refresh configuration and last auto-refresh/skipped state in the Index inspector.
- Update help and README with the read-only behavior and idle guards.

### Acceptance

- Auto-refresh never runs while the user is editing a prompt or while another action is pending.
- Auto-refresh uses the same snapshot replacement path as manual refresh.
- Successful auto-refresh preserves row identity/viewport behavior where possible.
- `--once` ignores or rejects auto-refresh options consistently because one-shot output should not start timers.
- Tests cover due/not-due, blocked-by-editing, blocked-by-pending-operation, and successful scheduling cases.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3 --exit-after 250 --no-alt-screen --auto-refresh 1000
```

## Phase 22E: JSON Frame View Model And Serializer

### Objective

Create a structured frame model for one-shot JSON output without coupling scripts to Ratatui text rendering.

### Likely Files

- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/lib.rs`
- possibly a new `crates/zorg-dash/src/json.rs`

### Main Work

- Add `serde` and `serde_json` dependencies to `zorg-dash` if JSON serialization lives there.
- Define explicit serializable types rather than deriving serialization on every internal model type. Suggested shape:
  - `DashboardJsonFrame`.
  - `DashboardJsonPanel`.
  - `DashboardJsonRow`.
  - `DashboardJsonInspector`.
  - `DashboardJsonTelemetry`.
  - `DashboardJsonFreshness`.
  - `DashboardJsonHealth`.
- Include:
  - schema/version marker for the dashboard JSON frame.
  - root and database path.
  - active panel and selected row index/ID.
  - query input and diagnostic filters where relevant.
  - row counts for all panels.
  - active panel rows with stable row kind and fields.
  - selected row inspector fields, including graph context when available for a selected zettel.
  - telemetry: snapshot time/generation, initial load/refresh/search/action durations, refresh count, and row counts.
  - health/freshness: source/index health and newer-index availability.
- Keep row data concise. Do not include full source files or unbounded body text.
- Ensure JSON output for degraded and loading-like one-shot states is still valid and includes useful errors.
- Add serializer unit tests that assert key field presence and stable row ordering without over-constraining every
  field.

### Acceptance

- Dashboard JSON is produced from an explicit view model, not terminal buffer scraping.
- JSON includes selected panel rows, row counts, telemetry, filters, health, and freshness.
- JSON does not expose ANSI escapes, Ratatui layout artifacts, or unbounded source content.
- Degraded frames serialize cleanly.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
```

## Phase 22F: CLI `--once --json`, Smoke Coverage, And Final Docs

### Objective

Expose the JSON frame through `zorg dash --once --json`, document the contract boundaries, and harden smoke coverage.

### Likely Files

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`
- `sdd/epics/202605` only if implementation agents maintain epic status docs

### Main Work

- Add `--json` parsing to `DashOptions`.
- Accept `--json` only with `--once`, or make non-interactive stdout fallback require `--once --json`; prefer rejecting
  interactive `--json` with a usage error to avoid implying a streaming API.
- Route `--once --json` through the JSON serializer and print pretty or compact JSON. Prefer compact JSON for scripts;
  tests can parse it through `serde_json`.
- Keep existing text `--once` output byte-for-byte stable where possible.
- Add usage tests for:
  - `--json` without `--once` fails with a clear usage error.
  - duplicate `--json` fails like other singleton flags.
  - `--once --json --panel today` succeeds and parses as JSON.
  - `--once --json --panel diagnostics` includes diagnostic rows.
  - degraded index JSON includes root/database/error.
- Add CLI smoke tests that parse JSON and assert high-value fields rather than string-matching raw JSON.
- Document:
  - `--once --json` is a one-shot frame export.
  - It is not a replacement for `zorg query --json` or `zorg watch --format json`.
  - Graph sections are bounded.
  - Auto-refresh and watcher integration boundaries.

### Acceptance

- `zorg dash --once --json --panel today` emits valid JSON and exits successfully.
- Interactive `zorg dash --json` is rejected or clearly unsupported.
- Text `--once` rendering remains available and covered by existing tests.
- README documents graph context, freshness hints, auto-refresh behavior, and JSON frame export.
- Full dashboard and CLI smoke suites pass.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-store
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic22.sqlite3 --once --json --panel today
```

## Explicitly Deferred

- Full terminal graph visualization.
- Embedded watcher writer mode.
- Spawning or supervising `zorg watch` from `zorg dash`.
- Watcher active/inactive/degraded status until there is a stable read-only status contract.
- Long-lived dashboard API, streaming JSON, sockets, or RPC.
- Persisted dashboard state; Epic 23 owns persistence.
- Dashboard-as-zettel custom panels; Epic 23 owns `--as @dashboard/id`.

## Overall Done Criteria

Epic 22 is complete when:

- Zettel inspectors show bounded outgoing links, incoming backlinks, ancestors, and descendants.
- Unresolved outgoing links are visible and clearly marked.
- Graph loading is lazy or otherwise bounded enough that high-degree zettel do not freeze rendering.
- The dashboard can detect and surface when a newer index exists than the current snapshot.
- Optional auto-refresh respects idle/pending/editing guards and uses the existing refresh path.
- Index inspector and JSON output include telemetry, row counts, filters, health, and freshness.
- `zorg dash --once --json` emits a documented, parseable one-shot frame for the selected panel.
- Existing text `--once` behavior remains intact.
- Required validation commands pass.

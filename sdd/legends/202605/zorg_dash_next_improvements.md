---
plan_name: zorg_dash_next_improvements
tier: legend
legend_bead_id: zorg-6
source:
- sdd/research/202605/zorg_dash_next_improvements_research.md
- sdd/research/202605/zorg_dash_dashboard_research.md
- sdd/epics/202605/epic16_zorg_dash_dashboard.md
- crates/zorg-dash/README.md
- crates/zorg-dash/src/lib.rs
- crates/zorg-dash/src/app.rs
- crates/zorg-dash/src/actions.rs
- crates/zorg-dash/src/data.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/ui.rs
- crates/zorg-fix/src/lib.rs
- crates/zorg-query/src/lib.rs
- crates/zorg-store/src/lib.rs
create_time: 2026-05-03 22:13:17
status: proposed
prompt: sdd/prompts/202605/zorg_dash_next_improvements.md
---

# Zorg Dash Next Improvements Multi-Epic Plan

## Scope Decision

This plan covers the post-MVP `zorg dash` improvements recommended by
`sdd/research/202605/zorg_dash_next_improvements_research.md`.

The work is intentionally split into seven epics after the completed Epic 16 dashboard MVP. Each epic is sized so a
later planner can split it into several implementation phases, and each phase can be handled by a distinct agent
instance. The epics are ordered by dependency and product value: first make the current dashboard navigable and
observable, then make diagnostics and Today actionable, then add new dashboard surfaces and persistence.

The product direction remains unchanged from Epic 16:

- Keep `zorg dash` as a native Rust TUI inside this workspace.
- Keep dashboard launch read-mostly and read-only for normal browsing.
- Route parser, query, fix, refactor, capture, and store semantics through shared Rust crates.
- Avoid a web/Tauri/Textual rewrite, embedded long-form editor, theme system, or embedded watcher writer mode.

## Current Baseline

The current checkout already contains the MVP dashboard:

- `crates/zorg-dash` implements Today, Inbox, Search, Diagnostics, and Index panels.
- `crates/zorg-cli/src/main.rs` dispatches `zorg dash` behind the default-on `dash` feature.
- The dashboard opens the store through `Store::open_read_only_with_options`.
- Today is built from built-in SWOG queries for due, do, and open todos, then augmented with diagnostics.
- Search accepts inline SWOG or stored `@query/id` through `zorg_query::execute_query_by_id`.
- Diagnostics can be listed and opened in `$EDITOR`, but not previewed or fixed.
- Capture delegates to `zorg-capture`, but only through a small one-template form.
- The event loop has worker threads for refresh, reindex, capture, and search.
- `--once` and `--exit-after` make rendering and bounded interactive runs testable.

The main constraints found in the current code are:

- `ui.rs` renders main rows as `Paragraph` lines, so there is no viewport, scroll offset, page movement, or true row
  virtualization.
- `AppState` stores only a selected index per panel; selection preservation is partial and not backed by a general row
  identity model.
- Footer status overwrites key help, and status history is not retained.
- Color/severity styling is effectively absent.
- Mouse capture is enabled by default but mouse events are not consumed.
- Initial snapshot loading is synchronous before first draw.
- `data::query_zettel` calls `Store::list_zettel()` to build preview text on every query execution, which multiplies
  refresh cost for Today, Inbox, and Search.
- Capture and Search input editing only append/pop at the end of fields.
- Existing store APIs are available for graph context: `list_outgoing_links`, `list_incoming_links`,
  `list_zettel_ancestors`, and `list_zettel_descendants`.
- Existing query APIs are available for stored query execution and definition lookup: `query_definition_by_id` and
  `execute_query_by_id`.
- `zorg-fix` owns safe fix-plan types, but dashboard-facing diagnostic-to-fix preview/apply work still needs a clean
  boundary.

## Cross-Epic Architecture

The dashboard should grow along stable boundaries rather than accumulating feature logic in the event loop:

- `model`: own panel definitions, stable row identities, viewport state, filters, selection sets, status/log state,
  input buffers, overlay state, and JSON/text frame view models.
- `data`: own snapshot loading, preview memoization, saved-query loading, graph-neighborhood loading, telemetry input,
  freshness checks, and read-only store access.
- `actions`: own explicit write-like operations: reindex, capture, diagnostic fix apply, todo action apply, and
  clipboard/export helpers. Actions should delegate to shared crates or to small shared planners, not ad hoc UI string
  edits.
- `ui`: own Ratatui rendering only. It should render from explicit view models with bounded dimensions and should be
  testable through `TestBackend`.
- `lib/app`: own CLI options, terminal lifecycle, event routing, async worker scheduling, generation checks, and
  startup/shutdown behavior.

Before adding more panels, introduce reusable primitives:

- Stable `PanelRowId` values for zettel rows, diagnostics, index rows, saved queries, graph rows, and custom dashboard
  rows.
- Per-panel viewport state: selected index, scroll offset, visible row count, total count, optional local filter,
  optional sort mode, optional marked row IDs.
- A reusable single-line input widget model with cursor movement, deletion operations, paste-safe insertion, and
  `KeyEventKind` filtering.
- A bounded status log ring with timestamps and severity.
- A test fixture corpus large enough to overflow visible list height.

## Epic 17: Dashboard Usability Foundation

### Product Goal

Make the existing dashboard panels usable on real corpora before expanding the dashboard surface. Users should be able
to scroll, see where they are, read severity at a glance, keep key help visible, and trust that terminal behavior is
intentional.

### Scope

Implement the shared list, viewport, status, styling, and mouse foundation:

- Replace main-row `Paragraph` rendering with Ratatui `List` or `Table` rendering plus explicit state.
- Add per-panel viewport state with selected index, scroll offset, visible row count, and total row count.
- Show compact position text such as `7/43` in the main panel title or footer.
- Preserve selection across refresh by stable row identity where possible, not only by numeric index.
- Add page up/down and half-page movement while preserving `j/k`, arrows, `g/G`.
- Prefer one row per item with truncation; put full message/body text in the inspector.
- Add diagnostic severity and status color styling, honoring `NO_COLOR` and a new `--no-color`.
- Split footer/status so key help remains visible while status is shown separately.
- Replace single `AppState.status` with a bounded `StatusEvent` ring and a log overlay.
- Either wire mouse wheel/click events into viewport scrolling and focus, or default mouse capture off until useful
  mouse behavior exists. Do not keep default-on mouse capture that consumes terminal text selection without benefit.
- Add first-run and empty-index guidance that tells users what root/database were resolved and how to reindex.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`
- new or expanded dashboard render fixtures/tests

### Suggested Phase Split

- Phase 17A: row identity and viewport model in `model.rs`/`app.rs`, with navigation tests.
- Phase 17B: migrate `ui.rs` main panels to `List`/`Table`, add row counts and narrow-terminal render tests.
- Phase 17C: color/no-color handling, severity styles, persistent key footer, status log ring, and log overlay.
- Phase 17D: mouse decision and first-run/empty-state guidance, with CLI smoke coverage.

### Acceptance

- Moving selection beyond the visible bottom scrolls the list.
- `--once` frames for a fixture with more rows than visible height show clear row counts and no row wrapping collisions.
- Refresh/search preserve selection when the same zettel or diagnostic remains present.
- Narrow terminal render tests pass without overlapping sections.
- Error/warning/info diagnostics are distinguishable unless `NO_COLOR` or `--no-color` is active.
- Key help remains visible after refresh, reindex, capture, search, and error states.
- The log overlay shows recent events with enough context to understand action outcomes.
- Mouse capture behavior is intentional and documented.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel diagnostics
```

## Epic 18: Actionable Diagnostics And Fix Queue

### Product Goal

Turn Diagnostics and Today diagnostic rows from passive notices into a repair queue. Users should be able to preview
safe fixes, understand why unsafe diagnostics are read-only, apply fixes explicitly, and return to a refreshed
dashboard.

### Scope

Add diagnostic fix preview/apply behavior without duplicating fix semantics in `zorg-dash`:

- Define a dashboard-facing fix preview model: rule code, path, line/column, replacement preview, preferred/safe flag,
  and explanation.
- Add selected-row fix preview action, for example `f`, available from Diagnostics and Today diagnostic rows.
- Add a Fixes overlay or panel that summarizes relevant plans for the selected diagnostic, selected file, or marked
  diagnostics.
- Add confirmed apply, for example `F`, that delegates to the same safe fix planning/apply path used by CLI/LSP.
- Add severity/rule/path local filters for Diagnostics.
- Add multi-select state that can mark diagnostics and later support bulk apply. Keep first apply behavior conservative
  if bulk apply is too risky for the first phase.
- Refresh the snapshot after successful apply and preserve user context where possible.
- Keep unsafe or ambiguous diagnostics read-only with a visible reason.

### Shared-Crate Boundary

If `zorg-fix` lacks a clean API to plan/apply fixes by diagnostic identity or source span, add it there first. The
dashboard should call a shared planner such as "plan fixes for selected diagnostic/file" and should not reimplement
rewrite logic or parse CLI JSON.

### Likely Files

- `crates/zorg-fix/src/lib.rs`
- `crates/zorg-fix/src/plan.rs`
- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`
- relevant CLI smoke tests and fixtures

### Suggested Phase Split

- Phase 18A: shared `zorg-fix` diagnostic-to-plan API and tests for safe/unsafe cases.
- Phase 18B: dashboard preview overlay for selected diagnostic with cancel/empty/unsafe states.
- Phase 18C: confirmed apply path, terminal suspension or in-process apply safety, refresh, and failure handling.
- Phase 18D: diagnostics filters and multi-select state with render and navigation tests.

### Acceptance

- A diagnostic with an available safe fix can be previewed from the dashboard.
- Applying a fix changes only the intended source span/file and validates rewritten source through the shared fix path.
- Ambiguous, unsafe, or unavailable fixes explain why no safe fix is available.
- Apply success refreshes Diagnostics and Today counts.
- Apply failure leaves source unchanged and records a log event.
- Tests cover preview, cancel, apply success, apply failure, unsafe no-op, and post-apply refresh.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-fix
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke fix
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel diagnostics
```

## Epic 19: Today Workflow Actions And Operational Feedback

### Product Goal

Make Today a daily work surface, not just a list. Users should be able to mark work done, postpone due/do work, schedule
inbox/open todos, copy useful row references, and see progress/timing feedback for long operations.

### Scope

Add safe todo lifecycle actions:

- `d`: mark selected todo done by changing marker to `[X]` and adding or updating `did::YYYY-MM-DD`.
- `p`: postpone due/do date through a small prompted interval/date flow.
- `s`: schedule or set `do::YYYY-MM-DD` for an inbox/open todo.
- Reject rows without a reliable source span, rows with stale content, ambiguous duplicate properties, or parse errors
  after rewrite.
- Refresh after success and preserve user context where possible.
- Show exact file/path/ID changes in the status log.
- Add Today toggles for todos-only, diagnostics-only, and combined mode.
- Add refresh/reindex elapsed timing, row deltas, and spinner/progress indication.
- Add row-level copy/yank actions for selected row ID, source link, and diagnostic message using a clipboard strategy
  that works locally and reasonably over SSH.

### Shared-Crate Boundary

The todo write side should be a small safe planner in a shared crate or clearly separable module, not dashboard-specific
string surgery hidden in `app.rs`. It should use indexed source spans as guards, read current file content before write,
apply deterministic edits, and validate by parsing after rewrite.

### Likely Files

- new shared todo-action module or crate if warranted
- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/README.md`
- fixtures for nested/file zettel todo edits

### Suggested Phase Split

- Phase 19A: safe todo action planner and tests for done/postpone/schedule edit plans.
- Phase 19B: dashboard bindings, confirmation/prompt overlays, apply flow, and refresh.
- Phase 19C: Today display toggles and selection preservation across row removal.
- Phase 19D: timing/spinner/action progress plus copy/yank bindings.

### Acceptance

- Mark-done modifies only the selected zettel and is idempotent.
- Postpone and schedule preserve unrelated properties, tags, title, body, and child zettel structure.
- Unsafe cases fail closed with a clear reason.
- Today rows disappear or move appropriately after successful action and refresh.
- Refresh/reindex/search/capture/todo/fix actions show elapsed timing and log entries.
- Copy/yank actions report success or unavailable clipboard transport without panicking.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel today
```

## Epic 20: Snapshot Performance, Startup Responsiveness, And Test Depth

### Product Goal

Make the dashboard scale beyond fixtures. Startup should draw immediately, refresh cost should not multiply with every
query, and regressions should be caught by realistic render and interaction tests.

### Scope

Address the largest performance and coverage gaps:

- Render an immediate loading frame in interactive mode instead of synchronously loading the full snapshot before first
  draw.
- Reuse the existing worker channel for initial snapshot load.
- Memoize preview text once per snapshot, or add a shared store/query path that returns preview text without repeated
  `Store::list_zettel()` calls.
- Avoid running full-corpus preview collection once per Today query spec.
- Add lightweight telemetry counters: refresh count, last refresh duration, search duration, row counts, and last action
  duration.
- Add golden/snapshot-style tests for `--once` frames per panel against a larger overflow fixture.
- Cover rapid refresh debounce/pending guards.
- Cover narrow capture/fix/todo overlays.
- Cover `--no-mouse`, `--no-alt-screen`, degraded index, empty corpus, and panic cleanup behavior where feasible.
- Profile with a generated or checked-in large test corpus before and after the performance changes.

### Likely Files

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/tests` if integration tests are split out
- fixture generation helpers or static overflow fixtures
- possibly `crates/zorg-store/src/lib.rs` or `crates/zorg-query/src/lib.rs` for preview-efficient APIs

### Suggested Phase Split

- Phase 20A: initial loading frame and async startup architecture.
- Phase 20B: preview memoization or query/store API improvement, with before/after test instrumentation.
- Phase 20C: telemetry model and Index inspector display.
- Phase 20D: golden frame tests and interaction coverage for known gaps.

### Acceptance

- Interactive startup can draw a loading/degraded frame before expensive snapshot work completes.
- A refresh of Today does not call full-corpus preview loading once per built-in query.
- Search and refresh duration are visible in the dashboard telemetry/status surfaces.
- A large overflow corpus completes `--once` and interactive bounded runs within documented expectations.
- New tests fail on the current known regressions: no viewport, no first-run guidance, untested pending refresh, and
  fragile overlay layout.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel today
```

## Epic 21: Saved Query Browser And Search UX

### Product Goal

Make saved `#z/query` zettel discoverable and make Search usable for repeated interactive triage. Users should browse
query definitions, run them without memorizing IDs, navigate query history, and understand query parse/evaluation
errors.

### Scope

Add a first-class Queries panel and stronger input/search behavior:

- Add `Panel::Queries` listing `#z/query` zettel.
- Show query ID, title, source type (`query::` property or fenced `swog` block), definition preview, validity, and row
  count preview where feasible.
- Pressing `enter` on a query runs it in Search.
- A separate binding opens the query zettel source.
- Search inspector shows stored query title and resolved definition when the active query is `@id`.
- Invalid query definitions appear in the Queries panel with diagnostic/error text instead of crashing.
- Add reusable input editing: cursor movement, Home/End, delete, Ctrl-W, Ctrl-U, paste-safe insertion, and
  `KeyEventKind` filtering.
- Add per-session search history, with Up/Down history recall while editing.
- Add optional persisted search history only after the per-session model is stable, or defer persistence to Epic 23.
- Render multi-line query errors without flattening away useful context.
- Add SWOG help overlay or help tab reachable from Search.

### Shared-Crate Boundary

Use `zorg-query` query definition extraction and execution APIs. If listing all saved query definitions is clumsy
through only `Store::list_zettel()` plus repeated source parsing, add a shared helper in `zorg-query` that can enumerate
definitions and per-definition errors consistently with `query_definition_by_id`.

### Likely Files

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- `docs/query.md` if dashboard search/help docs need updating

### Suggested Phase Split

- Phase 21A: query listing/definition data model and shared enumeration helper if needed.
- Phase 21B: Queries panel rendering, navigation, inspector, and run/open actions.
- Phase 21C: reusable input editor and Search history.
- Phase 21D: SWOG help overlay and multi-line error rendering.

### Acceptance

- Query zettel with `query::` properties and fenced `swog` blocks both appear in Queries.
- Invalid query definitions are visible with errors and do not crash snapshot loading.
- Running a query from Queries produces the same rows as `zorg query --id @id`.
- Search can recall recent queries and edit within the line.
- Stored-query Search inspector shows resolved title/source/definition instead of only raw `@id`.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke query
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel queries
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel search --query '@queries/inbox'
```

## Epic 22: Graph Context, Freshness Awareness, And JSON Frame Export

### Product Goal

Give users enough graph and freshness context to trust what they are seeing without building a full graph viewer or
embedding a watcher. Also expose a structured one-shot frame for scripts without turning the interactive TUI into a
long-lived API.

### Scope

Add bounded read-only context and operational surfaces:

- Enrich zettel inspector with outgoing links, incoming resolved backlinks, ancestors, and descendants.
- Show unresolved outgoing links explicitly, not silently omitted.
- Bound graph display to the first N rows per section with counts and truncation markers.
- Avoid high-degree responsiveness issues by loading graph details lazily or caching them per snapshot.
- Detect whether the index database changed since the current snapshot and show a "newer index available" hint.
- Add optional auto-refresh when idle, gated by conservative interval and generation checks.
- If `zorg watch` exposes a stable status surface, show watcher active/inactive/degraded state in the Index panel.
- Keep `zorg dash` read-only by default and do not embed a writer/watch loop.
- Add `--once --json` to emit a structured dashboard frame for the selected panel.
- Include inline telemetry in Index inspector and JSON frame: snapshot time, refresh/search timing, row counts, current
  panel, filters, and health.

### Likely Files

- `crates/zorg-store/src/lib.rs` only if small helper APIs are needed
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/Cargo.toml` if JSON serialization is introduced there
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`

### Suggested Phase Split

- Phase 22A: graph-neighborhood data loading and inspector rendering.
- Phase 22B: freshness detection and idle auto-refresh hints.
- Phase 22C: watcher status integration only if there is a stable status contract.
- Phase 22D: `--once --json` frame schema and telemetry export.

### Acceptance

- Inspector shows bounded backlink/outlink/ancestor/descendant counts and example rows.
- Unresolved links are shown as unresolved.
- High-degree zettel do not freeze rendering.
- External reindex/watch activity can be noticed without changing read-only launch behavior.
- Auto-refresh does not run while the user is editing search/capture/todo prompts or while another action is pending.
- `--once --json` emits a documented, deterministic-enough frame for the selected panel.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-store
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --json --panel today
```

## Epic 23: Dashboards-As-Zettel, Capture Template Picker, And Persisted State

### Product Goal

Let users customize the dashboard using Zorg's own model, improve capture ergonomics, and preserve useful dashboard
state across sessions once the state surface is large enough to be worth saving.

### Scope

Add higher-level customization after the foundational surfaces are stable:

- Add `zorg dash --as @dashboard/id`.
- Define `#z/dashboard` zettel semantics for named panels backed by query IDs or inline SWOG.
- Keep dashboard definitions ordinary zettel, not a parallel config/plugin format.
- Validate dashboard definitions and show invalid panels with local diagnostics instead of crashing.
- Allow built-in Today/Inbox/Diagnostics/Index/Search/Queries panels to coexist with query-backed custom panels.
- Add a capture template picker instead of defaulting to the first template.
- Show template ID, title, destination, and required fields.
- Keep long-form capture in `$EDITOR` or delegated `zorg-capture`; do not turn the dashboard into a full editor.
- Add persisted dashboard state under an XDG-appropriate path: last panel, last query, recent searches, selected
  dashboard, and small UI preferences such as color/mouse mode.
- Make persistence opt-out or conservative enough for CI and `--once`.

### Shared-Crate Boundary

Dashboard definitions should reuse query-definition parsing where possible. Capture template enumeration should reuse or
extend `zorg-capture` template discovery so CLI, dashboard, and future editor integrations agree on template validity.

### Likely Files

- `crates/zorg-capture` template discovery APIs if needed
- `crates/zorg-query/src/lib.rs` if dashboard query-definition helpers are reused
- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- docs describing `#z/dashboard`

### Suggested Phase Split

- Phase 23A: `#z/dashboard` definition model, validation, and `--as` parser.
- Phase 23B: custom panel loading/rendering backed by query IDs and inline SWOG.
- Phase 23C: capture template picker and template inspector.
- Phase 23D: persisted dashboard/search state and migration/backward compatibility tests.

### Acceptance

- `zorg dash --as @dashboard/id` renders named dashboard panels from an indexed `#z/dashboard` zettel.
- Invalid dashboard definitions are displayed as actionable diagnostics.
- Query-backed custom panels use the same row rendering, viewport, filtering, and inspector behavior as built-in panels.
- Capture lets the user choose among available templates before filling fields.
- Persisted state restores last panel/query/history without affecting `--once` determinism unless explicitly requested.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-capture
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --as @dashboards/daily
```

## Dependency Order

The recommended order is:

1. Epic 17: usability foundation.
2. Epic 18: actionable diagnostics and fix queue.
3. Epic 19: Today workflow actions and operational feedback.
4. Epic 20: snapshot performance, startup responsiveness, and test depth.
5. Epic 21: saved query browser and Search UX.
6. Epic 22: graph context, freshness awareness, and JSON frame export.
7. Epic 23: dashboards-as-zettel, capture template picker, and persisted state.

There is one deliberate tension in this order: Epic 20 performance work may need to move earlier if profiling on a real
corpus shows startup or refresh cost blocking usable development of Epics 18 or 19. If that happens, split out Epic 20A
and 20B immediately after Epic 17, then return to diagnostics and Today actions.

## Continuous Test Strategy

Every epic should add tests as part of its own phases, not as a cleanup epic:

- Keep `cargo fmt --check` and `cargo test -p zorg-dash` mandatory for every phase.
- Run relevant shared-crate tests whenever a phase touches `zorg-store`, `zorg-query`, `zorg-fix`, or `zorg-capture`.
- Keep CLI smoke tests for `zorg dash --once`, `--exit-after`, `--no-alt-screen`, `--no-mouse`, `--no-color`, and
  degraded index behavior.
- Maintain at least one fixture corpus with more rows than fit in the default `--once` frame.
- Prefer stable render assertions over fragile full-screen snapshots, but add golden frame tests for major panels once
  their layout is intentionally stable.
- Include narrow terminal `TestBackend` coverage for every new overlay.
- For write actions, test source guards, parse-after-write validation, idempotence, failure no-op behavior, and
  post-action refresh.

## Deferred Work

Defer the following unless a later research pass explicitly changes direction:

- Embedded long-form editor.
- Full terminal graph visualization.
- Embedded watcher writer mode.
- Theme/plugin system.
- Web/Tauri/Textual rewrite.
- A long-lived dashboard automation API. `--once --json` is acceptable as a bounded frame export; it should not become a
  replacement for `zorg query --json`, `zorg watch --format json`, or other stable command APIs.

## Done Criteria For The Whole Program

The post-MVP dashboard improvement program is complete when:

- Existing panels are scrollable, styled, filterable where useful, and stable under refresh.
- Diagnostics and Today rows support safe, explicit actions with preview/confirmation and reliable rollback/no-op
  behavior on failures.
- Startup and refresh remain responsive on corpora materially larger than fixtures.
- Saved queries and query-backed dashboards are discoverable without creating a parallel configuration language.
- The inspector provides useful graph context without attempting a full graph UI.
- The Index panel and status/log surfaces make freshness, telemetry, and action history understandable.
- Capture supports practical template selection while remaining delegated to `zorg-capture`.
- The dashboard remains a thin operational UI over shared Rust semantics.

---
research_date: 2026-05-04
bead_id: zorg-6
title: Zorg Dash Next Improvements legend review
source_legend: sdd/legends/202605/zorg_dash_next_improvements.md
reviewed_work:
  - sdd/epics/202605/epic17_zorg_dash_usability_foundation.md
  - sdd/epics/202605/zorg_dash_epic18_actionable_diagnostics.md
  - sdd/epics/202605/zorg_dash_epic19.md
  - sdd/epics/202605/zorg_dash_epic20_snapshot_performance.md
  - sdd/epics/202605/epic21_saved_query_browser.md
  - sdd/epics/202605/epic22_zorg_dash_graph_freshness_json.md
  - sdd/epics/202605/epic23_zorg_dashboards_capture_state.md
  - crates/zorg-dash/README.md
  - crates/zorg-dash/src/lib.rs
  - crates/zorg-dash/src/app.rs
  - crates/zorg-dash/src/actions.rs
  - crates/zorg-dash/src/data.rs
  - crates/zorg-dash/src/model.rs
  - crates/zorg-dash/src/ui.rs
  - crates/zorg-dash/src/json.rs
  - crates/zorg-dash/src/state.rs
  - crates/zorg-fix/src/plan.rs
  - crates/zorg-refactor/src/todo.rs
  - crates/zorg-query/src/lib.rs
verification:
  - sase bead show zorg-6
  - sase bead show zorg-6.7
  - jq 'select(.id|startswith("zorg-6"))' sdd/beads/issues.jsonl
  - git log --oneline 76bdb5b..HEAD
  - git diff --stat 76bdb5b..HEAD
  - cargo test -p zorg-dash
  - cargo test -p zorg-cli --test smoke dash
---

# Zorg Dash Next Improvements Legend Review

## Placement Note

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory convention used by generated SDD docs and adjacent research.

## Scope And Bead State

This review covers the work associated with `zorg-6`, "Zorg Dash Next Improvements." `sase bead show zorg-6` reports
the parent legend as **OPEN**, while all seven child epics are **CLOSED**:

1. `zorg-6.1` Epic 17: dashboard usability foundation.
2. `zorg-6.2` Epic 18: actionable diagnostics and fix queue.
3. `zorg-6.3` Epic 19: Today work surface.
4. `zorg-6.4` Epic 20: snapshot performance, startup, and frame tests.
5. `zorg-6.5` Epic 21: saved query browser and Search UX.
6. `zorg-6.6` Epic 22: graph context, freshness awareness, and JSON frame export.
7. `zorg-6.7` Epic 23: dashboards-as-zettel, capture template picker, and persisted state.

The append-only `sdd/beads/issues.jsonl` history contains duplicate intermediate rows for some Epic 23 phase records,
but the authoritative `sase bead show zorg-6.7` view resolves Epic 23 and all seven of its child phases as closed. The
remaining bead bookkeeping issue is that the parent `zorg-6` legend itself has not been closed.

## One-Screen Summary

The `zorg-6` work turns `zorg dash` from an MVP read-only terminal dashboard into a richer daily cockpit. The dashboard
now has scrollable, stable panels; color-aware diagnostics; persistent key help and a status log; actionable diagnostic
fix previews and single-fix apply; guarded Today todo actions; saved-query browsing; improved Search editing and help;
startup responsiveness and telemetry; graph context in inspectors; snapshot freshness checks and optional idle
auto-refresh; one-shot JSON frame export; custom query-backed dashboards defined as normal Zorg zettel; a capture
template picker; and persisted interactive dashboard state.

The implementation preserved the original product boundary: `zorg dash` remains a native Rust Ratatui TUI in this
workspace, normal browsing opens the store read-only, and explicit writes route through shared Rust crates rather than
dashboard-specific parsers or ad hoc source rewriting.

## Commit And Code Footprint

The reviewed implementation range starts at the legend plan commit `76bdb5b` and runs through `20d5429`, which closed
Epic 23. The range changes 43 files with about 21.6k insertions and 1.1k deletions. The largest changes are in
`crates/zorg-dash/src/app.rs`, `model.rs`, `ui.rs`, `data.rs`, `actions.rs`, plus two new dashboard support modules:
`json.rs` for one-shot frame serialization and `state.rs` for persisted state.

Shared-crate work was intentionally narrow:

- `zorg-fix` gained dashboard-facing diagnostic fix preview and shared apply orchestration.
- `zorg-refactor` gained `todo.rs`, a guarded todo lifecycle planner used by dashboard Today actions.
- `zorg-query` gained saved query catalog listing.
- `zorg-store` gained small support needed by dashboard data loading and freshness/graph behavior.
- `zorg-cli` grew the `zorg dash` option surface and CLI smoke coverage.

## New Functionality By Epic

### Epic 17: Usability Foundation

Epic 17 made the existing panels practical on real corpora:

- Stable row identities for zettel, diagnostics, and index rows.
- Per-panel viewport state with selected index, scroll offset, visible row count, and total row count.
- Row/page/half-page navigation with selection preserved across refresh when the same row remains present.
- Main panel rendering moved from paragraph wrapping to viewport-backed list/table style rows.
- Compact row counts such as `1/43` in frame output.
- Color mode with `NO_COLOR` and `--no-color` support.
- Severity/status styling for diagnostic and health states.
- Persistent footer key help plus adjacent status text.
- Bounded status-event ring and log overlay.
- Default-off mouse capture, with explicit `--mouse` and `--no-mouse` flags.
- First-run and empty-index guidance with resolved root/database and reindex next steps.

This is the foundation that made later Today, Diagnostics, Queries, and custom panels navigable instead of merely
visible.

### Epic 18: Diagnostics As A Repair Queue

Epic 18 converted diagnostic rows from passive notices into actionable repair items:

- Shared diagnostic fix preview API in `zorg-fix`.
- Shared apply orchestration migrated out of CLI-private code so CLI and dashboard use the same safety rules.
- Dashboard fix preview overlay for selected diagnostic rows in Diagnostics and Today.
- Confirmed single-fix apply path, including stale-source checks, async apply, refresh, and failure logging.
- Local severity, code, and path filters for Diagnostics and Today diagnostic rows.
- Diagnostic mark/unmark state for local review queues.
- Marked diagnostics survive refresh by stable identity and are pruned when diagnostics disappear.
- Bulk apply remains intentionally unavailable; the preview overlay explains that marked diagnostics are review state,
  while apply is limited to one selected safe preview.

### Epic 19: Today As A Work Surface

Epic 19 made Today directly useful for daily task maintenance:

- A shared guarded todo lifecycle planner in `zorg-refactor`.
- Mark-done action for selected Today todo rows, changing the marker and adding/updating `did::YYYY-MM-DD`.
- Postpone and schedule flows with prompts, strict `YYYY-MM-DD` dates, and simple relative intervals such as `+1d` and
  `+1w`.
- Explicit due/do field choice when a row is ambiguous.
- Guarded source matching, stale-content rejection, parse-after-rewrite validation, and no-write failures.
- Today display modes: combined, todos-only, and diagnostics-only.
- Row identity preservation after todo actions move or remove rows.
- Pending-operation feedback for refresh, reindex, search, capture, fix, and todo actions.
- Timing and row-delta events in the status/log surfaces.
- Row-level yank actions for row ID, source link, and diagnostic message, with OSC 52, local clipboard fallbacks, and
  log fallback when no clipboard transport is available.

### Epic 20: Startup, Performance, And Regression Coverage

Epic 20 improved responsiveness and made dashboard rendering easier to regression-test:

- Interactive startup now renders a loading frame immediately, then swaps in the ready snapshot when loading completes.
- `--once` and stdout fallback rendering remain synchronous so scripts get a complete frame.
- Snapshot preview memoization avoids repeated full-corpus preview collection across Today, Inbox, Search, and related
  panel loads.
- Dashboard telemetry tracks refresh count, initial load timing, refresh/search/action durations, and panel row counts.
- Index/status surfaces show compact timing and row-count feedback.
- Deterministic overflow fixtures and golden frame tests cover panels, overlays, pending guards, and narrow layouts.
- Large-corpus validation tooling and README guidance document how to run local performance checks.

### Epic 21: Saved Query Browser And Search UX

Epic 21 made stored `#z/query` definitions discoverable and easier to run:

- `zorg-query` can enumerate saved query definitions and return row-level errors for invalid query zettel.
- New first-class Queries panel in the dashboard.
- Query rows show ID/title/source kind/output kind, definition preview, validity, source location, and best-effort row
  count preview.
- Invalid saved queries remain visible with useful error text instead of degrading the full dashboard.
- Enter on a valid Queries row runs it in Search; opening a Queries row jumps to the source zettel.
- Search input is now a reusable single-line editor with cursor movement, Home/End, Delete, Backspace, Ctrl-W, Ctrl-U,
  and key event kind filtering.
- Per-session Search history supports Up/Down recall and de-duplicates adjacent repeated queries.
- SWOG help overlay is available from Search with `F1` or `H`.
- Multi-line query errors render in both Search and the inspector.

### Epic 22: Graph, Freshness, Auto-Refresh, And JSON

Epic 22 added context and script-friendly observability:

- Selected zettel inspectors show bounded graph context for Today, Inbox, and Search rows.
- Graph sections include outgoing links, incoming backlinks, ancestors, descendants, unresolved outgoing links, total
  counts, and truncation counts.
- Snapshot freshness distinguishes current, newer-index-available, stale-source, and check-failed states.
- Optional idle auto-refresh is available with `--auto-refresh MS`, default-off and guarded so it skips prompts,
  overlays, and pending worker operations.
- `--once --json` exports a compact structured frame with schema marker `zorg.dash.frame`.
- JSON includes root/database paths, active panel, rows, row counts, selected inspector details, telemetry, health,
  freshness, selected dashboard/custom panel metadata when present, and graph context for the selected zettel when
  available.
- Interactive `--json` is rejected; the JSON export is a one-shot frame, not a streaming automation API.

### Epic 23: Dashboards As Zettel, Capture Picker, And State

Epic 23 made the dashboard configurable through Zorg source rather than a separate config language:

- Dashboard zettel are ordinary `#z/dashboard` zettel selected with `zorg dash --as @dashboard/id`.
- Direct `#z/panel` children define query-backed custom panels with `key::`, `title::`, and either `query::@queries/id`
  or a fenced `swog` block.
- Dynamic panel registry supports built-in panels plus custom panel keys; `--panel key` works for custom panels when
  `--as` is provided.
- Custom panels render zettel rows with the same selection, source opening, yank, preview, and graph inspector behavior
  as built-in query-backed panels.
- Dashboard definition errors surface as dashboard-local diagnostics instead of degrading unrelated panels.
- The capture flow now opens a template picker when multiple `#z/tmpl` templates are available.
- Capture form rendering includes template metadata and required variables, currently editing the compact dashboard
  fields `title` and `body` while generated fields remain delegated to `zorg-capture`.
- Interactive dashboard state is saved on clean exit under `$XDG_STATE_HOME/zorg/dash/state.json`, or
  `$HOME/.local/state/zorg/dash/state.json` when `XDG_STATE_HOME` is unset.
- State stores active panel key, selected dashboard ID, last search query, recent search queries, mouse preference, and
  auto-refresh preference.
- `--no-state` disables state load/save, and `--state PATH` selects an alternate state file.
- Explicit CLI flags override restored state.

## User-Facing Command Surface

The dashboard README now documents these launch forms:

```sh
zorg dash --root PATH --db PATH
zorg dash --panel today
zorg dash --panel queries
zorg dash --panel search --query '#z/inbox'
zorg dash --once
zorg dash --once --json --panel today
zorg dash --as @dashboards/daily --panel open
NO_COLOR=1 zorg dash --once --panel diagnostics
zorg dash --no-color --once
zorg dash --exit-after 250 --no-alt-screen
zorg dash --auto-refresh 5000
zorg dash --mouse
zorg dash --no-state
zorg dash --state /tmp/zorg-dash-state.json
```

The practical keymap now includes panel navigation, search editing, refresh/reindex, capture, Today mode cycling,
mark-done, postpone, schedule, yank, diagnostic fix preview/apply, diagnostic marking/filtering, log/help overlays, and
SWOG help.

## Validation Results

Focused validation in this checkout passed:

```sh
cargo test -p zorg-dash
# 168 passed; 0 failed

cargo test -p zorg-cli --test smoke dash
# 22 passed; 0 failed; 72 filtered out
```

The `zorg-dash` tests cover model behavior, dashboard actions, async app flows, graph and freshness data loading,
JSON serialization, persisted state, render layout, overlays, no-color rendering, overflow fixtures, and custom
dashboard panels. The CLI smoke tests cover dash help/options, bounded runs, one-shot text and JSON frames, Queries,
Diagnostics, Today, custom dashboards, definition diagnostics, mouse/no-color/no-state flags, and JSON/auto-refresh
validation rules.

## Remaining Gaps And Reconciliation Items

- Close or otherwise reconcile the parent `zorg-6` legend bead if this review is accepted as completion evidence.
- The append-only bead log contains duplicate Epic 23 intermediate records; `sase bead show` resolves the final state
  correctly, but future audits should prefer `sase bead show` over raw JSONL order for this subtree.
- Bulk diagnostic apply is intentionally not implemented. Marking diagnostics is review state only.
- Auto-refresh does not start or supervise `zorg watch`; it only reloads the read-only snapshot while idle.
- Dashboard zettel support is intentionally query-backed. There is no plugin system, theme system, long-form editor, or
  separate dashboard config format.
- `zorg dash --once --json` is a bounded frame export, not a streaming API. Scripts needing query rows should still use
  `zorg query --json`; scripts needing watcher events should use `zorg watch --format json`.

## Bottom Line

`zorg-6` successfully delivered the next dashboard layer. The dashboard is now navigable, actionable, script-inspectable,
customizable through Zorg source, and stateful across interactive sessions, while keeping domain writes and parsing in
the shared Rust crates that already own those semantics.

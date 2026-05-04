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
  - fixtures/corpus/dashboard.z
verification:
  - sase bead show zorg-6
  - sase bead show zorg-6.7
  - jq 'select(.id|startswith("zorg-6"))' sdd/beads/issues.jsonl
  - git log --oneline 76bdb5b..HEAD
  - git diff --stat 76bdb5b..HEAD
  - cargo run -q -p zorg-cli -- dash --help
  - cargo run -q -p zorg-cli -- dash --once --json --panel index
  - cargo test -p zorg-dash
  - cargo test -p zorg-cli --test smoke dash
---

# Zorg Dash Next Improvements Legend Review

## Placement Note

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory convention used by generated SDD docs and adjacent research (e.g., `zorg_4_legend_review.md` is in the
same directory).

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
remaining bead bookkeeping issue is that the parent `zorg-6` legend itself has not been closed. The earlier `zorg-1`
(v1 MVP) and `zorg-4` (v1.1 non-dashboard) legends share the same "open legend, closed epics" pattern, so a single
reconciliation pass can close all three.

This review intentionally excludes Epic 16 (the original `zorg dash` MVP, tracked under `zorg-5`). Epic 16 produced
the read-only dashboard skeleton; `zorg-6` builds on it.

## Glossary

- **Frame**: One rendered dashboard view. `--once` writes a single frame to stdout; `--once --json` writes the same
  frame as compact JSON. Interactive runs are a sequence of frames with shared state.
- **Panel**: A named tab in the dashboard. Built-in keys are `today`, `inbox`, `queries`, `search`, `diagnostics`, and
  `index`. Custom keys come from `#z/panel` children of a loaded `#z/dashboard` zettel. Both kinds are exposed as
  `PanelId::BuiltIn(Panel)` or `PanelId::Custom(String)` in `crates/zorg-dash/src/model.rs`.
- **Snapshot**: The read-only view of the indexed corpus held by the dashboard for the current frame. States are
  `loading`, `ready`, and `degraded`.
- **Snapshot freshness**: The result of comparing the loaded snapshot's index generation with the current on-disk
  generation and source health. States are `unknown`, `current`, `newer_index_available`, `stale_sources`, and
  `check_failed` (`SnapshotFreshness` enum in `crates/zorg-dash/src/model.rs`).
- **Index generation**: A small fingerprint of the SQLite index — schema version, last-indexed timestamp, indexed file
  count, diagnostic count.
- **Inspector**: The right-hand pane that shows details for the selected row in the active panel. For zettel rows it
  also shows graph context.
- **Graph neighborhood**: Bounded outgoing links, incoming backlinks, ancestors, descendants, and unresolved outgoing
  links for a selected zettel, with truncation counts when high-degree.
- **Custom panel**: A query-backed panel whose definition lives in a `#z/panel` zettel inside a `#z/dashboard` zettel.
- **Saved query**: A `#z/query` zettel that supplies a `query::` property or a fenced `swog` block. The query catalog
  enumerates these (`QueryDefinitionListing::{Valid, Invalid}` in `crates/zorg-query/src/lib.rs`).
- **SWOG**: The inline query language used by `zorg query` and the dashboard Search panel. F1/H from Search opens an
  in-dashboard SWOG help overlay.
- **Yank**: Copying a row's identity, source link, or diagnostic message. The dashboard prefers OSC 52 when stdout is
  a terminal, then local clipboard commands, then a log fallback.
- **OSC 52**: An ANSI escape sequence terminals can use to set the host clipboard from a remote shell or TUI.
- **Telemetry**: Local in-memory dashboard counters: refresh count, last initial-load duration, last refresh, last
  search, last action, and per-panel row counts. Surfaced in the Index inspector and the JSON frame.
- **Pending operation**: A worker-driven job (refresh, reindex, search, capture, fix, todo). The dashboard shows a
  pending banner and skips auto-refresh while one is active.

## One-Screen Summary

The `zorg-6` work turns `zorg dash` from an MVP read-only terminal dashboard into a richer daily cockpit. The
dashboard now has scrollable, stable panels; color-aware diagnostics; persistent key help and a status log;
actionable diagnostic fix previews and single-fix apply; guarded Today todo actions; saved-query browsing; improved
Search editing and help; startup responsiveness and telemetry; graph context in inspectors; snapshot freshness checks
and optional idle auto-refresh; one-shot JSON frame export; custom query-backed dashboards defined as normal Zorg
zettel; a capture template picker; and persisted interactive dashboard state.

The implementation preserved the original product boundary: `zorg dash` remains a native Rust Ratatui TUI in this
workspace, normal browsing opens the store read-only, and explicit writes route through shared Rust crates rather than
dashboard-specific parsers or ad hoc source rewriting.

## Commit And Code Footprint

The reviewed implementation range starts at the legend plan commit `76bdb5b` and runs through `20d5429`, which closed
Epic 23. `git diff --stat 76bdb5b..HEAD` reports 51 files changed, ~22.0k insertions, ~1.2k deletions across crates,
docs, fixtures, SDD prompts/plans/epics, the legend, and this research file. The dashboard crate alone grew to
roughly 17.7k lines across `app.rs`, `model.rs`, `ui.rs`, `data.rs`, `actions.rs`, plus the two new modules `json.rs`
(one-shot frame serialization) and `state.rs` (persisted interactive state).

Shared-crate work was intentionally narrow:

- `zorg-fix` gained dashboard-facing diagnostic fix preview and shared apply orchestration.
- `zorg-refactor` gained `todo.rs`, a guarded todo lifecycle planner used by dashboard Today actions
  (`TodoActionRequest` → `plan_todo_action` → `apply_todo_action_plan`).
- `zorg-query` gained a saved query catalog (`QueryDefinitionSummary`, `QueryDefinitionListing`,
  `QueryDefinitionInvalidListing`).
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

- A shared guarded todo lifecycle planner in `zorg-refactor::todo`.
- Mark-done action for selected Today todo rows, changing the marker and adding/updating `did::YYYY-MM-DD`.
- Postpone and schedule flows with prompts, strict `YYYY-MM-DD` dates, and simple relative intervals such as `+1d`
  and `+1w`.
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

- Interactive startup now renders a loading frame immediately, then swaps in the ready snapshot when loading
  completes.
- `--once` and stdout fallback rendering remain synchronous so scripts get a complete frame.
- Snapshot preview memoization avoids repeated full-corpus preview collection across Today, Inbox, Search, and related
  panel loads.
- Dashboard telemetry tracks refresh count, initial load timing, refresh/search/action durations, and panel row
  counts.
- Index/status surfaces show compact timing and row-count feedback.
- Deterministic overflow fixtures and golden frame tests cover panels, overlays, pending guards, and narrow layouts.
- Large-corpus validation tooling (`tools/perf_large_corpus.py`) and README guidance document how to run local
  performance checks; printed timings are local regression signals, not CI thresholds.

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
- Snapshot freshness distinguishes `current`, `newer_index_available`, `stale_sources`, and `check_failed` states (an
  `unknown` state is used before the first comparison).
- Optional idle auto-refresh is available with `--auto-refresh MS` (minimum 1000), default-off and guarded so it skips
  prompts, overlays, and pending worker operations.
- `--once --json` exports a compact structured frame with schema marker `zorg.dash.frame`, `schema_version: 1`.
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
- Custom panels render zettel rows with the same selection, source opening, yank, preview, and graph inspector
  behavior as built-in query-backed panels.
- Dashboard definition errors surface as dashboard-local diagnostics instead of degrading unrelated panels.
- The capture flow now opens a template picker when multiple `#z/tmpl` templates are available.
- Capture form rendering includes template metadata and required variables, currently editing the compact dashboard
  fields `title` and `body` while generated fields remain delegated to `zorg-capture`.
- Interactive dashboard state is saved on clean exit under `$XDG_STATE_HOME/zorg/dash/state.json`, or
  `$HOME/.local/state/zorg/dash/state.json` when `XDG_STATE_HOME` is unset.
- State stores active panel key, selected dashboard ID, last search query, recent search queries (up to 50), mouse
  preference, and auto-refresh preference.
- `--no-state` disables state load/save, and `--state PATH` selects an alternate state file.
- Explicit CLI flags (`--as`, `--panel`, `--query`, `--mouse`, `--no-mouse`, `--auto-refresh`, `--no-auto-refresh`)
  override restored values.

## User-Facing Command Surface

`zorg dash --help` (verified in this checkout):

```
zorg dash [--root PATH] [--db PATH]
          [--panel today|inbox|queries|search|diagnostics|index]
          [--as @dashboard/id]
          [--query @id|SWOG]
          [--once]
          [--json]
          [--exit-after MS]
          [--auto-refresh MS]
          [--no-auto-refresh]
          [--no-state]
          [--state PATH]
          [--no-alt-screen]
          [--mouse]
          [--no-mouse]
          [--no-color]
```

Useful launch forms:

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

The practical keymap (verbatim from `crates/zorg-dash/README.md`):

| Key | Action |
| --- | --- |
| `tab`, `backtab`, left/right | Switch panels |
| up/down, `j`/`k`, `g`/`G` | Move selection |
| `/` | Edit the Search query |
| `r` | Refresh the read-only dashboard snapshot |
| `R` | Confirm and run reindex |
| `enter` | Run a selected Queries row, or open source outside Queries |
| `o` | Open the selected source in `$EDITOR` |
| `c` | Pick a capture template, review required variables, and create a zettel through `zorg-capture` |
| `t` | Cycle Today mode: combined, todos only, diagnostics only |
| `d` | Mark the selected Today todo done after confirmation |
| `p` | Postpone the selected due/do todo to `YYYY-MM-DD`, `+1d`, or `+1w` |
| `s` | Schedule the selected open/next todo by setting `do::YYYY-MM-DD` |
| `y` | Yank row values: row ID, source link, or diagnostic message |
| `f` | Preview a safe fix for the selected diagnostic row |
| space | Mark or unmark the selected diagnostic row |
| `e` | Cycle diagnostic severity filter: all, error, warning, info |
| `:` | Edit diagnostic code and path substring filters |
| `a` | Clear diagnostic filters |
| `L` | Show the recent status event log |
| `?` | Help overlay |
| `F1` from Search | SWOG syntax help |
| `H` on Search | SWOG syntax help |
| `q`, `Esc` | Quit or close the active overlay |

Search editing key behavior: left/right move by character, Home/End jump to start/end, Backspace/Delete remove around
the cursor, Ctrl-W deletes the previous word, Ctrl-U clears text before the cursor, Enter commits, Esc cancels and
restores prior Search panel state, Up/Down recall recent non-empty queries (adjacent duplicates are not stored).

## Sample JSON Frame Envelope

`zorg dash --once --json` emits one line of compact JSON. Schema marker:

- `schema`: `"zorg.dash.frame"`
- `schema_version`: `1`

Top-level keys include `root`, `database_path`, `selected_dashboard`, `active_panel`, `selection`, `query`,
`today_mode`, `diagnostic_filters`, `row_counts`, `panels[]`, `active_panel_rows[]`, `inspector`, `telemetry`,
`health`, `freshness`, and `snapshot`. Compact illustrative shape (Index panel, ready snapshot):

```jsonc
{
  "schema": "zorg.dash.frame",
  "schema_version": 1,
  "root": "/abs/zorg",
  "database_path": "/abs/zorg/.zorg/zorg.sqlite3",
  "selected_dashboard": null,
  "active_panel": "index",
  "selection": {
    "selected_index": 0,
    "selected_row_id": "index:Discovered files",
    "scroll_offset": 0,
    "visible_row_count": 8
  },
  "query": null,
  "today_mode": "combined",
  "diagnostic_filters": {"severity": "all", "code": "", "path": "", "active": false},
  "row_counts": {"today": 13, "inbox": 0, "queries": 0, "search": 0, "diagnostics": 0, "index": 8},
  "panels": [
    {"panel": "today", "title": "Today", "kind": "built_in", "row_count": 13, "active": false},
    {"panel": "index", "title": "Index", "kind": "built_in", "row_count": 8,  "active": true}
  ],
  "active_panel_rows": [
    {"index": 0, "kind": "index_status", "row_id": "index:Discovered files", "label": "Discovered files", "value": 8}
  ],
  "inspector": {
    "lines": ["Index metadata", "Schema version: 2", "..."],
    "graph": null
  },
  "telemetry": {
    "refresh_count": 0,
    "last_initial_load_ms": 118,
    "last_refresh_ms": null,
    "last_search_ms": null,
    "last_action": null,
    "row_counts": {"today": 13, "...": "..."}
  },
  "health": {
    "label": "current",
    "index": {"schema_version": 2, "discovered_files": 8, "indexed_files": 8, "...": "..."}
  },
  "freshness": {
    "state": "current",
    "generation": {"schema_version": 2, "last_indexed_at_unix_ms": 1777870349880, "indexed_files": 8, "diagnostic_count": 0}
  },
  "snapshot": {"state": "ready", "metrics": {"today_rows": 13, "...": "..."}}
}
```

When a `#z/dashboard` is loaded, `selected_dashboard` carries `requested_id`, `id`, `title`, `source_path`, and a
diagnostic count. When a custom panel is active, that panel's entry in `panels[]` includes a nested `custom` object
with `query_source`, `output_kind`, `definition`, `has_error`, and `error`. When a zettel row is selected on Today,
Inbox, Search, or a custom panel, the inspector also serializes a bounded `graph` neighborhood.

`freshness.state` is one of: `unknown`, `current`, `newer_index_available`, `stale_sources`, `check_failed`. The
non-current states attach `captured`, `current`, or `source_changes` payloads as appropriate.

## Sample Dashboard Zettel

`fixtures/corpus/dashboard.z` ships a working `#z/dashboard` example for tests and as user-facing documentation:

```text
%%% @dashboards/daily #z/dashboard title::Daily
Daily dashboard fixture.
%%%

- @dashboards/daily/open #z/panel key::open title::Open query::@queries/open

- @dashboards/daily/today #z/panel key::today-todos title::Today Todos
  ```swog
  #z/todo -did:*
  ```

- @queries/open #z/query title::Open query::#z/todo -did:*

- @todos/dashboard-one #z/todo [ ] do::2026-05-04
  Dashboard custom panel task.
```

Run it with `zorg dash --as @dashboards/daily --panel open`. Direct `#z/panel` children become custom panels; saved
`#z/query` zettel referenced via `query::@id` are resolved through the saved query catalog.

## Persisted Interactive State

Schema marker:

- `schema`: `"zorg.dash.state"`
- `version`: `1`

Default path: `$XDG_STATE_HOME/zorg/dash/state.json`, or `$HOME/.local/state/zorg/dash/state.json` when
`XDG_STATE_HOME` is unset. Override with `--state PATH`. Disable load/save with `--no-state`. `--once` and stdout
fallback rendering never read or write state.

Persisted shape (`PersistedDashboardState` in `crates/zorg-dash/src/state.rs`):

```jsonc
{
  "schema": "zorg.dash.state",
  "version": 1,
  "selected_panel": "search",
  "search_query": "#z/inbox",
  "search_history": ["#z/todo", "#z/inbox"],
  "selected_dashboard_id": "@dashboards/daily",
  "preferences": {
    "mouse": true,
    "auto_refresh_ms": 2000
  }
}
```

Load behavior is conservative:

- Missing file → `Missing` (no warning).
- Schema mismatch, version mismatch, parse error, or read error → `Ignored(message)` and surfaced in the status log.
- Search history is trimmed to the most recent 50 entries; blank entries are dropped.
- `selected_panel` resolves via `PanelId::from_key`: known built-in keys map to the matching `Panel`; everything else
  becomes a `PanelId::Custom`, which is only meaningful when a `#z/dashboard` defining that key is loaded.
- Explicit CLI flags always override restored values.

## Architecture Map For Contributors

Start with these boundaries (paths relative to the repo root):

| Area | Primary files |
| --- | --- |
| Dashboard model | `crates/zorg-dash/src/model.rs` (panels, snapshot, telemetry, freshness, graph, custom panels) |
| Async app loop | `crates/zorg-dash/src/app.rs` (event loop, workers, prompts, overlays, pending guards) |
| Render | `crates/zorg-dash/src/ui.rs` (Ratatui layout, color, footer, status, log overlay) |
| Data loading | `crates/zorg-dash/src/data.rs` (snapshot build, preview memoization, freshness, graph context) |
| Actions | `crates/zorg-dash/src/actions.rs` (refresh, reindex, capture, fix, todo, yank, search) |
| JSON frame | `crates/zorg-dash/src/json.rs` (`zorg.dash.frame` schema, panel/inspector/telemetry serialization) |
| Persisted state | `crates/zorg-dash/src/state.rs` (`zorg.dash.state` schema, default path, load/save) |
| Dashboard zettel parsing | `crates/zorg-dash/src/lib.rs` plus dashboard-aware data loading |
| Diagnostic fix preview/apply | `crates/zorg-fix/src/plan.rs` |
| Guarded todo lifecycle | `crates/zorg-refactor/src/todo.rs` (`plan_todo_action`, `apply_todo_action_plan`) |
| Saved query catalog | `crates/zorg-query/src/lib.rs` (`QueryDefinitionListing`, `QueryDefinitionSummary`) |
| CLI surface and smoke | `crates/zorg-cli/src/main.rs`, `crates/zorg-cli/tests/smoke.rs` |
| User docs | `crates/zorg-dash/README.md`, `docs/capture.md`, `docs/query.md` |
| Reference fixture | `fixtures/corpus/dashboard.z` |

## Common Pitfalls / FAQ

- **"Why does `zorg dash` look stale?"** The dashboard does not start a watcher. Run `zorg db reindex` once or keep
  `zorg watch` running in another shell. The Index inspector and JSON `freshness.state` will report
  `newer_index_available` or `stale_sources` when the snapshot is behind.
- **"Why was my `--auto-refresh` value rejected?"** The CLI requires `--auto-refresh MS` to be at least 1000 ms, and
  `--once` rejects `--auto-refresh` (one-shot output has no timer loop).
- **"Why does `zorg dash --json` error?"** `--json` is only valid with `--once`. Interactive runs do not stream JSON.
- **"Why didn't auto-refresh fire?"** Auto-refresh skips while a prompt, overlay, or pending worker operation is
  active, and never starts or supervises a watcher; it only reloads the read-only snapshot.
- **"Why didn't my custom panel appear?"** Custom panels require `--as @dashboard/id`. The dashboard zettel must be
  tagged `#z/dashboard`; each direct `#z/panel` child needs `key::`, `title::`, and either `query::@queries/id` or a
  single fenced `swog` block. Definition errors render as dashboard-local diagnostics, not whole-dashboard failures.
- **"Why did my saved query show as invalid in the Queries panel?"** The saved query catalog returns row-level errors
  for `#z/query` zettel that do not extract one valid SWOG definition. The row stays visible with the error message;
  the rest of the panel still works.
- **"Why is bulk-fix-apply not happening?"** It is intentionally not implemented. Marked diagnostics are local review
  state. Apply is limited to one selected safe preview after confirmation.
- **"Where did my dashboard state go?"** It is written on clean exit only. Crashes, kill -9, and `--no-state` skip
  the save. `--state PATH` lets tests and experiments use an alternate file.
- **"OSC 52 yank did not paste."** The yanker tries OSC 52 first, then local clipboard commands, then logs the value
  in the log overlay so it can be selected manually. SSH and tmux configurations may need OSC 52 forwarding enabled.
- **"Why does `p` ask which field?"** Postpone needs to know whether to update `due::` or `do::`. When the row already
  has exactly one of them, the field is chosen automatically. When both are present or neither is present, the prompt
  asks explicitly.

## Validation Surface

Focused validation in this checkout passed:

```sh
cargo test -p zorg-dash
# 168 passed; 0 failed

cargo test -p zorg-cli --test smoke dash
# 22 passed; 0 failed; 72 filtered out
```

Recommended broader validation (matches `docs/development.md` for v1.1):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p zorg-cli -- dash --help
cargo run -q -p zorg-cli -- dash --once --json --panel index
python3 tools/check_fixture_manifest.py
```

The `zorg-dash` tests cover model behavior, dashboard actions, async app flows, graph and freshness data loading,
JSON serialization, persisted state, render layout, overlays, no-color rendering, overflow fixtures, and custom
dashboard panels. The CLI smoke tests cover dash help/options, bounded runs, one-shot text and JSON frames, Queries,
Diagnostics, Today, custom dashboards, definition diagnostics, mouse/no-color/no-state flags, and JSON/auto-refresh
validation rules.

## Recommended Onboarding Path

For a user moving from the Epic 16 MVP to the `zorg-6` dashboard, a useful sequence:

1. Ensure the index is current: `zorg db reindex` once, or run `zorg watch` in another shell.
2. Launch the dashboard: `zorg dash`. Cycle panels with `tab` / `backtab`. The Index panel reports schema, file
   counts, and now telemetry.
3. Use Today: `t` cycles modes; `d` marks done after confirmation; `p` postpones with `YYYY-MM-DD` or `+1d`/`+1w`;
   `s` schedules `do::`; `y` yanks row identity, source link, or diagnostic message.
4. Triage diagnostics: `f` previews a safe fix; `space` marks rows for review; `e` cycles severity; `:` edits code and
   path filters; `a` clears filters.
5. Browse saved queries: open the Queries panel; `enter` runs a query in Search; `o` jumps to the source.
6. Use Search: `/` enters edit mode; `F1`/`H` opens SWOG help; Up/Down recall recent queries.
7. Define a personal dashboard: write a `#z/dashboard` zettel with `#z/panel` children, then run
   `zorg dash --as @dashboards/your-id`.
8. Capture: `c` opens the template picker, then a compact form for `title`/`body`; other template variables remain
   generated by `zorg-capture`.
9. Script integrations: `zorg dash --once --json --panel today` for a frame snapshot. Use `zorg query --json` for
   query rows and `zorg watch --format json` for indexer events instead of polling the dashboard.
10. Tune: `--auto-refresh MS` for idle reloads, `--mouse` to opt in to mouse capture, `--no-state` for hermetic runs.
    Clear `$XDG_STATE_HOME/zorg/dash/state.json` if a stale state file is causing surprises.

## Safety And Product Rules That Survived The Work

- `zorg dash` opens the store read-only for normal browsing. Writes go through shared crates with the same safety
  rules used by the CLI.
- Diagnostic apply is limited to one selected safe preview after confirmation. Bulk apply is not available and is
  intentionally out of scope.
- Today todo writes use a guarded planner: stale-source rejection, content-hash and byte-length guards, parse-after-
  rewrite validation, and explicit field choice when ambiguous.
- The dashboard never starts or supervises `zorg watch`. Auto-refresh only reloads the read-only snapshot.
- Dashboard zettel are query-backed only. There is no plugin system, theme system, embedded editor, or separate
  dashboard config language.
- Custom panel definition errors are dashboard-local diagnostics; they never degrade unrelated panels.
- `zorg dash --once --json` is a bounded frame export, not a streaming API. It carries an explicit `zorg.dash.frame`
  schema marker and version so scripts can pin behavior.
- Persisted state is best-effort: missing/corrupt/version-mismatched files are ignored with a status-log message and
  the dashboard still launches. Explicit CLI flags always override restored values.
- The capture form keeps the dashboard interaction compact by editing only `title` and `body`; `id`, `date`, and
  `source` continue to be filled by `zorg-capture`.

## Explicit Non-Scope (As Of zorg-6)

- Bulk diagnostic apply.
- Background watcher inside the dashboard process.
- Streaming JSON / live automation API. `--once --json` is a one-shot frame.
- Plugin system, theme system, or external dashboard config files.
- Long-form embedded editing. Source edits still use `$EDITOR` (`o` / `enter` outside Queries).
- Cross-root dashboards. The dashboard runs against one resolved root + database per launch.
- Dashboard-side parsing or query semantics. All of that lives in `zorg-store`, `zorg-query`, `zorg-fix`, and
  `zorg-refactor`.

## Remaining Gaps And Reconciliation Items

- Close or otherwise reconcile the parent `zorg-6` legend bead if this review is accepted as completion evidence.
  (Same pattern applies to `zorg-1` and `zorg-4`; consider doing all three in one pass.)
- The append-only bead log contains duplicate Epic 23 intermediate records; `sase bead show` resolves the final state
  correctly, but future audits should prefer `sase bead show` over raw JSONL order for this subtree.
- Bulk diagnostic apply is intentionally not implemented. Marking diagnostics is review state only.
- Auto-refresh does not start or supervise `zorg watch`; it only reloads the read-only snapshot while idle.
- Dashboard zettel support is intentionally query-backed. There is no plugin system, theme system, long-form editor,
  or separate dashboard config format.
- `zorg dash --once --json` is a bounded frame export, not a streaming API. Scripts needing query rows should still
  use `zorg query --json`; scripts needing watcher events should use `zorg watch --format json`.

## Bottom Line

`zorg-6` successfully delivered the next dashboard layer. The dashboard is now navigable, actionable,
script-inspectable, customizable through Zorg source, and stateful across interactive sessions, while keeping domain
writes and parsing in the shared Rust crates that already own those semantics.

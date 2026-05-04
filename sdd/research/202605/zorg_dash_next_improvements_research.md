---
research_date: 2026-05-04
title: Zorg dash next improvement research
status: draft
source_context:
  - crates/zorg-dash/README.md
  - crates/zorg-dash/src/lib.rs
  - crates/zorg-dash/src/app.rs
  - crates/zorg-dash/src/actions.rs
  - crates/zorg-dash/src/data.rs
  - crates/zorg-dash/src/model.rs
  - crates/zorg-dash/src/ui.rs
  - crates/zorg-cli/src/main.rs
  - crates/zorg-cli/tests/smoke.rs
  - sdd/epics/202605/epic16_zorg_dash_dashboard.md
  - sdd/research/202605/zorg_dash_dashboard_research.md
  - sdd/research/202605/zorg_next_feature_recommendations.md
validation:
  - cargo test -p zorg-dash
  - cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_research.sqlite3
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_research.sqlite3 --once --panel today
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_research.sqlite3 --once --panel diagnostics
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_research.sqlite3 --once --panel search --query '#z/todo'
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_research.sqlite3 --once --panel index
recommendation: prioritize list ergonomics, actionable diagnostics/fixes, and Today workflow actions before adding broad new dashboard surfaces; in parallel, fix cross-cutting gaps in input editing, color/styling, snapshot/preview cost, and mouse-capture wiring
---

# Zorg Dash Next Improvement Research

## Placement Note

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory layout already used by generated SDD docs and adjacent research.

## Current Dashboard Baseline

The `zorg dash` MVP has shipped as a native Rust dashboard in `crates/zorg-dash`, consistent with the prior research
and Epic 16 plan. The command is wired through `crates/zorg-cli/src/main.rs` behind the default-on `dash` feature, and
`crates/zorg-dash/README.md` documents the intended MVP behavior.

Implemented behavior observed in source and tests:

- Panels: Today, Inbox, Search, Diagnostics, and Index.
- Data loading: read-only store launch through `Store::open_read_only_with_options`.
- Today panel: built from built-in SWOG queries for due, do, and open todos, then augmented with diagnostics.
- Inbox panel: query-backed through `#z/inbox`.
- Search panel: accepts an inline SWOG query or stored `#z/query` ID through `execute_query_by_id`.
- Diagnostics panel: shows indexed diagnostics sorted by severity/path.
- Index panel: shows schema version, discovered/indexed/stale counts, diagnostics count, effective tag count, and
  current/stale/missing health.
- Actions: refresh, confirmed reindex, open selected row in `$EDITOR`, basic capture through `zorg-capture`, help,
  and log/error overlays.
- Testability: deterministic `--once`, bounded `--exit-after`, Ratatui `TestBackend` tests, and CLI smoke tests.

`cargo test -p zorg-dash` passes with 23 tests.

## Observed Product Constraints

The dashboard is now useful enough to expose second-order UX constraints:

- `ui.rs` renders rows by converting the entire active row set into text lines inside a `Paragraph`. There is no
  scroll offset, row count, paging, or "selected row N of M" state. In the fixture corpus, diagnostics already exceed
  the visible area.
- `AppState` stores only a selected index per panel. There is no viewport model, sort mode, local filter, or persisted
  query history.
- The Today panel is attention-oriented but not action-oriented. It can show due/do/open todo rows, but it cannot mark
  work done, postpone it, or adjust `due::`/`do::`.
- The Diagnostics panel can open source but cannot preview or apply `zorg-fix` plans even though `zorg-fix` already has
  deterministic source-span-backed fix plans and the CLI can emit JSON.
- Search can execute stored query IDs, but there is no panel that lists available `#z/query` zettel or lets users browse
  query definitions.
- The inspector shows row metadata and preview text, but does not use the existing link APIs for backlinks, outlinks,
  ancestors, or descendants.
- Index health is visible, but the dashboard does not detect an active `zorg watch`, tail watcher state, or auto-refresh
  after external index updates.
- Capture chooses the first available template and provides one-line fields. This is enough for MVP capture, but it is
  not yet a serious template selection workflow.

The important conclusion: the next round should make existing panels triage-capable before adding many more panel types.
Without scrolling, counts, and row actions, new panels will mostly add more content that cannot be navigated well.

## Highest-Value Next Features

### 1. Scrollable Lists, Row Counts, And Viewport State

Priority: highest. This is the usability foundation for every other dashboard feature.

Recommended shape:

- Add per-panel viewport state: selected index, scroll offset, visible row count, and total row count.
- Render main rows with a Ratatui `List` or `Table` plus explicit `ListState`, not a wrapped `Paragraph` of all rows.
- Show compact position text such as `7/43` in the main panel title or footer.
- Keep inspector selection stable when rows refresh by preserving the selected canonical ID/store row key when possible.
- Add page up/down and half-page movement. Keep `j/k`, arrows, `g/G`.
- Decide whether wrapped row text is allowed. For dense triage, prefer one row per item with truncation and put full
  message/body text in the inspector.

Why it is first:

- The dashboard already has enough rows to overflow common terminal sizes.
- Diagnostics, search results, saved queries, graph neighbors, and future import/export panels all need the same
  viewport model.
- It is mostly local to `model.rs`, `app.rs`, and `ui.rs`, and can be tested without adding new domain semantics.

Acceptance bar:

- Moving selection beyond the visible bottom scrolls the list.
- `--once` output for large fixture data includes a clear row count and does not wrap unrelated rows into each other.
- Search/refresh preserve selection when the same zettel remains present.
- Narrow terminal render tests still pass without text overlap.

### 2. Actionable Diagnostics And Fixes

Priority: high. The fixture corpus has 13 diagnostics; a daily cockpit should turn diagnostics into a repair queue.

Recommended shape:

- Add a selected-row action, for example `f`, that previews safe fixes relevant to the selected diagnostic or selected
  file.
- Add a Fixes overlay or panel that summarizes rule code, path, line/column, replacement preview, and whether the fix is
  preferred.
- Add confirmed apply, for example `F`, that delegates to the same `zorg-fix` planning/apply path used by CLI and LSP.
- After apply, refresh the dashboard snapshot and keep the user on Diagnostics or Today with updated counts.
- If no safe fix exists, show "no safe fix available" rather than pushing users to source blindly.

Why it is high value:

- Diagnostics are already first-class dashboard data, but today the only action is open-in-editor.
- `zorg-fix` already owns deterministic, idempotent, span-backed plans. The dashboard can add product value without
  inventing a rewrite engine.
- The Today panel currently includes diagnostics, so this feature improves both Today and Diagnostics.

Acceptance bar:

- A diagnostic with an available `fix.unresolved_absolute_link_typo` or formatting fix can be previewed and applied.
- Ambiguous or unsafe diagnostics remain read-only and explain why.
- Applying fixes exits raw/alt-screen safely if implementation shells out, or uses the shared Rust library boundary if
  it stays in-process.
- Tests cover preview, cancel, apply success, apply failure, and post-apply refresh.

### 3. Today Workflow Actions: Done, Postpone, And Schedule

Priority: high. This is the feature that turns `zorg dash` from a viewer into a daily work surface.

Recommended shape:

- Add explicit actions for selected todo rows:
  - `d`: mark done by changing marker to `[X]` and adding or updating `did::YYYY-MM-DD`.
  - `p`: postpone due/do date by a prompted interval such as tomorrow, next week, or a typed date.
  - `s`: schedule/set `do::YYYY-MM-DD` for an inbox or open todo.
- Implement the write side as a small safe planner, not as dashboard-specific string surgery.
- Reuse source spans from the indexed zettel row and reject rows without a known source span.
- Refresh after success and keep a log overlay showing the exact file and ID changed.

Why it is high value:

- The first question a daily dashboard answers is "what do I do now?" The second is "can I clear or reschedule it
  without context switching?"
- The existing Today queries already exclude `did:*`, so adding `did::today` naturally removes completed rows from
  Today after refresh.
- This feature will quickly reveal whether todo markers and lifecycle properties have enough source-span fidelity for
  higher-level workflows.

Acceptance bar:

- Mark-done modifies only the selected zettel and is idempotent.
- Postpone preserves unrelated properties, tags, title, body, and child zettel structure.
- Unsafe cases fail closed: missing source span, duplicate property ambiguity, parse errors after rewrite, or stale file
  content.
- CLI/library tests cover nested zettel, file zettel, existing `did::`, existing `due::`/`do::`, and rejected unsafe
  edits.

### 4. Saved Query Browser And Dashboards-As-Zettel

Priority: medium-high. Search already supports `@query/id`, but discoverability is missing.

Recommended shape:

- Add a Queries panel listing `#z/query` zettel with ID, title, query definition source, and row count preview.
- Pressing `enter` on a query should run it in Search; another binding can open the query zettel itself.
- Add optional `--query @id` polish after the panel exists: show the query title and resolved definition in the
  inspector, not only the raw `@id`.
- After the Queries panel is stable, add `zorg dash --as @dashboard/id` where a `#z/dashboard` zettel defines named
  panels backed by query IDs or inline SWOG.

Why it is high value:

- It keeps dashboard customization inside the Zorg model instead of introducing a parallel dashboard config file.
- Users who invest in saved queries get a payoff in the dashboard without memorizing IDs.
- It is a controlled path toward custom dashboards without building a plugin system.

Acceptance bar:

- Query zettel with `query::` properties and fenced `swog` blocks both appear.
- Invalid query definitions are listed with diagnostics instead of crashing the panel.
- Running a query from the Queries panel produces the same rows as `zorg query @id`.
- `--as` is deferred until the Queries panel and viewport model are stable.

### 5. Graph Neighborhood In The Inspector

Priority: medium. This is useful, bounded graph value without full visualization scope.

Recommended shape:

- Enrich zettel inspector details with outgoing links, incoming resolved backlinks, ancestors, and descendants using
  existing `Store::list_outgoing_links`, `Store::list_incoming_links`, `Store::list_zettel_ancestors`, and
  `Store::list_zettel_descendants`.
- Keep the first version in the inspector, not a separate graph canvas.
- Add a later Graph panel only after row virtualization exists.
- Bind `b`/`o` or inspector tabs only if the footer remains understandable.

Why it is valuable:

- Opening source is useful, but graph context is a core reason to use an indexed zettelkasten.
- Store APIs already exist, so the first version can stay read-only and low risk.
- It gives the right-size answer to "what is this connected to?" without attempting full terminal graph rendering.

Acceptance bar:

- Inspector shows bounded backlink/outlink counts and first N linked rows.
- Unresolved links are visible as unresolved, not silently omitted.
- The feature remains responsive on high-degree zettel by limiting display and loading details asynchronously.

### 6. Watcher Awareness And Live Refresh Hints

Priority: medium. Important for trust, but less immediately limiting than list ergonomics and row actions.

Recommended shape:

- Detect whether the index changed on disk since the current snapshot and show a "newer index available" hint.
- Add optional auto-refresh when idle, gated by a conservative interval and generation checks.
- If `zorg watch` exposes a stable status surface later, show watcher active/inactive state in the Index panel.
- Do not embed a writer in the dashboard until single-writer behavior is explicitly designed and tested.

Why it matters:

- The dashboard is read-only by default and documentation tells users to run `zorg watch` separately.
- A user should not have to wonder whether the dashboard is looking at fresh data after a save/reindex in another
  terminal.

Acceptance bar:

- External `zorg db reindex` or `zorg watch` activity can be noticed without corrupting read-only launch behavior.
- Auto-refresh never runs while the user is typing a search or editing capture fields.
- The Index panel explains stale/current/newer-index states in operational language.

### 7. Capture Template Selection And Better Capture Inputs

Priority: medium-low. Useful, but less central than triage and Today actions.

Recommended shape:

- Add a template picker instead of defaulting to the first template.
- Show template ID, title, destination, and required fields.
- Support multiline body capture only if terminal editing remains small and predictable; otherwise keep `$EDITOR` as the
  long-form path.
- Preserve the current rule that capture delegates to `zorg-capture`.

Why it is deferred:

- Basic capture already exists.
- The dashboard should not become a full editor.
- Template selection becomes more valuable after saved-query/dashboard customization clarifies the daily workflows users
  want to capture into.

## Cross-Cutting Gaps And Smaller Wins

These items are smaller than the seven major features above, but several are correctness or performance issues that
should not wait for a full feature phase. They were missed by the first pass.

### A. Color And Severity Styling

`crates/zorg-dash/src/ui.rs` uses only `Modifier::BOLD` and renders `error`, `warning`, and `info` rows identically. The
codebase has no `Color::` usages at all in the dash crate. A minimal palette (red/yellow/dim for diagnostic severity,
green for selection, dim for secondary metadata) would dramatically improve scanability before any new panels land.
Honor `NO_COLOR` and add `--no-color` for CI/non-TTY scenarios; the `--once` non-TTY path already exists.

### B. Status Surface: Keep Help Visible, Add A Log Ring

`render_footer` in `ui.rs` replaces the keybinding hint with the live `AppState.status` string, so users lose the help
line whenever any action runs. `AppState.status: String` is also overwritten by every event with no timestamp or
history. Two changes pay for themselves:

- Split the footer into a persistent keybinding line plus a separate status line, or render status above keys.
- Replace the single status string with a bounded ring (e.g. `VecDeque<StatusEvent>` of last ~50 entries with
  timestamps) and bind a log overlay (`L` or `:log`). Useful for after-the-fact triage of refresh/reindex/capture
  outcomes; mirrors lazygit/k9s.

### C. Input Editing In Capture And Search

`handle_capture_key` and the search edit handler in `app.rs` only push/pop characters from the end of the input. There
is no left/right cursor, `Home`/`End`, `Ctrl-W` word delete, or paste handling. They also do not filter `KeyEventKind`,
so on Windows key repeats fire on press and release. For any non-trivial query this becomes the first usability
complaint after lists overflow.

### D. Snapshot Loading And Preview Cost

Two perf issues compound:

- `lib.rs::load_frame` runs the initial `data::load_snapshot` synchronously on the main thread before drawing. Large
  corpora freeze startup with no visible "loading…" frame. The async worker channel already exists; reuse it for the
  first load and render an immediate placeholder frame.
- `data::query_zettel` calls `Store::list_zettel()` for the entire corpus on every query, just to build a
  `BTreeMap<id, preview>`. The Today panel triggers this three times per refresh because there are three Today query
  specs, plus once more for any active Search/Inbox/diagnostics-driven preview. Memoize previews per snapshot, or
  return preview text directly from the SWOG executor.

This is the largest scalability cliff in the current implementation and will be felt as soon as users index >5k zettel.

### E. Mouse Capture Is Enabled But Never Consumed

`TerminalGuard::enter` calls `EnableMouseCapture` whenever `--no-mouse` is not passed (default on), but the event loop
reads only `Event::Key`. Mouse events are silently swallowed: users cannot wheel-scroll, click to focus a panel, or
select text the way they expect inside a normal terminal pager. Either:

- Wire wheel events into the (forthcoming) viewport scroll, plus click-to-focus on panels, or
- Default `mouse: false` until viewport scrolling lands and let users opt in with a future `--mouse` flag.

Leaving the current behavior costs the user terminal-native text selection in many emulators.

### F. Filtering, Sorting, And Multi-Select Within Panels

The Diagnostics panel hard-sorts by severity rank/path and offers no filter; Today cannot toggle "due-only" vs
"do-only" even though badges already label rows. Two cheap additions unlock a lot of triage value:

- A panel-local filter line such as `f severity:error path:foo.z` and a sort cycle (`s` cycles severity/path/code).
- Multi-select via `space` to mark and `*` to mark-all-visible, stored on `AppState`. This is prerequisite for any
  future bulk fix-apply, bulk done, or bulk archive.

### G. Refresh/Reindex Progress, Timing, And Spinner

The async worker reports only a final `AsyncResult`. The status flips from "running" to "complete" with no elapsed
time, no row delta, and no animation. Add elapsed timing in the result line ("refresh complete in 230 ms, +3 / -1
rows") and drive a spinner glyph from the existing `poll(Duration::from_millis(100))` tick. Without this, "is anything
happening?" is a recurring question, especially for reindex.

### H. Search Quality Of Life: History, Help, Multi-Line Errors

`SearchPanel` keeps only the current `input/rows/error`. There is no `Up/Down` query history, no recent-queries
overlay, and no SWOG syntax cheatsheet. Parse errors are flattened to a single line in the row area, which loses any
multi-line context the engine might emit. Adding history (per-session is enough for a first pass, persisted later via
gap M), `?`/`F1` SWOG help overlay, and multi-line error rendering converts Search from a one-shot tool into the
primary triage path.

### I. Diagnostics Filtering And Severity-Aware Today

Even before fix-apply lands, users want "errors only" or "warnings of code X only". This is a 5-minute predicate over
`DiagnosticRow::sort_key`-aware data and pairs naturally with gap F. The Today panel currently merges diagnostics with
todo rows; a quick `t`/`d` toggle to scope Today to todos vs diagnostics avoids visual interleaving when both surfaces
are full.

### J. Row-Level Secondary Actions: Yank ID, Copy Path, Copy Source Link

`handle_key` only routes `Enter` to Open. Add:

- `y`: copy the selected zettel's `@id` (or diagnostic code) to the clipboard.
- `Y`: copy `path:line:col` source link.
- `c`: copy diagnostic message body.

Use `arboard` where available with an OSC 52 fallback so it works over SSH without extra dependencies. These are
trivial bindings that close the loop with editors, chat tools, and PR descriptions.

### K. First-Run And Empty-State Guidance

`empty_state` in `ui.rs` prints brief one-liners. There is no detection of an uninitialized corpus beyond a `Degraded`
message, and the user gets no actionable instruction. A first-run frame should display the resolved root, the resolved
DB path, and an explicit "press R to reindex / run `zorg db reindex`" call to action. This is the single highest-value
change for the "I just installed Zorg" path.

### L. Telemetry And JSON Frame Export

There is no in-process counter for refresh count, search latency, or fix-apply success rate, so "is the dashboard
slow?" cannot be answered without reproducing under tracing. A small `Telemetry { refreshes, last_refresh_ms,
search_p95_ms, ... }` exposed in the Index panel inspector would unblock self-tuning. Likewise, `--once` only emits
text; a `--once --json` mode that emits the structured frame would let scripts and external tools (Slack bots, status
pages, `gh-dash`-style aggregators) consume Zorg's daily view without screen-scraping.

### M. Persisted Dashboard State

`DashOptions` is constructed only from CLI args. Selected panel, last query, search history, and selection do not
survive restart. An opt-in JSON state file (e.g. under `${XDG_STATE_HOME}/zorg/dash.json`) for last panel, last query,
and recent searches would give returning users the same continuity that lazygit/k9s offer. Pair with gap H so history
persists across sessions.

### N. Test Coverage Gaps

Existing tests assert substring presence on `--once` output, but coverage is shallow. Concrete additions:

- Golden-file (snapshot) tests of `--once` frames per panel against a fixture larger than the visible area.
- Refresh debounce test under rapid `r` presses (the `pending_refresh` guard exists but is uncovered).
- Narrow-terminal capture overlay rendering.
- Combinations of `--no-mouse` and `--no-alt-screen` end-to-end through the CLI.
- Empty-corpus first-run frame.
- Panic-cleanup: a forced panic in interactive mode must restore the terminal and the prior panic hook.

### O. Comparable-Tool Inspirations

Several mature TUIs solve adjacent problems and are worth consulting for keymap/feature parity expectations:

- `lazygit`: status line + log overlay + per-panel keymaps + persisted `state.yml`.
- `k9s`: command bar (`:command`), filter prefix (`/`), and `?` help overlay.
- `gh dash`: query-driven sections defined as config; analogous to dashboards-as-zettel in feature 4.
- `taskwarrior-tui`: bulk operations and tag/priority filters over a task list.

Borrowing the command-bar / `?` help / `/` filter conventions costs little and immediately matches user muscle memory.

## Recommended Build Order

1. Scrollable lists, row counts, and viewport state. Bundle gaps A (color), B (status surface), and E (mouse capture
   wiring or default-off) in this phase since they share `ui.rs`/`lib.rs` surface area and are cheap once the viewport
   model lands.
2. Actionable diagnostics/fixes. Bundle gap I (severity/rule filter) and gap F multi-select since fix-apply benefits
   most from selection plumbing.
3. Today workflow actions: done, postpone, schedule. Bundle gap G (timing/spinner) and gap J (yank/copy bindings)
   because both touch the same key-routing layer and are user-facing wins per release.
4. Snapshot/preview perf rework (gap D). Pull this earlier than feature expansion if real-corpus profiling shows
   `list_zettel` cost dominating refresh time.
5. Saved Queries panel. Bundle gap H (search history/help/multi-line errors) and gap C (input editing) since the same
   input widget is used.
6. Graph neighborhood in the inspector.
7. Watcher awareness/live refresh hints. Bundle gap K (first-run/empty-state) and gap L (telemetry + `--once --json`)
   because they share the Index panel/inspector area.
8. Dashboard-as-zettel and capture template picker. Bundle gap M (persisted state) once the user-facing state surface
   is large enough to be worth saving.

Run gap N (test coverage additions) continuously; do not save it for a final phase. This order deliberately improves
the current MVP before expanding it. It also keeps the dashboard aligned with the existing product rule: parser,
query, fix, refactor, capture, and store semantics stay in shared Rust crates; the dashboard remains a thin
operational UI.

## Work To Defer

- Embedded long-form editing. Keep `$EDITOR` as the editing surface.
- Full terminal graph visualization. Start with neighborhood context.
- A dashboard JSON API. Keep `zorg query --json`, `zorg db status`, and watch JSON as machine contracts.
- A web/Tauri/Textual rewrite. The native Rust dashboard is now present and should be matured before another runtime is
  introduced.
- Themes and mouse-first interaction. They are polish, not the main adoption constraint.
- Embedded watcher writer mode. Read-only dashboard launch is still the safest default.

## Suggested Next Epic Shape

If this becomes a new implementation plan, split it into focused phases:

- Phase A: viewport/list model and render migration; color/severity styling; status-surface split with log ring;
  decide on mouse capture (wire wheel/click or default off).
- Phase B: diagnostics fix preview/apply; severity/rule filter; multi-select state.
- Phase C: safe todo action planner plus dashboard bindings; refresh/reindex timing and spinner; yank/copy bindings.
- Phase D: snapshot/preview perf rework (preview memoization or SQL-side join), driven by profiling on a corpus
  larger than fixtures.
- Phase E: Queries panel and query inspector; persistent search history; richer input editing; multi-line error
  rendering.
- Phase F: graph neighborhood inspector; watcher freshness hints; first-run guidance; `--once --json` and inline
  telemetry.
- Phase G: dashboard-as-zettel and capture template picker; persisted dashboard state.

Each phase should end with `cargo fmt --check`, `cargo test -p zorg-dash`, relevant CLI smoke tests, and at least one
`zorg dash --once` frame against a corpus with more rows than fit on screen. Add a snapshot-style golden test of the
`--once` frame for each new panel surface so future regressions are caught at the buffer level rather than via ad-hoc
`assert!(rendered.contains(...))` checks.

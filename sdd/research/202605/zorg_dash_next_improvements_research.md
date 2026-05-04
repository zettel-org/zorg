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
recommendation: prioritize list ergonomics, actionable diagnostics/fixes, and Today workflow actions before adding broad new dashboard surfaces
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

## Recommended Build Order

1. Scrollable lists, row counts, and viewport state.
2. Actionable diagnostics/fixes.
3. Today workflow actions: done, postpone, schedule.
4. Saved Queries panel.
5. Graph neighborhood in the inspector.
6. Watcher awareness/live refresh hints.
7. Dashboard-as-zettel and capture template picker.

This order deliberately improves the current MVP before expanding it. It also keeps the dashboard aligned with the
existing product rule: parser, query, fix, refactor, capture, and store semantics stay in shared Rust crates; the
dashboard remains a thin operational UI.

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

- Phase A: viewport/list model and render migration.
- Phase B: diagnostics fix preview/apply.
- Phase C: safe todo action planner plus dashboard bindings.
- Phase D: Queries panel and query inspector.
- Phase E: graph neighborhood inspector and watcher freshness hints.

Each phase should end with `cargo fmt --check`, `cargo test -p zorg-dash`, relevant CLI smoke tests, and at least one
`zorg dash --once` frame against a corpus with more rows than fit on screen.

---
plan_name: epic21_saved_query_browser_and_search_ux
bead_id: zorg-6.5
tier: epic
legend_bead_id: zorg-6
legend: sdd/legends/202605/zorg_dash_next_improvements.md
epic: 21
created: 2026-05-04
status: proposed
create_time: 2026-05-04 01:55:50
prompt: sdd/prompts/202605/epic21_saved_query_browser.md
---

# Epic 21: Saved Query Browser And Search UX

## Goal

Implement epic 21 from `sdd/legends/202605/zorg_dash_next_improvements.md`: make saved `#z/query` zettel discoverable in
`zorg dash`, allow users to run stored queries without memorizing IDs, improve Search input editing and history, and
make query errors/help useful inside the TUI.

This plan intentionally defers persisted search history to Epic 23. Epic 21 should keep history per process/session so
`--once` output and tests remain deterministic unless a query is explicitly supplied.

## Current Baseline

The current checkout already includes the earlier dashboard foundation that this epic should build on:

- `Panel` currently has Today, Inbox, Search, Diagnostics, and Index.
- `DashboardSnapshot::Ready` stores `today`, `inbox`, `search`, `diagnostics`, and `index`, but no query catalog.
- `SearchPanel` stores only `input`, `rows`, and an optional flat `error`.
- Search already accepts inline SWOG and stored `@id` through `execute_query_by_id`.
- `zorg-query` exposes `query_definition_by_id`, `execute_query_by_id`, and `parse_output_query`, but does not expose an
  enumeration API for all saved query definitions.
- `query_definition_by_id` reparses the source file to extract `query::` or a direct fenced `swog` block and returns
  definition errors for invalid/missing/ambiguous definitions.
- Dashboard input editing for Search is currently append/pop only. It does not track a cursor, Home/End, Delete, Ctrl-W,
  history, or key event kind filtering.
- Existing UI tests already render panels through `TestBackend`; CLI smoke tests already cover dash Search with
  `--query @queries/inbox`.

## Scope Decisions

Implement five phases. The legend suggested four, but the shared query boundary and dashboard integration are separate
enough that splitting them lowers coordination risk for distinct future agent instances.

Keep the main implementation boundaries:

- `zorg-query` owns saved query discovery/extraction semantics.
- `zorg-dash/src/data.rs` owns snapshot loading, query catalog loading, row count previews, and search metadata loading.
- `zorg-dash/src/model.rs` owns query catalog view models, stored-query metadata on `SearchPanel`, reusable input state,
  history state, and new overlay types.
- `zorg-dash/src/app.rs` owns panel routing, key handling, async search scheduling, history recall, and run/open
  actions.
- `zorg-dash/src/ui.rs` owns Ratatui rendering only.

Do not add a dashboard-specific parser, do not parse CLI text output, and do not create persistent state files in this
epic.

## Phase 21A: Shared Query Catalog API

### Objective

Add a `zorg-query` API that can enumerate saved query zettel and report per-definition success or failure without making
dashboard snapshot loading crash because one query definition is invalid.

### Main Work

- Add public model types in `crates/zorg-query/src/lib.rs`, with names along these lines:
  - `QueryDefinitionSourceKind` for `query::` property vs fenced `swog` block.
  - `QueryDefinitionSummary` for valid definitions, including zettel ID, source path, title when indexed, extracted
    query, source kind, source span, and output kind.
  - `QueryDefinitionListing` or equivalent wrapper for either a valid summary or a `QueryDefinitionError`.
- Add a public enumeration function, for example
  `list_query_definitions(store: &Store) -> Result<Vec<QueryDefinitionListing>, QueryExecutionError>`.
- Reuse the same extraction and validation rules as `query_definition_by_id`.
- Preserve deterministic ordering by indexed file/source order.
- Group source reparsing by file when practical, so multiple query zettel in one file do not repeatedly read/parse the
  same source.
- Keep per-query definition problems as row-level errors. Reserve function-level errors for store access or file access
  failures that prevent enumeration from being meaningful.
- Add unit or integration tests covering:
  - `query::` property definitions.
  - fenced `swog` definitions.
  - invalid syntax.
  - missing definition.
  - duplicate/ambiguous definition forms.
  - deterministic ordering.
  - consistency with `query_definition_by_id` for a valid ID.

### Acceptance

- A caller can enumerate all indexed `#z/query` zettel.
- Invalid query zettel are returned as listing rows with error details.
- Valid query listings include the source form and output kind.
- Existing `zorg query --id` behavior remains unchanged.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-cli --test smoke query
```

## Phase 21B: Dashboard Queries Panel Data And Rendering

### Objective

Introduce a first-class Queries panel that displays saved query definitions, including invalid definitions, without yet
adding advanced Search editing behavior.

### Main Work

- Extend `Panel` with `Queries`, update `Panel::ALL`, labels, values, indexes, CLI `--panel` parsing, help text, README,
  row counts, viewport arrays, and smoke expectations.
- Extend `DashboardSnapshot::Ready` with a `queries` collection.
- Add `QueryRow` and `QueryPanel` model types in `zorg-dash`:
  - stable row identity based on query zettel ID.
  - ID, title, relative source path, source kind, output kind, definition preview, validity, error detail, and optional
    row count preview.
  - source location for opening the query zettel.
- Add `PanelRow::Query` and render list/inspector lines for valid and invalid query rows.
- Load query rows in `data::load_ready_snapshot` through the new `zorg-query` enumeration API.
- For valid definitions, compute a best-effort row count preview during snapshot load when feasible. If execution fails,
  keep the query row visible and display the count error in the inspector instead of degrading the whole dashboard.
- Add empty-state guidance for the Queries panel.
- Add render tests for:
  - valid property and fenced query rows.
  - invalid query rows.
  - source kind and definition preview in the inspector.
  - narrow terminal rendering without overlapping footer/status.
- Add CLI smoke coverage for `zorg dash --once --panel queries`.

### Acceptance

- `zorg dash --panel queries --once` renders a Queries panel.
- Query zettel using `query::` properties and fenced `swog` blocks both appear.
- Invalid saved queries are visible with useful error text.
- Query row count preview is shown when it can be computed, and a preview failure is non-fatal.
- The normal dashboard snapshot is not degraded by one invalid query definition.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel queries
```

## Phase 21C: Query Run/Open Actions And Stored-Query Search Inspector

### Objective

Make the Queries panel actionable and make Search explain stored `@id` queries.

### Main Work

- Change action semantics only for the Queries panel:
  - `Enter` on a valid query row switches to Search, sets the Search input to `@<id>`, and starts/runs the search.
  - Invalid query rows should not run; record a warning and keep selection stable.
  - Add a separate source-open binding for query rows, preferably `o`, and document it. Keep existing `Enter`
    source-open behavior for non-Queries panels unless the implementation chooses a broader explicit-open convention and
    updates tests/docs accordingly.
- Add `SearchQueryInfo` or similar metadata to `SearchPanel` for stored queries:
  - stored query ID, title, source path, source kind, resolved definition, output kind, and definition error if any.
- Update `data::search_panel` so stored `@id` searches attach metadata from `query_definition_by_id` or the new listing
  helper. Inline searches should keep metadata empty.
- Render stored-query metadata in the Search header and inspector:
  - keep the raw input visible.
  - show title/source/definition for valid stored queries.
  - show definition errors for invalid stored query IDs without flattening away context.
- Preserve selection and viewport behavior when a query is run from Queries and Search results arrive asynchronously.
- Add app tests for `Enter` from Queries, invalid query rejection, and `o` source-open.
- Add smoke coverage that compares the visible Search results from `zorg dash --query @id` with `zorg query --id @id`
  using the same fixture corpus.

### Acceptance

- Pressing `Enter` on a valid Queries row runs the same stored query as `zorg query --id @id`.
- Pressing `Enter` on an invalid Queries row does not crash and gives a clear warning/error.
- A separate binding opens the query zettel source.
- Search inspector shows stored query title, source form, and resolved definition for `@id`.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke query
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel search --query '@queries/inbox'
```

## Phase 21D: Reusable Input Editor And Per-Session Search History

### Objective

Replace append/pop Search editing with a reusable single-line editor and add per-session query history recall.

### Main Work

- Add a reusable single-line input model in `zorg-dash/src/model.rs` or a small new module if that keeps `model.rs`
  manageable:
  - text buffer.
  - cursor position as a character boundary.
  - insertion at cursor.
  - Backspace and Delete.
  - Left/Right movement.
  - Home/End.
  - Ctrl-W delete previous word.
  - Ctrl-U clear before cursor or clear whole line, choosing one behavior and documenting it.
  - paste-safe insertion for sequential character events.
  - `KeyEventKind` filtering so key release/repeat behavior does not duplicate input.
- Wire Search editing to this model.
- Add a per-session `SearchHistory` model:
  - add non-empty committed queries when Enter runs a search or a query is run from Queries.
  - de-duplicate adjacent repeats.
  - bound memory with a small limit such as 50.
  - Up/Down recall while Search editing.
  - preserve in-progress draft while navigating history.
- Keep debounce behavior for typed edits, but make Enter commit immediately.
- Add app/model tests for cursor edits, word deletion, history recall, Enter commit, Esc cancel, and key kind filtering.
- Update footer/help/README Search editing documentation.

### Acceptance

- Search can edit inside the line rather than only at the end.
- Home/End/Delete/Backspace/Ctrl-W/Ctrl-U work predictably.
- Up/Down recalls recent searches during Search editing.
- Running a query from Queries adds it to session history.
- Key release/repeat events do not cause duplicate characters.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 21E: SWOG Help Overlay And Multi-Line Query Errors

### Objective

Make failed searches and SWOG syntax support readable in the TUI.

### Main Work

- Add a SWOG help overlay or Search-specific help tab reachable from Search. Candidate bindings:
  - keep global `?` for dashboard help.
  - add `F1` or `H` from Search for SWOG help, or make `?` inside Search editing open SWOG help if that does not
    conflict with typing literal `?`.
- Populate help from stable built-in text, not by scraping external docs:
  - basic tags like `#z/todo`.
  - property filters like `due:<=today`, `did:*`.
  - todo filters like `todo:[ ]`.
  - links/file/text/modified examples.
  - boolean grouping and `OR`.
  - stored query IDs with `@queries/foo`.
  - output forms `TABLE <query>` and `count(<query>)` if supported in Search.
- Replace flat query error rendering with structured multi-line presentation:
  - preserve line breaks from `Display` messages or add a small formatter for parse/definition errors.
  - show the raw input and resolved stored definition context when available.
  - keep the list header compact and place detailed multi-line errors in the inspector/log overlay when needed.
- Add render tests for multi-line parse errors, stored-query definition errors, and help overlay in narrow and normal
  widths.
- Update `docs/query.md` only if dashboard-specific help text should point to it; otherwise keep docs changes to
  `crates/zorg-dash/README.md`.

### Acceptance

- Search users can open SWOG help without leaving the dashboard.
- Multi-line query errors remain readable and do not collapse into one lossy line.
- Stored-query definition errors show the query zettel ID and source path when available.
- Narrow terminal render tests pass with the help overlay visible.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke query
cargo test -p zorg-cli --test smoke dash
```

## Cross-Phase Coordination Notes

- Phase 21A should land before 21B. All other phases depend on the query row/search metadata shapes established in
  21B/21C.
- Each future agent should start by reading the current `crates/zorg-dash/src/model.rs`, `app.rs`, `data.rs`, and
  `ui.rs`; this area is active and likely to have changed after prior phases.
- Do not duplicate saved-query parsing in `zorg-dash`. If a needed value is missing from the shared API, extend
  `zorg-query` rather than reparsing source in the dashboard.
- Avoid introducing persistent dashboard state in Epic 21. Leave persisted history, last panel, and last query for
  Epic 23.
- The Queries panel should remain read-only except for explicit open/run actions.
- Search result rows should continue to use the same `ZettelRow` rendering and viewport behavior as the current Search
  panel.

## Final Epic Acceptance

- `zorg dash --once --panel queries` lists saved query zettel from the indexed corpus.
- Both `query::` property definitions and fenced `swog` definitions are shown.
- Invalid query definitions appear as rows with errors and do not degrade or crash dashboard loading.
- Running a saved query from Queries produces the same result set as `zorg query --id @id`.
- Search supports cursor editing and per-session history recall.
- Search inspector explains stored `@id` queries with title, source, and resolved definition.
- Multi-line query errors and SWOG help are readable in the TUI.

## Final Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke query
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel queries
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel search --query '@queries/inbox'
```

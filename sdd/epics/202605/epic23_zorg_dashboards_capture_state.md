---
plan_name: epic23_zorg_dashboards_capture_state
bead_id: zorg-6.7
tier: epic
legend_bead_id: zorg-6
legend: sdd/legends/202605/zorg_dash_next_improvements.md
epic: 23
created: 2026-05-04
status: done
source:
- sdd/legends/202605/zorg_dash_next_improvements.md
- sdd/epics/202605/epic22_zorg_dash_graph_freshness_json.md
- crates/zorg-dash/README.md
- crates/zorg-dash/src/lib.rs
- crates/zorg-dash/src/app.rs
- crates/zorg-dash/src/actions.rs
- crates/zorg-dash/src/data.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/ui.rs
- crates/zorg-dash/src/json.rs
- crates/zorg-capture/src/lib.rs
- crates/zorg-query/src/lib.rs
- crates/zorg-cli/src/main.rs
- crates/zorg-cli/tests/smoke.rs
create_time: 2026-05-04 07:33:21
prompt: sdd/prompts/202605/epic23_zorg_dashboards_capture_state.md
---

# Epic 23: Dashboards-As-Zettel, Capture Template Picker, And Persisted State

## Goal

Implement Epic 23 from `sdd/legends/202605/zorg_dash_next_improvements.md`: let users define custom dashboard panels as
ordinary Zorg zettel, make dashboard capture choose from available templates, and persist useful interactive state once
that state has enough surface area to justify saving.

The dashboard should remain a native Rust Ratatui TUI over shared Zorg semantics. This epic should not introduce a
parallel dashboard config language, embedded long-form editor, plugin system, or long-lived automation API.

## Current Baseline

The current checkout includes the previous dashboard improvement work:

- `zorg dash` has built-in Today, Inbox, Queries, Search, Diagnostics, and Index panels.
- `Panel` is currently a closed enum with `Panel::ALL`, fixed numeric indexes, and fixed `--panel` parsing.
- `DashboardSnapshot::Ready` carries built-in panel data only: index, diagnostics, Today, Inbox, Queries, and Search.
- `DashboardFrame::active_rows` and `rows_for_panel` derive rows from the active built-in panel and existing filters.
- Per-panel viewport state is held in `AppState` as an array sized by `Panel::ALL.len()`.
- Search history is per-session only through `SearchHistory`.
- The JSON one-shot frame exports `Panel::ALL` and the selected frame, but has no custom dashboard metadata.
- Capture currently opens a small form seeded by `actions::capture_defaults`, which picks the first discovered template.
- `zorg-capture` already exposes `list_templates` and `inspect_template`, including template ID, title, path, and
  required variables.
- `zorg-query` already exposes `query_definition_by_id`, `list_query_definitions`, and stored-query execution helpers.
- CLI interactive capture already has a template picker, but the dashboard does not.

## Scope Decisions

Use seven phases. The legend suggested four, but the current architecture makes a smaller split safer for distinct agent
instances:

1. Dashboard definition model and parser.
2. Dynamic panel registry and CLI `--as`/`--panel` integration.
3. Custom panel query execution, rendering, inspectors, and JSON.
4. Capture template picker.
5. Capture form improvements after template selection.
6. Persisted state storage and restore.
7. Documentation, smoke coverage, and final compatibility pass.

This split keeps shared-crate semantics separate from dashboard state rewiring, avoids mixing capture UI with custom
panel plumbing, and lets persistence land after the state model is stable.

## Product Semantics

Define dashboard zettel as ordinary `#z/dashboard` zettel. Prefer a compact property-based format first, with optional
inline SWOG fences only where query definitions already support them.

A first implementation should support named query-backed panels with these semantics:

- A dashboard zettel is selected by canonical ID through `zorg dash --as @dashboard/id`.
- The selected zettel must carry effective tag `#z/dashboard`.
- Dashboard title comes from `title::` when present, then canonical ID.
- Panel definitions are ordered in source order.
- A panel has a stable key, a display title, and exactly one query source:
  - stored query ID such as `@queries/inbox`, or
  - inline SWOG text from a dashboard panel block.
- Built-in panels remain available by default unless the dashboard definition explicitly disables or replaces them.
- Invalid dashboard definitions produce dashboard-local diagnostic rows and visible panel errors; they do not degrade
  the entire snapshot.

The exact source syntax can be finalized in Phase 23A, but it should optimize for simple authoring and parser stability.
Do not add a TOML/YAML/JSON config file under XDG for dashboard layout.

## Cross-Phase Design

Keep ownership aligned with the existing dashboard architecture:

- `model.rs` owns dashboard definition view models, dynamic panel identities, custom panel rows, capture picker state,
  persisted state structs, and restored UI preferences.
- `data.rs` owns read-only loading of dashboard definitions and query-backed custom panels through `zorg-store` and
  `zorg-query`.
- `actions.rs` owns capture template discovery and capture execution, delegating to `zorg-capture`.
- `app.rs` owns CLI option application, dynamic panel navigation, viewport preservation, capture picker routing, and
  persistence load/save timing.
- `ui.rs` owns rendering only: dynamic tab labels, custom panel rows, dashboard-local diagnostics, picker overlays, and
  persistence-related status text.
- `json.rs` owns one-shot frame export additions for selected dashboard and custom panel metadata.
- `lib.rs` owns CLI flags and terminal/once behavior.
- `zorg-capture` should change only if dashboard template selection needs metadata that cannot be obtained from the
  current `CaptureTemplate`.
- `zorg-query` should change only if dashboard definition parsing can cleanly reuse query-definition extraction or needs
  a small shared helper.

Avoid array indexing by a fixed `Panel::ALL` once custom panels exist. Introduce stable panel IDs and dynamic panel
lists before adding query-backed panels. Preserve built-in key behavior and keep custom panels visually consistent with
Search/Inbox row rendering.

## Phase 23A: Dashboard Definition Model And Parser

### Objective

Define and validate `#z/dashboard` zettel semantics without changing the TUI navigation surface yet.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-query/src/lib.rs` only for a narrow reusable helper, if needed
- fixtures containing valid and invalid `#z/dashboard` examples

### Main Work

- Add model types such as `DashboardDefinition`, `DashboardPanelDefinition`, `DashboardPanelQuerySource`,
  `DashboardDefinitionDiagnostic`, and `SelectedDashboard`.
- Add a data loader for `zorg dash --as @id` that:
  - resolves canonical ID through the indexed store,
  - verifies effective `#z/dashboard`,
  - reads and parses the source zettel,
  - extracts panel definitions in source order,
  - validates duplicate panel keys, missing titles, missing query sources, invalid stored query IDs, and invalid inline
    SWOG.
- Reuse existing `zorg-query` parsing and definition semantics where possible instead of creating dashboard-only query
  behavior.
- Return row-level/local diagnostics for invalid panels and definitions while preserving any valid panels.
- Add tests for valid stored-query panels, valid inline SWOG panels, missing dashboard ID, wrong tag, duplicate keys,
  invalid stored query, invalid inline SWOG, and mixed valid/invalid panels.

### Acceptance

- A `#z/dashboard` zettel can be loaded by canonical ID into a stable dashboard definition model.
- Definition problems are represented as local diagnostics instead of panics or whole-dashboard degradation.
- No TUI behavior changes are required yet.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
```

## Phase 23B: Dynamic Panel Registry And CLI Integration

### Objective

Replace fixed built-in panel assumptions with a dynamic panel registry that can include custom panels, then wire
`zorg dash --as @dashboard/id` and custom `--panel` selection.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/json.rs`
- `crates/zorg-cli/tests/smoke.rs`

### Main Work

- Introduce a stable `PanelId` or equivalent with built-in and custom variants.
- Replace fixed `Panel::ALL` assumptions in frame rendering, navigation, row counts, JSON panel lists, and app viewport
  storage.
- Keep built-in labels and `--panel today|inbox|queries|search|diagnostics|index` compatible.
- Add `--as @dashboard/id` parsing and carry the selected dashboard ID through config, loading, loading-frame creation,
  async initial load, refresh, reindex, search, JSON, and status surfaces.
- Allow `--panel custom-key` when used with `--as`; invalid panel selection should explain available built-in and custom
  panel keys.
- Ensure `tab`, `backtab`, left/right navigation, per-panel viewport preservation, and row identity preservation work
  with dynamic panel lists.
- Add tests covering built-in-only behavior, dynamic panel ordering, custom panel selection, invalid custom panel, and
  viewport preservation across built-in/custom panel switching.

### Acceptance

- Existing dashboard launches and tests behave the same without `--as`.
- `zorg dash --as @dashboard/id` loads a selected dashboard definition and exposes a dynamic panel list.
- Built-in panel keys remain backward compatible.
- Custom panel IDs are stable enough for persisted state and JSON export.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 23C: Custom Query-Backed Panels

### Objective

Execute and render custom dashboard panels backed by stored query IDs or inline SWOG, using the same row model and
inspector behavior as built-in zettel query panels.

### Likely Files

- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/json.rs`
- `crates/zorg-dash/README.md`

### Main Work

- Extend `DashboardSnapshot::Ready` with custom panel data keyed by stable custom panel ID.
- Execute each valid custom panel query through existing `zorg-query` APIs during snapshot load.
- Reuse `ZettelRow`, preview memoization, graph inspector enrichment, selection preservation, and viewport behavior.
- Represent per-panel query errors as visible custom panel error rows or dashboard-local diagnostics.
- Show custom panel title, query source, row count, and error state in the panel title/inspector.
- Include selected dashboard ID, custom panel metadata, active rows, and selected inspector details in `--once --json`.
- Add render tests for custom panels, empty custom query results, query errors, narrow layouts, and JSON output.

### Acceptance

- Query-backed custom panels render zettel rows like Inbox/Search and support the existing zettel inspector.
- Invalid custom panel queries do not prevent valid custom panels or built-in panels from rendering.
- JSON one-shot output includes enough custom panel metadata for scripts to identify the selected dashboard and panel.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-query
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3 --once --as @dashboards/daily
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3 --once --json --as @dashboards/daily
```

## Phase 23D: Capture Template Picker

### Objective

Replace the dashboard's "first template" capture default with an interactive template picker that lists available
templates before opening the existing capture form.

### Likely Files

- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-capture/src/lib.rs` only if template metadata must be extended
- `crates/zorg-dash/README.md`

### Main Work

- Add `CaptureTemplateRow`, `CaptureTemplatePicker`, or equivalent dashboard model state.
- Add an action to list templates through `zorg_capture::list_templates`.
- On `c`, open a picker overlay when multiple templates exist; optionally skip directly to the form when exactly one
  template exists.
- Show template ID, title, destination if available, source path, and required variables.
- Support picker navigation with existing list keys, `enter` to choose, `esc` to cancel, and stable narrow rendering.
- After selection, seed the existing `CaptureDraft` from the selected template.
- Preserve existing capture execution behavior after the draft is submitted.
- Add tests for no templates, one template, multiple templates, invalid/unselectable template metadata, picker
  navigation, cancel, and selection.

### Acceptance

- Dashboard capture lets the user choose among available templates before filling fields.
- The picker exposes enough metadata to choose the correct template.
- Existing capture creation still delegates to `zorg-capture`.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-capture
cargo test -p zorg-dash
```

## Phase 23E: Capture Form Completion And Template Inspector

### Objective

Make the capture flow respond to selected template metadata while staying a compact form, not an embedded editor.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`

### Main Work

- Use selected template metadata to show required fields and template destination in the form.
- Prompt only for variables required by the selected template, while retaining current title/body/destination support
  for compatibility.
- Add a template inspector area or detail lines in the picker/form for required variables and destination.
- Keep long-form body capture minimal; if the body is large or multiline editing is needed, direct users through
  `$EDITOR` or `zorg capture`.
- Record capture success/failure status events with selected template ID/title.
- Refresh after capture and preserve active custom dashboard/panel context.
- Add narrow render tests for picker and form states.

### Acceptance

- Capture form fields match selected template requirements where feasible.
- Capture success refreshes the current dashboard, including custom panels.
- The dashboard does not become a full text editor.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-capture
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 23F: Persisted Dashboard State

### Objective

Persist useful interactive dashboard state under an XDG-appropriate path without changing deterministic one-shot output.

### Likely Files

- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/data.rs` or new state module if the implementation grows
- `crates/zorg-dash/src/json.rs` only if state metadata is exposed
- `crates/zorg-dash/README.md`

### Main Work

- Choose an XDG state path, for example `$XDG_STATE_HOME/zorg/dash/state.json` or the platform-appropriate equivalent
  through a small dependency if already acceptable in the workspace.
- Add a versioned persisted state model containing:
  - last selected panel ID,
  - last search query,
  - recent search history,
  - selected dashboard ID,
  - small UI preferences such as color mode, mouse mode, and maybe auto-refresh preference only if unambiguous.
- Add flags:
  - `--no-state` to disable load and save,
  - optionally `--state PATH` for tests and power users,
  - keep `--once` state-neutral by default.
- Load state before initial frame selection in interactive mode only, then let explicit CLI flags override restored
  values.
- Save state on clean interactive exit and after important state changes if needed; do not save on `--once` unless a
  future explicit flag is added.
- Handle missing, corrupt, old-version, or partially invalid state by warning in the status log and continuing with
  defaults.
- Keep persisted state scoped to dashboard UI state, not corpus data or query results.
- Add tests for load defaults, CLI override precedence, corrupt state, version mismatch, save-on-exit, no-state mode,
  and deterministic `--once`.

### Acceptance

- Interactive launches can restore last panel, query, recent searches, selected dashboard, and small UI preferences.
- Explicit CLI flags always override persisted state.
- `--once` remains deterministic and does not read or write state by default.
- Corrupt state cannot prevent dashboard launch.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 23G: Documentation, Compatibility, And Final Smoke Coverage

### Objective

Close the epic with user-facing docs, complete smoke coverage, and a compatibility pass over built-in and custom
dashboard behavior.

### Likely Files

- `crates/zorg-dash/README.md`
- docs describing `#z/dashboard`, likely `docs/query.md` or a new dashboard docs section if warranted
- `crates/zorg-cli/tests/smoke.rs`
- fixtures under `fixtures/corpus`
- `sdd/epics/202605/epic23_zorg_dashboards_capture_state.md` if the submitted plan is promoted into the SDD tree
- `sdd/prompts/202605/epic23_zorg_dashboards_capture_state.md` if a future prompt artifact is needed

### Main Work

- Document `#z/dashboard` authoring syntax with valid and invalid examples.
- Document `zorg dash --as`, custom `--panel`, capture picker behavior, and persisted state flags.
- Add CLI smoke coverage for:
  - built-in launch without state,
  - `--as @dashboard/id --once`,
  - `--as @dashboard/id --panel custom-key --once`,
  - invalid dashboard ID,
  - invalid custom panel displayed without crash,
  - `--once --json --as @dashboard/id`,
  - capture picker render paths where smoke testing is feasible,
  - `--no-state` behavior.
- Run workspace validation and a manual fixture command sequence.
- Check that help text, README, JSON schema marker, and key binding docs agree.

### Acceptance

- Users can author and run dashboard zettel from docs alone.
- The final implementation meets the Epic 23 acceptance criteria in the legend.
- Existing built-in dashboard workflows remain compatible.
- CI-friendly one-shot output stays deterministic.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-capture
cargo test -p zorg-query
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3 --once --as @dashboards/daily
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic23.sqlite3 --once --json --as @dashboards/daily
```

## Risks And Mitigations

- Dynamic panels touch many fixed-panel assumptions. Mitigate by introducing `PanelId`/registry first, keeping built-in
  behavior covered before loading custom panels.
- Dashboard syntax can accidentally become a second config language. Mitigate by using ordinary zettel tags, properties,
  source order, and SWOG query definitions.
- Custom panel query loading can make snapshot refresh expensive. Mitigate by preserving preview memoization and
  bounding invalid/error work per panel.
- Persisted state can make tests flaky. Mitigate by making `--once` state-neutral, adding `--no-state`, and supporting a
  test state path.
- Capture picker may duplicate CLI logic. Mitigate by using `zorg-capture::list_templates`/`inspect_template` and only
  adding shared metadata when needed.

## Out Of Scope

- Embedded long-form editing.
- Dashboard theme/plugin systems.
- A separate config file format for dashboards.
- Background watcher ownership or writer mode.
- Full graph visualization.
- Streaming dashboard automation beyond existing `--once --json`.

## Done Criteria

Epic 23 is done when:

- `zorg dash --as @dashboard/id` loads ordinary `#z/dashboard` zettel and renders custom query-backed panels beside
  built-in panels.
- Invalid dashboard definitions and panel queries are visible as actionable local diagnostics.
- Custom panels use existing row rendering, viewport behavior, inspectors, graph context, refresh, and JSON frame export
  patterns.
- Capture starts with a template picker and then delegates actual capture to `zorg-capture`.
- Interactive state restores last panel/query/history/dashboard/preferences while explicit flags override state and
  `--once` stays deterministic.
- Docs, help text, smoke tests, and workspace tests agree with the implemented behavior.

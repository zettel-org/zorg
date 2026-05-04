---
create_time: 2026-05-04 14:10:44
status: wip
prompt: sdd/prompts/202605/zorg_dash_epic_26_rich_rows.md
---
# Zorg Dash Epic 26 Rich Row Rendering Plan

## Context

Implement Epic 26 from `sdd/legends/202605/zorg_dash_visual_refresh_1.md`: replace monolithic main-panel row strings
with span-based row rendering and semantic badges for diagnostics, zettels/todos, query/custom rows, and index status
rows.

The current code already has much of the Epic 24/25 foundation:

- `crates/zorg-dash/src/ui.rs` defines `DashTheme`, semantic style tokens, block helpers, and span helpers.
- Main rows are still rendered through `row_items`, `row_list_line(row, frame) -> String`, and
  `row_style(row, palette) -> Style`.
- `row_items` currently applies a single row style to all content, so diagnostic rows and invalid query rows are still
  effectively whole-line colored.
- Selection is applied by wrapping whole prefix/content spans with `selected_span`.
- Marked diagnostics keep the `*` prefix, but there is no row-wide marked composition beyond that prefix.
- Model row structs already expose enough fields for most rendering through
  `PanelRow::{Zettel, Query, Diagnostic, IndexStatus}` and their inner public fields.
- `PanelRow::list_line()` in `model.rs` is still useful as a plain-text fallback and for non-rendering row summaries, so
  this epic should avoid removing it unless a later phase proves it is dead.

This plan intentionally keeps all behavior presentation-only. It must not change row membership, sorting, dashboard
state, write behavior, JSON output, or default `--once` plain-text semantics beyond small spacing differences.

## Phase Strategy

Use five sequential phases. Each phase is intended for a distinct agent instance and should be landed before the next
starts. Do not run these phases in parallel because the main write target is `crates/zorg-dash/src/ui.rs`.

Every phase should:

- Preserve one `ListItem` per logical `PanelRow`.
- Preserve meaningful row text in `buffer_to_string` and `--once` output.
- Keep `NO_COLOR=1` / `--no-color` strict: all rendered cells must have reset fg/bg in disabled mode.
- Add or update focused TestBackend assertions for the row type touched.
- Prefer existing `DashTheme`, `badge_span`, `label_span`, `metadata_span`, `path_span`, `id_span`, `selected_style`,
  and style assertion helpers over new visual primitives.
- Avoid literal colors outside the theme and tests.

## Phase 26A: Row View Infrastructure

### Objective

Replace the main-row rendering pipeline with a row view abstraction that can render multiple styled spans per row while
keeping selection and marked-state composition centralized.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Optional file:

- `crates/zorg-dash/src/model.rs`, only if a tiny `pub(crate)` helper is needed for presentation text that already
  exists privately.

### Work

1. Introduce a small render-only abstraction in `ui.rs`, such as:
   - `struct RowRender { spans: Vec<Span<'static>>, base_style: Style, marked: bool }`, or
   - `fn row_line(row, frame, theme) -> Line<'static>` plus helper functions for selected/marked composition.
2. Replace `row_list_line(row, frame) -> String` in `row_items` with a styled line builder.
3. Keep prefix rendering (`>`, `*`, and leading space) centralized so every later row type gets the same selection and
   marked behavior.
4. Add helper functions to apply selected style across every span in a row without losing meaningful row-specific
   foreground styles unless `theme.selection()` intentionally overrides them.
5. Add helper functions to apply marked-row background or marker styling across the row while retaining the `*` text
   marker.
6. Keep the initial row content text equivalent to the old `row_list_line` output in this phase. This phase is
   infrastructure, not row redesign.

### Tests

Add or update tests for:

- selected row spans receive selection styling across prefix and content
- marked diagnostic rows retain the `*` marker and receive marked styling in color mode
- no-color rendering still has reset fg/bg for every cell
- a long diagnostic row remains clipped to one terminal row

### Acceptance

- `row_items` consumes styled row lines/spans, not only `String`.
- There is one central place for selection and marked-row composition.
- Existing row text assertions still pass or are updated only for harmless spacing changes.
- `cargo test -p zorg-dash` passes.

## Phase 26B: Diagnostic Rows

### Objective

Make diagnostic rows scan as structured information instead of coloring the entire row by severity.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

### Work

1. Add a diagnostic row renderer that composes spans for:
   - severity badge using `BadgeTone::Severity(row.severity_kind())`
   - diagnostic code, or category fallback, in a strong severity/domain style
   - message as primary body text
   - path and position as muted/path metadata
2. Preserve the Today-panel special case that currently renders diagnostics more compactly than the Diagnostics panel.
3. Stop applying severity style to the entire diagnostic row in normal color mode.
4. Keep path, code, severity, and message text present in `buffer_to_string`.
5. Ensure fallback diagnostics with no path or no position render readable placeholder text rather than empty visual
   gaps.

### Tests

Add or update tests for:

- error/warning/info/unknown diagnostic severity badges use distinct semantic styles
- diagnostic message text is not severity-colored
- diagnostic code retains severity/domain emphasis
- Today combined mode shows diagnostic rows distinctly from todo rows
- no-color diagnostic rows remain text-readable and color-free

### Acceptance

- Diagnostic rows are no longer whole-line red/yellow/cyan/magenta.
- Style tests assert badge/code semantics instead of full-line severity.
- Existing diagnostic filters, marking, yank, fix preview, and inspector behavior are unchanged.
- `cargo test -p zorg-dash` passes.

## Phase 26C: Zettel, Todo, And Marked Rows

### Objective

Render zettel and todo rows with clear token hierarchy: todo marker, canonical ID, title/preview, query badges, and
source metadata.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

### Work

1. Add a zettel row renderer that composes spans for:
   - todo marker, when present, using `theme.todo_accent()`
   - canonical ID using `id_span`; store fallback remains plain/muted
   - title or preview as primary body text
   - query badges using existing badge helpers
   - file path and optional lifecycle metadata as muted/path metadata
2. Keep todo and non-todo zettels visually distinct without relying only on color. The existing `[ ]`, `[N]`, etc.
   marker text must remain visible.
3. Apply marked-row composition consistently if a marked row ever intersects this renderer, while retaining the `*`
   prefix contract.
4. Preserve enough spacing that existing `--once` panel assertions still find canonical IDs, titles, paths,
   tags/properties/badges where applicable.

### Tests

Add or update tests for:

- todo marker has todo accent in color mode
- canonical ID has ID/domain accent
- path metadata is muted/path-styled, not primary title style
- selected todo rows keep marker/ID/text legible after selection composition
- Today combined mode visually distinguishes todo rows from diagnostic rows

### Acceptance

- Zettel/todo rows are span-composed in main list rendering.
- Todo markers and canonical IDs have separate semantic styles.
- `--once` text still includes the same meaningful zettel row content.
- `cargo test -p zorg-dash` passes.

## Phase 26D: Query, Custom Panel, And Search Rows

### Objective

Render saved queries, invalid queries, custom dashboard panel rows, and search-backed rows with consistent semantic
tokens.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Optional file:

- `crates/zorg-dash/src/model.rs`, only if query source/output labels should be made `pub(crate)` instead of duplicated
  in `ui.rs`.

### Work

1. Add a query row renderer that composes spans for:
   - `ok` or warning/error badge
   - query ID using query/domain accent
   - title as primary text
   - row count preview as metadata/value
   - source path as path metadata
   - source/output kind where there is room and existing fields are available
2. Render invalid query rows with a warning badge and readable error/source details; do not color the whole row.
3. Confirm custom dashboard panel rows that are backed by query/search results reuse the zettel, diagnostic, or query
   row renderers through `PanelRow`.
4. Keep panel header lines such as `Query: ...`, dynamic dashboard panel metadata, and custom panel error rows intact.

### Tests

Add or update tests for:

- valid query row badge/style
- invalid query row warning badge, with title/source still readable
- row count preview styling
- search panel rows continue to render through the shared zettel renderer
- custom dashboard panel query error rows continue to render as diagnostic rows

### Acceptance

- Query rows are span-composed and no longer whole-line warning-colored when invalid.
- Search and custom panel rows use the same row helper patterns as built-in panels.
- Existing query selection/open behavior is unchanged.
- `cargo test -p zorg-dash` passes.

## Phase 26E: Index Rows And Final Integration

### Objective

Finish row coverage for index status rows and consolidate the Epic 26 row rendering contract across all panels.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Secondary files only if tests require expected-output updates:

- `crates/zorg-dash/src/lib.rs`

### Work

1. Add an index status row renderer that composes spans for:
   - label as muted/body text for normal rows
   - value as body/emphasis
   - attention labels/counts using `theme.index_row(row)` for non-zero diagnostics/deleted/new/changed files
2. Ensure normal counts remain quiet and attention counts remain visually distinct.
3. Remove or shrink obsolete `row_style` and `row_list_line` paths if all row variants now use span renderers.
4. Review row rendering across all built-in panels:
   - Today
   - Inbox
   - Queries
   - Search
   - Diagnostics
   - Index
   - dynamic custom panels
5. Run CLI one-shot checks and update brittle text assertions only where semantic content remains intact.

### Tests

Add or update tests for:

- normal index counts are not attention-colored
- non-zero diagnostics/deleted/new/changed counts use semantic attention style
- selected index rows compose selection with attention style legibly
- all built-in panels render at a narrow terminal size without hiding core row content
- `--once` output remains plain text with no ANSI escapes

### Acceptance

- Every `PanelRow` variant has a span-based main-row renderer.
- No row type relies on whole-line semantic coloring for meaning.
- `row_style` is removed or limited to compatibility code with no broad row coloring.
- `buffer_to_string` and `--once` output remain deterministic plain text.
- `cargo fmt --check`, `cargo test -p zorg-dash`, and the CLI validation commands pass.

## Cross-Phase Validation

Each phase should run:

```sh
cargo fmt --check
cargo test -p zorg-dash
```

Phases 26B through 26E should additionally run the relevant CLI checks after tests pass:

```sh
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel diagnostics
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel queries
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
```

Before running the dashboard commands against `fixtures/corpus`, reindex if the database does not exist or appears
stale:

```sh
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3
```

Final phase should also run:

```sh
cargo test -p zorg-cli --test smoke dash
```

## Risk Management

- `ui.rs` is the main conflict hotspot. Keep phases strictly sequential and scoped.
- Selection style can erase semantic foregrounds if applied incorrectly. Test selected diagnostics, selected todos,
  selected invalid queries, and selected index attention rows.
- Marked-row background can reduce contrast with selection or severity badges. Centralize composition and assert
  representative cells.
- `model.rs` private `list_line` helpers may still be useful to non-UI code. Prefer leaving them in place unless
  compiler dead-code warnings or clarity demands otherwise.
- Exact spacing in `--once` output may shift once rows become span-composed. Preserve content and update tests toward
  semantic substrings instead of column-perfect rows.
- Narrow terminals can clip rows aggressively. Preserve one row per item and rely on Ratatui clipping rather than adding
  wrapped row sublines in this epic.

## Handoff Notes For Agents

Each phase agent should start by reading:

- `sdd/legends/202605/zorg_dash_visual_refresh_1.md`, Epic 26 section
- this plan file
- `crates/zorg-dash/src/ui.rs`, especially `DashTheme`, span helpers, `row_items`, `row_list_line`, tests near row
  rendering
- `crates/zorg-dash/src/model.rs`, especially `PanelRow`, `ZettelRow`, `QueryRow`, `DiagnosticRow`, and `IndexStatusRow`

Each phase agent should finish with:

- a short summary of changed row behavior
- exact tests and CLI commands run
- any follow-up left for the next phase

---
create_time: 2026-05-03 22:21:34
status: wip
prompt: sdd/prompts/202605/epic17_zorg_dash_usability_foundation.md
---
# Epic 17 Zorg Dash Usability Foundation Plan

## Goal

Implement Epic 17 from `sdd/legends/202605/zorg_dash_next_improvements.md`: make the existing `zorg dash` panels usable
on real corpora before adding more dashboard surfaces.

The current baseline is healthy but still MVP-shaped:

- `cargo test -p zorg-dash` passes with 23 tests.
- Main rows are rendered as wrapped `Paragraph` text in `crates/zorg-dash/src/ui.rs`.
- `AppState` tracks only `selected_by_panel: [usize; Panel::ALL.len()]`.
- Selection preservation exists only for search zettel rows, and only by ad hoc zettel key.
- Status is a single string that replaces footer key help.
- Mouse capture defaults on, but mouse events are ignored by the event loop.
- `--no-color` does not exist, and `NO_COLOR` is not modeled.
- Degraded and empty states do not yet show concrete first-run guidance with the resolved root/database.

The implementation should preserve the dashboard's existing product stance: native Rust TUI, read-only launch for normal
browsing, explicit writes only through existing action paths, and deterministic `--once` frames for tests.

## Phase Count Decision

Use five phases, each intended for a distinct agent instance. The suggested legend split had four phases, but separating
status/log work from color/styling reduces merge risk and gives each agent a clean acceptance boundary. The final
mouse/first-run phase stays last because it depends on the render and CLI option surfaces stabilized by earlier phases.

## Cross-Phase Design

Introduce reusable primitives instead of one-off fixes:

- `PanelRowId`: stable identity for zettel, diagnostic, and index rows.
- `PanelViewport`: selected index, scroll offset, visible row count, total row count, and selected row identity.
- `DashboardViewState` or equivalent render input: current panel viewport plus status/log/color settings needed by
  `ui.rs`.
- `StatusEvent`: bounded ring event with severity, message, optional detail, and timestamp/order.
- `ColorMode`: automatic/default color, disabled by `NO_COLOR` or `--no-color`.

The event loop should keep model state in `AppState`; `ui.rs` should remain render-only. If the renderer needs terminal
dimensions for viewport clamping, compute those dimensions in `run_interactive`/`render_frame_to_string` before draw and
sync them into `AppState`, rather than having `ui.rs` mutate application state.

## Phase 17A: Row Identity, Viewport Model, And Navigation

### Scope

Add the model and app-state foundation while keeping the current paragraph renderer working.

### Technical Shape

- Add stable row identity methods in `model.rs`:
  - Zettel: canonical ID when present plus store row ID fallback.
  - Diagnostic: diagnostic DB ID, with code/path/span as fallback only if needed by tests or degraded fixtures.
  - Index status: label.
- Replace `selected_by_panel` with a per-panel viewport store in `app.rs`.
- Track selected row identity per panel and preserve selection after refresh, reindex, capture, and search when the same
  row still exists.
- Add viewport movement methods:
  - single row: up/down, `j`/`k`
  - first/last: `g`/`G`
  - page: PageUp/PageDown
  - half-page: choose explicit bindings and document them in help, for example Ctrl-U/Ctrl-D if they do not conflict
    with active text editing modes.
- Keep all navigation unavailable or no-op inside overlays and input-editing modes, consistent with existing behavior.
- Add tests for clamping, preserving selection by row identity across changed row order, zettel fallback identity,
  diagnostic identity, page movement, and half-page movement.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`

### Acceptance

- Existing tests still pass.
- Selection is preserved across snapshot replacement for zettel, diagnostic, and index rows when row identity remains
  present.
- Page and half-page keys update selected index and viewport offset predictably.
- Switching panels preserves each panel's viewport.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
```

## Phase 17B: List/Table Rendering, Counts, And Responsive Layout

### Scope

Replace the main `Paragraph` row rendering with explicit list/table rendering backed by viewport state.

### Technical Shape

- Change `ui::render_dashboard_with_state` to receive the active viewport or a small dashboard render state.
- Render main rows with `ratatui::widgets::List` or `Table` and explicit selected/offset state.
- Show compact position text such as `7/43` in the Main title, panel title, or a stable footer/status slot.
- Prefer one item per row with truncation; move full message/body details to the inspector.
- Keep search query and query errors visible without consuming unpredictable row height.
- Add overflow fixture/test data large enough to exceed the `--once` main-panel height.
- Add render tests for:
  - selected row below the visible bottom causes later rows to appear.
  - row count appears for non-empty panels.
  - long diagnostic/title rows do not wrap into adjacent rows.
  - narrow terminal layout still shows Panels, Main, Inspector, and footer/status areas without overlap.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/model.rs`
- dashboard render tests or test helpers

### Acceptance

- Moving selection beyond the visible bottom scrolls the visible list.
- `--once` frames for an overflow fixture show counts and stable one-row-per-item output.
- Narrow render tests pass without overlapping sections.
- Inspector still reflects the selected row after scrolling.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic17.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic17.sqlite3 --once --panel diagnostics
```

## Phase 17C: Color Mode And Severity/Status Styling

### Scope

Add intentional color behavior and severity styling, while honoring non-color environments.

### Technical Shape

- Add `--no-color` to `DashOptions::parse`, help text, README examples, and smoke coverage.
- Honor `NO_COLOR` by disabling color unless explicitly supported otherwise. For this epic, keep the policy simple:
  `NO_COLOR` and `--no-color` both disable foreground/background colors.
- Add a `ColorMode`/style palette passed into `ui.rs`; avoid direct environment lookups from render helpers.
- Style diagnostics by severity:
  - error, warning, info, unknown are visually distinct when color is enabled.
  - non-color mode can still use text labels and bold selection, but no color attributes.
- Style index health and status severity through the same palette.
- Add render tests that inspect `TestBackend` cell styles for color-enabled mode and assert absence of color in no-color
  mode.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`

### Acceptance

- `zorg dash --help` documents `--no-color`.
- `NO_COLOR=1 zorg dash --once ...` and `zorg dash --no-color --once ...` render without color attributes.
- Diagnostic severities are distinguishable when color is enabled.
- Existing `--once` text assertions remain stable.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 17D: Persistent Key Footer, Status Event Ring, And Log Overlay

### Scope

Replace the single status string with a bounded event log and make key help permanently visible.

### Technical Shape

- Add `StatusEvent` with severity, short message, optional detail, and monotonic display order or timestamp.
- Replace `AppState.status: String` with a bounded ring, for example 50 events.
- Keep a current/latest status summary available for compact rendering.
- Split footer rendering so key help always remains visible and latest status appears in a separate stable slot.
- Add a log overlay that shows recent events, opened by a key such as `L`; keep the existing one-off log overlay
  behavior compatible by appending details to the ring and opening the log/detail overlay.
- Record refresh, reindex, capture, search, open, and error outcomes as status events.
- Update help text and README key table.
- Add tests for ring truncation, event ordering, footer key visibility after status-producing actions, and log overlay
  rendering.

### Likely Files

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`

### Acceptance

- Key help remains visible after refresh, reindex, capture, search, open failures, and action errors.
- The log overlay shows recent events with enough detail to understand outcomes.
- Existing overlay close behavior still works with Esc/q/? as appropriate.
- Status-event tests cover bounded retention and severity.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 17E: Mouse Decision, First-Run/Empty Guidance, Docs, And Smoke Coverage

### Scope

Make terminal behavior intentional and improve first-run/empty-index guidance.

### Technical Shape

- Choose the conservative mouse path for Epic 17: default mouse capture off until useful mouse behavior exists.
- Add `--mouse` to opt in to mouse capture for experiments or future phases.
- Keep `--no-mouse` accepted for compatibility; it should explicitly disable capture even if defaults change later.
- Update terminal guard and panic cleanup paths to use the resolved mouse setting.
- Update help and README launch forms to document default mouse behavior.
- Add first-run/degraded guidance that includes:
  - resolved root path.
  - resolved database path.
  - an actionable reindex command.
  - a short note that the dashboard opens the index read-only.
- Add empty-index/empty-corpus guidance in the Index and Today/Inbox/Search empty states where appropriate.
- Add CLI smoke tests for `--mouse`, `--no-mouse`, `--no-color`, degraded index guidance, and empty/current index
  guidance.

### Likely Files

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`

### Acceptance

- Mouse capture behavior is intentional, documented, and defaults off.
- Degraded `--once --panel index` output shows resolved root/database and a reindex command.
- Empty/current index states tell users what is empty and what to do next.
- CLI smoke coverage protects the new flags and guidance text.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_missing_epic17.sqlite3 --once --panel index
```

## Final Epic 17 Validation

After all five phases land, run:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel diagnostics
NO_COLOR=1 cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel diagnostics
```

## Merge And Agent Coordination Notes

- Agents should work strictly in phase order. Later phases assume APIs introduced by earlier phases.
- Each phase should leave the repo formatted and tests green.
- Avoid broad refactors outside `zorg-dash` and the CLI smoke tests unless required by the phase acceptance criteria.
- Do not introduce new dashboard write behavior in Epic 17.
- Do not start Epic 18 diagnostics fixes until Epic 17's viewport, footer/status, color, and mouse decisions are
  complete.

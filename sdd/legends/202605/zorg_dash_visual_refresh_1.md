---
plan_name: zorg_dash_visual_refresh
created: 2026-05-04
status: done
legend_bead_id: zorg-7
tier: legend
source:
- sdd/research/202605/zorg_dash_visual_design_research.md
- sdd/research/202605/zorg_dash_dashboard_research.md
- sdd/research/202605/zorg_dash_next_improvements_research.md
- crates/zorg-dash/README.md
- crates/zorg-dash/src/lib.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/ui.rs
- crates/zorg-dash/src/app.rs
create_time: 2026-05-04 12:36:50
prompt: sdd/prompts/202605/zorg_dash_visual_refresh_1.md
---

# Zorg Dash Visual Refresh Plan

## Goal

Make the dashboard launched by `zorg dash` feel like a polished, calm, high-density operational tool instead of a
debug-style terminal frame. The work should preserve the existing product shape: native Ratatui TUI, read-only browsing
by default, deterministic `--once` output, explicit write confirmations, and accessibility through `NO_COLOR=1` and
`--no-color`.

The visual design research recommends a semantic theme system, clearer panel hierarchy, span-based row rendering,
structured inspector content, focused overlays, and stronger visual regression coverage. This plan splits that work into
five epics. Each epic is intentionally large enough to matter at product level, but still divisible into small phases
that can be implemented by distinct agent instances.

## Current Baseline

The renderer is concentrated in `crates/zorg-dash/src/ui.rs`.

- `StylePalette` currently exposes only emphasis, selection, severity, health, index-row, and status styles.
- Selection is bold plus `Color::DarkGray` background when color is enabled, with no explicit foreground.
- Most widgets use identical `Block::default().title(...).borders(Borders::ALL)` construction.
- Status, nav, main, inspector, footer, latest-status, and overlays have similar border weight.
- Main rows are still mostly monolithic strings through `row_list_line(row, frame) -> String`.
- Diagnostic rows often color the whole row by severity.
- Inspector content is plain strings except for the first line.
- Overlay renderers produce useful text, but use a generic bordered paragraph shell.
- Tests already inspect Ratatui buffer cells for severity color, no-color behavior, and index health colors.
- `render_frame_to_string` uses TestBackend plus `buffer_to_string`, so style metadata is intentionally not emitted in
  default `--once` text output.

These facts imply that the visual refresh should start with renderer primitives before touching every call site.

## Non-Goals

- Do not add a web dashboard, alternate renderer, or screenshot dependency for core validation.
- Do not add ANSI escapes to default `--once` text output.
- Do not change dashboard data semantics, write behavior, or panel membership unless a small presentation-only view
  model is required.
- Do not rely on color alone. Text labels, prefixes, markers, selection indicators, and counts must remain meaningful in
  no-color mode.
- Do not turn the dashboard into a marketing-style surface. The target is dense, quiet, scannable software.

## Cross-Epic Design Principles

Use semantic primitives, not scattered literal colors.

- Introduce a `DashTheme` or expanded palette with methods for surfaces, borders, titles, text, muted text, selection,
  marked rows, status/severity/health, domain accents, key hints, paths, links, and modal elevation.
- Keep `ColorMode::Disabled` strict. In disabled mode every style that reaches the buffer must have reset foreground and
  reset background. Modifiers such as bold can remain only if existing no-color tests and readability support them.
- Prefer Ratatui `Line` and `Span` composition over precolored full-line strings.
- Keep renderer helpers render-only. `app.rs` and `model.rs` should continue owning state and data decisions.
- Keep one row per list item unless an epic explicitly chooses a bounded detail subrow. Viewport behavior should remain
  predictable.
- Add focused TestBackend style assertions with helper functions, not brittle full-screen snapshots.
- Maintain narrow terminal behavior. Every epic should include at least one narrow render case when layout or row text
  is touched.

## Epic Count Decision

Use five epics:

1. Visual system foundation.
2. Frame hierarchy, header, footer, nav, and inspector.
3. Rich row rendering and badges.
4. Overlay polish.
5. Visual regression, documentation, and final compatibility pass.

This order reduces merge risk. The theme epic gives later agents stable APIs. The frame epic improves the shell without
rewriting row types. The row epic performs the highest-risk conversion separately. The overlay epic can then reuse the
same theme and badge helpers. The final epic validates consistency across `--once`, interactive rendering, no-color, and
docs.

## Epic 24: Visual System Foundation

### Objective

Create the semantic theme and rendering helper layer that all later dashboard polish will use.

### Product Outcome

No major visible redesign is required yet, but the codebase should have a clear visual language ready for the rest of
the refresh. Existing behavior should remain stable while style decisions move into named tokens.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/model.rs` only if small presentation enums are needed
- `crates/zorg-dash/README.md` only if color-mode behavior needs clarification

### Suggested Phases

#### Phase 24A: Theme Tokens

- Replace or expand `StylePalette` into `DashTheme`.
- Add semantic methods for:
  - app background, panel surface, elevated modal surface
  - subtle border, active border, warning border, error border
  - title, active title, text, muted text
  - selection foreground/background, marked row background
  - severity, health, status
  - todo, query, dashboard, graph/link, path, key hint accents
- Use `Color::Indexed` or `Color::Rgb` consistently. Pick a restrained neutral base with teal/cyan, amber, coral/red,
  green, and a small violet/blue accent.
- Keep `ColorMode::Disabled` returning reset fg/bg styles.

#### Phase 24B: Shared Block And Span Helpers

- Add helpers such as `panel_block(title, role, active, theme)`, `status_block`, `footer_block`, and `overlay_block`.
- Add span helpers for labels, values, muted metadata, paths, keys, and simple badges.
- Keep call sites conservative in this phase. Convert repeated block construction where the helper can be dropped in
  without changing layout.

#### Phase 24C: Style Test Helpers

- Add small test utilities for locating text cells and asserting style categories.
- Replace tests that assume raw named colors only where necessary. Prefer testing theme API output or rendered semantic
  behavior.
- Strengthen no-color tests so future additions cannot accidentally leak foreground/background colors.

### Acceptance

- Existing dashboard tests pass.
- No-color mode still leaves all rendered cells with `Color::Reset` foreground and background.
- Theme names cover every visual state called out in the research.
- Literal colors are centralized in the theme, with no new scattered `Color::Red`, `Color::Yellow`, etc. in renderer
  call sites except tests or theme definitions.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
```

## Epic 25: Frame Hierarchy, Header, Footer, Nav, And Inspector

### Objective

Make the stable dashboard shell easier to scan before rewriting row rendering.

### Product Outcome

The first screen should immediately communicate: app identity and health at top, available panels on the left or top
stack, active work area in the main panel, structured details in the inspector, and persistent keys/status at the
bottom. Borders should stop competing equally.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs` tests if one-shot golden text needs minor updates
- `crates/zorg-dash/README.md` if visible key/status layout changes need documentation

### Suggested Phases

#### Phase 25A: Panel Block Roles

- Apply subtle inactive borders and stronger active main border/title.
- Keep nav, inspector, keys, latest, and status visually lower than the main work area.
- Avoid changing area sizes unless needed for readability.
- Add tests that active and inactive blocks render with different semantic styles when color is enabled.

#### Phase 25B: Status Cockpit

- Convert the top status line into styled spans with compact badge-like groups:
  - product/title
  - index health
  - diagnostics
  - freshness
  - marked count
  - active panel
  - rows
  - pending operation, when present
- Move always-visible root/db paths out of the top line in normal ready states. Keep them visible in loading/degraded
  guidance and index/inspector context.
- Define narrow degradation rules: drop root/db first, then detailed row counts, while preserving health, diagnostics,
  and pending activity.
- Keep text output stable enough that `--once` tests can assert labels without relying on exact spacing.

#### Phase 25C: Nav And Footer Polish

- Style active nav row consistently with selection, and inactive rows with muted text.
- Split key hints into styled spans: key token distinct from action label.
- Keep the latest status area readable with status severity style and muted empty state.
- Add narrow tests that keys and latest status remain visible.

#### Phase 25D: Inspector Structure

- Convert inspector rendering from plain `Vec<String>` display to styled `Line` conversion.
- Detect and style known section headings such as `Graph context`, `Outgoing links`, `Incoming backlinks`, `Properties`,
  `Preview`, `Telemetry`, `Today queries`, and `Rows`.
- Style labels and paths muted, IDs and links accented, warnings/errors semantically colored.
- Keep `model.rs` inspector line producers unchanged unless a tiny metadata wrapper is clearly better.

### Acceptance

- The dashboard has visibly distinct active, inactive, footer, and modal surfaces in color mode.
- Top status is shorter and less path-heavy in normal ready frames.
- Inspector sections are visually structured without losing text content in `--once`.
- Narrow terminal render tests continue to show `Zorg Dash`, `Panels`, `Main`, `Inspector`, and `Keys`.
- No-color mode remains readable and color-free.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
```

## Epic 26: Rich Row Rendering And Badges

### Objective

Replace monolithic row strings with span-based row view rendering for diagnostics, zettels, query rows, custom dashboard
rows, and index status rows.

### Product Outcome

Rows should be easier to scan at speed. Important tokens such as todo status, canonical IDs, diagnostic severity, query
validity, paths, positions, and counts should have distinct visual weight without coloring whole lines aggressively.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/model.rs` only if row view helpers belong closer to row structs
- `crates/zorg-dash/src/lib.rs` one-shot text tests if exact row text changes

### Suggested Phases

#### Phase 26A: Row View Abstraction

- Replace `row_list_line(row, frame) -> String` with a `Line<'static>` or small row view model.
- Preserve one list item per logical row.
- Add helpers to apply selection and marked-row styles across spans without erasing meaningful foreground colors unless
  the selected state requires it for readability.
- Keep `buffer_to_string` output text close enough to current expectations.

#### Phase 26B: Diagnostic Rows

- Render severity as a compact styled text badge.
- Render code strongly, message as primary text, and path/position as muted metadata.
- Avoid coloring the entire diagnostic row red/yellow/cyan.
- Update style tests to assert badge/code colors rather than full-line color.

#### Phase 26C: Zettel And Todo Rows

- Style todo marker separately from ID and title.
- Give canonical IDs a stable accent, title/body preview primary text, tags/properties/path muted metadata.
- Marked rows should show a subtle marked background or marker in color mode while retaining the `*` text marker.
- Today combined mode should make todo rows and diagnostic rows visually distinct without relying only on labels.

#### Phase 26D: Query, Custom Panel, And Index Rows

- Style invalid query rows with warning badge plus readable title/source details.
- Style saved query IDs, query source labels, row counts, and dashboard/custom panel metadata with semantic accents.
- For index rows, keep normal counts quiet and attention counts colored by health or severity.
- Ensure Search, Queries, custom panels, Diagnostics, Today, Inbox, and Index all use the same row helper patterns.

### Acceptance

- Main row renderer accepts styled lines/spans, not only strings.
- Diagnostic rows are no longer whole-line severity color in normal color mode.
- Selection, marked state, and row-specific styles compose predictably.
- `--once` text output still includes the same meaningful row content.
- Tests cover selected rows, marked rows, diagnostics, invalid queries, and index attention counts.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel diagnostics
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel queries
```

## Epic 27: Overlay Polish

### Objective

Make modal overlays feel focused, elevated, and consistent with the refreshed dashboard shell.

### Product Outcome

Help, SWOG help, capture, capture template picker, diagnostic filters, todo prompts, yank, fix preview, confirmations,
event log, and generic logs should read as intentional modal surfaces instead of generic bordered text boxes.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/app.rs` only if overlay state needs small presentation metadata
- `crates/zorg-dash/README.md` for help/key copy only if visible behavior changes

### Suggested Phases

#### Phase 27A: Overlay Shell

- Apply an elevated surface style, stronger title/border, and consistent inner padding where available.
- Use warning/error border variants for destructive or failed confirmations.
- Keep `Clear` behavior, but ensure the filled overlay surface is styled in color mode.
- Add tests for overlay border/title styles and no-color behavior.

#### Phase 27B: Form Overlays

- Refresh capture, diagnostic filter, todo prompt, and template picker lines with shared form-row helpers.
- Selected form field should use the same selection semantics as main/nav.
- Labels should be muted or structured; current values should be primary; errors should use semantic error styling.
- Preserve all existing keyboard behavior and text content.

#### Phase 27C: Action And Confirmation Overlays

- Style fix preview safe/unsafe/preferred states as badges.
- Style replacement preview labels, paths, diagnostics, warnings, and unavailable reasons.
- Style confirm-fix and confirm-todo overlays so the operation, target, planned changes, and confirmation instruction
  are visually distinct.
- Preserve strong textual confirmation prompts for no-color mode.

#### Phase 27D: Help, SWOG Help, Yank, And Logs

- Group key help and SWOG examples into styled lines with key/source tokens distinct from descriptions.
- Style yank available values differently from unavailable choices.
- Style event log order/severity/detail lines with clear hierarchy.
- Keep overlays usable at 56x22, matching current narrow overlay coverage.

### Acceptance

- Every `DashboardOverlay` variant uses the shared overlay shell.
- Selection styling is consistent across list rows and overlay form rows.
- Warning/error/safe/unsafe states remain text-visible in no-color mode.
- Existing overlay behavior tests still pass, with added style assertions for representative overlays.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Epic 28: Visual Regression, Documentation, And Compatibility Pass

### Objective

Harden the refreshed design with targeted regression coverage, documentation, and an end-to-end compatibility review.

### Product Outcome

The visual refresh should be shippable, documented, and protected against accidental regression. Future agents should
have clear theme conventions and tests to extend.

### Likely Files

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`
- `sdd/research/202605/zorg_dash_visual_design_research.md` only if adding implementation notes is desired
- Optional new SDD epic/prompt files after this plan is accepted

### Suggested Phases

#### Phase 28A: Test Matrix Consolidation

- Add or consolidate TestBackend tests for:
  - color-enabled semantic styles
  - no-color all-cell reset behavior
  - active/inactive panel hierarchy
  - status cockpit with and without pending activity
  - selected and marked rows
  - diagnostic badge styling
  - overlay shell and selected form field
  - narrow terminal layout
- Prefer semantic helper assertions over exact color literals at call sites.

#### Phase 28B: One-Shot And Smoke Compatibility

- Verify `--once` outputs remain plain text without ANSI escape sequences.
- Exercise all built-in panels against `fixtures/corpus`.
- Verify `--json` is unchanged unless prior epics intentionally touched presentation-only metadata.
- Verify `NO_COLOR=1` and `--no-color` behavior through direct render tests and CLI smoke where practical.

#### Phase 28C: Documentation And Contributor Notes

- Update `crates/zorg-dash/README.md` with the refreshed key visual behavior only where useful.
- Add a short renderer/theme note near `DashTheme` explaining semantic token conventions and no-color requirements.
- Document how to add a new row style or overlay style without bypassing the theme.

#### Phase 28D: Final Design Pass

- Review actual interactive rendering with common terminal sizes:
  - 140x40
  - 120x30
  - 100x28
  - 80x24
  - 56x22
- Check that the palette is not one-note, text does not overlap, selected text is legible, and diagnostics are not
  visually overwhelming.
- File follow-up SDD items for remaining nonblocking polish rather than expanding this epic indefinitely.

### Acceptance

- `cargo test -p zorg-dash` and relevant CLI smoke tests pass.
- `--once` remains deterministic plain text.
- No-color mode is covered and passes across main frame plus at least one overlay.
- README and renderer comments explain the theme contract.
- The dashboard visual refresh is complete enough that later feature work can reuse the theme rather than reinventing
  styling.

### Validation

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel diagnostics
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
```

## Dependency Order

Implement epics in numeric order.

- Epic 24 must land first because it defines the theme API.
- Epic 25 depends on the block helpers and can proceed before row conversion.
- Epic 26 depends on the theme and should start after the stable frame is settled.
- Epic 27 depends on the theme and selection helpers, and can overlap late Epic 26 only if agents coordinate write
  ownership in `ui.rs`.
- Epic 28 should land last because it consolidates tests, docs, and final visual QA.

## Risk Management

- `ui.rs` will be the main conflict hotspot. Later prompt files should assign narrow ownership by renderer area.
- Existing text golden assertions may need updates after status or row formatting changes. Keep assertions semantic
  rather than spacing-specific.
- Selection styling can easily erase row-specific colors. Define composition rules in Epic 24 and test them in Epic 26.
- No-color regressions are likely when adding surfaces/backgrounds. Keep all-cell reset tests strict.
- Truecolor choices may render differently across terminals. Prefer a compact theme with good contrast and avoid relying
  on subtle hue differences for meaning.

## Recommended Next Step After Plan Acceptance

Create five SDD epic files and matching prompts under `sdd/epics/202605/` and `sdd/prompts/202605/`, likely numbered
Epic 24 through Epic 28 unless the project owner prefers a different numbering scheme. Each epic prompt should include
the research file, this plan, and a narrow phase assignment so a distinct agent can implement it without rediscovering
the full design context.

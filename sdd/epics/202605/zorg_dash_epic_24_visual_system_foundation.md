---
create_time: 2026-05-04 12:57:43
bead_id: zorg-7.1
tier: epic
legend_bead_id: zorg-7
status: wip
prompt: sdd/prompts/202605/zorg_dash_epic_24_visual_system_foundation.md
---
# Zorg Dash Epic 24 Visual System Foundation Plan

## Context

Epic 24 in `sdd/legends/202605/zorg_dash_visual_refresh_1.md` is the foundation epic for the broader Zorg Dash visual
refresh. The current renderer is concentrated in `crates/zorg-dash/src/ui.rs`, with a small `StylePalette` that covers
only emphasis, selection, severity, health, index-row attention, and status styles. Repeated widget construction uses
raw `Block::default().title(...).borders(Borders::ALL)`, and tests assert some literal Ratatui colors directly.

This epic should not redesign the dashboard shell, row rendering, or overlays yet. Its job is to create a stable
semantic visual API, migrate the lowest-risk call sites onto it, and add guardrails so later epics can style panels,
rows, inspectors, and overlays without spreading literal color decisions through the renderer.

Because each phase will be completed by a distinct agent instance, the phases below are scoped to reduce merge risk and
make ownership clear. All phases should preserve default `--once` plain-text output and strict no-color behavior.

## Goals

- Replace the narrow `StylePalette` concept with a semantic `DashTheme` foundation.
- Centralize visual tokens for surfaces, borders, text hierarchy, selections, marked rows, severity, health, status,
  domain accents, key hints, paths, links, and modal elevation.
- Add shared block and span helpers without forcing later visual redesign decisions into this epic.
- Add test helpers that let future agents assert semantic rendering behavior without brittle full-screen snapshots.
- Keep all rendered foreground and background colors reset in `ColorMode::Disabled`.

## Non-Goals

- Do not convert main list rows from strings to rich spans; that belongs to Epic 26.
- Do not restructure the top status cockpit, nav hierarchy, footer, or inspector; that belongs to Epic 25.
- Do not restyle each overlay in detail; that belongs to Epic 27.
- Do not add ANSI escapes to `--once` output.
- Do not change dashboard data semantics, write behavior, keyboard behavior, or layout sizes except where a helper
  requires a mechanically equivalent call-site update.

## Phase 24.1: Theme Core And Compatibility Shim

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Do not touch `model.rs`, `app.rs`, or README in this phase unless compilation requires a tiny import adjustment.

### Work

1. Replace `StylePalette` with `DashTheme`, or introduce `DashTheme` and temporarily keep `StylePalette` as a small
   compatibility wrapper only if that lowers risk.
2. Add semantic style methods for:
   - background/surfaces: app background, panel surface, elevated overlay surface
   - borders: subtle, active, warning, error
   - text: title, active title, body text, muted text
   - state: selection, marked row
   - existing behavior: severity, health, index attention, status
   - accents: todo, query, dashboard/custom panel, graph/link, path, key hint
3. Preserve existing behavior at current call sites as much as practical. Existing severity, health, index, status, and
   selection callers should compile against the new theme API with minimal visible changes.
4. Make the no-color contract explicit in code: any style method that can set `fg` or `bg` in color mode must return a
   reset-foreground/reset-background style in disabled mode. Modifiers such as bold may remain where already used.
5. Centralize literal color choices in the theme implementation. New renderer call sites should not introduce raw
   `Color::Red`, `Color::Yellow`, etc. outside tests and the theme itself.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Existing no-color tests still prove every rendered cell has `Color::Reset` foreground and background.
- Existing tests for diagnostic severity and index attention colors are updated only as needed for the new theme names.

### Notes For The Implementing Agent

Keep this phase intentionally conservative. The result may look almost identical to today; the product value is that
later phases can call named tokens instead of deciding colors locally.

## Phase 24.2: Shared Block Helpers

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Limit edits to block construction helpers and low-risk call sites that already render blocks.

### Work

1. Add a compact block-role abstraction for current dashboard surfaces. A small enum such as `PanelRole` or `BlockRole`
   is acceptable if it stays render-only.
2. Add helpers for repeated shell construction:
   - `panel_block(title, role, theme)`
   - `status_block(theme)`
   - `footer_block(title, theme)` or equivalent
   - `overlay_block(title, role, theme)`
3. Convert mechanically equivalent `Block::default().title(...).borders(Borders::ALL)` call sites where no layout or
   content behavior changes are required.
4. Do not make active/inactive panel hierarchy decisions in this phase beyond what the helper API must represent. Epic
   25 should apply the hierarchy.
5. Ensure helpers do not leak colors in disabled mode.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- A representative rendered dashboard still contains `Zorg Dash`, `Panels`, `Main`, `Inspector`, `Keys`, and `Latest`.
- Block helper APIs are expressive enough for later epics to request subtle, active, footer, status, and elevated modal
  shells without constructing raw blocks everywhere.

### Notes For The Implementing Agent

This phase will touch many nearby lines in `ui.rs`, so keep each change mechanical. Avoid opportunistic row, status, or
overlay text rewrites.

## Phase 24.3: Span And Badge Helpers

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Avoid changing `row_list_line` behavior except where existing styled spans already exist, such as status/header/form
  lines.

### Work

1. Add small span helpers for common visual tokens:
   - label/value pairs
   - muted metadata
   - paths
   - IDs and links
   - key hints
   - simple text badges for severity/status/domain labels
2. Convert low-risk existing span constructions to use the helpers where the visible text remains unchanged:
   - status-line labels and values
   - footer key help only if it can remain one line and plain-text-compatible
   - existing overlay/form lines that already compose spans, without redesigning overlay content
3. Define selection composition guidance for future row conversion. The helper should make it clear whether selection
   patches foreground/background onto existing spans or fully replaces them.
4. Keep `buffer_to_string` output stable: helper conversion must not add decorative glyphs or ANSI-oriented text.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- `cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today`
  still emits plain text with no ANSI escape sequences.
- The helper names cover the token categories needed by Epics 25-27.

### Notes For The Implementing Agent

Badges in this phase are helpers, not a broad product redesign. Use text that is already present unless a local caller
already has a compact label ready.

## Phase 24.4: Semantic Style Test Utilities

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs` test module
- Do not rewrite unrelated tests.

### Work

1. Add test utility functions for:
   - locating the first cell for a text fragment
   - asserting a text fragment has a specific semantic style from `DashTheme`
   - asserting a rendered buffer has no foreground or background colors in disabled mode
2. Refactor existing style tests to use these helpers where that makes intent clearer:
   - diagnostic severity colors
   - index health and attention counts
   - no-color all-cell reset behavior
3. Add at least one direct theme-level test, or rendered semantic assertion, that proves disabled-mode semantic styles
   reset both foreground and background for newly introduced tokens such as border, surface, selection, path, key hint,
   and modal/elevated styles.
4. Avoid brittle full-screen snapshots. Keep assertions tied to meaningful fragments or all-cell invariants.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Future test failures identify semantic style regressions by helper name or token purpose rather than only by raw
  literal colors.
- No-color coverage includes the newly introduced semantic tokens, not just pre-existing severity/status styles.

### Notes For The Implementing Agent

This phase should be done after the helper APIs settle. It is the right place to rename older helper functions such as
`cell_for_text` if that improves test readability, but keep the scope local to the test module.

## Phase 24.5: Foundation Integration And Documentation Note

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Secondary file: `crates/zorg-dash/README.md` only if a short developer-facing note is clearly useful.

### Work

1. Review `crates/zorg-dash/src/ui.rs` for remaining literal color use. Literal colors should be limited to:
   - `DashTheme` token definitions
   - tests asserting specific theme outcomes
   - unavoidable Ratatui reset/default values
2. Add a concise code comment near `DashTheme` explaining:
   - styles must be semantic tokens
   - disabled color mode must reset foreground/background
   - later renderer code should use block/span helpers rather than raw color literals
3. Run the full Epic 24 validation set.
4. If README is touched, keep it brief and user-relevant. Do not document internal token names unless the README already
   has a developer section appropriate for that.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- This command succeeds and emits deterministic plain text:

  ```sh
  cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
  ```

- This command succeeds with no color assumptions in output:

  ```sh
  cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
  ```

- Epic 24 leaves the codebase ready for Epic 25 to apply panel hierarchy without first inventing theme primitives.

## Suggested Agent Order

Run the phases strictly in order:

1. Phase 24.1 defines the API.
2. Phase 24.2 depends on the block-related API.
3. Phase 24.3 depends on the span/token API.
4. Phase 24.4 depends on settled helper names.
5. Phase 24.5 is a final cleanup and validation pass.

Do not run these phases in parallel. They share `crates/zorg-dash/src/ui.rs` and intentionally build on the previous
phase's names and contracts.

## Cross-Phase Guardrails

- Preserve `--once` as plain deterministic text. `buffer_to_string` must continue to ignore style metadata.
- Preserve `NO_COLOR=1` and `--no-color` behavior. Disabled mode must not set foreground or background colors anywhere
  in the rendered buffer.
- Prefer semantic helper assertions over exact spacing or full-screen output snapshots.
- Keep row conversion, inspector structuring, status cockpit redesign, and overlay polish out of this epic even if the
  new helpers make those tasks tempting.
- Avoid unrelated refactors in `model.rs`, `app.rs`, and CLI code. Epic 24 should remain a renderer foundation change.

## Overall Validation

Each phase should run:

```sh
cargo fmt --check
cargo test -p zorg-dash
```

The final phase should additionally run:

```sh
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
```

---
plan_name: zorg_dash_epic_28_visual_regression_compatibility
bead_id: zorg-7.5
tier: epic
legend_bead_id: zorg-7
created: 2026-05-04
source:
- sdd/legends/202605/zorg_dash_visual_refresh_1.md
- sdd/epics/202605/zorg_dash_epic_24_visual_system_foundation.md
- sdd/epics/202605/zorg_dash_epic_25_frame_hierarchy.md
- sdd/epics/202605/zorg_dash_epic_26_rich_rows.md
- sdd/epics/202605/zorg_dash_epic_27_overlay_polish.md
- crates/zorg-dash/src/ui.rs
- crates/zorg-dash/src/lib.rs
- crates/zorg-cli/tests/smoke.rs
- crates/zorg-dash/README.md
status: done
create_time: 2026-05-04 15:31:49
prompt: sdd/prompts/202605/zorg_dash_epic_28_visual_regression_compatibility.md
---

# Zorg Dash Epic 28 Visual Regression, Documentation, And Compatibility Plan

## Context

Epic 28 is the closing hardening pass for the Zorg Dash visual refresh. In the current tree, the earlier visual-refresh
work is already largely present:

- `crates/zorg-dash/src/ui.rs` contains `DashTheme`, semantic block roles, span helpers, badge helpers, row rendering
  through `RowRender`, structured inspector rendering, overlay shell handling, and many semantic style assertions.
- `crates/zorg-dash/src/lib.rs` already has overflow-corpus render coverage for every built-in panel, golden-region text
  assertions, one-shot rendering helpers, and JSON tests.
- `crates/zorg-cli/tests/smoke.rs` already exercises several `zorg dash --once` paths, JSON export, custom dashboards,
  no-color launch paths, and read-only behavior.
- `crates/zorg-dash/README.md` already documents the shorter ready status bar and basic color/no-color behavior.

This means Epic 28 should not reopen the visual design. It should consolidate coverage, fill compatibility gaps,
document the renderer contract, and do a final small QA pass. The work should be split into sequential phases because
all phases touch nearby dashboard files and each phase will be run by a distinct agent instance.

## Goals

- Make the refreshed visual system regression-resistant with focused TestBackend style assertions.
- Preserve deterministic, plain-text `--once` output with no ANSI escapes.
- Preserve `--once --json` output shape unless an earlier epic intentionally changed presentation-only metadata.
- Verify every built-in panel still renders against fixture/overflow corpora in standard and narrow sizes.
- Verify `NO_COLOR=1` and `--no-color` behavior at both renderer and CLI levels.
- Document the theme/no-color contract so future dashboard work extends `DashTheme` instead of adding ad hoc styles.
- Complete a final visual QA pass without expanding scope into new dashboard features.

## Non-Goals

- Do not introduce a screenshot framework, web renderer, new terminal backend, or ANSI styling in `--once` output.
- Do not redesign the palette, layout, panel membership, row semantics, keyboard behavior, write confirmations, or JSON
  schema.
- Do not add brittle full-screen golden snapshots where semantic cell assertions or region assertions are sufficient.
- Do not make broad refactors in `app.rs`, `model.rs`, or data-loading code unless a test exposes an actual regression.
- Do not add visual polish follow-ups directly into this epic unless they are small, blocking compatibility defects.

## Cross-Phase Guardrails

Every phase should:

- Prefer the existing `DashTheme`, block helpers, span helpers, `selected_style`, row helpers, overlay helpers, and
  style test helpers in `ui.rs`.
- Keep literal colors centralized in `DashTheme`; tests may compare semantic styles, but renderer call sites should not
  gain new raw color literals.
- Keep `ColorMode::Disabled` strict: every rendered cell must have `Color::Reset` foreground and background.
- Keep `buffer_to_string` and `render_frame_to_string` text meaningful and stable, but avoid exact spacing assertions
  except where spacing is the actual behavior under test.
- Keep each phase's tests narrowly tied to the area it changes so later agents can diagnose failures quickly.
- Run `cargo fmt --check` and `cargo test -p zorg-dash` before handing off, unless explicitly blocked.

## Phase 28.1: Regression Matrix Audit And Test Helpers

### Objective

Inventory the existing visual-regression coverage and add small reusable test helpers only where the matrix has clear
gaps.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Secondary file only if needed:

- `crates/zorg-dash/src/lib.rs`

Avoid README and CLI smoke tests in this phase.

### Work

1. Map the current style/render tests in `ui.rs` against the Epic 28 matrix:
   - color-enabled semantic styles
   - no-color all-cell reset behavior
   - active/inactive panel hierarchy
   - status cockpit with and without pending activity
   - selected and marked rows
   - diagnostic badge styling
   - overlay shell and selected form field
   - narrow terminal layout
2. Extend or normalize existing helper functions if there are duplicated local patterns, such as:
   - finding text cells
   - checking semantic styles
   - asserting all cells are no-color reset
   - rendering frames at standard/narrow sizes
3. Add only the lowest-risk missing renderer tests discovered by the audit. Likely candidates:
   - pending-activity status group style/visibility at standard width
   - no-color frame including both main dashboard and one overlay in the same render
   - narrow render preserving the key structural labels after visual-refresh helpers are applied
4. Prefer adapting existing tests over adding a new parallel fixture setup.

### Acceptance

- The test helper layer is clearer, not larger for its own sake.
- The visual-regression matrix is represented by focused tests or by comments in the plan handoff explaining why a
  category is already covered.
- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.

## Phase 28.2: One-Shot Plain Text And JSON Compatibility

### Objective

Harden one-shot output guarantees after the visual refresh.

### Scope

Primary files:

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-cli/tests/smoke.rs`

Secondary file only if a renderer defect is found:

- `crates/zorg-dash/src/ui.rs`

### Work

1. Add or strengthen tests proving `render_frame_to_string` and CLI `zorg dash --once` output contain no ANSI escape
   sequences in both enabled-color and disabled-color modes.
2. Exercise all built-in panels against fixture or overflow corpora and assert semantic regions rather than exact full
   frames.
3. Verify `--once --json` remains parseable and presentation-neutral:
   - active panel key
   - rows
   - selected dashboard/custom panel metadata where applicable
   - degraded index state
4. Add a CLI smoke assertion for `NO_COLOR=1` or `--no-color` that also checks there are no escape bytes, not just that
   text renders.
5. Keep tests portable: use existing temp workspace helpers, existing fixture corpus patterns, and avoid terminal-size
   assumptions outside `TestBackend` render helpers.

### Acceptance

- One-shot text output is explicitly protected from ANSI escape regressions.
- JSON compatibility remains covered and unchanged by renderer styling.
- All built-in panels have one-shot text coverage through existing or strengthened tests.
- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- `cargo test -p zorg-cli --test smoke dash` passes.

## Phase 28.3: No-Color And Narrow-Layout Closure

### Objective

Close the two highest-risk compatibility classes for terminal visual work: color leakage and cramped layouts.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Secondary files only if needed:

- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-cli/tests/smoke.rs`

### Work

1. Ensure no-color assertions cover:
   - full dashboard frame
   - selected/marked rows
   - at least one form overlay
   - at least one warning/error/destructive overlay
   - status/footer/chrome cells, not only content cells
2. Ensure narrow render assertions cover the standard final QA widths where practical through `TestBackend`:
   - 80x24
   - 64x28 or similar existing narrow split
   - 56x22 for overlays
3. Check that narrow output still includes structural labels and key operational text:
   - `Zorg Dash`
   - `Panels`
   - `Main`
   - `Inspector`
   - `Keys`
   - active panel label
   - core overlay title/instruction for representative overlays
4. If a narrow test exposes overlapping or missing critical text, make the smallest renderer adjustment possible. Prefer
   shortening labels or applying existing degradation rules over changing layout areas.

### Acceptance

- No-color tests fail if any foreground/background color leaks anywhere in representative full-frame and overlay
  renders.
- Narrow tests cover both dashboard frame and overlays without relying on fragile full-screen snapshots.
- Any renderer adjustments are small and behavior-preserving.
- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.

## Phase 28.4: Documentation And Theme Contract Notes

### Objective

Make the completed visual system understandable for future dashboard contributors.

### Scope

Primary files:

- `crates/zorg-dash/README.md`
- `crates/zorg-dash/src/ui.rs`

Optional file:

- `sdd/research/202605/zorg_dash_visual_design_research.md`

Avoid new behavior or large test changes in this phase.

### Work

1. Update `crates/zorg-dash/README.md` with a short "Visual Rendering Contract" or equivalent section covering:
   - semantic theme tokens
   - color-enabled versus no-color behavior
   - one-shot plain-text behavior
   - how rows, inspector lines, and overlays should be styled
   - where to add tests for a new row or overlay style
2. Add or refine a concise comment near `DashTheme` explaining:
   - all renderer colors should flow through semantic tokens
   - disabled mode must reset foreground and background
   - selection/marked styles compose with row-specific semantic styles
3. If useful, add a short implementation note to the visual design research file saying the contract landed in
   `README.md` and `ui.rs`; do not duplicate the README.
4. Keep docs factual and contributor-oriented. Do not describe unreleased features as implemented.

### Acceptance

- README explains the visual-rendering contract without bloating user-facing usage docs.
- `DashTheme` comments are concise and actionable.
- Documentation matches the current implementation and tests.
- `cargo fmt --check` passes if Rust comments changed.
- `cargo test -p zorg-dash` passes unless only Markdown changed and the agent clearly states tests were not needed.

## Phase 28.5: Final Design QA And Follow-Up Capture

### Objective

Run the final compatibility/design pass, fix only blocking regressions, and capture nonblocking leftovers for later
work.

### Scope

Primary files as needed:

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `crates/zorg-dash/README.md`

Optional SDD file if follow-ups are found:

- a small follow-up note under `sdd/tales/202605/` or another repo-standard follow-up location

### Work

1. Run the full Epic 28 validation set:
   - `cargo fmt --check`
   - `cargo test -p zorg-dash`
   - `cargo test -p zorg-cli --test smoke dash`
   - `cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3`
   - `cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today`
   - `cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel diagnostics`
   - `cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color`
2. Manually inspect representative rendered text for the target sizes called out in the legend where possible:
   - 140x40
   - 120x30
   - 100x28
   - 80x24
   - 56x22
3. Check final design constraints:
   - palette is not one-note
   - selected text is legible
   - diagnostics are not visually overwhelming
   - key labels remain visible
   - text does not overlap or disappear in narrow layouts
4. Fix only defects that block shipping Epic 28. Examples:
   - no-color leakage
   - ANSI escape leakage
   - broken CLI smoke compatibility
   - missing critical text in narrow dashboard/overlay renders
   - stale docs that contradict implementation
5. Record nonblocking polish as follow-up SDD items instead of extending this epic.

### Acceptance

- Full validation commands pass, or blockers are documented with exact failing commands and likely causes.
- The final pass does not introduce a new redesign or new feature surface.
- Any follow-up items are explicitly nonblocking and scoped for later work.
- The dashboard visual refresh is shippable from a regression, documentation, and compatibility standpoint.

## Recommended Agent Sequencing

Run the phases sequentially, not in parallel:

1. Phase 28.1 establishes the exact regression matrix and helper baseline.
2. Phase 28.2 protects script-facing `--once` and JSON compatibility.
3. Phase 28.3 closes terminal compatibility gaps around no-color and narrow layouts.
4. Phase 28.4 documents the finished contract after tests reflect reality.
5. Phase 28.5 performs final validation and captures nonblocking follow-ups.

This sequencing minimizes conflicts in `ui.rs`, avoids writing documentation before compatibility tests settle, and
keeps the last agent focused on verification and small blocking fixes.

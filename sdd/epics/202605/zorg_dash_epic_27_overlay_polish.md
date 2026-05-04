---
plan_name: zorg_dash_epic_27_overlay_polish
bead_id: zorg-7.4
tier: epic
legend_bead_id: zorg-7
created: 2026-05-04
source:
- sdd/legends/202605/zorg_dash_visual_refresh_1.md
- sdd/epics/202605/zorg_dash_epic_24_visual_system_foundation.md
- sdd/epics/202605/zorg_dash_epic_25_frame_hierarchy.md
- sdd/epics/202605/zorg_dash_epic_26_rich_rows.md
- crates/zorg-dash/src/ui.rs
- crates/zorg-dash/src/model.rs
- crates/zorg-dash/src/app.rs
create_time: 2026-05-04 14:46:51
status: wip
prompt: sdd/prompts/202605/zorg_dash_epic_27_overlay_polish.md
---

# Zorg Dash Epic 27 Overlay Polish Plan

## Context

Epic 27 in `sdd/legends/202605/zorg_dash_visual_refresh_1.md` is the modal overlay polish epic for the broader Zorg Dash
visual refresh. Epics 24, 25, and 26 are already present in this working tree:

- `crates/zorg-dash/src/ui.rs` defines `DashTheme`, semantic style tokens, `BlockRole`, shell block helpers, span
  helpers, badge helpers, and semantic style test helpers.
- The dashboard shell already has active/subtle/status/footer hierarchy.
- Main-panel rows already render through span-based row helpers.
- Strict no-color coverage exists and asserts that all rendered foreground/background colors reset in disabled mode.
- Overlay rendering is centralized in `render_overlay`, but overlay content quality is uneven. Some overlays already use
  semantic spans, while help text, confirmations, generic logs, and many inactive form rows are still plain strings.

This epic should complete the visual refresh for `DashboardOverlay` without changing dashboard behavior. The goal is not
to add new interactions, new overlay variants, or a new modal framework. The goal is to make every existing overlay look
deliberate, elevated, scannable, and consistent with the refreshed shell while preserving deterministic plain-text
`--once` output and keyboard behavior.

Because each phase will be completed by a distinct agent instance, the phases below are sequential. Do not run them in
parallel: the main write target is `crates/zorg-dash/src/ui.rs`, and later phases should reuse helpers introduced by
earlier phases.

## Goals

- Make every `DashboardOverlay` variant render through one shared overlay shell path.
- Give overlays an elevated surface, clear title/border hierarchy, and variant-appropriate warning/error styling.
- Add shared helpers for form rows, selected overlay rows, key-help rows, confirmation sections, and log/detail rows.
- Style help, SWOG help, capture, capture template picker, diagnostic filters, todo prompts, yank, fix preview,
  confirmations, event log, and generic logs with semantic spans.
- Keep all warning, error, unsafe, unavailable, and confirmation states text-visible in no-color mode.
- Preserve all existing overlay keyboard behavior, state transitions, text meaning, and read/write confirmation flow.
- Keep overlays usable at 56x22, matching current narrow overlay coverage.

## Non-Goals

- Do not change `DashboardOverlay` state semantics, keyboard handling, write behavior, or confirmation requirements.
- Do not introduce a separate renderer, screenshot dependency, ANSI output, or user-configurable theme.
- Do not change row rendering, panel membership, main dashboard layout, or JSON output.
- Do not make the overlays larger unless a specific variant needs a bounded adjustment to remain readable.
- Do not replace plain-text confirmation prompts with color-only affordances.
- Do not add decorative glyphs that make `buffer_to_string` assertions brittle.

## Cross-Phase Guardrails

Every phase should:

- Prefer existing `DashTheme`, `BlockRole`, `overlay_block`, `badge_span`, `label_span`, `metadata_span`, `path_span`,
  `id_span`, `key_hint_span`, `selected_span`, `selected_style`, and semantic test helpers.
- Avoid literal colors outside `DashTheme` and tests.
- Keep `ColorMode::Disabled` strict: all rendered cells must have `Color::Reset` foreground and background.
- Preserve meaningful overlay text in `buffer_to_string`.
- Add focused TestBackend assertions for the touched overlay category.
- Keep overlay body text concise enough for 56x22 renders.

## Phase 27.1: Overlay Shell, Tone, And Shared Helpers

### Objective

Create the modal chrome and helper foundation that all overlay content phases will reuse.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Avoid `app.rs`, `model.rs`, README, and broad content rewrites in this phase.

### Work

1. Introduce a render-only overlay shell abstraction in `ui.rs`, such as `OverlaySpec` or `OverlayChrome`, that
   centralizes:
   - title
   - width/height percentage
   - semantic tone: neutral, warning, error, destructive, or success/safe if useful
   - block role / border style
   - optional body padding or inner margin, if it can be implemented without clipping narrow overlays
2. Extend block/shell handling only as needed so warning/error/destructive overlays can use `warning_border` or
   `error_border` while normal overlays keep elevated modal styling.
3. Keep `Clear` behavior and ensure the filled overlay surface is styled in color mode through the shared shell.
4. Route every `DashboardOverlay` variant through the shared shell/spec path, even if its body lines remain unchanged
   for now.
5. Add small shared helpers for later phases:
   - selected overlay/form row composition
   - overlay section heading line
   - muted instruction line
   - key/action help line
   - optional warning/error line helper
6. Add representative tests for:
   - neutral overlay uses elevated surface and subtle/elevated border
   - destructive or failed confirmation uses warning/error border
   - no-color overlay rendering resets foreground/background for all cells
   - `DashboardOverlay::Log` and `DashboardOverlay::ConfirmReindex` still render expected title/text

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Every `DashboardOverlay` variant reaches `Paragraph::block(...)` through one shared overlay spec/shell path.
- Overlay shell tests assert semantic styles without brittle full-screen snapshots.
- No overlay text content or keyboard behavior changes beyond harmless wrapping/spacing.

## Phase 27.2: Form Overlays

### Objective

Make editable and selectable overlays scan like structured forms while preserving their current behavior.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Do not change `CaptureDraft`, `CaptureTemplatePicker`, `DiagnosticFilterDraft`, or `TodoPromptDraft` behavior in
`model.rs` unless compilation reveals a tiny display-only helper is clearly safer.

### Work

1. Add or reuse a shared form-row helper that renders:
   - active marker `>`
   - label
   - required marker where applicable
   - current value or `-`
   - selected row styling for active rows
   - muted label styling for inactive rows
2. Apply that helper to:
   - `capture_lines`
   - `capture_template_picker_lines`
   - `diagnostic_filter_lines`
   - `todo_prompt_lines`
3. Style template metadata consistently:
   - template IDs with `id_span`
   - destination and variables as label/value metadata
   - paths with `path_span` / `path_line`
4. Style todo prompt available fields as structured metadata instead of one plain sentence where practical.
5. Keep errors in todo prompts semantically styled with an error badge, but make sure the literal `Error:` text remains
   visible in no-color mode.
6. Preserve all instruction text and current keyboard behavior.
7. Add tests for:
   - selected capture field uses selection style across marker, label, and value
   - inactive form labels are muted
   - template picker selected row uses selection style and metadata uses path/value styles
   - diagnostic filter active row uses selection style
   - todo prompt error text uses semantic error style
   - 56x22 narrow overlay test still includes titles and core instructions

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Capture, capture picker, diagnostic filter, and todo prompt overlays use one shared form-row styling path.
- Active form fields visually match main/nav selection semantics.
- Plain-text render output still contains the same labels, values, and instructions expected by existing tests.

## Phase 27.3: Fix Preview And Confirmation Overlays

### Objective

Make write-adjacent overlays clearly communicate operation, target, risk, planned changes, and confirmation instruction.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Avoid changing fix selection, todo action planning, confirmation keys, or write behavior in `app.rs`.

### Work

1. Update `fix_preview_lines` and `fix_preview_row_lines` to use structured sections:
   - diagnostic context
   - candidate fix rows
   - replacement preview
   - marked diagnostics summary
   - action hint
2. Style fix state text as badges or badge-like spans:
   - safe
   - unsafe
   - preferred
   - unavailable
3. Style replacement preview labels, paths, diagnostic codes, warnings, and unavailable reasons with semantic helpers.
4. Update `confirm_fix_apply_lines` to style:
   - operation prompt
   - target path/position/diagnostic/message
   - selected fix code and explanation
   - replacement preview label/content
   - final confirmation instruction
5. Update `confirm_todo_lines` to style:
   - operation prompt
   - target path/ID/title
   - planned changes heading
   - changed field labels and before/after values
   - warning list
   - final confirmation instruction
6. Update `ConfirmReindex` body lines to use warning/destructive shell tone and structured confirmation instruction,
   while preserving the explicit `Press y or enter...` text.
7. Add tests for:
   - fix preview safe/preferred/unavailable badges
   - fix preview path and diagnostic code semantic styles
   - confirm-fix destructive/warning shell tone and confirmation instruction
   - confirm-todo warning lines use warning style
   - no-color confirmation overlays remain text-readable and color-free

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Fix preview no longer depends on plain unstructured paragraphs for safety/risk states.
- Confirm-fix, confirm-todo, and confirm-reindex remain explicit confirmations with no-color text affordances.
- No write behavior, fix eligibility, or todo action planning changes.

## Phase 27.4: Help And SWOG Reference Overlays

### Objective

Make help/reference overlays dense, grouped, and easier to scan without changing documented commands.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

README only if this phase intentionally changes visible help copy in a way that should be documented.

### Work

1. Replace the inline `DashboardOverlay::Help` vector with a helper such as `help_lines(theme)`.
2. Group general help into styled lines where key tokens are distinct from action descriptions:
   - quit/close
   - navigation
   - capture/todo actions
   - diagnostics/fixes
   - search/SWOG
   - log/open/yank
3. Keep every existing meaningful key/action visible, including `q/Esc`, `tab/backtab`, movement keys, capture, todo
   actions, yank, fix preview/apply, diagnostics filters, refresh/reindex, enter/open, search, F1, L, and search edit
   behavior.
4. Refine `swog_help_lines` so sources/operators/examples use semantic spans:
   - labels muted
   - tags/query IDs/links/domain examples accented
   - operators and output kinds readable as primary/value text
5. Add tests for:
   - help key tokens use `key_hint` style
   - descriptions use body or muted text rather than key style
   - SWOG tag/query/link examples use existing domain/link/id styles
   - normal and 56x22 SWOG/help overlays still include current important examples and `Keys`

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Help and SWOG overlays use structured line helpers instead of a large plain string vector.
- Existing help content remains findable in plain-text render output.
- Narrow overlay coverage remains green at 56x22.

## Phase 27.5: Yank, Event Log, And Generic Logs

### Objective

Polish the remaining non-form, non-confirmation overlays that expose copied values, status history, and ad hoc messages.

### Scope

Primary file:

- `crates/zorg-dash/src/ui.rs`

Avoid changing `YankOverlay`, `StatusEvent`, clipboard behavior, or log event creation.

### Work

1. Update `yank_lines` so each option has clear token hierarchy:
   - selected marker and row use selection styling
   - option number/key is key-hint or muted metadata
   - value kind label is structured
   - available value is primary/path/id-like when recognizable
   - unavailable reason uses warning style while retaining literal text
2. Keep direct-copy number hints (`1-3`) and fallback clipboard guidance visible.
3. Update `status_event_lines` so events have consistent hierarchy:
   - order/severity badge
   - message primary text
   - detail lines muted or severity-styled based on event severity
4. Update `DashboardOverlay::Log { title, message }` conversion so generic logs get styled detail lines instead of raw
   `Line::from`, while preserving exact text content.
5. Add tests for:
   - selected yank option uses selection style
   - unavailable yank choice uses warning style and remains text-visible
   - event log severity badge and detail line styles
   - generic log title/body render through the shared overlay shell
   - no-color yank/log overlays reset foreground/background

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Yank, event log, and generic logs visually match the overlay system.
- Clipboard and status-event behavior is unchanged.
- Plain-text output still includes target summaries, values, unavailable reasons, event order/severity, messages, and
  details.

## Phase 27.6: Integration, Compatibility, And Documentation Pass

### Objective

Close the epic by verifying every overlay variant, consolidating tests, and documenting only durable conventions.

### Scope

Primary files:

- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs` only if one-shot render tests need semantic expectation updates

Secondary file:

- `crates/zorg-dash/README.md` only for concise user-facing copy if visible help/overlay behavior changed materially.

### Work

1. Audit all `DashboardOverlay` variants:
   - `Help`
   - `SwogHelp`
   - `ConfirmReindex`
   - `ConfirmFixApply`
   - `ConfirmTodoApply`
   - `TodoPrompt`
   - `Yank`
   - `CapturePicker`
   - `Capture`
   - `DiagnosticFilter`
   - `FixPreview`
   - `EventLog`
   - `Log`
2. Confirm every variant uses:
   - shared overlay shell/spec path
   - semantic title/border/surface style
   - text-visible state labels for warnings/errors/destructive actions/unavailable values
   - strict no-color rendering
3. Consolidate duplicated overlay tests where practical, but keep failures specific enough to identify the affected
   overlay.
4. Keep or extend the existing 56x22 narrow overlay matrix so it covers all variants, including generic `Log`.
5. Run the full validation set for the epic.
6. Add a concise renderer comment near overlay helper definitions explaining the overlay styling contract:
   - centralize chrome through the overlay spec
   - use semantic helpers for body rows
   - keep no-color text affordances
7. Update README only if help/key copy changed in a user-visible way. Do not document internal helper names in README.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- `cargo test -p zorg-cli --test smoke dash` passes.
- Every `DashboardOverlay` variant is covered by at least one render assertion.
- No-color coverage includes at least one shell overlay, one form overlay, one confirmation overlay, and one log/yank
  overlay.
- The final code has no new scattered literal colors in overlay call sites.
- Epic 27 leaves the renderer ready for Epic 28 final visual-regression and documentation consolidation.

## Validation Commands

Run these at the end of every phase unless a phase explicitly narrows validation during intermediate work:

```sh
cargo fmt --check
cargo test -p zorg-dash
```

Run these for Phase 27.6 and whenever confirmation/help behavior touches CLI smoke expectations:

```sh
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel diagnostics
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
```

## Suggested Agent Order

Run the phases strictly in order:

1. Phase 27.1 defines shared overlay chrome and helper APIs.
2. Phase 27.2 depends on form-row helpers and updates form-like overlays.
3. Phase 27.3 depends on shell tone support and updates write-adjacent overlays.
4. Phase 27.4 updates reference/help overlays without touching form/action logic.
5. Phase 27.5 updates yank/log overlays after shared body helpers have settled.
6. Phase 27.6 performs final audit, test consolidation, and validation.

Do not run these phases in parallel. They intentionally share `crates/zorg-dash/src/ui.rs`, and the later agents should
build on the helper names and tests introduced by earlier agents rather than inventing competing overlay patterns.

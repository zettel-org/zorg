---
bead_id: zorg-7.2
tier: epic
legend_bead_id: zorg-7
---
# Zorg Dash Epic 25 Frame Hierarchy, Header, Footer, Nav, And Inspector Plan

## Context

Epic 25 in `sdd/legends/202605/zorg_dash_visual_refresh_1.md` is the first visible dashboard-shell polish pass after the
Epic 24 visual foundation. Epic 24 is already present in the codebase: `crates/zorg-dash/src/ui.rs` has `DashTheme`,
`BlockRole`, block helpers, span helpers, badge helpers, and semantic style test utilities. That means this epic should
not invent a new visual system. It should apply the existing semantic primitives to the stable frame areas:
status/header, nav, main block hierarchy, inspector, footer, and latest-status slot.

The current renderer is still concentrated in `crates/zorg-dash/src/ui.rs`.

- `render_status` already uses styled spans, but the normal ready status line still includes long `root` and `db` path
  fields.
- `render_nav` highlights only the active label and leaves inactive rows plain instead of muted.
- `render_main` already uses an active block role; status uses the same active border family, while nav, inspector, and
  footer use lower-emphasis roles.
- `render_footer` already splits key tokens and labels into spans, but it has fixed 70/30 layout and an empty latest
  status slot with no explicit muted empty state.
- `render_inspector` treats only the first inspector line as emphasized and renders all other model-produced strings as
  plain text.
- `model.rs` owns the inspector text producers and should mostly remain unchanged for this epic.
- Existing tests assert key substrings in `--once` text output, narrow frames, style categories, and strict no-color
  behavior.

Because each phase will be completed by a distinct agent instance, these phases are sequential and intentionally scoped
around handoff boundaries. The agents should not run these phases in parallel: almost every phase touches `ui.rs`, and
the status/inspector phases will update overlapping tests.

## Goals

- Make the dashboard shell visually hierarchical without changing product behavior.
- Keep the active work area strongest, with nav, inspector, footer, and latest status visibly quieter.
- Make the top status cockpit compact and less path-heavy in normal ready frames.
- Preserve root and database path visibility in loading/degraded guidance and index/inspector context.
- Improve nav, footer, and latest-status scanability using existing semantic span helpers.
- Convert inspector rendering to section-aware styled `Line` values without a large model refactor.
- Preserve deterministic plain-text `--once` output and strict no-color rendering.

## Non-Goals

- Do not convert main list rows to rich span rows; that belongs to Epic 26.
- Do not redesign overlays; that belongs to the overlay epic.
- Do not add ANSI escapes, screenshots, or a non-Ratatui renderer.
- Do not change dashboard state, keyboard behavior, write behavior, JSON export semantics, or panel membership.
- Do not introduce user-configurable themes.
- Do not rewrite inspector producers in `model.rs` unless a tiny render-only metadata helper is clearly safer than
  parsing existing strings in `ui.rs`.

## Phase 25.1: Frame Block Hierarchy

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Tests in the `ui.rs` test module
- Avoid touching `model.rs`, `app.rs`, `lib.rs`, and README in this phase.

### Work

1. Audit current uses of `panel_block`, `status_block`, `footer_block`, and `overlay_block`.
2. Refine `BlockRole` only if needed to express the shell hierarchy more clearly. For example, keep or add roles for
   active main, inactive/subtle panel, status, footer, and elevated overlay.
3. Apply stronger active title/border treatment only to the main work area. Keep nav, inspector, footer, latest, and
   normal status visually lower than main while retaining readable titles.
4. Make status/header visually identifiable as app chrome without competing with main focus. If `BlockRole::Status`
   currently maps to the same active border as main, lower it or give it a distinct title treatment.
5. Keep all block titles and layout sizes stable unless a one-cell adjustment is required for readability.
6. Add tests that assert active and inactive block borders or titles render with different semantic styles in color
   mode.
7. Keep the disabled color-mode all-cell reset test passing.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- A standard rendered frame still contains `Zorg Dash`, `Panels`, `Main`, `Inspector`, `Keys`, and `Latest`.
- Color-mode tests prove the main block style differs from at least one inactive block style.
- No-color mode still leaves every rendered cell with reset foreground and background.

### Notes For The Implementing Agent

This phase is a shell hierarchy pass, not a status rewrite. Do not remove root/db from the status line here, and do not
touch inspector text styling beyond any block title/border style that comes from the role helpers.

## Phase 25.2: Status Cockpit

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Tests in `crates/zorg-dash/src/ui.rs` and `crates/zorg-dash/src/lib.rs` if one-shot text expectations need updates
- Avoid `model.rs` changes unless a very small display helper on existing frame data eliminates duplication.

### Work

1. Replace the path-heavy normal ready status line with compact grouped spans:
   - product/title signal, preserving visible `Zorg Dash`
   - index health
   - diagnostics
   - freshness
   - marked count
   - active panel
   - row counts
   - pending operation when present
2. Move always-visible `root` and `db` fields out of normal ready status frames. Keep root/database paths in loading and
   degraded guidance and in existing index/inspector contexts.
3. Define width-aware degradation based on the status area width:
   - drop root/db first if any non-ready fallback includes them
   - then drop detailed row-count text
   - preserve product/title, health, diagnostics, active panel, and pending operation whenever possible
4. Keep status content plain-text-compatible. Avoid decorative symbols that make `buffer_to_string` harder to assert.
5. Update golden-region and one-shot tests to assert stable labels and important values without depending on exact
   spacing.
6. Add tests for:
   - ready status excludes root/db while preserving health/diagnostics/panel
   - degraded or loading guidance still exposes root/db somewhere in the frame
   - narrow status retains the critical health/diagnostics/pending fields
   - pending activity remains deterministic.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- `cargo test -p zorg-cli --test smoke dash` passes if CLI smoke expectations are affected.
- Normal ready `--once` output is shorter and no longer shows root/db in the top status line.
- Degraded/missing-index `--once` output still includes `Root:`, `Database:`, `zorg db reindex`, and read-only guidance.
- Narrow renders still contain `Zorg Dash`, health, diagnostics, and active panel text.

### Notes For The Implementing Agent

Keep this phase about header/status content only. Do not restyle nav rows or parse inspector sections here, even if the
status helper introduces useful span utilities.

## Phase 25.3: Nav, Footer, And Latest Status Polish

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Tests in the `ui.rs` test module
- README only if visible footer/key behavior changes enough to need a user-facing note.

### Work

1. Style inactive nav labels with `muted_text`.
2. Style the active nav row consistently with the selection token across the visible row text, including the marker and
   label, while keeping `>` as the non-color active signal.
3. Keep custom panel nav entries using the same styling path as built-in panels.
4. Revisit footer layout for narrow widths. Keep key help and latest status visible without overlap; prefer stable text
   clipping over dynamic layout churn.
5. Keep key hints as styled spans with key token distinct from action label. Preserve current plain-text key help such
   as `q quit`, `? help`, `L log`, and `y yank`.
6. Give the latest status slot a muted empty state, and use semantic status severity style when an event is present.
7. Add tests for:
   - active nav row style
   - inactive nav row muted style
   - footer key token style
   - latest status severity style
   - narrow frame keeps keys and latest/status area visible.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Standard and narrow frames still contain `Panels`, the active panel label, `Keys`, and key hints.
- Latest status messages remain visible when present and do not hide key help.
- No-color mode remains readable and color-free.

### Notes For The Implementing Agent

The footer already uses span helpers; treat this as a behavior-preserving polish and narrow-layout hardening phase.
Avoid changing the keybinding table in README unless the visible behavior genuinely changed.

## Phase 25.4: Inspector Structure

### Ownership

- Primary file: `crates/zorg-dash/src/ui.rs`
- Tests in the `ui.rs` test module
- `crates/zorg-dash/src/model.rs` only if a tiny helper type is clearly better than UI-side parsing.

### Work

1. Convert inspector rendering from `Vec<String>` directly into section-aware `Vec<Line<'static>>`.
2. Keep model inspector text content intact. The rendered buffer should still contain the same useful words and values.
3. Add UI-side classification helpers for common inspector line shapes:
   - blank lines
   - known section headings such as `Graph context`, `Outgoing links`, `Incoming backlinks`, `Properties`, `Preview`,
     `Telemetry`, `Today queries`, `Rows`, `Stored query`, `Search query`, `Query error`, and `Index metadata`
   - label/value lines such as `ID:`, `Path:`, `Source:`, `Position:`, `Tags:`, `Definition:`, `Health:`, and
     `Snapshot freshness:`
   - warning/error/unavailable lines
   - link-ish and ID-ish values such as `@id`, `#tag`, `->`, `<-`, and source paths
4. Style section headings with title/accent styling, labels and metadata muted, paths with path style, IDs/links with
   domain accent style, and warnings/errors with severity styles.
5. Keep wrapping behavior and inspector block dimensions stable.
6. Add tests that assert semantic styles for:
   - `Graph context` and link section headings
   - a path label/value pair
   - an ID/link value
   - an error or unavailable inspector line
7. Keep plain-text tests that assert inspector content, graph context, query info, and index metadata passing.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- Inspector sections are visually structured in color mode.
- `buffer_to_string` still contains the same inspector content needed by one-shot tests and JSON-related expectations.
- No-color mode remains readable and color-free.

### Notes For The Implementing Agent

Prefer a small `inspector_line_for_render(text, theme) -> Line<'static>` style helper inside `ui.rs`. Only move
structure into `model.rs` if string classification becomes brittle enough to justify a focused model change.

## Phase 25.5: Integration, Documentation, And Compatibility Pass

### Ownership

- Primary files: `crates/zorg-dash/src/ui.rs`, `crates/zorg-dash/src/lib.rs` tests
- Secondary file: `crates/zorg-dash/README.md` only for concise user-facing documentation updates
- Do not start Epic 26 row rendering work.

### Work

1. Review the full Epic 25 shell for consistency:
   - active main is strongest
   - status/header is compact
   - nav and inspector are quieter
   - footer key help remains stable
   - latest status is readable
   - inspector sections use semantic styles
2. Update golden-region tests and one-shot assertions to focus on stable labels and content rather than exact spacing.
3. Add or consolidate narrow render tests so a 64-column frame shows `Zorg Dash`, `Panels`, `Main`, `Inspector`, and
   `Keys` without requiring exact coordinates.
4. Run validation commands for indexed and degraded states.
5. Update README only if the status/footer visible behavior changed in a way users need to know, such as root/db paths
   moving from the normal status line to the inspector/degraded guidance.
6. Leave code comments only where they clarify width degradation or inspector classification rules.

### Acceptance

- `cargo fmt --check` passes.
- `cargo test -p zorg-dash` passes.
- `cargo test -p zorg-cli --test smoke dash` passes.
- Indexed `--once` output for `index` and `today` panels renders successfully.
- Degraded `--once` output still gives clear read-only/reindex guidance.
- `--no-color` and `NO_COLOR=1` render without foreground/background colors.
- Epic 25 leaves main-row conversion untouched and ready for Epic 26.

### Notes For The Implementing Agent

This is the only phase that should take a broad compatibility view. If a behavior question arises, prefer preserving
existing text content and changing only style or placement.

## Suggested Agent Order

Run the phases strictly in order:

1. Phase 25.1 creates the shell hierarchy baseline.
2. Phase 25.2 changes top status content and updates affected one-shot expectations.
3. Phase 25.3 improves nav/footer/latest using the settled block/status hierarchy.
4. Phase 25.4 structures inspector lines after the chrome is stable.
5. Phase 25.5 performs final compatibility, docs, and validation.

Do not parallelize these phases. They share `crates/zorg-dash/src/ui.rs`, and later phases depend on the text and style
contracts introduced by earlier phases.

## Cross-Phase Guardrails

- Preserve `--once` as deterministic plain text. Styled spans must not introduce ANSI escapes.
- Preserve strict no-color behavior: rendered cells must have `Color::Reset` foreground and background in disabled mode.
- Use existing `DashTheme`, block helpers, span helpers, and semantic test helpers instead of raw color literals.
- Prefer semantic style assertions and substring checks over brittle full-screen snapshots.
- Keep layout dimensions stable unless narrow-readability tests prove a small adjustment is needed.
- Keep each phase focused. Do not begin Epic 26 row rendering or overlay-specific Epic 27 polish.
- Avoid unrelated refactors in `model.rs`, `app.rs`, CLI code, or action code.

## Overall Validation

Each implementation phase should run:

```sh
cargo fmt --check
cargo test -p zorg-dash
```

The final integration phase should additionally run:

```sh
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel index --no-color
NO_COLOR=1 cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual.sqlite3 --once --panel today
```

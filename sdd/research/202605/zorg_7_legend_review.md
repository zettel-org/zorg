---
research_date: 2026-05-04
bead_id: zorg-7
title: Zorg Dash Visual Refresh legend review
source_legend: sdd/legends/202605/zorg_dash_visual_refresh_1.md
reviewed_work:
  - sdd/epics/202605/zorg_dash_epic_24_visual_system_foundation.md
  - sdd/epics/202605/zorg_dash_epic_25_frame_hierarchy.md
  - sdd/epics/202605/zorg_dash_epic_26_rich_rows.md
  - sdd/epics/202605/zorg_dash_epic_27_overlay_polish.md
  - sdd/epics/202605/zorg_dash_epic_28_visual_regression_compatibility.md
  - crates/zorg-dash/README.md
  - crates/zorg-dash/src/ui.rs
  - crates/zorg-dash/src/lib.rs
  - crates/zorg-cli/tests/smoke.rs
verification:
  - sase bead show zorg-7
  - sase bead show zorg-7.1
  - sase bead show zorg-7.2
  - sase bead show zorg-7.3
  - sase bead show zorg-7.4
  - sase bead show zorg-7.5
  - git log --oneline db4d202..a1bcb36
  - git diff --stat db4d202..a1bcb36
  - cargo test -p zorg-dash
  - cargo test -p zorg-cli --test smoke dash
  - cargo run -q -p zorg-cli -- dash --help
  - cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3 --once --panel today
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3 --once --panel diagnostics --no-color
---

# Zorg Dash Visual Refresh Legend Review

## Placement Note

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory convention used by adjacent legend reviews such as `zorg_6_legend_review.md` and
`zorg_4_legend_review.md`. This is the same path the previous research pass produced; it is preserved here.

## Scope And Bead State

This review covers the completed work associated with `zorg-7`, "Zorg Dash Visual Refresh." `sase bead show zorg-7`
reports the parent legend and all five child epics as **CLOSED**:

1. `zorg-7.1` Epic 24: Visual System Foundation.
2. `zorg-7.2` Epic 25: Frame Hierarchy, Header, Footer, Nav, And Inspector.
3. `zorg-7.3` Epic 26: Rich Row Rendering.
4. `zorg-7.4` Epic 27: Overlay Polish.
5. `zorg-7.5` Epic 28: Visual Regression, Documentation, And Compatibility.

Unlike `zorg-1`, `zorg-4`, and `zorg-6`, the parent legend bead was actually closed during this work, so there is no
"open legend, closed epics" reconciliation gap left behind by `zorg-7`.

The work started with the visual refresh plan commit `bfbafec` and finished with `a1bcb36`. The relevant
implementation footprint is intentionally concentrated in the dashboard renderer and its compatibility tests.
`git diff --stat db4d202..a1bcb36` reports 22 files changed, ~7.7k insertions, ~0.9k deletions, with the dashboard
renderer dominating the diff:

| File | Net change |
| --- | --- |
| `crates/zorg-dash/src/ui.rs` | ~5.9k inserted/changed |
| `crates/zorg-dash/src/lib.rs` | small test additions |
| `crates/zorg-dash/src/model.rs` | minimal (~5 lines) |
| `crates/zorg-dash/README.md` | added "Visual Rendering Contract" |
| `crates/zorg-cli/tests/smoke.rs` | hardened no-color/--once coverage |

Almost everything else in the diff is SDD bookkeeping: the legend file, the five epic plans, the matching prompt
files, two unrelated research notes that landed during the same window, and the bead store updates.

This review focuses on what changed for users of `zorg dash`, not the renderer internals.

## One-Screen Summary

The `zorg-7` work turns `zorg dash` from a functional terminal view into a calmer, more polished operations surface.
Users still launch the same command, use the same panels and key bindings, and get the same read-only browsing and
explicit write confirmations, but the screen is now easier to scan.

The visible experience now has a compact status header, a quieter panel rail, a stronger main work area, a structured
inspector, a persistent key-help footer, and a latest-status slot. Rows are no longer mostly flat strings: todos,
diagnostics, saved queries, search results, custom panel rows, and index health rows now expose their important tokens
separately. Overlays for help, search syntax, capture, diagnostic filters, todo prompts, fix preview/apply,
confirmation, yank, and logs now use consistent modal treatment and clearer warning/error language.

The refresh preserves important existing contracts: `zorg dash --once` remains deterministic plain text with no ANSI
escape sequences, `zorg dash --once --json` remains the machine-readable frame export, and `NO_COLOR=1` / `--no-color`
remain first-class ways to run the dashboard without foreground or background colors.

## Glossary Of Visual Tokens

These are presentation-level concepts introduced or hardened by this legend. They are user-visible in the sense that
new dashboard styling decisions are expected to flow through them rather than through ad hoc colors.

- **DashTheme**: The semantic style provider in `crates/zorg-dash/src/ui.rs`. It exposes named methods such as
  `app_background`, `panel_surface`, `elevated_overlay_surface`, `subtle_border`, `chrome_border`, `active_border`,
  `warning_border`, `error_border`, `title`, `active_title`, `chrome_title`, `body_text`, `muted_text`, `selection`,
  `marked_row`, `severity`, `health`, `status`, `index_row`, `todo_accent`, `query_accent`, `dashboard_accent`,
  `graph_link`, `path`, and `key_hint`. Every method returns reset-foreground/reset-background styles in
  `ColorMode::Disabled`.
- **BlockRole**: The block surface role (`Subtle`, `Active`, `Status`, `Footer`, `Elevated`, `Warning`, `Error`) used
  by `panel_block`, `status_block`, `footer_block`, and `overlay_block`.
- **BadgeTone**: The compact text-badge style picker (`Severity`, `Status`, `Domain`). Domain tones cover `Todo`,
  `Query`, `Dashboard`, and `Link`.
- **RowRender**: The render-only struct used by main-panel rows. It carries spans, a base style, and a marked flag,
  and centralizes how selection and marked-row composition apply on top of row-specific semantic styles.
- **OverlaySpec / OverlayTone**: The overlay shell descriptor. Tones (`Neutral`, `Warning`, `Error`, `Destructive`)
  pick a `BlockRole` so warning, error, and destructive overlays read as intentional modal surfaces.
- **Visual Rendering Contract**: The README section under `crates/zorg-dash/README.md` that documents the durable
  rules for semantic styling, no-color behavior, one-shot plain text, JSON neutrality, row composition, inspector
  styling, and overlay styling.

These are internal in the strict sense, but they are the right things for daily users to know exist when filing bugs
("the diagnostic badge color is wrong on Today", "this overlay does not feel like an overlay", "no-color leaked
something") and the right things for contributors to extend rather than work around.

## How Users Interact With The Refreshed Dashboard

`zorg dash --help` (verified in this checkout) still reports the same option surface it had after `zorg-6`:

```
zorg dash [--root PATH] [--db PATH]
          [--panel today|inbox|queries|search|diagnostics|index]
          [--as @dashboard/id]
          [--query @id|SWOG]
          [--once]
          [--json]
          [--exit-after MS]
          [--auto-refresh MS]
          [--no-auto-refresh]
          [--no-state]
          [--state PATH]
          [--no-alt-screen]
          [--mouse]
          [--no-mouse]
          [--no-color]
```

Users still start with familiar launch forms:

```sh
zorg dash
zorg dash --panel today
zorg dash --panel diagnostics
zorg dash --once
zorg dash --once --json --panel today
zorg dash --as @dashboards/daily --panel open
NO_COLOR=1 zorg dash --once --panel diagnostics
zorg dash --no-color --once
```

The first-frame interaction model is unchanged: tab or arrow between panels, move selection with arrows or `j`/`k`,
use `/` for Search, `?` for help, `L` for the event log, `y` for yank options, and panel-specific keys for diagnostics
and todos. What changed is how much useful state is visible without reading every line.

In a ready indexed frame, the top status bar now emphasizes the facts users need most:

- index health
- diagnostic count
- freshness
- marked diagnostic count
- active panel
- compact per-panel row counts
- pending operation, when one is active

Root and database paths no longer dominate normal ready status frames. They still appear where they are useful:
loading and degraded guidance, the Index inspector, and other inspector contexts where the path is the answer rather
than chrome.

## Dashboard Shell

The main dashboard layout is now visually hierarchical:

- The active work area (`Main`) gets the strongest visual treatment via `BlockRole::Active`.
- Panels (`Panels`), inspector (`Inspector`), footer (`Keys`), and latest-status slot (`Latest`) use quieter
  `Subtle` / `Footer` roles.
- The top status bar uses `BlockRole::Status`, which keeps it identifiable as app chrome without competing with the
  active main border.
- Narrow terminal rendering is exercised by tests so labels such as `Zorg Dash`, `Panels`, `Main`, `Inspector`, and
  `Keys` should remain visible at the smaller standard widths covered by the QA matrix below.

For users, this reduces the "all boxes are equally loud" feel. The eye lands first on the active panel and selected
row, while supporting information remains available around it.

## Rich Rows

The biggest user-facing improvement is row scanability. Main-panel rows now separate the content users care about
instead of treating each row as one large text blob. Every `PanelRow` variant has a span-based renderer; whole-line
severity coloring is no longer the meaning carrier in normal color mode.

Diagnostic rows now read as structured repair/review items:

- severity appears as an explicit text badge such as `error`, `warning`, `info`
- diagnostic code remains visible with severity/domain emphasis
- message text stays readable without the whole line becoming visually severe
- path and position metadata are separated from the core message

Todo and zettel rows now make the todo marker, canonical ID, title/preview, badges, and source metadata easier to pick
apart. Users scanning Today can distinguish work items from diagnostics faster, while still seeing the same `[ ]`,
`[N]`, and ID text that works in no-color mode.

Saved query and custom dashboard rows now show validity, query identity, title, row counts, and source metadata more
clearly. Invalid saved queries remain visible as warning-badged rows instead of visually overwhelming the whole list.

Index rows now distinguish normal counts from attention-worthy counts such as diagnostics, deleted files, new files,
or changed files. This makes the Index panel more useful as a health summary rather than a plain dump of counters.

Selection and marked-state composition is now centralized in `RowRender`, so selected diagnostics, selected todos,
selected invalid queries, and selected index attention rows all compose readably without erasing their row-specific
semantics.

## Inspector

The inspector is now section-aware. Users selecting rows in Today, Inbox, Search, Diagnostics, Queries, or Index still
see the same underlying details, but the details are grouped and weighted more clearly.

Common section headings such as `Graph context`, `Outgoing links`, `Incoming backlinks`, `Properties`, `Preview`,
`Telemetry`, `Today queries`, `Stored query`, `Search query`, `Query error`, `Rows`, and `Index metadata` stand out.
Labels such as `ID:`, `Path:`, `Source:`, `Position:`, `Tags:`, `Definition:`, `Health:`, and `Snapshot freshness:` are
easier to distinguish from values. IDs, links, paths, warning lines, and error lines have separate visual treatment
when color is enabled, while the literal text remains present in no-color output.

This matters most when users are moving quickly through rows and relying on the inspector to answer: "What is this?",
"Where is it?", "Why is it broken?", and "What is connected to it?"

## Overlays

The refresh gives overlays a consistent modal feel without changing the commands that open them. Every
`DashboardOverlay` variant now reaches `Paragraph::block(...)` through one shared `OverlaySpec` shell path and uses
shared form-row, key-help, section-heading, and warning/error helpers.

Users should see clearer structure in:

- Help and SWOG reference overlays, where key tokens and examples are styled distinctly from descriptions.
- Capture template picker and capture form overlays, where active fields, template IDs, destinations, and required
  variables are organized like compact forms.
- Diagnostic filter and todo prompt overlays, where selected fields and validation errors are visually distinct.
- Fix preview and confirmation overlays, where safe, preferred, unsafe, and unavailable badges, target path,
  diagnostic code, replacement preview, warnings, and final confirmation instructions are easier to evaluate before a
  write.
- Yank overlays, where row ID, source link, diagnostic message, unavailable choices, and copy fallback guidance are
  clearer.
- Event log and generic log overlays, where severity and detail lines are easier to read.
- The reindex confirmation overlay, which now uses warning/destructive shell tone while preserving the explicit
  `Press y or enter...` text affordance.

The important safety behavior is unchanged: write-adjacent operations still require explicit confirmation and remain
understandable without color.

## Accessibility And Script Compatibility

The legend did not add a new user feature flag, but it strengthened two important user contracts.

First, no-color mode is now documented and tested as a strict contract. With `NO_COLOR=1` or `--no-color`, every
rendered cell should reset foreground and background colors while preserving labels, prompts, markers, warnings,
errors, selected-row text, and unavailable-choice text. Users in low-color terminals, logs, screenshots, or
accessibility-constrained setups should still be able to understand the dashboard. CLI smoke tests now assert there
are no ANSI escape bytes in `--once` output, not just that text renders.

Second, one-shot output remains plain text. `zorg dash --once` is still suitable for tests and scripts that need a
quick health frame, and the visual refresh does not introduce ANSI escape sequences there. Users who need structured
automation should keep using `zorg dash --once --json` for frame data, `zorg query --json` for query results, and
`zorg watch --format json` for watcher events.

## Per-Epic Detail

Each epic decomposed into ~5 sequential phases, run by distinct agent instances. Phase commit IDs are visible in
`git log db4d202..a1bcb36`.

### Epic 24 (`zorg-7.1`) — Visual System Foundation

Built the semantic theme and helper layer the rest of the refresh would consume.

- Phase 24.1 (`fc9535b`) introduced `DashTheme` with the full semantic token surface and a strict no-color contract.
- Phase 24.2 (`f70c3de`) added the shared block helpers (`shell_block`, `panel_block`, `status_block`,
  `footer_block`, `overlay_block`) and the `BlockRole` enum.
- Phase 24.3 (`de803c1`) added span and badge helpers (`label_span`, `value_span`, `metadata_span`, `path_span`,
  `id_span`, `link_span`, `key_hint_span`, `badge_span`) and `BadgeTone` / `DomainTone`.
- Phase 24.4 (`a5de7e7`) added semantic test helpers so future regressions point at semantic tokens rather than raw
  literal colors.
- Phase 24.5 (`a640ff7`) ran a final integration pass and added a concise theme guardrail comment.

This epic was intentionally not visually loud; later epics depend on its API.

### Epic 25 (`zorg-7.2`) — Frame Hierarchy, Header, Footer, Nav, And Inspector

Applied the foundation to the stable dashboard shell.

- Phase 25.1 (`bfebe5a`) gave `Main` a stronger active border/title and lowered nav, inspector, footer, and status to
  quieter roles.
- Phase 25.2 (`bcdd242`) compacted the top status cockpit into grouped spans (health, diagnostics, freshness, marked,
  panel, row counts, pending operation) and moved root/db paths out of normal ready frames.
- Phase 25.3 (`f826952`) styled inactive nav rows with muted text, matched active nav selection styling with main row
  selection, split footer key tokens from action labels, and gave the latest-status slot a muted empty state.
- Phase 25.4 (`8393a90`) converted inspector rendering from `Vec<String>` to section-aware `Vec<Line<'static>>` with
  semantic labels, paths, IDs, links, and warning/error styles.
- Phase 25.5 (`840c261`) ran a shell compatibility pass and updated narrow render expectations.

### Epic 26 (`zorg-7.3`) — Rich Row Rendering

Replaced monolithic main-row strings with span-based rendering for every panel.

- Phase 26.1 (`2ab923f`) introduced `RowRender` and centralized selection / marked composition.
- Phase 26.2 (`500b330`) restyled diagnostic rows with severity badges instead of whole-line color.
- Phase 26.3 (`1737eb0`) restyled zettel and todo rows with separated todo marker, canonical ID, title, and metadata.
- Phase 26.4 (`9cb398d`) restyled saved query and custom dashboard rows with `ok` / warning badges, query accents,
  row count metadata, and path styling.
- Phase 26.5 (`18dd136`) finished index status row rendering and consolidated the row contract across panels.

### Epic 27 (`zorg-7.4`) — Overlay Polish

Routed every overlay variant through a single shared shell.

- Phase 27.1 (`1c15662`) introduced `OverlaySpec` / `OverlayTone` and the shared overlay chrome.
- Phase 27.2 (`b323617`) refreshed capture, capture picker, diagnostic filter, and todo prompt overlays with shared
  form-row helpers.
- Phase 27.3 (`283f1c5`) restyled fix preview, confirm-fix, confirm-todo, and confirm-reindex overlays with badge,
  warning, and destructive tones.
- Phase 27.4 (`6208850`) restructured help and SWOG help overlays with key/action span hierarchy.
- Phase 27.5 (`8cc5c83`) restyled yank, event log, and generic log overlays.
- Phase 27.6 (`6313e55`) ran a final overlay audit and consolidated coverage.

### Epic 28 (`zorg-7.5`) — Visual Regression, Documentation, And Compatibility

Hardened the refresh and made it shippable.

- Phase 28.1 (`c866002`) consolidated regression test helpers.
- Phase 28.2 (`6d901f6`) added one-shot plain text and JSON compatibility coverage, including ANSI-escape-byte
  assertions in CLI smoke.
- Phase 28.3 (`e22cf37`) closed no-color leakage and narrow overlay rendering.
- Phase 28.4 (`e7a3645`) added the README "Visual Rendering Contract" section and the `DashTheme` guardrail comment.
- Phase 28.5 (`0ffc71a`) ran final visual QA across the documented terminal sizes.

## Visual Rendering Contract

The README now documents the durable contract under `crates/zorg-dash/README.md` ("Visual Rendering Contract"):

- All renderer styles flow through `DashTheme` semantic tokens. Block, span, row, and overlay helpers in `src/ui.rs`
  are the supported surface for new styling.
- New theme tokens are added only when an existing token does not describe the role.
- `NO_COLOR=1` and `--no-color` must reset both foreground and background for every rendered cell while preserving
  labels, markers, prompts, warning/error wording, and selection / marked-row text.
- Color is never the only signal for diagnostic severity, destructive actions, selected form fields, or unavailable
  choices.
- `zorg dash --once` is plain text. It must not emit ANSI escape sequences in either enabled-color or no-color mode.
- `zorg dash --once --json` is the machine-readable frame export and stays presentation-neutral; visual styling
  changes do not alter the JSON contract.
- Rows compose semantic content styles with selection and marked-row styles through `RowRender`, `selected_style`,
  and the row helper functions.
- Inspector lines keep labels, values, paths, IDs, links, metadata, and severity text distinct through span helpers.
- Overlays use the shared overlay shell, tone-aware blocks, form-row helpers, and warning/error/instruction line
  helpers so narrow and no-color rendering stay readable.
- New row kinds, inspector sections, badges, and overlay states should add focused `TestBackend` assertions in
  `src/ui.rs`. New one-shot text or JSON behavior belongs in `src/lib.rs`. CLI-level compatibility belongs in
  `crates/zorg-cli/tests/smoke.rs`.

## Important Non-Changes

The visual refresh intentionally does not change the dashboard's product boundary:

- No web dashboard was added.
- No screenshot renderer or alternate terminal backend was introduced.
- No user-configurable theme system was added.
- Panel membership, JSON semantics, write confirmations, Search behavior, diagnostic filtering, Today todo behavior,
  capture behavior, source opening, and dashboard state behavior were preserved.
- `zorg dash --once --json` remains presentation-neutral rather than carrying styling details. The
  `zorg.dash.frame` schema and `schema_version: 1` from `zorg-6` Epic 22 are unchanged.
- The `DashboardOverlay` variants are the same set as before. No new modal interactions were added.
- Row membership, sorting, dashboard state, write behavior, and JSON output are unchanged in Epic 26 by design.

This was a visual and usability pass over an already feature-rich dashboard, not a new dashboard mode.

## Evidence From The Current Checkout

`sase bead show zorg-7` reports the parent legend closed, with all five child epics closed. The child bead views also
show every phase under each epic closed.

The verified `today` one-shot frame over `fixtures/corpus` shows the refreshed structure:

- compact top line: `index current diagnostics 13 freshness current marked 0 panel today rows T/I/Q/S/D/X 16/1/5/0/13/8`
- left panel rail with active `> Today`
- main area titled `Main Today 1/16 marked 0`
- structured Today rows mixing todos and diagnostics
- inspector details with ID, path, position, todo state, lifecycle, properties, today queries, preview, and graph
  context
- footer with key hints and `Latest idle`

The verified `diagnostics --no-color` one-shot frame shows that no-color output remains meaningful: diagnostic
severity, code, message, absolute path, relative path, position, byte range, and zettel row context are visible as
plain text, while the surrounding chrome remains color-free.

The README now includes a "Visual Rendering Contract" section documenting the lasting expectations for semantic
styling, no-color behavior, one-shot plain text, JSON neutrality, row/inspector/overlay styling, and where future
tests belong.

## Validation Surface

Focused validation in this checkout passed:

```sh
cargo test -p zorg-dash
# 211 passed; 0 failed; 0 ignored

cargo test -p zorg-cli --test smoke dash
# 22 passed; 0 failed; 72 filtered out
```

The `zorg-dash` test suite covers theme tokens, block roles, badges, row composition, inspector classification,
overlay shells, selected and marked rows, severity and health styles, narrow and standard layouts, no-color
all-cell reset behavior, overflow corpus rendering across every built-in panel, JSON serialization, persisted state,
async app flows, and golden-region text assertions.

The CLI smoke tests cover `zorg dash --help`, the `--json` and `--auto-refresh` validation rules, indexed and
degraded one-shot text frames for every panel, JSON frame export for Today, Diagnostics, custom dashboards, definition
diagnostics, mouse / no-color / no-state / no-alt-screen flags, queries panel coverage, search invalidation, and
`assert_no_ansi` checks on `--once` output for both enabled-color and no-color modes.

A final design QA pass was run against the documented terminal sizes:

- 140x40
- 120x30
- 100x28
- 80x24
- 56x22 for overlays

Recommended broader validation (matches `docs/development.md` for v1.1):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p zorg-cli -- dash --help
cargo run -q -p zorg-cli -- dash --once --json --panel today
python3 tools/check_fixture_manifest.py
```

## Contributor Recipes

Future dashboard work should plug into the visual system rather than around it.

- **Add a new row kind**: extend the relevant `PanelRow` variant, return a `RowRender` from `ui.rs`, compose spans
  with `id_span`, `path_span`, `metadata_span`, `badge_span`, and `value_span`, choose an appropriate `BadgeTone`,
  and add a focused `TestBackend` assertion that the new row's badge/code/message uses the expected semantic style
  in color mode and resets in no-color mode.
- **Add a new inspector section heading**: produce the heading text from the model as before, then add it to the
  inspector classification helpers in `ui.rs` so it picks up `active_title()` styling. Add a render test for the
  section heading and any new label/value lines.
- **Add a new overlay variant**: build it through `OverlaySpec::new(...).tone(...)`, compose body lines using
  `overlay_form_row_line`, `overlay_section_heading_line`, `overlay_instruction_line`, `overlay_key_help_line`,
  `overlay_warning_line`, and `overlay_error_line`. Add a 56x22 narrow render test plus a no-color overlay test that
  asserts every cell resets foreground and background.
- **Add a new badge**: extend `BadgeTone` (or `DomainTone` for domain-specific accents) and route it through
  `badge_span`. Avoid raw `Color::*` literals at call sites.
- **Add a new no-color signal**: prefer text affordances (labels, markers, prompts, explicit warning/error wording)
  over modifier-only styling. Confirm with a no-color render test that the signal is still visible.

For every recipe, run at minimum:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Common Pitfalls / FAQ

- **"Why is my new dashboard color showing up wrong?"** Most likely the call site uses a raw `Color::*` literal
  instead of a `DashTheme` token, or it sets foreground/background outside of `enabled_style(...)`. Centralize the
  color in `DashTheme` and have the renderer call the new token.
- **"My overlay does not feel like an overlay."** Either it is not going through `OverlaySpec`, or its tone is
  `Neutral` when it should be `Warning`, `Error`, or `Destructive`. The shared shell handles surface, border, and
  title styles by tone.
- **"My change leaked color into no-color mode."** A new theme method or span style probably forgot to wrap its
  result in `enabled_style(...)`, or a new test asserts a raw color literal that the disabled-mode reset cannot
  satisfy. Add a no-color all-cell reset assertion for the affected render.
- **"My one-shot output broke a script."** Visual styling changes are intentionally not reflected in
  `buffer_to_string` or `--once`, but row content text or status spacing may have shifted slightly. Update scripts
  toward semantic substring checks rather than column-perfect parsing; for stable structured output, switch to
  `zorg dash --once --json` (`zorg.dash.frame`, `schema_version: 1`).
- **"My selected row erased the diagnostic severity color."** Selection composes with row-specific semantic styles
  through `selected_style(base_style, &theme)` rather than overwriting them. Make sure the row uses `RowRender` and
  composes through that helper.
- **"My fix preview overlay does not look destructive."** Confirmation overlays choose their tone explicitly. For
  destructive operations, set `OverlayTone::Destructive` (or `Warning`) on the `OverlaySpec` so the shell picks the
  right border and surface, and keep the literal `Press y or enter...` affordance.

## User-Facing Takeaway

For daily users, `zorg dash` should now feel less like a debug dump and more like a compact terminal cockpit. The
same workflows are present, but the refreshed screen makes it easier to answer these questions quickly:

- What is the health of my workspace?
- Which panel am I in?
- Which row is selected?
- Is this row a todo, diagnostic, query, search result, or index health item?
- What details matter for this selected item?
- Is this action safe, unavailable, destructive, or waiting for confirmation?
- Can I still read and operate the dashboard without color?

That is the main value of `zorg-7`: it raises the quality and readability of the dashboard experience without moving
the interaction model out from under existing users or scripts.

## Bottom Line

`zorg-7` successfully delivered the next dashboard layer for visual quality. The dashboard now reads as a calm,
hierarchical, badge-aware terminal cockpit, with consistent overlay chrome, structured inspectors, span-composed
rows, and a documented theme contract. None of the underlying behavior (panels, keys, write confirmations, JSON
schema, freshness logic, custom dashboards, persisted state) changed. Future dashboard work should extend
`DashTheme`, `BlockRole`, `BadgeTone`, `RowRender`, `OverlaySpec`, and the README "Visual Rendering Contract" rather
than reinventing styling.

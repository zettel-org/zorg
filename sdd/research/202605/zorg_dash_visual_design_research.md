---
research_date: 2026-05-04
title: Zorg dash visual design and color research
status: draft
source_context:
  - crates/zorg-dash/README.md
  - crates/zorg-dash/Cargo.toml
  - crates/zorg-dash/src/lib.rs
  - crates/zorg-dash/src/model.rs
  - crates/zorg-dash/src/ui.rs
  - crates/zorg-dash/src/app.rs
  - sdd/research/202605/zorg_dash_dashboard_research.md
  - sdd/research/202605/zorg_dash_next_improvements_research.md
validation:
  - cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3 --once --panel today
recommendation: build a small semantic theme system, use color to clarify hierarchy and state instead of only severity, and convert row rendering from whole-line strings to styled spans so the dashboard can feel calmer without sacrificing dense terminal ergonomics
---

# Zorg Dash Visual Design And Color Research

## Placement Note

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory layout used by adjacent generated SDD research, prompts, legends, epics, and tales.

## Current Visual Baseline

`zorg dash` is already a real Ratatui dashboard with useful interaction depth, but the visual system is still close to a
plain terminal debug frame. The source has a centralized `StylePalette` in `crates/zorg-dash/src/ui.rs`, yet it mostly
maps a small set of states onto default terminal colors:

- selection: bold plus `Color::DarkGray` background;
- diagnostics: red, yellow, cyan, and magenta by severity;
- health/status: green, yellow, red, and cyan;
- index rows: warning/error color only for non-zero attention counts.

The layout is functional: status on top, nav/main/inspector body, footer at bottom. The same `Block::default()
.borders(Borders::ALL)` treatment is used almost everywhere, so the screen has many equally weighted boxes. In a fixture
Today frame, the left nav, main list, inspector, keys, and latest-status areas all compete at similar visual strength.
The status line also carries too much long text in one run: index health, diagnostics, freshness, marked count, active
panel, row counts, root path, and database path.

The one-shot frame confirms the issue. Useful data is present, but the eye has few anchors beyond ASCII borders and the
selected `>` row. Diagnostic rows appear as repeated red-like attention items in the live TUI, while todo rows and query
rows are mostly unstyled default text. The inspector has rich content, including graph context, but section breaks are
plain text rather than visual hierarchy.

## Constraints Worth Preserving

The visual work should keep these existing product constraints intact:

- `NO_COLOR=1` and `--no-color` are documented and tested. Disabled color must leave a readable UI with no foreground or
  background colors in the Ratatui buffer.
- `--once` renders through `TestBackend` and `buffer_to_string`, so its text output intentionally loses style metadata.
  Do not add ANSI escapes to the default one-shot text path.
- Interactive rendering uses `CrosstermBackend`, so foreground/background styling, modifiers, border styles, and
  256-color or RGB colors are available in normal terminal use.
- The dashboard is dense operational software, not a marketing surface. The better target is calm, scannable, and
  stateful, with restrained color and clear hierarchy.
- Existing render tests already inspect individual cell colors for diagnostics, no-color behavior, and index health.
  The test harness is good enough to protect a richer theme.

## Ratatui Capabilities Already Available

The repo uses `ratatui = "0.29"` and `crossterm = "0.28"` in `crates/zorg-dash/Cargo.toml`. Ratatui 0.29 already has the
pieces needed for a much better visual system without new dependencies:

- `Block::style`, `Block::border_style`, and `Block::title_style` for low-emphasis panels and stronger active panels.
- `BorderType::Rounded`, `Plain`, `Double`, and `Thick` if a border change is desired.
- `Paragraph::style`, `List::style`, `List::highlight_style`, `ListItem::style`, and styled `Line`/`Span` values.
- `Color::Indexed(u8)` and `Color::Rgb(r, g, b)` in addition to the named colors currently used.
- TestBackend buffer inspection for foreground, background, and modifiers at precise cells.

The current code leaves most of this unused because rows collapse into strings before styling. `row_items` styles the
whole row, and `row_list_line` returns a `String`; that prevents separate colors for todo marker, ID, title, path,
diagnostic code, and muted metadata.

## Main Visual Problems

### 1. The Screen Has No Ambient Theme

Most cells inherit the terminal default. That is safe, but it means the dashboard depends on the user's terminal theme
for almost all visual personality. The app should define a small set of semantic surfaces while remaining conservative:

- app background;
- panel surface;
- elevated overlay surface;
- border subtle;
- border active;
- title/accent;
- text primary;
- text muted;
- selection foreground/background;
- marked row background;
- success, warning, error, info;
- domain accents for todo, zettel, query, dashboard, graph, path, and key hints.

The key is semantic naming. Avoid scattering literal colors across renderer functions.

### 2. All Boxes Have Equal Weight

Status, panels, main, inspector, keys, latest, and overlays all use the same border treatment. This produces visual noise
because every edge has the same importance. Better hierarchy:

- status bar: one subdued block or colored band, with compact colored badges;
- nav: muted border, active panel row colored, inactive labels dimmed;
- main: active border/accent because it is the primary work area;
- inspector: subdued border and muted metadata with strong section headers;
- footer: lower-contrast border and muted key labels;
- overlays: elevated surface with stronger title/border and a backdrop clear/fill.

### 3. Rows Are Too Monolithic

Rows are currently styled as full lines. That makes diagnostics visually loud and leaves normal work rows too plain.
Rows should be rendered as span-based `Line` values:

- todo marker in a todo accent;
- canonical ID in accent or muted accent;
- title/body preview in primary text;
- path/position/date metadata in muted text;
- diagnostic severity as a short colored badge, diagnostic code in strong text, path/message in primary/muted text;
- query validity as a warning badge, query title in primary, query source/row count in muted text;
- index labels in primary with count values colored by health/attention.

This is likely the highest-leverage code change for perceived polish.

### 4. The Status Line Is Overloaded

The current status line is technically useful but difficult to scan. It should become a compact cockpit:

- left: product/title and index health badge;
- middle: diagnostics, freshness, marked, active panel, row counts;
- right: pending operation or latest important status;
- root/db paths moved to the Index panel or inspector unless degraded/loading.

For narrow terminals, the status line should degrade by dropping root/db first, then row-count detail, while retaining
health and diagnostics.

### 5. Inspector Content Needs Section Styling

The inspector already contains high-value metadata and graph context, but only the first line is bold. Add section
styling for headings like `Graph context`, `Outgoing links`, `Incoming backlinks`, `Properties`, and `Preview`. Muted
styles should cover labels and paths; IDs and active links should get accent colors. This makes the inspector less like a
wrapped log and more like a structured detail pane.

### 6. Overlays Need Better Focus

Overlays use `Clear` plus a normal bordered paragraph. They should read as modal surfaces:

- filled elevated background;
- title/border style based on overlay type;
- selected form field background matching the main selection style;
- warning/error lines using semantic color;
- key instructions muted and placed consistently at the bottom when space allows.

This will improve help, capture, diagnostic filters, todo prompts, yank, and fix preview without changing behavior.

## Recommended Theme Direction

Use a restrained dark terminal theme by default, with enough hue variation to avoid a one-note palette:

- background: near-black neutral, not saturated blue;
- surfaces: very dark gray and slightly lighter elevated gray;
- borders: muted gray, active border in teal or blue-green;
- primary text: soft off-white;
- muted text: medium gray;
- selection: deep teal/blue-green background with bright foreground;
- success: green;
- warning: amber;
- error: red/coral;
- info: cyan/sky;
- query/dashboard accent: violet or blue, used sparingly;
- todo accent: green/teal;
- graph/link accent: cyan;
- marked rows: subtle amber-tinted background.

Prefer `Color::Indexed` values for broad compatibility, or a small `Color::Rgb` palette if the project is comfortable
assuming truecolor-capable terminals. A pragmatic compromise is to define the theme in one place and choose colors that
map cleanly in common terminals.

Do not use color as the only signal. Keep text labels, markers, severity words, `>`, `*`, and position counts.

## Implementation Shape

### Phase 1: Theme Tokens And Styled Blocks

Replace `StylePalette` with a richer `DashTheme` or expand it into semantic methods:

- `app_bg()`, `surface()`, `surface_elevated()`;
- `border()`, `border_active()`, `title()`, `title_active()`;
- `text()`, `muted()`, `accent()`, `selection()`, `marked()`;
- `severity(kind)`, `health(label)`, `status(kind)`;
- `domain_todo()`, `domain_query()`, `domain_path()`, `domain_link()`, `key_hint()`.

Add helpers such as `panel_block(title, role, active, theme)` so every renderer stops constructing identical default
blocks by hand. Keep `ColorMode::Disabled` returning default/no-color styles exactly as it does today.

### Phase 2: Header, Nav, Footer, And Inspector Hierarchy

Style the stable frame first:

- active main block gets active border/title;
- inactive blocks get subtle border/title;
- nav active row uses `List::highlight_style` or the existing explicit row style, with inactive labels muted;
- footer key hints are muted, with action keys styled distinctly from descriptions;
- status line becomes badge-like spans and removes root/db from the always-visible path;
- inspector section headings and labels receive distinct styles.

This phase changes the dashboard's feel without touching data models.

### Phase 3: Span-Based Rows

Change row rendering from `String` to `Line<'static>` or a small row view model:

- replace `row_list_line(row, frame) -> String` with `row_list_line(row, frame, theme) -> Line<'static>`;
- style selected rows by patching each span or by applying `ListItem::style` plus explicit foreground overrides where
  necessary;
- introduce badge helpers for severity, todo marker, query validity, custom panel source, and index health;
- keep one row per item to preserve viewport behavior.

This phase gives the biggest payoff for "easier on the eyes" because it reduces full-line warning/error color and makes
normal rows visually richer.

### Phase 4: Overlay Polish

Style modal overlays as elevated surfaces. Apply the same form-row selected style to capture, diagnostic filters, todo
prompts, and yank. Give fix previews clear safe/unsafe/preferred badges, and make destructive confirmation borders use
warning colors.

### Phase 5: Visual Regression Tests

Extend existing Ratatui tests rather than creating screenshot infrastructure:

- no-color mode still has `Color::Reset` foreground/background in every cell;
- active main border/title uses active style when color is enabled;
- inactive block borders are subtle;
- selected row has selection background;
- marked row has marked style when not selected;
- diagnostic rows color the badge/code, not necessarily the entire line;
- inspector section headers use title/accent style;
- narrow terminal render keeps all sections visible and avoids text overlap.

## Concrete Code Findings That Shape The Design

These are the load-bearing facts about the current renderer that the rest of this note builds on:

- `StylePalette` (`crates/zorg-dash/src/ui.rs`, around line 183) holds only `color_mode: ColorMode` and exposes
  `emphasis`, `selection`, `severity`, `health`, `index_row`, and `status`. There is no token for borders, titles, muted
  text, surfaces, or domain accents.
- The only text modifier used in the entire crate is `Modifier::BOLD`, applied once in `emphasis()`. `DIM`, `ITALIC`,
  `UNDERLINED`, and `REVERSED` are unused.
- Selection style is `BOLD` plus `Color::DarkGray` background with no explicit foreground. On terminals whose default
  foreground is itself dark gray, the selected row can be hard to read; on light terminals the dark-gray background
  inverts in the wrong direction. Selection should set both fg and bg, or fall back to `Modifier::REVERSED` when colour
  is disabled.
- Every panel uses `Block::default().title(...).borders(Borders::ALL)` (status, nav, main, inspector, keys, latest,
  every overlay). No `BorderType` variants are used. There is no notion of an active vs inactive border.
- Rows render as preformatted `String` values via `row_list_line()`. Diagnostics use a `"{severity:<7} {code:<30}
  {message}"` template, so the whole line gets `severity()` color rather than a badge.
- Only `List` and `Paragraph` widgets are used. There is no `Table` despite the diagnostic and todo views being
  effectively tabular. Switching to `Table` for those views would let columns be styled per-cell while keeping
  alignment, which is otherwise hard with `Line`/`Span`.
- Row kinds (`PanelRow::Diagnostic`, `Zettel`, `Query`, `IndexStatus`) each carry distinct fields, so a span-based
  `row_list_line(row, frame, theme) -> Line<'static>` should dispatch on the variant rather than try to be generic.
- `ColorMode` is a binary toggle wired to `--no-color` and `NO_COLOR` (lib.rs around 277 and 374). There is no notion of
  256-color vs truecolor. A future palette that uses `Color::Rgb` will silently degrade on 8-color terminals; that is
  acceptable but should be explicit in the theme module.

## Accessibility And Colour-Blind Safety

The current severity palette is the textbook worst-case axis for colour-vision deficiency: it leans on red, yellow, and
green for error, warning, and health. Roughly 1 in 12 men and 1 in 200 women have some form of red-green CVD, and
deuteranopia in particular collapses red and green into similar yellow-browns. Concrete mitigations:

- Keep redundant non-colour signals on every coloured element. Severity rows must keep their text label or short badge
  (`ERR`, `WRN`, `INF`); todo markers must keep their glyph; selection must keep `>`; marked rows must keep `*`. The
  current renderer already does this and the visual pass must not regress it.
- Prefer hue pairs that survive deuteranopia: blue/orange and cyan/magenta read as distinct under simulation; pure
  red/green do not. For severity, consider error in red-orange (`Color::Indexed(202)` or similar) and warning in amber
  (`Color::Indexed(214)`) so they remain distinguishable from each other and from the success green used for index
  health.
- Rely on luminance, not just hue, for the most important contrasts. The selection background and the marked-row
  background should differ from the panel surface in luminance so they remain visible in greyscale.
- Avoid using colour alone to indicate "active panel". Pair the active border with a bold title, a different
  `BorderType`, or a leading glyph so a CVD user or `NO_COLOR` user still sees focus.
- Aim for WCAG AA-equivalent contrast (about 4.5:1) between primary text and surface. With Ratatui this is set by
  picking specific indexed values rather than named `Color::Gray`, which is terminal-defined.

## Terminal Capability Tiers

The current code only branches on `ColorMode::Enabled` vs `Disabled`. A theme that uses `Color::Rgb` will be silently
remapped on terminals without truecolor, which is fine in modern terminals but worth being explicit about. A small,
explicit tier model keeps the dashboard predictable:

- `tier = NoColor` when `NO_COLOR` is set or `--no-color` is passed: every cell uses `Color::Reset` for fg and bg, and
  emphasis falls back to `BOLD`/`DIM`/`REVERSED`.
- `tier = Basic` (8/16 colour) when the environment looks limited: the theme uses `Color::Red`, `Color::Yellow`,
  `Color::Cyan`, etc., as today.
- `tier = Indexed256` when the terminal supports 256 colours: the theme uses `Color::Indexed(u8)` for muted greys,
  surfaces, and accents.
- `tier = Truecolor` when `COLORTERM=truecolor` or `COLORTERM=24bit`: the theme uses `Color::Rgb(r, g, b)` with
  hand-picked values.

Detection can be a thin helper rather than a dependency. Reading `COLORTERM` at startup is enough for the truecolor
case; defaulting the rest to `Indexed256` is safe in practice. The point is to keep this decision in the theme module,
not scattered across renderers.

## Underused Ratatui APIs

Ratatui 0.29 already exposes everything needed; the renderer just does not call it.

- The `Stylize` trait lets you write `"ERR".red().bold()` or `Span::raw(code).fg(theme.accent())` instead of building
  `Style::default()` values by hand. This dramatically shortens span construction once rows become span-based.
- `Block::title_alignment(Alignment::Right)` and `Block::title_top`/`title_bottom` allow row counts and marked counts
  to live in the right title slot, freeing the left title for the panel name. Today the main block crams everything
  into one left-aligned title string.
- `BorderType::Rounded` for inactive panels and `BorderType::Thick` (or `Plain` plus active colour) for the active
  panel gives a non-colour focus signal.
- `Modifier::DIM` is the right tool for muted metadata (paths, dates, hint text, empty-state lines) on terminals that
  honour it. On terminals that do not, the muted colour token still carries the visual difference.
- `Modifier::ITALIC` is appropriate for empty-state messages ("no diagnostics") and inspector preview placeholders.
- `Modifier::REVERSED` is the correct fallback for selection in `NoColor` mode and for the `*` marked-row indicator.
- `Clear` with a subsequent filled `Block` (using `Block::style(...)` for a background colour) gives an "elevated
  surface" feel for overlays without needing to draw a backdrop manually.
- `Table` (currently unused) is a better fit than `List` for the diagnostics panel, because the severity / code /
  message / path columns want fixed widths.

## Concrete Token Inventory

A first cut of the semantic tokens for a `DashTheme`. Names are illustrative; the point is the count and the role
split, not the exact RGB values. The previous section listed token names; the additions below show where each token is
expected to be used so the implementation does not have to guess:

| Token | Used by |
| --- | --- |
| `surface` | App-wide background; default `Block::style` |
| `surface_elevated` | Overlay block background after `Clear` |
| `border` | Inactive panel borders, footer divider |
| `border_active` | Active panel border, overlay border |
| `title` | All panel titles when inactive |
| `title_active` | Active panel title; overlay title |
| `text` | Primary row text, inspector body |
| `muted` | Paths, timestamps, "no rows" lines, footer hint descriptions |
| `accent` | Canonical IDs, links, key-hint keys |
| `selection_fg` / `selection_bg` | Selected row; both must be set, not just bg |
| `marked_bg` | Marked-row background, separate from selection |
| `severity_error` / `_warning` / `_info` | Diagnostic badges, status badges |
| `health_ok` / `_warn` / `_bad` / `_loading` | Index health badge, freshness badge |
| `domain_todo` | Todo marker, todo panel accent |
| `domain_query` | Query-validity badge, query panel accent |
| `domain_graph` | Inspector graph-context heading and link IDs |

A typical render call should reach for one of these tokens, not a literal `Color::Yellow`. Greppability matters: the
tests already grep for `Color::Yellow` in `assert_text_has_color`; once the theme exists, those tests should compare
against `theme.severity_warning()` so palette swaps do not silently break assertions.

## Badge Format Conventions

Badges are the highest-leverage replacement for full-line severity colour. Useful conventions to adopt up front so
every renderer reaches for the same shape:

- Width: pad badges to a fixed width per family so columns line up. Severity badges as `[ERR]`, `[WRN]`, `[INF]` (five
  cells including brackets) read well next to a left-aligned diagnostic code.
- Style: badge text bold, badge brackets in `muted` so the eye latches onto the letters. Background colour optional;
  prefer foreground-only badges for calmness.
- Health badges in the status line: `index:OK`, `index:STALE`, `index:LOADING…` where the value is colour-coded and
  the label is `muted`. This keeps the status bar scannable even without colour.
- Query validity: a leading `?` in `severity_warning` for invalid, nothing for valid; the query title stays primary.
- Todo markers: keep the existing single-character glyph but colour it with `domain_todo` and bold only when open.

Badges should never be the only signal. Every coloured badge keeps its text label.

## Overlay Backdrop And Focus

`Clear` removes content under the overlay rect, but the surrounding panels remain at full intensity, which makes the
overlay feel less modal. Two cheap options:

- After laying out the overlay, walk the cells of the surrounding `Frame::area()` and apply `Modifier::DIM` to each
  cell that is not inside the overlay rect. This is O(area) per frame but trivial compared with the rest of rendering.
- Or, render a dim `Block::default().style(Style::default().bg(theme.surface()))` over the full frame before drawing
  the overlay, so the backdrop colour shifts subtly. This is simpler but loses the underlying content.

Either is acceptable; the first preserves context and is preferable. Both should be skipped under `NO_COLOR` since they
rely on style.

## Span-Based Rows: Alignment Caveat

Moving from `String` rows to `Line` rows loses `format!` width specifiers (`{:<7}`, `{:<30}`). Three ways to keep
columns aligned:

1. Pad each `Span` text with spaces during construction, e.g. `Span::styled(format!("{code:<30}"), …)`. Simple, works
   today, but any double-width unicode in the value will desynchronise columns.
2. Use Ratatui's `Table` widget for the diagnostic and (eventually) todo panels. Each cell can carry its own style and
   the widget handles widths. This is the cleanest answer for tabular data and avoids the unicode-width pitfall.
3. Pre-compute column widths from the visible viewport and pad with `unicode-width` for correctness. Heaviest option;
   only worth it if the dashboard later needs full unicode support in IDs or titles.

The recommendation is `List` plus padded spans for non-tabular panels and a `Table` migration for diagnostics. The
migration is small because diagnostic rows already carry the right fields.

## Testing Strategy Additions

The existing tests inspect single cells via `assert_text_has_color`. Three additions keep the visual contract
verifiable without a snapshot framework:

- `assert_cell_has_modifier(buffer, x, y, modifier)` so DIM/ITALIC/REVERSED usage can be asserted (especially the
  `NO_COLOR` selection fallback).
- A theme fixture that returns deterministic test colours independent of tier detection, so tests do not flake based on
  `COLORTERM`. The renderer should accept a theme argument rather than reaching into env vars.
- A "no full-line severity colour" assertion: scan the diagnostic row and assert that only the badge cells carry
  severity foreground while the message cells carry `text` colour. This locks in the calmer-row goal.

If snapshot testing is desired later, `insta` works well with `buffer_to_string` output, but it adds review overhead
for every visual tweak; keeping targeted cell assertions is cheaper for a dashboard whose visual surface keeps
evolving.

## Open Questions

These need a human call before implementation begins:

- Is a dark-only default acceptable, or must the theme detect a light terminal background? Detection costs a
  dependency or a fragile escape-sequence query; defaulting to dark is the cheaper path.
- Is truecolor the assumed minimum on developer machines, or should the indexed-256 path be the default and truecolor
  the upgrade?
- Should the marked-row background coexist with selection (overlay both) or should selection visually dominate when a
  row is both marked and selected? Current code lets selection win; this should be made explicit.
- Should the diagnostic panel migrate to `Table` as part of this pass, or be deferred to a separate change to keep the
  visual diff reviewable?
- Are there terminals in the contributor base where `Modifier::DIM` is a no-op? If yes, muted text must rely on the
  colour token, not the modifier.

## Non-Goals For The First Pass

- Do not introduce a user-facing theme config file yet. The current need is one polished default.
- Do not add ANSI styling to default `--once` text output. Keep deterministic script output clean.
- Do not rewrite layout or add new dashboard features as part of the visual pass.
- Do not rely on color alone for severity, selection, marked rows, or pending actions.
- Do not make the UI look like a full-screen editor clone. Zorg Dash should remain a compact operational cockpit.

## Suggested Acceptance Criteria

- Interactive dashboard has a coherent default color theme with visibly different status, nav, main, inspector, footer,
  and overlay hierarchy.
- Todo, diagnostic, query, index, and graph/inspector content each have recognizable but restrained styling.
- Full-line red/yellow diagnostic rows are replaced with calmer badge/code emphasis.
- `NO_COLOR=1 zorg dash --once ...` and `zorg dash --no-color --once ...` remain readable and style-free.
- Existing keyboard help, row counts, and inspector details remain visible at standard and narrow sizes.
- `cargo test -p zorg-dash` passes, including expanded buffer-style tests.


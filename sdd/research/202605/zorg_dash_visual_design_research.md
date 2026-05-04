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


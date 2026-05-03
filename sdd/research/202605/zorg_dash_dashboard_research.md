---
research_date: 2026-05-03
title: Zorg dash dashboard implementation research
source_context:
  - sdd/legends/202605/zorg_next_features_without_dashboard.md
  - sdd/research/202605/zorg_next_feature_recommendations.md
  - crates/zorg-cli/src/main.rs
  - crates/zorg-query/src/lib.rs
  - crates/zorg-store/src/lib.rs
  - crates/zorg-watch/src/lib.rs
recommendation: implement zorg dash as a Rust TUI crate in this workspace
status: draft
---

# Zorg Dash Dashboard Implementation Research

## Scope

This note researches how to implement a future `zorg dash` command that launches an interactive terminal dashboard for a
Zorg corpus. It focuses on two decisions:

1. Whether the dashboard should be written in Rust in this repository or in a sibling `../zorg-dash` repository using a
   different language such as Python with Textual.
2. What user experience the first useful dashboard should provide.

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory layout already used by generated SDD docs and adjacent research.

## Context Reviewed

The requested context file, `sdd/legends/202605/zorg_next_features_without_dashboard.md`, explicitly excludes
dashboard work from the current feature plan. It also states a cross-cutting product rule that Rust remains the source of
truth for parse, query, refactor, import/export, and graph semantics.

The earlier recommendation file, `sdd/research/202605/zorg_next_feature_recommendations.md`, treated the daily dashboard
as a high-value follow-up after live indexing, query JSON, refactoring commands, and import/export foundations. In this
checkout, much of that foundation is already present:

- `crates/zorg-cli/src/main.rs` has `watch`, `query --json`, `path/open --json`, `promote`, `move`, `extract`, `fix`,
  and `capture` command surfaces.
- `crates/zorg-query/src/lib.rs` has structured `QueryExecutionResult` and row types with IDs, paths, titles, todo
  markers, spans, lifecycle dates, tags, and properties.
- `crates/zorg-store/src/lib.rs` has schema version 2 with FTS5 (`zettel_fts`) and an `IndexStatus` shape that can drive
  index freshness UI.
- `crates/zorg-watch/src/lib.rs` has a live watcher service, structured watcher events, stable JSON-ish states, and
  bounded run controls suitable for health checks and tests.

That means `zorg dash` no longer needs to invent core data contracts. It can be a thin interactive surface over existing
Rust boundaries.

## External Framework Notes

Ratatui is a Rust library for building fast, lightweight terminal UIs, and its documentation describes the default
`crossterm` backend as the usual choice for Linux, macOS, and Windows. Its backend docs also call out a `TestBackend`
for unit testing UI rendering. Sources: [ratatui.rs](https://ratatui.rs/),
[Ratatui backends](https://ratatui.rs/concepts/backends/), and
[docs.rs ratatui features](https://docs.rs/ratatui/latest/ratatui/).

Textual is a Python rapid application development framework for sophisticated terminal UIs, with widgets such as data
tables, tree controls, inputs, text areas, and list views. Its PyPI page lists Textual 8.2.5 as the latest release on
2026-04-30 and requires Python >=3.9,<4.0. Sources:
[Textual docs](https://textual.textualize.io/), [Textual GitHub](https://github.com/Textualize/textual), and
[Textual on PyPI](https://pypi.org/project/textual/).

Both are viable. Ratatui is a lower-level Rust UI toolkit; Textual is a higher-level Python app framework with faster UI
iteration and richer batteries included.

## Recommendation

Implement `zorg dash` in this Rust workspace as a new internal crate, likely `crates/zorg-dash`, wired into
`crates/zorg-cli` as a first-class subcommand.

Use Ratatui with the default Crossterm backend for the product path. Keep the dashboard read-mostly at first and route
all writes through existing Rust planners and command/library boundaries (`zorg-capture`, `zorg-fix`, `zorg-refactor`,
`zorg-store`, `zorg-query`, and `zorg-watch`).

Do not start with a separate `../zorg-dash` Python/Textual repository unless the team explicitly wants a disposable UX
prototype. Textual is attractive for experimentation, but it creates a second distribution story and pushes the UI toward
shelling out to `zorg --json` instead of sharing Rust types directly.

## Decision Matrix

| Question | Rust in this repo with Ratatui | `../zorg-dash` with Python/Textual |
| --- | --- | --- |
| Source-of-truth alignment | Strong. The dashboard can depend on existing Zorg crates and cannot accidentally reimplement semantics. | Medium. It should call `zorg --json`, which preserves semantics but adds process and schema friction. |
| Distribution | Strong. Ships inside the existing `zorg` binary or release archive. | Weaker. Requires Python, package management, and version compatibility alongside the Rust binary. |
| Development speed | Medium. Ratatui is lower level and needs explicit event/state management. | Strong. Textual has a high-level widget, layout, CSS, and testing model. |
| Testability | Strong enough. Ratatui has a test backend; existing Rust smoke tests can add bounded dashboard tests. | Strong. Textual has an app testing story, but tests would live outside the Rust workspace. |
| Runtime dependency footprint | Low. Adds Rust crates to an already Rust-first workspace. | Higher. Adds Python runtime and package dependency chain. |
| Contract pressure | Low. Can use Rust APIs directly and only expose JSON where useful. | High. Needs stable JSON for every panel and action. |
| Future Neovim integration | Strong. Neovim can keep using CLI/LSP JSON; TUI remains an optional terminal frontend. | Medium. A Python dashboard is a peer app, not naturally part of the Rust release surface. |
| Prototype flexibility | Medium. | Strong. |

The deciding factor is product ownership. `zorg dash` sounds like a native command, not an optional companion app. A
native command should live with the Rust CLI and use Rust semantics directly.

## Proposed Architecture

Add a `zorg-dash` crate with three layers:

- `model`: dashboard view models derived from `Store`, `QueryContext`, `IndexStatus`, query results, diagnostics, and
  watcher state.
- `actions`: typed dashboard actions that delegate to existing crates. Examples: reindex, capture, open path, mark done
  through a safe rewrite planner, run fix preview/apply, promote/move/extract preview.
- `ui`: Ratatui state, layout, keyboard handling, selection state, and rendering.

Wire `crates/zorg-cli/src/main.rs` with:

```text
zorg dash [--root PATH] [--db PATH] [--panel PANEL] [--query @id|SWOG]
```

Initial behavior:

- Resolve config with the same `ResolvedConfig` path used by existing commands.
- Open the store and read `IndexStatus`.
- If the index is missing or stale, show a degraded dashboard with clear actions: reindex, start watch externally, or
  continue with stale data.
- Avoid running a long-lived watcher inside the first dashboard version. Poll status on refresh and let users run
  `zorg watch` in another process. A later version can embed `zorg-watch` if single-writer behavior and terminal
  lifecycle are fully tested.
- Keep the command interactive only. Do not make `zorg dash --json`; use `zorg query --json`, `zorg db status`, and
  `zorg watch --format json` for machine output.

## UX Principles

The dashboard should be an operational cockpit, not a marketing screen or a replacement editor. It should answer:

- What needs my attention today?
- Is my index trustworthy?
- What can I capture quickly?
- What note or task should I open next?
- What structural problems need fixing?
- Which saved query or tag slice do I want to inspect?

Use dense, keyboard-first panels. The first screen should show useful corpus state immediately, not instructions. A
compact footer can show key bindings.

## Proposed First Screen

Default layout:

- Top status bar: root, database, index freshness, watcher hint, diagnostics count, current date.
- Left navigation: Today, Inbox, Search, Queries, Diagnostics, Graph, Capture, Index.
- Main list/table: rows for the selected panel.
- Right inspector: details for the selected row: title, ID, path, tags, properties, due/do dates, backlinks summary,
  diagnostics, and available actions.
- Footer: context-sensitive keys.

Recommended global keys:

- `q`: quit.
- `r`: refresh data from the store.
- `R`: run `db reindex` with confirmation.
- `/`: focus search/query input.
- `tab` / `shift-tab`: move focus between nav, list, inspector, and command line.
- `enter`: open selected row in `$EDITOR` or print/open path according to config.
- `c`: capture.
- `f`: fix preview for current file or corpus.
- `?`: help overlay.

## Panels And Actions

### Today

Purpose: daily command center.

Rows:

- overdue todos.
- todos due today.
- `do::today` or equivalent lifecycle-date items.
- recently modified notes.
- unresolved error diagnostics.

Actions:

- open selected zettel.
- mark todo done when the row has a source span and the rewrite is safe.
- postpone due/do date with a small selector.
- capture a new note or task.
- jump to related saved query.

Implementation note: this panel should be composed from built-in SWOG queries plus diagnostics/status calls, not custom
search logic.

### Inbox

Purpose: triage unprocessed material.

Rows:

- `#z/inbox`.
- uncategorized capture output.
- notes with missing area/project tags if a convention emerges.

Actions:

- add or edit tags/properties only through a safe edit planner.
- move/promote selected zettel when refactor planners support the selected shape.
- mark item as triaged.

### Search

Purpose: fast ad hoc retrieval.

Rows:

- results from a query input, with LIST/TABLE/count support when available.

Actions:

- edit query text.
- save query as a `#z/query` zettel through capture/template flow.
- open result.
- narrow by tag/property from the inspector.

### Queries

Purpose: browse saved dashboards without building dashboard-specific configuration yet.

Rows:

- zettel tagged `#z/query`.
- each saved query's title, ID, and result count.

Actions:

- run selected query.
- pin selected query as a dashboard panel in a future config file.
- open query definition.

### Diagnostics

Purpose: make corpus health visible.

Rows:

- syntax, semantic, resolution, and strict check diagnostics.

Actions:

- open diagnostic location.
- preview safe fixes.
- apply fix when the selected fix plan is complete and validation passes.
- reindex after writes.

### Graph

Purpose: inspect neighborhood context without replacing the editor.

Rows:

- current selection's backlinks.
- outgoing links.
- unresolved links.
- sibling/child links when represented by the semantic model.

Actions:

- open linked zettel.
- copy ID/path.
- run references-style lookup.

### Capture

Purpose: quick creation without leaving the dashboard.

Rows:

- available `#z/tmpl` templates.
- recent captures.

Actions:

- create note from template.
- create inbox item.
- create task with due/do date.
- open newly created note.

### Index

Purpose: trust and maintenance.

Rows:

- discovered files, indexed files, unchanged/new/changed/deleted counts, diagnostics count, last indexed time.
- watcher state when a watcher is embedded or observed in a later version.

Actions:

- run reindex.
- show configured root/database.
- display stale/missing index explanation.
- start `zorg watch` in a child process only after terminal lifecycle and shutdown behavior are designed.

## MVP Scope

The smallest useful `zorg dash` should include:

- Rust `crates/zorg-dash` crate.
- `zorg dash` subcommand in `zorg-cli`.
- Ratatui alternate-screen TUI.
- status bar, nav list, main table, inspector, footer.
- Today, Inbox, Search, Diagnostics, and Index panels.
- open selected row in `$EDITOR`.
- refresh and reindex actions.
- capture action that delegates to `zorg-capture`.
- no embedded watcher.
- no persistent dashboard layout config.
- no plugin system.
- no direct ad hoc source rewrites from the UI except through existing safe planners.

This MVP proves the daily loop without turning dashboard work into a separate application platform.

## Later Scope

After the MVP is stable:

- Saved dashboard layout derived from config and `#z/query` zettel.
- Embedded watcher mode or supervised watcher process.
- Mark-done/postpone actions backed by a small safe todo-property rewrite planner.
- Rich graph neighborhood view.
- Import/export progress panels.
- Theme config.
- Mouse support if it does not complicate keyboard-first workflows.
- Optional `zorg.nvim` command that opens `zorg dash` in a terminal split.

## Textual Prototype Option

A sibling `../zorg-dash` repo using Textual is reasonable only for a time-boxed UX prototype. The prototype should:

- consume only public `zorg --json` command output.
- not write Zorg source directly.
- not define product-only configuration that the Rust dashboard cannot read.
- document which widgets and flows should be ported back to Rust.

This path is useful if the team needs to quickly compare layouts, interaction models, or web-mode possibilities. It
should not become the default implementation unless the project explicitly accepts Python as a distribution dependency
for the `zorg dash` experience.

## Key Risks

- TUI scope creep. Mitigation: make panels query-backed and actions delegated.
- Stale data. Mitigation: show index freshness constantly and make refresh/reindex first-class.
- Split semantics. Mitigation: keep dashboard in Rust and use existing crates.
- Terminal lifecycle bugs. Mitigation: defer embedded watcher and child-process management until after the read-mostly
  dashboard is stable.
- Test fragility. Mitigation: separate model/action tests from rendering tests; use Ratatui's test backend for stable
  layout assertions and keep smoke tests bounded.

## Final Decision

Build `zorg dash` in this repository, in Rust, as a first-class part of the `zorg` CLI. Use Ratatui/Crossterm for the TUI
and structure the dashboard as a thin UI layer over existing Zorg store/query/watch/refactor/capture/fix crates.

Keep `../zorg-dash` + Textual as a prototype escape hatch, not the product direction. The dashboard's main job is to make
the existing Rust semantics usable every day; sharing those semantics directly matters more than faster initial UI
iteration.

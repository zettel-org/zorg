---
research_date: 2026-05-03
updated: 2026-05-03
revision: 2
title: Zorg dash dashboard implementation research
source_context:
  - sdd/legends/202605/zorg_next_features_without_dashboard.md
  - sdd/research/202605/zorg_next_feature_recommendations.md
  - crates/zorg-cli/src/main.rs
  - crates/zorg-query/src/lib.rs
  - crates/zorg-store/src/lib.rs (IndexStatus, schema v2, FTS5)
  - crates/zorg-watch/src/lib.rs
  - Cargo.toml (workspace already depends on tokio 1.52, notify 8.2, serde 1)
recommendation: implement zorg dash as a Rust TUI crate in this workspace using Ratatui + Crossterm
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

### Rust TUI options

- **Ratatui** is an immediate-mode Rust library for fast, lightweight terminal UIs. The default `crossterm` backend is
  the cross-platform choice on Linux, macOS, and Windows; a `TestBackend` is exposed for snapshot-style rendering
  tests. Sources: [ratatui.rs](https://ratatui.rs/), [Ratatui backends](https://ratatui.rs/concepts/backends/),
  [docs.rs ratatui features](https://docs.rs/ratatui/latest/ratatui/).
- **Cursive** is a higher-level retained-mode Rust TUI library with built-in views, focus, and a callback-driven
  event loop. It is more ergonomic for form-heavy apps but less common in modern Rust dashboards and has a larger
  opinion footprint. Source: [Cursive on crates.io](https://crates.io/crates/cursive).
- **iocraft** is a newer JSX/component-style Rust TUI experiment. Useful to know about but not yet a stable
  foundation for a shipping product. Source: [iocraft repo](https://github.com/ccbrown/iocraft).
- **Backend choice for Ratatui**: `crossterm` is the default and the right pick for Zorg because it does not require
  a separate `termios` build path on Windows and because the workspace already exposes `tokio` (which composes well
  with crossterm's async event reader). `termwiz` and `termion` are alternative backends but give us no concrete win
  for `zorg dash`.
- **Helper crates**: `tui-input` for line editing in the search box, `tui-textarea` if a multiline capture editor is
  ever wanted in-app, and `ratatui-image` only if cover/preview images are ever in scope (none planned).

### Python TUI option

- **Textual** is a Python rapid application development framework with sophisticated widgets (data tables, tree
  controls, inputs, text areas, list views), CSS-like styling, devtools, and a first-class app-testing harness. PyPI
  lists Textual 8.2.5 (2026-04-30) and requires Python `>=3.9,<4.0`. Sources:
  [Textual docs](https://textual.textualize.io/), [Textual GitHub](https://github.com/Textualize/textual),
  [Textual on PyPI](https://pypi.org/project/textual/).
- **Textual web mode** (`textual serve` / `textual-web`) can serve the same Textual app over HTTPS as a browser TUI.
  This is genuinely interesting for a future "share my Zorg dashboard" demo, but it would still call into Rust over a
  CLI/JSON or LSP boundary; it does not change the source-of-truth question.

### Other shapes considered (and rejected for now)

- **Web app / Tauri**: a browser dashboard backed by `zorg-ls` or a small Rust HTTP server is feasible, but it
  introduces a long-lived server process, port management, asset packaging, and a second UI runtime. None of those are
  justified by the daily-cockpit use case, which is keyboard-first, terminal-local, and shells out to `$EDITOR`.
- **Neovim-only dashboard**: technically possible inside `../zorg-nvim`, but it locks the experience to one editor and
  duplicates the panels we already plan to render in `:ZorgQuery` result buffers. `zorg dash` should be editor-agnostic
  and runnable from a plain shell.

Ratatui is a lower-level Rust UI toolkit; Textual is a higher-level Python app framework with faster UI iteration and
richer batteries included. Both are viable, but they sit on different ends of the build/buy curve and have different
implications for the source-of-truth boundary discussed below.

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
zorg dash [--root PATH] [--db PATH]
          [--panel PANEL]            # initial panel (today|inbox|search|queries|diagnostics|graph|capture|index)
          [--query @id|SWOG]         # preload Search panel with a query
          [--once]                   # render one frame and exit (screenshots, golden tests)
          [--exit-after MS]          # bounded run for smoke tests / hung-CI safety
          [--no-alt-screen]          # for debugging or terminals that misbehave on alt-screen
          [--no-mouse]               # disable mouse capture
          [--theme NAME]             # later; reserved
```

Initial behavior:

- Resolve config with the same `ResolvedConfig` path used by existing commands. Reuse `parse_store_options` so flag
  precedence matches `zorg query` and `zorg db status` exactly.
- Open the store read-only and read `IndexStatus` to drive the status bar and Index panel.
- If the index is missing or stale, show a degraded dashboard with clear actions: reindex, start watch externally, or
  continue with stale data. Never silently reindex on launch.
- Do not run a long-lived watcher inside the first dashboard version. Poll status on refresh and let users run
  `zorg watch` in another process. A later version can embed `zorg-watch` if single-writer behavior and terminal
  lifecycle are fully tested.
- Keep the command interactive only. Do not add `zorg dash --json`; use `zorg query --json`, `zorg db status`, and
  `zorg watch --format json` for machine output. The dashboard is a renderer, not a contract surface.

## Async / Event Architecture

Use a single-threaded `tokio` runtime (the `rt` feature already in the workspace) plus a small set of channels:

- **Terminal events** come from `crossterm::event::EventStream` (key, mouse, resize, paste).
- **Timer events** come from a `tokio::time::interval` ticking at ~250 ms for time-based UI updates (clock, "stale
  for Ns", animation of progress).
- **Background load events** come from a `tokio::sync::mpsc` populated by spawned tasks that run blocking store/query
  calls inside `tokio::task::spawn_blocking`. Each task carries a generation counter so stale results can be dropped
  on debounce.
- **Watcher hint events** (later): an optional `mpsc` populated by an external `zorg watch --format json` child
  process or by an embedded `zorg-watch` runtime, used only to flag "your view is older than the index" without
  forcing redraws.

The render loop stays simple: drain all pending events into a `select!`, fold them into the dashboard model, then
call `terminal.draw(|f| ui::render(f, &model))`. Avoid per-keystroke synchronous SQLite calls; everything that hits
disk goes through the background channel with a debounce of 80–120 ms for query input.

Cancellation is cooperative: each background task takes a `CancellationToken` (or a generation comparison) so
keystrokes that obsolete a query do not waste CPU. The dashboard never blocks the UI thread on a `Store::query`
call.

## Concurrency With The Watcher And Single-Writer Rule

The cross-cutting product rule from Epic 11 is "single writer when watcher is active; one-shot CLI commands remain
deterministic." `zorg dash` must respect that:

- The dashboard opens the store **read-only by default**. Read-only opens never contend with a `zorg watch` writer.
- Write actions (reindex, capture, fix-apply, refactor-apply) shell out to the existing one-shot CLI paths — they
  acquire whatever lock the writer side already takes, and they fail visibly if the watcher is mid-batch.
- The dashboard does not embed a writer in MVP. If a watcher is detected (PID file, lock file, or watcher status
  endpoint when one lands), the Index panel surfaces it as a banner: "Watcher active in PID N — writes go through
  it." This avoids two writers fighting for SQLite.
- Refresh strategy: in MVP, refresh on `r`, on background-task completion, and on a 2 s idle tick when the Index
  panel is focused. A later version can subscribe to watcher events and refresh on real changes.

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
- `~`: toggle in-app log overlay (errors, warnings, slow query notices).
- `?`: help overlay.

Bindings should lean Vim-friendly inside lists (`j`/`k` to move, `g`/`G` to jump, `n`/`N` for next/prev match) but
must stay discoverable: every binding shown in the footer should also be reachable from the help overlay, and the
help overlay must list panel-scoped bindings separately from globals.

### Terminal capability matrix

| Capability | MVP | Later |
| --- | --- | --- |
| Alt-screen (saved on exit) | yes | — |
| Resize (recompute layout) | yes | — |
| 256-color | yes | — |
| Truecolor (24-bit) when `COLORTERM=truecolor` | yes | — |
| Light/dark adaptive palette | yes (auto-detect via `COLORFGBG` then fall back to dark) | themable via config |
| Mouse (select row, scroll) | optional, behind `--no-mouse` opt-out | drag-resize panes |
| Bracketed paste in search input | yes | — |
| Kitty keyboard protocol (disambiguated keys) | best-effort if available | required for chord keys |
| `NO_COLOR` env respected | yes | — |
| Unicode width via `unicode-width` | yes | — |

Truecolor detection uses the standard `COLORTERM=truecolor|24bit` heuristic; everything degrades to a 256-color
palette and finally to a no-color theme when `NO_COLOR` is set.

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

## Testing Strategy

Three tiers of tests, all runnable from `cargo test --workspace`:

1. **Model + action unit tests** in `zorg-dash`: pure functions that turn `(IndexStatus, QueryExecutionResult,
   Diagnostic[])` into a panel view model. No terminal, no tokio. Fast and deterministic.
2. **Render snapshot tests** using Ratatui's `TestBackend`. Drive the model into known states and assert the rendered
   buffer (or a stable subset like the status bar text and the first N rows). Snapshots live next to the crate and
   are reviewed on change, similar to how `zorg-cli` golden output is reviewed today.
3. **Smoke tests** in `crates/zorg-cli/tests/smoke.rs` that spawn `zorg dash --once --root TMP --db TMP/db.sqlite`
   against a fixture corpus and assert exit status plus a captured frame on stdout (`--once` writes the rendered
   buffer to stdout and exits 0). A second variant uses `--exit-after 250 --panel today` to confirm the bounded run
   path still tears the terminal down cleanly.

Explicitly out of scope for tests in MVP: real keystroke fuzzing, mouse interaction, multi-process watcher coupling.
Those land with the features they cover.

## Distribution And Build Feature

Ship `zorg dash` inside the existing `zorg` binary, but gate the dashboard behind a default-on Cargo feature:

```toml
# crates/zorg-cli/Cargo.toml
[features]
default = ["dash"]
dash = ["dep:zorg-dash"]
```

Rationale:

- Default users get `zorg dash` with no extra steps — same install path, same release artifact.
- Headless or minimal builds (CI containers, embedded boxes) can run `cargo build -p zorg-cli --no-default-features`
  to skip Ratatui/Crossterm and shrink the binary if that ever matters.
- The CLI dispatch arm in `crates/zorg-cli/src/main.rs` becomes:

  ```rust
  Some("dash") => {
      #[cfg(feature = "dash")]
      { zorg_dash::run(args.collect()) }
      #[cfg(not(feature = "dash"))]
      { eprintln!("zorg dash: not built with the `dash` feature"); std::process::exit(2); }
  }
  ```

This keeps the workspace homogeneous (no separate distribution story, no second package manager) while leaving an
escape hatch for slim builds.

## Dashboards As Zettel (Future)

Once the MVP panel set is stable, lean into Zorg's own model: a "dashboard" can be a zettel tagged `#z/dashboard`
whose body is a list of named panels, each defined by a SWOG query (or a query zettel `@id`). For example:

```
#z/dashboard
+today  query:: due <= today AND #z/task
+inbox  query:: #z/inbox
+stuck  query:: #z/task AND -#z/done AND modified < -14d
```

`zorg dash --as @id` would then render that zettel's panels instead of the built-in default. This keeps dashboard
configuration inside the corpus (versioned, queryable, importable/exportable) and avoids inventing a parallel TOML
format. It is explicitly post-MVP — the MVP panels are hardcoded — but the model layer should be designed so a
future "load panels from zettel" path is a swap, not a rewrite.

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

## Explicit Non-Goals

These are deliberately out of scope so the dashboard does not drift into a second product:

- Replace `$EDITOR`. The dashboard renders, navigates, and triggers actions; it does not host long-form editing.
- Become a notes app with its own data model. All persistence stays in `.z` files and the SQLite index.
- Host a plugin system. New panels land as Rust code and tests, not as third-party scripts.
- Render the full graph as ASCII art. Neighborhood views (selection's backlinks, outlinks) are enough; a full graph
  visualization belongs in a separate tool (or a future web view).
- Provide a programmatic JSON output. Use `zorg query --json`, `zorg db status`, and `zorg watch --format json` for
  machine consumers. `zorg dash --once` exists for screenshots and golden tests, not as a contract.
- Bypass safe-rewrite planners. Every write action goes through the same code paths as `zorg fix`, `zorg refactor`,
  `zorg capture`. The dashboard never edits source spans directly.
- Replace `zorg.nvim`. The two surfaces complement each other; the dashboard is for shell users and
  triage/review-style sessions.

## Open Questions

- Do we want a `--server` mode (single long-lived `zorg dash` process attached to via a socket) so multiple terminals
  share state? Probably no for MVP, but worth deciding before the watcher is embedded.
- How does the dashboard show in-progress reindex output? Streaming log panel vs. a progress widget vs. a status-bar
  spinner. Default to spinner + log overlay (`~`), revisit if it feels thin.
- Should `enter` open in `$EDITOR` synchronously (suspend dashboard) or spawn a child editor and detach? Suspend is
  simpler and more familiar; spawn-detach is fancier but loses tight focus return. Pick suspend in MVP.
- Do we want a built-in capture wizard (multi-step form) or always shell out to `zorg capture --interactive`? Shell
  out in MVP to avoid duplicating template UX.
- Tree-sitter highlighting in the inspector preview: nice-to-have or scope creep? Start with raw text; add
  highlighting only after the parse crate exposes a stable highlight API.

## Bead And Epic Plan

The legend at `sdd/legends/202605/zorg_next_features_without_dashboard.md` already reserves epics 10–15 for the
non-dashboard v1.1 work. The natural slot for the dashboard is **Epic 16** (or `zorg-1.16` under the existing
naming), opened only after Epics 10–14 land so the dashboard can target stable contracts.

Suggested phase breakdown for Epic 16:

- **Phase 16.1** — `zorg-dash` crate skeleton, `dash` Cargo feature, CLI dispatch arm, `--once`/`--exit-after`
  smoke harness, empty terminal that draws status bar + nav.
- **Phase 16.2** — Today, Index, Diagnostics panels backed by `Store::index_status` and existing query/diagnostics
  APIs. Inspector with raw zettel preview. Refresh + reindex actions.
- **Phase 16.3** — Search panel using `zorg-query` with debounced FTS-backed input. Capture action shells out to
  `zorg-capture`.
- **Phase 16.4** — Inbox panel, fix preview/apply via `zorg-fix`, open-in-`$EDITOR`.
- **Phase 16.5** — Graph neighborhood panel and Queries panel (built-in only, no `#z/dashboard` zettel yet).
- **Phase 16.6** — Optional: dashboards-as-zettel loader, theme config, embedded watcher.

Each phase ends with `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and the `tools/validate_cross_repo.sh` harness once it exists.

## Key Risks

- TUI scope creep. Mitigation: make panels query-backed and actions delegated; keep the "Explicit Non-Goals" list
  visible in the crate README.
- Stale data. Mitigation: show index freshness constantly and make refresh/reindex first-class; never silently reindex
  on launch.
- Split semantics. Mitigation: keep dashboard in Rust and use existing crates; refuse to add a parallel JSON contract.
- Terminal lifecycle bugs (alt-screen not restored on panic, raw mode left on after crash). Mitigation: install a
  panic hook that always disables raw mode and leaves the alt-screen, mirroring the standard Ratatui pattern; cover
  with a `--once` smoke test that asserts the parent terminal is restored.
- Two-writer corruption when a `zorg watch` is running alongside the dashboard. Mitigation: open the store read-only
  in MVP, route writes through the same one-shot CLI paths the watcher already coordinates with, and surface watcher
  presence in the Index panel.
- Slow SQLite calls blocking the UI on large corpora. Mitigation: route every store/query call through
  `spawn_blocking` with debounce + generation counters; show a spinner when a query is in flight; budget worst-case
  latency in benchmarks (Phase 10.4 baseline corpus).
- Keystroke ambiguity on terminals without the kitty keyboard protocol (e.g. `Ctrl-I` vs `Tab`). Mitigation: avoid
  bindings that require disambiguation in MVP; add chord support only after detection lands.
- Test fragility from rendering snapshots churning on small style tweaks. Mitigation: separate model/action tests from
  rendering tests; assert structured frame summaries (rows, focused panel, status text) rather than raw character
  buffers wherever possible.
- Dependency footprint. Ratatui + Crossterm + tui-input add ~Nx00 KB to the binary. Mitigation: gate behind the
  `dash` feature so slim builds can opt out.

## Final Decision

Build `zorg dash` in this repository, in Rust, as a first-class part of the `zorg` CLI. Use Ratatui/Crossterm for the TUI
and structure the dashboard as a thin UI layer over existing Zorg store/query/watch/refactor/capture/fix crates.

Keep `../zorg-dash` + Textual as a prototype escape hatch, not the product direction. The dashboard's main job is to make
the existing Rust semantics usable every day; sharing those semantics directly matters more than faster initial UI
iteration.

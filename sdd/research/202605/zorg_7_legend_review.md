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
  - git diff --stat db4d202..a1bcb36 -- crates/zorg-dash crates/zorg-cli/tests/smoke.rs sdd/legends/202605/zorg_dash_visual_refresh_1.md sdd/epics/202605/zorg_dash_epic_24_visual_system_foundation.md sdd/epics/202605/zorg_dash_epic_25_frame_hierarchy.md sdd/epics/202605/zorg_dash_epic_26_rich_rows.md sdd/epics/202605/zorg_dash_epic_27_overlay_polish.md sdd/epics/202605/zorg_dash_epic_28_visual_regression_compatibility.md
  - cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3 --once --panel today
  - cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_visual_research.sqlite3 --once --panel diagnostics --no-color
---

# Zorg Dash Visual Refresh Legend Review

## Scope And Bead State

This review covers the completed work associated with `zorg-7`, "Zorg Dash Visual Refresh." The authoritative bead
view reports the parent legend and all five child epics as **CLOSED**:

1. `zorg-7.1` Epic 24: Visual System Foundation.
2. `zorg-7.2` Epic 25: Frame Hierarchy, Header, Footer, Nav, And Inspector.
3. `zorg-7.3` Epic 26: Rich Row Rendering.
4. `zorg-7.4` Epic 27: Overlay Polish.
5. `zorg-7.5` Epic 28: Visual Regression, Documentation, And Compatibility.

The work started with the visual refresh legend planning commits around `db4d202` / `bfbafec` and was closed by
`a1bcb36`. The relevant implementation footprint is intentionally concentrated in the dashboard renderer and its
compatibility tests: `git diff --stat db4d202..a1bcb36` shows most code movement in `crates/zorg-dash/src/ui.rs`, with
supporting changes in `crates/zorg-dash/README.md`, dashboard render tests, CLI smoke tests, and the SDD epic records.

This review focuses on what changed for users of `zorg dash`, not the renderer internals.

## One-Screen Summary

The `zorg-7` work turns the dashboard from a functional terminal view into a calmer, more polished operations surface.
Users still launch the same command, use the same panels, and get the same read-only browsing and explicit write
confirmations, but the screen is now easier to scan.

The visible experience now has a compact status header, a quieter panel rail, a stronger main work area, a structured
inspector, a persistent key-help footer, and a latest-status slot. Rows are no longer mostly flat strings: todos,
diagnostics, saved queries, search results, custom panel rows, and index health rows now expose their important tokens
separately. Overlays for help, search syntax, capture, diagnostic filters, todo prompts, fix preview/apply,
confirmation, yank, and logs now use consistent modal treatment and clearer warning/error language.

The refresh preserves important existing contracts: `zorg dash --once` remains deterministic plain text with no ANSI
escape sequences, `zorg dash --once --json` remains the machine-readable frame export, and `NO_COLOR=1` / `--no-color`
remain first-class ways to run the dashboard without foreground or background colors.

## How Users Interact With The Refreshed Dashboard

Users still start with familiar launch forms:

```sh
zorg dash
zorg dash --panel today
zorg dash --panel diagnostics
zorg dash --once
zorg dash --once --json --panel today
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
loading/degraded guidance, Index context, and inspector details.

## Dashboard Shell

The main dashboard layout is now visually hierarchical:

- The active work area gets the strongest visual treatment.
- The panel list, inspector, footer, and latest-status area are quieter.
- The footer keeps key hints visible while the latest operation result appears beside them.
- Narrow terminal rendering is covered so critical labels such as `Zorg Dash`, `Panels`, `Main`, `Inspector`, and
  `Keys` should remain visible at small sizes.

For users, this reduces the "all boxes are equally loud" feel. The eye lands first on the active panel and selected
row, while supporting information remains available around it.

## Rich Rows

The biggest user-facing improvement is row scanability. Main-panel rows now separate the content users care about
instead of treating each row as one large text blob.

Diagnostic rows now read as structured repair/review items:

- severity appears as an explicit text badge such as `error` or `warning`
- diagnostic code remains visible
- message text stays readable without the whole line becoming visually severe
- path and position metadata are separated from the core message

Todo and zettel rows now make the todo marker, canonical ID, title/preview, badges, and source metadata easier to pick
apart. Users scanning Today can distinguish work items from diagnostics faster, while still seeing the same `[ ]`,
`[N]`, and ID text that works in no-color mode.

Saved query and custom dashboard rows now show validity, query identity, title, row counts, and source metadata more
clearly. Invalid saved queries remain visible as warning-like rows instead of visually overwhelming the whole list.

Index rows now distinguish normal counts from attention-worthy counts such as diagnostics, deleted files, new files,
or changed files. This makes the Index panel more useful as a health summary rather than a plain dump of counters.

## Inspector

The inspector is now section-aware. Users selecting rows in Today, Inbox, Search, Diagnostics, Queries, or Index still
see the same underlying details, but the details are grouped and weighted more clearly.

Common section headings such as graph context, outgoing links, incoming backlinks, properties, preview, telemetry,
stored query, search query, query error, rows, and index metadata stand out. Labels such as `ID:`, `Path:`, `Position:`,
`Tags:`, `Health:`, and `Snapshot freshness:` are easier to distinguish from values. IDs, links, paths, warning lines,
and error lines have separate visual treatment when color is enabled, while the literal text remains present in
no-color output.

This matters most when users are moving quickly through rows and relying on the inspector to answer: "What is this?",
"Where is it?", "Why is it broken?", and "What is connected to it?"

## Overlays

The refresh gives overlays a consistent modal feel without changing the commands that open them.

Users should see clearer structure in:

- Help and SWOG reference overlays, where key tokens and examples are easier to scan.
- Capture template picker and capture form overlays, where active fields, template IDs, destinations, and required
  variables are organized like compact forms.
- Diagnostic filter and todo prompt overlays, where selected fields and validation errors are visually distinct.
- Fix preview and confirmation overlays, where safe, preferred, unsafe, unavailable, target path, diagnostic code,
  replacement preview, warnings, and final confirmation instructions are easier to evaluate before a write.
- Yank overlays, where row ID, source link, diagnostic message, unavailable choices, and copy fallback guidance are
  clearer.
- Event log and generic log overlays, where severity and detail lines are easier to read.

The important safety behavior remains unchanged: write-adjacent operations still require explicit confirmation and
must remain understandable without color.

## Accessibility And Script Compatibility

The legend did not add a new user feature flag, but it strengthened two important user contracts.

First, no-color mode is now documented and tested as a strict contract. With `NO_COLOR=1` or `--no-color`, every
rendered cell should reset foreground and background colors while preserving labels, prompts, markers, warnings,
errors, selected-row text, and unavailable-choice text. Users in low-color terminals, logs, screenshots, or accessibility
constrained setups should still be able to understand the dashboard.

Second, one-shot output remains plain text. `zorg dash --once` is still suitable for tests and scripts that need a
quick health frame, and the visual refresh should not introduce ANSI escape sequences there. Users who need structured
automation should keep using `zorg dash --once --json` for frame data, `zorg query --json` for query results, and
`zorg watch --format json` for watcher events.

## Important Non-Changes

The visual refresh intentionally does not change the dashboard's product boundary:

- No web dashboard was added.
- No screenshot renderer or alternate terminal backend was introduced.
- No user-configurable theme was added.
- Panel membership, JSON semantics, write confirmations, Search behavior, diagnostic filtering, Today todo behavior,
  capture behavior, source opening, and dashboard state behavior were preserved.
- `zorg dash --once --json` remains presentation-neutral rather than carrying styling details.

This was a visual and usability pass over an already feature-rich dashboard, not a new dashboard mode.

## Evidence From The Current Checkout

`sase bead show zorg-7` reports the parent legend closed, with all five child epics closed. The child bead views also
show every phase under each epic closed.

The verified `today` one-shot frame over `fixtures/corpus` shows the refreshed structure:

- compact top line: `index current diagnostics 13 freshness current marked 0 panel today`
- left panel rail with active `Today`
- main area titled `Main Today 1/16 marked 0`
- structured Today rows mixing todos and diagnostics
- inspector details with ID, path, position, todo state, lifecycle, properties, preview, and graph context
- footer with key hints and `Latest` status

The verified `diagnostics --no-color` one-shot frame shows that no-color output remains meaningful: diagnostic severity,
code, message, absolute path, relative path, position, and source row context are visible as plain text.

The README now includes a "Visual Rendering Contract" section documenting the lasting expectations for semantic
styling, no-color behavior, one-shot plain text, JSON neutrality, row/inspector/overlay styling, and where future tests
belong.

## User-Facing Takeaway

For daily users, `zorg dash` should now feel less like a debug dump and more like a compact terminal cockpit. The same
workflows are present, but the refreshed screen makes it easier to answer these questions quickly:

- What is the health of my workspace?
- Which panel am I in?
- Which row is selected?
- Is this row a todo, diagnostic, query, search result, or index health item?
- What details matter for this selected item?
- Is this action safe, unavailable, destructive, or waiting for confirmation?
- Can I still read and operate the dashboard without color?

That is the main value of `zorg-7`: it raises the quality and readability of the dashboard experience without moving
the interaction model out from under existing users or scripts.

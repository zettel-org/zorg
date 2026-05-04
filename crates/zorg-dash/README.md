# zorg-dash

`zorg-dash` implements the terminal dashboard used by `zorg dash`.

The dashboard opens the configured SQLite store read-only for normal browsing.
Run `zorg db reindex` or `zorg watch` separately to keep that store current; the
dashboard does not start a watcher in the MVP. Explicit write actions are routed
through existing crates:

- `R` confirms and runs a one-shot reindex through `zorg-store`.
- `c` opens a capture template picker, then creates a compact template-aware
  form through `zorg-capture`.
- `enter` runs a selected Queries row, or temporarily leaves raw/alt-screen state
  and opens the selected source location in `$EDITOR` outside Queries.
- `o` opens the selected source location in `$EDITOR`.

Useful launch forms:

```sh
zorg dash --root PATH --db PATH
zorg dash --panel today
zorg dash --panel queries
zorg dash --panel search --query '#z/inbox'
zorg dash --once
zorg dash --once --json --panel today
zorg dash --as @dashboards/daily --panel open
NO_COLOR=1 zorg dash --once --panel diagnostics
zorg dash --no-color --once
zorg dash --exit-after 250 --no-alt-screen
zorg dash --auto-refresh 5000
zorg dash --mouse
zorg dash --no-state
zorg dash --state /tmp/zorg-dash-state.json
```

Panels are Today, Inbox, Queries, Search, Diagnostics, and Index. `--once` renders one
deterministic text frame for tests and scripts that need a quick health check.
`--once --json` emits the same selected frame as a compact JSON object with a
`zorg.dash.frame` schema marker, root and database paths, active panel rows,
row counts, selected inspector details, telemetry, health, and freshness.
Interactive `--json` is rejected; the JSON output is a bounded one-shot frame
export, not a streaming dashboard API.

The capture picker and form show template metadata from `#z/tmpl` definitions:
selector, title, destination, source path, and required variables. The form keeps
the dashboard interaction compact by editing only the selected template plus the
template variables that need dashboard input, currently `title` and `body`;
`id`, `date`, and `source` remain generated or filled by the capture action.

Dashboard zettels tagged `#z/dashboard` can add query-backed custom panels. Run
`zorg dash --as @dashboard/id` to load the dashboard definition, and select a
custom panel with `--panel key` or normal tab navigation. Each direct
`#z/panel` child needs `key::`, `title::`, and either `query::@queries/id` or a
single fenced `swog` block. Custom panels render zettel rows with the same
selection, source opening, yank, preview, and graph inspector behavior as Inbox
and Search. The panel header and inspector show the query source, row count, and
any panel-local query error; `--once --json` includes the selected dashboard,
custom panel metadata, active rows, and inspector details.

The inspector for selected zettel rows in Today, Inbox, and Search includes
bounded graph context: outgoing links, incoming backlinks, ancestors, and
descendants. Unresolved outgoing links are shown explicitly, and high-degree
sections show a truncation count instead of expanding without bound. JSON frame
exports include graph context only for the selected zettel row when available.

Interactive terminal startup renders a loading frame immediately, then replaces
it when the first read-only snapshot finishes loading. `--once` and stdout
fallback rendering still load synchronously so scripts receive a complete frame.

The normal ready status bar keeps health, diagnostics, freshness, marked count,
active panel, pending activity, and compact per-panel row counts visible without
printing root or database paths. Loading and degraded frames still show the root
and database path alongside read-only reindex guidance, and the Index inspector
shows dashboard telemetry: refresh count, initial load, refresh, search, and
last action timings when they are available. Idle auto-refresh is default-off;
when enabled with `--auto-refresh MS`, the Index inspector shows its configured
interval and latest refresh or skipped state.

Large-corpus validation uses the repository generator and perf helper:

```sh
tmp_parent="$(mktemp -d)"
python3 tools/generate_large_corpus.py --output "$tmp_parent/corpus" --files 1000 --zettels-per-file 4 --seed 10
cargo build -p zorg-cli
python3 tools/perf_large_corpus.py --root "$tmp_parent/corpus" --db "$tmp_parent/zorg-perf.sqlite3" --zorg-bin target/debug/zorg
```

The helper reindexes the generated corpus, runs a query freshness check, renders
Today and Index with `zorg dash --once`, and starts a PTY-backed bounded
interactive dashboard run with `--exit-after 50 --no-alt-screen --no-mouse`.
Treat the printed dashboard timings as local regression signals rather than CI
thresholds; the hard expectation is that the full commands complete and render a
dashboard frame.

Color is enabled by default for interactive rendering. Set `NO_COLOR` or pass
`--no-color` to disable foreground and background colors while keeping text
labels visible.

## Visual Rendering Contract

Dashboard rendering uses semantic theme tokens from `DashTheme` rather than
ad hoc color literals at call sites. New surfaces, borders, row text, badges,
status text, inspector labels, paths, IDs, links, and overlay lines should be
styled through the existing block, span, row, and overlay helpers in
`src/ui.rs`; add a new theme token only when an existing token does not describe
the role.

No-color mode is a strict rendering contract. `NO_COLOR=1` and `--no-color`
must reset both foreground and background colors for every rendered cell while
preserving labels, markers, prompts, warning/error wording, and selection or
marked-row text. Do not rely on color as the only signal for diagnostic
severity, destructive actions, selected form fields, or unavailable choices.

`zorg dash --once` is plain text for scripts and tests. It must not emit ANSI
escape sequences in either enabled-color or no-color mode. `zorg dash --once
--json` is the machine-readable frame export and should stay presentation
neutral; visual styling changes should not alter the JSON contract unless the
data model intentionally changes.

Rows should compose their semantic content styles with selection and marked-row
styles through `RowRender`, `selected_style`, and the row helper functions.
Inspector lines should keep labels, values, paths, IDs, links, metadata, and
severity text distinct through the span helpers. Overlays should use the shared
overlay shell, tone-aware blocks, form-row helpers, and warning/error/instruction
line helpers so narrow and no-color rendering stay readable.

When adding a new row kind, inspector section, badge, or overlay state, add or
extend the focused `TestBackend` assertions in `src/ui.rs`. New one-shot text or
JSON behavior belongs in `src/lib.rs` tests, and CLI-level compatibility belongs
in `crates/zorg-cli/tests/smoke.rs`.

Mouse capture is disabled by default because the dashboard does not yet attach
mouse gestures to useful actions. Pass `--mouse` to opt in for experiments;
`--no-mouse` remains accepted and keeps capture disabled.

Auto-refresh never starts a watcher or writes the store. It only reloads the
read-only dashboard snapshot while the interactive dashboard is idle, after the
configured interval or when freshness checks report a newer index. It is skipped
while prompts, overlays, or worker operations are active. `--once` rejects
`--auto-refresh` because one-shot output has no timer loop.

Interactive dashboard state is saved on clean exit under
`$XDG_STATE_HOME/zorg/dash/state.json`, or
`$HOME/.local/state/zorg/dash/state.json` when `XDG_STATE_HOME` is unset. The
state file stores the active panel key, selected dashboard ID, last search
query, recent search queries, mouse preference, and auto-refresh preference.
Explicit launch flags such as `--as`, `--panel`, `--query`, `--mouse`,
`--no-mouse`, `--auto-refresh`, and `--no-auto-refresh` override restored
values. Use `--no-state` to disable load and save, or `--state PATH` to use an
alternate state file. `--once` and stdout fallback rendering do not read or
write dashboard state.

Use `zorg query --json` when scripts need query results rather than a dashboard
frame. Use `zorg watch --format json` when scripts need a stream of watcher
state changes. `zorg dash --once --json` does not start, supervise, or inspect a
watcher; freshness only compares the loaded frame with the read-only index
generation and source/index health.

Key bindings:

| Key | Action |
| --- | --- |
| `tab`, `backtab`, left/right | Switch panels |
| up/down, `j`/`k`, `g`/`G` | Move selection |
| `/` | Edit the Search query |
| `r` | Refresh the read-only dashboard snapshot |
| `R` | Confirm and run reindex |
| `enter` | Run a selected Queries row, or open source outside Queries |
| `o` | Open the selected source in `$EDITOR` |
| `c` | Pick a capture template, review required variables, and create a zettel through `zorg-capture` |
| `t` | Cycle Today mode: combined, todos only, diagnostics only |
| `d` | Mark the selected Today todo done after confirmation |
| `p` | Postpone the selected due/do todo to `YYYY-MM-DD`, `+1d`, or `+1w` |
| `s` | Schedule the selected open/next todo by setting `do::YYYY-MM-DD` |
| `y` | Yank row values: row ID, source link, or diagnostic message |
| `f` | Preview a safe fix for the selected diagnostic row |
| space | Mark or unmark the selected diagnostic row |
| `e` | Cycle diagnostic severity filter: all, error, warning, info |
| `:` | Edit diagnostic code and path substring filters |
| `a` | Clear diagnostic filters |
| `L` | Show the recent status event log |
| `?` | Help overlay |
| `F1` from Search | SWOG syntax help |
| `H` on Search | SWOG syntax help |
| `q`, `Esc` | Quit or close the active overlay |

Search editing is a single-line editor. While editing Search, left/right move by
character, Home/End jump to the start/end, Backspace/Delete remove around the
cursor, Ctrl-W deletes the previous word, and Ctrl-U clears text before the
cursor. Enter runs and commits the query immediately. Esc cancels the edit and
restores the previous Search panel state. Up/Down recall recent non-empty
queries from the current dashboard session; adjacent duplicate queries are not
stored.

Key help remains visible in the footer while the latest refresh, reindex,
capture, todo write, search, or open result appears in the adjacent status slot.
Diagnostic filters are local dashboard state. They affect Diagnostics rows and
diagnostic rows in Today; zettel rows in Today remain visible.

Search accepts inline SWOG or stored query IDs such as `@queries/foo`. Press
`F1` while editing Search to keep query syntax examples in the dashboard,
including tags, property filters, todo markers, links, files, text, modified
date filters, boolean grouping, `TABLE <query>`, and `count(<query>)`.

Today defaults to combined mode, showing due/do/open todo rows alongside
diagnostics. Press `t` on Today to cycle through todos-only and
diagnostics-only modes. The Today header shows the active mode and filtered todo
and diagnostic counts, and selection is preserved by row identity when the row
remains visible.

Todo writes use guarded planner previews before touching source. `d` confirms a
mark-done edit, `p` prompts for a new due/do date, and `s` prompts for a
scheduled `do` date. Prompted dates accept strict `YYYY-MM-DD` values plus
simple relative intervals such as `+1d` and `+1w`; invalid dates stay in the
prompt until corrected or canceled with Esc.

Press `y` on any row to open a compact yank overlay. Row IDs prefer canonical
IDs like `@project/task`, then stable fallbacks such as `store:42`; source links
include path plus line/column when indexed, and diagnostic messages are offered
only on diagnostic rows. The dashboard first uses OSC 52 when stdout is a
terminal, then local clipboard commands where available; if no clipboard
transport works, the selected value is shown in the log overlay for manual use.

Marked diagnostics are local dashboard state for review queues. Marks use stable
diagnostic row identity, survive refresh while the same diagnostic remains, and
are dropped when a diagnostic disappears after reindex or apply. The fix preview
overlay summarizes marked diagnostics and explains that bulk apply is not yet
available; apply remains limited to one selected safe preview after confirmation.

The MVP intentionally avoids embedded long-form editing, persistent dashboard
layout configuration, a background watcher, and dashboard-specific parser/query
semantics.

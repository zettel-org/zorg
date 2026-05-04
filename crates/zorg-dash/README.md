# zorg-dash

`zorg-dash` implements the terminal dashboard used by `zorg dash`.

The dashboard opens the configured SQLite store read-only for normal browsing.
Run `zorg db reindex` or `zorg watch` separately to keep that store current; the
dashboard does not start a watcher in the MVP. Explicit write actions are routed
through existing crates:

- `R` confirms and runs a one-shot reindex through `zorg-store`.
- `c` opens a small capture form and creates a zettel through `zorg-capture`.
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
NO_COLOR=1 zorg dash --once --panel diagnostics
zorg dash --no-color --once
zorg dash --exit-after 250 --no-alt-screen
zorg dash --mouse
```

Panels are Today, Inbox, Queries, Search, Diagnostics, and Index. `--once` renders one
deterministic frame for tests and scripts that need a quick health check, but it
is not a JSON or stable automation contract.

Interactive terminal startup renders a loading frame immediately, then replaces
it when the first read-only snapshot finishes loading. `--once` and stdout
fallback rendering still load synchronously so scripts receive a complete frame.

The status bar includes compact per-panel row counts. The Index inspector also
shows dashboard telemetry: refresh count, initial load, refresh, search, and
last action timings when they are available.

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

Mouse capture is disabled by default because the dashboard does not yet attach
mouse gestures to useful actions. Pass `--mouse` to opt in for experiments;
`--no-mouse` remains accepted and keeps capture disabled.

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
| `c` | Capture a zettel through `zorg-capture` |
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

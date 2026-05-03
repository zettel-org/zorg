# zorg-dash

`zorg-dash` implements the terminal dashboard used by `zorg dash`.

The dashboard opens the configured SQLite store read-only for normal browsing.
Run `zorg db reindex` or `zorg watch` separately to keep that store current; the
dashboard does not start a watcher in the MVP. Explicit write actions are routed
through existing crates:

- `R` confirms and runs a one-shot reindex through `zorg-store`.
- `c` opens a small capture form and creates a zettel through `zorg-capture`.
- `enter` temporarily leaves raw/alt-screen state and opens the selected source
  location in `$EDITOR`.

Useful launch forms:

```sh
zorg dash --root PATH --db PATH
zorg dash --panel today
zorg dash --panel search --query '#z/inbox'
zorg dash --once
zorg dash --exit-after 250 --no-alt-screen --no-mouse
```

Panels are Today, Inbox, Search, Diagnostics, and Index. `--once` renders one
deterministic frame for tests and scripts that need a quick health check, but it
is not a JSON or stable automation contract.

Key bindings:

| Key | Action |
| --- | --- |
| `tab`, `backtab`, left/right | Switch panels |
| up/down, `j`/`k`, `g`/`G` | Move selection |
| `/` | Edit the Search query |
| `r` | Refresh the read-only dashboard snapshot |
| `R` | Confirm and run reindex |
| `enter` | Open the selected source in `$EDITOR` |
| `c` | Capture a zettel through `zorg-capture` |
| `?` | Help overlay |
| `q`, `Esc` | Quit or close the active overlay |

The MVP intentionally avoids embedded long-form editing, persistent dashboard
layout configuration, a background watcher, and dashboard-specific parser/query
semantics.

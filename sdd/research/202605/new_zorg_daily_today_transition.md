---
research_date: 2026-05-04
last_revised: 2026-05-04
title: New Zorg daily/today workflow transition research
source_context:
  - README.md
  - docs/quickstart.md
  - docs/syntax.md
  - docs/capture.md
  - docs/query.md
  - docs/import_export.md
  - docs/refactor.md
  - docs/lsp.md
  - docs/fix.md
  - crates/zorg-cli/src/main.rs
  - crates/zorg-dash/README.md
  - crates/zorg-dash/src/model.rs
  - crates/zorg-dash/src/app.rs
  - crates/zorg-watch/src/lib.rs
  - fixtures/corpus/query_and_template.z
  - fixtures/corpus/dashboard.z
  - fixtures/corpus/query_focus.z
  - ~/projects/github/bbugyi200/zorg/README.md
  - ~/projects/github/bbugyi200/zorg/src/zorg/app/runners/_run_edit.py
  - ~/projects/github/bbugyi200/zorg/src/zorg/service/templates.py
  - ~/projects/github/bbugyi200/zorg/src/zorg/service/file_groups.py
  - ~/org/zot/{all_day_logs,day_log,habit_log,done_log,poms_log,month_logs,year_logs}.zot
  - ~/org/2026/20260421*.zo
  - ~/org/2026/20260424*.zo
verification:
  - cargo run -q -p zorg-cli -- --help
  - cargo run -q -p zorg-cli -- watch --help
  - cargo run -q -p zorg-cli -- capture --help
  - cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus
  - cargo run -q -p zorg-cli -- query '#z/todo due:<=today -did:*' --root fixtures/corpus
  - cargo run -q -p zorg-cli -- query '#z/todo do:<=today -did:*' --root fixtures/corpus
  - cargo run -q -p zorg-cli -- query '#z/todo todo:[ ] -did:*' --root fixtures/corpus
  - cargo run -q -p zorg-cli -- query 'modified:<7d' --root fixtures/corpus
  - cargo run -q -p zorg-cli -- dash --once --no-color --panel today --root fixtures/corpus
---

# New Zorg Daily/Today Workflow Transition Research

## Scope

This note researches how to start using the Rust `zorg` implementation in this repo as the main second-brain driver,
with special attention to replacing the legacy daily/today workflow from the Python-era `zorg` repo at
`~/projects/github/bbugyi200/zorg/` and the legacy zettels under `~/org/202*/`.

The goal is not to port every old daily file. Existing migration research already recommends keeping most old day,
done, habit, and pomodoro logs as archive-only. The goal here is to preserve the active workflow shape: open today's
surface quickly, see due/do/open work, capture new tasks, record events and pomodoros, close work into a daily record,
and keep the index/query/dashboard layer trustworthy.

## What Legacy Zorg Did For The Daily Workflow

Legacy `zorg edit` was the main entry point. It accepted explicit `.zo` paths or file-group names, expanded file groups
like `@...`, initialized missing files from `.zot` templates, and launched Vim over the resulting file set.

Important legacy behaviors:

- The default subcommand was `edit`, so `zorg @today-like-group` could become "create/open today's files."
- `file_group_map` expanded dynamic path templates with today's date and the previous six days.
- `template_pattern_map` matched requested filenames such as `YYYY/YYYYMMDD.zo`, `YYYY/YYYYMMDD_day.zo`,
  `YYYY/YYYYMMDD_done.zo`, `YYYY/YYYYMMDD_habit.zo`, and `YYYY/YYYYMMDD_poms.zo` to Jinja `.zot` templates.
- The `.zot` renderer could compute `day_before` and `day_after`, then write rich day-navigation comments and default
  checklists.
- Day files used legacy note markers (`o`, `x`, `~`, `>`) plus `P0`, short IDs like `260421#02`, project tags such as
  `+project`, and links like `[[2026/20260420_habit]]`.
- Daily state was split across several files:
  - `YYYYMMDD.zo`: hub page linking day, done, habit, and poms sidecars.
  - `YYYYMMDD_day.zo`: today's plan, recurring review checklist, events, and link to poms.
  - `YYYYMMDD_done.zo`: completed/canceled daily work, often with carryover and audit context.
  - `YYYYMMDD_habit.zo`: habit and expense counters.
  - `YYYYMMDD_poms.zo`: planned/done pomodoro blocks with `p::`, `start::`, and `end::`.
- Saved `.zoq` query outputs were often materialized as readable pages, for example `todos.zoq`, `created_yest.zoq`,
  `modified_yest.zoq`, and `needs_attn.zoq`.

The practical effect was that one command could create a small dated workspace and open the right buffers. The cost was
that daily artifacts were highly mechanical and generated a lot of archive noise.

## What New Zorg Provides Instead

New `zorg` is intentionally not a compatibility layer for `.zo`, `.zoq`, or `.zot`. Normal source is `.z` under the
configured root, defaulting to `~/zorg`.

The replacement primitives are:

- File headers: `%%% @id #tags key::value ... %%%`.
- Nested zettel: list items with IDs, tags, todo markers, properties, and body text.
- IDs: `@absolute/id` and `^local-id`, with local IDs resolved under the nearest ancestor ID.
- Links: `#absolute/id`, `+child`, and `~sibling`.
- Type tags: `#z/todo`, `#z/ref`, `#z/inbox`, `#z/query`, `#z/tmpl`, `#z/dashboard`, and `#z/panel`.
- Lifecycle properties: `do::`, `due::`, and `did::`.
- Timebox properties: `p::`, `start::`, and `end::`.
- Todo markers: `[ ]`, `[N]`, `[X]`, and `[?]`.
- Stored queries and templates as ordinary zettel, not `.zoq` or `.zot` files.

The verified command surface, as printed by `zorg --help`, is broader than the old MVP notes imply:

- `zorg db reindex --root PATH` and `zorg db status --root PATH` manage the SQLite index.
- `zorg watch --root PATH [--debounce MS] [--once|--exit-after-ready|--exit-after-events N]` keeps the index live;
  events are debounced (default 250 ms) into incremental reindex passes and emit `starting/ready/indexing/indexed`
  states. The bounded flags exist for editor health checks; the unbounded form is the "leave running while editing"
  default.
- `zorg query '<swog>' --root PATH` and `zorg query --id @query/id --root PATH` run inline or saved queries.
- `zorg capture --template @id|TITLE [--title T] [--source T] [--body T] [--dest PATH] [--id @new-id] [--allow-outside]`
  creates zettel from `#z/tmpl` templates. Without `--template` in a TTY it prompts.
- `zorg path @id` and its alias `zorg open @id` print the indexed source location (`PATH:LINE:COL @id title`) and
  support `--json`. They are the editor-jump primitive a daily wrapper should use to resolve "today's file" once it
  has the canonical ID.
- `zorg promote @id [--to PATH] [--check|--write]` lifts a nested zettel into its own `.z` file.
- `zorg move @id --to PATH_OR_PARENT [--check|--write]` moves a zettel to a path or under another parent ID.
- `zorg extract --file PATH --range L:C-L:C --id @new/id [--to PATH]` extracts a body range into a new file zettel and
  replaces the selection with a link.
- `zorg fix [--check] [--json] FILE...` applies safe autofixes (bullet normalization, property whitespace, ID
  stamping, modified-date stamping, SORT-pragma sorting).
- `zorg check [--root PATH] FILE...` runs strict syntax and semantic validation.
- `zorg export markdown (--id|--subtree|--query|--query-id)` renders indexed `.z` zettels to Markdown.
- `zorg import legacy plan|apply PATH... [--root R] [--dest D]` is the only path that reads `.zo`/`.zoq`/`.zot`. Plan
  is read-only, apply writes only canonical `.z`. Apply refuses overwrites; there is no force/replace mode in v1.
- `zorg dash --root PATH [--panel today|inbox|queries|search|diagnostics|index] [--auto-refresh MS]` launches the TUI.
  Today combines due/do/open todo rows with diagnostics.

### Dashboard Keybindings (verified in `crates/zorg-dash/src/app.rs`)

Top-level keys, when no overlay is active:

| Key | Action |
| --- | --- |
| `q`, `Esc` | Quit |
| `?` | Help overlay |
| `j`/`k`, `↓`/`↑` | Move selection |
| `g`/`G` | Top / bottom |
| `PageDown`/`PageUp` | Page |
| `Ctrl-d`/`Ctrl-u` | Half-page |
| `Tab`/`BackTab`, `→`/`←` | Next/previous panel |
| `Enter` | Run selected query (Queries panel) or open source (other panels) |
| `o` | Open selected source in `$EDITOR` |
| `/` | Inline SWOG/query edit (search) |
| `:` | Diagnostic-filter edit |
| `r` | Refresh snapshot |
| `R` | Confirm reindex |
| `c` | Capture flow (template picker) |
| `f` | Fix preview |
| `d` | Mark selected todo done (`[X] did::today`) |
| `p` | Postpone prompt (rewrites `do::`/`due::`) |
| `s` | Schedule prompt (sets `do::YYYY-MM-DD`) |
| `t` | Cycle today-mode |
| `e` | Cycle diagnostic-severity filter |
| `a` | Clear diagnostic filters |
| `Space` | Toggle selected diagnostic mark |
| `y` | Yank overlay (copy a value) |
| `L` | Event-log overlay |

Overlays add their own keys: `y`/`Y` and `n`/`N` confirm/cancel reindex and fix-apply; capture and prompt overlays use
`Tab`/`Up`/`Down` to cycle fields, `Ctrl-u` to clear the current field, `Ctrl-w` to delete a word, and `Enter` to
submit. Capture from the dashboard refreshes the snapshot but does **not** start `zorg watch`; a separate watcher (or
manual reindex) is still needed for live indexing.

The built-in Today queries in `crates/zorg-dash/src/model.rs` are:

```swog
#z/todo due:<=today -did:*
#z/todo do:<=today -did:*
#z/todo todo:[ ] -did:*
```

That is the key conceptual replacement for legacy "today" files: today is a queryable slice of the graph, not only a
set of dated files.

## Recommended New Corpus Shape

Use `~/zorg` as the new active corpus and keep `~/org` read-only as legacy archive. Do not point new `zorg` directly at
`~/org`; the parser intentionally rejects normal `.zo`, `.zoq`, and `.zot` syntax.

Recommended starter layout:

```text
~/zorg/
  daily/
    2026/
      2026-05-04.z
  inbox.z
  system/
    queries.z
    templates.z
    dashboards.z
  areas/
    work.z
    personal.z
  projects/
    zorg.z
```

Use one canonical daily `.z` file per date at first. Avoid recreating five sidecar files until there is a clear need.
The old sidecars can become sections or child zettel under the daily file:

```z
%%% @daily/2026-05-04 #z/ref #journal day::2026-05-04
2026-05-04 Monday
%%%

- ^review #z/todo [ ] do::2026-05-04 area::personal/gtd Daily review.
  - [ ] Review calendar for today and tomorrow.
  - [ ] Review inbox.
  - [ ] Review yesterday and close carryover.

- ^events #z/ref title::Events

- ^poms #z/ref title::Pomodoros

- ^done #z/ref title::Done
```

This preserves the "one dated place for journaling" behavior while letting the dashboard find tasks anywhere in the
graph. If daily files become too large, promote `^poms`, `^done`, or substantial event notes into their own files with
`zorg promote`.

## Mapping Legacy Concepts To New Zorg

| Legacy pattern | New Zorg equivalent | Notes |
| --- | --- | --- |
| `~/org/YYYY/YYYYMMDD_day.zo` | `~/zorg/daily/YYYY/YYYY-MM-DD.z` file zettel | Start with one daily file instead of separate day/done/habit/poms files. |
| `YYYYMMDD.zo` daily hub | Usually unnecessary | Query and path structure replace most generated hub links. |
| `o P0 ...` open todo | `#z/todo [ ]` or `[N]` | Preserve priority as a property or tag only if it drives decisions. |
| `x ...` done todo | `#z/todo [X] did::YYYY-MM-DD` | Dashboard mark-done writes `[X] did::...`. |
| `~ ...` canceled/paused | `#z/todo [?]` plus `status::canceled` or `#status/canceled` | Pick one convention and make saved queries exclude it. |
| `tick::YYYY-MM-DD` | `do::YYYY-MM-DD` | Import may map old `tick::` to `modified::`; active reminders should use `do::`. |
| `due::YYYY-MM-DD` | `due::YYYY-MM-DD` | Same concept; dashboard Today includes due dates <= today. |
| `p::`, `start::`, `end::` pom blocks | Same properties on timebox zettel | New Zorg already treats these as canonical timebox properties. |
| `[[foo/bar]]` links | `#foo/bar`, `+child`, or `~sibling` | Legacy bracket links are import input, not active syntax. |
| `ID::foo` / `LID::bar` | `@foo` / `^bar` | Use slash IDs for hierarchy. |
| `.zoq` saved query files | `#z/query` zettel | Queries live in normal `.z` files. |
| `.zot` Jinja templates | `#z/tmpl` zettel | New templates support fixed variables, not arbitrary Jinja. |
| Generated query result pages | `zorg query`, `zorg dash`, or Markdown export | Avoid committing generated query pages unless there is a durable reason. |

## Template Strategy

New `zorg capture` templates are deliberately simpler than Jinja `.zot` files. The full variable set is `{{id}}`,
`{{title}}`, `{{date}}` (current UTC date as `YYYY-MM-DD`), `{{source}}`, and `{{body}}`. Literal braces escape as
`{{{{` and `}}}}`. Templates do not compute yesterday/tomorrow paths or arbitrary loops.

Template metadata properties recognized by `zorg capture`:

- `title::` — human-readable picker name and default title.
- `dest::` — destination path or directory under the corpus root; can be overridden with `--dest`.
- `tags::` — slash-separated default tags applied to the captured zettel.
- `source::` — default source field value.

Capture refuses silent overwrites: if `dest::` resolves to a directory, the template is appended into the directory's
`init.z` as a child zettel; if it resolves to a file, the template is appended as a child of the file zettel.

Use `#z/tmpl` for small captures:

````z
%%% @system/templates #z/ref
Capture templates
%%%

- @system/templates/todo #z/tmpl title::Todo dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] do::{{date}} source::{{source}} {{title}}
    {{body}}
  ```

- @system/templates/daily-note #z/tmpl title::Daily note dest::daily/inbox.z
  ```zorg-template
  - @{{id}} #z/ref day::{{date}} source::{{source}} {{title}}
    {{body}}
  ```
````

Do not try to express the whole old day-file scaffolding as a `zorg-template` block. A shell wrapper or small helper is
the better replacement for date arithmetic and multi-file creation. The helper can create today's daily file from a
static skeleton, then rely on `zorg capture` for individual notes and todos.

## SWOG Operators Worth Memorizing

The daily/today loop only needs a small slice of SWOG, but it's the slice you'll use every day:

- Date comparisons against properties: `due:<=today`, `do:<=today`, `due:<=2026-05-15`, `did:>=2026-05-01`. `today` is
  resolved from the local date in the query context.
- Relative modify-date ranges: `modified:<7d` (within the last 7 days), `modified:>=30d` (at least 30 days old).
  Useful for "what did I touch this week" and dormant-note queries.
- Todo markers: `todo:[ ]`, `todo:[N]`, `todo:[X]`, `todo:[?]`.
- Existence and negation: `due:*`, `-did:*`, `-#z/inbox`. `-did:*` is the canonical "not done" filter.
- Property comparisons: `p:>3`, `area:work/zorg` (segment match against slash-separated property values).
- Boolean combination: whitespace = AND, explicit `OR`, and grouped negation, e.g. `(#z/todo OR #z/query) -did:*`.
- Path filters: `file:projects/*.z` and `links:#foo/bar`.

A practical reading list for replacing legacy review pages:

```text
#z/todo due:<=today -did:*                       # due today or overdue
#z/todo do:<=today -did:*                        # scheduled today or earlier
#z/todo todo:[ ] -did:*                          # all open
#z/todo todo:[N] -did:*                          # next actions only
#z/inbox -did:*                                  # unprocessed capture
modified:<7d                                     # touched this week
modified:>=30d                                   # dormant notes
(#z/todo OR #z/inbox) -did:* -#area/work         # personal review surface
```

## Query And Dashboard Strategy

Create a small `@system/queries` file with query zettel that match the old daily review surfaces:

````z
%%% @system/queries #z/ref
Saved queries
%%%

- @system/queries/today #z/query title::Today
  ```swog
  (#z/todo due:<=today OR #z/todo do:<=today OR #z/todo todo:[ ]) -did:*
  ```

- @system/queries/inbox #z/query title::Inbox query::#z/inbox -did:*

- @system/queries/open #z/query title::Open todos query::#z/todo -did:*

- @system/queries/poms #z/query title::Timeboxes query::p:*
````

Then create a custom dashboard zettel if the built-in Today/Inbox/Search/Diagnostics panels are not enough:

````z
%%% @dashboards/daily #z/dashboard title::Daily
Daily dashboard
%%%

- @dashboards/daily/today #z/panel key::today title::Today query::@system/queries/today

- @dashboards/daily/inbox #z/panel key::inbox title::Inbox query::@system/queries/inbox

- @dashboards/daily/poms #z/panel key::poms title::Pomodoros query::@system/queries/poms
````

Launch forms:

```sh
zorg db reindex --root ~/zorg
zorg dash --root ~/zorg --panel today
zorg dash --root ~/zorg --as @dashboards/daily --panel today
zorg watch --root ~/zorg
```

Run `zorg watch` in another terminal or tmux pane during active editing. The dashboard can refresh snapshots, and `R`
can run a one-shot reindex, but watch mode is the smoother default.

## Selectively Migrating From `~/org`

Use `zorg import legacy` rather than hand-converting files. The contract:

- Inputs: `.zo`, `.zoq`, `.zot` files (paths or directories). `.zoc` cache files are rejected.
- Inline marker translation: `ID::value` → `@value`, `LID::value` → `^value` under the nearest imported parent,
  `tick::YYYY-MM-DD` → `modified::YYYY-MM-DD` (lossy: tick history collapses to one canonical property).
- Absolute legacy links `[[some/id]]` → `#some/id` when the target is a valid canonical ID; unknown forms stay as
  body text with a diagnostic.
- Plan vs apply: `plan` is read-only and prints text or JSON; `apply` writes only canonical `.z` after the same
  planning checks, refuses to overwrite existing files, and exits nonzero on fatal diagnostics.
- Destinations: default is `<root>/<normalized-id>.z`. `--dest imported` plans `imported/legacy/project.z` for
  `ID::legacy/project`, which is the safest way to keep imported content quarantined from new authoring.

Recommended migration shape:

```sh
# Dry run, scoped to a couple of projects you actually still touch:
zorg import legacy plan ~/org/projects/zorg.zo ~/org/projects/gtd.zo \
  --root ~/zorg --dest imported

# Apply once the plan is clean:
zorg import legacy apply ~/org/projects/zorg.zo ~/org/projects/gtd.zo \
  --root ~/zorg --dest imported
```

Do not bulk-import `~/org/2026/2026*.zo` daily files. They were generated by the legacy template machinery and are the
exact archive churn the new workflow is trying to retire. Cherry-pick only durable content (projects, areas, durable
reference notes, and any daily entries that turned out to be essays). Treat the rest of `~/org` as cold storage.

## Daily Startup Recommendation

Replace the old `zorg edit @today` muscle memory with a small wrapper, probably named something like `ztoday` or a shell
alias. It should:

1. Compute today's date in local time.
2. Ensure `~/zorg/daily/YYYY/YYYY-MM-DD.z` exists.
3. Insert a static daily skeleton if the file is new.
4. Run `zorg db reindex --root ~/zorg` or assume `zorg watch --root ~/zorg` is already running.
5. Open the daily file in `$EDITOR`. `zorg path @daily/YYYY-MM-DD --json` is the supported way to resolve the absolute
   path and 1-based line/column once the file is indexed; falling back to a direct path is fine for the create-on-miss
   case where the index hasn't seen the file yet.
6. Optionally launch `zorg dash --root ~/zorg --panel today` in another pane.

This wrapper is needed because new `zorg` does not currently have a direct equivalent to legacy `zorg edit` with
`file_group_map`, Jinja date math, and multi-file Vim launch.

The wrapper should not create old-style `*_done`, `*_habit`, and `*_poms` files by default. Use child zettel in the one
daily file until the data proves sidecars are worth the overhead. If a daily file later grows beyond comfort,
`zorg promote @daily/YYYY-MM-DD/poms` will lift the `^poms` zettel into its own `.z` file without breaking links.

## Suggested First-Week Operating Loop

1. Keep `~/org` untouched and create `~/zorg`.
2. Add `system/templates.z`, `system/queries.z`, and optionally `system/dashboards.z`.
3. Start `zorg watch --root ~/zorg` during writing sessions.
4. Use the wrapper to create/open today's daily file.
5. Capture loose tasks into `inbox.z` with `zorg capture --template @system/templates/todo ...`.
6. Use `zorg dash --root ~/zorg --panel today` as the daily control surface.
7. Mark done, postpone, or schedule from the dashboard when possible.
8. Move durable notes from daily files into project/area files only when they outgrow the daily context.
9. Query old `~/org` read-only when needed; migrate only active GTD buckets, reference notes, projects, and durable
   extracted daily entries.

## Things To Change In Your Habits

- Stop thinking of "today" as one generated file set. In new Zorg, "today" is all zettel with due/do/open state that
  query into the Today panel.
- Prefer `do::YYYY-MM-DD` for scheduled work and `due::YYYY-MM-DD` for deadlines.
- Prefer `[N]` for true next actions, but remember the built-in dashboard open-todo query currently targets `[ ]`;
  use saved queries if you want `[N]` semantics to drive the day.
- Use `did::YYYY-MM-DD` for completed items you want excluded from active queries.
- Keep daily files for narrative context, event capture, and lightweight journaling; keep projects and areas for durable
  knowledge.
- Use `#z/inbox` for unprocessed capture and query it, instead of relying on generated `.zoq` result files.
- Treat old habit and pomodoro logs as archive data. Reintroduce habit tracking only after the core daily/today loop is
  comfortable.

## Editor Integration

The Neovim plugin lives in a sibling repository, `../zorg-nvim`, not in this repo. `zorg-ls` deliberately delegates
prompt-style behavior (final-ID prompts, edit-preview UI) to the editor; `zorg.nvim` owns `.z` filetype, Tree-sitter
highlighting, LSP startup, and keymaps. The legacy Python `zorg` Vim plugin still owns `.zo`, so the two coexist
without conflict as long as you keep `~/org` and `~/zorg` separate.

`zorg-ls` features that are useful while editing daily files:

- Completion triggers on `#`, `+`, `~`, and `/`. That covers tag IDs, child links, sibling links, and slash-segment
  retrigger. **Property-key completion (`do::`, `due::`, `did::`) is explicitly deferred in the MVP**, so those keys
  are still typed by hand. A snippet plugin or editor abbreviation is a good bridge until property completion lands.
- Code actions expose safe refactors: `refactor.rewrite` (promote nested → file), `refactor.extract` (with
  `zorg.extract.preview`), unresolved-link typo fixes, and `zorg-fix` autofix actions (bullet normalization, property
  whitespace, ID stamping, `modified::` stamping, SORT-pragma sorting).
- `textDocument/didSave` triggers a conservative incremental reindex, so you do not strictly need `zorg watch` running
  if the editor is the only thing changing files. Run `zorg watch` when external tools (capture, scripts, sync) write
  alongside the editor.
- Rename is safe only when the canonical ID resolves to a single declaration with all references known and the new ID
  is collision-free; cross-file ID renames go through this code path.

## Version Control For `~/zorg`

Keep `~/zorg` under git. The repo's own `.gitignore` already excludes the SQLite store family (`*.sqlite`, `*.sqlite3`,
`*.db`, `*.db-shm`, `*.db-wal`) and the `.zorg-test/` directory used by the tooling; mirror that in `~/zorg/.gitignore`
so the index is rebuildable rather than committed. Commit `.z` sources, `system/templates.z`, `system/queries.z`,
`system/dashboards.z`, and any `.gitattributes` you add for `.z` highlighting. Avoid committing dashboard yank/log
output or any Markdown produced by `zorg export markdown` unless there is a durable reason — it is generated.

## Open Gaps And Follow-Up Work

- A first-class `zorg today` or `zorg edit` replacement does not exist. A wrapper is the fastest path; a native command
  could come later if the workflow stabilizes.
- New templates are intentionally not Jinja. Multi-file/day-before/day-after scaffolding needs either a wrapper or a
  future richer template feature.
- LSP property-key completion is deferred. `do::`, `due::`, and `did::` are typed by hand; consider editor snippets
  while waiting.
- Capture from the dashboard (`c`) refreshes the snapshot but does not start `zorg watch`. Run watch separately or
  rely on editor save-triggered reindex.
- Legacy priority syntax (`P0`, `P1`, etc.) has no canonical mapping in the new MVP. Decide whether priority should be a
  property (`priority::0`), a tag (`#priority/0`), or mostly retired in favor of `[N]`, `do::`, and `due::`.
- Legacy recurrence (`recur::...`, `tick::...`, `tock::...`) is not fully replaced. `tick::` is imported as
  `modified::` (lossy). For now, keep recurring/tickler sources in legacy archive or model next occurrences manually
  with `do::`.
- Habit tracking has no dedicated new design. Keep it outside the MVP daily loop unless it becomes important again.
- `zorg import legacy apply` has no force/replace mode. Re-imports require deleting the previous output by hand, so
  prefer `--dest imported` to keep a stable quarantine root.
- Work-confidentiality rules from the migration triage still apply before copying old Google/work material into
  `~/zorg`.

## Bottom Line

The smoothest transition is not a one-for-one port of daily files. It is:

- `~/zorg` as the active `.z` corpus.
- One daily file per date for context.
- Dashboard Today as the operational view.
- `#z/todo` plus `do::`, `due::`, and `did::` as the task lifecycle.
- `#z/query` and `#z/tmpl` zettel replacing `.zoq` and `.zot`.
- A small local wrapper replacing legacy `zorg edit` date/file orchestration.

That keeps the useful part of the old workflow while dropping most of the generated archive churn.

# Zorg Refactor Commands

Structural refactors are planned by the Rust `zorg-refactor` crate and exposed
through CLI commands with stable preview output for editor clients. Commands use
the existing SQLite index as their starting point, then reload and reparse source
files before planning writes. Run `zorg db reindex` after changing source files.

This is non-dashboard, Rust-authoritative refactoring support. Editor clients
should call the CLI or LSP surfaces described here and should not reproduce
source rewrite rules in Lua or another host language.

## Safety Model

Refactor commands use the SQLite index only to find candidate zettels and source
files. Before planning a write, `zorg-refactor` reads the source files from
disk, verifies that their content hash and byte length still match the index,
reparses them, and builds byte-span edits from the freshly parsed source.

Every edit must be in bounds, aligned to UTF-8 character boundaries, and
non-overlapping within a file. Write mode refuses plans with rejections, source
guard mismatches, destination collisions, paths outside the corpus root,
non-`.z` path destinations, and planned output that fails parsing or semantic
validation. Multi-file writes are prepared through temporary files before
renaming over guarded sources.

The commands preserve canonical IDs where possible. Existing link text is left
alone unless a planner can prove a rewrite is deterministic; unsafe relative
reference contexts are refused instead of guessed.

## Modes And Output

All structural write commands default to preview mode. Preview mode and
`--check` never write files. `--write` is required for filesystem changes.
After any successful write, refresh the store before graph-backed commands or
editor jumps:

```sh
zorg db reindex --root ~/zorg
```

`--json` and `--format json` produce the same schema-versioned preview envelope
for preview, check, and write modes:

```json
{
  "schema_version": 1,
  "plan": {
    "operation": "promote",
    "mode": "preview",
    "root": "/absolute/corpus/root",
    "target_id": "project/plan",
    "warnings": [],
    "rejections": [],
    "files": [
      {
        "absolute_path": "/absolute/corpus/root/project.z",
        "root_relative_path": "project.z",
        "original_guard": {
          "content_hash": "0000000000000000",
          "mtime_unix_ms": 1770000000000,
          "byte_len": 128
        },
        "edits": [
          {
            "span": {
              "start_byte": 0,
              "end_byte": 8,
              "start_line": 1,
              "start_column": 1,
              "end_line": 1,
              "end_column": 9
            },
            "replacement": "@project/plan",
            "label": "rewrite declaration"
          }
        ]
      }
    ]
  }
}
```

Exit code `0` means the requested preview, check, or write succeeded. Exit code
`1` means the refactor was refused or failed during planning/application, with a
human-readable error on stderr. Exit code `2` means CLI usage was invalid.

## Promote

`zorg promote @id` promotes a nested zettel into a file zettel while preserving
the canonical ID.

```sh
zorg promote @project/plan --root ~/zorg
zorg promote @project/plan --json --root ~/zorg
zorg promote @project/plan --write --root ~/zorg
zorg promote @project/plan --write --to plans/project-plan.z --root ~/zorg
```

The default mode is a dry-run preview. `--check` validates the same plan without
writing. `--write` is required to update files. JSON output wraps the shared
refactor preview envelope.

Without `--to`, the destination is derived from the canonical ID under the root:
`@foo/bar` becomes `foo/bar.z`. Explicit destinations may be relative to the
root or absolute, but they must remain under the corpus root and use `.z`.

Promotion refuses to write when the target is not nested, the destination already
exists, the index is stale, a planned source no longer matches its guard, or the
planned corpus fails parsing or semantic validation. The source zettel is
reparsed before planning, planned output is reparsed before writing, and written
files are parsed again during guarded application.

## Move

`zorg move @id --to PATH_OR_PARENT` moves a zettel while preserving its
canonical ID.

```sh
zorg move @project/plan --to archive/project-plan.z --root ~/zorg
zorg move @project/plan --to @archive --json --root ~/zorg
zorg move @project/plan --to archive/project-plan.z --write --root ~/zorg
```

The default mode is a dry-run preview. `--check`, `--write`, `--json`, and
`--format json` use the same refactor preview envelope as `promote`, with
`"operation": "move"`.

Path destinations move file zettels to another `.z` path, or convert nested
zettels into file zettels using the same conservative source conversion as
promotion. Parent destinations are written as `@parent/id` and move nested
zettels under that parent, preserving the moved nested block and reindenting it
for the new source context.

Move refuses destination collisions, path destinations outside the corpus root,
non-`.z` path destinations, directory zettel moves, file-to-parent moves, nested
moves under descendants, local or anonymous nested zettels moved to another
parent, stale indexes, stale source guards, and planned output that fails
parsing or semantic validation. If the target is already at the requested
destination, the plan is a deterministic no-op with a warning.

## Extract

`zorg extract` extracts an editor selection into a new file zettel and replaces
the selected source with an absolute link to the new ID.

```sh
zorg extract --file notes.z --range 12:1-14:1 --id @notes/extracted --root ~/zorg
zorg extract --file notes.z --byte-range 128..224 --id @notes/extracted --json --root ~/zorg
zorg extract --file notes.z --range 12:8-12:19 --id @notes/term --replace-with-link --write --root ~/zorg
```

Line and column ranges use one-based positions:
`START_LINE:START_COL-END_LINE:END_COL`. Byte ranges use zero-based
`START..END` offsets and must align with UTF-8 character boundaries. A
positional line/column range is also accepted after `extract` for editor
wrappers that prefer `zorg extract RANGE --file PATH --id @id`.

The default destination is derived from the requested canonical ID under the
root: `@foo/bar` becomes `foo/bar.z`. `--to PATH` may override the destination,
but the path must remain under the corpus root and use `.z`.

By default, the range must cover a paragraph-like body block; the replacement
preserves leading horizontal whitespace and a trailing line ending around the
generated `#foo/bar` link. Smaller selections inside a paragraph or fenced-code
body require `--replace-with-link`. Ranges must stay inside one paragraph or
fenced-code body and must not cross zettel boundaries, openings, child zettels,
or fence delimiters.

Extraction refuses invalid line/column or byte ranges, non-UTF-8 boundaries,
existing IDs, destination collisions, stale indexes, stale source guards, and
planned output that fails parsing or semantic validation. JSON output uses the
same refactor preview envelope as `promote` and `move`, with
`"operation": "extract"`, source replacement edits, destination file details,
and a create edit containing the generated zettel opening.

## LSP Code Actions

`zorg-ls` exposes selected safe refactors through standard code-action kinds:

- `refactor.rewrite` promotes a nested zettel when the shared promote planner
  can produce a complete preview plan. The action includes a `WorkspaceEdit`
  with `documentChanges`, including a destination `createFile` operation.
- `refactor.extract` validates paragraph-like selections through the shared
  extract selection checks and returns the `zorg.extract.preview` command with
  CLI-style arguments. Editor clients should replace the `@new/id` placeholder,
  show the normal preview, and only write after user confirmation.

The LSP server omits refactor actions for unsafe or incomplete contexts instead
of returning disabled actions. Rename and quickfix behavior remains separate:
rename uses the existing LSP rename planner, while quickfixes use
`zorg-fix::plan_fixes`.

## Epic 15 Editor Contract

Neovim integration should wrap these argv patterns:

```sh
zorg path @id --root ROOT --db DB --format json
zorg open @id --root ROOT --db DB --format json
zorg promote @id [--to PATH] --root ROOT --db DB --format json
zorg promote @id [--to PATH] --write --root ROOT --db DB --format json
zorg move @id --to PATH_OR_PARENT --root ROOT --db DB --format json
zorg move @id --to PATH_OR_PARENT --write --root ROOT --db DB --format json
zorg extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id --root ROOT --db DB --format json
zorg extract --file PATH --byte-range START..END --id @new/id --write --root ROOT --db DB --format json
```

Editor clients should always show preview JSON before invoking `--write`.
Clients may render `plan.files[].edits[]` as a confirmation diff, then rerun the
same command with `--write` after confirmation. A successful write should be
followed by `zorg db reindex` or an already-running `zorg watch` refresh before
depending on `zorg path`, query results, or graph-backed LSP features.

For LSP extraction, `zorg-ls` returns the command `zorg.extract.preview` with
CLI-style arguments and the placeholder ID `@new/id`. The Neovim wrapper owns
prompting for the final ID, preview display, and write confirmation; Rust owns
selection validation and source rewriting.

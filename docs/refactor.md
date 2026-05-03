# Zorg Refactor Commands

Structural refactors are planned by the Rust `zorg-refactor` crate and exposed
through CLI commands with stable preview output for editor clients. Commands use
the existing SQLite index as their starting point, then reload and reparse source
files before planning writes. Run `zorg db reindex` after changing source files.

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
refactor preview envelope:

```json
{
  "schema_version": 1,
  "plan": {
    "operation": "promote",
    "mode": "preview",
    "target_id": "project/plan",
    "files": []
  }
}
```

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

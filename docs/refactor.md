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

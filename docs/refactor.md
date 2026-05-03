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

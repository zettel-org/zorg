# Zorg v1 Fix And Check Contract

`zorg fix` and strict check mode keep `.z` files consistent without inventing
new semantics. The command may normalize safe syntax and report diagnostics, but
it must not silently migrate legacy formats.

## Strict Check Mode

Strict check mode validates source files and exits nonzero on diagnostics that
make the corpus unsafe to index or rewrite.

Strict checks should include:

- Malformed file headers.
- Duplicate canonical IDs.
- Unresolved absolute, child-relative, sibling-relative, or local references.
- Invalid property syntax or invalid known typed property values.
- Unsupported legacy-looking syntax.
- Query/template zettel definition errors.

## Allowed Autofixes

Autofixes must be deterministic, source-span-backed, and idempotent. MVP-safe
fixes include:

- Normalize list bullet symbols when this does not change nesting.
- Normalize surrounding whitespace for `key::value` properties.
- Add or update generated modified-date metadata only after that metadata key is
  explicitly specified by a later implementation.
- Sort only regions guarded by an explicit SORT pragma once that pragma is
  specified.
- Rewrite IDs and links only through LSP/CLI operations that have passed rename
  safety checks.

If the command cannot prove a rewrite is safe, it should emit a diagnostic and
leave the file unchanged.

## Idempotency

Running `zorg fix` twice on the same corpus must produce no additional changes
on the second run. Tests should compare the second run against the first fixed
output for every fixture that exercises autofix behavior.

## Legacy-Looking Input

Strict mode should diagnose, not migrate, these forms:

- `.zo`, `.zoq`, `.zot`, and `.zoc` source/cache formats.
- `ID::`, `LID::`, and old folgezettel IDs.
- `tick::` lifecycle dates.
- Python-era link behavior.
- `@@@` code fences.
- Tag sugar.

Old notes will be migrated outside Zorg v1. The v1 formatter must not become a
compatibility layer.

## Deferred Formatting

Deferred beyond this contract:

- Full document pretty-printing.
- Markdown export.
- Manual table formatting.
- Embedded-note expansion.
- Query result rewriting.
- Capture template reformatting beyond generated output validation.

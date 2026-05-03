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

Both `zorg check` and `zorg fix --check` accept either explicit `FILE...`
arguments or `--root PATH`. With `--root PATH` the strict pass discovers every
canonical `.z` source under `PATH`, parses it once, and runs corpus-level
validation and resolution so cross-file references are resolved against every
indexed source. Diagnostics are printed in the stable
`path:line:column: code: message` shape.

## Allowed Autofixes

Autofixes must be deterministic, source-span-backed, and idempotent. MVP-safe
fixes include:

- Normalize list bullet symbols when this does not change nesting.
- Normalize surrounding whitespace for `key::value` properties.
- Stamp a missing absolute ID when the zettel has an unambiguous canonical-name
  source. File and directory zettel may be stamped from their source path stem
  (`note.z` -> `@note`, `dir/init.z` -> `@dir`). Nested zettel with a resolved
  local ID may be stamped by replacing `^local` with its canonical absolute ID.
- Add or update generated modified-date metadata using the
  `modified::YYYY-MM-DD` property when `zorg fix` applies another content edit
  to the same zettel. The date value is the UTC calendar date of the fix run.
- Sort only regions guarded by the explicit SORT pragma: `zorg-sort:start`
  followed later by `zorg-sort:end`.
- Rewrite IDs and links only through LSP/CLI operations that have passed rename
  safety checks.
- Rewrite unresolved absolute links through LSP code actions only when the
  current graph has exactly one canonical ID at one ASCII edit distance from
  the source link target.

If the command cannot prove a rewrite is safe, it should emit a diagnostic and
leave the file unchanged.

SORT pragma regions are sorted line-by-line by trimmed UTF-8 text. A region is
eligible only when every nonblank line is either a plain line or a flat bullet
line with the same indentation. Mixed bullet/plain regions, nested bullet
regions, fenced code, and already sorted regions are left unchanged.

Malformed SORT pragmas are strict diagnostics. This includes misspelled pragma
lines containing `zorg-sort`, nested starts, unmatched ends, and an unterminated
start. The pragma marker lines themselves are never moved.

## Idempotency

Running `zorg fix` twice on the same corpus must produce no additional changes
on the second run. Tests should compare the second run against the first fixed
output for every fixture that exercises autofix behavior.

## Shared Fix Plan Model

`zorg-fix` exposes the planner that both the CLI and the LSP consume:

- `FixPlan` — deterministic, source-span-backed list of `FixOp`s for one
  parsed document.
- `FixOp` — one logical autofix (rule kind, severity, stable rule code,
  preferred flag, message) carrying one or more `FixEdit`s.
- `FixEdit` — single source-span replacement, never crossing zettel
  boundaries.
- `FixKind` — stable enum of rule identifiers; Phase 7.1 ships
  `UnresolvedAbsoluteLinkTypo`; later phases add the source token, stamping,
  modified-date, and SORT-pragma variants behind the same public enum.
- `plan_fixes(document, corpus_view)` — the only entry point downstream
  surfaces should call. Adding a new rule means adding a `FixKind` variant and
  a planner branch; CLI output and LSP code actions pick it up automatically.

The planner is idempotent: planning the same document with the same
`CorpusView` twice returns equal `FixPlan` values.

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
- Auto-priority rewriting. It is deliberately deferred because v1 does not yet
  have a conflict-free rule for choosing `[N]` among sibling tasks without
  changing author intent.

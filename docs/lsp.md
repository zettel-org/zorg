# Zorg v1 LSP Contract

`zorg-ls` exposes the indexed Zorg graph to editors without making Neovim or
any other editor the source of truth. The server delegates parsing and semantic
validation to the Rust model and Tree-sitter parser.

## Launch

Run the language server over stdio from this workspace or from an installed
binary:

```sh
cargo run -p zorg-ls
```

`zorg-ls --help` and `zorg-ls --version` are ordinary CLI paths. They print to
stdout and exit without starting an LSP session.

## Workspace Root

The default root is `~/zorg`. LSP initialization may override the root from
client configuration or workspace folders. If multiple roots are supplied, the
MVP server selects a single root deterministically: explicit initialization
options first, then the first workspace folder, then `rootUri`/`rootPath`, then
`~/zorg`. If multiple workspace folders are supplied, the server logs that it
selected one root and ignored the rest.

Supported `initializationOptions` fields:

- `rootPath` or `root`: absolute or client-relative path to the Zorg workspace.
- `databasePath` or `dbPath`: SQLite database path. If omitted, the server uses
  the store default under the selected root, currently `<root>/.zorg/zorg.sqlite3`.
- `trace` or `logLevel`: optional text value logged during initialization for
  simple client-side tracing.

Only `.z` files are canonical source. Directory zettel are `init.z`.

`zorg-ls` opens the configured store on initialize and records whether the
snapshot is ready or degraded. Missing databases, stale indexes, and unreadable
roots do not crash the server. Live open-buffer diagnostics still work, while
graph-backed features such as navigation, completion, rename, and code actions
return no result until the index is ready. The server advertises save
notifications and refreshes the store snapshot after `textDocument/didSave`, so
a stale or missing database can recover without restarting the LSP session when
the configured root is readable.

Build or refresh the index before starting editor sessions that need graph
features:

```sh
cargo run -p zorg-cli -- db reindex --root ~/zorg
```

## MVP Features

The LSP MVP should support:

- Initialize/shutdown and text document sync.
- Diagnostics for syntax/model failures.
- Completion for IDs and links after `#`, `+`, and `~`.
- Go to definition for ID and link references.
- Find references for canonical zettel IDs.
- Rename for zettel IDs when all affected declarations and links can be safely
  rewritten.
- Code actions for safe ID/link fixes where the fix is deterministic.

Formatting, query result virtual documents, advanced workspace commands, and
query-driven completion are deferred.

## Implemented Capabilities

The protocol foundation advertises full text document sync with open/change/
close/save notifications. Diagnostics are published for indexed store findings
after initialization, after save-triggered refreshes, and for live open buffers
after open/change notifications. Definition, references, document symbols,
workspace symbols, completion, and code actions are available when the indexed
graph snapshot can be loaded. Prepare-rename and rename are advertised with
prepare support and are also graph-backed.

## Save Refresh

`textDocument/didSave` is the conservative live-indexing trigger for `zorg-ls`.
The server does not host a filesystem watcher. On save it opens the configured
store, runs the incremental `Store::reindex()` path, reloads the LSP graph
snapshot, logs whether the store is ready or degraded, and republishes
diagnostics for known indexed files plus any open buffers. If the configured
database path does not exist, the save refresh creates it through the normal
store open path before indexing. If the root is missing or not a directory, the
refresh remains degraded and graph-backed features continue returning empty
results instead of panicking.

Running `zorg-ls` with no arguments starts the server over stdio. `zorg-ls
--help` and `zorg-ls --version` remain regular CLI paths and do not start an
LSP session.

## Diagnostics

LSP diagnostics should mirror model diagnostics and carry accurate line/column
ranges:

- Duplicate IDs.
- Unresolved links and local IDs.
- Invalid property syntax or typed values.
- Unsupported legacy-looking syntax.
- Malformed query/template definitions when a zettel is tagged `#z/query` or
  `#z/tmpl`.

Live open-buffer diagnostics are produced by parsing and validating the
in-memory document text. Indexed diagnostics are read from the SQLite snapshot,
including source file paths and one-based stored spans converted to LSP
zero-based ranges. When an open document has both live and indexed diagnostics,
live syntax and single-document validation diagnostics are published first and
identical indexed diagnostics are deduplicated. Closing a document republishes
the indexed diagnostics for that URI, or an empty array when the snapshot has no
diagnostics for it.

Legacy syntax must not be silently translated into v1 model data.

## Completion

The MVP completion provider advertises `#`, `+`, `~`, and `/` as trigger
characters. It reads candidates from the loaded graph snapshot and uses the
current open document text only to identify the token range being completed.
When the store snapshot is missing, degraded, or lacks source-backed graph data,
completion returns an empty list.

Completion behavior:

- `#` offers source-backed canonical zettel IDs as absolute links, plus known
  type tags and explicit/effective corpus tags. Link and tag items use distinct
  details and stable sort text.
- `+` offers direct child IDs relative to the containing canonical zettel.
- `~` offers sibling IDs using the containing canonical ID path.
- `/` retriggers completion while a slash-separated ID or tag path is being
  typed; query-driven completion and property-key completion are deferred.

Completion items include labels, insert text, kind, detail, text edits for the
current token, and deterministic ordering. Relative completions are scoped to
the source-backed zettel containing the request position; if no containing
zettel can be found, the server returns no relative suggestions.

## Source Spans

Every completion, definition, reference, rename, diagnostic, and code action
must be anchored to model source spans. If a parsed item lacks a reliable span,
the LSP should decline the feature for that item rather than rewrite unrelated
text.

## Rename Safety

Rename is safe only when:

- The declaration resolves to exactly one canonical zettel ID.
- Every reference to rewrite is known and source-backed.
- The new ID is syntactically valid.
- The new ID does not collide with another canonical ID.
- Relative links can either remain valid or be rewritten deterministically.

If any condition fails, return a clear error and make no edits.

The MVP rename planner accepts source-backed zettel declarations and resolved
link occurrences. Absolute declarations are rewritten as `@new/id`, and
absolute links are rewritten as `#new/id`. Local declarations may be renamed
only within the same absolute ancestor, where the declaration can remain a
local `^id`. Child-relative, sibling-relative, and local references are kept in
relative form only when the new target remains a direct child of the same
current or parent canonical ID; otherwise the rename is rejected instead of
guessing a broader rewrite.

## Code Actions

The code-action provider advertises quick fixes plus safe refactor kinds:
`quickfix`, `refactor.rewrite`, and `refactor.extract`. Quick fixes always
include an exact `WorkspaceEdit`. Refactor actions use `zorg-refactor` planners
or validation helpers as the source of truth instead of reimplementing rewrite
rules in the server.

Supported quick fixes:

- An unresolved absolute link such as `#poject/plan` may be rewritten when the
  loaded graph contains exactly one source-backed canonical ID that differs by
  one ASCII insertion, deletion, or substitution. The replacement preserves the
  absolute link form, for example `#project/plan`.
- Source-token autofixes from the shared `zorg-fix` planner are exposed when
  their edits intersect the requested range, including bullet-symbol
  normalization, property whitespace normalization, ID stamping,
  modified-date stamping, and SORT-pragma sorting.

The CLI and LSP both consume `zorg-fix::plan_fixes`, so new safe fix rules
should be added once in `zorg-fix` rather than reimplemented in the server.

Supported refactors:

- A nested zettel declaration may be promoted with a `refactor.rewrite` action
  when `zorg-refactor::plan_promote` can produce a complete preview plan. The
  action returns a `WorkspaceEdit` with `documentChanges`, including `createFile`
  for the destination zettel and text edits for the source removal and new file
  contents.
- A paragraph-like body selection may return a `refactor.extract` command action
  after the shared extract selection validator accepts the range. The command is
  `zorg.extract.preview` and carries CLI-style arguments for `zorg extract`; the
  editor integration must replace the `@new/id` placeholder and run the normal
  CLI preview/write confirmation flow. LSP does not invent new IDs.

Intentionally unavailable actions return an empty result instead of a disabled
or speculative edit:

- Ambiguous unresolved links where more than one canonical ID matches the typo
  rule.
- Child-relative, sibling-relative, and local-reference unresolved links.
- Legacy migration diagnostics, including `ID::`, `LID::`, `tick::`, old cache
  formats, or Python-era link behavior.
- Refactor requests on top-level zettels, openings, selections that cross
  structural boundaries, or extract ranges that are not paragraph-like.
- Requests made while the store is missing, stale, or unable to produce a
  source-backed graph snapshot.

## Boundaries

`zorg-ls` should not own parser semantics, query evaluation, capture writes, or
format rules. It should use the same Rust crates and fixture contracts as CLI
commands. Capture is available to editor integrations through
`zorg capture --json`; an LSP `workspace/executeCommand` wrapper is deferred so
the server does not grow editor-specific prompt or process-management behavior.
Editor-specific defaults belong in `zorg-nvim`, not in the server.

## Troubleshooting

If the client shows no completions, definitions, references, renames, or code
actions, check the server log for a degraded store warning. Common causes are:

- The configured root is not a readable directory.
- The SQLite database does not exist yet.
- Source files were added, changed, or deleted after the last reindex.

Save a `.z` document in the configured root to trigger the conservative LSP
refresh path, or refresh the index with `cargo run -p zorg-cli -- db reindex
--root <root>` for batch workflows. If the server remains degraded after save,
inspect the `zorg-ls store refresh degraded: ...` log message for the root,
database, or permission problem. A separate `zorg watch` process may keep the
SQLite index current while files change, but `zorg-ls` still reloads its graph
snapshot through initialization and save-triggered refreshes.

Tests create temporary indexes as needed; no committed SQLite database under
`fixtures/corpus/.zorg` is required.

## Verification

Run this local command from the repository root to verify the LSP MVP:

```sh
cargo test -p zorg-ls
```

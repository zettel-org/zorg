# Zorg v1 LSP Contract

`zorg-ls` exposes the indexed Zorg graph to editors without making Neovim or
any other editor the source of truth. The server delegates parsing and semantic
validation to the Rust model and Tree-sitter parser.

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
snapshot is ready or degraded. A missing, stale, or unreadable store must not
crash the server; later features should check the recorded status and decline
graph-backed behavior when the snapshot is unavailable.

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
close notifications. Diagnostics are published for indexed store findings after
initialization and for live open buffers after open/change notifications.
Definition, references, document symbols, workspace symbols, and completion are
available when the indexed graph snapshot can be loaded. Prepare-rename and
rename are advertised with prepare support and are also graph-backed.

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

## Boundaries

`zorg-ls` should not own parser semantics, query evaluation, capture writes, or
format rules. It should use the same Rust crates and fixture contracts as CLI
commands. Editor-specific defaults belong in `zorg-nvim`, not in the server.

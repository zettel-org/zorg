# Zorg v1 LSP Contract

`zorg-ls` exposes the indexed Zorg graph to editors without making Neovim or
any other editor the source of truth. The server delegates parsing and semantic
validation to the Rust model and Tree-sitter parser.

## Workspace Root

The default root is `~/zorg`. LSP initialization may override the root from
client configuration or workspace folders. If multiple roots are supplied, the
server should either index each root independently or report unsupported
multi-root behavior clearly.

Only `.z` files are canonical source. Directory zettel are `init.z`.

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

## Diagnostics

LSP diagnostics should mirror model diagnostics and carry accurate line/column
ranges:

- Duplicate IDs.
- Unresolved links and local IDs.
- Invalid property syntax or typed values.
- Unsupported legacy-looking syntax.
- Malformed query/template definitions when a zettel is tagged `#z/query` or
  `#z/tmpl`.

Legacy syntax must not be silently translated into v1 model data.

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

## Boundaries

`zorg-ls` should not own parser semantics, query evaluation, capture writes, or
format rules. It should use the same Rust crates and fixture contracts as CLI
commands. Editor-specific defaults belong in `zorg-nvim`, not in the server.

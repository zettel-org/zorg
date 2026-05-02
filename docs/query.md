# Zorg v1 Query Contract

Zorg v1 supports a SWOG LIST query MVP. The query engine reads the indexed
zettel graph and returns ordered zettel results. TABLE output, aggregation,
custom functions, saved dot-snippets, and alternate renderers are deferred.

## Query Location

Queries can run from the CLI as an inline SWOG string or from ordinary zettel
tagged `#z/query`.

Query zettel definitions may use:

- A `query::` property for short definitions.
- A fenced `swog` block in the zettel body for longer definitions.

If both exist, v1 consumers should report an ambiguous query definition rather
than guessing.

## Result Shape

Output format is LIST only. Each row represents one matching zettel and should
include enough identity to navigate back to source: canonical ID when present,
file path, title or first body line, and todo marker when present.

Default ordering should be deterministic:

1. Explicit query order if the query includes one.
2. Due/do date where relevant.
3. Source path.
4. Source order within the file.

## Supported Filters

The MVP supports these filter families:

- Property equality: `foo:bar`.
- Property comparison: `p:>3`, `due:<=2026-05-15`.
- Property existence: `foo:*`.
- Tag filters: `#z/todo`, `#area/work`.
- Link filters: `links:#foo/bar` or an equivalent documented operator.
- File glob filters: `file:projects/*.z`.
- Todo status and priority: `todo:[ ]`, `todo:[N]`, `todo:[X]`, `todo:[?]`.
- Negation: `-#z/inbox`, `-did:*`.
- Text search: quoted text or an explicit `text:` filter.
- Relative modify-date ranges: `modified:<7d`, `modified:>=30d`, or an
  equivalent documented range form.

Whitespace between filters means logical AND. OR groups, nested expressions, and
custom functions are deferred unless later specs explicitly add them.

## Query Zettel Execution

Running a query by ID resolves the target zettel, verifies it has `#z/query`,
extracts its query definition, evaluates against the configured corpus root,
and renders LIST output.

Query zettel are part of the same corpus as every other note. They can have
ordinary IDs, tags, properties, links, children, and source spans. `.zoq` files
are not v1 input.

## Error Handling

Query parser errors should identify the query source span when the query lives
inside a `.z` file and byte/column position when supplied inline. Unknown fields
are allowed as property filters unless their syntax is malformed.

Unsupported output modes such as TABLE should produce clear unsupported-feature
errors, not partial output.

## Deferred Query Features

Deferred beyond v1:

- TABLE output.
- Aggregation and `count()`.
- Custom functions.
- Saved query dot-snippets.
- Embedded query pragmas.
- Manual tables as source syntax.
- Query-driven completion hints.

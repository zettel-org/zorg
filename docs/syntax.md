# Zorg v1 Syntax

This document is the shared syntax contract for Zorg `.z` files. Tree-sitter,
Rust parsing/model code, fixtures, CLI commands, LSP behavior, and editor
integrations should treat this file as the source of truth for Epic 1.

## File Format

- Canonical extension: `.z`.
- Default corpus root: `~/zorg`.
- Directory zettel file name: `init.z`.
- Text encoding: UTF-8.
- Line endings: readers should accept LF and CRLF; writers should produce LF.
- Code blocks: ordinary Markdown fences using triple backticks.

Legacy Python-era formats are not v1 syntax. Implementations must not read or
write `.zo`, `.zoq`, `.zot`, generated `.zoc`, `ID::`, `LID::`, `tick::`,
custom `@@@` code fences, old folgezettel IDs, tag sugar, or Python-era link
behavior as compatible data. Strict checks may diagnose legacy-looking text as
invalid input.

## Zettel Blocks

Every first-class object is a zettel: a file, a directory `init.z`, a nested
note, a query, or a template.

File and directory zettel headers use a percent fence:

```z
%%% @project #z/ref area::work/research
Title text may follow the header line.
%%%
```

The opening line may contain an ID, tags, properties, and plain title words.
The closing line is exactly `%%%` with optional surrounding whitespace.
Content after the closing line belongs to the file or directory zettel.

Nested zettel are line-oriented blocks that begin with a list marker and may
carry the same syntax primitives:

```z
- @project/next #z/todo [N] due::2026-05-15 Write the next implementation note.
  Paragraph content may continue under the nested zettel.
```

Grammar work may choose the exact parse nodes for list nesting, but the semantic
model must preserve parent-child structure and source spans.

## IDs

Zettel IDs are declared with `@`:

```z
@alpha
@alpha/beta
```

An absolute ID is a slash-separated path of non-empty segments. For v1, each
segment should contain ASCII letters, digits, `_`, or `-`, and should begin with
an ASCII letter or digit.

Local IDs are declared with `^`:

```z
- ^meeting-notes Notes for this child zettel.
```

A local ID resolves beneath the nearest ancestor zettel that has an absolute ID.
For example, `^meeting-notes` under `@alpha` resolves to `@alpha/meeting-notes`.
If no ancestor has an absolute ID, strict checks should report an unresolved
local ID.

## Links

Zorg v1 supports three zettel link forms:

- Absolute: `#foo/bar` resolves to `@foo/bar`.
- Child-relative: `+child` resolves under the current zettel ID.
- Sibling-relative: `~sibling` resolves under the current zettel parent's ID.

Links are references, not declarations. Link resolution, duplicate detection,
and unresolved-link diagnostics belong in semantic validation.

## Tags

Tags begin with `#` and use slash-separated segments:

```z
#project
#area/work
#z/todo
```

Type tags use the reserved `#z/...` namespace. Common v1 type tags include
`#z/todo`, `#z/ref`, `#z/inbox`, `#z/query`, and `#z/tmpl`.

Tag inheritance is semantic, not syntactic. Child zettel inherit tags from
ancestor zettel and directory/file ancestry as described in `docs/model.md`.
There is no v1 tag-sugar expansion.

## Properties

Properties use `key::value` with no required space after `::`:

```z
status::active
due::2026-05-15
area::work/research
```

Property keys should contain ASCII letters, digits, `_`, or `-`, and should
begin with an ASCII letter. Values are raw trimmed strings in syntax. Semantic
layers may assign types for known keys.

Slash-separated lists are the only v1 list property form:

```z
area::work/research/writing
```

Required lifecycle and timebox property names:

- Lifecycle: `do::`, `due::`, `did::`.
- Timebox: `p::`, `start::`, `end::`.

`tick::` is legacy-looking input and must not be accepted as a v1 alias.

## Todos

Todo markers are zettel/task markers:

- `[ ]` open
- `[N]` priority or next
- `[X]` done
- `[?]` in progress or unknown

Todo state should attach to the containing zettel. Implementations should not
create a separate task model that is disconnected from zettel identity, tags,
properties, and links.

## Query Zettel

Queries are ordinary zettel tagged `#z/query`. A query definition may be stored
in a `query::` property or in the zettel body as a fenced `swog` block:

```z
- @queries/today #z/query query::due:<=today
```

````z
- @queries/inbox #z/query Inbox query
  ```swog
  #z/inbox -did:*
  ```
````

LIST is the only v1 query output format. Query details are in `docs/query.md`.

## Template Zettel

Templates are ordinary zettel tagged `#z/tmpl`:

````z
- @templates/todo #z/tmpl title::Todo Capture
  ```zorg-template
  - @{{id}} #z/todo [ ] do::{{date}} {{title}}
  ```
````

`.zot` files are not a v1 template format. Capture details are in
`docs/capture.md`.

## Deferred Syntax

The following are intentionally out of v1 syntax: generated cache files,
separate query/template file types, custom code-block syntax, folgezettel IDs,
tag aliases, drawers, description lists, manual tables, embedded-note syntax,
named URL shorthand, and table/aggregate query output.

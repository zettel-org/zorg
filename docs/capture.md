# Zorg v1 Capture Contract

`zorg capture` creates new zettel from ordinary template zettel tagged
`#z/tmpl`. Capture supports scripted editor/global-key workflows, JSON output
for integrations, and a minimal TTY prompt flow for direct CLI use.

## Template Discovery

Templates are normal `.z` zettel in the configured corpus root. A template
candidate must have the `#z/tmpl` type tag.

Template content may be supplied in:

- A fenced `zorg-template` block.
- The body of the template zettel when no template fence exists.

`.zot` files are not v1 templates.

## Template Metadata

Useful template properties include:

- `title::` human-readable template name.
- `dest::` destination path or directory under the corpus root.
- `tags::` slash-separated default tags or domain-specific metadata.
- `source::` optional source-file field name or static source label.

V1 template expansion is case-sensitive and recognizes these variables:

- `{{id}}`: captured zettel ID without the leading `@`. It comes from
  `--id`; when omitted, Zorg generates a unique slug from `--title` or the
  template `title::` value.
- `{{title}}`: captured title text from `--title`, falling back to the template
  `title::` value and then to an empty string.
- `{{date}}`: current UTC calendar date in `YYYY-MM-DD` form.
- `{{source}}`: source text or URL from `--source`, falling back to template
  `source::` and then to an empty string.
- `{{body}}`: body text from `--body`, or an empty string when omitted.

Unknown variables and unclosed `{{` pairs are errors. Literal braces are
written as `{{{{` for `{{` and `}}}}` for `}}`.

## Destinations

Capture destinations must stay under the configured Zorg root unless the user
explicitly requests and confirms another path. Directory destinations should use
`init.z` when creating a directory zettel and `.z` for ordinary files.

If a destination would overwrite existing content, capture should append a child
zettel or fail clearly according to the selected template mode. Silent overwrite
is not allowed.

## Source File Recording

Capture should be able to record the source file or URL that initiated capture.
The canonical MVP form is a `source::` property on the created zettel when a
source is provided.

## Noninteractive Flags

The CLI supports noninteractive execution for editor integrations:

- `--template @id|TITLE` selects a template by canonical ID or exact
  `title::` value.
- `--title TEXT`, `--source TEXT`, and `--body TEXT` provide template
  variables.
- `--dest PATH` overrides template `dest::`.
- `--id @new-id` chooses the created zettel ID.
- `--root PATH` and `--db PATH` mirror the rest of the CLI.
- `--allow-outside` permits a destination outside the configured root.
- `--json` or `--format json` prints machine-readable output.

Default success output remains:

```text
destination: /abs/path/inbox.z
zettel_id: @tasks/new
```

JSON success output is:

```json
{ "destination": "/abs/path/inbox.z", "zettel_id": "@tasks/new" }
```

JSON failure output is:

```json
{ "error": "message", "code": "capture.failed" }
```

## Interactive Flow

When `--template` is omitted and both stdin and stdout are TTYs, `zorg capture`
discovers `#z/tmpl` zettel under the configured root and prompts for a template
from a deterministic list sorted by ID, title, and path. If the selected
template references `{{title}}`, `{{source}}`, or `{{body}}` and the value was
not supplied by a flag, the CLI prompts for that value.

The command never prompts in non-TTY mode. Missing noninteractive inputs fail
with a clear error such as `missing inputs: --template`.

## Mode Matrix

| Mode | Template missing | Variable missing | Output |
| --- | --- | --- | --- |
| TTY text | prompt | prompt | text |
| TTY JSON | prompt | prompt | JSON |
| non-TTY text | fail | use documented fallback | text error |
| non-TTY JSON | fail | use documented fallback | JSON error |

## Deferred Behavior

Deferred beyond the MVP contract:

- External migration tooling for old template files.
- Cross-root capture.
- Template inheritance.
- Prompt scripting languages.
- Promotion/move workflows.

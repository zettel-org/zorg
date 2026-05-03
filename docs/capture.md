# Zorg v1 Capture Contract

`zorg capture` creates new zettel from ordinary template zettel tagged
`#z/tmpl`. Capture is designed for noninteractive editor/global-key workflows
as well as explicit CLI use.

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

The CLI should support noninteractive execution for editor integrations:

- Select template by ID or name.
- Provide destination override.
- Provide title/body variables.
- Provide source file/URL.
- Print the created file path and zettel ID when available.

Interactive template selection and rich prompts are useful but not required for
the first implementation.

## Deferred Behavior

Deferred beyond the MVP contract:

- External migration tooling for old template files.
- Cross-root capture.
- Template inheritance.
- Prompt scripting languages.
- Automatic ID generation policy beyond documented placeholders.
- Promotion/move workflows.

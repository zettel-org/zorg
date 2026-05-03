# Zorg Import And Export Bridge Contract

Zorg v1 uses canonical `.z` files for normal parsing, indexing, query,
capture, refactor, LSP, and editor workflows. Import and export bridges are
explicit conversion tools at the boundary of that corpus. They do not expand
the accepted v1 source language.

Legacy Python-era syntax is import input only. Markdown is export output only.
Neither format is read by the store, Tree-sitter grammar, LSP, or normal
`zorg parse` / `zorg check` flows.

## Legacy Import Inputs

The legacy bridge accepts explicit user-supplied paths with these extensions:

- `.zo`: legacy note files.
- `.zoq`: legacy saved query files.
- `.zot`: legacy template files.

The bridge may recognize these inline legacy markers inside accepted legacy
files:

- `ID::value`: absolute zettel ID, rendered as `@value` after normalization.
- `LID::value`: local zettel ID, rendered as `^value` under the nearest
  imported parent with an absolute ID.
- `tick::YYYY-MM-DD`: legacy modification tick, rendered as
  `modified::YYYY-MM-DD` and reported as lossy because tick history is reduced
  to one canonical property.

Legacy absolute links written as `[[some/id]]` map to Zorg absolute links
`#some/id` when the target is a valid canonical ID. Unknown or malformed link
forms are preserved as body text with a diagnostic instead of guessed.

## Unsupported Inputs

The bridge must reject generated/cache `.zoc` files as import sources. It must
also report unsupported diagnostics for custom legacy constructs that have no
deterministic `.z` mapping, including generated state, Python-only query or
template code, old cache metadata, ambiguous folgezettel IDs, custom tag sugar,
and custom `@@@` fences whose language or body cannot be represented as an
ordinary Markdown fence.

Unsupported input does not become canonical source. Normal `.z` parsing and
indexing must continue to reject `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`,
`LID::`, and `tick::`.

## Planning And Apply

Import has two phases:

- Planning reads legacy inputs, builds deterministic canonical `.z` output in
  memory, computes diagnostics, and prints human or JSON output. It never
  writes source files, reindexes a store, or mutates hidden state.
- Apply/write mode must be explicit. It writes only canonical `.z` files after
  the same planning checks pass. It never writes legacy files, generated
  `.zoc`, sidecar databases, or hidden compatibility metadata.

Planned output uses LF line endings, UTF-8, normalized Zorg IDs, and ordinary
v1 syntax. Implementations must parse and strict-check generated `.z` before
reporting a writeable plan without fatal diagnostics.

The read-only CLI surface is:

```bash
zorg import legacy plan PATH... [--root ROOT] [--dest DEST] [--json|--format json]
```

`PATH` may name one or more files or directories. Directory traversal is sorted
by stable display path. `--root` enables existing-output collision checks, and
`--dest` prefixes the planned root-relative output path, for example
`--dest imported` plans `imported/legacy/project.z` for
`ID::legacy/project`.

`plan` is the only supported import subcommand in this phase. It exits `0`
when planning completes without fatal diagnostics, `1` when readable inputs
produce fatal plan diagnostics, and `2` for CLI usage errors.

The explicit write CLI surface is:

```bash
zorg import legacy apply PATH... [--root ROOT] [--dest DEST] [--json|--format json]
```

`apply` runs the same planner first, refuses fatal diagnostics, and then writes
only the planned canonical `.z` files under `--root`. When `--root` is omitted,
the current directory is the destination root. Parent directories are created as
needed. Existing destination files are always refused; this version does not
provide a force or replace mode. JSON apply output keeps the plan envelope with
`command: "import legacy apply"` and `mode: "apply"`, plus `write_results`
entries that list the exact paths written or failed.

## Destinations And Collisions

The import planner derives an output path from the normalized ID unless the
caller supplies a destination policy. The default path is:

```text
<dest-or-root>/<normalized-id>.z
```

For example, `ID::legacy/project` plans `legacy/project.z`.

Collision detection is part of planning. The bridge reports fatal diagnostics
for:

- duplicate legacy inputs that normalize to the same canonical ID;
- duplicate planned output paths;
- planned output paths that already exist, unless a documented replace/force
  mode is explicitly selected;
- generated `.z` that fails parsing or strict validation.

Default write behavior refuses overwrites. There is no replace/force option in
this version. A future replace/force option must state whether it replaces
complete files only or can merge zettel blocks; silent merge is not allowed.

## Diagnostics

Human output should be stable and compact: input path, planned output path,
status, and diagnostic summaries.

JSON output should use a versioned envelope. Future implementations may add
fields, but the following shape is the compatibility floor:

```json
{
  "schema_version": 1,
  "command": "import legacy plan",
  "mode": "plan",
  "inputs": [
    {
      "path": "fixtures/import_export/legacy/notes/project.zo",
      "kind": "legacy_note"
    }
  ],
  "outputs": [
    {
      "input_path": "fixtures/import_export/legacy/notes/project.zo",
      "root_relative_path": "legacy/project.z",
      "canonical_id": "legacy/project",
      "status": "planned",
      "lossiness": ["tick_history_collapsed"]
    }
  ],
  "diagnostics": [
    {
      "severity": "warning",
      "kind": "lossy",
      "code": "legacy.tick_history_collapsed",
      "path": "fixtures/import_export/legacy/notes/project.zo",
      "line": 5,
      "message": "tick:: was converted to modified::; historical tick state is not preserved"
    }
  ],
  "collisions": [],
  "summary": {
    "planned": 1,
    "lossy": 1,
    "unsupported": 0,
    "fatal": 0
  }
}
```

Diagnostic severities are:

- `info`: deterministic conversion note.
- `warning`: lossy but writeable conversion.
- `error`: unsupported input or collision that prevents writing affected
  output.

Diagnostic kinds are:

- `lossy`: deterministic conversion with reduced legacy semantics.
- `unsupported`: no deterministic `.z` mapping.
- `collision`: duplicate ID, duplicate output path, or overwrite risk.
- `invalid_output`: generated `.z` failed parser or strict validation.
- `io`: unreadable input or destination error.

## `.zo` Note Mapping

A legacy note file maps to one canonical file zettel. `ID::` becomes the file
zettel ID, legacy tag fields become ordinary `#` tags, `key::value` fields with
v1 property keys are preserved, and body text follows the header.

Nested legacy sections with `LID::` become nested zettel with local IDs. Todo
markers map only when they match v1 states: open to `[ ]`, next to `[N]`, done
to `[X]`, and unknown/in-progress to `[?]`.

## `.zoq` Query Mapping

A legacy saved query maps to an ordinary zettel tagged `#z/query`.

Short query text maps to a `query::` property. Multi-line query text maps to a
fenced `swog` block. If both forms are present or the legacy query uses
unsupported Python-only behavior, the planner reports an unsupported diagnostic
instead of guessing.

## `.zot` Template Mapping

A legacy template maps to an ordinary zettel tagged `#z/tmpl` when its template
body can be represented with the v1 template language. The template body is
rendered as a fenced `zorg-template` block. Supported variables are the v1
capture variables documented in `docs/capture.md`.

Templates that depend on Python code, hidden state, or unknown variables are
unsupported unless the implementation can preserve the body as inert text with
an explicit lossy diagnostic.

## Markdown Export

Markdown export renders canonical `.z` zettel for reading or external
publishing. It does not create importable Markdown.

The exporter should support a single zettel, a subtree, or a query result set.
Input ordering is deterministic: explicit command order, query result order, or
source order within the selected subtree.

Mapping rules:

- Each rendered item has an `ExportPlan` entry with schema version, target
  selector, rendered Markdown, diagnostics, and summary counts.
- IDs render as Markdown headings and root item front matter `id` values.
- Tags render in front matter as slash-preserving strings.
- Properties render in front matter as key/value pairs.
- Todo markers render in headings or list items using the original `[ ]`,
  `[N]`, `[X]`, and `[?]` text.
- Body text is copied as Markdown after Zorg metadata is removed.
- Ordinary Markdown code fences are preserved.
- Zorg links render as Markdown links whose target uses a `zorg:` URL and whose
  visible text preserves the original link text when the target is part of the
  export set, for example `[#legacy/query/open](zorg:#legacy/query/open)`.
- Links outside the export set and unresolved relative links are preserved as
  their original text and reported as lossy export diagnostics. The exporter does
  not silently create broken Markdown links.
- Nested child zettels render as nested headings. Local IDs and todo markers stay
  visible in the child heading, while child properties render as a compact
  property list.

Markdown output should be stable for golden tests. It may be lossy when Zorg
metadata has no Markdown equivalent, but the exporter should not silently drop
IDs, tags, properties, todos, body text, code fences, or Zorg links.

## Fixtures

Bridge fixtures live under `fixtures/import_export`. Legacy inputs, expected
canonical `.z` outputs, expected Markdown outputs, and expected JSON plans are
tracked separately from the canonical parser/store corpus in
`fixtures/manifest.json`.

The normal fixture corpus remains `fixtures/corpus/**/*.z`. Adding `.zo`,
`.zoq`, `.zot`, `.zoc`, or legacy inline markers under `fixtures/corpus` is a
policy violation unless the file is an explicitly negative `.z` fixture such as
`fixtures/corpus/legacy_invalid.z`.

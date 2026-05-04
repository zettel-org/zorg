# Zorg Quickstart

This guide takes a fresh source checkout to an installed `zorg` CLI, an
installed `zorg-ls` language server, and a small working `.z` corpus under the
default `~/zorg` root.

## Prerequisites

- A Rust toolchain that supports Rust 2024. This workspace declares
  `rust-version = "1.85"`.
- A checkout of this repository.
- For source builds, the sibling `../zorg-treesitter` checkout with a generated
  parser at `../zorg-treesitter/src/parser.c`.

If the generated parser is missing or stale, refresh it before building Zorg:

```bash
(cd ../zorg-treesitter && npm install && npm run generate)
```

## Install From Source

From the repository root, first build the workspace as a sanity check:

```bash
cargo build --workspace
```

Install the user-facing binaries from their local crate paths:

```bash
cargo install --path crates/zorg-cli
cargo install --path crates/zorg-ls
```

Verify that both commands are on your `PATH`:

```bash
zorg --version
zorg-ls --version
```

## Create A First Corpus

Zorg's default corpus root is `~/zorg`, and canonical source files use the
`.z` extension.

```bash
mkdir -p ~/zorg
cat > ~/zorg/start.z <<'EOF'
%%% @start #z/ref area::personal/quickstart
Quickstart home
%%%

This first file zettel links to #start/today and #start/templates/todo.

- @start/today #z/todo [N] due::2026-05-05 Write the first Zorg note.
  Keep the note small and link related work with absolute links like #start/queries/open.

- @start/queries/open #z/query title::Open todos query::#z/todo -did:*

- @start/templates/todo #z/tmpl title::Todo capture dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] do::{{date}} source::{{source}} {{title}}
    {{body}}
  ```
EOF
```

This file contains a file zettel, a todo zettel, a saved query zettel, and a
template zettel using the v1 `.z` syntax. Legacy `.zo`, `.zoq`, `.zot`, and
`.zoc` files are migration inputs only, not normal v1 source files.

## Validate And Index

Run strict validation over the corpus:

```bash
zorg check --root ~/zorg
```

Inspect the resolved root and database path:

```bash
zorg db status --root ~/zorg
```

By default, the database lives at:

```text
~/zorg/.zorg/zorg.sqlite3
```

Build the index:

```bash
zorg db reindex --root ~/zorg
```

## Start Using Zorg

Run an inline SWOG query against the current index:

```bash
zorg query '#z/todo -did:*' --root ~/zorg
```

Run the saved query from `start.z`:

```bash
zorg query --id @start/queries/open --root ~/zorg
```

Resolve an indexed ID back to its source location:

```bash
zorg path @start/today --root ~/zorg
```

Keep the index current while editing in another terminal:

```bash
zorg watch --root ~/zorg
```

After indexing, launch the terminal dashboard:

```bash
zorg dash --root ~/zorg
```

Optionally create a new todo from the template:

```bash
zorg capture \
  --root ~/zorg \
  --template @start/templates/todo \
  --id @inbox/first-capture \
  --title "First captured note" \
  --source "quickstart" \
  --body "Write the next action."
```

Run `zorg db reindex --root ~/zorg` again after capture if you are not also
running `zorg watch`.

## Editor Integration

Install `zorg-ls` with the source install command above, then point your
editor's LSP configuration at the `zorg-ls` binary.

Refresh the index before starting editor sessions that need graph-backed
features such as completions, definitions, references, rename, and code
actions:

```bash
zorg db reindex --root ~/zorg
```

Editor clients may configure the selected corpus root and database path. The
LSP defaults to `~/zorg` and `<root>/.zorg/zorg.sqlite3` when no explicit
configuration is supplied.

## Configuration And Next Reading

Most store-aware commands accept `--root PATH` and `--db PATH`. Zorg resolves
paths from CLI flags first, then environment variables such as `ZORG_ROOT` and
`ZORG_DATABASE_PATH`, then config files, then defaults. Use
`zorg db status --root ~/zorg` to inspect the final `root:` and `database:`
values.

Read next:

- [Syntax](syntax.md) for the `.z` source contract.
- [Query](query.md) for SWOG LIST, TABLE, `count()`, and saved query zettels.
- [Capture](capture.md) for `#z/tmpl` templates and `zorg capture`.
- [LSP](lsp.md) for `zorg-ls` setup and editor feature boundaries.
- [Import and export](import_export.md) for migrating legacy files and exporting
  Markdown.
- [Development](development.md) for workspace layout, validation commands, and
  config-file details.

---
research_date: 2026-05-03
bead_id: zorg-1
title: Zorg v1 MVP legend review
---

# Zorg v1 MVP Legend Review

## Research Basis

This review is based on:

- `sase bead show zorg-1`.
- The `zorg-1` bead ledger in `sdd/beads/issues.jsonl`.
- The legend and epic plans under `sdd/legends/202605/` and `sdd/epics/202605/`.
- The current sibling repositories:
  - `../zorg`: Rust core, CLI, SQLite store, query engine, fix/capture, and LSP.
  - `../zorg-treesitter`: Tree-sitter grammar and editor query files.
  - `../zorg-nvim`: Neovim integration.
- Light smoke checks in `../zorg`:
  - `cargo run -p zorg-cli -- --help`.
  - `cargo run -p zorg-cli -- db reindex --root fixtures/corpus`.
  - `cargo run -p zorg-cli -- db status --root fixtures/corpus`.
  - `cargo run -p zorg-cli -- query '#z/todo -did:*' --root fixtures/corpus`.
  - `cargo run -p zorg-ls -- --version`.

`sase bead show zorg-1` reports the legend bead itself as open, but all nine child epics are closed:

1. Foundations.
2. Tree-sitter grammar and highlighting.
3. Rust parse and semantic model.
4. SQLite store and incremental indexing.
5. SWOG query MVP.
6. LSP MVP.
7. Capture and fix.
8. Neovim integration.
9. Documentation, release, and cross-repo validation.

## High-Level Product Summary

Zorg is a plaintext zettelkasten system built around one reusable primitive: the zettel. A zettel can be a whole `.z` file, a directory `init.z`, a nested note inside another file, a todo, a reference, a saved query, or a capture template. The value of that choice is that notes, tasks, links, metadata, queries, and editor features all operate on the same graph instead of separate note/task/project systems.

The v1 MVP now has the main pieces needed for practical use:

- `.z` syntax with a Tree-sitter grammar and editor queries.
- A Rust semantic model for IDs, local IDs, links, tags, properties, todos, source spans, and diagnostics.
- SQLite indexing under a corpus root, defaulting to `~/zorg`.
- Incremental `zorg db reindex` and `zorg db status`.
- SWOG LIST queries through `zorg query`.
- Stored query zettel tagged `#z/query`.
- Strict `zorg check`.
- Safe `zorg fix` planning and in-place autofixes.
- `zorg capture` from ordinary template zettel tagged `#z/tmpl`.
- `zorg-ls` for diagnostics, completion, navigation, references, safe rename, and code actions.
- `zorg.nvim` for `.z` filetype detection, Tree-sitter query loading, LSP startup, CLI commands, optional mappings, and health checks.

The fast teaching angle is: "Write normal plaintext `.z` notes, add IDs and tags only where they help, index the folder, then use queries and editor features to recover structure."

## Why It Is Valuable

Zorg is valuable because it keeps authoring cheap while making the corpus computable.

- Plaintext stays durable. The source of truth is ordinary UTF-8 `.z` files, not a database or editor-private format.
- The same object model serves notes, todos, templates, and queries. Users do not have to decide up front whether something is a note, task, project, or saved search.
- SQLite indexing makes a folder of notes fast to query without giving up file ownership.
- Source spans make editor actions safer. Diagnostics, rename, references, and autofixes are grounded in exact file locations.
- Tree-sitter and LSP keep the system editor-agnostic. Neovim is supported, but the core value lives in Rust CLI/LSP and grammar repos.
- The explicit no-legacy policy keeps the MVP small. Old Python-era `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, and `tick::` syntax is intentionally not compatibility input.

## The 15-Minute Mental Model

Teach these concepts first:

1. A zettel is the only primitive.
   A file header declares a file zettel:

   ```z
   %%% @project #z/ref area::work/zorg
   Project notes
   %%%
   ```

2. Nested bullets can also be zettel:

   ```z
   - @project/plan #z/todo [N] due::2026-05-15 Plan the next milestone.
     - ^task #z/todo [ ] do::2026-05-02 Write implementation notes.
   ```

3. IDs and links form the graph:
   - `@project/plan` declares an ID.
   - `^task` declares a local ID under the nearest ancestor, resolving to `@project/plan/task`.
   - `#project/plan` links to an absolute ID.
   - `+task` links to a child.
   - `~review` links to a sibling.

4. Tags and properties make the graph queryable:
   - Tags: `#z/todo`, `#z/ref`, `#area/work`.
   - Properties: `due::2026-05-15`, `area::work/zorg`, `p::3`.
   - Todos: `[ ]`, `[N]`, `[X]`, `[?]`.

5. Query and template definitions are just zettel:

   ````z
   - @system/queries/today #z/query title::Today query query::due:<=today

   - @system/templates/todo #z/tmpl title::Todo capture dest::inbox.z
     ```zorg-template
     - @{{id}} #z/todo [ ] do::{{date}} source::{{source}} {{title}}
       {{body}}
     ```
   ````

## Fast Usage Path

Use this path to teach a new user quickly.

1. Create or choose a corpus root. The default is `~/zorg`.

   ```sh
   mkdir -p ~/zorg
   ```

2. Add `.z` files. Use `init.z` for a directory zettel and ordinary `name.z` files for notes.

3. Validate a file or corpus.

   ```sh
   zorg parse ~/zorg/inbox.z
   zorg check --root ~/zorg
   ```

4. Build the index after changing notes.

   ```sh
   zorg db reindex --root ~/zorg
   zorg db status --root ~/zorg
   ```

5. Query the graph.

   ```sh
   zorg query '#z/todo -did:*' --root ~/zorg
   zorg query 'due:<=today -did:*' --root ~/zorg
   zorg query --id @system/queries/today --root ~/zorg
   ```

6. Fix safe issues.

   ```sh
   zorg fix --check --root ~/zorg
   zorg fix --root ~/zorg
   ```

7. Capture from templates.

   ```sh
   zorg capture --template @system/templates/todo --title "Follow up" --root ~/zorg
   zorg capture --template @system/templates/todo --title "Follow up" --json --root ~/zorg
   ```

8. Start editor support.
   - Build or install `zorg` and `zorg-ls`.
   - Build/install the `zorg` Tree-sitter parser from `../zorg-treesitter`.
   - Install `zorg.nvim`.
   - Add:

   ```lua
   require("zorg").setup({
     root = "~/zorg",
   })
   ```

9. In Neovim, use:
   - `:ZorgIndex`
   - `:ZorgStatus`
   - `:ZorgQuery #z/todo -did:*`
   - `:ZorgFix %`
   - `:ZorgCapture --template @system/templates/todo --title "Follow up"`
   - `:checkhealth zorg`

## What Each Epic Delivered

| Epic | Result |
| --- | --- |
| 1. Foundations | Established the v1 syntax/model/query/LSP/capture/fix docs, canonical fixtures, Rust workspace crates, Tree-sitter skeleton, Neovim skeleton, and cross-repo handoff contracts. |
| 2. Tree-sitter | Built the `.z` grammar for file headers, nested zettel, IDs, local IDs, links, tags, properties, todos, paragraphs, blank lines, and Markdown fences. Added highlight, fold, locals, and injection queries. |
| 3. Parse/model | Connected Rust parsing to the grammar, lowered parse trees into typed semantic documents, added strict validation, and resolved absolute, local, child, and sibling links. |
| 4. Store/index | Added SQLite schema/migrations, corpus discovery, `zorg db reindex`, `zorg db status`, full and incremental indexing, deletion handling, graph persistence, and tag/path ancestry materialization. |
| 5. Query | Added SWOG parser, normalization, store-backed evaluation, deterministic LIST rendering, inline CLI queries, and stored `#z/query` execution by ID. |
| 6. LSP | Replaced the placeholder server with `zorg-ls` over stdio, including initialization, diagnostics, graph navigation, symbols, completion, safe rename, and quick fixes. |
| 7. Capture/fix | Added strict check, shared fix plans, safe in-place autofixes, ID/modified stamping, SORT pragma behavior, noninteractive capture, interactive capture, JSON output, and editor-facing polish. |
| 8. Neovim | Added real `.z` filetype behavior, Tree-sitter runtime queries, `zorg-ls` setup, CLI-backed `:Zorg*` commands, optional mappings/helpers, health checks, README/help docs, and tests. |
| 9. Docs/release/validation | Added fixture synchronization contracts, Rust end-to-end harness work, documented validation flow, contributor docs, and release dry-run/checklist work. |

## Things New Users Should Learn Early

- Run `zorg db reindex` after changing files. Query and graph-backed LSP features depend on a current SQLite index.
- `zorg index` is not the command; use `zorg db reindex`.
- Query output is LIST only in v1. TABLE output, aggregation, OR, and parenthesized query groups are deliberately deferred.
- `zorg-ls` can still publish live diagnostics when the store is missing or stale, but navigation/completion/rename/code actions need a ready index.
- Neovim does not own Zorg semantics. It shells out to `zorg`, starts `zorg-ls`, registers filetype/parser behavior, and displays results.
- Legacy Python-era syntax is not accepted as a migration layer. Old notes need external migration before becoming v1 Zorg source.
- `.z` is the only accepted source extension for the MVP.

## Observed Smoke Results

The current Rust CLI help reports these available command groups:

- `parse FILE`
- `check`
- `db status`
- `db reindex`
- `query`
- `fix`
- `capture`

The fixture corpus reindexed successfully with:

- 8 discovered files.
- 8 indexed files.
- 25 indexed zettel.
- 13 diagnostics in the fixture set.

The sample active-todo query returned LIST rows including:

- `@project/plan/task` from `nested.z`.
- `@query-fixture/inbox` from `query_focus.z`.
- `@project/plan` from `nested.z`.
- `@project/review` from `nested.z`.

`cargo run -p zorg-ls -- --version` printed `zorg-ls 0.1.0`.

## Documentation Gaps And Caveats

- The parent legend bead `zorg-1` is still open even though all child epics are closed. If the work is considered complete, the bead state should be reconciled.
- The `../zorg` README status paragraph still says capture and broader fix behavior remain later implementation phases, but Epic 7 is closed and the CLI now exposes `fix` and `capture`. That paragraph should be refreshed.
- `../zorg-treesitter` and `../zorg-nvim` mention running `../zorg/tools/validate_cross_repo.sh`, but `../zorg/tools/validate_cross_repo.sh` is not present in the current `../zorg/tools` directory. Only `tools/zorg_sibling_commit_stop_hook` exists there.
- I did not run the full workspace, Tree-sitter, or Neovim test suites during this review. The verification above was limited to lightweight command smoke checks.

## Recommended Teaching Order

For a new user, avoid starting with architecture. Teach in this order:

1. Write one `.z` file with a percent-fenced `@id` header.
2. Add one nested `#z/todo` zettel with a due date.
3. Run `zorg check`, then `zorg db reindex`.
4. Run `zorg query '#z/todo -did:*'`.
5. Add one `#z/query` zettel and run it by ID.
6. Add one `#z/tmpl` zettel and run `zorg capture`.
7. Open the folder in Neovim and show diagnostics, completion, go-to-definition, and `:ZorgQuery`.
8. Explain the implementation only after the loop is visible: Tree-sitter parses, Rust models and indexes, SQLite stores, SWOG queries, LSP edits safely, Neovim delegates.

That path gets a new user from "plain files" to "queryable, navigable knowledge graph" quickly, which is the core reason Zorg is worth using.

---
research_date: 2026-05-03
bead_id: zorg-1
title: Zorg next feature recommendations after v1 MVP
source_research:
  - ../zorg/research/202605/v1_mvp_curation.md
  - sdd/research/zorg_1_legend_review.md
  - sdd/legends/202605/zorg_v1_mvp.md
---

# Zorg Next Feature Recommendations After v1 MVP

## Scope

This research reviews the recent `zorg-1` legend work, the legend review note, and the original MVP curation file that
inspired the Rust v1 implementation. `sase bead show zorg-1` reports the parent legend as open, but all nine child epics
are closed:

1. Foundations.
2. Tree-sitter grammar and highlighting.
3. Rust parse and semantic model.
4. SQLite store and incremental indexing.
5. SWOG query MVP.
6. LSP MVP.
7. Capture and fix.
8. Neovim integration.
9. Documentation, release, and cross-repo validation.

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory layout used by generated SDD docs.

## Current Product Baseline

The v1 work successfully built the spine described in the curation research:

- `.z` files parse through Tree-sitter and lower into a Rust semantic model.
- Files, directory `init.z` files, nested notes, todos, refs, queries, and templates share the same zettel primitive.
- IDs, local IDs, absolute links, child links, sibling links, tags, properties, todos, source spans, and diagnostics are
  represented in typed Rust structures.
- SQLite indexing persists files, zettel, IDs, links, tags, effective tags, properties, todos, text payloads, diagnostics,
  and metadata.
- `zorg query` supports SWOG LIST queries and stored `#z/query` zettel.
- `zorg-ls` exposes diagnostics, completion, navigation, references, symbols, rename, and code actions.
- `zorg capture` and `zorg fix` cover the first write-side workflow.
- `zorg.nvim` delegates semantics to the CLI/LSP and provides filetype, Tree-sitter, commands, LSP startup, mappings, and
  health checks.

The practical MVP loop is now: write `.z` notes, run check/fix/capture, reindex, query, and use editor graph features.

## Observed Gaps That Matter Most

The gaps below are not defects in the MVP. They are the highest leverage places to turn a coherent first implementation
into a daily system:

- The indexed graph is manually refreshed. `zorg query` expects a current SQLite index, and graph-backed LSP features
  degrade when the store is missing or stale.
- Query output is intentionally LIST-only. TABLE output, aggregation, OR, parenthesized grouping, custom functions, and
  saved dot-snippets are deferred.
- `text_index` exists, but full-text search is not enabled. Text filters therefore do not yet behave like a mature search
  layer.
- The write-side surface can create and safely fix notes, but it cannot yet move, promote, extract, copy/open, or otherwise
  refactor zettel structure.
- The original curation calls `zorg dash` high-value for v1.1, and daily workflows still require users to compose CLI
  queries manually.
- Cross-repo validation is documented, but a referenced `tools/validate_cross_repo.sh` is not present in `../zorg`.
- The `../zorg` README status paragraph still says capture and broader fix behavior remain later phases, even though Epic
  7 is closed and the commands exist.

## Recommended Next Features

### 1. Live Workspace Indexing

Build a live indexing path that keeps the SQLite graph current without making users remember `zorg db reindex`.

Recommended shape:

- Add `zorg watch` or `zorg serve` as a long-running process over one root and database.
- Use file notifications to trigger debounced incremental reindex.
- Let `zorg-ls` request or host the same refresh path so editor graph features recover after saves.
- Teach `zorg.nvim` to surface index state and start the watcher when configured.
- Keep `zorg db reindex` as the deterministic batch command for scripts and CI.

Why this is highest impact:

- It removes the largest day-to-day friction in the current product.
- It makes completion, go-to-definition, references, rename, code actions, and query results feel trustworthy while
  writing.
- It reuses the existing incremental store architecture instead of inventing new semantics.

Acceptance bar:

- Saving a `.z` file updates `db status` and query results without manual reindex.
- `zorg-ls` transitions from degraded to ready after the watcher refreshes the store.
- Deleted and renamed files are reflected correctly.
- Watch mode has deterministic shutdown, logging, and test coverage for debounce behavior.

### 2. Search And Query v1.1

Upgrade SWOG from a useful filter language into a practical retrieval layer.

Recommended shape:

- Enable SQLite FTS for title/body/raw text and route text filters through it.
- Add `--json` or `--format json` for query results so editor/TUI integrations do not scrape LIST output.
- Add OR and parenthesized grouping before TABLE output. These unlock common saved-query use cases without requiring an
  analytics renderer.
- Add TABLE and `count()` only after the expression model is stable.
- Keep query definitions as ordinary `#z/query` zettel.

Why this is high impact:

- Queries are the main payoff for indexing a plaintext corpus.
- Better text search makes Zorg useful even before a user has carefully tagged every note.
- Structured query output becomes the foundation for dashboards, virtual documents, and Neovim result buffers.

Acceptance bar:

- Existing LIST queries remain stable.
- Text filters use FTS when available and fall back clearly when not.
- `zorg query --json` returns source paths, IDs, titles, todo markers, source spans, and matched metadata.
- Unsupported query forms continue to fail clearly until implemented.

### 3. Zettel Refactoring Commands

Add safe structural editing commands that exercise the "everything is a zettel" model after creation.

Recommended shape:

- `zorg move @id --to PATH_OR_PARENT` to move a zettel while preserving references.
- `zorg promote @id` to turn a nested zettel into a file zettel.
- `zorg extract RANGE --id @id` for turning selected text into a child or sibling zettel.
- `zorg open @id` or `zorg path @id` for scripts and editor integrations.
- Share the same source-span and rewrite safety rules as LSP rename.

Why this is high impact:

- Capture creates new material, but refactoring is what keeps a large note graph healthy.
- Promotion was called strategically important in the original curation because it proves the subnote-to-note-to-page
  continuum.
- This turns Zorg from an indexer into a structural authoring tool.

Acceptance bar:

- Commands reject ambiguous, missing-span, or collision-prone edits.
- Links and IDs are rewritten only when every affected source span is known.
- Neovim can call the commands without reimplementing rewrite logic in Lua.
- Golden tests cover nested-to-file promotion, move across directories, and rejected unsafe rewrites.

### 4. Daily Dashboard

Build the deferred `zorg dash` around query zettel and the existing store.

Recommended shape:

- Start with a terminal dashboard, not a plugin-only UI.
- Show inbox, due/overdue, next actions, recently modified notes, unresolved diagnostics, and saved query panels.
- Allow basic actions: open path, mark done, run capture, run fix, refresh index.
- Back every panel with a normal SWOG query or built-in query definition.
- Let `zorg.nvim` call the dashboard or expose equivalent query result buffers later.

Why this is high impact:

- It gives users a default daily entry point instead of making them memorize queries.
- It demonstrates the value of unified zettel types: todos, refs, inbox notes, queries, diagnostics, and templates can
  appear in one operational view.
- It was already identified as a high-value v1.1 candidate in the original curation.

Acceptance bar:

- Dashboard starts quickly against an existing index and reports stale/missing index state clearly.
- Default panels are useful on the fixture corpus and a real root.
- Actions call existing safe commands instead of mutating files directly.
- The dashboard remains optional; CLI, LSP, and Neovim keep working independently.

### 5. Import And Export Bridges

Add one-way bridges that make Zorg easier to adopt and easier to leave.

Recommended shape:

- `zorg import legacy` or `zorg migrate plan` for old `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, and `tick::` material,
  producing a reviewable plan before writing `.z`.
- `zorg export markdown` for selected zettel, query results, or a subtree.
- Preserve the v1 rule that canonical source is `.z` and legacy forms are not accepted as normal input.
- Prefer explicit, auditable transforms over silent compatibility.

Why this is high impact:

- The original MVP intentionally avoided migration compatibility to keep v1 small, but adoption still depends on getting
  real notes into the new format.
- Export lowers risk for users who want plaintext durability and publication options.
- A plan-first importer fits the existing strict-check and safe-fix philosophy.

Acceptance bar:

- Import planning reports every unsupported or lossy transform.
- Writing imported output creates `.z` files only and never leaves hidden compatibility state.
- Export can render a single zettel, a subtree, and a query result set to Markdown.
- Import/export tests use small fixtures and do not expand the core parser into a legacy compatibility layer.

## Recommended Build Order

1. Live Workspace Indexing.
2. Search And Query v1.1.
3. Zettel Refactoring Commands.
4. Daily Dashboard.
5. Import And Export Bridges.

Before starting feature epics, do two small housekeeping tasks:

- Reconcile the parent `zorg-1` bead if the legend is considered complete.
- Refresh stale status and validation documentation, especially the `../zorg` README and the missing cross-repo validation
  script reference.

## Features Not Recommended As The Next Five

- Plugin system: wait until the core command/API surface stabilizes after watch, query JSON, and refactoring commands.
- Habit tracking: useful vertical workflow, but not central to the zettel graph.
- Recutils integration or alternate graph database: SQLite is already sufficient for the next layer.
- Folgezettel tree jumpers: valuable later, but they should build on refactoring/search/dashboard primitives.
- Embedded notes: useful, but higher risk because it introduces rendering and source-rewrite semantics before move/promote
  are proven.

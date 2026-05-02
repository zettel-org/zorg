# Zorg v1 MVP Implementation Plan

## Context

Implement the full Rust-based Zorg v1 MVP described by `research/202605/v1_mvp_curation.md`, with these user overrides:

- Canonical zettel files use the `.z` extension, not `.zo`.
- The default zettel root is `~/zorg`, not `~/org`.
- The v1 implementation must not support reading or writing the Python version's legacy syntax.
- The initialized repos are:
  - `../zorg`: main Rust CLI, library, store, query, formatter, and LSP implementation.
  - `../zorg-treesitter`: Tree-sitter grammar and generated parser bindings.
  - `../zorg-nvim`: Neovim plugin and user-facing editor integration.
- Old notes will be externally migrated by the user, so this plan should not allocate migration compatibility work.

The current repo state is effectively blank: each target repo has a `README.md` and git history, but no implementation
files.

## Product Spine

Zorg v1 is an editor-agnostic plaintext zettelkasten built around one primitive: the zettel. Files, directory `init.z`
files, and nested notes are all zettel. The MVP acceptance bar is:

1. `.z` files parse through a Tree-sitter grammar.
2. The Rust core indexes a `~/zorg` corpus into SQLite.
3. Zettel IDs, links, tags, properties, todos, and query zettel are represented in a single semantic model.
4. SWOG LIST queries run against the indexed corpus.
5. `zorg-ls` exposes useful LSP features over that corpus.
6. `zorg capture` and `zorg fix` make daily writing practical.
7. `zorg-nvim` provides a working Neovim frontend over the grammar, CLI, and LSP.

## Non-Negotiable Syntax Decisions

- File extension: `.z`.
- Default root: `~/zorg`.
- File zettel header: `%%% @foo ... %%%`.
- Directory zettel: `init.z`.
- Zettel IDs: `@foo`.
- Local IDs: `^bar`, resolved under the nearest ancestor zettel ID as `@ancestor/bar`.
- Absolute links: `#foo/bar`.
- Relative links: `+child` and `~sibling`.
- Type tags: `#z/todo`, `#z/ref`, `#z/inbox`, `#z/query`, `#z/tmpl`, etc.
- Properties: `key::value`, with slash-separated list values for v1.
- Lifecycle dates: `do::`, `due::`, `did::`.
- Timeboxing: `p::`, `start::`, `end::`.
- Todos: `[ ]`, `[N]`, `[X]`, `[?]` as zettel/task markers.
- Code blocks: ordinary Markdown fences.
- Query output: LIST only.
- Query and template definitions live in normal zettel, typically tagged `#z/query` or `#z/tmpl`.

Legacy syntax is not part of the grammar or semantic model. Do not implement `ID::`, `LID::`, `.zo`, `.zoq`, `.zot`,
`tick::`, generated `.zoc`, custom `@@@` code blocks, old folgezettel IDs, tag sugar, or Python-era link behavior. If
unsupported legacy-looking text appears, strict checks may report it as invalid input, but the parser and writer must
not treat it as a compatible data model.

## Repository Strategy

### `../zorg`

Create a modern Rust workspace, likely with these crates:

- `zorg-core`: syntax-independent data model, IDs, links, tags, properties, diagnostics, source spans.
- `zorg-parse`: wrapper around the Tree-sitter grammar and AST-to-model lowering.
- `zorg-store`: SQLite schema, ingestion, incremental hashing, text index, query-facing repository API.
- `zorg-query`: SWOG parser, planner, evaluator, LIST rendering.
- `zorg-fix`: formatter/linter/autofix operations.
- `zorg-capture`: template selection and capture write path.
- `zorg-ls`: LSP server binary built on `tower-lsp` or equivalent maintained LSP library.
- `zorg-cli`: `zorg` binary.

Use current stable Rust practices: Rust 2024 edition if available in the local toolchain, locked dependency versions,
workspace-level linting, `rustfmt`, `clippy`, integration tests, snapshot/golden tests for syntax and CLI output, and
documented crate boundaries. Avoid premature plugin architecture.

### `../zorg-treesitter`

Create a standard Tree-sitter grammar repo for Zorg `.z` files:

- `grammar.js` as the source of truth.
- `queries/highlights.scm`, `queries/locals.scm`, `queries/folds.scm`, and optionally `queries/injections.scm`.
- `corpus/` tests covering valid and invalid syntax.
- Generated parser artifacts only if the repo policy expects them.
- Rust binding crate if needed by `../zorg`, plus Node package metadata if useful for Tree-sitter tooling.

The grammar should parse structural syntax and preserve source spans. Semantic validation belongs in `../zorg`.

### `../zorg-nvim`

Create a small, documented Neovim plugin:

- Filetype detection for `*.z`.
- Tree-sitter parser registration and query installation guidance.
- LSP setup for `zorg-ls`.
- User commands for common CLI operations: index, query, fix, capture.
- Keymaps as opt-in setup examples, not forced globals.

Prefer simple Lua modules and clear docs over a large framework.

## Epic Breakdown

### Epic 1: Specification and Repository Foundations

Goal: turn the curated MVP into executable repo structure, documentation, and test harnesses.

Phase 1.1: Product and syntax docs

- Add `docs/syntax.md`, `docs/model.md`, `docs/query.md`, `docs/lsp.md`, `docs/capture.md`, and `docs/fix.md` in
  `../zorg`.
- Add explicit `.z`, `~/zorg`, and no-legacy policy.
- Define examples that become shared fixtures across the repos.
- Acceptance: docs contain enough detail for grammar, parser, store, LSP, and Neovim agents to work independently.

Phase 1.2: Rust workspace skeleton

- Initialize `../zorg` as a Rust workspace with the crates listed above or a comparable modular layout.
- Add workspace lint, format, test, and CI documentation.
- Add baseline CLI binaries `zorg` and `zorg-ls`.
- Acceptance: `cargo test --workspace`, `cargo fmt --check`, and `cargo clippy --workspace --all-targets` run on the
  empty skeleton.

Phase 1.3: Tree-sitter repo skeleton

- Initialize `../zorg-treesitter` with grammar package metadata, corpus test setup, query folders, and README docs.
- Acceptance: Tree-sitter generation and grammar tests run locally.

Phase 1.4: Neovim repo skeleton

- Initialize `../zorg-nvim` with Lua module layout, filetype detection, parser registration stub, LSP setup stub, docs,
  and README.
- Acceptance: plugin loads in a minimal Neovim runtime and recognizes `.z` buffers.

### Epic 2: Tree-sitter Grammar and Highlighting

Goal: produce the parser of record for `.z` files.

Phase 2.1: Core block grammar

- Implement document, paragraph, headings or note boundaries if used, nested bullets, file headers, code fences, and
  comments/blank lines.
- Acceptance: corpus fixtures parse without errors and preserve nesting.

Phase 2.2: Zettel syntax nodes

- Add nodes for `@id`, `^local_id`, `#absolute/link`, `+child`, `~sibling`, tags, type tags, properties, and todos.
- Acceptance: corpus tests cover each must-have syntax form and malformed examples.

Phase 2.3: Query and template zettel syntax

- Parse query blocks or query properties as specified in docs, without introducing `.zoq` or `.zot`.
- Acceptance: query zettel fixtures parse through the same grammar as ordinary notes.

Phase 2.4: Editor queries

- Add highlight, fold, locals, and injection queries for Zorg syntax and fenced code blocks.
- Acceptance: `tree-sitter highlight` produces useful groups for representative fixtures.

### Epic 3: Rust Parse and Semantic Model

Goal: lower Tree-sitter parse trees into a typed Zorg model.

Phase 3.1: Core data structures

- Implement `Zettel`, file zettel, directory zettel, nested note zettel, IDs, links, tags, properties, todos, source
  spans, and diagnostics.
- Acceptance: unit tests construct and compare canonical model values.

Phase 3.2: Parser integration

- Link `../zorg` to the grammar from `../zorg-treesitter`.
- Implement `zorg parse FILE` to emit a stable JSON AST/model dump.
- Acceptance: golden tests verify representative `.z` fixtures.

Phase 3.3: Semantic validation

- Validate duplicate IDs, unresolved local IDs, malformed relative links, invalid property values, and unsupported
  legacy-looking syntax under strict mode.
- Acceptance: diagnostics include file paths and source ranges.

Phase 3.4: ID and link resolution

- Implement absolute, child-relative, sibling-relative, and local ID resolution.
- Acceptance: tests cover file, directory, parent-child, sibling, anonymous note, and ambiguity cases.

### Epic 4: SQLite Store and Incremental Indexing

Goal: make the zettel graph persistent and queryable.

Phase 4.1: Store schema

- Design SQLite tables for files, zettel, IDs, links, tags, inherited tags, properties, todos, text, hashes, and
  metadata.
- Acceptance: migrations are deterministic and tested.

Phase 4.2: Corpus discovery

- Implement default root discovery at `~/zorg`, with CLI override.
- Index only `.z` files and directory `init.z` files.
- Acceptance: fixtures prove `.zo`, `.zoq`, `.zot`, and other files are ignored or diagnosed as unsupported when
  explicitly passed.

Phase 4.3: Incremental reindex

- Track file hashes and note hashes to avoid unnecessary work.
- Implement `zorg db reindex`, `zorg db status`, and clean handling of deleted/renamed files.
- Acceptance: tests prove unchanged files are skipped and changed notes are refreshed.

Phase 4.4: Tag inheritance and graph materialization

- Apply path-ancestor and parent-zettel tag inheritance.
- Persist resolved and unresolved links.
- Acceptance: query-facing APIs can distinguish explicit tags from inherited tags.

### Epic 5: SWOG Query MVP

Goal: support daily LIST queries against the store.

Phase 5.1: SWOG parser

- Implement filters for property equality, comparisons, property existence, tags, links, file globs, todo
  status/priority, negation, text search, and relative modify-date ranges.
- Acceptance: parser tests cover examples derived from the MVP curation file.

Phase 5.2: Query planner and evaluator

- Translate SWOG filters into SQLite-backed evaluation with deterministic ordering.
- Acceptance: integration tests run queries against a fixture corpus.

Phase 5.3: LIST renderer

- Implement `zorg query '<swog>'` and LIST output rendering.
- Keep TABLE, aggregation, custom functions, saved queries, and dot-snippets out of scope.
- Acceptance: golden tests verify terminal output.

Phase 5.4: Query zettel execution

- Execute query definitions stored in `#z/query` zettel.
- Acceptance: CLI can run a query by zettel ID and render LIST output.

### Epic 6: LSP MVP

Goal: expose the indexed zettel graph in editor-agnostic form.

Phase 6.1: LSP server foundation

- Implement initialize/shutdown, workspace root handling, document sync, config for `~/zorg`, logging, and index
  loading.
- Acceptance: integration test can start `zorg-ls` and open a `.z` document.

Phase 6.2: Diagnostics

- Publish duplicate ID, unresolved ID/link, invalid property, malformed syntax, and unsupported legacy-looking syntax
  diagnostics.
- Acceptance: diagnostic fixtures match expected ranges and messages.

Phase 6.3: Navigation and references

- Implement go-to-definition, find-references, document symbols, and workspace symbols.
- Acceptance: tests cover cross-file and nested-note navigation.

Phase 6.4: Completion and rename

- Implement completion for `#` links/tags and safe rename across IDs and links.
- Acceptance: rename edits are atomic and reject ambiguous targets.

Phase 6.5: Code actions

- Implement safe ID/link rewrite code actions and simple fix actions shared with `zorg fix`.
- Acceptance: code actions produce valid workspace edits.

### Epic 7: Capture and Fix

Goal: close the write loop for everyday use.

Phase 7.1: Formatter and check mode

- Implement `zorg check` and `zorg fix --check` for syntax, semantic diagnostics, duplicate IDs, unresolved links, and
  project/tag existence checks if configured.
- Acceptance: check exits nonzero on invalid corpora and emits stable messages.

Phase 7.2: Autofix operations

- Implement bullet-symbol normalization, auto-priority, ID stamping, modified-date stamping, and SORT pragma behavior.
- Acceptance: golden tests verify idempotent formatting.

Phase 7.3: Capture templates

- Implement `#z/tmpl` template discovery, template variable expansion, source-file recording, and destination selection.
- Acceptance: `zorg capture` can append or create `.z` files under `~/zorg`.

Phase 7.4: Interactive and noninteractive capture

- Add CLI flags for scripted capture and a minimal interactive path for humans and editor integrations.
- Acceptance: tests cover noninteractive capture, while manual docs cover interactive behavior.

### Epic 8: Neovim Integration

Goal: make Zorg useful inside Neovim without locking the core product to Neovim.

Phase 8.1: Filetype and parser integration

- Register `.z`, configure Tree-sitter parser lookup, and document install flow.
- Acceptance: opening a `.z` buffer sets the expected filetype and highlight queries can load.

Phase 8.2: LSP setup

- Provide `require("zorg").setup()` with `zorg-ls` config, root detection, and conservative defaults.
- Acceptance: users can enable LSP with one setup call.

Phase 8.3: Commands and UX helpers

- Add `:ZorgIndex`, `:ZorgQuery`, `:ZorgFix`, `:ZorgCapture`, and optional mappings.
- Acceptance: commands call the CLI and surface errors in Neovim.

Phase 8.4: Documentation and health checks

- Add `:checkhealth zorg`, installation docs, troubleshooting docs, and examples.
- Acceptance: README is enough to install from a plugin manager and connect to local Rust binaries.

### Epic 9: Documentation, Release, and Cross-Repo Validation

Goal: make the MVP coherent across three repos.

Phase 9.1: Shared fixtures

- Establish a small canonical fixture corpus and keep copies or submodule references synchronized across repos.
- Acceptance: parser, Rust, LSP, query, and Neovim tests all use the same representative `.z` examples.

Phase 9.2: End-to-end tests

- Add tests that parse, index, query, run LSP navigation, fix, and capture against temporary `~/zorg`-like roots.
- Acceptance: one documented command verifies the MVP locally.

Phase 9.3: README and docs pass

- Write strong READMEs for all three repos with install, usage, syntax, architecture, and development sections.
- Acceptance: a new contributor can build the parser, CLI, LSP, and Neovim plugin from docs alone.

Phase 9.4: Release packaging

- Define versioning, changelog policy, binary naming, generated parser policy, and minimal release checklist.
- Acceptance: a dry-run release checklist completes without hidden manual steps.

## Suggested Execution Order

1. Epic 1 first, because it creates the contracts and repo scaffolding other agents need.
2. Epic 2 and Epic 3 next, because every downstream feature depends on syntax and model stability.
3. Epic 4 after the model is stable enough to persist.
4. Epic 5 and Epic 6 can proceed in parallel once the store API is usable.
5. Epic 7 can begin after parser/model basics exist, but some autofixes should wait for semantic validation.
6. Epic 8 can begin after Tree-sitter and `zorg-ls` have stable entry points.
7. Epic 9 runs throughout, with the final release pass at the end.

## Cross-Epic Interfaces

- Grammar node names are a public contract between `zorg-treesitter`, `zorg-parse`, and `zorg-nvim`.
- `zorg-core` model structs are the public contract between parse, store, query, fix, capture, and LSP.
- Store repository APIs are the public contract for query and LSP.
- Fix operations should be shared between CLI and LSP code actions.
- Capture template parsing should use the same parser/model as ordinary zettel.
- Neovim should shell out to `zorg` and connect to `zorg-ls`; it should not reimplement semantics.

## Major Risks

- Tree-sitter grammar ambiguity around nested bullets, zettel boundaries, properties, and Markdown-like text.
- Scope creep from deferred features such as TABLE output, export, dashboard, plugins, saved queries, migration support,
  or old syntax compatibility.
- LSP rename and code actions corrupting links if source ranges are not precise.
- Query semantics becoming too broad before the store model is stable.
- Cross-repo drift if examples and node names are not documented early.

## Definition of Done for v1 MVP

- `zorg parse` parses `.z` files and reports useful diagnostics.
- `zorg db reindex` indexes `~/zorg` by default and only `.z` files.
- `zorg query` runs SWOG LIST queries against the indexed corpus.
- `zorg-ls` provides diagnostics, completion, go-to-definition, references, rename, and code actions.
- `zorg capture` writes new notes from `#z/tmpl` zettel.
- `zorg fix` and strict check mode are idempotent and tested.
- `zorg-nvim` recognizes `.z`, loads Tree-sitter highlighting, starts `zorg-ls`, and exposes common commands.
- All three repos have modern README files and focused docs.
- The implementation does not read or write Python-era legacy Zorg syntax.

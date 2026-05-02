---
create_time: 2026-05-02 16:46:40
bead_id: zorg-1.3
tier: epic
legend_bead_id: zorg-1
status: done
prompt: sdd/prompts/202605/epic_3_rust_parse_semantic_model.md
---
# Epic 3 Implementation Plan: Rust Parse and Semantic Model

## Context

Epic 3 implements the Rust semantic model and parser lowering layer for Zorg v1. The current repository already has the
Epic 1 workspace skeleton, docs, fixtures, and crate boundaries:

- `crates/zorg-core`: shared semantic types and diagnostics, currently minimal.
- `crates/zorg-parse`: parser boundary, currently an explicit stub.
- `crates/zorg-cli`: `zorg` binary, currently no `parse` command.
- `fixtures/corpus`: canonical `.z` examples for Rust and cross-repo tests.
- `../zorg-treesitter`: generated grammar artifacts exist locally, with public node contracts documented in
  `../zorg-treesitter/docs/grammar.md`.

Baseline validation before planning: `cargo test --workspace` passes.

Epic 3 must preserve the non-negotiable v1 contract:

- Canonical files are `.z`; default root is `~/zorg`.
- File and directory zettel headers use `%%% ... %%%`; directory zettel are `init.z`.
- IDs are `@foo`, local IDs are `^bar`, links are `#foo/bar`, `+child`, and `~sibling`.
- Tags, properties, todos, query zettel, and template zettel are ordinary zettel semantics.
- Python-era legacy syntax must not become a compatible input model.

## Recommended Phase Split

Use five sequential phases. Each phase is sized for a distinct agent instance and should leave the repo in a passing
state. Later agents should read the previous phase's tests and public types before changing behavior.

## Phase 3.1: Core Semantic Model

Goal: replace the foundation-only `zorg-core` types with the complete typed model needed by parser, store, query, fix,
capture, and LSP consumers.

Primary ownership:

- `crates/zorg-core/src/lib.rs`
- optional submodules under `crates/zorg-core/src/`
- `crates/zorg-core/Cargo.toml`
- focused `zorg-core` unit tests

Implementation scope:

- Define stable source types:
  - `SourcePath` or path fields on documents/diagnostics.
  - byte spans plus line/column positions suitable for later LSP conversion.
  - helpers to convert byte offsets to one-based line/column positions.
- Define ID and reference types:
  - absolute `ZettelId`
  - `LocalId`
  - unresolved and resolved link/reference forms for absolute, child-relative, sibling-relative, and local declarations.
- Define zettel graph types:
  - `ZettelDocument`
  - `Zettel`
  - `ZettelKind` for file, directory, and nested zettel.
  - parent/child identity fields that work before persistence exists.
  - body blocks for paragraphs, fenced code blocks, and child zettel.
- Define semantic atoms:
  - `Tag` and type-tag classification for `#z/...`.
  - ordered `Property` with raw values and parsed key/value spans.
  - `TodoMarker` for `[ ]`, `[N]`, `[X]`, and `[?]`.
  - `TitlePart` or equivalent plain-title representation.
- Expand diagnostics:
  - severity, code/category, path, span, message.
  - diagnostic constructors for syntax recovery, semantic validation, and unsupported legacy-looking input.
- Add serde support if it is needed by Phase 3.3 JSON output. Prefer deriving `Serialize`/`Deserialize` in the core
  model now if the public shape is stable.

Acceptance:

- Unit tests construct representative file, directory, nested, query, template, todo, and anonymous zettel values.
- Tests prove canonical ID text formatting and parsing decisions for valid and invalid ID/link/tag/property forms.
- Tests prove source span to line/column conversion.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff notes:

- This phase should not depend on Tree-sitter yet.
- Keep validation-light constructors available for parser lowering, but expose explicit checked constructors for
  user-authored identifiers.
- Avoid store/query-specific fields; keep the model syntax and semantic.

## Phase 3.2: Tree-sitter Binding and Parse Tree Boundary

Goal: connect `zorg-parse` to the `../zorg-treesitter` grammar and expose a real syntax parse boundary without
completing all semantic lowering.

Primary ownership:

- `crates/zorg-parse/Cargo.toml`
- `crates/zorg-parse/src/lib.rs`
- optional parser/binding modules under `crates/zorg-parse/src/`
- parser-focused tests and golden syntax snapshots

Implementation scope:

- Add `tree-sitter` Rust dependencies and a local grammar binding strategy. The current grammar repo has generated C
  sources under `../zorg-treesitter/src` but documents that generated output is not final packaging policy. For this
  phase, prefer a local path/build-script integration that is easy to replace in release packaging.
- Parse `.z` UTF-8 source into a `ParsedSyntaxDocument` that retains:
  - original source
  - Tree-sitter tree/root node metadata needed for traversal
  - syntax diagnostics for parser errors or missing required structures
  - file path when provided
- Define stable internal node-name constants matching `../zorg-treesitter/docs/grammar.md`: `source_file`,
  `file_header`, `file_header_open`, `file_header_close`, `zettel_item`, `zettel_opening`, `paragraph`,
  `paragraph_line`, `fenced_code_block`, `fence_start`, `code_fence_body`, `fence_end`, `id`, `local_id`,
  `absolute_link`, `child_link`, `sibling_link`, `tag`, `type_tag`, `property`, `property_key`, `property_value`,
  `todo_marker`, and `title_text`.
- Add tests that parse every file in `fixtures/corpus`, including the strict invalid legacy fixture. Valid fixtures
  should have no Tree-sitter `ERROR` nodes; legacy-looking text may recover as text or diagnostics but must not produce
  compatibility model nodes.

Acceptance:

- `zorg-parse::parse_syntax` or equivalent returns a real syntax document.
- Existing `parse_document` stub behavior is replaced or routed through the new parser boundary.
- Tests cover `minimal.z`, `nested.z`, `query_and_template.z`, `dir/init.z`, and `legacy_invalid.z`.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff notes:

- Do not overfit to generated node IDs; downstream code should rely on public node names.
- If grammar integration requires generated artifacts, document the exact local expectation in `docs/development.md`
  without changing the cross-repo generated parser policy.

## Phase 3.3: AST-to-Model Lowering and `zorg parse`

Goal: lower Tree-sitter syntax into the typed `zorg-core` model and expose a stable CLI JSON dump for fixtures and
downstream agents.

Primary ownership:

- `crates/zorg-parse/src/`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/`
- golden fixtures under an appropriate Rust test fixture path
- minor `zorg-core` additions only where Phase 3.1 missed necessary model shape

Implementation scope:

- Implement syntax traversal into `ZettelDocument`.
- Map top-level file header to a file or directory zettel:
  - normal `.z` file: `ZettelKind::File`
  - `init.z`: `ZettelKind::Directory`
- Preserve nested zettel hierarchy from nested `zettel_item` nodes.
- Extract inline primitives from file headers, nested openings, and paragraph text:
  - IDs and local IDs
  - links
  - tags and type tags
  - properties
  - todo markers
  - title text
- Preserve body blocks:
  - paragraphs with inline links/properties where present
  - fenced code blocks with info string and body spans
  - child zettel order
- Add `zorg parse FILE`:
  - reads a `.z` file
  - emits deterministic pretty JSON model output to stdout
  - exits nonzero on unreadable files or parser errors that should block model output
  - includes diagnostics in output for recoverable issues
- Add golden tests for representative parse JSON. Keep output stable and intentionally model-oriented rather than
  Tree-sitter-oriented.

Acceptance:

- `cargo run -p zorg-cli -- parse fixtures/corpus/minimal.z` emits JSON with the file zettel, ID `@minimal`,
  type/tag/property/title, and no diagnostics.
- Golden tests cover minimal, nested, directory init, query/template fenced blocks, and legacy-invalid recoverable
  diagnostics.
- JSON output does not include absolute machine-specific paths unless tests normalize them or use relative fixture
  paths.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff notes:

- This phase should not attempt full link resolution or duplicate detection.
- It should preserve enough parent, child, and span data for Phase 3.4 and Phase 3.5 to validate semantics without
  re-parsing.

## Phase 3.4: Semantic Validation

Goal: add strict semantic validation over parsed documents and corpora, including duplicate IDs, malformed values, local
ID context checks, and legacy-looking input diagnostics.

Primary ownership:

- `crates/zorg-core/src/` validation-related model additions
- `crates/zorg-parse/src/` semantic validation module
- `crates/zorg-cli/src/main.rs` parse/check-mode wiring as needed
- validation tests and diagnostic goldens

Implementation scope:

- Introduce a semantic validation API that can run on:
  - a single parsed document
  - a small in-memory corpus of parsed documents
- Validate canonical IDs:
  - duplicate absolute IDs within a document and across a corpus
  - duplicate local IDs after canonicalization under the same ancestor
  - malformed ID-like text that the grammar recovered as text where practical
- Validate local ID declarations:
  - `^bar` requires a nearest ancestor with an absolute ID.
  - canonical form is `@ancestor/bar`.
- Validate properties:
  - keys must match v1 syntax if they enter the model.
  - typed values for known date/time properties should be checked conservatively enough to catch obvious invalid values
    without committing future query semantics too early.
  - `tick::` must be diagnosed as legacy-looking input, not accepted as an alias.
- Validate unsupported legacy-looking input:
  - `ID::`
  - `LID::`
  - `tick::`
  - custom `@@@` fences
  - `.zo`, `.zoq`, `.zot`, `.zoc` textual references in strict mode
  - old folgezettel-looking IDs when confidently detectable
- Ensure every diagnostic has severity, message, optional code, file path, and source range when the source allows it.
- Decide and document strictness modes:
  - parser/model JSON may include diagnostics and still emit a model.
  - strict validation APIs return failure status when error diagnostics exist.

Acceptance:

- Diagnostic tests assert paths, byte spans, line/column positions, severity, and stable messages or codes.
- `fixtures/corpus/legacy_invalid.z` produces legacy diagnostics without producing legacy-compatible model fields.
- Duplicate ID tests cover single-file and multi-file corpora.
- Invalid local ID tests cover missing ancestor and duplicate canonical local IDs.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff notes:

- Keep validation APIs independent of SQLite; Epic 4 will persist results later.
- Prefer diagnostic codes for tests and downstream LSP mapping so message text can evolve.

## Phase 3.5: ID and Link Resolution

Goal: resolve Zorg links and local IDs into canonical IDs, attach unresolved diagnostics, and finalize Epic 3 behavior
for downstream store, query, LSP, fix, and capture work.

Primary ownership:

- `crates/zorg-core/src/` resolved reference model refinements
- `crates/zorg-parse/src/` resolver module
- `crates/zorg-cli/src/main.rs` parse JSON output refinements
- resolver tests and golden outputs
- small documentation updates if behavior choices need to be recorded

Implementation scope:

- Build an in-memory symbol table from a parsed document or corpus:
  - absolute IDs
  - local IDs canonicalized beneath nearest ID-bearing ancestor
  - parent/child relationships
  - file and directory zettel contexts
- Resolve written links:
  - `#foo/bar` to `@foo/bar`
  - `+child` beneath the current zettel canonical ID
  - `~sibling` beneath the current zettel parent's canonical ID
- Report unresolved diagnostics for:
  - absolute links without targets
  - child links when current zettel has no canonical ID
  - sibling links when parent context has no canonical ID
  - ambiguous or duplicate targets
- Ensure anonymous zettel are allowed but cannot be direct link targets.
- Add corpus-level tests for:
  - file zettel links
  - directory `init.z` links and local IDs
  - nested parent-child links
  - sibling links
  - anonymous note contexts
  - duplicate/ambiguous targets
- Update `zorg parse FILE` JSON so unresolved and resolved links are visible in deterministic output.
- Add a final Epic 3 fixture test that parses, validates, and resolves the whole `fixtures/corpus` valid subset.

Acceptance:

- All Epic 3 roadmap acceptance points are met:
  - core data structures exist and are tested.
  - parser integrates with the Tree-sitter grammar.
  - `zorg parse FILE` emits stable JSON model output.
  - semantic validation reports duplicate IDs, local-ID issues, malformed values, relative-link context errors, and
    unsupported legacy-looking syntax.
  - ID and link resolution handles absolute, child-relative, sibling-relative, local, file, directory, nested,
    anonymous, and ambiguity cases.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff notes:

- Do not start SQLite indexing in this phase. The resolver can expose the in-memory data Epic 4 will persist.
- Do not implement SWOG parsing, LSP protocol behavior, fix actions, or capture writes beyond keeping their crate
  boundaries compiling.

## Cross-Phase Design Decisions

### Serialization

Use serde for JSON output unless a strong blocker appears. Stable JSON is a public testing contract for later agents, so
prefer explicit field names and avoid serializing internal Tree-sitter details.

### Error Handling

Keep `ZorgResult<T>` and `ZorgError` for operational failures such as unreadable files or impossible parser
initialization. Use model diagnostics for user-source problems that can be reported with spans.

### Paths

Model diagnostics should carry source paths. Tests should avoid hard-coded absolute paths by parsing fixtures with
relative paths or normalizing output.

### Tree-sitter Integration

The grammar repo currently contains generated artifacts but says generated parser output is not the final committed
contract. For Epic 3, a pragmatic local binding is acceptable as long as:

- node names come from `docs/grammar.md`, not private grammar rules;
- the integration is documented;
- replacing it with a packaged binding later does not require changing `zorg-core` public model types.

### Legacy Policy

Legacy-looking syntax is diagnostic input only. Do not add model variants for `ID::`, `LID::`, `.zo`, `.zoq`, `.zot`,
`.zoc`, old folgezettel IDs, tag sugar, custom `@@@` code blocks, or Python-era link behavior.

## Final Epic 3 Validation Command Set

The final phase should leave these commands passing from the repo root:

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- parse fixtures/corpus/minimal.z
cargo run -p zorg-cli -- parse fixtures/corpus/nested.z
cargo run -p zorg-cli -- parse fixtures/corpus/query_and_template.z
```

Optional if the Tree-sitter repo is available and generated artifacts are touched:

```sh
(cd ../zorg-treesitter && npm test)
```

## Out of Scope for Epic 3

- SQLite schema, incremental indexing, and corpus discovery beyond in-memory multi-document validation fixtures.
- SWOG query parser/evaluator.
- LSP protocol implementation.
- Capture writing and template expansion.
- Formatter/autofix behavior.
- Neovim plugin changes.
- Python legacy migration or compatibility.

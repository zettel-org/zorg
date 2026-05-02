---
create_time: 2026-05-02 19:05:24
status: wip
prompt: sdd/prompts/202605/zorg_epic6_lsp_mvp.md
---
# Zorg Epic 6: LSP MVP Implementation Plan

## Goal

Implement Epic 6 from `sdd/legends/202605/zorg_v1_mvp.md`: an editor-agnostic `zorg-ls` language server that exposes the
indexed Zorg graph to editors.

The finished epic should let an LSP client:

- Start `zorg-ls`, initialize it for a `.z` workspace, and synchronize open documents.
- Receive diagnostics for parser, semantic, query/template, and unsupported legacy-looking input.
- Navigate between zettel IDs and links.
- Search document/workspace symbols and references.
- Complete IDs, links, and tags at relevant trigger positions.
- Safely rename zettel IDs and update source-backed links.
- Offer deterministic code actions where the server can prove the edit is safe.

## Current Context

The target implementation lives in this Rust workspace.

Relevant crate state:

- `crates/zorg-ls`: currently a placeholder binary that supports `--help` and `--version` only.
- `crates/zorg-core`: owns IDs, links, tags, properties, todos, diagnostics, source spans, and zettel model structs.
- `crates/zorg-parse`: parses `.z` files, lowers to semantic model, validates corpora, resolves links, and preserves
  spans for many syntax primitives.
- `crates/zorg-store`: owns SQLite indexing and query-facing APIs for files, zettel, links, tags, effective tags,
  properties, todos, diagnostics, ancestors, descendants, incoming links, and outgoing links.
- `crates/zorg-query`: owns SWOG query execution and may already be complete enough for query/template diagnostics.
- `crates/zorg-fix`: currently a strict-check/autofix stub. LSP code actions should share edit-planning concepts with
  this crate only where that can be done without pulling Epic 7 into this epic.

Important gaps found before planning:

- `zorg-ls` has no LSP protocol implementation or async runtime dependency yet.
- `zorg-store::StoredDiagnostic` does not expose source ranges even though the database persists diagnostic spans.
- `StoredZettel` exposes byte spans but not line/column positions or declaration-specific spans.
- Rename needs source-backed declaration and reference ranges. Store rows are useful for graph discovery, but precise
  rewrites should come from reparsed source documents and `zorg-core`/`zorg-parse` spans.
- Code actions must be conservative because `zorg-fix` is still a stub.

## Non-Negotiables

- `zorg-ls` must not invent parser, resolver, query, capture, or formatting semantics. It delegates to existing crates.
- Only `.z` files are canonical source. Default root is `~/zorg`, with LSP initialization/config able to override it.
- Directory zettel are `init.z`.
- Legacy Python-era syntax is diagnosed, not translated or migrated.
- Every navigation, rename, completion detail, diagnostic, and code action must be source-span-backed. If the server
  lacks a reliable span, it should decline that feature instead of guessing.
- Rename must be atomic. If any affected declaration or reference cannot be rewritten safely, return an error and no
  edits.
- Multi-root support is not required for the MVP. If multiple roots are supplied, the server should either choose a
  documented single root deterministically or report unsupported multi-root behavior clearly.
- Formatting, query-result virtual documents, workspace commands, advanced semantic tokens, and query-driven completion
  are out of scope for Epic 6.

## Proposed Phase Split

Use seven sequential phases. Each phase is intended for a distinct agent instance and should leave the workspace in a
passing state. The split intentionally creates a reusable LSP support layer before feature work, then adds graph
features from least risky to most rewrite-heavy.

## Phase 6.1: Protocol Foundation and Workspace Lifecycle

Purpose: replace the placeholder `zorg-ls` binary with a real but minimal LSP server.

Primary ownership:

- `crates/zorg-ls/Cargo.toml`
- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- `docs/lsp.md`

Scope:

- Add maintained LSP/runtime dependencies, preferably `tower-lsp` plus `tokio`, unless the agent has a strong reason to
  choose another maintained Rust LSP library.
- Keep `zorg-ls --help` and `zorg-ls --version` behavior working.
- Implement stdio LSP startup for normal execution.
- Implement `initialize`, `initialized`, `shutdown`, and `exit`.
- Advertise only capabilities implemented in this phase:
  - text document open/change/close sync
  - diagnostics if Phase 6.1 emits a harmless empty diagnostic publish path
- Parse initialization options and workspace folders into a `ServerConfig`:
  - root path, defaulting to `~/zorg`
  - optional database path, defaulting under the root through `StoreOptions`
  - tracing/log level if a simple environment/config knob is cheap
- Create an internal server state:
  - selected root and database path
  - open document map keyed by URI
  - store open/load status
  - cancellation-safe read/write lock around mutable state
- Load the current store snapshot on initialize when possible. If the database is missing or stale, do not crash; record
  degraded status for later diagnostics/features.
- Add protocol integration tests that start `zorg-ls`, send JSON-RPC initialize/shutdown messages, and open a `.z`
  document.

Acceptance:

- `cargo run -p zorg-ls -- --version` and `cargo run -p zorg-ls -- --help` still work.
- Integration tests can initialize and shut down the server over stdio.
- Integration tests can open and change a `.z` document without panics.
- `docs/lsp.md` documents initialization options and the single-root MVP behavior.
- `cargo fmt --check`, `cargo test -p zorg-ls`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Later phases can add capabilities by extending a real LSP backend, not by replacing the binary again.
- The server has a stable config/state module and test harness.

## Phase 6.2: Diagnostics Pipeline

Purpose: publish accurate LSP diagnostics for both indexed workspace diagnostics and live open-document diagnostics.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- targeted additions to `crates/zorg-store/src/lib.rs`
- `docs/lsp.md`

Scope:

- Add conversion helpers between `zorg_core::SourceSpan` and LSP zero-based ranges.
- Add severity/category/code mapping from `zorg_core::Diagnostic` and stored diagnostic rows into LSP diagnostics.
- Extend `zorg-store::StoredDiagnostic` or add a new query method so stored diagnostics expose:
  - file path or file row information sufficient to produce a URI
  - zettel row when known
  - severity, category, code, message
  - byte span and line/column span fields when present
- On initialize and relevant file events, publish diagnostics for indexed files when the store has diagnostics.
- For open documents, parse and validate the in-memory text instead of relying only on the SQLite snapshot.
- Merge live document diagnostics with corpus-level diagnostics conservatively:
  - live syntax and single-document validation take precedence for the open URI
  - duplicate ID and cross-file unresolved-link diagnostics may still come from the store until a full live overlay
    exists
- Publish empty diagnostic arrays when a file becomes clean or closes and the indexed snapshot has no diagnostics.
- Cover diagnostics required by `docs/lsp.md` where the lower layers already support them:
  - duplicate IDs
  - unresolved absolute, child-relative, sibling-relative, and local references
  - invalid property syntax or typed values
  - unsupported legacy-looking syntax
  - malformed query/template definitions if available from prior epics
- Do not implement automatic reindexing in this phase unless it is a tiny, deterministic refresh hook. Prefer explicit
  store-load behavior and clear degraded diagnostics for missing/stale indexes.

Acceptance:

- Tests assert LSP diagnostic ranges use zero-based line/character coordinates and match fixture source spans.
- Tests cover at least one live unsaved buffer diagnostic and one stored cross-file diagnostic.
- Tests prove legacy-looking input is diagnosed and not transformed.
- Diagnostics are cleared when a document changes from invalid to valid.
- `cargo fmt --check`, `cargo test -p zorg-ls`, `cargo test -p zorg-store`, and `cargo clippy --workspace --all-targets`
  pass.

Handoff:

- Later features can reuse URI/path conversion, span conversion, and live document parsing.
- Store diagnostics include enough location data for editor consumers.

## Phase 6.3: LSP Graph Snapshot, Symbols, and Navigation

Purpose: build the reusable source-backed graph view needed by definition, references, symbols, completion, rename, and
code actions.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- small targeted additions to `crates/zorg-store/src/lib.rs` only if current read APIs cannot supply necessary graph
  rows

Scope:

- Introduce an internal `LspIndex` or similarly named graph snapshot:
  - indexed files by URI and root-relative path
  - zettel declarations by canonical ID
  - local ID declarations by resolved canonical ID
  - explicit tags and effective tags
  - outgoing and incoming links
  - zettel hierarchy, source order, titles, and source spans
- For precision-sensitive operations, reparse relevant source files and collect declaration/reference spans from
  `zorg-parse` semantic models. Use the SQLite store as the graph discovery layer, not as the only source of edit
  ranges.
- Add helper functions:
  - find the containing zettel for a document position
  - find token/reference/declaration at a position
  - locate a canonical ID declaration
  - locate all source-backed references to a canonical ID
  - convert zettel/tag/link rows to LSP locations
- Implement go-to-definition:
  - on `@id` declarations: return the declaration itself or the canonical declaration location
  - on `#absolute`, `+child`, `~sibling`, and local references: return the resolved target declaration when unique
  - return no result for anonymous or unresolved targets
- Implement find-references for canonical zettel IDs:
  - include declaration when requested
  - include source-backed incoming links
  - include local declaration references after canonicalization
- Implement document symbols for zettel in the current file:
  - nested symbols should follow zettel hierarchy
  - symbol names should prefer canonical ID, then title, then root-relative path
- Implement workspace symbols for canonical IDs and queryable titles.

Acceptance:

- Tests cover cross-file go-to-definition from `#project/plan` to `@project/plan`.
- Tests cover nested child and sibling navigation from `+task` and `~review`.
- Tests cover references to an ID from multiple files and nested zettel.
- Tests cover document symbols preserving nested-note hierarchy.
- Tests cover workspace symbols finding canonical IDs.
- Operations with missing spans return no result rather than pointing at the wrong text.
- `cargo fmt --check`, `cargo test -p zorg-ls`, `cargo test -p zorg-store`, and `cargo clippy --workspace --all-targets`
  pass.

Handoff:

- Later phases can reuse `LspIndex` and source-backed occurrence collection for completion, rename, and code actions.

## Phase 6.4: Completion MVP

Purpose: provide useful editor completions without mutating source.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- `docs/lsp.md`

Scope:

- Advertise completion capability with trigger characters:
  - `#` for absolute links and tags
  - `+` for child-relative links
  - `~` for sibling-relative links
  - optionally `/` for continuing slash-separated ID/tag paths
- Implement context-aware completion:
  - after `#`, offer canonical zettel IDs as link text using `#foo/bar`
  - after `#`, offer explicit/effective tags such as `#z/todo` and `#area/work` when the context appears tag-like
  - after `+`, offer child canonical IDs relative to the containing zettel
  - after `~`, offer siblings relative to the containing zettel or canonical ID path
- Keep tag versus link ambiguity simple for the MVP:
  - use surrounding parse context when available
  - otherwise offer both categories with distinct labels/kinds/details
- Include enough metadata in completion items:
  - label
  - insert text
  - kind
  - detail showing title or root-relative path
  - stable sort text
- Do not implement query-driven completion or property-key completion unless it is trivial and does not widen scope.
- Ensure completions degrade cleanly when the store is missing or the current document cannot be parsed.

Acceptance:

- Tests cover `#`, `+`, and `~` completions inside representative fixture buffers.
- Tests prove child and sibling suggestions are scoped to the containing zettel.
- Tests prove tag completions include known type tags and explicit corpus tags.
- Tests prove completion item ordering is deterministic.
- `docs/lsp.md` documents MVP completion behavior and deferred completion classes.
- `cargo fmt --check`, `cargo test -p zorg-ls`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Completion uses the same graph snapshot and position helpers as navigation.

## Phase 6.5: Safe Rename Planning and Execution

Purpose: implement atomic zettel ID rename with conservative source rewrites.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- possible small additions to `crates/zorg-core` for ID validation reuse if needed
- `docs/lsp.md`

Scope:

- Advertise prepare-rename and rename capabilities.
- Implement a source-backed rename planner that:
  - identifies the canonical zettel ID at the request position
  - validates the requested new ID with existing `ZettelId` rules
  - rejects anonymous zettel, unresolved references, ambiguous declarations, and missing spans
  - rejects collisions with an existing canonical ID
  - collects all source-backed declarations and references to rewrite
  - computes a single `WorkspaceEdit`
- Implement declaration rewrites for:
  - absolute `@foo/bar` declarations
  - local `^bar` declarations only when the rename is changing the resolved canonical suffix under the same ancestor and
    the local declaration text can be rewritten deterministically
- Implement reference rewrites conservatively:
  - absolute `#foo/bar` references can be rewritten to the new absolute link target
  - relative `+child` and `~sibling` references may remain unchanged if they still resolve to the renamed target after
    the declaration edit
  - relative references that would stop resolving to the same target must be rewritten only when the new relative form
    is deterministic; otherwise reject the rename
- Preserve text outside the exact source spans.
- Return a clear LSP error when rename is unsafe and no edits are produced.
- Add tests that inspect exact `WorkspaceEdit` ranges and replacement text.

Acceptance:

- `textDocument/prepareRename` succeeds only on source-backed ID/link occurrences.
- Rename of a simple absolute ID rewrites the declaration and all absolute links atomically.
- Rename rejects duplicate target IDs.
- Rename rejects unresolved, anonymous, or spanless occurrences.
- Rename handles nested/local ID cases where safe and rejects the rest with a clear error.
- Relative links are either preserved because they still resolve correctly, rewritten deterministically, or cause a full
  rename rejection.
- `cargo fmt --check`, `cargo test -p zorg-ls`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Code actions can reuse the rename planner's validation and edit application primitives.

## Phase 6.6: Code Actions and Safe Fix Hooks

Purpose: expose deterministic fixes through LSP without implementing the full Epic 7 formatter.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- focused additions to `crates/zorg-fix/src/lib.rs` only for shared edit-plan types or trivial safe operations
- `docs/lsp.md`
- `docs/fix.md` if shared behavior is clarified

Scope:

- Advertise code action capability for `.z` documents.
- Define a small code-action catalog tied to diagnostics and source spans. MVP-safe candidates:
  - rewrite an unresolved absolute link when exactly one canonical ID differs only by a clearly documented typo rule, if
    such a rule is implemented and tested
  - convert a stale absolute link to a known renamed canonical ID only if the index contains an unambiguous mapping from
    the current source state
  - normalize property whitespace only if `zorg-fix` gains an idempotent helper in this phase
  - offer rename-driven ID/link rewrite actions by delegating to the Phase 6.5 edit planner
- If none of the above can be proven deterministic, implement a minimal but honest code-action framework that returns no
  actions for unsafe diagnostics and documents why. Do not fabricate fuzzy fixes.
- Use `WorkspaceEdit` and exact source ranges for every action.
- Ensure code actions are unavailable for legacy migration. Legacy diagnostics can explain that migration is outside v1,
  but must not offer automatic conversion to v1 data.

Acceptance:

- Tests cover at least one positive deterministic code action if a safe action exists after Phase 6.5.
- Tests cover negative cases: unresolved ambiguous links, legacy syntax, missing spans, and stale indexes produce no
  unsafe edits.
- Any helper added to `zorg-fix` has idempotency tests if it mutates text.
- `docs/lsp.md` documents supported and intentionally unavailable code actions.
- `cargo fmt --check`, `cargo test -p zorg-ls`, `cargo test -p zorg-fix`, and `cargo clippy --workspace --all-targets`
  pass.

Handoff:

- Epic 7 can extend the shared fix/edit planner without changing LSP protocol plumbing.

## Phase 6.7: End-to-End LSP Hardening and Documentation Pass

Purpose: make the full Epic 6 feature set coherent, tested, and ready for `zorg-nvim`.

Primary ownership:

- `crates/zorg-ls/src/`
- `crates/zorg-ls/tests/`
- `docs/lsp.md`
- `README.md` or `docs/development.md` if local verification commands belong there
- shared fixtures under `fixtures/corpus/` only if new LSP-specific cases are needed

Scope:

- Add end-to-end tests that initialize `zorg-ls` against a temporary copied corpus and exercise:
  - diagnostics
  - go-to-definition
  - references
  - document symbols
  - workspace symbols
  - completion
  - prepare-rename and rename
  - code actions
- Add tests for degraded states:
  - missing database
  - stale database
  - invalid root
  - non-`.z` document
  - unsupported multi-root initialization
- Review capability advertisement so the server only advertises implemented behavior.
- Review logs/errors so editor users get useful messages without noisy stdout corruption.
- Ensure fixture databases are not accidentally committed or required. Tests should create temporary indexes as needed.
- Update docs with:
  - launch command
  - initialization options
  - expected root/database behavior
  - feature list
  - deferred features
  - troubleshooting for missing/stale indexes
- Run full workspace verification.

Acceptance:

- One documented local command verifies the LSP MVP.
- No LSP test depends on a preexisting committed SQLite database under `fixtures/corpus/.zorg`.
- `zorg-ls` communicates only through valid LSP JSON-RPC on stdio during server mode.
- `zorg-ls --help` documents CLI options without implying unsupported behavior.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets` pass.

## Cross-Phase Technical Design

### LSP Architecture

Keep `crates/zorg-ls/src/main.rs` small. A reasonable module split is:

- `main.rs`: CLI option dispatch and stdio server startup.
- `server.rs`: LSP backend implementation.
- `config.rs`: initialization options, root/database resolution, single-root policy.
- `state.rs`: open documents, store status, graph snapshot, synchronization.
- `diagnostics.rs`: diagnostic conversion and publication.
- `spans.rs`: URI/path and `SourceSpan`/LSP range conversion.
- `index.rs`: source-backed `LspIndex` and occurrence collection.
- `navigation.rs`: definition, references, document symbols, workspace symbols.
- `completion.rs`: completion providers.
- `rename.rs`: safe rename planner.
- `actions.rs`: code action providers.
- `testing.rs` or integration-test helpers: JSON-RPC test harness utilities if useful.

Agents may choose different file names, but they should preserve the same separation of concerns.

### Store Versus Live Documents

The SQLite store is the stable workspace graph snapshot. Open documents are potentially newer than the store.

MVP behavior should be:

- Use live text for syntax diagnostics and position-based operations in the open document.
- Use the store for cross-file graph data until automatic live overlay indexing is explicitly added.
- When live text changes make graph operations unsafe, decline the operation with a clear error or return no result.
- Avoid automatic writes or reindexing unless the operation is explicitly a safe LSP edit.

### Source Ranges

Use one-based line/column positions inside `zorg-core::SourceSpan` and convert to zero-based LSP positions at the edge.
Prefer existing line/column data. If only byte offsets are available, recompute line/column from the exact source text.
Decline feature work when neither reliable line/column nor source text is available.

### Testing Strategy

Prefer protocol-level integration tests for externally visible behavior and focused unit tests for edit planners and
span conversion. Tests should create temporary corpora and run `Store::reindex_full` or `zorg db reindex` as needed,
rather than depending on committed database artifacts.

Use existing fixtures first:

- `fixtures/corpus/nested.z` for nested, child, sibling, todo, and reference behavior.
- `fixtures/corpus/query_focus.z` for cross-file links and query zettel.
- `fixtures/corpus/query_and_template.z` for query/template zettel cases.
- `fixtures/corpus/legacy_invalid.z` for unsupported legacy diagnostics.
- `fixtures/corpus/dir/init.z` for directory zettel behavior.

Add LSP-specific fixtures only when the existing corpus cannot express the case clearly.

## Risks And Mitigations

- Risk: LSP rename corrupts source because declaration spans are not persisted in the store. Mitigation: reparse source
  files for all edit ranges and reject spanless cases.
- Risk: live documents diverge from the indexed store. Mitigation: use live text for the active document and reject
  cross-file graph edits when the affected graph is stale or ambiguous.
- Risk: code actions expand into Epic 7 formatter work. Mitigation: implement only deterministic, source-span-backed
  actions. A no-action framework is acceptable where fixes cannot be proven safe.
- Risk: dependency/API churn in the LSP library. Mitigation: keep LSP protocol usage behind local modules and add
  protocol tests early.
- Risk: multi-root support delays the MVP. Mitigation: document and test single-root behavior, and report unsupported
  multi-root clearly.

## Definition Of Done For Epic 6

- `zorg-ls` is a real LSP server over stdio while preserving `--help` and `--version`.
- Initialization selects the correct root/database and handles missing or stale indexes without crashing.
- Diagnostics publish accurate source ranges for syntax, semantic, legacy, and supported query/template failures.
- Go-to-definition, references, document symbols, and workspace symbols work for source-backed zettel graph data.
- Completion works for absolute links/tags, child links, and sibling links with deterministic ordering.
- Rename validates safety and emits atomic workspace edits for supported ID/link rewrites.
- Code actions exist only for deterministic safe edits and decline unsafe or legacy-migration cases.
- The full workspace passes `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets`.
- `docs/lsp.md` describes the implemented behavior, options, limitations, and troubleshooting path for editor
  integrations.

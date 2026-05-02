---
create_time: 2026-05-02 18:14:07
status: done
prompt: sdd/prompts/202605/zorg_epic5_swog_query.md
bead_id: zorg-1.5
tier: epic
legend_bead_id: zorg-1
---
# Zorg Epic 5: SWOG Query MVP Implementation Plan

## Goal

Implement Epic 5 from `sdd/legends/202605/zorg_v1_mvp.md`: SWOG LIST queries against the indexed Zorg corpus.

The finished epic should let a user run inline queries such as:

```bash
zorg query '#z/todo -did:* due:<=today'
zorg query 'area:work/zorg todo:[ ]'
zorg query 'links:#project/plan modified:<7d'
```

It should also let a user execute a query definition stored in an ordinary `#z/query` zettel.

## Current Context

The target implementation lives in `../zorg`.

Relevant planned crate boundaries:

- `crates/zorg-core`: semantic model, IDs, links, tags, properties, todos, diagnostics, and source spans.
- `crates/zorg-store`: SQLite persistence and query-facing read APIs over indexed zettel, files, tags, effective tags,
  properties, todos, links, text payloads, source order, and modified metadata.
- `crates/zorg-query`: SWOG parsing, planning, evaluation, and LIST result rendering.
- `crates/zorg-cli`: `zorg query` command wiring.

At the time this plan was written, the checked `../zorg` tree still has foundation stubs for `zorg-query` and
`zorg-store`, but earlier epic plans define the expected Epic 3 and Epic 4 handoff surfaces. Epic 5 agents should begin
only after Epic 4 exposes a stable query-facing store API. If an agent starts before that API exists, its first task is
to add only minimal adapter traits and tests in `zorg-query`, not to reimplement indexing.

## Non-Negotiables

- Query output is LIST only.
- Query input is SWOG MVP only.
- Whitespace between filters means logical AND.
- No OR groups, nested boolean expressions, TABLE output, aggregation, `count()`, custom functions, saved dot-snippets,
  `.zoq` files, or embedded query pragmas.
- Query zettel are ordinary `.z` zettel tagged `#z/query`.
- A query zettel may define its query using either a `query::` property or one fenced `swog` block.
- If a query zettel has both `query::` and a `swog` block, report an ambiguity error.
- Do not accept Python-era legacy query files or legacy syntax as compatibility input.
- Ordering must be deterministic even when the user does not request an explicit order.

## Proposed Phase Split

Use seven sequential phases. Each phase is intended for a distinct agent instance and should leave `../zorg` in a
passing state. The phase boundaries intentionally separate pure query language work from SQLite/store work and CLI UX.

## Phase 5.1: Query Contract, AST, and Parser

Purpose: make SWOG syntax explicit and testable without touching SQLite evaluation.

Primary ownership:

- `../zorg/crates/zorg-query/src/`
- `../zorg/crates/zorg-query/Cargo.toml`
- `../zorg/docs/query.md`
- query parser tests under `../zorg/crates/zorg-query/tests/` or module tests

Scope:

- Replace the `run_list_query` placeholder with a real query language module while keeping evaluation stubbed.
- Define public query types:
  - `Query`
  - `Filter`
  - `ComparisonOp`
  - `PropertyFilter`
  - `SpecialFieldFilter`
  - `NegatedFilter`
  - `TextFilter`
  - `SortSpec` only if explicit order syntax is documented in this phase
  - `QueryDiagnostic` or reuse `zorg-core` diagnostics if the shape is adequate
- Implement parser support for:
  - property equality: `foo:bar`
  - property comparison: `p:>3`, `due:<=2026-05-15`, `due:<=today`
  - property existence: `foo:*`
  - tags and type tags: `#z/todo`, `#area/work`
  - link filters: `links:#foo/bar`
  - file glob filters: `file:projects/*.z`
  - todo status: `todo:[ ]`, `todo:[N]`, `todo:[X]`, `todo:[?]`
  - negation: `-#z/inbox`, `-did:*`
  - text search: quoted phrases and `text:phrase`
  - relative modified ranges: `modified:<7d`, `modified:>=30d`
- Define lexical rules for quoting and escaping. Keep them conservative:
  - unquoted values end at whitespace
  - quoted strings may contain spaces
  - backslash escapes are supported only inside quoted strings if implemented
- Add explicit unsupported-feature errors for TABLE, aggregation, `count()`, OR markers, and parenthesized groups.
- Update `docs/query.md` with concrete syntax examples and the exact parser decisions made in this phase.

Acceptance:

- Parser tests cover every supported filter family and representative malformed inputs.
- Parser errors include byte offsets and display-friendly messages.
- Unsupported deferred features fail clearly rather than being parsed as property filters.
- `cargo fmt --check`, `cargo test -p zorg-query`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Later agents can depend on a stable AST and parser entry point such as
  `parse_query(&str) -> Result<Query, QueryError>`.
- No SQLite or CLI behavior is required yet.

## Phase 5.2: Query Semantics, Normalization, and Store Adapter

Purpose: define the bridge between parsed SWOG filters and Epic 4 store data before writing complex SQL.

Primary ownership:

- `../zorg/crates/zorg-query/src/`
- minor additions to `../zorg/crates/zorg-store/src/` only if Epic 4 read APIs are insufficient
- focused tests using in-memory fake store data where possible

Scope:

- Introduce a query-facing store trait or adapter in `zorg-query`, for example `QueryStore`, implemented by
  `zorg-store::Store`.
- Define a normalized filter representation suitable for evaluation:
  - known special fields: `links`, `file`, `todo`, `text`, `modified`
  - all other `key:value` expressions are property filters
  - tags query effective tags by default, while preserving a path for explicit-tag queries later if needed
- Define comparison typing:
  - numeric comparison for `p::` and numeric-looking property values
  - date comparison for lifecycle dates such as `do`, `due`, and `did`
  - time comparison for `start` and `end` if Epic 3/Epic 4 persisted time values cleanly
  - string equality for unknown properties
- Define slash-list property semantics:
  - equality matches the full raw value or any slash-separated segment when the stored property is list-like
  - comparisons apply only to scalar typed values
- Define relative date semantics:
  - `today` means the local date supplied by a `QueryContext`, not a direct system clock read in pure tests
  - `modified:<7d` means modified within the last seven days
  - `modified:>=30d` means modified at least thirty days ago
  - document any alternative if implementation constraints require a different interpretation
- Define deterministic default ordering as:
  - due/do date where the query references lifecycle/todo fields
  - source path
  - source order within the file
  - stable zettel store ID as final tie-breaker
- Add a `QueryContext` carrying root, current date/time, timezone policy if needed, and optional current zettel for
  future relative query work.

Acceptance:

- Unit tests prove normalization for each supported filter family.
- Unit tests prove date, numeric, string, slash-list, and relative-modified comparisons.
- `zorg-query` tests can run without a real SQLite database for normalization behavior.
- Any required store read API gaps are documented and covered by minimal tests in `zorg-store`.
- `cargo fmt --check`, `cargo test -p zorg-query`, `cargo test -p zorg-store`, and
  `cargo clippy --workspace --all-targets` pass.

Handoff:

- Later agents can implement evaluation against a typed `QueryPlan` or normalized query without revisiting parser
  decisions.

## Phase 5.3: SQLite-Backed Planner and Evaluator

Purpose: evaluate normalized SWOG filters against the indexed SQLite store.

Primary ownership:

- `../zorg/crates/zorg-query/src/`
- `../zorg/crates/zorg-store/src/` query adapter implementation
- integration tests using temporary indexed corpora

Scope:

- Implement a planner that translates normalized filters into SQLite-backed evaluation.
- Prefer a correct and maintainable MVP plan over a clever SQL optimizer:
  - candidate zettel set from `zettel`
  - joins or `EXISTS` subqueries over effective tags, properties, todos, links, files, text, and metadata
  - negation implemented with `NOT EXISTS` or set subtraction
  - deterministic ordering pushed to SQL where practical
- Support all required filter families:
  - property equality and slash-list matching
  - property comparisons
  - property existence
  - effective tag filters
  - resolved and unresolved link filters where Epic 4 stores both
  - file glob filters over root-relative paths
  - todo marker filters
  - text filters over stored body/title text
  - modified age/range filters over file metadata
- Return a rich internal result row:
  - zettel store ID
  - canonical ID when present
  - root-relative file path
  - title or first body line
  - todo marker when present
  - source order
  - relevant dates if needed for rendering/debug tests
- Make evaluator errors distinct from parser errors and store errors.

Acceptance:

- Integration tests create a temporary corpus, run `zorg db reindex`, and evaluate queries against that store.
- Tests cover positive and negative cases for every supported filter family.
- Tests prove negation works for tags, properties, todos, and links.
- Tests prove deterministic ordering across repeated runs.
- Tests prove unresolved links do not masquerade as resolved matches unless the query explicitly targets written link
  text.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Later agents can render and expose query results without changing evaluation semantics.

## Phase 5.4: LIST Result Model and Renderer

Purpose: turn query result rows into stable human-facing LIST output and golden-test it.

Primary ownership:

- `../zorg/crates/zorg-query/src/`
- golden tests for renderer output
- `../zorg/docs/query.md`

Scope:

- Replace the placeholder `ListRow { label: String }` with a useful result type if Phase 5.3 has not already done so.
- Implement LIST rendering with enough identity to navigate:
  - canonical ID when present
  - root-relative path
  - title or first meaningful body line
  - todo marker when present
- Choose one stable text format and document it. A compact MVP format is acceptable, for example:

```text
[ ] @project/plan  nested.z  Plan the next Zorg milestone.
    @minimal       minimal.z  Minimal fixture
```

- Keep terminal output deterministic and machine-testable.
- Add a structured result API separate from rendering so LSP or future Neovim commands can consume results without
  scraping terminal text.
- Add tests for missing IDs, long titles, todo markers, relative paths, and empty result sets.

Acceptance:

- Golden tests verify LIST output for representative fixture queries.
- Empty result output is intentional and documented, either no rows or a clear stable message.
- Renderer does not perform query evaluation or access SQLite directly.
- `cargo fmt --check`, `cargo test -p zorg-query`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- CLI integration can call a single high-level function that parses, evaluates, and renders.

## Phase 5.5: Inline `zorg query` CLI

Purpose: expose SWOG LIST queries as a practical command line workflow.

Primary ownership:

- `../zorg/crates/zorg-cli/src/main.rs`
- `../zorg/crates/zorg-cli/tests/`
- minor `zorg-query` API polish if needed

Scope:

- Implement `zorg query '<swog>'`.
- Support root and database overrides consistently with Epic 4 commands:
  - `--root PATH`
  - `--db PATH`
  - optional `--no-reindex` only if Epic 4 already established a convention
- Decide whether `zorg query` auto-opens an existing index only or performs a light reindex first. Recommended MVP:
  - require an existing index by default
  - produce a clear error suggesting `zorg db reindex` if no index exists
  - allow explicit auto-reindex later if the CLI design already supports it
- Print LIST output to stdout.
- Print parser/evaluator/store errors to stderr with nonzero exit codes.
- Keep shell quoting behavior simple: one query string argument for MVP.
- Add CLI integration tests using temporary roots and databases.

Acceptance:

- `cargo run -p zorg-cli -- query '#z/todo' --root <tmp-root> --db <tmp-db>` prints stable LIST rows after reindex.
- Parse errors exit nonzero and include a query position.
- Missing or stale database errors are actionable.
- CLI tests cover tags, property filters, todo filters, and empty results.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Users and later editor integrations have a stable inline query command.

## Phase 5.6: Query Zettel Discovery and Execution

Purpose: execute query definitions stored in ordinary `#z/query` zettel.

Primary ownership:

- `../zorg/crates/zorg-query/src/`
- `../zorg/crates/zorg-store/src/` only if query-zettel lookup helpers are missing
- `../zorg/crates/zorg-cli/src/main.rs`
- fixtures and integration tests

Scope:

- Implement query-zettel lookup by canonical zettel ID.
- Verify the target zettel has effective or explicit `#z/query`. Recommended MVP: require explicit `#z/query` on the
  zettel itself to avoid surprising inherited query behavior.
- Extract a query definition from:
  - one `query::` property
  - one fenced `swog` block
- Report errors for:
  - target ID not found
  - target zettel is not a query zettel
  - no query definition
  - multiple `query::` values
  - multiple fenced `swog` blocks
  - both `query::` and fenced `swog` present
  - parser errors within the extracted query, with source span mapped back to the `.z` file when possible
- Add CLI syntax for query IDs. Recommended:

```bash
zorg query --id @system/queries/today
```

- Preserve inline `zorg query '<swog>'` behavior unchanged.

Acceptance:

- CLI can execute `@system/queries/today` from `fixtures/corpus/query_and_template.z`.
- Tests cover property-defined and fenced-block query zettel.
- Tests cover every ambiguity/error case above.
- Errors for query zettel definitions include the zettel ID and source path.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets` pass.

Handoff:

- Epic 5 user-facing scope is feature-complete after this phase, pending hardening and docs.

## Phase 5.7: End-to-End Hardening, Docs, and Scope Guardrails

Purpose: close the epic by proving the whole query path works and that deferred features remain out of scope.

Primary ownership:

- `../zorg/docs/query.md`
- `../zorg/README.md` if it documents CLI usage
- `../zorg/fixtures/`
- cross-crate integration tests

Scope:

- Add or refine a query-focused fixture corpus if the shared fixtures are too small. Keep additions canonical `.z` files
  only.
- Add end-to-end tests covering:
  - parse
  - index
  - inline query
  - query by `#z/query` zettel ID
  - LIST rendering
  - unsupported legacy/deferred query forms
- Document:
  - supported SWOG filter syntax
  - examples for daily todo, inbox, due, modified, tag, link, file, and text queries
  - error behavior
  - default ordering
  - root/database expectations
  - explicit deferred features
- Add a small "query implementation notes" section if needed to explain effective tag matching, slash-list property
  matching, and relative modified ranges.
- Review crate APIs for accidental leakage of SQLite details into `zorg-core`.

Acceptance:

- One documented command sequence can index fixtures and run representative queries.
- Deferred features have explicit negative tests.
- Documentation and tests agree on syntax and output.
- Final verification passes:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p zorg-cli -- db reindex --root fixtures/corpus
cargo run -p zorg-cli -- query '#z/query' --root fixtures/corpus
```

## Cross-Phase Technical Guidance

- Keep `zorg-query` responsible for language semantics and result rendering.
- Keep `zorg-store` responsible for persistence and efficient retrieval.
- Keep `zorg-cli` thin: argument parsing, opening the store, calling query APIs, printing output, and mapping errors to
  exit codes.
- Use deterministic clocks in tests. Query code should accept a `QueryContext` date instead of reading the system clock
  directly.
- Avoid building a broad expression language. If syntax is not part of this MVP, reject it clearly.
- Prefer small integration corpora in temporary directories over tests that depend on the user's real `~/zorg`.
- Do not add `.zoq`, `.zot`, `.zo`, or `.zoc` positive fixtures.
- Do not make query zettel a separate file type or parser mode.

## Risks

- Store API churn could cause query agents to overreach into indexing internals. Mitigation: introduce a narrow
  `QueryStore` adapter in Phase 5.2.
- Date and modified-range semantics can become ambiguous. Mitigation: define them in tests before evaluator SQL grows.
- Tag inheritance can surprise users if inherited `#z/query` makes ordinary child zettel executable as queries.
  Mitigation: require explicit `#z/query` for query execution unless product docs decide otherwise.
- Text search can sprawl into ranking and full-text indexing. Mitigation: MVP only requires filtering and deterministic
  ordering; ranking can wait.
- LIST rendering can become unstable if it tries to be too pretty. Mitigation: choose a compact golden-tested format.

## Definition of Done

- `zorg-query` parses the SWOG MVP and rejects deferred features clearly.
- Queries evaluate against the Epic 4 SQLite store.
- Supported filters include properties, comparisons, existence, tags, links, files, todos, negation, text, and relative
  modified ranges.
- Results are deterministic and render as LIST output.
- `zorg query '<swog>'` works from the CLI.
- `zorg query --id @some/query` executes ordinary `#z/query` zettel definitions.
- Query errors carry useful inline positions or source `.z` spans.
- The implementation does not support legacy Python-era query files or compatibility syntax.

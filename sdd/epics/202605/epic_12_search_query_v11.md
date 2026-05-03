---
create_time: 2026-05-03 14:45:42
bead_id: zorg-4.3
tier: epic
legend_bead_id: zorg-4
status: done
prompt: sdd/prompts/202605/epic_12_search_query_v11.md
---
# Epic 12 Search And Query v1.1 Implementation Plan

## Context

This plan covers epic #3 from `sdd/legends/202605/zorg_next_features_without_dashboard.md`, which maps through the
document's dependency order to Epic 12: Search And Query v1.1.

The current baseline already has:

- Sync indexing through `zorg db reindex` and live updates through `zorg watch`.
- SQLite schema version `1`, including `text_index` but no FTS virtual table.
- `zorg query` over a store-backed snapshot with LIST text output only.
- Query zettel execution through `--id @query`, with `query::` and fenced `swog` definitions.
- Parser/evaluator support for implicit AND, negation, tags, properties, links, files, todos, text substring filters,
  and modified-age filters.
- Explicit rejection of `OR`, parentheses, `TABLE`, and `count()`.

The epic should keep Rust as the source of truth. Neovim support should consume the JSON/CLI contracts later in Epic 15;
this epic should not move query semantics into Lua and should not introduce dashboard behavior.

## Phase 1: Stable Query JSON Contract

Owner: one Rust query/CLI agent.

Primary files:

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-query/Cargo.toml`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`

Scope:

- Add `zorg query --json` and `zorg query --format json` for inline queries and `--id @query`.
- Define a versioned JSON envelope for LIST results, for example:
  - `schema_version`
  - `kind: "list"`
  - `query_source: "inline" | "zettel"`
  - optional query zettel metadata when running by ID
  - `rows`
  - optional `diagnostics`
- Include row fields needed by editor clients without parsing LIST text:
  - canonical ID, root-relative path, title, todo marker, source order, store row ID
  - source span and line/column if available from the store
  - tags and properties needed for display/filter refinement
- Preserve existing LIST output byte-for-byte unless a test intentionally updates only help text.
- Keep error behavior script-detectable:
  - normal text errors remain on stderr
  - JSON success always prints valid JSON, including empty results
  - JSON error output may remain deferred unless a small, consistent envelope can be added without widening the phase

Acceptance:

- `zorg query '#z/todo' --format json` returns valid JSON with stable list rows.
- `zorg query --id @some/query --json` returns valid JSON and includes query zettel metadata.
- Empty results return a valid JSON envelope with an empty `rows` array.
- Existing LIST query smoke tests still pass.
- Documentation names JSON as the machine-readable editor contract and LIST as the stable human renderer.

Validation:

- `cargo fmt --check`
- `cargo test -p zorg-query`
- `cargo test -p zorg-cli --test smoke zorg_query`

## Phase 2: SQLite FTS Schema And Store API

Owner: one Rust store agent.

Primary files:

- `crates/zorg-store/src/lib.rs`
- `crates/zorg-store/Cargo.toml`

Scope:

- Bump the schema from version `1` to version `2`.
- Add an FTS5-backed virtual table for zettel title/body/raw text.
- Keep the existing `text_index` table if it remains useful for compatibility, migration, or fallback.
- Populate the FTS table during fresh indexing and during incremental file reindex/delete paths.
- Add migration coverage from schema version `1`:
  - create a v1 database with existing `text_index` rows
  - open with the new store
  - verify schema version `2`
  - verify FTS rows are populated from existing rows
- Expose a query-facing store API that returns matching zettel IDs for a text query, plus optional rank/snippet/match
  metadata if practical.
- Add an explicit capability/error path for SQLite builds without FTS5. Prefer making local tests pass with the existing
  dependency set before adding bundled SQLite features.

Acceptance:

- Fresh databases create all v2 objects.
- Opening a v1 database migrates forward idempotently.
- Reindex create/update/delete keeps FTS rows synchronized with zettel rows.
- Store tests prove title, body, and raw text searches.
- Unknown future schema handling remains unchanged.

Validation:

- `cargo fmt --check`
- `cargo test -p zorg-store`

## Phase 3: FTS-Backed Text Query Evaluation

Owner: one Rust query agent.

Primary files:

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-query/Cargo.toml`
- `crates/zorg-store/src/lib.rs` only for narrow API adjustments
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`

Scope:

- Route quoted text filters and `text:` filters through the store FTS API when executing against `zorg_store::Store`.
- Preserve the generic snapshot evaluator for non-text filters and unit tests, but avoid loading every text body for the
  main Store path when FTS is available.
- Intersect FTS candidate IDs with structured filters deterministically.
- Preserve current semantics where documented:
  - case-insensitive matching
  - quoted phrase handling
  - deterministic LIST ordering
  - clear unsupported/error behavior if FTS cannot run
- Decide whether JSON rows expose text match metadata. If included, keep it small and versioned.

Acceptance:

- Existing SWOG tests pass.
- CLI text query tests prove title/body/raw matches and phrase behavior.
- Combined queries such as `file:query_focus.z text:"alpha text" #z/todo` work through the FTS path.
- LIST output remains stable.
- FTS unavailable behavior is explicit and tested if it can be simulated cheaply.

Validation:

- `cargo fmt --check`
- `cargo test -p zorg-query`
- `cargo test -p zorg-cli --test smoke zorg_query`

## Phase 4: Boolean Expression AST, OR, And Parentheses

Owner: one Rust query-parser/evaluator agent.

Primary files:

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`

Scope:

- Replace the flat `Query { filters: Vec<Filter> }` model with an expression model that can represent:
  - filter atoms
  - implicit AND
  - explicit `OR`, `|`, and/or `||` only if documented
  - unary negation
  - parenthesized grouping
- Preserve simple existing queries as the simplest expression form.
- Define precedence clearly:
  - parentheses first
  - unary negation
  - implicit AND
  - OR
- Update normalization and evaluation around expression trees without changing individual filter semantics.
- Keep `TABLE` and aggregation rejected in this phase.
- Update query zettel validation so stored OR/grouped queries work and invalid spans remain useful.

Acceptance:

- Existing simple query parse/normalize/evaluate tests pass.
- OR/grouping tests cover precedence, negation, malformed expressions, and query zettel definitions.
- Examples:
  - `#z/todo OR #z/query`
  - `(#z/todo OR #z/query) -did:*`
  - `#z/todo (#area/work OR #area/personal)`
- Deferred `TABLE` and `count()` syntax still fail clearly.

Validation:

- `cargo fmt --check`
- `cargo test -p zorg-query`
- `cargo test -p zorg-cli --test smoke zorg_query`

## Phase 5: TABLE Output Contract And Renderer

Owner: one Rust query renderer/CLI agent.

Primary files:

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`

Scope:

- Add a minimal non-dashboard TABLE query form after the boolean model is stable.
- Keep the syntax deliberately small. Recommended initial contract:
  - `TABLE <query expression>`
  - default columns: todo, id, file, title
  - no custom column expressions in this phase unless the local parser shape makes them low risk
- Render human text tables deterministically and add JSON support in the same envelope family as LIST:
  - `kind: "table"`
  - `columns`
  - `rows`
- Keep LIST output stable and keep existing LIST as default.
- Return useful errors for unsupported table features rather than accepting ambiguous syntax.

Acceptance:

- `zorg query 'TABLE #z/todo'` works.
- `zorg query 'TABLE (#z/todo OR #z/query)' --format json` works.
- Unsupported custom columns/functions produce explicit parser errors.
- LIST query behavior and output are unchanged.

Validation:

- `cargo fmt --check`
- `cargo test -p zorg-query`
- `cargo test -p zorg-cli --test smoke zorg_query`

## Phase 6: `count()` MVP And Final Epic Hardening

Owner: one Rust query renderer/docs agent.

Primary files:

- `crates/zorg-query/src/lib.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`
- `README.md`

Scope:

- Add minimal `count()` support only after TABLE and JSON result kinds are stable.
- Recommended contract:
  - `count(<query expression>)`
  - returns one aggregate result with a numeric count
  - JSON uses `kind: "aggregate"` with named values
  - text output is intentionally simple and script-friendly
- Do not add dashboard panels, saved dashboard queries, or broad aggregation language.
- Update docs and README examples to reflect Query v1.1.
- Run a full Rust validation pass and fix regressions in touched query/store/CLI code.

Acceptance:

- `zorg query 'count(#z/todo)'` returns the matching zettel count.
- `zorg query 'count(#z/todo OR #z/query)' --format json` returns a valid aggregate JSON envelope.
- Unsupported aggregations such as `sum(...)` still fail clearly.
- Query docs distinguish LIST, TABLE, and aggregate outputs with limitations.
- No dashboard behavior is introduced.

Validation:

- `python3 tools/check_fixture_manifest.py`
- `cargo fmt --check`
- `cargo test --workspace`
- `cargo test --workspace mvp_e2e`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Cross-Phase Rules

- Each phase must start by reading the current implementation because earlier phases may have changed contracts.
- Each phase should update docs and tests for the behavior it owns.
- Avoid unrelated refactors and keep compatibility with existing commands.
- Do not change parser input syntax outside SWOG query parsing.
- Keep `zorg.nvim` work out of this epic except for documenting the JSON contract it will consume later.
- If a phase cannot complete its full scope safely, it should leave a documented limitation and all touched tests passing.

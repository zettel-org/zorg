---
create_time: 2026-05-02 17:31:04
status: done
prompt: sdd/prompts/202605/zorg_epic4_store_indexing.md
bead_id: zorg-1.4
tier: epic
legend_bead_id: zorg-1
---
# Zorg Epic 4: SQLite Store and Incremental Indexing Plan

## Goal

Implement Epic 4 from `sdd/legends/202605/zorg_v1_mvp.md`: make the parsed Zorg zettel corpus persistent and queryable
through SQLite, with deterministic migrations, `.z` corpus discovery, incremental reindexing, delete/rename handling,
tag inheritance, and persisted link graph data.

Each phase below is intended for a distinct agent instance. Phases are ordered so each one leaves a stable handoff
surface for the next phase.

## Current State

- `crates/zorg-store` is a placeholder with `Store::open` returning `Unsupported`.
- `crates/zorg-core` already defines the semantic model: `ZettelDocument`, `Zettel`, IDs, local IDs, tags, properties,
  todos, links, resolved links, spans, and diagnostics.
- `crates/zorg-parse` already parses, validates, and resolves corpora in memory through
  `parse_zettel_document_with_path`, `validate_corpus`, and `resolve_corpus`.
- `crates/zorg-cli` has working `parse` and `check` commands, but database/index commands are placeholders.
- Fixtures under `fixtures/corpus` cover canonical `.z` examples, `dir/init.z`, nested zettel, query/template zettel,
  and unsupported legacy-looking input.

## Non-Negotiables

- Canonical source files are `.z`.
- Default root is `~/zorg`.
- Directory zettel are `init.z`.
- Do not accept or index `.zo`, `.zoq`, `.zot`, `.zoc`, or Python legacy syntax as compatible input.
- Store code must preserve source paths and spans well enough for later query and LSP work.
- Keep `zorg-store` responsible for persistence/indexing; do not move SQLite concerns into `zorg-core` or `zorg-parse`.

## Proposed Phase Split

### Phase 4.1: Store API, Path Policy, and SQLite Migrations

Purpose: replace the placeholder store with a real SQLite boundary and deterministic schema foundation.

Scope:

- Add SQLite dependencies to `zorg-store` using the repo's locked Rust workspace conventions.
- Define store configuration types:
  - corpus root path
  - database path
  - default root discovery for `~/zorg`
  - explicit root override
- Implement `Store::open` and an `open_with_options` style API that creates or opens the database and runs migrations.
- Add deterministic embedded migrations for the Epic 4 tables:
  - schema metadata/version table
  - files
  - zettel
  - zettel_ids or canonical ID lookup
  - links
  - tags
  - inherited/effective tags
  - properties
  - todos
  - text/body index storage
  - diagnostics
  - hashes/index metadata
- Keep the schema normalized enough for Epic 5 queries without overfitting SWOG before it exists.
- Add migration tests using temporary directories/databases.

Acceptance:

- `cargo test -p zorg-store` passes.
- Opening a store creates a SQLite DB with the expected schema version.
- Reopening an existing DB is idempotent.
- Empty root/database path errors are clear.
- No indexing behavior is required yet beyond opening and migrating.

Handoff:

- Later phases can depend on a concrete `Store`, `StoreOptions`, schema migration helper, and table names/columns.

### Phase 4.2: Corpus Discovery and CLI Database Command Skeleton

Purpose: make the store able to discover canonical source files and expose database commands without yet doing full
indexing.

Scope:

- Implement corpus discovery in `zorg-store`:
  - default root `~/zorg`
  - CLI-provided root override
  - recursive walk under the root
  - include ordinary `.z` files
  - include `init.z` directory zettel naturally as `.z`
  - stable deterministic ordering
  - ignore non-`.z` files during root discovery
- Add explicit-file validation helpers for later CLI usage:
  - accepted: `.z`
  - rejected/diagnosed when explicitly passed: `.zo`, `.zoq`, `.zot`, `.zoc`, or non-`.z`
- Add `zorg db` command skeleton in `zorg-cli`:
  - `zorg db status [--root PATH] [--db PATH]`
  - `zorg db reindex [--root PATH] [--db PATH]`
  - preserve the old `index` placeholder only as an alias or documented deferred command if desired
- For this phase, `status` may report root, DB path, schema version, and discovered file count.
- Add CLI tests for argument handling and discovery behavior with temporary roots.

Acceptance:

- `zorg db status --root <fixture-root> --db <tmp-db>` opens the store and reports deterministic discovered `.z` counts.
- Discovery ignores `.zo`, `.zoq`, `.zot`, `.zoc`, and unrelated files under a root.
- Explicit unsupported source paths produce clear diagnostics/errors.
- `cargo test --workspace` passes.

Handoff:

- Later phases can call a tested file discovery API and a stable CLI command path.

### Phase 4.3: Full Snapshot Indexing

Purpose: populate the SQLite store from parsed semantic documents in a clean full-reindex path before optimizing
incrementality.

Scope:

- Add an indexing pipeline in `zorg-store`:
  - discover files
  - read UTF-8 source
  - parse each file through `zorg-parse`
  - validate and resolve the corpus in memory
  - flatten each `ZettelDocument` into store rows
  - persist document-level and zettel-level diagnostics
- Persist the core query-facing semantic data:
  - file path, relative path, mtime, byte length, content hash
  - zettel kind, parser key, parent key/store ID, source order, title, canonical ID, local ID, spans
  - explicit tags and type tags
  - properties with key/value and spans
  - todo marker
  - body/text search payload sufficient for Epic 5 MVP text filters
  - unresolved links as written
  - resolved links with canonical target IDs when available
- Add query-facing read APIs for store consumers:
  - list files
  - list zettel
  - lookup by canonical ID
  - list links/tags/properties/todos
  - list diagnostics
- Make `zorg db reindex` perform a full snapshot rebuild transactionally.
- Prefer replacing existing indexed rows for the root in one transaction over partial writes.

Acceptance:

- Integration tests build a temporary corpus from canonical fixtures, run `reindex`, and assert persisted zettel, ID,
  tag, property, todo, text, link, and diagnostic rows.
- Resolved and unresolved links are distinguishable in the DB/API.
- A failed parse/read does not leave a partially updated index.
- `cargo test --workspace` passes.

Handoff:

- Later phases can optimize the snapshot pipeline without redesigning the persistence model.
- Epic 5 can begin using read APIs even before incremental indexing is complete.

### Phase 4.4: Incremental Reindex, Hashing, and Deletion Handling

Purpose: make repeated indexing practical by skipping unchanged files and cleaning stale rows.

Scope:

- Implement file-level hashing and index metadata:
  - content hash for each source file
  - mtime/size retained for status/debugging, but hash is authoritative for content equality
  - previous indexed timestamp/status
- Optionally implement note/zettel-level hashes if the flattened model makes this reliable without excess complexity.
- Update `zorg db reindex` to:
  - skip unchanged files
  - reparse and refresh changed files
  - add new files
  - remove rows for deleted files
  - treat renamed files as delete plus add unless a simple stable identity exists
- Add `zorg db status` detail:
  - indexed file count
  - discovered file count
  - changed/new/deleted counts before reindex
  - diagnostics count
  - last indexed timestamp if available
- Ensure incremental updates run inside transactions and leave the previous good index intact on failure where feasible.

Acceptance:

- Tests prove unchanged files are skipped across repeated `reindex` calls.
- Tests prove changed files refresh their zettel, tags, properties, links, text, and diagnostics.
- Tests prove deleted files remove all dependent zettel/link/tag/property/todo/text rows.
- Tests prove renamed files do not leave stale canonical IDs behind.
- `cargo test --workspace` passes.

Handoff:

- Later phases can rely on efficient reindex behavior and meaningful status output.

### Phase 4.5: Tag Inheritance and Graph Materialization

Purpose: complete Epic 4's semantic graph requirements by materializing effective tags and path/parent ancestry
relationships.

Scope:

- Define and implement inheritance rules in store code:
  - explicit tags remain separately queryable
  - effective tags include explicit tags plus inherited tags
  - parent-zettel ancestry contributes tags to child zettel
  - directory/path ancestry contributes tags from directory `init.z` and containing file zettel where applicable
- Persist ancestry/closure data if it materially simplifies Epic 5 queries:
  - parent-child edges
  - path ancestor relationships
  - effective tag rows with source/inheritance provenance
- Add read APIs:
  - explicit tags for zettel
  - effective tags for zettel
  - zettel descendants/ancestors where useful
  - outgoing and incoming links
- Extend `status` or a debug/test-only API to confirm graph materialization counts.

Acceptance:

- Tests distinguish explicit tags from inherited/effective tags.
- Tests cover nested parent inheritance, file-level inheritance, directory `init.z` inheritance, and mixed
  explicit/inherited tags.
- Tests confirm resolved and unresolved link graph rows survive incremental updates.
- `cargo test --workspace` passes.

Handoff:

- Epic 5 query implementation has stable APIs/tables for tags, properties, todos, links, source order, file globs,
  modified metadata, and text search.
- Epic 6 LSP implementation can use lookup, diagnostics, references, and graph APIs without reparsing the whole corpus
  itself.

## Cross-Phase Technical Guidance

- Use transactions for every multi-table index update.
- Keep the SQLite schema versioned and tested; do not rely on implicit table creation spread across indexing code.
- Keep parsing and semantic validation delegated to `zorg-parse`.
- Keep store rows source-span aware; later LSP features need byte and line/column ranges.
- Use deterministic ordering everywhere: file discovery, zettel flattening, query-facing reads, and test assertions.
- Treat unsupported legacy-looking files differently depending on context:
  - discovered under root: ignore non-`.z` files
  - explicitly passed to a source/indexing API: reject or diagnose clearly
  - legacy-looking content inside `.z`: preserve parser/validator diagnostics, never translate into legacy model fields
- Prefer focused integration tests around temporary corpora over brittle tests that depend on the user's actual
  `~/zorg`.

## Suggested Final Epic Verification

Run these from the repo root after Phase 4.5:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
cargo run -p zorg-cli -- db status --root fixtures/corpus
```

Also run an integration or smoke command against a temporary copied corpus, then modify, delete, and rename files before
a second `zorg db reindex` to verify incremental behavior.

## Risks

- Schema churn could block Epic 5 if the first schema only mirrors Rust structs. Mitigation: design query-facing tables
  in Phase 4.1 and add read APIs in Phase 4.3.
- Inheritance rules can become ambiguous across directory and nested zettel boundaries. Mitigation: document the exact
  rule in tests during Phase 4.5 before adding broad behavior.
- Incremental indexing can corrupt graph tables if deletes are incomplete. Mitigation: use foreign keys, cascading
  deletes where appropriate, and deletion-focused tests in Phase 4.4.
- Full corpus validation before incremental writes may become expensive later. Mitigation: implement correct snapshot
  behavior first; optimize only after file-level skipping is reliable.

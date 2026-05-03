---
create_time: 2026-05-03 00:25:39
status: done
prompt: sdd/prompts/202605/zorg_epic10_foundations.md
bead_id: zorg-4.1
tier: epic
legend_bead_id: zorg-4
---
# Implementation Plan: Zorg Roadmap Epic 1 Foundations

## Scope

Implement the first roadmap epic from `sdd/legends/202605/zorg_next_features_without_dashboard.md`, which is the first
item in the roadmap dependency order: **Epic 10: Foundations, Configuration, Validation, And Baselines**.

This plan targets the current Rust repo at `zorg_100`, with sibling validation against `../zorg-nvim` and
`../zorg-treesitter` where relevant. It deliberately does not implement live watching, FTS/query v1.1, refactoring,
import/export, dashboard features, or new editor UI behavior. The goal is to leave the repo ready for those later epics.

## Current Baseline

- `crates/zorg-store/src/lib.rs` already has `StoreOptions`, default `~/zorg` root behavior, default database path
  resolution, schema metadata, an embedded migration list, and basic migration idempotence tests.
- `crates/zorg-cli/src/main.rs` already routes `--root` and `--db` through `StoreOptions`; `zorg db status` already
  prints resolved root and database paths.
- `tools/validate_cross_repo.sh`, `docs/cross_repo.md`, and `docs/development.md` already exist and describe a broad
  validation gate.
- There is no documented config resolution contract, no root-local/user TOML config loader, no watcher-oriented config
  fields, no explicit v1 fixture migration test harness, and no deterministic large-corpus/performance tooling.

## Phase 1: Config Contract And Store Path Resolution

Owner: one Rust CLI/store agent.

Primary files:

- `crates/zorg-store/src/lib.rs`
- `crates/zorg-store/Cargo.toml`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/development.md`
- `README.md`

Work:

1. Introduce a resolved config type in `zorg-store` while keeping `StoreOptions` as the canonical path object consumed
   by existing store APIs.
2. Implement config precedence: CLI flags > environment variables > root-local `.zorg/config.toml` > user config >
   defaults.
3. Add config keys needed by later epics: `root`, `database_path`, `watcher_debounce_ms`, `watcher_log_path`, and an
   optional named-roots map if it can be kept simple and well tested.
4. Preserve current explicit `--root` and `--db` behavior exactly. Existing scripts that pass both flags must see the
   same paths and output shape.
5. Add CLI-facing error messages for invalid config, invalid paths, duplicate or malformed named roots, and unreadable
   config files.
6. Update `zorg db status` documentation to describe resolved path behavior. Do not add JSON output in this phase unless
   it falls out naturally and is fully tested.

Design notes:

- Prefer a small typed TOML parser path using normal Rust dependencies rather than ad hoc string parsing.
- Tests must not read the developer's real home directory. Inject isolated home/config paths through environment setup
  or helper APIs.
- If user config paths are platform-sensitive, document the chosen path and keep tests deterministic.

Acceptance:

- Existing CLI smoke tests pass unchanged except where they intentionally assert new config behavior.
- New tests prove precedence, default behavior, invalid config errors, and `--root`/`--db` override behavior.
- `zorg db status` remains line-oriented and script-friendly and still includes resolved root/database.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-store
cargo test -p zorg-cli zorg_db
cargo test --workspace
```

## Phase 2: Schema Migration Harness

Owner: one store-focused Rust agent.

Primary files:

- `crates/zorg-store/src/lib.rs`
- `crates/zorg-store/Cargo.toml`

Work:

1. Turn the existing embedded migration machinery into an explicit reusable migration test harness.
2. Add helper code in tests that creates a real schema-version-1 database fixture from SQL, reopens it through
   `Store::open_with_options`, and verifies it migrates to the current version without data loss.
3. Test fresh database creation, reopen idempotence, v1-to-current migration, non-integer schema metadata errors, and
   future schema refusal.
4. Keep migration execution deterministic and idempotent. Do not introduce a feature migration unless required to make
   the harness meaningful.
5. If Phase 1 added config helpers to `zorg-store`, preserve that public API and avoid mixing config tests with
   migration tests.

Design notes:

- Current `SCHEMA_VERSION` is `1`; a v1-to-current migration test may initially verify a no-op migration from an
  explicitly created v1 fixture. That is still valuable because later epics can extend the fixture and expected checks.
- Avoid storing binary SQLite fixtures unless there is a clear maintenance benefit. Inline SQL or a small test helper is
  easier to review.

Acceptance:

- Store tests cover fresh create, reopen, v1 fixture migration, corrupt schema metadata, and future schema handling.
- Migration tests document which tables and metadata must exist after opening a v1 database.
- No migration test depends on the user's real corpus or home directory.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-store
```

## Phase 3: Cross-Repo Validation And Docs Refresh

Owner: one docs/tools/cross-repo agent.

Primary files:

- `tools/validate_cross_repo.sh`
- `docs/cross_repo.md`
- `docs/development.md`
- `README.md`
- sibling docs as needed: `../zorg-nvim/README.md` `../zorg-treesitter/README.md`

Work:

1. Audit the existing validation script against the current roadmap acceptance criteria. Keep working behavior, but fix
   stale assumptions, missing prerequisite checks, brittle path handling, or output that hides the failing step.
2. Confirm that the script uses fixture roots and explicit temporary paths, never the user's real `~/zorg`.
3. Refresh README and development docs around the current v1 baseline: `db reindex`, `db status`, `query`, `check`,
   `fix`, `capture`, and `zorg-ls`.
4. Document the validation commands for Rust, Tree-sitter, and Neovim, including environment overrides for sibling repo
   locations.
5. Mention upcoming v1.1 work only as roadmap context. Do not promise dashboard support or plugin-only behavior.

Design notes:

- This phase should be mostly documentation/tooling. Do not change Rust behavior unless the script exposes a clear bug
  in the validation entry points.
- Because this phase can touch sibling repositories, it should report exactly which repositories changed.

Acceptance:

- `tools/validate_cross_repo.sh --help` is accurate.
- The validation script fails early with clear messages when required sibling repos or tools are unavailable.
- Docs describe the current baseline and the validation gate without stale command names.

Validation:

```sh
tools/validate_cross_repo.sh --help
python3 tools/check_fixture_manifest.py
cargo test --workspace
```

Run the full cross-repo gate if local dependencies are available:

```sh
tools/validate_cross_repo.sh
```

## Phase 4: Large-Corpus Generator And Performance Baseline

Owner: one Rust tooling/performance agent.

Primary files:

- `tools/`
- `crates/zorg-store/src/lib.rs`
- `docs/development.md`

Work:

1. Add a deterministic synthetic corpus generator under `tools/` that can create hundreds or thousands of `.z` files in
   a caller-provided output directory.
2. Include realistic enough content to exercise IDs, nested zettel, tags, properties, todo markers, links, and queryable
   body text.
3. Add a documented benchmark-like command for `zorg db reindex` throughput and status freshness checks. It may be a
   shell script, Rust example, or test harness, but exact wall-clock timings must not become a brittle CI requirement.
4. Add focused store tests only for deterministic generator invariants or incremental indexing behavior that is
   currently untested and low-flake.
5. Document how later watcher and FTS phases should use the generated corpus as a regression target.

Design notes:

- The generator should be deterministic from explicit inputs such as file count, zettel-per-file count, and seed.
- It must refuse unsafe output paths and should be easy to run under `/tmp`.
- Prefer line-oriented output with counts and elapsed time so later agents can compare results manually or in CI logs.

Acceptance:

- Developers can generate a large corpus under a temp root with one documented command.
- A documented command reports reindex throughput and status cost.
- The generated corpus passes `zorg check`, can be indexed by `zorg db reindex`, and is queryable.

Validation:

```sh
cargo fmt --check
cargo test --workspace
python3 tools/check_fixture_manifest.py
# plus the new documented generator/performance command on a small corpus
```

## Phase 5: Foundation Integration Gate

Owner: one final integration agent after Phases 1-4 land.

Primary files:

- any touched docs/tests from Phases 1-4
- `sdd/legends/202605/zorg_next_features_without_dashboard.md` only if status annotations are desired by the maintainer

Work:

1. Rebase mentally across the distinct phase outputs and remove accidental inconsistencies in docs, command names,
   config names, and validation instructions.
2. Verify all Epic 10 acceptance criteria from the roadmap are either implemented or explicitly deferred with a reason.
3. Run the normal Rust validation set and the full cross-repo validation gate if local dependencies are installed.
4. Produce a short handoff note for Epic 11 agents describing stable config keys, migration harness conventions, and
   performance baseline commands.

Acceptance:

- The repo has one coherent config story, one coherent validation story, and one documented performance baseline story.
- All touched Rust crates pass formatting, tests, and clippy where available.
- Later watcher/query/refactor/import agents can build on this foundation without guessing path/config/migration
  conventions.

Validation:

```sh
python3 tools/check_fixture_manifest.py
cargo fmt --check
cargo test --workspace
cargo test --workspace mvp_e2e
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p zorg-cli -- --help
cargo run -p zorg-ls -- --version
tools/validate_cross_repo.sh
```

## Suggested Agent Order

Run Phase 1 first because it defines the config and path contract that later docs and tools should reference. Run Phase
2 after Phase 1 because both touch `zorg-store/src/lib.rs` and test helpers. Phase 3 and Phase 4 can run after Phase 1,
but they should avoid editing the same doc sections simultaneously. Run Phase 5 only after the first four phases are
merged or otherwise available in the same worktree.

## Non-Scope For All Phases

- Do not implement `zorg watch`, LSP refresh, FTS, boolean query parsing, refactoring commands, import/export, or any
  dashboard feature.
- Do not accept legacy `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`, `LID::`, or `tick::` syntax as normal parser input.
- Do not move Zorg semantics into Lua or sibling editor code.
- Do not make CI depend on exact performance timings.

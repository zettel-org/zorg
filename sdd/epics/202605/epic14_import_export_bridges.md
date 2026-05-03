---
plan_name: epic14_import_export_bridges
bead_id: zorg-4.5
tier: epic
legend_bead_id: zorg-4
recommended_epic_bead_id: zorg-4.5
source: sdd/legends/202605/zorg_next_features_without_dashboard.md
create_time: 2026-05-03 17:29:16
status: done
prompt: sdd/prompts/202605/epic14_import_export_bridges.md
---

# Epic 14 Import And Export Bridges Implementation Plan

## Scope Decision

This plan covers epic #5 from `sdd/legends/202605/zorg_next_features_without_dashboard.md`, interpreted through the
document's dependency order as **Epic 14: Import And Export Bridges**.

The prior child epics are already represented as:

- `zorg-4.1`: Epic 10 Foundations.
- `zorg-4.2`: Epic 11 Live Workspace Indexing.
- `zorg-4.3`: Epic 12 Search And Query v1.1.
- `zorg-4.4`: Epic 13 Zettel Refactoring Commands.

The next parent should be `zorg-4.5`, with one implementation bead per phase below.

## Current Baseline

Inspection of the current checkout shows:

- Canonical `.z` remains the only normal source format. Store discovery ignores non-`.z` files, and explicit source path
  validation rejects `.zo`, `.zoq`, `.zot`, `.zoc`, missing extensions, and unrelated extensions.
- Parser/model/store/query surfaces already preserve zettel IDs, local IDs, tags, properties, todos, links, body text,
  parent IDs, source order, file paths, source spans, and diagnostics.
- `zorg query` has JSON, TABLE, `count()`, query-by-ID, FTS-backed text search, and deterministic row metadata.
- `zorg-refactor` has preview/write planning infrastructure, source guards, non-overlapping byte edits, temp-file style
  write helpers, and CLI JSON precedent for editor-facing contracts.
- The fixture manifest is enforced by `tools/check_fixture_manifest.py`, and current fixture policy says legacy-looking
  syntax is negative normal input.
- There is no import/export bridge crate or module, no `zorg import ...`, no `zorg export ...`, and no
  `docs/import_export.md`.

This epic should add adoption and exit paths without weakening the normal parser. Legacy syntax is import input only;
Markdown is export output only.

## Product Goal

Users should be able to:

- preview what legacy Python-era Zorg files would become before writing anything;
- write deterministic canonical `.z` output only after an explicit apply/write flag;
- see lossy, unsupported, collision, and overwrite decisions in stable human and JSON output;
- export canonical `.z` zettels to Markdown for a single zettel, a subtree, or a query result set;
- run the resulting imported `.z` through `zorg check`, `zorg db reindex`, and `zorg query` like any other canonical
  corpus.

## Non-Goals

- Do not accept `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`, `LID::`, or `tick::` in normal `zorg parse`, `zorg check`, store
  indexing, Tree-sitter grammar, or Neovim runtime behavior.
- Do not preserve hidden compatibility metadata, sidecar databases, generated `.zoc` state, or Python-era query/template
  files as active sources.
- Do not build a general Markdown importer.
- Do not attempt a perfect historical converter. Prefer deterministic best-effort output plus explicit lossy/unsupported
  diagnostics.
- Do not implement Neovim wrappers in this epic. Epic 15 should consume stable CLI/JSON contracts from this plan.

## Phase 14A: Bridge Specs, Fixtures, And Manifest Contract

Owner: one docs/fixtures agent.

Touched repo: main Rust Zorg checkout. Cross-repo edits should be avoided unless the fixture manifest requires a
downstream note.

Likely files:

- `docs/import_export.md`
- `docs/syntax.md`
- `docs/development.md`
- `README.md`
- `fixtures/import_export/`
- `fixtures/manifest.json`
- `tools/check_fixture_manifest.py` only if the manifest schema must distinguish import-only fixtures

Scope:

- Write the bridge contract before implementation:
  - supported legacy import inputs: `.zo`, `.zoq`, `.zot`, selected inline legacy markers such as `ID::`, `LID::`, and
    `tick::`;
  - unsupported legacy inputs: generated/cache `.zoc`, custom legacy constructs with no deterministic `.z` mapping, and
    any ambiguous form discovered while creating fixtures;
  - import planning vs write/apply behavior;
  - overwrite/collision policy;
  - lossy and unsupported diagnostic shape;
  - Markdown export mapping for IDs, tags, properties, todos, body text, code fences, and Zorg links.
- Add import-only fixture inputs and expected outputs:
  - at least one `.zo` note with `ID::`, tags, properties, body, absolute links, and `tick::`;
  - one `.zoq` query input that maps to ordinary `#z/query` zettel syntax;
  - one `.zot` template input that maps to ordinary `#z/tmpl` zettel syntax or is explicitly marked unsupported if there
    is not enough stable template syntax;
  - collision and unsupported/lossy fixture cases.
- Keep fixtures out of the normal `fixtures/corpus/**/*.z` accepted corpus unless they are canonical expected output.
  Import-only legacy files should live in a separate path and be marked as import fixtures, not parser/store fixtures.
- Update the manifest/check tool so import-only legacy fixture files are tracked without relaxing normal `.z` fixture
  policy.

Acceptance:

- Docs state clearly that legacy syntax is not accepted by normal parsing/indexing.
- Import-only fixtures cover `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, and `tick::`.
- Expected canonical `.z` outputs and expected Markdown outputs are present where deterministic.
- The fixture manifest check passes and still prevents accidental legacy fixtures from becoming accepted parser/store
  corpus input.

Validation:

```sh
python3 tools/check_fixture_manifest.py
```

## Phase 14B: Bridge Crate Foundation And Legacy Import Planner

Owner: one Rust bridge/import library agent.

Touched repo: main Rust Zorg checkout.

Likely files:

- `Cargo.toml`
- new `crates/zorg-bridge/Cargo.toml`
- new `crates/zorg-bridge/src/lib.rs`
- optional focused modules such as `legacy.rs`, `render_z.rs`, `diagnostics.rs`
- `crates/zorg-core/src/lib.rs` only for narrowly shared diagnostic/type additions
- fixtures from Phase 14A

Scope:

- Add a dedicated `zorg-bridge` crate for import/export bridge behavior. Avoid putting legacy parsing into `zorg-parse`;
  the canonical parser must remain legacy-free.
- Define stable import data types with serde support:
  - `ImportPlan`, `ImportInput`, `ImportOutput`, `BridgeDiagnostic`, `BridgeDiagnosticKind`, `BridgeSeverity`,
    `Lossiness`, and collision records;
  - normalized destination paths, root-relative paths, planned generated content, and source provenance;
  - schema version for JSON output.
- Implement a deterministic legacy import planner over explicit paths:
  - accepts files and directories passed to the bridge API;
  - recognizes `.zo`, `.zoq`, `.zot`, and legacy inline markers from the import fixtures;
  - produces canonical `.z` source text using LF line endings;
  - records unsupported/lossy cases instead of silently dropping them;
  - detects duplicate target IDs, duplicate output paths, existing destination paths if a root is provided, and invalid
    generated `.z` through reparsing.
- Keep write behavior out of this phase. The library returns plans only.
- Unit/golden tests should compare fixture legacy inputs to expected `.z` plan outputs and expected diagnostics.

Acceptance:

- `zorg-bridge` compiles as part of the workspace.
- Library tests cover successful `.zo` planning, `.zoq` query planning, `.zot` template planning or explicit unsupported
  reporting, marker conversion for `ID::`, `LID::`, and `tick::`, collisions, unsupported forms, and invalid generated
  `.z`.
- Generated canonical output passes `zorg_parse` validation in tests when no fatal diagnostics are present.
- No CLI command is required yet.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-bridge
cargo test --workspace
```

## Phase 14C: `zorg import legacy plan`

Owner: one Rust CLI/import contract agent.

Touched repo: main Rust Zorg checkout.

Likely files:

- `crates/zorg-bridge/src/lib.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/Cargo.toml`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/import_export.md`
- `README.md`

Scope:

- Add the read-only CLI surface:

```sh
zorg import legacy plan PATH... [--root ROOT] [--dest DEST] [--json|--format json]
```

- Keep `plan` as the only supported import subcommand in this phase.
- Human output should be concise and deterministic: input path, planned output path, status, and diagnostic summaries.
- JSON output should expose the full `ImportPlan` envelope from Phase 14B, including schema version, command, inputs,
  outputs, diagnostics, collisions, and lossy/unsupported counts.
- Exit behavior should be scriptable:
  - `0` when planning succeeds without fatal diagnostics;
  - `1` when inputs are readable but the plan has fatal diagnostics;
  - `2` for CLI usage errors.
- Do not write files, reindex, or mutate hidden state.

Acceptance:

- Planning works for one file, multiple files, and directories with deterministic ordering.
- `--format json` prints valid JSON for success, lossy success, and fatal-plan cases.
- Existing parser/store behavior still rejects legacy files as normal source input.
- CLI help and docs identify `plan` as safe/read-only.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-bridge
cargo test -p zorg-cli --test smoke import_legacy_plan
```

## Phase 14D: Import Write/Apply Mode

Owner: one Rust import write-safety agent.

Touched repo: main Rust Zorg checkout.

Likely files:

- `crates/zorg-bridge/src/lib.rs`
- optional `crates/zorg-bridge/src/apply.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/import_export.md`

Scope:

- Add explicit write/apply mode:

```sh
zorg import legacy apply PATH... [--root ROOT] [--dest DEST] [--json|--format json]
```

or, if the CLI stays cleaner:

```sh
zorg import legacy plan PATH... --write [--root ROOT] [--dest DEST] [--json|--format json]
```

The phase agent should choose one surface and document it; avoid supporting two spellings unless the extra compatibility
is trivial.

- Write only canonical `.z` files. Never write `.zo`, `.zoq`, `.zot`, `.zoc`, hidden compatibility files, or sidecars.
- Refuse overwrites by default. Add `--force` or `--replace` only with documented semantics, and require tests for it.
- Apply writes using a guarded, deterministic strategy:
  - create parent directories under the destination/root;
  - prepare temp files before final rename where practical;
  - report exactly which files were written if a partial failure cannot be made impossible;
  - never leave unreported hidden state.
- After writing, parse and strict-check generated `.z` files. If a store root/database is supplied, add a smoke path
  that reindexes a temp root and proves imported output is queryable.
- JSON output should include the same plan envelope plus write results.

Acceptance:

- Successful apply writes the expected `.z` outputs and nothing else.
- Overwrites and destination collisions are refused unless the documented force/replace flag is used.
- Failed imports either write nothing or report the exact written set.
- Written output passes `zorg check`, `zorg db reindex`, and at least one relevant `zorg query` in temp-root smoke
  tests.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-bridge
cargo test -p zorg-cli --test smoke import_legacy_apply
cargo test --workspace
```

## Phase 14E: Markdown Export Core Renderer

Owner: one Rust export library agent.

Touched repo: main Rust Zorg checkout.

Likely files:

- `crates/zorg-bridge/src/lib.rs`
- optional `crates/zorg-bridge/src/markdown.rs`
- `crates/zorg-store/src/lib.rs` only for narrow read API additions
- fixture expected Markdown outputs
- `docs/import_export.md`

Scope:

- Add export data types with serde support:
  - `ExportPlan`, `ExportItem`, `ExportDiagnostic`, `ExportTarget`, `MarkdownRenderOptions`, and schema version.
- Implement canonical `.z` to Markdown rendering from parsed/indexed data, not from ad hoc legacy syntax:
  - heading/title mapping;
  - canonical IDs rendered predictably, for example as an HTML anchor or visible metadata line;
  - tags/properties/todo markers rendered in a documented compact form;
  - body text and fenced code preserved with Markdown-safe fence handling;
  - child zettels rendered as nested headings or list sections according to docs.
- Implement documented lossy mapping for links:
  - absolute `#id` links should become Markdown links when the target is part of the export set, otherwise a stable text
    fallback;
  - `+child`, `~sibling`, and `^local` should resolve through indexed semantics when available and otherwise produce a
    lossy diagnostic;
  - unresolved links must not be silently rewritten into broken Markdown links.
- Build golden tests against Phase 14A expected Markdown fixture outputs.
- Do not add the user-facing CLI in this phase except small private test helpers if needed.

Acceptance:

- Renderer tests cover single zettel, nested children, properties/tags/todos, code fences, resolved absolute links,
  child/sibling/local links, unresolved links, and lossy diagnostics.
- Export plans are deterministic.
- Export rendering never mutates source `.z` files.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-bridge markdown
cargo test -p zorg-store
```

## Phase 14F: `zorg export markdown` CLI Selectors And Output Modes

Owner: one Rust export CLI/query/store agent.

Touched repo: main Rust Zorg checkout.

Likely files:

- `crates/zorg-bridge/src/lib.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/import_export.md`
- `docs/query.md` only for a small export-query cross-reference
- `README.md`

Scope:

- Add the user-facing export command:

```sh
zorg export markdown --id @id [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
zorg export markdown --subtree @id [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
zorg export markdown --query '<swog>' [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
zorg export markdown --query-id @queries/foo [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
```

- Require a current index using the same stale/missing checks as query/refactor commands.
- Selection semantics:
  - `--id`: one zettel only unless renderer options intentionally include children;
  - `--subtree`: target plus descendants using store ancestry APIs;
  - `--query`: query result rows in deterministic query order;
  - `--query-id`: stored `#z/query` execution.
- Output semantics:
  - stdout mode emits one Markdown document when the selection is one item, or a deterministic concatenated document
    with separators for multi-item exports;
  - output-directory mode writes one `.md` file per exported zettel or a documented tree layout derived from IDs/paths;
  - default should be conservative, preferably stdout for one item and requiring `--out` for multi-item exports if
    ambiguity would otherwise be high.
- JSON output should include selection metadata, written paths or stdout item count, diagnostics, and lossy counts. It
  should not embed huge Markdown bodies unless an explicit flag is added.
- Export must never mutate source `.z` files.

Acceptance:

- CLI smoke tests cover single ID export to stdout, subtree export, inline query export, query-by-ID export, output
  directory writes, unresolved links, and lossy diagnostics.
- Query-result export preserves query order.
- Missing/stale index, invalid IDs, empty query results, output path collisions, and unsupported option combinations
  produce clear errors.
- Existing `zorg query`, `zorg path`, and refactor tests still pass.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-bridge
cargo test -p zorg-cli --test smoke export_markdown
cargo test --workspace
```

## Phase 14G: Epic Hardening, Docs, And Cross-Repo Handoff

Owner: one Rust/docs/validation agent.

Touched repos:

- Main Rust Zorg checkout.
- `../zorg-nvim` only for README/help notes if the local cross-repo docs explicitly require a handoff reference.
- `../zorg-treesitter` should not need implementation changes unless Phase 14A changed shared fixture rules.

Likely files:

- `docs/import_export.md`
- `docs/cross_repo.md`
- `docs/development.md`
- `docs/release.md`
- `README.md`
- `tools/validate_cross_repo.sh`
- `fixtures/manifest.json`
- `sdd/epics/202605/` if the plan needs to be copied into the long-term SDD location

Scope:

- Reconcile all help text, README examples, and docs with the final chosen CLI spellings.
- Add import/export checks to the cross-repo validation command without making normal parser/indexing accept legacy
  sources.
- Ensure fixture manifest policy remains clear:
  - import-only legacy fixtures are tracked and tested by `zorg-bridge`;
  - normal accepted corpus fixtures remain canonical `.z`;
  - negative legacy-looking `.z` examples still fail strict checks as before.
- Add an Epic 15 handoff section naming the stable Neovim-facing commands:
  - import plan/apply JSON;
  - export markdown selectors and JSON;
  - expected diagnostics/warnings fields.
- Run full validation and fix regressions in touched import/export/docs surfaces.

Acceptance:

- All docs agree on command names, flags, JSON schema versions, and non-scope.
- Full workspace validation passes.
- Cross-repo validation either exercises import/export directly or documents why those checks remain Rust-local.
- No dashboard behavior or Lua-side semantics are introduced.

Validation:

```sh
cargo fmt --check
cargo test --workspace
python3 tools/check_fixture_manifest.py
./tools/validate_cross_repo.sh
```

## Dependency Graph

- Phase 14A is first and should complete before implementation work.
- Phase 14B depends on 14A.
- Phase 14C depends on 14B.
- Phase 14D depends on 14C.
- Phase 14E depends on 14A and can begin after 14B if import CLI work is delayed, but it should not change import plan
  types without coordinating with 14C/14D.
- Phase 14F depends on 14E and the existing query/store surfaces from Epic 12.
- Phase 14G depends on 14D and 14F.

Recommended bead dependencies:

- `zorg-4.5.1`: Phase 14A.
- `zorg-4.5.2`: Phase 14B, depends on `zorg-4.5.1`.
- `zorg-4.5.3`: Phase 14C, depends on `zorg-4.5.2`.
- `zorg-4.5.4`: Phase 14D, depends on `zorg-4.5.3`.
- `zorg-4.5.5`: Phase 14E, depends on `zorg-4.5.1` and `zorg-4.5.2`.
- `zorg-4.5.6`: Phase 14F, depends on `zorg-4.5.5`.
- `zorg-4.5.7`: Phase 14G, depends on `zorg-4.5.4` and `zorg-4.5.6`.

## Cross-Phase Interface Rules

- `zorg-bridge` owns all import/export bridge semantics.
- `zorg-parse` must not learn legacy syntax.
- `zorg-store` should only receive narrow read APIs needed for export selectors or link resolution.
- `zorg-query` should not render Markdown; export query selection may call existing query execution and pass rows to the
  bridge crate.
- `zorg-refactor` write helpers may be reused if the APIs fit, but import/export should not couple to structural
  refactor operation names.
- CLI JSON envelopes must be versioned from their first implementation.

## Risks And Mitigations

- Risk: legacy import accidentally becomes parser compatibility. Mitigation: keep all legacy parsing inside
  `zorg-bridge`, keep fixture paths import-only, and retain strict rejection tests.
- Risk: import writes partial output on failure. Mitigation: plan first, reject fatal diagnostics before writes, prepare
  files before final rename, and report exact write results.
- Risk: Markdown export hides lossy link semantics. Mitigation: every unresolved or relative-link fallback emits a
  diagnostic surfaced in text and JSON output.
- Risk: output paths collide across legacy inputs or export selections. Mitigation: deterministic path derivation plus
  explicit collision records before any write.
- Risk: phase agents duplicate CLI JSON shapes. Mitigation: Phase 14B defines shared serde plan types; Phase 14C/14D and
  Phase 14F wrap them instead of inventing unrelated envelopes.

## Suggested Agent Prompts

1. "Implement Phase 14A from `sase_plan_epic14_import_export_bridges.md`: bridge specs, import/export fixtures, and
   fixture manifest policy only. Do not add Rust import/export behavior yet."
2. "Implement Phase 14B from `sase_plan_epic14_import_export_bridges.md`: add the `zorg-bridge` crate and legacy import
   planner library, with golden tests, but no CLI write behavior."
3. "Implement Phase 14C from `sase_plan_epic14_import_export_bridges.md`: add read-only `zorg import legacy plan` human
   and JSON CLI output over the Phase 14B planner."
4. "Implement Phase 14D from `sase_plan_epic14_import_export_bridges.md`: add explicit legacy import write/apply mode,
   guarded writes, overwrite refusal, and temp-root check/reindex/query smoke coverage."
5. "Implement Phase 14E from `sase_plan_epic14_import_export_bridges.md`: add Markdown export renderer/plans in
   `zorg-bridge`, including link mapping diagnostics and golden tests, without adding the final CLI selector surface."
6. "Implement Phase 14F from `sase_plan_epic14_import_export_bridges.md`: add `zorg export markdown` selectors for ID,
   subtree, inline query, and query ID, with stdout/output-directory modes and JSON diagnostics."
7. "Implement Phase 14G from `sase_plan_epic14_import_export_bridges.md`: harden docs, help text, fixture policy,
   cross-repo validation, and Epic 15 handoff for import/export."

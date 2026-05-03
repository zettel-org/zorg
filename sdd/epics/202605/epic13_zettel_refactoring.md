---
plan_name: epic13_zettel_refactoring
bead_id: zorg-4.4
tier: epic
legend_bead_id: zorg-4
recommended_epic_bead_id: zorg-4.4
source: sdd/legends/202605/zorg_next_features_without_dashboard.md
create_time: 2026-05-03 15:56:41
status: done
prompt: sdd/prompts/202605/epic13_zettel_refactoring.md
---

# Epic 13 Zettel Refactoring Commands Implementation Plan

## Scope Decision

This plan covers epic #4 from `sdd/legends/202605/zorg_next_features_without_dashboard.md`, interpreted through the
document's dependency order as **Epic 13: Zettel Refactoring Commands**. The prior child epics are already represented
as:

- `zorg-4.1`: Epic 10 Foundations, closed.
- `zorg-4.2`: Epic 11 Live Workspace Indexing, closed.
- `zorg-4.3`: Epic 12 Search And Query v1.1, closed.

The next parent should be `zorg-4.4`, with one implementation bead per phase below.

## Current Baseline

Inspection of the current `../zorg` code shows:

- `zorg-fix` owns span-backed edit planning for document-local fix operations, with common `FixPlan`, `FixEdit`, and
  `apply_plan_to_source` semantics.
- `zorg-ls` already has rename support in `crates/zorg-ls/src/rename.rs`, including declaration/reference collection,
  duplicate-ID rejection, relative-link rewrite checks, and `WorkspaceEdit` output.
- `zorg-ls` navigation keeps useful symbol/reference metadata in a private `LspIndex`; that logic is not reusable by a
  CLI refactor command yet.
- `zorg-store` records indexed zettel source spans, canonical IDs, parent IDs, source order, links, tags, properties,
  todos, and file paths.
- `zorg-query` now has JSON output, source spans, boolean expressions, TABLE, FTS-backed text matching, and `count()`,
  so `zorg path` / `zorg open` can share query-facing row data where useful.
- There is no `zorg-refactor` crate/module, no CLI `path`/`open`, `promote`, `move`, or `extract` commands, and no LSP
  code actions for structural refactors.

The implementation should keep Rust as the only semantic rewrite authority. `zorg.nvim` work belongs to the later Neovim
integration epic and should consume stable JSON/CLI contracts from this epic.

## Product Goal

Users should be able to locate, preview, and safely perform structural zettel edits from the Rust CLI, with
deterministic JSON contracts for editor clients. Refactors must preserve source text where required, update links only
when spans make that deterministic, and refuse ambiguous or unsafe rewrites.

## Non-Goals

- Do not implement import/export bridges, dashboard behavior, or Neovim command wrappers in this epic.
- Do not move rewrite semantics into Lua or into LSP-only code.
- Do not accept legacy `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, or `tick::` as normal parser input.
- Do not build a general formatter. Refactor commands should preserve user text unless a command explicitly documents a
  small generated wrapper such as a destination zettel opening.
- Do not perform source rewrites from stale or incomplete index data. Reparse source files before planning writes.

## Phase 13A: Shared Refactor Planning Foundation

Owner: one Rust refactor/fix/LSP architecture agent.

Touched repo: `../zorg`.

Likely files:

- `Cargo.toml`
- new `crates/zorg-refactor/Cargo.toml`
- new `crates/zorg-refactor/src/lib.rs`
- `crates/zorg-fix/src/plan.rs` only for small shared helper extraction if needed
- `crates/zorg-ls/src/rename.rs` only for narrow reuse/adaptation
- `docs/development.md`

Scope:

- Add a small Rust refactor boundary, preferably a new `zorg-refactor` crate, for multi-file structural edit planning.
- Define common public types:
  - `RefactorPlan` with operation name, mode, root, target ID, warnings, rejections, files, and ordered edits.
  - `RefactorFilePlan` with absolute path, root-relative path, original content hash or mtime/length guard, and edits.
  - `RefactorEdit` with byte span, line/column span, replacement, and optional label.
  - `RefactorPreview` / JSON serialization shape that the CLI and later Neovim wrappers can consume.
  - `RefactorMode`: check/preview/write, with writes requiring explicit non-preview flags.
- Implement generic helpers for:
  - loading and reparsing indexed source files from `StoreOptions`;
  - resolving exactly one canonical zettel by ID;
  - extracting source slices by span;
  - validating in-bounds, non-overlapping edits per file;
  - applying multi-file edits atomically enough for CLI use: write temp files and rename, refuse if source guards
    changed.
- Move or copy the LSP rename relative-reference replacement rules into the shared refactor crate in a form usable by
  CLI refactors. Keep LSP rename behavior unchanged in this phase except for calling shared helpers if low risk.
- Document the initial JSON preview contract and safety invariants.

Acceptance:

- `zorg-refactor` compiles as part of the workspace.
- Unit tests cover non-overlapping edit validation, out-of-bounds rejection, deterministic edit ordering, source guard
  mismatch rejection, and JSON preview serialization.
- Existing `zorg-fix`, `zorg-cli`, and `zorg-ls` tests still pass.
- No user-facing refactor command is required yet.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor
cargo test -p zorg-ls rename
cargo test --workspace
```

## Phase 13B: `zorg path` / `zorg open`

Owner: one Rust CLI/query/store agent.

Touched repo: `../zorg`.

Likely files:

- `crates/zorg-refactor/src/lib.rs` or a focused lookup module
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/query.md`
- `README.md`

Scope:

- Add `zorg path @id` and accept `zorg open @id` as an alias if the CLI surface stays simple.
- Resolve the target from the configured store and print the source file plus line/column for the zettel opening when
  available.
- Support `--root`, `--db`, `--json`, and `--format json`.
- JSON output should include:
  - `schema_version`
  - `command: "path"` or `"open"`
  - canonical ID
  - absolute path
  - root-relative path
  - source span with byte and line/column fields
  - title and zettel kind if cheaply available
- Make errors clear and script-detectable for invalid IDs, missing index, missing target, duplicate/ambiguous canonical
  IDs, and zettel rows without usable source spans.
- Keep this command read-only. It should not reindex implicitly unless the repo's existing command pattern already does
  so consistently.

Acceptance:

- `zorg path @fixture --root ROOT --db DB` prints a stable human path form.
- `zorg path @fixture --format json` returns a valid JSON object with location fields.
- Missing, invalid, and ambiguous IDs exit nonzero with clear stderr.
- Docs identify this command as the stable editor jump contract.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor
cargo test -p zorg-cli --test smoke zorg_path
```

## Phase 13C: `zorg promote @id`

Owner: one Rust structural refactor agent.

Touched repo: `../zorg`.

Likely files:

- `crates/zorg-refactor/src/lib.rs`
- optional `crates/zorg-refactor/src/promote.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/refactor.md`

Scope:

- Add `zorg promote @id` for promoting a nested zettel into a file zettel.
- Require preview/check behavior first:
  - `--check` or default dry-run should print a plan and exit without writes.
  - `--write` or an equivalent explicit flag should apply changes.
  - `--json` / `--format json` should work for both preview and write results.
- Define destination path rules. Recommended conservative contract:
  - default destination derived from canonical ID under the root, such as `foo/bar.z` for `@foo/bar`, unless the source
    file layout already has a clearer local convention;
  - support `--to PATH` for explicit destination;
  - refuse paths outside the root unless an existing `--allow-outside`-style pattern is intentionally adopted.
- Preserve the promoted zettel's declaration, title/opening text, body, tags, properties, todos, children, and internal
  text exactly where possible.
- Remove the original nested source block from its parent file without damaging adjacent siblings.
- Update references only when every affected source span and relative rewrite is deterministic. Absolute links should
  remain valid when the canonical ID is preserved; relative links may need refusal unless the shared helper can prove
  the same target text remains correct.
- Reparse all affected files after planning and after applying writes. Refuse writes that produce syntax or validation
  errors.

Acceptance:

- Golden tests cover nested-to-file promotion, destination collisions, explicit `--to`, root-boundary refusal, anonymous
  or missing-ID refusal, and unsafe relative-reference refusal.
- Write-mode smoke tests prove promoted output indexes and `zorg path @id` finds the new file location.
- JSON preview includes all files/edits/warnings needed by editor clients.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor promote
cargo test -p zorg-cli --test smoke zorg_promote
```

## Phase 13D: `zorg move @id --to PATH_OR_PARENT`

Owner: one Rust structural refactor agent.

Touched repo: `../zorg`.

Likely files:

- `crates/zorg-refactor/src/lib.rs`
- optional `crates/zorg-refactor/src/move_zettel.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/refactor.md`

Scope:

- Add `zorg move @id --to PATH_OR_PARENT` with preview/check/write and JSON behavior matching `promote`.
- Support conservative move forms:
  - file zettel to another file path;
  - nested zettel to another parent zettel or file when the destination can be represented safely;
  - optional `--as @new/id` only if ID changes and link rewrites reuse the shared rename logic safely.
- Preserve the moved zettel source text as much as possible, including child blocks.
- Refuse destination collisions, anonymous targets, moves that would create cycles, moves into unsupported source
  contexts, and rewrites with incomplete spans.
- Update references only through the shared deterministic replacement rules. If the command preserves canonical ID and
  source text, prefer no link edits over risky relative-link rewrites.
- Make already-at-target behavior deterministic: either no-op with a clear preview or refuse with a specific error.

Acceptance:

- Tests cover file-to-file moves, nested-to-file moves, nested parent changes, collisions, cycle refusal, outside-root
  refusal, and no-op/already-at-target behavior.
- Write-mode tests prove moved output passes `zorg check`, reindexes, and keeps links resolvable.
- JSON preview is compatible with Phase 13C's envelope and adds command-specific destination fields.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor move
cargo test -p zorg-cli --test smoke zorg_move
```

## Phase 13E: `zorg extract RANGE --id @id`

Owner: one Rust range refactor agent.

Touched repo: `../zorg`.

Likely files:

- `crates/zorg-refactor/src/lib.rs`
- optional `crates/zorg-refactor/src/extract.rs`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/refactor.md`

Scope:

- Add a range extraction command with editor-friendly arguments. Recommended CLI contract:
  - `zorg extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id`
  - optional byte range form for tests and advanced clients: `--byte-range START..END`
  - `--parent @id` or `--sibling-of @id` only if destination placement is unambiguous
  - preview/check/write and JSON behavior matching earlier phases.
- Validate range boundaries against UTF-8 character boundaries and source spans.
- Extract the selected text exactly into a new child or sibling zettel with the requested ID.
- Replace the selected source text with a deterministic link or short placeholder only if specified and documented.
  Recommended v1.1 default: replace the selected range with `#new/id` plus surrounding whitespace preservation only when
  the range is paragraph-like; otherwise require an explicit `--replace-with-link`.
- Refuse invalid ranges, selections crossing unsupported structural boundaries, ID collisions, destination collisions,
  and source text that cannot be reparsed safely after extraction.

Acceptance:

- Tests cover line/column and byte ranges, multiline extraction, UTF-8 boundaries, invalid ranges, ID collisions,
  selections crossing zettel boundaries, and write-mode reparse/reindex success.
- JSON preview includes source range, destination details, created zettel opening, and replacement edits.
- CLI docs show the exact range syntax intended for editor wrappers.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor extract
cargo test -p zorg-cli --test smoke zorg_extract
```

## Phase 13F: LSP Code Actions For Safe Refactors

Owner: one Rust LSP/refactor integration agent.

Touched repo: `../zorg`.

Likely files:

- `crates/zorg-ls/src/actions.rs`
- `crates/zorg-ls/src/main.rs`
- `crates/zorg-ls/tests/smoke.rs`
- `crates/zorg-refactor/src/lib.rs` only for narrow API adjustments
- `docs/lsp.md`
- `docs/refactor.md`

Scope:

- Advertise relevant refactor code action kinds in addition to existing quickfix support.
- Expose safe opportunities where the LSP has enough context:
  - promote nested zettel;
  - extract selected range when the client sends a range and the shared planner accepts it;
  - optional move action only for clear contexts, likely as a command-style action that shells through the CLI later.
- Keep the CLI/refactor crate as the source of truth. LSP code actions should call shared planners or return commands
  that future editor clients can execute, not reimplement source rewriting.
- Return `WorkspaceEdit` only when the complete edit plan is safe and self-contained. Otherwise return a command with
  arguments for preview or omit the action.
- Preserve existing quickfix and rename behavior.

Acceptance:

- LSP tests cover advertised action kinds, promote action availability on nested zettels, extract action availability on
  valid ranges, and rejection/omission in unsafe contexts.
- Existing rename and quickfix tests still pass.
- Docs state which refactors are direct LSP edits and which should be invoked through CLI preview/write flows.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-ls
cargo test --workspace
```

## Phase 13G: Epic Hardening, Docs, And Cross-Repo Contract Handoff

Owner: one Rust/docs validation agent.

Touched repos: primarily `../zorg`; `../zorg-nvim` only docs/contract notes if necessary.

Likely files:

- `docs/refactor.md`
- `docs/development.md`
- `docs/cross_repo.md`
- `README.md`
- `tools/validate_cross_repo.sh`
- `crates/zorg-cli/tests/mvp_e2e.rs`
- `../zorg-nvim/README.md` or `../zorg-nvim/doc/zorg.txt` only for forward-looking command contract notes

Scope:

- Add or finish a dedicated refactor user/developer doc covering:
  - safety model;
  - dry-run/write modes;
  - JSON preview envelope;
  - command examples;
  - refusal cases;
  - how to refresh the index after writes.
- Extend cross-repo validation where useful:
  - `zorg path --format json`;
  - preview JSON for promote/move/extract;
  - one small write-mode refactor against a temp corpus followed by `zorg check`, `zorg db reindex`, and `zorg query`.
- Add final end-to-end smoke coverage across path, promote, move, and extract using fixture or temp roots only.
- Document the exact CLI contracts Epic 15 `zorg.nvim` should wrap:
  - argv patterns;
  - JSON fields;
  - preview/write confirmation expectations;
  - exit code and stderr behavior.
- Reconcile help text and README examples with the final command names.

Acceptance:

- Full Rust validation passes.
- Cross-repo validation does not use the user's real `~/zorg`.
- Docs clearly mark this as non-dashboard, Rust-authoritative refactoring support.
- The epic can be closed with stable contracts for Neovim integration.

Validation:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
tools/validate_cross_repo.sh
```

## Sequencing And Handoffs

Run the phases in order. Phase 13A is the compatibility boundary for all later agents. Phase 13B is deliberately
read-only and gives editor clients a location contract before writes exist. Phases 13C, 13D, and 13E should be separate
agent instances because each command has distinct source-rewrite risks. Phase 13F consumes the stable planner APIs.
Phase 13G is the integration gate and handoff to the later Neovim epic.

Each phase agent should start with:

```sh
git status --short
```

Each phase handoff should include:

- files changed;
- public API or CLI/JSON contract changes;
- validation commands run and results;
- intentionally deferred limitations;
- any follow-up needed by Epic 15.

## Risk Controls

- Span safety: every write must come from source spans in freshly reparsed source, not from stale database rows alone.
- Atomicity: plan all files first, validate all edits, then write with temp files and source guards. Avoid partial
  writes where possible; if partial failure remains possible, report exactly what was written.
- Relative links: preserve canonical IDs and existing link text whenever possible. Rewrite relative links only through
  shared deterministic rules; otherwise refuse.
- Formatting drift: avoid broad pretty-printing. Prefer source slicing and minimal generated openings.
- Test stability: use temp roots/databases. Do not depend on wall-clock timing, real home directories, or long-running
  watchers.
- Cross-repo boundaries: do not require `../zorg-nvim` changes until Epic 15, except documentation of the final Rust
  contracts if helpful.

---
title: Zorg Epic 9 Documentation, Release, and Cross-Repo Validation Plan
legend: sdd/legends/202605/zorg_v1_mvp.md
legend_bead_id: zorg-1
epic: 9
bead_id: zorg-1.9
tier: epic
created: 2026-05-02
create_time: 2026-05-02 21:52:12
status: wip
prompt: sdd/prompts/202605/zorg_epic9_docs_release_validation.md
---

# Zorg Epic 9: Documentation, Release, and Cross-Repo Validation

## Goal

Implement Epic 9 from `sdd/legends/202605/zorg_v1_mvp.md`: make the Zorg v1 MVP coherent, verifiable, and releasable
across the Rust, Tree-sitter, and Neovim repositories.

The finished epic should let a contributor or release operator:

- validate the full MVP from one documented command;
- verify that parser, Rust, LSP, query, capture, fix, and Neovim behavior all exercise the same representative `.z`
  examples;
- build and test all three sibling repos from their READMEs alone;
- understand the release contract for versions, generated parser artifacts, binaries, changelog entries, and dry-run
  checks;
- complete a dry-run release checklist without hidden manual steps.

## Current Context

The roadmap describes three repos:

- `../zorg`: Rust CLI, libraries, store, query, formatter/fix, capture, and LSP. In this workspace the Rust repo is the
  current checkout.
- `../zorg-treesitter`: Tree-sitter grammar, query files, corpus tests, and generated parser boundary.
- `../zorg-nvim`: Neovim plugin, command wrappers, LSP setup, Tree-sitter query runtime files, health checks, and docs.

Current implementation state found while planning:

- The Rust workspace is no longer skeletal. It has parser/model, SQLite indexing, SWOG LIST queries, `zorg-ls`, fix, and
  capture implementations plus broad CLI and LSP smoke tests.
- `fixtures/corpus` in the Rust repo is the canonical shared fixture source, with `fixtures/README.md` documenting the
  current inventory and policy.
- `docs/cross_repo.md` records a Phase 5 validation snapshot and already names validation commands for all three repos,
  but it is not yet a release-grade validation gate.
- `../zorg-treesitter` has grammar docs, corpus tests, query files, generated parser outputs in its working tree, and
  README instructions for `npm run generate`, `npm test`, query compilation, highlighting, and parsing shared fixtures.
- `../zorg-nvim` has README/help docs, filetype/Tree-sitter/LSP/commands/health modules, copied Tree-sitter query files,
  and headless Neovim tests for smoke, commands, helpers, and LSP setup.
- All three worktrees were clean at planning time.

## Non-Negotiables

- The shared corpus remains `.z`-only for accepted fixtures. Legacy-looking examples are negative fixtures only.
- `../zorg/fixtures/corpus` remains the canonical fixture source unless a later release policy deliberately chooses
  submodules or another mechanism.
- No repo may introduce Python-era compatibility behavior while hardening cross-repo tests.
- Runtime semantics stay in Rust. Tree-sitter owns syntax structure and editor query captures; Neovim delegates to
  `zorg`, `zorg-ls`, and the `zorg` parser.
- Every phase must leave its touched repo or repos with their local validation commands passing.
- Cross-repo validation commands must be deterministic and must not depend on the operator's real `~/zorg` corpus.
- Release packaging must define generated parser policy explicitly before changing whether generated artifacts are
  committed.
- Distinct agents should work sequentially by default. A later agent may read all repos, but it should avoid rewriting
  outputs owned by an earlier phase except to fix a documented validation failure.

## Proposed Phase Split

Use five sequential phases. Each phase is sized for one distinct agent instance. The split deliberately avoids assigning
"all docs" or "all tests" to one agent; instead, it hardens the contracts in the order later phases need them.

## Phase 9.1: Fixture Manifest and Synchronization Contract

Purpose: turn the shared fixture corpus from a convention into a testable cross-repo contract.

Primary ownership:

- `fixtures/README.md`
- `fixtures/corpus/`
- `docs/cross_repo.md`
- a small fixture manifest/check tool in the Rust repo, preferably under `tools/` or `fixtures/`
- targeted copies or references in `../zorg-treesitter/test/corpus`, `../zorg-treesitter/test/highlight`, and
  `../zorg-nvim/.tests/root` only where needed

Scope:

- Add a machine-readable fixture manifest in the Rust repo. It should list each canonical fixture, its role, whether it
  is valid or negative, and which downstream surfaces should exercise it.
- Add a deterministic fixture sync/check command. Keep it simple: a shell, Rust, or portable script is acceptable if the
  repo already documents how to run it. The command should detect drift between canonical fixtures and the copied
  examples used by Tree-sitter and Neovim tests.
- Decide and document how Tree-sitter corpus fixtures relate to canonical `.z` files. Tree-sitter's `test/corpus/*.txt`
  format cannot always be a byte-for-byte copy, so the manifest should record source fixture provenance instead of
  pretending all formats are identical.
- Ensure Rust parser/model, CLI, LSP, query, capture, and fix tests can point back to canonical fixture names rather
  than anonymous inline examples where practical.
- Update `docs/cross_repo.md` with the fixture ownership workflow:
  1. add/update canonical fixture;
  2. update manifest;
  3. update derived Tree-sitter or Neovim test fixture;
  4. run sync/check command plus repo-local tests.
- Keep fixture additions small. Add new examples only when they cover a cross-repo MVP behavior missing from the current
  corpus, such as a capture result fixture or a full valid multi-file corpus.

Out of scope:

- Full end-to-end test orchestration.
- Release checklist work.
- Rewriting grammar or parser semantics except for narrowly fixing fixture drift.

Acceptance:

- A documented command fails when a downstream fixture copy is stale or missing.
- The manifest covers every file in `fixtures/corpus`.
- Tree-sitter and Neovim test fixtures identify their canonical source or explain why they are local-only.
- Existing validation still passes:
  - `cargo fmt --check`
  - `cargo test --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `npm run generate && npm test` in `../zorg-treesitter`
  - all documented headless Neovim tests in `../zorg-nvim`

Handoff:

- Later phases must use the manifest/check tool when adding E2E fixtures or release validation scripts.

## Phase 9.2: Rust MVP End-to-End Test Harness

Purpose: add a first-class Rust-side E2E harness that exercises the complete CLI and LSP MVP against temporary
`~/zorg`-like roots.

Primary ownership:

- `crates/zorg-cli/tests/`
- `crates/zorg-ls/tests/`
- optional shared test support under a workspace test-support crate or local test modules
- `docs/development.md`
- `README.md` Rust validation section

Scope:

- Add an integration test or test module named around "mvp_e2e" that builds a temporary corpus from the canonical
  fixtures and runs the user workflow in order:
  - parse representative `.z` files;
  - reject explicit legacy source paths and legacy-looking strict input;
  - `zorg db reindex` into an explicit temp database;
  - run inline SWOG LIST queries and query-by-`#z/query` ID;
  - run LSP initialize/open/diagnostics/navigation/reference or definition checks against the same temp root;
  - run `zorg capture --json`, reindex, query the captured zettel, run `zorg fix`, then `zorg fix --check`;
  - confirm the workflow does not touch the real home directory or real `~/zorg`.
- Prefer invoking the built binaries via `CARGO_BIN_EXE_zorg` and `CARGO_BIN_EXE_zorg-ls` for behavioral coverage.
- Keep low-level parser/query/fix unit tests where they are; the new harness should validate integration boundaries, not
  duplicate every unit case.
- Add stable assertions for machine-readable outputs where available (`--json`, LSP JSON-RPC responses). For human text
  output, assert only user-visible contract lines that matter.
- Document the Rust E2E command in `docs/development.md` and README. If the command is simply `cargo test --workspace`,
  call out which test covers the full MVP loop.

Out of scope:

- Neovim command execution.
- Tree-sitter corpus validation.
- Release packaging.

Acceptance:

- A single Rust command verifies parse, index, query, LSP, capture, and fix against an isolated temp root.
- The E2E harness uses canonical fixtures or manifest-declared derived fixtures.
- Tests prove explicit `--root`/`--db` paths are honored and no real `~/zorg` state is required.
- `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` pass.

Handoff:

- Phase 9.3 should call this Rust command from the cross-repo validation gate instead of reimplementing its individual
  steps.

## Phase 9.3: Cross-Repo Validation Command

Purpose: provide the documented one-command local validation gate promised by Epic 9.

Primary ownership:

- cross-repo validation script in the Rust repo, preferably under `tools/`
- `docs/cross_repo.md`
- `docs/development.md`
- targeted README validation sections in all three repos
- small test or smoke-script updates in `../zorg-treesitter` and `../zorg-nvim` if the gate reveals gaps

Scope:

- Add a single command from the Rust repo root that validates all three sibling repos in order:
  - Rust formatting, tests, clippy, `zorg --help`, and `zorg-ls --version`;
  - Tree-sitter dependency/generation/test flow, query compilation, highlight smoke, and parsing shared fixtures;
  - Neovim headless tests for smoke, commands, helpers, LSP, and health-relevant runtime query loading;
  - fixture manifest/sync check from Phase 9.1.
- Make the command fail fast by default but provide enough context to identify which repo/step failed.
- Avoid hidden global dependencies where practical. If external tools are required (`nvim`, `stylua`, `luacheck`, `npm`,
  `tree-sitter` through npm), the script should check and report them clearly.
- Use the locally built Rust binaries for Neovim command tests when possible, or document when fake test binaries are
  intentionally used.
- Keep generated artifacts policy unchanged in this phase unless Phase 9.5 has already landed. If `npm run generate`
  changes generated files that are not meant to be committed, the validation docs should state that clearly.
- Update `docs/cross_repo.md` so the current validation results section becomes a reproducible command, not a dated
  manual transcript.

Out of scope:

- Deciding release artifact names or versioning policy.
- Large README rewrites beyond validation instructions.
- Expanding feature behavior.

Acceptance:

- One documented command run from the Rust repo root validates the MVP locally across all three repos.
- Missing tools produce clear setup errors instead of confusing downstream failures.
- Cross-repo validation uses temp roots and explicit database paths.
- `docs/cross_repo.md` names the command, expected duration/outputs, and troubleshooting notes.
- Repo-local validation commands still pass independently.

Handoff:

- Phase 9.4 docs should present this command as the contributor confidence check.
- Phase 9.5 release checklist should use this command as the pre-release gate.

## Phase 9.4: Contributor Documentation Pass

Purpose: make the three repos buildable and understandable from docs alone.

Primary ownership:

- `README.md`, `docs/*.md`, and `fixtures/README.md` in the Rust repo
- `../zorg-treesitter/README.md` and `../zorg-treesitter/docs/grammar.md`
- `../zorg-nvim/README.md`, `../zorg-nvim/doc/zorg.txt`, and regenerated `../zorg-nvim/doc/tags`

Scope:

- Audit every README for current command names and remove stale roadmap language such as "skeletal" or "later phase"
  where the behavior now exists.
- Rust README/docs must cover:
  - install/build from source;
  - CLI command examples for parse, check, db status/reindex, query, fix, and capture;
  - `zorg-ls` startup/config expectations;
  - fixture policy and validation commands;
  - architecture/crate map concise enough for new contributors.
- Tree-sitter README/docs must cover:
  - npm install/generate/test;
  - generated parser artifact policy as it stands before release packaging;
  - public node names and query file contract;
  - how to validate against shared fixtures and how to update derived corpus tests.
- Neovim README/help must cover:
  - local development with sibling Rust binaries and parser;
  - plugin manager installation;
  - `require("zorg").setup()` defaults;
  - `:ZorgIndex`, `:ZorgQuery`, `:ZorgFix`, `:ZorgCapture`, `:ZorgStatus`;
  - LSP setup, root detection, health checks, and troubleshooting.
- Keep docs aligned with non-negotiable syntax: `.z`, `~/zorg`, no legacy compatibility.
- Run examples that can be run cheaply. If a documented command is illustrative and not executed by tests, mark the
  assumptions.

Out of scope:

- Changing release policy beyond linking to the Phase 9.5 release docs if they already exist.
- Adding new user-facing features.

Acceptance:

- A new contributor can follow README instructions in each repo to build/test that repo.
- All documented commands use the current CLI/LSP/Neovim command names.
- `doc/tags` is regenerated after Neovim help changes.
- Cross-links between the three repos are accurate and do not imply semantic ownership in Tree-sitter or Lua.
- Cross-repo validation from Phase 9.3 passes after the docs changes.

Handoff:

- Phase 9.5 should not need to rewrite general contributor docs; it should add release-specific docs and link to the
  existing validation command.

## Phase 9.5: Release Packaging and Dry-Run Checklist

Purpose: define and verify the MVP release process without actually publishing artifacts.

Primary ownership:

- new release docs in the Rust repo, for example `docs/release.md`
- release notes/changelog files in each repo if chosen by the policy
- version fields in `Cargo.toml`, `../zorg-treesitter/package.json`, and Neovim docs only if the dry-run policy requires
  a version alignment change
- generated parser artifact policy in `../zorg-treesitter/README.md` and `docs/cross_repo.md`
- optional release helper script under `tools/`

Scope:

- Define versioning policy across the three repos:
  - whether Rust crates, Tree-sitter package, and Neovim plugin share one version or release independently;
  - how pre-1.0 breaking changes are signaled;
  - how `zorg` and `zorg-ls` binary versions are reported.
- Define changelog policy:
  - file name and location;
  - required sections;
  - how cross-repo changes are referenced.
- Define binary naming and packaging:
  - `zorg` and `zorg-ls` names;
  - target triples intended for initial release;
  - archive naming;
  - checksum/signature expectations, even if signing is deferred.
- Decide generated parser policy for `../zorg-treesitter`:
  - whether generated `src/parser.c`, `src/grammar.json`, and `src/node-types.json` are committed for release;
  - how Rust `zorg-parse` consumes parser artifacts in source builds versus packaged releases;
  - how Neovim users obtain a compiled parser.
- Add a dry-run release checklist that includes:
  - clean worktree checks for all repos;
  - fixture sync check;
  - cross-repo validation command;
  - version/changelog audit;
  - build/package commands;
  - artifact inspection;
  - rollback notes for a failed dry run.
- If a helper script is added, it should perform non-publishing checks only. It must not tag, push, upload, or mutate
  versions unless explicitly invoked with a clearly named future release flag.

Out of scope:

- Publishing a real release.
- Creating CI pipelines unless the repo already has a clear place for them and the work is tiny.
- Solving distribution channels such as Homebrew, crates.io, npm, or plugin manager metadata beyond documenting the
  intended first release path.

Acceptance:

- `docs/release.md` or equivalent contains a complete dry-run checklist with no hidden manual steps.
- Version, changelog, binary naming, generated parser, and artifact policies are explicit.
- A dry-run release command/checklist completes locally without publishing.
- Cross-repo validation from Phase 9.3 is the required pre-release gate.
- All repo-local validation commands pass.

Handoff:

- The MVP is ready for a later release/operator agent to perform an actual tagged release using the documented
  checklist.

## Final Definition of Done for Epic 9

- `fixtures/corpus` has a manifest-backed synchronization policy, and drift is detectable.
- A Rust E2E harness exercises parse, index, query, LSP, capture, and fix against isolated temp roots.
- One documented command validates all three sibling repos locally.
- READMEs and contributor docs in all three repos describe current behavior accurately.
- Release docs define versioning, changelog, binary naming, generated parser policy, and a dry-run checklist.
- The dry-run checklist completes without publishing artifacts.
- No accepted fixture, parser query, Rust semantic behavior, or Neovim integration reintroduces legacy syntax
  compatibility.

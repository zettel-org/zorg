---
create_time: 2026-05-02 15:12:25
status: done
prompt: sdd/prompts/202605/zorg_epic1_foundations.md
bead_id: zorg-1.1
tier: epic
---
# Zorg Epic 1 Foundations Plan

## Objective

Implement Epic 1, "Specification and Repository Foundations", from `sdd/legends/202605/zorg_v1_mvp.md` across the three
target repos:

- `../zorg`: Rust CLI, library crates, docs, fixtures, and development harness.
- `../zorg-treesitter`: Tree-sitter grammar skeleton, package metadata, queries, corpus tests, and generated-binding
  policy.
- `../zorg-nvim`: Neovim plugin skeleton, `.z` filetype detection, parser/LSP setup stubs, commands, docs, and health
  checks.

The goal is not to implement the parser, model, store, query engine, LSP behavior, capture, or formatter yet. The goal
is to leave later agents with executable scaffolding, stable contracts, and documentation precise enough to proceed
independently.

## Product Decisions To Lock In

All phases must consistently apply these decisions:

- Canonical extension is `.z`.
- Default root is `~/zorg`.
- Legacy Python-era syntax is not supported as readable or writable v1 syntax.
- Canonical file zettel header is `%%% @foo ... %%%`.
- Directory zettel is `init.z`.
- IDs are `@foo`; local IDs are `^bar`, resolved under the nearest ancestor zettel ID as `@ancestor/bar`.
- Links are absolute `#foo/bar`, child-relative `+child`, and sibling-relative `~sibling`.
- Type tags use `#z/...`.
- Properties are `key::value`, with slash-separated list values for v1.
- Lifecycle properties are `do::`, `due::`, and `did::`.
- Timebox properties are `p::`, `start::`, and `end::`.
- Todos are `[ ]`, `[N]`, `[X]`, and `[?]`.
- Code blocks are ordinary Markdown fences.
- Query output is LIST only.
- Query and template definitions are ordinary zettel tagged `#z/query` and `#z/tmpl`.

## Execution Model

Each phase below is intended for one distinct agent instance. Agents should not assume that other in-progress agents
have finished unless their phase explicitly depends on an earlier phase.

The safest ordering is:

1. Phase 1: Shared specification and fixtures.
2. Phase 2: Rust workspace foundation.
3. Phase 3: Tree-sitter foundation.
4. Phase 4: Neovim plugin foundation.
5. Phase 5: Cross-repo validation and integration pass.

Phases 2, 3, and 4 can begin after Phase 1 lands. Phase 5 must run last.

## Phase 1: Shared Specification And Fixtures

Owner: one documentation/specification agent.

Target repo: `../zorg`.

Purpose: establish the contract that all implementation agents consume.

Expected changes:

- Add `docs/syntax.md` defining `.z` syntax, file headers, directory zettel, zettel IDs, local IDs, links, tags,
  properties, todos, code fences, query zettel, template zettel, and no-legacy policy.
- Add `docs/model.md` defining the semantic model: zettel kinds, hierarchy, IDs, links, tags, inherited tags,
  properties, todos, diagnostics, and source spans.
- Add `docs/query.md` defining the SWOG LIST MVP surface, supported filters, ordering expectations, query zettel
  execution, and explicit deferrals.
- Add `docs/lsp.md` defining LSP MVP behavior, feature boundaries, diagnostics, source-span expectations, rename safety
  rules, and workspace-root handling.
- Add `docs/capture.md` defining `#z/tmpl` template discovery, capture destinations, source-file recording,
  noninteractive flags, and deferred behavior.
- Add `docs/fix.md` defining strict check mode, allowed autofixes, idempotency, legacy-looking syntax diagnostics, and
  deferred formatting features.
- Add `fixtures/README.md` plus a small canonical fixture corpus under `fixtures/corpus/`.
- Add at least these fixture files:
  - `minimal.z`: one file zettel with an ID, property, tag, and paragraph.
  - `nested.z`: nested notes, a local ID, child/sibling/absolute links, and todos.
  - `query_and_template.z`: `#z/query` and `#z/tmpl` examples.
  - `legacy_invalid.z`: unsupported legacy-looking examples documented as invalid input for strict checks.
  - `dir/init.z`: a directory zettel example.
- Update `../zorg/README.md` with the MVP summary, repo role, documentation map, fixture policy, and current skeleton
  status.

Design constraints:

- The docs must be explicit enough that grammar, Rust parser/model, store, LSP, and Neovim agents can proceed without
  re-reading the roadmap.
- The fixture corpus is a public cross-repo contract. Keep it small but representative.
- Where behavior is intentionally deferred, say so directly.
- Do not introduce compatibility support for `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, `tick::`, generated `.zoc`, old
  folgezettel IDs, tag sugar, or Python-era link behavior.

Acceptance:

- A later agent can identify every v1 syntax primitive from the docs.
- Fixtures demonstrate every non-negotiable syntax decision.
- `README.md` points to the docs and explains that implementation is intentionally skeletal.
- No generated code or implementation crates are required in this phase.

## Phase 2: Rust Workspace Foundation

Owner: one Rust scaffolding agent.

Target repo: `../zorg`.

Dependency: Phase 1 should be complete.

Purpose: create an executable Rust workspace skeleton with crate boundaries matching the documented architecture.

Expected changes:

- Add a root `Cargo.toml` workspace.
- Use Rust 2024 edition if the installed stable toolchain supports it; otherwise use the latest stable edition available
  and document why.
- Add crates:
  - `crates/zorg-core`
  - `crates/zorg-parse`
  - `crates/zorg-store`
  - `crates/zorg-query`
  - `crates/zorg-fix`
  - `crates/zorg-capture`
  - `crates/zorg-ls`
  - `crates/zorg-cli`
- Add minimal library files for library crates and binary entry points for `zorg` and `zorg-ls`.
- Add workspace-level lint configuration in `Cargo.toml`.
- Add `rustfmt.toml`.
- Add development docs such as `docs/development.md` or a README development section with exact validation commands.
- Add a small integration-test or smoke-test harness proving the binaries exist and libraries compile.
- Add `.gitignore` appropriate for Rust build outputs and local SQLite/test artifacts.

Design constraints:

- Keep all behavior stubbed and honest. It is acceptable for `zorg --help` and `zorg-ls --version` to exist, but
  parser/store/query behavior should not pretend to work.
- Prefer stable, mainstream Rust dependencies. Avoid broad dependency choices that later implementation agents will be
  forced to unwind.
- The crate graph should express intended boundaries:
  - `zorg-core` has no dependency on parser/store/query/LSP crates.
  - `zorg-parse` depends on `zorg-core`.
  - `zorg-store`, `zorg-query`, `zorg-fix`, and `zorg-capture` depend on `zorg-core` as needed.
  - `zorg-cli` wires command entry points but does not own core domain logic.
  - `zorg-ls` remains a separate binary crate.
- Avoid implementing Tree-sitter integration in this phase beyond a clearly named placeholder module or error.

Acceptance:

- `cargo fmt --check` passes.
- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes, or the phase documents any unavailable local toolchain
  component.
- `cargo run -p zorg-cli -- --help` or equivalent binary invocation works.
- `cargo run -p zorg-ls -- --version` or equivalent binary invocation works.

## Phase 3: Tree-Sitter Foundation

Owner: one grammar scaffolding agent.

Target repo: `../zorg-treesitter`.

Dependency: Phase 1 should be complete.

Purpose: create a standard Tree-sitter repository skeleton that later grammar agents can extend without reorganizing.

Expected changes:

- Add `package.json` and any required Tree-sitter package metadata.
- Add `grammar.js` with a minimal but valid Zorg grammar skeleton that recognizes a document and basic text without
  claiming full MVP syntax support.
- Add `corpus/` with initial tests based on Phase 1 fixtures, starting with the smallest valid parse cases.
- Add `queries/highlights.scm`, `queries/locals.scm`, `queries/folds.scm`, and optionally `queries/injections.scm` as
  placeholder query files with documented intent.
- Add binding metadata only if it is required by current Tree-sitter tooling policy; otherwise document
  generated-artifact policy in the README.
- Add `.gitignore` for generated parser/build outputs according to the selected policy.
- Update `README.md` with build/test commands, repo role, grammar scope, query file descriptions, and links back to
  `../zorg/docs`.

Design constraints:

- The grammar should be skeletal but executable. It should not encode ambiguous or speculative syntax before Epic 2.
- Keep node names conservative and documented because they become public contracts for `zorg-parse` and `zorg-nvim`.
- Do not implement legacy syntax nodes.
- Prefer current Tree-sitter CLI conventions over bespoke scripts.

Acceptance:

- Dependency installation instructions are documented.
- Tree-sitter generation succeeds locally if the CLI and dependencies are available.
- Tree-sitter corpus tests pass locally if the CLI and dependencies are available.
- If local tooling is missing, the phase documents the exact command that could not be run and why.
- README tells later agents where to add grammar rules, corpus examples, and editor queries.

## Phase 4: Neovim Plugin Foundation

Owner: one Neovim/Lua scaffolding agent.

Target repo: `../zorg-nvim`.

Dependency: Phase 1 should be complete.

Purpose: create a minimal Neovim plugin skeleton that can load, detect `.z` files, expose setup stubs, and provide
command surfaces for later CLI/LSP integration.

Expected changes:

- Add Lua module layout, likely:
  - `lua/zorg/init.lua`
  - `lua/zorg/config.lua`
  - `lua/zorg/lsp.lua`
  - `lua/zorg/commands.lua`
  - `lua/zorg/treesitter.lua`
  - `lua/zorg/health.lua`
- Add filetype detection for `*.z`, using current Neovim conventions such as `ftdetect/` or `filetype.lua`.
- Add parser registration stub for the Zorg Tree-sitter parser.
- Add LSP setup stub for `zorg-ls` with conservative defaults and root detection aligned with `~/zorg`.
- Add user command stubs:
  - `:ZorgIndex`
  - `:ZorgQuery`
  - `:ZorgFix`
  - `:ZorgCapture`
- Add `doc/zorg.txt` help docs and update `README.md` with installation, setup, commands, health checks, and development
  validation.
- Add minimal Lua tests or a documented headless Neovim smoke test.

Design constraints:

- `require("zorg").setup()` should be stable and minimal.
- Do not force global keymaps.
- Do not reimplement Zorg semantics in Lua. The plugin should delegate to `zorg`, `zorg-ls`, and Tree-sitter.
- Commands may be stubs or safe shell wrappers depending on whether the Rust binaries exist after Phase 2; they should
  fail clearly when binaries are missing.
- Keep plugin loading compatible with modern Neovim and common plugin managers.

Acceptance:

- A minimal headless Neovim invocation can load the plugin.
- Opening or simulating a `*.z` buffer sets the expected filetype.
- `require("zorg").setup({})` returns without error.
- `:checkhealth zorg` or equivalent health module entry point exists.
- README explains the current skeleton status and later integration expectations.

## Phase 5: Cross-Repo Validation And Handoff

Owner: one integration agent.

Targets: `../zorg`, `../zorg-treesitter`, and `../zorg-nvim`.

Dependency: Phases 1-4 should be complete.

Purpose: catch contract drift, make validation commands reproducible, and leave Epic 1 ready for Epic 2 and Epic 3
implementation agents.

Expected changes:

- Verify all three READMEs agree on:
  - `.z` extension.
  - `~/zorg` default root.
  - no legacy support.
  - repo responsibilities.
  - development commands.
- Verify Tree-sitter node-name placeholders and Neovim parser registration naming are aligned.
- Verify Rust crate names and binary names match Neovim command/LSP docs.
- Add or update a top-level `docs/cross_repo.md` in `../zorg` describing how the three repos fit together and how shared
  fixtures should be synchronized.
- Optionally add lightweight helper scripts only if they reduce repeated validation friction without adding a new build
  system.
- Run all available validation commands from the three repos and record any skipped commands due to missing local tools.

Design constraints:

- Do not expand scope into Epic 2 grammar implementation, Epic 3 model implementation, or real LSP/query behavior.
- Prefer documentation alignment and smoke tests over premature integration code.
- Preserve each repo's clean ownership boundary.

Acceptance:

- `../zorg` validation commands pass or skipped toolchain pieces are clearly documented.
- `../zorg-treesitter` validation commands pass or skipped toolchain pieces are clearly documented.
- `../zorg-nvim` validation commands pass or skipped local Neovim/tooling pieces are clearly documented.
- The final handoff notes list the next recommended agent tasks for Epic 2 and Epic 3.

## Global Validation Checklist

At the end of Epic 1, the following should be true:

- `../zorg/docs/` contains syntax, model, query, LSP, capture, and fix documentation.
- `../zorg/fixtures/` contains canonical examples shared by downstream repos.
- `../zorg` is a compiling Rust workspace with `zorg` and `zorg-ls` binary surfaces.
- `../zorg-treesitter` is a valid Tree-sitter grammar repo with corpus and query folders.
- `../zorg-nvim` is a loadable Neovim plugin skeleton with `.z` filetype detection and setup stubs.
- All READMEs state that implementation is skeletal and future epics own actual parser/model/store/query/LSP behavior.
- No repo supports legacy Python-era syntax as a v1 compatibility layer.

## Risks And Mitigations

- Risk: docs become too vague for parallel agents.
  - Mitigation: Phase 1 must include concrete examples and explicit deferrals, not just prose.
- Risk: Rust skeleton chooses dependencies that constrain later implementation.
  - Mitigation: Phase 2 should keep dependencies minimal and document why each dependency exists.
- Risk: Tree-sitter node names drift from Rust parser and Neovim expectations.
  - Mitigation: Phase 3 documents placeholder node names; Phase 5 verifies naming across repos.
- Risk: Neovim plugin starts owning core semantics.
  - Mitigation: Phase 4 limits Lua to filetype, setup, health, command wrappers, and LSP/Tree-sitter configuration.
- Risk: agents accidentally implement deferred compatibility behavior from `research/202605/v1_mvp_curation.md`.
  - Mitigation: every phase repeats the roadmap override: no `.zo`, `.zoq`, `.zot`, `ID::`, `LID::`, `tick::`, old link
    behavior, or Python compatibility model.

## Recommended Agent Prompts

Use one fresh agent instance per phase. Suggested prompts:

1. "Implement Phase 1 from `sase_plan_zorg_epic1_foundations.md`: shared specification docs and fixtures only."
2. "Implement Phase 2 from `sase_plan_zorg_epic1_foundations.md`: Rust workspace skeleton in `../zorg`, preserving Phase
   1 docs and fixtures."
3. "Implement Phase 3 from `sase_plan_zorg_epic1_foundations.md`: Tree-sitter repo skeleton in `../zorg-treesitter`
   based on the Phase 1 docs."
4. "Implement Phase 4 from `sase_plan_zorg_epic1_foundations.md`: Neovim plugin skeleton in `../zorg-nvim` based on the
   Phase 1 docs."
5. "Implement Phase 5 from `sase_plan_zorg_epic1_foundations.md`: cross-repo validation and handoff after Phases 1-4 are
   complete."

---
create_time: 2026-05-02 21:11:20
status: wip
prompt: sdd/prompts/202605/zorg_epic8_neovim_integration.md
---
# Zorg Epic 8 Neovim Integration Plan

## Context

Epic 8 in `sdd/legends/202605/zorg_v1_mvp.md` makes Zorg useful inside Neovim without moving core Zorg semantics into
Lua. The implementation target is the sibling repo `../zorg-nvim`. The Rust repo `../zorg` and grammar repo
`../zorg-treesitter` provide the contracts:

- `zorg` is the CLI binary.
- `zorg-ls` is the LSP binary.
- `.z` buffers use Neovim filetype `zorg`.
- Tree-sitter parser and query language name is `zorg`.
- Query files live in `../zorg-treesitter/queries`.
- Neovim must shell out to `zorg` and connect to `zorg-ls`; it must not reimplement parse, query, capture, fix, or graph
  semantics.

The current `../zorg-nvim` state is an Epic 1 skeleton. It already has `filetype.lua`, `ftplugin/zorg.lua`,
`lua/zorg/{init,config,treesitter,lsp,commands,health}.lua`, `plugin/zorg.lua`, help docs, README, and a headless smoke
test. The skeleton is loadable but several behaviors are still placeholders:

- `:ZorgIndex` currently calls the deferred `zorg index` path; the real Rust command is `zorg db reindex`.
- LSP setup starts `zorg-ls`, but it does not pass the server initialization options that `zorg-ls` currently reads
  (`rootPath`, `databasePath` / `dbPath`, `trace` / `logLevel`).
- Tree-sitter registration is minimal and does not make highlight query loading verifiable from the plugin runtimepath.
- CLI command handling needs richer Neovim UX: async output, stable error surfacing, query result display,
  current-buffer defaults for fix, and capture result handling.
- Health and docs need to reflect the real Rust CLI/LSP state rather than future placeholders.

## Guiding Decisions

- Keep all product semantics in Rust and Tree-sitter. Lua only wires Neovim UI to those tools.
- Prefer conservative defaults: no forced global mappings, no automatic writes beyond commands the user explicitly runs,
  no hidden parser installation.
- Use current Neovim APIs with a 0.10+ support target. Compatibility shims are acceptable for simple APIs already
  present in the skeleton.
- Keep each phase independently runnable by a distinct agent. Later phases may build on earlier phase APIs, but no phase
  should require simultaneous edits in another phase.
- Add or extend headless Neovim tests in the same phase as behavior changes.
- Do not modify `../zorg` or `../zorg-treesitter` unless an integration contract is wrong or missing. If such a blocker
  appears, document it and keep the fix narrowly scoped.

## Phase 1: Filetype And Tree-Sitter Runtime Integration

Owner: one Neovim/Lua agent.

Target repo: `../zorg-nvim`.

Purpose: close Epic 8.1 by making `.z` filetype detection and Tree-sitter query loading real and testable.

Scope:

- Keep `*.z` filetype detection on the modern `vim.filetype.add` path.
- Harden `ftplugin/zorg.lua` with appropriate buffer-local options only: `commentstring`, `comments`, and any harmless
  markdown-like defaults that are already justified by Zorg syntax. Avoid indentation or folding behavior that would
  duplicate parser semantics.
- Update `lua/zorg/treesitter.lua` so `require("zorg").setup()`:
  - registers the `zorg` parser name for filetype `zorg`;
  - works when the parser binary is already installed;
  - reports a clear warning through health, not during every setup, when the parser binary or Tree-sitter runtime is
    unavailable;
  - exposes small testable helpers for parser/query availability.
- Add `queries/zorg/highlights.scm`, `queries/zorg/folds.scm`, `queries/zorg/locals.scm`, and
  `queries/zorg/injections.scm` to the Neovim runtimepath by syncing the current public query files from
  `../zorg-treesitter/queries`. Keep them direct copies unless Neovim requires a tiny runtime-specific adjustment, and
  document any adjustment in comments.
- Extend the headless smoke test, or add a focused Tree-sitter test, to verify:
  - opening a `.z` buffer sets `filetype=zorg`;
  - `vim.treesitter.language.register("zorg", "zorg")` setup path is safe;
  - highlight query lookup for `zorg` succeeds when Neovim exposes query APIs.

Out of scope:

- Installing parsers for the user.
- Changing grammar node names.
- Implementing syntax behavior in Lua.

Acceptance:

- `nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"` passes from `../zorg-nvim`.
- Tree-sitter query files are loadable from the plugin runtimepath.
- README and help mention how users connect the parser from `zorg-treesitter` without implying this plugin owns parser
  generation.

## Phase 2: LSP Setup And Root Configuration

Owner: one Neovim/Lua agent.

Target repo: `../zorg-nvim`.

Purpose: close Epic 8.2 by making `require("zorg").setup()` start `zorg-ls` with the configuration shape the Rust server
actually consumes.

Scope:

- Extend configuration with explicit LSP initialization options:
  - `root` remains the user-facing default corpus root and expands to `~/zorg`;
  - optional `db_path` / `database_path`;
  - optional `trace` / `log_level`;
  - configured values are passed through `initialization_options`, not only `settings`.
- Harden root detection:
  - look upward from the buffer path for configured `root_markers`;
  - include sensible markers such as `.zorgroot` and `init.z` only if they do not cause false positives outside a
    corpus;
  - fall back to the configured `root`;
  - keep single-root behavior aligned with `zorg-ls`.
- Make LSP startup idempotent:
  - reuse an existing `zorg-ls` client for the same resolved root;
  - avoid duplicate notifications for a missing binary;
  - avoid starting on non-`zorg` buffers.
- Add tests for `lua/zorg/lsp.lua` by stubbing `vim.lsp.start` in headless Neovim:
  - verifies command, root, and initialization options;
  - verifies missing executable path does not throw;
  - verifies setup installs a `FileType zorg` autocmd.
- Update health to check `zorg-ls --version` when executable and report the configured root/database path clearly.

Out of scope:

- Changing `zorg-ls` capabilities.
- Adding editor commands through LSP `workspace/executeCommand`.

Acceptance:

- One `require("zorg").setup()` call is enough for users with `zorg-ls` on `PATH`.
- The LSP client receives `initialization_options.rootPath` matching the configured or detected root.
- Existing smoke tests and the new LSP tests pass.

## Phase 3: CLI Commands And Neovim Result Surfaces

Owner: one Neovim/Lua agent.

Target repo: `../zorg-nvim`.

Purpose: close the required Epic 8.3 command surface by connecting Neovim commands to the real Rust CLI and presenting
output in useful Neovim surfaces.

Scope:

- Replace the stale `:ZorgIndex` implementation with `zorg db reindex --root {root}`.
- Keep the required commands:
  - `:ZorgIndex [args]`
  - `:ZorgQuery [args]`
  - `:ZorgFix [args]`
  - `:ZorgCapture [args]`
- Add `:ZorgStatus [args]` only if it falls out naturally from the same command runner; it should call
  `zorg db status --root {root}`. Do not let this optional command delay the required commands.
- Build one shared async job runner around `vim.system` with a `jobstart` fallback:
  - captures stdout, stderr, and exit code;
  - returns structured results to command-specific callbacks;
  - never blocks the UI;
  - reports missing executables and nonzero exits through `vim.notify`;
  - preserves stderr details for troubleshooting.
- Make `:ZorgQuery` pass a normal inline SWOG query as a single CLI argument, while still supporting `--id @query/id`.
  This avoids breaking queries such as `#z/todo -did:*` into multiple positional arguments when the Rust CLI expects one
  query string.
- Display query output in a scratch buffer or quickfix-style location that can be read after the command completes. Do
  not parse SWOG output in Lua.
- Make `:ZorgFix` default to the current `.z` buffer path when no file arguments are provided. If the buffer is
  modified, save first only with an explicit bang or document that the user must write before running the command.
  Prefer conservative behavior.
- Use `zorg fix --json` only if the implementation consumes the JSON for better UX; otherwise preserve text output and
  surface it clearly. Do not invent a Lua fix planner.
- Make `:ZorgCapture` prefer `zorg capture --json` so the plugin can open or reveal the created destination when the
  JSON success payload includes `destination` and `zettel_id`. Fall back to showing text output if JSON is not
  parseable.
- Add command tests with a fake `zorg` executable on `PATH` or a stubbed job runner:
  - verifies argv for each required command;
  - verifies nonzero exit notification path;
  - verifies query argument handling;
  - verifies capture JSON result handling.

Out of scope:

- Interactive terminal UI for capture beyond delegating to the CLI.
- Parsing LIST output into custom Neovim objects.
- Running `zorg db reindex` automatically before every query.

Acceptance:

- Required commands call the current Rust CLI contract.
- CLI errors are visible in Neovim with useful detail.
- Query output and capture success are visible without leaving the user to hunt in `:messages`.
- Headless command tests pass.

## Phase 4: Optional UX Helpers And User Configuration

Owner: one Neovim/Lua agent.

Target repo: `../zorg-nvim`.

Purpose: finish the optional parts of Epic 8.3 without making opinionated defaults.

Scope:

- Add opt-in mappings under a configuration key such as:
  - `mappings.enabled = false` by default;
  - `mappings.prefix = "<leader>z"` by default when enabled;
  - mappings call existing commands or Lua functions rather than duplicating command logic.
- Provide small helper functions in Lua for common editor workflows:
  - query prompt using `vim.ui.input` and then `:ZorgQuery`;
  - capture prompt using `vim.ui.input` for title/body only if this remains a thin CLI delegation;
  - run fix on current buffer;
  - reindex configured root.
- Add optional completion for user commands where it is cheap and reliable:
  - command names / flags for known CLI flags;
  - file completion for fix file arguments;
  - no semantic completion that would require parsing the corpus in Lua.
- Ensure helpers respect configuration for `root`, CLI command, and LSP command.
- Add tests for mapping registration being opt-in and for helper functions constructing the same command paths as
  Phase 3.

Out of scope:

- Default global keymaps.
- Telescope, fzf-lua, snacks, or other third-party integrations.
- A custom query browser or note picker.

Acceptance:

- Users can enable a modest keymap set, but default setup remains noninvasive.
- Helpers are thin wrappers over the Phase 3 command runner.
- Tests prove mappings are not installed unless requested.

## Phase 5: Health, Documentation, And Cross-Repo Validation

Owner: one documentation/integration agent.

Target repos: primarily `../zorg-nvim`; read-only validation against `../zorg` and `../zorg-treesitter`; narrowly scoped
docs updates in `../zorg` only if a cross-repo contract changed during earlier phases.

Purpose: close Epic 8.4 and make the whole Neovim integration coherent for a new user.

Scope:

- Expand `lua/zorg/health.lua` so `:checkhealth zorg` reports:
  - Neovim version support;
  - `zorg` executable presence and version;
  - `zorg-ls` executable presence and version;
  - configured root and database path;
  - Tree-sitter runtime availability;
  - `zorg` parser availability when Neovim can check it;
  - query file availability on runtimepath.
- Update `README.md` with:
  - plugin manager examples;
  - local development install from sibling repos;
  - Tree-sitter parser install/registration guidance;
  - `require("zorg").setup()` defaults and examples;
  - command examples using the real CLI commands;
  - LSP startup expectations and troubleshooting;
  - health check instructions.
- Update `doc/zorg.txt` to match README and regenerate `doc/tags`.
- Add a final integration smoke test that can run in CI-like headless Neovim:
  - loads plugin from runtimepath;
  - opens a `.z` buffer;
  - verifies required commands exist;
  - verifies Tree-sitter query lookup if APIs are available;
  - stubs command execution for command-path checks;
  - stubs LSP startup for initialization option checks.
- Run available validation:
  - `nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/smoke.lua -c "qa"`
  - `stylua --check .` if installed;
  - `luacheck lua tests filetype.lua plugin ftplugin` if installed;
  - `cargo run -p zorg-cli -- --help` from `../zorg`;
  - `cargo run -p zorg-ls -- --version` from `../zorg`;
  - parser query/highlight validation from `../zorg-treesitter` if available.
- Record skipped optional validations explicitly when local tools are missing.

Out of scope:

- Release packaging beyond docs needed for MVP usage.
- CI service setup unless the repo already has CI conventions.

Acceptance:

- `:checkhealth zorg` gives actionable results for every external dependency.
- README and help docs are enough for a user to install the plugin, connect Tree-sitter, start `zorg-ls`, and run the
  required commands.
- Final headless smoke validation passes.
- Epic 8 definition of done is satisfied: `zorg-nvim` recognizes `.z`, loads Tree-sitter highlighting, starts `zorg-ls`,
  and exposes common commands.

## Suggested Agent Execution Prompts

Use one distinct agent instance per phase. Each prompt should include this plan file and the current repo paths.

1. "Implement Phase 1 from `sase_plan_zorg_epic8_neovim_integration.md` in `../zorg-nvim`. Focus only on filetype and
   Tree-sitter runtime/query loading. Do not change CLI or LSP behavior except docs needed for this phase."
2. "Implement Phase 2 from `sase_plan_zorg_epic8_neovim_integration.md` in `../zorg-nvim`. Focus only on `zorg-ls`
   setup, root detection, initialization options, health checks for LSP, and tests."
3. "Implement Phase 3 from `sase_plan_zorg_epic8_neovim_integration.md` in `../zorg-nvim`. Focus only on required Zorg
   commands, the shared async CLI runner, output/error surfaces, and command tests."
4. "Implement Phase 4 from `sase_plan_zorg_epic8_neovim_integration.md` in `../zorg-nvim`. Focus only on opt-in
   mappings, small UX helper functions, user-command completion, and tests."
5. "Implement Phase 5 from `sase_plan_zorg_epic8_neovim_integration.md`. Focus on health, README/help docs, final smoke
   validation, and cross-repo contract checks. Keep any `../zorg` or `../zorg-treesitter` edits limited to necessary
   documentation corrections."

## Cross-Phase Interfaces

- Phase 1 owns `lua/zorg/treesitter.lua`, runtime `queries/zorg/*`, filetype, and ftplugin behavior.
- Phase 2 owns `lua/zorg/lsp.lua` and LSP-related config fields.
- Phase 3 owns the shared CLI runner and required command callbacks in `lua/zorg/commands.lua`.
- Phase 4 may add helper modules, but it should call Phase 3 command functions instead of spawning jobs independently.
- Phase 5 owns docs and health finalization. It may refine health checks added earlier, but should avoid rewriting core
  command or LSP behavior unless tests reveal a bug.

## Risks And Mitigations

- Risk: Neovim Tree-sitter parser availability differs across user parser managers. Mitigation: only register
  filetype-to-language mapping, ship query files, document parser installation, and make health checks explicit.
- Risk: command wrappers accidentally drift from Rust CLI syntax. Mitigation: tests assert argv construction and Phase 5
  checks `zorg --help`.
- Risk: `:ZorgQuery` argument splitting breaks valid SWOG queries. Mitigation: pass ordinary query text as one argument
  and special-case known flag forms such as `--id`.
- Risk: LSP starts with the wrong root because Neovim `settings` are not the same as LSP initialization options.
  Mitigation: pass `initialization_options` with `rootPath` / `databasePath`, matching `crates/zorg-ls/src/config.rs`.
- Risk: plugin becomes opinionated and intrusive. Mitigation: no default mappings, no automatic indexing, no hidden
  writes, and no third-party plugin dependency.

## Overall Definition Of Done

- `../zorg-nvim` is a usable Neovim plugin for Zorg MVP.
- Opening `*.z` sets filetype `zorg`.
- Tree-sitter query files for `zorg` are available on runtimepath, and parser registration is documented and
  health-checked.
- `require("zorg").setup()` starts or can start `zorg-ls` with the configured root and database initialization options.
- `:ZorgIndex`, `:ZorgQuery`, `:ZorgFix`, and `:ZorgCapture` call the real Rust CLI and surface success/error output in
  Neovim.
- Optional mappings and helpers are available but disabled by default.
- `:checkhealth zorg`, README, and `:help zorg` reflect the final behavior.
- Headless Neovim smoke tests pass, and unavailable optional tooling is clearly reported rather than silently ignored.

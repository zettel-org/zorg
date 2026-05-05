---
create_time: 2026-05-05 13:00:33
status: wip
prompt: sdd/prompts/202605/zorg_nvim_boolean_config.md
---
# Plan: Diagnose and Fix `zorg.config` Boolean Load Failure

## Problem

`lazy.nvim` reports a failure while running the user's `zorg-nvim` config:

```text
.../lazy/zorg-nvim/lua/zorg/init.lua:4: attempt to index a boolean value
```

The current line 4 is:

```lua
local config = require("zorg.config").setup(opts)
```

That error means `require("zorg.config")` returned a boolean before `.setup` was indexed. In Lua `require` returns
`true` when a module file executes but returns no value, or when `package.loaded[modname]` has already been set to a
boolean. The current `zorg-nvim/lua/zorg/config.lua` returns its module table in every tracked commit, so the likely
root cause is not the current plugin source omitting `return M`.

## Evidence Gathered

- `../zorg-nvim/lua/zorg/init.lua` line 4 indexes the return value of `require("zorg.config")` directly.
- `../zorg-nvim/lua/zorg/config.lua` returns `M` and exposes `setup`.
- Historical tracked versions of `config.lua` also returned `M`.
- A headless check from the local checkout succeeds when `runtimepath` points at `../zorg-nvim`.
- The user's lazy spec calls `require("zorg").setup({...})` at the same line shown in the remote stack trace.
- The plugin uses the public module namespace `zorg` for both its public entrypoint and internal modules, so another
  runtimepath entry such as `~/.config/nvim/lua/zorg/config.lua`, or a stale `package.loaded["zorg.config"] = true`, can
  poison the internal `require` while still allowing `require("zorg")` to resolve to the plugin.

## Root Cause Hypothesis

The root cause is an unsafe internal module lookup in `lua/zorg/init.lua`: it assumes `require("zorg.config")` always
resolves to the sibling plugin module. On the failing machine, `zorg.config` is already a boolean or resolves to another
`zorg/config.lua` on `runtimepath` that returns no value. This converts the module to `true`, causing line 4 to fail.

This is plausible on machines that previously had custom Zorg Lua config under `~/.config/nvim/lua/zorg/...`, or
machines with stale Neovim/lazy bytecode/module state after plugin iteration.

## Fix Strategy

1. Add a small internal loader in `lua/zorg/init.lua` that resolves plugin modules relative to the actual `init.lua`
   file being executed instead of relying solely on runtimepath ordering.
2. Use that loader for the setup-time internal modules: `config`, `treesitter`, `commands`, `watcher`, `mappings`, and
   `lsp`.
3. Store loaded internal modules back into `package.loaded["zorg.<name>"]` so later internal `require("zorg.config")`
   calls from modules like `commands.lua`, `lsp.lua`, and `watcher.lua` reuse the known-good plugin module.
4. Validate module shape and produce a clear error if a sibling plugin module cannot be loaded or does not return a
   table. This turns the current misleading boolean-index crash into a diagnosable plugin-internal load error.
5. Add a headless Neovim regression test that pre-poisons `package.loaded["zorg.config"] = true` before
   `require("zorg").setup(...)`; the setup should still succeed and should replace the boolean with the plugin config
   table.
6. Run the existing `zorg-nvim` headless tests, plus the new regression test. Run formatting/lint checks if the tools
   are available.

## Scope

Primary changes are expected in `../zorg-nvim`:

- `lua/zorg/init.lua`
- a focused test under `tests/`, likely `tests/module_loading.lua` or an extension to `tests/smoke.lua`
- README/help docs only if the final behavior or troubleshooting guidance needs documenting

No Rust workspace changes are expected.

## Verification

- Reproduce the failure class with a headless script that sets `package.loaded["zorg.config"] = true` before setup.
- Confirm `require("zorg").setup({ lsp = { enabled = false }, treesitter = { enabled = false } })` succeeds.
- Confirm `type(require("zorg.config")) == "table"` and `type(require("zorg.config").setup) == "function"` after setup.
- Run the existing headless test suite from `../zorg-nvim`.
- Optionally advise the user to inspect the other machine with:

```vim
:lua print(vim.inspect(vim.api.nvim_get_runtime_file("lua/zorg/config.lua", true)))
:lua print(type(package.loaded["zorg.config"]), vim.inspect(package.loaded["zorg.config"]))
```

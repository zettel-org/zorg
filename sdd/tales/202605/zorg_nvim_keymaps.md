---
create_time: 2026-05-03 20:52:14
status: done
prompt: sdd/prompts/202605/zorg_nvim_keymaps.md
---
# Zorg Neovim Keymaps Plan

## Goal

Add useful Zorg keymaps to the chezmoi-managed Neovim plugin config at `home/dot_config/nvim/lua/plugins/zorg_nvim.lua`.
Every new keymap should be under `<leader>z`, and the total should stay at or below ten.

## Current Context

- `zorg_nvim.lua` currently disables the plugin's mapping layer with `mappings.enabled = false`.
- The plugin documents and implements an optional ten-map helper set for common day-to-day actions: capture, export
  current, fix, reindex, open, query, database status, watcher start, watcher status, and watcher stop.
- The chezmoi config already defines legacy mappings in `home/dot_config/nvim/lua/config/zorg.lua`:
  - `<leader>zi` appends to `~/org/inbox.zo`.
  - `<leader>zgi` opens the inbox.
  - `<leader>zgt` opens a scratch temp file.
- Enabling the plugin defaults unchanged would collide with `<leader>zi`.

## Proposed Keymaps

Use the plugin's built-in mapping support, with prefix `<leader>z` and a custom `index = "r"` suffix to avoid the
existing inbox mapping:

- `<leader>zc`: capture prompt.
- `<leader>ze`: export current zettel to Markdown stdout.
- `<leader>zf`: fix current buffer.
- `<leader>zr`: reindex root.
- `<leader>zo`: open zettel ID prompt.
- `<leader>zq`: query prompt.
- `<leader>zs`: database status.
- `<leader>zw`: start watcher.
- `<leader>zS`: watcher status.
- `<leader>zW`: stop watcher.

This uses all ten supported helper actions without hand-rolling wrappers in the chezmoi config, so future plugin
improvements to those helpers are inherited automatically.

## Implementation

1. Update `zorg_nvim.lua` to set `mappings.enabled = true`.
2. Set `mappings.prefix = "<leader>z"`.
3. Add the `keys` table with the suffixes listed above.
4. Keep existing CLI and LSP executable fallback configuration unchanged.

## Verification

1. Run a headless Neovim check with the local `zorg-nvim` checkout on `runtimepath`, execute this plugin spec's
   `config`, and assert all ten normal mode mappings exist.
2. Run Lua formatting or syntax checks available in the environment.
3. Review `git diff` to confirm the change is scoped to `zorg_nvim.lua`.

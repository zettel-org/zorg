---
create_time: 2026-05-04 18:00:50
status: done
prompt: sdd/prompts/202605/zorg_nvim_performance.md
---
# Plan: Investigate and Fix zorg.nvim-Related Neovim Slowness

## Context

The reported symptom is that Neovim became slow after installing `zorg-nvim`. The relevant code lives in the sibling
`../zorg-nvim` checkout, with Rust LSP and store behavior in this `zorg` workspace.

Initial investigation found that the startup-only Lua cost is small: a headless startup with this plugin on
`runtimepath` and `require("zorg").setup({ lsp = { enabled = false }, treesitter = { enabled = false } })` added roughly
2-3 ms in this environment. That points away from command registration as the primary cause.

The likely problem is the default editor integration path:

- `../zorg-nvim/lua/zorg/config.lua` enables LSP by default.
- `../zorg-nvim/lua/zorg/lsp.lua` installs a `FileType zorg` autocmd that starts `zorg-ls` for every Zorg buffer.
- `crates/zorg-ls/src/main.rs` advertises `TextDocumentSyncKind::FULL`, so Neovim sends the full buffer on every edit.
- `crates/zorg-ls/src/main.rs` runs live diagnostics on each `didChange`.
- `crates/zorg-ls/src/main.rs` calls `ServerState::refresh_store_snapshot()` on every `didSave`.
- `crates/zorg-ls/src/state.rs` refresh uses `Store::reindex()`.
- `crates/zorg-store/src/lib.rs` incremental reindex still recursively discovers every `.z` file, reads every source,
  hashes all content, and, when a change is detected, parses/resolves corpus-level state.

A quick synthetic check on an 800-file generated corpus showed initial reindex around 0.87s and one-file incremental
reindex around 0.69s on this machine. That is appropriate for explicit indexing or a background watcher, but it is too
much work to trigger from the editor LSP on every save. Large single-file zettels can also suffer from full-buffer
didChange diagnostics on each edit.

There is a second risk if users enable `watcher.autostart`: `zorg watch` can refresh the same store in the background
while `zorg-ls` also refreshes on save, causing duplicated CPU/disk work and possible SQLite writer contention.

## Goals

1. Preserve useful `zorg-nvim` behavior while making installation safe for normal editing.
2. Avoid corpus-wide work on the foreground edit path.
3. Avoid duplicate indexing when the watcher is enabled.
4. Keep defaults clear and documented so users can choose between low-latency editing and richer LSP behavior.
5. Add targeted regression coverage for startup/autocmd behavior and LSP save refresh policy.

## Non-Goals

- Do not rewrite the query, parser, or store architecture.
- Do not remove explicit commands like `:ZorgIndex`, `:ZorgQuery`, or `:ZorgWatchStart`.
- Do not make watcher autostart the only supported freshness mechanism.
- Do not introduce broad Neovim package-manager assumptions.

## Proposed Technical Direction

### Phase 1: Confirm the Exact User-Facing Hot Paths

- Add a focused local benchmark or reproducible headless script that opens a generated Zorg corpus through `zorg-nvim`,
  starts `zorg-ls`, edits a `.z` buffer, saves it, and records time until the LSP refresh log arrives.
- Capture a second scenario with `watcher.autostart = true` to verify duplicate index work and any SQLite contention
  symptoms.
- Capture a large-single-buffer edit case to estimate the full-sync didChange/live-diagnostic cost.

This phase should not commit generated corpora. Any benchmark fixture should use temporary directories and the existing
`tools/generate_large_corpus.py`.

### Phase 2: Make LSP Save Reindex Policy Explicit

Introduce a configuration option owned by the Neovim-facing contract, for example:

```lua
require("zorg").setup({
  lsp = {
    enabled = true,
    refresh_on_save = "diagnostics", -- or false / "reindex"
  },
})
```

The conservative target is to stop doing full store reindex from the default save path when a watcher is configured or
when low-latency editing is desired. Implementation can land in Rust LSP initialization options, not only in Lua,
because `didSave` behavior is owned by `zorg-ls`.

Candidate behavior:

- `refresh_on_save = "reindex"` preserves the current behavior for users who want save-driven freshness and do not run a
  watcher.
- `refresh_on_save = false` skips store reindex on save and publishes only live diagnostics for the saved buffer.
- `refresh_on_save = "diagnostics"` keeps cheap live diagnostics while avoiding corpus-wide store refresh.
- If watcher autostart is enabled, default to no LSP reindex unless explicitly overridden.

Exact option names should match the existing config style before implementation.

### Phase 3: Reduce Edit-Time Diagnostics Cost

If the large-single-buffer measurement shows meaningful lag, add an LSP-side diagnostics debounce or size guard:

- Continue accepting full text sync for compatibility in the first pass.
- Defer `live_diagnostics()` after rapid `didChange` events instead of parsing on every keystroke.
- Optionally skip live diagnostics above a configured buffer-size threshold and rely on save/check commands.

This should be implemented in `zorg-ls` rather than only in Lua, because the expensive parse/validate work happens in
the server.

### Phase 4: Clean Up Neovim Startup and Setup Semantics

Even though startup cost is not the main issue, make setup behavior predictable:

- Ensure `plugin/zorg.lua` only does minimal command registration and does not accidentally start watchers or LSP before
  user config is applied.
- Ensure `require("zorg").setup()` is idempotent and does not repeatedly create autocmds or restart watcher state in
  surprising ways.
- Consider adding a `lsp.autostart` option so users can keep commands and Tree-sitter enabled without automatic
  `zorg-ls` startup.

### Phase 5: Tests and Documentation

Add tests in `../zorg-nvim/tests` for:

- Default config behavior, including the new save-refresh/autostart settings.
- LSP initialization options passed from Lua to `zorg-ls`.
- Watcher-enabled config avoiding duplicate LSP reindex policy by default.
- Idempotent setup/autocmd behavior.

Add Rust tests in `crates/zorg-ls/tests` or focused unit tests for:

- `didSave` behavior under each refresh policy.
- Initialization option parsing.
- No store reindex when the policy disables it.

Update `../zorg-nvim/README.md`, `../zorg-nvim/doc/zorg.txt`, and `docs/cross_repo.md` with the performance model:

- `zorg-ls` provides live buffer diagnostics and navigation from snapshots.
- `zorg watch` is the preferred background freshness path for large corpora.
- Save-driven full reindex is opt-in or explicitly configurable.

## Validation

Run targeted checks first:

```sh
nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/config.lua -c "qa"
nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/lsp.lua -c "qa"
nvim --headless -u NONE -n --cmd "set rtp^=." -S tests/watcher.lua -c "qa"
cargo test -p zorg-ls
cargo test -p zorg-store incremental_reindex
```

Then run the cross-repo gate if the local toolchain supports it:

```sh
ZORG_NVIM_DIR=/home/bryan/projects/github/zettel-org/zorg-nvim \
  ./tools/validate_cross_repo.sh
```

Finally, rerun the synthetic large-corpus measurement. The expected result is that opening/editing/saving through
`zorg-nvim` no longer performs a corpus-wide reindex by default when the watcher is responsible for freshness, and users
can still opt into the old save-refresh behavior.

## Open Questions

- Should the default change be made in `zorg-ls` globally, or should `zorg-nvim` pass an explicit initialization option
  while preserving Rust defaults for other clients?
- Should live diagnostics be debounced immediately, or only after measuring a concrete large-buffer edit regression?
- Should watcher autostart remain disabled by default while still being the recommended large-corpus path?

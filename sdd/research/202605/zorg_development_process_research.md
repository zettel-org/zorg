---
research_date: 2026-05-04
title: Zorg development process and local binary update research
status: draft
source_context:
  - sdd/README.md
  - Cargo.toml
  - crates/*/Cargo.toml
  - README.md
  - docs/quickstart.md
  - docs/development.md
  - docs/cross_repo.md
  - tools/validate_cross_repo.sh
  - tools/release_dry_run.sh
verification:
  - cargo metadata --no-deps --format-version 1
  - cargo install --list
  - command -v -a zorg
  - command -v -a zorg-ls
  - cargo run -q -p zorg-cli -- --version
  - cargo run -q -p zorg-ls -- --version
---

# Zorg Development Process And Local Binary Updates

## Scope

This research captures a practical local-development process for the Rust Zorg workspace: which packages matter in a
developer install, how to update the binaries on a machine after source changes, and what adjacent habits reduce
surprises when working across the Rust, Tree-sitter, and Neovim repositories.

There is no `sdd/research/README.md` in this checkout. This file is placed under `sdd/research/202605/` to match the
month-directory convention used by generated SDD docs and adjacent research files.

## Current Workspace Shape

The Rust workspace has twelve packages:

| Package | Role | User-facing binary? |
| --- | --- | --- |
| `zorg-cli` | Command-line entry point. Owns `parse`, `check`, `db`, `query`, `watch`, `dash`, refactor, import/export, capture, and fix subcommands. | Yes: `zorg` |
| `zorg-ls` | Language server over stdio. | Yes: `zorg-ls` |
| `zorg-dash` | Terminal dashboard implementation used by `zorg dash`. | No; linked into `zorg-cli` through the default `dash` feature. |
| `zorg-watch` | Live indexing watcher service used by `zorg watch`. | No; exposed through `zorg-cli`. |
| `zorg-capture` | Capture/template logic used by `zorg capture`. | No; exposed through `zorg-cli`. |
| `zorg-fix` | Strict check and deterministic autofix logic. | No; exposed through `zorg-cli` and `zorg-ls`. |
| `zorg-query` | SWOG query evaluation. | No; exposed through `zorg-cli` and dashboard surfaces. |
| `zorg-refactor` | Structural refactor planning and application. | No; exposed through `zorg-cli` and LSP actions. |
| `zorg-store` | SQLite index, config resolution, status, and migrations. | No. |
| `zorg-parse` | Parser boundary using the generated sibling Tree-sitter parser. | No. |
| `zorg-core` | Shared model, spans, and diagnostics. | No. |
| `zorg-bridge` | Legacy import and Markdown export planning. | No; exposed through `zorg-cli`. |

The practical consequence is simple: a developer normally installs only `zorg-cli` and `zorg-ls`. The other packages are
still "relevant" because edits there flow into one or both binaries, but they do not need separate installation.

## Recommended Local Development Modes

### 1. Source-run mode for active edits

Use this while changing behavior and iterating quickly:

```sh
cargo run -p zorg-cli -- --version
cargo run -p zorg-cli -- check fixtures/corpus/minimal.z
cargo run -p zorg-ls -- --version
```

This guarantees the command is running from the current checkout, not from an older installed binary on `PATH`. It is the
best mode for verifying a source change before deciding whether to update global binaries.

For repeated manual testing, build once and run the target binaries directly:

```sh
cargo build -p zorg-cli -p zorg-ls
target/debug/zorg --version
target/debug/zorg-ls --version
target/debug/zorg db status --root fixtures/corpus --db /tmp/zorg-dev.sqlite3
```

This avoids the `cargo run` wrapper overhead while still using the current worktree.

### 2. PATH-install mode for daily personal use

Use this when you want your shell, editor, and Neovim integration to use the current source tree:

```sh
cargo install --path crates/zorg-cli --force
cargo install --path crates/zorg-ls --force
```

`cargo install` puts binaries under `~/.cargo/bin` unless `--root` is supplied. The installed binary names are `zorg` and
`zorg-ls`, not `zorg-cli`.

Use `--force` in the development loop because the workspace version is currently `0.1.0`; without `--force`, Cargo can
decide an already-installed package is current enough and leave an old binary in place.

Use `--locked` when checking that the install works from the committed dependency graph:

```sh
cargo install --path crates/zorg-cli --force --locked
cargo install --path crates/zorg-ls --force --locked
```

Use `--debug` only for short-lived local testing when install time matters more than runtime performance:

```sh
cargo install --path crates/zorg-cli --force --debug
cargo install --path crates/zorg-ls --force --debug
```

### 3. Editor-dev mode

For editor integration work, avoid reinstalling on every Rust edit by pointing the editor or local wrapper at
`target/debug/zorg-ls` after `cargo build -p zorg-ls`. This is especially useful when iterating on LSP behavior because a
server restart will pick up the freshly built binary without changing the global install.

For Neovim tests or wrappers that need a stable command name, create local dev wrappers outside the repo, for example
`~/bin/zorg-dev` and `~/bin/zorg-ls-dev`, that execute the current checkout's `target/debug` binaries or delegate to
`cargo run --manifest-path /path/to/zorg_100/Cargo.toml`. Keep those names distinct from `zorg` and `zorg-ls` so daily
use and experimental use do not silently swap.

## Updating Binaries After Source Changes

Use this decision tree:

1. If you only need to test the change once, use `cargo run -p zorg-cli -- ...` or `cargo run -p zorg-ls -- ...`.
2. If you need repeated local manual checks from this checkout, run `cargo build -p zorg-cli -p zorg-ls` and then execute
   `target/debug/zorg` or `target/debug/zorg-ls`.
3. If your shell/editor should use the new implementation globally, run both `cargo install --path ... --force` commands.
4. If only one binary is affected, reinstall only that binary. Examples:
   - CLI, store, query, capture, fix, bridge, watcher, or dashboard changes usually affect `zorg`.
   - LSP, store, parse, fix, refactor, or core changes affect `zorg-ls`.
   - Core parser/model changes often affect both, so reinstall both.

After installing, verify the exact executable resolution:

```sh
command -v -a zorg
command -v -a zorg-ls
cargo install --list | rg 'zorg-cli|zorg-ls|zorg$'
zorg --version
zorg-ls --version
```

In this local environment, `cargo install --list` reports `zorg-cli` and `zorg-ls` as installed from
`/home/bryan/projects/github/zettel-org/zorg/...`, while the current research checkout is
`/home/bryan/projects/github/zettel-org/zorg_100`. That means editing `zorg_100` does not update the installed binaries
unless they are reinstalled from `zorg_100` or the editor is pointed at `zorg_100/target/debug/...`.

Also note the local `PATH` order observed during research:

```text
/home/bryan/.pyenv/shims/zorg
/home/bryan/.cargo/bin/zorg
/home/bryan/.local/bin/zorg
/home/bryan/.cargo/bin/zorg-ls
```

When behavior looks stale, check `command -v -a` before debugging Rust. A shim or older local copy can hide the binary
you thought you installed.

## Tree-Sitter And Sibling Repos

`zorg-parse` compiles against the generated parser from the sibling `../zorg-treesitter` checkout. Before building Rust
after grammar changes, refresh the generated parser:

```sh
(cd ../zorg-treesitter && npm install && npm run generate)
cargo build --workspace
```

The Rust repo, `../zorg-treesitter`, and `../zorg-nvim` are meant to stay aligned. The cross-repo contract says Rust owns
semantics, Tree-sitter owns syntax structure and editor queries, and Neovim delegates to `zorg`, `zorg-ls`, and the
Tree-sitter parser.

For local validation across all three repos, use:

```sh
tools/validate_cross_repo.sh
```

Set these variables if the sibling checkouts are not adjacent to the Rust checkout:

```sh
ZORG_TREESITTER_DIR=/path/to/zorg-treesitter \
ZORG_NVIM_DIR=/path/to/zorg-nvim \
tools/validate_cross_repo.sh
```

The gate is intentionally heavier than a per-edit loop. It validates Rust workspace checks, watcher/refactor/import/export
contracts, Tree-sitter generation and fixture parsing, Neovim headless tests, and real-CLI Neovim smoke coverage.

## Suggested Day-To-Day Loop

For a normal Rust change:

```sh
cargo fmt
cargo test -p <package-you-changed>
cargo run -p zorg-cli -- --version
cargo run -p zorg-ls -- --version
```

For parser, store, query, or command-surface changes:

```sh
python3 tools/check_fixture_manifest.py
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

For changes that affect editor behavior, live indexing, or cross-repo contracts:

```sh
tools/validate_cross_repo.sh
```

For release confidence without publishing artifacts:

```sh
tools/release_dry_run.sh
```

`tools/release_dry_run.sh` requires clean Rust, Tree-sitter, and Neovim worktrees, so it is not a normal inner-loop
command. Use it near handoff or before cutting a release.

## Tips Beyond Installation

### Keep real notes isolated from tests

Most Zorg commands default to `~/zorg` and `<root>/.zorg/zorg.sqlite3`. During development, pass explicit temp roots and
database paths:

```sh
tmp_root="$(mktemp -d)"
tmp_db="$tmp_root/.zorg/zorg.sqlite3"
cargo run -p zorg-cli -- db reindex --root fixtures/corpus --db "$tmp_db"
```

This prevents tests, reindex experiments, import/apply flows, and refactor write-mode checks from mutating a real corpus.

### Treat `zorg watch` and `zorg-ls` freshness separately

`zorg watch` keeps the SQLite index current. `zorg-ls` reloads graph data on save-triggered refreshes and can be degraded
even if a watcher has already indexed the files. When editor graph behavior looks stale, check both the watcher status
and LSP logs before assuming a parser or query bug.

### Use feature flags intentionally

`zorg-cli` enables `zorg dash` by default through the `dash` feature. If you are debugging a minimal CLI build or trying
to isolate terminal UI dependencies, build without default features:

```sh
cargo build -p zorg-cli --no-default-features
```

For ordinary installs, keep the default feature set so the installed `zorg` command includes `zorg dash`.

### Prefer JSON surfaces for integration checks

For editor or automation work, prefer JSON-producing command paths such as `query --json`, `path/open --format json`,
refactor preview JSON, watcher `--format json`, import/export JSON, and dashboard `--once --json`. Text output is useful
for humans, but JSON catches contract drift earlier and avoids brittle scraping.

### Rebuild from the right worktree

This repository has several sibling worktrees (`zorg`, `zorg_100`, `zorg_101`, `zorg_102`). Before installing, run:

```sh
pwd
git status --short
cargo metadata --no-deps --format-version 1 | rg '"workspace_root"'
```

That small check prevents installing an older clone's binaries after making changes in a different worktree.

## Recommendation

Adopt three named modes and make them explicit in docs or shell aliases:

1. `source-run`: `cargo run -p ...` for correctness while editing.
2. `target-debug`: `cargo build -p zorg-cli -p zorg-ls` plus `target/debug/...` for editor/server iteration.
3. `path-install`: `cargo install --path crates/zorg-cli --force` and `cargo install --path crates/zorg-ls --force` for
   global daily use.

Do not try to install every `zorg-*` package. Install only the two binary crates, and let Cargo rebuild their library
dependencies from the current workspace. The main process risk is not missing a package; it is accidentally running a
binary installed from a different worktree.

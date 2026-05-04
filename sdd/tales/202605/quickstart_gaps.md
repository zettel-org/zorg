---
create_time: 2026-05-04 11:44:32
status: wip
prompt: sdd/prompts/202605/quickstart_gaps.md
---
# Plan: Fill Gaps In `docs/quickstart.md`

## Context

`docs/quickstart.md` was added in commit `55519c4` per the approved tale at `sdd/tales/202605/zorg_quickstart_docs.md`.
Re-reading the file against the plan and against the actual `zorg --help` / `zorg-ls --help` surface, every section the
plan required is present. The remaining gaps are usability and "first-run reality" issues: things a new user actually
trips over that the prior agent did not cover.

I smoke-tested the documented sample `start.z` again in a fresh `mktemp -d` root: `check`, `db reindex`, inline query,
saved query, and `path` all work as written. So the gaps are additive, not corrective.

## Gaps Identified

1. **`PATH` / `~/.cargo/bin` stumble.** After `cargo install --path ...`, the binaries live in `~/.cargo/bin`. If that
   directory is not on `$PATH`, `zorg --version` errors with "command not found". The current doc says "Verify that both
   commands are on your `PATH`" but does not tell the user how to fix it. This is the single most common first-run
   failure for any `cargo install`-based tool and worth one short note.

2. **`ZORG_ROOT` shortcut never demonstrated.** The doc lists `ZORG_ROOT` under "Configuration And Next Reading" as a
   knob, but every example still spells `--root ~/zorg`. Showing how to export `ZORG_ROOT=~/zorg` once so the rest of
   the session can drop the flag is the actual ergonomic win the env var exists for. The plan specifically asked the
   configuration section to "mention `ZORG_ROOT`" — it does, but only nominally.

3. **Silent-success expectation for `zorg check`.** On a clean corpus `zorg check` exits 0 with no stdout. A first-time
   user staring at a blank line wonders if the command ran. One sentence ("no output means success") removes that doubt.

4. **Capture loop never closes.** The optional `zorg capture` example writes a new todo to `~/zorg/inbox.z` and tells
   the user to reindex, but never shows the visible payoff (the new zettel appearing in the saved query results).
   Re-running `zorg query --id @start/queries/open` after capture would close the loop in one extra command.

5. **Capture destination is implicit.** The template carries `dest::inbox.z`, so capture creates `~/zorg/inbox.z`. The
   doc never says so. A sentence naming the resulting file makes the example concrete.

6. **`zorg watch` lifecycle.** The doc starts the watcher but does not mention stopping it (Ctrl-C). Trivial but worth
   one phrase.

7. **`../zorg-nvim` is the obvious editor and is unmentioned.** The "Editor Integration" section talks abstractly about
   "your editor's LSP configuration." The repo has a sibling Neovim plugin (`../zorg-nvim`, referenced from README and
   `docs/cross_repo.md`). For users who picked Zorg, that is the canonical client. A single pointer line is enough; the
   detailed configuration belongs in `lsp.md` and the plugin's own README, not here.

8. **Repository root is implicit for `cargo install`.** The "Install From Source" section says "From the repository
   root" once at the top, but the `cargo install --path crates/...` invocations are relative paths and silently fail
   outside the repo. Worth tightening so that intent is unambiguous.

9. **No troubleshooting hooks.** When something fails — parser missing, indexed_zettel: 0 because the user wrote `.zo`
   instead of `.z`, PATH not set — the user has no recovery path from this doc. A compact "If something goes wrong"
   subsection (4–6 lines, pointers only) closes the loop without bloating the guide.

The plan said "do not over-document every command from README" and "keep the guide user-facing and command-oriented."
Each of the items above is a user-facing usability fix, not a command catalog expansion, so they fit inside that
guardrail.

## Non-Goals

- No new sections beyond the eight already in the doc; gaps are filled by small additions inside existing sections (plus
  one short "If something goes wrong" block at the end of "Validate And Index" or before "Configuration And Next
  Reading").
- No changes to README.md — the documentation map entry is already correct.
- No changes to the sample `start.z` content; it parses, indexes, and queries cleanly today.
- No new `cargo run -p zorg-cli` examples; installed-binary form stays.
- No tutorial-style prose for SWOG, capture templates, or LSP feature matrices — those are owned by `query.md`,
  `capture.md`, and `lsp.md`.

## Documentation Edits

All changes land in `docs/quickstart.md`.

1. **Install From Source**
   - Add one line after `cargo install ...` noting that the binaries land in `~/.cargo/bin` and that directory must be
     on `$PATH`. Mention the standard remedy (add `export PATH="$HOME/.cargo/bin:$PATH"` to the shell rc) without
     prescribing a shell.
   - Tighten the "From the repository root" framing so the relative `--path crates/...` invocations are unambiguous.

2. **Create A First Corpus**
   - Add an optional `export ZORG_ROOT="$HOME/zorg"` snippet immediately after `mkdir -p ~/zorg`, with one sentence
     saying that exporting it lets later commands drop `--root ~/zorg`. Keep every subsequent example with the explicit
     `--root ~/zorg` flag so copy-paste still works for users who skipped the export.

3. **Validate And Index**
   - One sentence after the `zorg check` block: a clean corpus prints nothing and exits 0; any output means a strict
     diagnostic.

4. **Start Using Zorg**
   - After `zorg watch`: one phrase noting Ctrl-C stops it.
   - In the capture subsection: name the resulting file (`~/zorg/inbox.z`), and add a follow-up line re-running
     `zorg query --id @start/queries/open --root ~/zorg` so the user sees the captured todo show up alongside
     `@start/today`.

5. **Editor Integration**
   - Add a single line pointing Neovim users to `../zorg-nvim`, with a hand-off to `docs/lsp.md` for protocol details
     and to the plugin's own docs for installation. Keep the rest of the section editor- agnostic.

6. **New short subsection: "If Something Goes Wrong"**
   - Inserted as the second-to-last section, before "Configuration And Next Reading". Bullet list, ≤6 lines, each
     pointing at a remedy:
     - `zorg: command not found` → `~/.cargo/bin` not on `$PATH`.
     - `cargo install` fails on parser symbols → regenerate `../zorg-treesitter` parser per Prerequisites.
     - `zorg db status` reports `discovered_files: 0` → confirm files use the `.z` extension and live under `--root`.
     - `zorg check` reports diagnostics → fix at the reported line; legacy `.zo*` syntax is rejected by design (link to
       `docs/syntax.md` and `docs/fix.md`).
   - This subsection is pointers-only; remedies live in the linked docs.

## Verification

- Re-run `cargo run -p zorg-cli -- --help` and `zorg-ls --version` to confirm command surface unchanged.
- Re-run the temp-root smoke loop:
  ```
  tmp=$(mktemp -d)
  cp ~/zorg/start.z "$tmp/"   # or recreate per the doc heredoc
  zorg check --root "$tmp"
  zorg db status --root "$tmp"
  zorg db reindex --root "$tmp"
  zorg query '#z/todo -did:*' --root "$tmp"
  zorg query --id @start/queries/open --root "$tmp"
  zorg path @start/today --root "$tmp"
  zorg capture --root "$tmp" --template @start/templates/todo \
    --id @inbox/first-capture --title "First captured note" \
    --source "quickstart" --body "Write the next action."
  zorg db reindex --root "$tmp"
  zorg query --id @start/queries/open --root "$tmp"
  ```
  The final query should now show both `@start/today` and the captured `@inbox/first-capture`. Confirm `~/zorg/inbox.z`
  was created (so the "destination" sentence is accurate).
- Confirm `ZORG_ROOT="$tmp" zorg db status` works without `--root`, so the env-var snippet is honest.
- Visual scan of the rendered Markdown for broken relative links (`syntax.md`, `query.md`, `capture.md`, `lsp.md`,
  `import_export.md`, `development.md`, `fix.md`).

## Out Of Scope

- Editing README.md, `docs/lsp.md`, or `docs/development.md`.
- Adding a "Quickstart" section to any other doc.
- Documenting `zorg parse`, `zorg promote/move/extract`, `zorg fix`, or `zorg open`. These are intentionally omitted by
  the original plan.
- Translating the quickstart for non-`~/zorg` workflows (e.g. multi-root, fixture corpora) — `--root`/`--db` precedence
  is named in the existing Configuration section, which is enough for the first-use audience.

---
create_time: 2026-05-02 16:37:38
status: wip
prompt: sdd/prompts/202605/zorg_sibling_commit_stop_hook.md
---
# Plan: Add Zorg Sibling Commit Stop Hook

## Problem

The `zorg` project has sibling repositories under `/home/bryan/projects/github/zettel-org`, including `zorg-nvim` and
`zorg-treesitter`, but it does not currently have a repo-local sibling commit stop hook. The SASE project already solves
the same class of problem with `tools/sase_sibling_commit_stop_hook`: when an agent finishes, the hook checks sibling
repositories plus the chezmoi repo for dirty work, blocks once per session, and gives the agent runtime-specific commit
instructions.

The target `zorg` repo currently has no `tools/`, `.claude/`, `.gemini/`, or project-specific Codex hook files. Codex
hooks are managed through chezmoi at `~/.local/share/chezmoi/home/dot_codex/hooks.json`, with the live copy at
`~/.codex/hooks.json`. That existing global Codex hook already runs the SASE sibling hook path:

```json
"\"${CODEX_PROJECT_DIR:-$PWD}\"/tools/sase_sibling_commit_stop_hook"
```

That command is safe for SASE repos because the script exists there, but adding `zorg_sibling_commit_stop_hook` should
not make unrelated Codex sessions fail because a project lacks the new script.

There are also existing uncommitted changes in the target `zorg` repo:

- `docs/cross_repo.md`
- `sdd/epics/202605/zorg_epic1_foundations.md`

Those should be preserved and not folded into this work unless the implementation directly requires touching them.

## Goals

1. Add `tools/zorg_sibling_commit_stop_hook` to the `zorg` repo, mirroring the SASE sibling hook behavior and runtime
   semantics.
2. Check primary `zorg-*` sibling repositories and `~/.local/share/chezmoi`.
3. Skip ephemeral agent/workspace clones such as names ending in `_<number>`.
4. Block only once per session via a marker file under `${SASE_TMPDIR:-/tmp}`.
5. Emit correct stop-hook responses for Claude, Gemini, and Codex.
6. Add project-level Claude and Gemini hook config in the `zorg` repo.
7. Add Codex coverage through the chezmoi-managed Codex hook config without breaking SASE or other repositories.

## Implementation Plan

### 1. Add the repo-local Zorg sibling hook

Create `tools/zorg_sibling_commit_stop_hook` in `/home/bryan/projects/github/zettel-org/zorg`, based closely on
`/home/bryan/projects/github/sase-org/sase/tools/sase_sibling_commit_stop_hook`.

Keep these behaviors unchanged:

- resolve project dir from `CLAUDE_PROJECT_DIR`, `GEMINI_PROJECT_DIR`, `CODEX_PROJECT_DIR`, then `pwd`
- honor `SASE_DISABLE_COMMIT_STOP_HOOK`
- parse Gemini hook input enough to preserve the existing runtime shape
- return Codex JSON `{ "decision": "block", "reason": ... }`
- return Gemini JSON `{ "decision": "deny", "reason": ... }`
- return Claude stderr text plus exit code `2`
- use a once-per-session marker file

Adapt these details:

- script name and marker file prefix: `zorg_sibling_hook_done_${session_id}`
- sibling glob: `"$PROJECT_DIR"/../zorg-*/`
- user-facing messages: refer to `zorg` sibling repos, but keep the same commit instructions:
  - Gemini gets `sase commit -m '<msg>'`
  - Claude/Codex get `use your /sase_git_commit skill from inside that repo`

Make the script executable.

### 2. Add Claude project-level hook config

Add `/home/bryan/projects/github/zettel-org/zorg/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"$CLAUDE_PROJECT_DIR\"/tools/zorg_sibling_commit_stop_hook",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

This mirrors the SASE project-level Claude configuration while pointing at the Zorg-specific script.

### 3. Add Gemini project-level hook config

Add `/home/bryan/projects/github/zettel-org/zorg/.gemini/settings.json`:

```json
{
  "hooks": {
    "AfterAgent": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "\"$GEMINI_PROJECT_DIR\"/tools/zorg_sibling_commit_stop_hook",
            "timeout": 60000
          }
        ]
      }
    ]
  }
}
```

This matches SASE's Gemini hook event and timeout shape.

### 4. Add Codex hook coverage through chezmoi

Update `~/.local/share/chezmoi/home/dot_codex/hooks.json` so Codex can run either project-specific sibling hook when
present:

- keep `sase_commit_stop_hook` first
- keep the existing SASE sibling hook command, but guard it with `[ -x ... ]` so non-SASE repos do not fail
- add a guarded Zorg sibling hook command after it

The intended command shape is:

```json
"p=\"${CODEX_PROJECT_DIR:-$PWD}/tools/sase_sibling_commit_stop_hook\"; [ ! -x \"$p\" ] || \"$p\""
```

and:

```json
"p=\"${CODEX_PROJECT_DIR:-$PWD}/tools/zorg_sibling_commit_stop_hook\"; [ ! -x \"$p\" ] || \"$p\""
```

Use the same `timeout` as the existing Codex sibling hook (`300`). Then run `chezmoi apply` and verify
`~/.codex/hooks.json` matches the source.

### 5. Add or skip tests based on the repo's test shape

First check whether `zorg` has any shell-script or hook test patterns. If it does, add a focused test modeled on SASE's
`tests/test_sibling_commit_stop_hook.py`.

At minimum, manually verify the hook script with temporary git repositories:

- clean sibling repos exit successfully
- dirty `../zorg-nvim`-style sibling repo blocks
- dirty `../zorg_100`-style workspace is ignored
- second run with the same `SASE_AGENT_TIMESTAMP` exits cleanly
- Codex runtime emits parseable JSON with `decision: block`
- Gemini runtime emits parseable JSON with `decision: deny`

Because the Codex hook config is in chezmoi, also validate the guarded command exits successfully from a project without
the Zorg hook.

### 6. Verification

Run JSON and shell syntax checks:

```bash
jq . .claude/settings.json .gemini/settings.json
bash -n tools/zorg_sibling_commit_stop_hook
jq . ~/.local/share/chezmoi/home/dot_codex/hooks.json ~/.codex/hooks.json
```

Run repo checks where available:

```bash
cargo test
```

from `/home/bryan/projects/github/zettel-org/zorg`, and:

```bash
chezmoi apply
```

from the normal environment after updating the chezmoi source. If the chezmoi repo has its own check command, run that
as well.

## Expected Outcome

Claude, Gemini, and Codex sessions launched in the `zorg` repo will stop once when `zorg-*` sibling repositories or the
chezmoi repo contain uncommitted changes. The hook will direct agents to commit those sibling changes from the correct
repo before continuing, while leaving unrelated Codex projects unaffected by guarding project-specific hook commands.

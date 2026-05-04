---
create_time: 2026-05-04 11:42:33
status: wip
prompt: sdd/prompts/202605/zorg_help_beautification.md
---
# Zorg Help Beautification Plan

## Goal

Make `zorg -h` / `zorg --help` feel polished and deliberately designed while keeping it reliable in scripts, terminals,
and tests. The requested visible outcomes are:

- Add color to the top-level help output.
- Sort all top-level subcommands alphabetically.
- Improve layout enough that the command list is easier to scan and feels like a first-class CLI surface.

## Current Shape

`crates/zorg-cli/src/main.rs` implements CLI dispatch manually. The top-level help is currently a single static string
in `print_help()`. Subcommands are dispatched in a match near `main()`, and command-specific help text is also printed
manually.

The current top-level order follows historical implementation order rather than alphabetical order. The smoke test for
`zorg --help` checks only broad substrings, so a layout redesign should not require large test churn.

## Design Direction

The help output should read like a compact command palette:

- A colored product header, e.g. `zorg 0.1.0`, with `zorg` emphasized.
- A clear `Usage` section with syntax highlighted separately from section labels.
- An alphabetized `Commands` table with command names visually distinct from arguments.
- Short descriptions kept in a consistent column so scanning down the list is easy.
- A small `Options` section using the same treatment as command rows.
- A concise footer that explains the current scope without drowning out the command table.

The color palette should be tasteful and functional:

- Cyan/teal for the binary name and command names.
- Muted gray/dim style for metavariables, punctuation, and footer text.
- Bold section labels for structure.
- No saturated rainbow treatment; the visual hierarchy should be quiet and useful.

## Terminal And Script Behavior

Implement color without pulling in a new dependency unless the standard-library approach becomes awkward. Since the CLI
already imports `std::io::IsTerminal`, use stdout TTY detection to decide whether ANSI color is enabled.

Respect `NO_COLOR` as a hard opt-out. This keeps `zorg --help` pleasant interactively while ensuring captured help
remains plain text in tests, docs, pipes, and tooling.

The current user request only asks for top-level `zorg -h`, so command-specific help can remain plain for now. The
implementation should still make the color helpers reusable if later help screens get the same treatment.

## Implementation Steps

1. Introduce small local formatting helpers in `crates/zorg-cli/src/main.rs`:
   - A `HelpStyle` or equivalent that knows whether color is enabled.
   - Minimal functions for section labels, command names, usage text, metavariables, and dim text.
   - A `should_color_help()` helper using stdout TTY detection plus `NO_COLOR`.

2. Replace the static top-level help string with structured rendering:
   - Build a list of command rows as data.
   - Sort the command rows by command label before rendering.
   - Format all rows through a shared row renderer so alignment is stable.

3. Keep behavior stable:
   - `zorg -h` and `zorg --help` still exit successfully.
   - `zorg --help` captured by tests remains plain text because stdout is not a TTY.
   - Existing durable substrings such as `Usage: zorg` and `dash [--root PATH]` remain present in non-TTY output.

4. Add or tighten tests:
   - Assert top-level commands appear in alphabetical order in captured help.
   - Assert help output remains ANSI-free under normal captured test execution.
   - Keep existing substring checks for compatibility.

5. Verify:
   - `cargo test -p zorg-cli zorg_help_works`
   - `cargo test -p zorg-cli`
   - Manually inspect `cargo run -q -p zorg-cli -- -h` output. Captured output will be plain unless run in a TTY, so use
     the helper behavior and tests to validate color policy.

## Acceptance Criteria

- Top-level subcommands are alphabetically sorted.
- Interactive help uses ANSI color unless `NO_COLOR` is set or stdout is not a terminal.
- Non-interactive help remains plain text and test-friendly.
- The resulting `zorg -h` output is more legible, aligned, and visually intentional without changing command behavior.

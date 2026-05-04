---
create_time: 2026-05-04 16:09:17
status: wip
prompt: sdd/prompts/202605/legend_land_agent.md
---
# Legend Land Agent Launch Plan

## Context

`sase bead work <epic_id>` already launches one phase agent per ready phase plus a final landing agent that invokes the
tag-resolved `#bd/land_epic` xprompt. The recently added legend bead path (`sase bead work <legend_id>`) launches one
epic-planning agent per `epic_count`, and `#bd/land_legend` already exists in `src/sase/default_config.yml`, but the
legend launcher does not currently resolve or render that final landing prompt.

The goal is to make legend work mirror epic work: after the epic-planning chain has completed and the generated epic
work has had a chance to finish, launch one final agent that invokes `#bd/land_legend:<legend_id>` and can close the
legend / mark the legend plan file done.

## Current Design Facts

- `src/sase/bead/cli_work.py` has separate handlers for epic and legend work.
- Epic work resolves both `work_phase_bead` and `land_epic` xprompts before rendering via `render_multi_prompt`.
- Legend work currently calls `render_legend_multi_prompt(plan, vcs_context=...)` with no xprompt dependency, so it
  cannot substitute a user override for the landing prompt.
- `src/sase/bead/xprompts.py` exposes `resolve_work_phase_xprompt()` and `resolve_land_epic_xprompt()`, but no
  `resolve_land_legend_xprompt()`.
- `src/sase/xprompt/tags.py` has `land_epic` but no `land_legend` enum value.
- `bd/land_legend` in `src/sase/default_config.yml` has no semantic tag.
- `LegendWorkPlan` currently stores only the planning assignments. The Rust payload already gives enough information to
  derive a final land wait target in Python, so the first implementation should avoid touching `sase-core` unless tests
  expose an actual backend contract gap.

## Proposed Behavior

For a legend bead `zorg-7` with `epic_count=3`, dry-run output should render four multi-prompt segments:

1. Epic-planning segment for epic 1, named `zorg-7.1.0`.
2. Epic-planning segment for epic 2, named `zorg-7.2.0`, waiting on `zorg-7.1`.
3. Epic-planning segment for epic 3, named `zorg-7.3.0`, waiting on `zorg-7.2`.
4. Final land segment, named `zorg-7`, waiting on `zorg-7.3`, invoking `#bd/land_legend:zorg-7`.

The wait target should follow the existing legend assignment convention (`<legend_id>.<epic_number>`) rather than the
planning segment name (`<legend_id>.<epic_number>.0`), because the existing chain already waits on the root workflow
created by each auto-epic planning agent.

If a user supplies a custom xprompt tagged `land_legend`, the renderer should use that custom xprompt name exactly as
epic landing does for `land_epic`.

## Implementation Steps

1. Add `land_legend` to the xprompt tag system.
   - Add `land_legend = "land_legend"` to `XPromptTag`.
   - Add `tags: land_legend` to `bd/land_legend` in `default_config.yml`.
   - Extend `tests/test_bead_xprompt_tags.py` to assert parsing, built-in loading, and resolver behavior.

2. Add a resolver for the legend landing xprompt.
   - Add `resolve_land_legend_xprompt(project: str | None = None)` in `src/sase/bead/xprompts.py`.
   - Keep the same strict semantics as `resolve_land_epic_xprompt`: missing tag should raise `BeadXPromptNotFoundError`,
     duplicate tags should raise the loader's `ValueError`.

3. Extend the legend work plan model with final land metadata.
   - Add `land_agent_name` and `land_waits_on` to `LegendWorkPlan`.
   - In `_legend_plan_from_payload`, derive:
     - `land_agent_name = legend_id`
     - `land_waits_on = ()` if there are no assignments, otherwise `(f"{legend_id}.{last_epic_number}",)`
   - The no-assignment case should remain defensive only; current validation requires positive `epic_count`.

4. Render the final legend land segment.
   - Change `render_legend_multi_prompt` to accept `land_legend_xprompt: Workflow`.
   - Keep the existing VCS prefix behavior for all segments, including the final land segment.
   - Append a final segment with:
     - `%name:<plan.land_agent_name>`
     - `%approve`
     - optional `%w:<comma-joined plan.land_waits_on>`
     - `#<land_legend_xprompt.name>:<plan.legend_id>`
   - Do not add `%epic` to the final landing segment.

5. Wire the CLI handler through the resolver and update summaries/collisions.
   - In `_handle_legend_bead_work`, resolve `resolve_land_legend_xprompt()` before rendering, and handle
     missing/duplicate xprompt errors the same way the epic handler does.
   - Pass `land_legend_xprompt` into `render_legend_multi_prompt`.
   - Include `plan.land_agent_name` in `_expected_legend_agent_names()` so live-collision checks block a duplicate final
     land agent.
   - Update `_print_legend_work_plan_summary()` to say there are N epic agents plus 1 land agent and show the land wait.
   - Update the success count/message so live launch reports `len(plan.assignments) + 1` total agents while still making
     clear that N of them are epic-planning agents.

6. Update focused tests.
   - `tests/test_bead/test_work.py`:
     - assert `LegendWorkPlan.land_agent_name` and `land_waits_on`
     - update legend render snapshots from N to N+1 segments
     - add a user override assertion for custom `land_legend` prompt names
     - assert VCS prefix applies to the final land segment
   - `tests/test_bead/test_cli_work_legend.py`:
     - update dry-run/live launch expected segment counts
     - assert `#bd/land_legend:<legend_id>` appears
     - assert final `%name:<legend_id>` and `%w:<legend_id>.<last_epic_number>` appear
     - assert collision helpers include the final land agent
   - `tests/test_bead_xprompt_tags.py`:
     - assert `land_legend` parses, resolves, and is present on the built-in prompt.

7. Verify with targeted tests first, then broader bead coverage.
   - Run:
     - `just test tests/test_bead_xprompt_tags.py tests/test_bead/test_work.py tests/test_bead/test_cli_work_legend.py`
   - If those pass, run:
     - `just test tests/test_bead`
   - If changes touch formatting-sensitive YAML or Python style, run:
     - `just fmt-py-check`
     - `just lint-keep-sorted`

## Risks and Checks

- The main behavioral risk is waiting on the wrong agent name. The existing legend chain uses waits like
  `<legend_id>.1`, not `<legend_id>.1.0`; the final land segment should follow that established convention.
- The second risk is counting/reporting confusion. Tests should distinguish epic-planning agent count from total
  launched segments after the land agent is added.
- The third risk is accidentally making `bd/land_legend` non-overridable. The resolver/tag tests should cover custom
  xprompt substitution just like `land_epic`.

## Out of Scope

- Changing the `#bd/land_legend` prompt copy beyond adding its semantic tag.
- Changing the Rust `sase-core` bead planner payload unless the Python derivation proves insufficient.
- Changing how legend epic-planning agents create child epic beads or how `bd/new_epic` launches phase work.

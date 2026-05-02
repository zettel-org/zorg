---
create_time: 2026-05-02 16:53:55
status: wip
prompt: sdd/prompts/202605/agent_second_level_name_groups.md
---
# Agent Second-Level Name Groups

## Context

The `sase ace` Agents tab already renders grouped banners through `sase.ace.tui.models.agent_groups` and
`widgets/_agent_list_render_banner.py`. In STANDARD mode, a panel currently groups as:

```
project -> optional ChangeSpec -> first dotted name segment
```

For agents named `foo.alpha`, `foo.beta`, or `foo.bar.baz`, the existing deepest group is `foo`. The requested behavior
adds a second name namespace group for agents whose names share the same first two dotted segments, such as
`foo.bar.one` and `foo.bar.two`. That new `foo.bar` group should appear below the existing `foo` group and above the
matching agents.

The current tree model already supports arbitrary tuple group keys and level-based rendering, but the grouping key model
only stores a single `name_root`. Styling also treats every deepest non-ChangeSpec banner as the same name-root visual.

## Goals

- Add an additional visible group level for agent names that share the same `<foo>.<bar>.` prefix.
- Keep the existing `<foo>.` group behavior intact and make the new `<foo>.<bar>` group nested beneath it.
- Make the new level visually distinct from project, ChangeSpec/time-window, and existing name-root banners.
- Preserve existing behavior for singleton groups: do not add noise for one-off second-level prefixes.
- Preserve workflow-child adjacency and grouping inheritance from parent agents.
- Keep BY_DATE's time-window hierarchy unchanged unless code structure requires defensive handling.
- Cover tree shape, fold keys, rendering, gutters, and row lookup with focused tests.

## Product Design

The new namespace tier should read as a refined subcluster rather than another heavy heading. I will keep the existing
visual hierarchy:

```
▌ project ━━━
│  ▎ ChangeSpec ───
│  │  ▸ foo ───
│  │  │  ◦ foo.bar ┈┈┈
│  │  │  │  agent rows
```

Design choices:

- Existing `<foo>` groups stay teal and use the current branch glyph `▸`.
- New `<foo>.<bar>` groups use a lighter, more delicate dotted rule and a small hollow bullet-style glyph, likely `◦`,
  with a soft violet accent. This differentiates the subnamespace without competing with higher-level banners.
- Agent rows under the new tier receive one additional vertical gutter segment, so the nested structure remains clear in
  narrow terminals.
- Banner labels show `foo.bar` instead of only `bar`. That makes the group self-contained and keeps scanability high
  when users jump between collapsed banners.
- The summary chip remains unchanged (`N agents · ...`) and still counts only top-level agents.

## Technical Design

Extend grouping keys with a second namespace field:

```
name_root = "foo"
name_branch = "foo.bar"
```

Only populate `name_branch` when the effective agent name has at least three dotted segments and therefore starts with
`<foo>.<bar>.`. Names like `foo.bar` remain members of the `foo` group but do not create a `foo.bar` subnamespace,
because they do not start with `foo.bar.`.

Tree building changes:

- Compute root counts as today for `name_root`.
- Compute branch counts per visible root parent for `name_branch`.
- Emit the root banner when two or more agents share `name_root`, as today.
- Under an emitted root banner, emit a branch banner only when two or more agents share the same `name_branch`.
- Let unbranched or singleton-branched agents remain directly under the root group, before or near branch groups using a
  deterministic sort.
- Add branch fold keys as `(*parent_key, name_root, name_branch)`; in 3-level STANDARD mode that becomes
  `(project, changespec, "foo", "foo.bar")`.
- Update `enumerate_group_keys()` to include the new branch keys only when the banner would be visible.
- Update `walk_order()` so branch-capable agents sort together beneath the root without disrupting existing project,
  ChangeSpec, bucket, and root ordering.

Rendering changes:

- Add new banner style constants for the branch tier in `_agent_list_styling.py`.
- Teach `_agent_list_render_banner.py` to select that style for group rows that represent a second-level name namespace.
- Extend tier gutter style selection so branch banners and their child agent rows receive the right number and color of
  guide segments.
- Keep width/chip alignment logic unchanged; only glyph, style, rule character, and effective nesting depth change.

BY_DATE should remain bucket -> 4-hour window -> one-hour window. Because BY_DATE suppresses `name_root`, the new
namespace branch should be inactive there. BY_STATUS can support the new nested namespace naturally under status bucket
-> `foo` -> `foo.bar`, since it already uses name-root grouping.

## Test Plan

Add or update tests in the existing agent grouping suites:

- Model tests for STANDARD 3-level panels:
  - `foo.bar.one` and `foo.bar.two` render project -> ChangeSpec -> `foo` -> `foo.bar` -> agents.
  - `foo.bar` plus `foo.bar.one` does not create `foo.bar`, because only one agent starts with `foo.bar.`.
  - multiple second-level groups under one root sort deterministically.
  - `enumerate_group_keys()` includes the branch key only when visible.
- Fold tests:
  - collapsing a branch key hides only that branch's agents.
  - collapsing the parent `foo` still hides all nested branch rows.
- Widget/render tests:
  - row entries include the extra banner and agent highlight/row resolution still land on the correct agent.
  - branch banner plain text and styles are distinct from the root banner.
  - agent rows under the branch carry the extra gutter segment.
- Grouping-mode tests:
  - BY_STATUS supports status -> root -> branch.
  - BY_DATE remains unchanged and emits no name-root or branch namespace banners.

Validation should run the targeted TUI grouping/rendering tests first, then the repository's standard check target.

## Risks And Mitigations

- Risk: existing navigation and fold caches assume a maximum of three group levels. Mitigation: inspect callers that
  compare `group.level` or use `len(group_key)` and update tests around jump hints, folding, and `j`/`k` stops.
- Risk: row density gets too busy with four visible tiers. Mitigation: make the new tier visually light and rely on
  gutters rather than heavy headings.
- Risk: label ambiguity if only `bar` is shown. Mitigation: render `foo.bar` for branch banners.

## Non-Goals

- Do not redesign grouping modes or add a new user-facing grouping mode.
- Do not change agent naming, loading, or artifact storage.
- Do not alter BY_DATE's time-window hierarchy.
- Do not add configuration for group thresholds in this pass.

---
create_time: 2026-05-03 23:56:14
status: done
prompt: sdd/prompts/202605/zorg_dash_epic19.md
bead_id: zorg-6.3
tier: epic
legend_bead_id: zorg-6
---
# Zorg Dash Epic 19 Implementation Plan

## Objective

Implement Epic 19 from `sdd/legends/202605/zorg_dash_next_improvements.md`: make the Today panel a daily work surface.
Users should be able to complete, postpone, and schedule todo rows safely; switch Today between todos, diagnostics, and
combined views; see useful operational timing/progress; and copy row references without fragile dashboard-only logic.

This plan intentionally stops at planning. Implementation should happen phase by phase, with each phase owned by a
distinct agent instance.

## Current Baseline

The current checkout already includes the Epic 17 foundation and much of Epic 18:

- `cargo test -p zorg-dash` passes: 64 tests.
- Dashboard rows have stable row identity and per-panel viewport state.
- Today combines zettel rows from built-in due/do/todo queries plus diagnostics.
- Diagnostic filters apply to Diagnostics and Today diagnostic rows.
- Status events, footer key help, log overlay, no-color mode, and default-off mouse capture are already present.
- Diagnostic fix preview/apply already uses shared crates and async workers.
- `zorg-refactor` already has reusable source guards, `RefactorEdit`, `apply_edits_to_source`, `apply_refactor_plan`,
  indexed source loading, and parse-after-rewrite safety.

The main Epic 19 gap is the todo write boundary. Current dashboard `ZettelRow` values carry enough data for display and
opening in an editor, but not enough stable source-token metadata for safe lifecycle edits. `Property` already has
source spans in `zorg-core`; todo marker spans are not retained in the semantic model. `StoredProperty` and `StoredTodo`
tables contain byte spans, but the public store/query row types do not currently expose them. Epic 19 should fix that at
the shared-crate boundary before wiring dashboard keys.

## Phase Count Decision

Use six phases. The legend suggested four phases, but the safe todo planner deserves its own shared-crate phase, and the
copy/yank plus progress surfaces are independent enough to avoid coupling them to write behavior. This keeps each agent
scope narrow and gives every phase a clear validation boundary.

The phases should land in order:

1. Shared todo lifecycle planner.
2. Dashboard row/action plumbing and mark-done apply.
3. Postpone and schedule prompt/apply flows.
4. Today display modes and post-action selection behavior.
5. Operation timing, row deltas, and pending progress indicators.
6. Copy/yank actions and final docs/smoke coverage.

## Cross-Phase Design

Keep todo writes outside `app.rs`.

Preferred shared boundary: add a `todo` module to `crates/zorg-refactor`, because this crate already owns guarded source
edits, refactor plans, parse-after-rewrite validation, and indexed source loading. A new crate is not warranted unless
dependency cycles appear.

Suggested shared types, exact names can follow local style:

- `TodoActionKind`: `MarkDone`, `Postpone`, `Schedule`.
- `TodoActionTarget`: indexed zettel identity, root-relative path, absolute path, zettel source span, optional canonical
  ID, store row ID, and source guard.
- `TodoActionDate`: ISO date plus today-relative helpers.
- `TodoActionPlan`: operation name, target summary, warnings/rejections, one file plan, displayable changes.
- `TodoActionApplyOutcome`: changed path, zettel identity, action kind, changed fields, reindex summary if applied from
  dashboard.

Safety rules:

- Require an indexed zettel source span and a current source guard.
- Re-read source before planning/apply and reject guard mismatches as stale content.
- Match the selected zettel by source span first, then canonical ID/store metadata where available.
- Reject missing todo markers for mark-done, spanless markers, unsupported marker values, duplicate target properties,
  invalid dates, malformed rewritten source, and ambiguous rows.
- For `d`, change the selected marker to `[X]` and add or update exactly one `did::YYYY-MM-DD`.
- For `p`, update exactly one relevant `due::` or `do::` date. If both are present or both Today badges match, require
  an explicit choice in the dashboard prompt rather than guessing.
- For `s`, add or update exactly one `do::YYYY-MM-DD` on an inbox/open todo.
- Preserve unrelated properties, tags, title text, body text, indentation, nested child structure, and line endings.

Dashboard writes should follow the existing async worker pattern used by refresh, reindex, capture, and fix apply.
Successful todo actions should reindex/refresh and preserve context by row identity where possible.

## Phase 19A: Shared Todo Lifecycle Planner

Owner scope:

- `crates/zorg-core/src/lib.rs`
- `crates/zorg-parse/src/lib.rs`
- `crates/zorg-store/src/lib.rs` only if public span access is needed
- `crates/zorg-query/src/lib.rs` only if dashboard/query result rows need richer metadata
- `crates/zorg-refactor/src/lib.rs`
- new `crates/zorg-refactor/src/todo.rs`
- focused `zorg-refactor`, parser, store, and query tests

Build the safe todo lifecycle planner without touching `zorg-dash`.

Technical shape:

- Retain todo marker source spans in the semantic model, or expose equivalent parser metadata through a narrow helper.
- Add planner helpers for mark-done, postpone, and schedule.
- Reuse `SourceGuard`, `RefactorEdit`, `apply_edits_to_source`, and parse-after-rewrite validation.
- Include deterministic display changes such as `todo: [ ] -> [X]`, `did: - -> 2026-05-04`, and
  `do: 2026-05-02 -> 2026-05-09`.
- Add date parsing for strict `YYYY-MM-DD`; keep relative interval parsing out of the shared core unless the
  representation is trivial and testable.

Acceptance:

- Mark-done modifies only the selected zettel opening and is idempotent when `did::today` already exists.
- Postpone updates the intended `due::` or `do::` property without touching unrelated properties.
- Schedule adds or updates `do::YYYY-MM-DD` for an open/inbox todo.
- Duplicate `did`, `due`, or `do` properties fail closed with a structured reason.
- Stale guards, missing spans, missing markers, ambiguous matches, and rewritten parse failures all fail without writes.
- Nested zettel fixtures preserve child structure and indentation.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-refactor
cargo test -p zorg-parse
cargo test -p zorg-query
cargo test -p zorg-store
```

## Phase 19B: Dashboard Mark-Done Binding And Apply Flow

Owner scope:

- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/data.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- focused dashboard app/action/render tests

Wire the first write action: `d` marks the selected Today todo row done.

Technical shape:

- Add `zorg-refactor` as a dashboard dependency if not already present.
- Extend `ZettelRow` with source span bytes/end position/source order and whatever planner target metadata Phase 19A
  requires.
- Convert selected Today zettel rows into `TodoActionTarget`.
- Add a confirmation overlay for mark-done showing file, ID/title, and exact planned changes.
- Add `AsyncResult::TodoApply` or equivalent and keep writes off the event loop.
- Apply through the shared planner, reindex on success, load a fresh snapshot, sync viewports, and log exact changes.
- Non-Today panels or non-zettel rows should produce a warning status, not an empty overlay.

Acceptance:

- Pressing `d` on an actionable Today todo opens a confirmation overlay.
- Confirmed apply changes only the selected zettel and refreshes Today.
- Cancel leaves source unchanged and records a status event.
- Unsafe planner rejections show a clear log/detail message.
- The row disappears or moves after refresh according to the built-in Today queries.
- Selection lands on the nearest useful remaining row after the selected row disappears.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-refactor
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic19.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic19.sqlite3 --once --panel today
```

## Phase 19C: Postpone And Schedule Prompt Flows

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`
- focused dashboard prompt/action tests

Add `p` and `s` with compact prompted flows.

Behavior:

- `p` opens a postpone prompt for due/do rows. It should support simple relative intervals like `+1d`, `+1w`, and an
  explicit `YYYY-MM-DD`.
- `s` opens a schedule prompt for inbox/open todos and sets `do::YYYY-MM-DD`.
- If a row has both due and do fields, the prompt must make the target field explicit before applying.
- Enter applies, Esc cancels, Tab moves fields when multiple fields are visible.
- Invalid dates and unsupported intervals stay in the prompt with a visible error.

Acceptance:

- Postpone and schedule preserve unrelated source content exactly.
- Relative intervals resolve from the current date consistently in tests through an injectable clock/date source.
- Ambiguous due/do rows require explicit field selection.
- Cancel, invalid input, planner rejection, apply success, and apply failure are covered.
- Help/footer/README mention `d`, `p`, and `s` without crowding existing key help.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-refactor
```

## Phase 19D: Today Display Modes And Selection Preservation

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/README.md`
- render and app tests

Add Today toggles for todos-only, diagnostics-only, and combined mode.

Technical shape:

- Add `TodayMode`: `Combined`, `TodosOnly`, `DiagnosticsOnly`.
- Filter only Today rows; do not change the Diagnostics panel behavior.
- Use stable row IDs and existing viewport sync to preserve selection across mode changes and post-action row removal.
- Show active mode and filtered counts in the Today header/title.
- Choose compact bindings that do not conflict with the new todo actions, for example `t` cycles Today mode.

Acceptance:

- Combined mode matches existing Today behavior.
- Todos-only hides diagnostics and diagnostics-only hides zettel rows.
- Mode changes preserve selected row identity when possible and clamp safely otherwise.
- Empty states distinguish "no todos", "no diagnostics", and "no rows in combined mode".
- Narrow render tests show mode/count text without overlap.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
```

## Phase 19E: Timing, Row Deltas, And Progress Indicators

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/actions.rs`
- dashboard tests

Add operational feedback for long-running work.

Technical shape:

- Track start time for pending refresh, reindex, search, capture, fix preview/apply, and todo apply.
- Include elapsed duration in completion/failure status events.
- Report row deltas for snapshot-affecting actions: Today, Inbox, Search, Diagnostics, and Index diagnostic count where
  relevant.
- Add a render-facing pending activity model so the UI can show a spinner/progress label while an operation is running.
- Keep spinner rendering deterministic enough for `--once` and tests by deriving it from a tick counter or elapsed
  bucket supplied by app state.

Acceptance:

- Refresh, reindex, search, capture, todo apply, and fix apply log elapsed time.
- Refresh/reindex/todo/fix actions report useful row deltas after snapshot replacement.
- While an operation is pending, the dashboard shows which operation is running.
- Duplicate pending actions still fail politely with status events.
- Tests cover success, failure, stale async result ignored, and deterministic spinner/progress rendering.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 19F: Copy/Yank Actions, Docs, And Final Smoke Coverage

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/lib.rs` if terminal writing is needed for OSC 52
- `crates/zorg-dash/README.md`
- `crates/zorg-cli/tests/smoke.rs`

Add row-level copy/yank actions.

Behavior:

- `y` opens a compact yank overlay for the selected row.
- Support row ID, source link, and diagnostic message when available.
- Row ID should prefer `@canonical/id`, then a stable fallback like `store:<id>` for zettel rows, diagnostic code/path
  for diagnostics, and label for index rows.
- Source link should include a path plus line/column when available.
- Diagnostic message is available only for diagnostic rows.
- Clipboard strategy should be explicit:
  - Prefer OSC 52 when stdout is a terminal because it works for many SSH-aware terminal emulators.
  - Optionally try local platform commands such as `pbcopy`, `wl-copy`, or `xclip` when appropriate.
  - If no transport is available, show the value in a log/detail overlay and report that clipboard transport is
    unavailable.
- Do not panic if clipboard programs are missing or stdout is not a terminal.

Acceptance:

- Copying each supported value reports success with the copied kind and target summary.
- Unsupported values produce a clear status/log message.
- SSH/no-terminal/no-command cases degrade by showing the value rather than failing.
- Help/footer/README document the yank flow.
- CLI smoke coverage protects new help text and `--once` still renders Today successfully.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel today
```

## Final Epic Validation

After all six phases land:

```sh
cargo fmt --check
cargo test -p zorg-refactor
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo test --workspace
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_next.sqlite3 --once --panel today
```

Manual interactive checks:

- Select a due/do Today row, press `d`, confirm, and verify the file changes to `[X]` with `did::YYYY-MM-DD`.
- Select a due/do Today row, press `p`, enter `+1w`, confirm, and verify only the intended date changes.
- Select an inbox/open todo, press `s`, enter an explicit date, confirm, and verify `do::YYYY-MM-DD`.
- Toggle Today modes and confirm selection/scroll remain sane.
- Start refresh/reindex and verify pending plus elapsed feedback.
- Use yank actions locally and in a no-clipboard environment.

## Agent Coordination Notes

- Phases should be implemented sequentially; later phases rely on public APIs from earlier phases.
- Each phase should leave formatting and its listed tests green.
- Do not add dashboard-specific string surgery for todo edits.
- Do not widen Epic 19 into startup performance work from Epic 20 except for the operation timing/progress explicitly
  listed here.
- Avoid unrelated UI redesign. Use the current footer/status/overlay/list patterns.
- Preserve user-authored source formatting wherever the planner can avoid touching it.

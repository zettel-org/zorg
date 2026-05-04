---
create_time: 2026-05-03 23:05:09
status: wip
prompt: sdd/prompts/202605/zorg_dash_epic18_actionable_diagnostics.md
bead_id: zorg-6.2
tier: epic
legend_bead_id: zorg-6
---
# Epic 18 Implementation Plan: Actionable Diagnostics And Fix Queue

## Objective

Implement Epic 18 from `sdd/legends/202605/zorg_dash_next_improvements.md`: turn dashboard diagnostics into an explicit
repair queue. Users should be able to preview safe fixes from Diagnostics and Today diagnostic rows, understand
unavailable or unsafe fixes, apply a selected fix only after confirmation, refresh back into the dashboard, and filter
or mark diagnostic rows for later bulk workflows.

This plan assumes the current checkout already includes the Epic 17 dashboard foundation:

- `crates/zorg-dash` has stable row IDs, per-panel viewport state, `List` rendering, status events, a log overlay, key
  footer, no-color support, and mouse capture disabled by default.
- `DiagnosticRow` currently carries ID, severity, category, code, message, path, line/column, and zettel ID, but not
  byte spans or absolute paths.
- `zorg-fix` already owns `FixPlan`, `FixOp`, `FixEdit`, `CorpusView`, `plan_fixes`, and `apply_plan_to_source`.
- CLI and LSP both consume `zorg-fix`, but the CLI still owns private parse/validate/write orchestration in
  `crates/zorg-cli/src/main.rs`.
- `zorg-dash` does not currently depend on `zorg-fix`, `zorg-core`, or `zorg-parse`.

The main architectural rule for the epic: dashboard code must not duplicate fix semantics or parse CLI JSON. It should
ask shared Rust APIs for preview/apply outcomes.

## Phase Split

Use six phases. The legend's suggested four phases are directionally correct, but the shared fix boundary is large
enough that preview and apply should be separated before any dashboard UI depends on it. Filtering and multi-select can
then land after selected-row fix behavior is stable.

## Phase 18A: Shared Diagnostic Fix Preview API

Owner scope:

- `crates/zorg-fix/src/lib.rs`
- `crates/zorg-fix/src/plan.rs`
- `crates/zorg-fix/Cargo.toml` if promoted dependencies are needed
- focused `zorg-fix` unit tests

Build a dashboard-facing preview surface in `zorg-fix` without file writes.

Proposed API shape, exact names may follow local style:

- `DiagnosticFixSelector`: source path plus at least one of diagnostic code, byte span, line/column span, or rule code.
- `FixPreview`: rule code, severity, path, primary line/column, replacement preview, preferred flag, safe flag,
  explanation, and source span metadata.
- `FixPreviewSet`: selected diagnostic summary plus zero or more previews and an unavailable reason when no safe fix
  exists.
- Planning helpers that filter `FixPlan.ops` by diagnostic/source span overlap and rule mapping.

Important design points:

- Matching should prefer byte spans because `StoredDiagnostic` already stores `start_byte` and `end_byte`.
- Fallback matching by line/column is acceptable for diagnostics without bytes, but should be treated as less precise in
  the explanation.
- The mapping from diagnostic codes to fix kinds should be shared here, not duplicated from `zorg-ls/src/actions.rs`.
- It must distinguish "no matching fix", "ambiguous matching fixes", "diagnostic has no source span", "source path
  unavailable", and "known unsafe/unavailable" as explicit reasons.
- Replacement preview should be bounded and deterministic. Do not dump whole source files.

Acceptance for this phase:

- `zorg-fix` can preview at least unresolved absolute link typo fixes and source-token normalizer fixes where source
  spans match.
- Ambiguous or spanless diagnostics return structured unavailable reasons.
- Tests cover safe preview, no-op unavailable, ambiguous match, byte-span matching, line/column fallback, and preview
  truncation.
- Existing CLI and LSP fix tests continue to pass.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-fix
cargo test -p zorg-ls
cargo test -p zorg-cli --test smoke fix
```

## Phase 18B: Shared Apply API And CLI Migration

Owner scope:

- `crates/zorg-fix/src/lib.rs`
- `crates/zorg-fix/src/plan.rs` or a new focused module under `crates/zorg-fix/src/`
- `crates/zorg-cli/src/main.rs`
- focused CLI and `zorg-fix` tests

Move the reusable apply orchestration out of CLI-private helpers and into `zorg-fix`.

The shared API should support:

- Apply one selected safe op to source.
- Apply a selected set of safe ops for one file.
- Validate rewritten source through the shared fix path before returning success.
- Return unchanged source on failure.
- Return an apply summary with changed path, applied edits, applied rule codes, and failure diagnostics or error text.

File I/O can be either shared directly in `zorg-fix` or kept in callers, but the validation, operation selection, edit
application, and failure semantics must be shared. If atomic write remains caller-owned, extract enough source-level API
that CLI and dashboard both use the same safety checks.

CLI should be migrated to the shared apply path so the dashboard does not become the only consumer. Preserve existing
CLI behavior and JSON shape unless changing it is required, in which case update tests and docs deliberately.

Acceptance for this phase:

- Applying a selected op modifies only its intended span.
- Rewritten source is reparsed and strict validation failures refuse writes.
- Apply failure leaves source unchanged.
- CLI `zorg fix` behavior remains compatible with existing smoke coverage.
- Shared apply APIs are documented enough for dashboard agents to use without reading CLI internals.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-fix
cargo test -p zorg-cli --test smoke fix
```

## Phase 18C: Dashboard Diagnostic Preview Overlay

Owner scope:

- `crates/zorg-dash/Cargo.toml`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- focused dashboard render and app tests

Add selected-row preview behavior to the dashboard, still without applying writes.

Behavior:

- `f` opens a Fix Preview overlay from Diagnostics rows and Today diagnostic rows.
- Non-diagnostic rows should show a small log/status explanation instead of opening an empty overlay.
- The overlay should render selected diagnostic context, safe preview rows, replacement snippets, preferred/safe state,
  and unavailable reason.
- `Esc`, `q`, or `f` closes the overlay.
- Help and footer key text should include `f fix preview` only when concise enough; do not crowd out existing key help.

Model/data integration:

- Extend `DiagnosticRow` with `absolute_path`, `start_byte`, `end_byte`, `end_line`, and `end_column` from
  `StoredDiagnostic`.
- Add a dashboard fix preview view model separate from `zorg-fix` public structs if rendering needs a narrower shape.
- Add `zorg-fix` and any necessary parser/core dependencies to `zorg-dash`.
- Implement preview in `actions`, reading the selected diagnostic's source file and calling the shared preview API.
- Keep preview synchronous only if it is bounded and cheap; otherwise use the existing worker-result pattern. Prefer a
  worker if file parsing or corpus planning may grow expensive.

Acceptance for this phase:

- Preview opens from Diagnostics and Today diagnostic rows.
- Safe fixes show rule code, path, position, explanation, and a bounded replacement preview.
- Unavailable fixes explain why no safe fix can be applied.
- Preview cancel leaves dashboard state unchanged.
- `--once` rendering remains deterministic.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic18.sqlite3 --once --panel diagnostics
```

## Phase 18D: Dashboard Confirmed Apply, Refresh, And Failure Handling

Owner scope:

- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/ui.rs`
- dashboard tests and relevant smoke coverage

Wire confirmed selected-fix apply.

Behavior:

- `F` from a safe Fix Preview asks for confirmation before writing.
- Confirmed apply runs through the shared apply path from Phase 18B.
- Writes must be in-process and bounded; no shelling out to `zorg fix`.
- On success, refresh/reindex the dashboard snapshot through existing store APIs and preserve context by row ID where
  possible.
- On failure, leave source unchanged, close or keep the overlay according to the error type, and record a status log
  event with enough detail to debug.
- If the index is stale relative to source, prefer refusing with a clear stale-source message over applying against
  uncertain spans.

Implementation notes:

- Add an `AsyncResult::FixApply` variant rather than blocking the event loop.
- Reuse existing generation checks and status log behavior.
- Include changed path, applied rule code, and applied edit count in the success detail.
- Apply only the selected preview item in this phase. Do not bulk-apply marked diagnostics yet.

Acceptance for this phase:

- Applying a previewed safe fix changes only the intended source span/file.
- Success refreshes Diagnostics and Today counts.
- Apply success preserves the nearest useful panel context.
- Apply failure leaves source unchanged and records a log event.
- Tests cover confirm, cancel, apply success, apply failure, unsafe no-op, and post-apply refresh.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-fix
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke fix
cargo test -p zorg-cli --test smoke dash
```

## Phase 18E: Diagnostics Filters

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- dashboard app/render tests
- `crates/zorg-dash/README.md`

Add local filters for Diagnostics and Today diagnostic rows without changing stored data.

Behavior:

- Support severity, rule/code, and path filters for Diagnostics.
- Today should respect diagnostic filters for diagnostic rows only; zettel rows should remain visible unless the user is
  on the Diagnostics panel.
- Keep filter state per panel or in a diagnostic-specific model, whichever best matches current viewport ownership.
- Preserve selection by row identity when filters change.
- Show active filters in the Diagnostics title/header or inspector without overwhelming narrow terminals.

Suggested controls:

- `e` cycles severity filter: all, error, warning, info.
- `a` clears diagnostic filters.
- `:` or another existing-safe input affordance can open a compact rule/path filter prompt if a reusable input model
  exists. If not, implement minimal end-edit input for rule/path and document the limitation.

Acceptance for this phase:

- Diagnostics can be filtered by severity, rule/code, and path substring.
- Filtered row counts and empty states are clear.
- Selection and scroll offsets stay sane as filters reduce or expand rows.
- Render tests cover narrow width with active filters.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke dash
```

## Phase 18F: Multi-Select Fix Queue And Final Hardening

Owner scope:

- `crates/zorg-dash/src/model.rs`
- `crates/zorg-dash/src/app.rs`
- `crates/zorg-dash/src/ui.rs`
- `crates/zorg-dash/src/actions.rs`
- `crates/zorg-dash/README.md`
- CLI smoke tests or dashboard fixtures as needed

Add conservative multi-select state and final integration hardening.

Behavior:

- Space toggles mark/unmark for diagnostic rows in Diagnostics and Today.
- Marked state should use stable diagnostic row IDs, survive refresh when the same diagnostic remains, and be removed
  when diagnostics disappear.
- UI should show marked count and a clear mark glyph.
- Preview should be able to summarize selected diagnostic, selected file, or marked diagnostics, but first write
  behavior should remain conservative unless Phase 18D safety is already strong enough for batch.
- Bulk apply may be preview-only in this epic if selected-op apply is safer; if implemented, it must group by file,
  validate all rewrites, and refuse the whole file on any conflict.

Final hardening:

- Update README key help and diagnostic workflow docs.
- Add or update smoke coverage for at least previewable diagnostics and post-apply clean dashboard state.
- Confirm `--once` frames still do not mutate files.
- Confirm no new dashboard path shells out to CLI JSON.

Acceptance for this phase:

- Diagnostics can be marked and unmarked.
- Marked rows persist across refresh by identity where possible.
- The fix overlay can summarize marked diagnostics and explain unsupported bulk apply when not available.
- Documentation matches actual keys and safety behavior.
- The full Epic 18 validation set passes.

Validation:

```sh
cargo fmt --check
cargo test -p zorg-fix
cargo test -p zorg-dash
cargo test -p zorg-cli --test smoke fix
cargo test -p zorg-cli --test smoke dash
cargo run -q -p zorg-cli -- db reindex --root fixtures/corpus --db /tmp/zorg_dash_epic18.sqlite3
cargo run -q -p zorg-cli -- dash --root fixtures/corpus --db /tmp/zorg_dash_epic18.sqlite3 --once --panel diagnostics
```

## Cross-Phase Contracts

- `zorg-dash` must never reimplement source rewrite rules.
- `zorg-dash` must never parse CLI JSON or invoke `zorg fix` as a subprocess for preview/apply.
- Shared fix APIs should be source-span backed and deterministic.
- Any write path must either use atomic replacement or a documented equivalent already accepted in this repo.
- Unsafe, ambiguous, spanless, missing-source, and stale-source cases are product states, not generic errors.
- Every phase should leave the workspace in a buildable state for its owned crates.

## Risks And Mitigations

- Risk: diagnostic rows lack enough identity to match fix ops reliably. Mitigation: Phase 18C extends `DiagnosticRow`
  with byte spans and absolute path; Phase 18A prioritizes byte-span selectors.

- Risk: source-token fix ops do not correspond to indexed diagnostics. Mitigation: preview API supports both
  diagnostic-code matching and span/rule matching, and unavailable reasons should clearly distinguish "no indexed
  diagnostic owns this fix".

- Risk: moving CLI apply semantics into `zorg-fix` changes CLI behavior. Mitigation: Phase 18B keeps CLI smoke tests as
  mandatory validation and preserves JSON shape unless tests are deliberately updated.

- Risk: bulk apply can produce overlapping edits across marked diagnostics. Mitigation: Phase 18F may keep bulk apply
  preview-only. If bulk apply is implemented, group by file and reject conflicts before writing.

- Risk: dashboard apply uses stale spans after source changes outside the dashboard. Mitigation: compare source metadata
  or source text expectations before writing, and refresh/refuse on mismatch.

## Recommended Agent Ordering

1. 18A must land first.
2. 18B depends on 18A and should land before dashboard apply.
3. 18C depends on 18A but can start before 18B if it remains preview-only.
4. 18D depends on 18B and 18C.
5. 18E can start after 18C model changes, but should avoid conflicting with 18D overlay/apply fields.
6. 18F should be last because it touches marked state, docs, and integration behavior across the previous phases.

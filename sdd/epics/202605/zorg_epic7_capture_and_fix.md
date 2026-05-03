---
create_time: 2026-05-02 20:15:55
status: wip
prompt: sdd/prompts/202605/zorg_epic7_capture_and_fix.md
---
# Zorg Epic 7: Capture and Fix Implementation Plan

## Goal

Implement Epic 7 from `sdd/legends/202605/zorg_v1_mvp.md`: close the daily write loop for Zorg v1 by giving users a
working `zorg check`, `zorg fix`, and `zorg capture` toolchain that is idempotent, span-backed, and shared cleanly
between the CLI and the LSP code-action surface.

The finished epic should let a user:

- Run `zorg check FILE...` (already partial today) and `zorg check --root ~/zorg` and have it diagnose every strict
  condition listed in `docs/fix.md`.
- Run `zorg fix --check FILE...` and see every pending autofix without writing.
- Run `zorg fix FILE...` (or `--root`) and have safe autofixes applied idempotently, with the same edit semantics that
  the LSP exposes through code actions.
- Run `zorg capture --template @system/templates/todo --title "Plan" --source ./inbox.md` and produce a new zettel under
  the configured root from a `#z/tmpl` template.
- Trigger an editor-friendly capture path that prints created file path / zettel ID for Neovim and other clients.

## Current Context

Relevant crate state in this Rust workspace:

- `crates/zorg-fix`: thin stub. Owns `check_strict` (returns `Unsupported`) and a single shipped helper,
  `suggest_absolute_link_typo_fix`, currently consumed only by the LSP code-action layer.
- `crates/zorg-capture`: stub with a `CaptureRequest { title }` struct and a `capture` function returning `Unsupported`.
- `crates/zorg-cli`: implements `parse`, `check`, `db status`, `db reindex`, and `query`. `zorg fix` and `zorg capture`
  are placeholder error paths (`crates/zorg-cli/src/main.rs:44`).
- `crates/zorg-ls`: ships diagnostics, navigation, completion, rename, and one code action that wraps
  `zorg_fix::suggest_absolute_link_typo_fix` (`crates/zorg-ls/src/actions.rs`).
- `crates/zorg-parse`: provides `parse_zettel_document_with_path`, `validate_document`, `validate_corpus`,
  `resolve_document`, `resolve_corpus`, and a typed `ZettelDocument` model with source-spans, properties, type tags,
  todos, and `BodyBlock::FencedCode { info }` for fenced blocks (the source of `zorg-template` and `swog` blocks).
- `crates/zorg-store`: indexes corpora, materialises canonical IDs, type tags, properties, links, todos, and exposes
  rich read APIs that downstream features (LSP, query, capture template discovery) can build on.
- `docs/fix.md`: ratified contract for strict checks and autofix surface area, including idempotency requirement and
  legacy-input policy. Notes that modified-date stamping and SORT-pragma sorting may not be enabled until "explicitly
  specified by a later implementation."
- `docs/capture.md`: ratified contract for `#z/tmpl` discovery, content extraction (`zorg-template` fence or body),
  template metadata (`title::`, `dest::`, `tags::`, `source::`), reserved variables (`{{id}}`, `{{title}}`, `{{date}}`,
  `{{source}}`), destination policy (no silent overwrite, must stay under root unless explicitly approved), and
  noninteractive flag expectations.

Important constraints surfaced before planning:

- `docs/fix.md` punts modified-date stamping and SORT-pragma behavior to a "later implementation"; that later
  implementation is part of this epic, but lives in its own phase that begins with a small spec PR before code.
- `docs/capture.md` says template variables are "implementation-defined in later capture work" — capture phases must
  land both spec and behavior together.
- The strict check and link-typo code action are the only fix functionality consumed by the LSP today. Any shared data
  structures introduced by Epic 7 must continue to expose at least the typo-rewrite path to `zorg-ls` without a flag
  day.
- The CLI is not async and does not depend on `tokio`. Capture and fix should not pull async runtimes into the CLI
  binary unless absolutely required.
- Tests must continue to use the existing `tests/smoke.rs` style in `zorg-cli` (process invocation against
  `CARGO_BIN_EXE_zorg`) and a temporary workspace harness.

## Non-Negotiables

- `.z` only. `zorg fix` and `zorg capture` must never read or write `.zo`, `.zoq`, `.zot`, or `.zoc`.
- No legacy migration. Legacy syntax is diagnosed strictly; it is not transformed.
- Default root is `~/zorg`. Both fix and capture must respect `--root`/`--db` flag overrides identical to existing CLI
  commands and use `StoreOptions` for path resolution.
- Autofixes must be deterministic, source-span-backed, and idempotent. A second run must be a no-op for any fixture
  exercised in the first run.
- `zorg fix` writes only when invoked without `--check` (or with an explicit `--write` flag chosen by the agent), never
  overwrites unchanged files, and never silently changes nesting or semantics.
- `zorg capture` never silently overwrites an existing file. It either appends a child zettel to a directory/file
  destination or fails with a clear error.
- Capture destinations must stay under the configured Zorg root unless an explicit override flag is supplied.
- Fix and capture operations must produce useful errors with file path, line, and column when they cannot proceed.
- LSP and CLI must share a single fix-edit data model. New autofixes added to the CLI become available as code-action
  candidates without re-implementation in `zorg-ls`.
- Capture must use the existing parser/model. It must not maintain a parallel template grammar.

## Proposed Phase Split

Use **five sequential phases**. Each phase is intended for a distinct agent instance and must leave the workspace in a
passing state (`cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`). The split
separates a strict-check/fix-plan foundation from progressively more aggressive autofix work, then adds capture in two
slices (noninteractive engine, then interactive/editor surface). Phases 7.4 and 7.5 may be pulled forward and worked in
parallel with 7.2/7.3 if scheduling demands, but the default is sequential.

### Phase 7.1: Strict Check Pipeline and Shared Fix Plan Model

Purpose: lift `zorg check` to the full strict diagnostic surface promised by `docs/fix.md`, and introduce the shared
fix-plan abstraction that the rest of the epic plus the LSP will consume.

Primary ownership:

- `crates/zorg-fix/src/`
- `crates/zorg-fix/tests/`
- `crates/zorg-cli/src/main.rs` (extending `check`, adding `fix --check`)
- `crates/zorg-cli/tests/smoke.rs`
- `crates/zorg-ls/src/actions.rs` (adapting to shared types only)
- `docs/fix.md` (clarifications only; new specs are deferred to Phase 7.3)

Scope:

- Audit the diagnostics emitted by `zorg-parse` (`validate_document`, `validate_corpus`, `resolve_document`,
  `resolve_corpus`) plus the query/template definition errors from `zorg-query` and confirm coverage of every strict
  category listed in `docs/fix.md`: malformed file headers, duplicate canonical IDs, unresolved
  absolute/child/sibling/local references, invalid property syntax and known typed values, unsupported legacy-looking
  syntax, query/template definition errors. Add any missing pass-throughs in `zorg-fix` so callers do not need to know
  which crate owns each diagnostic.
- Introduce a `FixPlan` / `FixOp` / `FixEdit` model in `zorg-fix` with:
  - source URI/path, source span, replacement text, autofix kind, severity, "is preferred" flag, and a stable rule code
  - a `FixKind` enum that starts with the existing `UnresolvedAbsoluteLinkTypo` rule and reserves variants for the
    autofixes added in 7.2 and 7.3
  - documented invariants (idempotent, span-backed, deterministic ordering, never crosses zettel boundaries)
- Provide a `plan_fixes(document, corpus_view)` entry point that returns a deterministic `Vec<FixPlan>` for a parsed
  document. In 7.1 the planner only emits the existing typo-rewrite rule, but the shape is in place for later phases to
  add rules without touching call sites.
- Extend `zorg check FILE...` to optionally accept `--root PATH` and run corpus-level validation (today it parses each
  file in isolation but already reads `validate_corpus`/`resolve_corpus`; ensure ergonomics line up).
- Wire `zorg fix --check FILE...` (and `--root`) to:
  - exit zero only when the file has no strict diagnostics and no pending autofixes
  - exit nonzero with a stable, parseable diagnostic line for any strict diagnostic or pending fix
  - print one line per pending fix that includes file:line:column, rule code, and a one-line description
- Adapt `crates/zorg-ls/src/actions.rs` to call `zorg_fix::plan_fixes` (or equivalent) instead of the current direct
  call to `suggest_absolute_link_typo_fix`. Behavior of the existing code action must not change. The legacy helper can
  remain or be made `pub(crate)` behind the new entry point.
- Add unit and integration tests in `zorg-fix` for `FixPlan` ordering and idempotency-of-planning (running the planner
  twice on the same document yields equal output), and CLI smoke tests for `zorg check --root` and
  `zorg fix --check FILE`.

Acceptance:

- `zorg check FILE...` and `zorg check --root PATH` exit nonzero on every diagnostic class listed in `docs/fix.md`.
- `zorg fix --check FILE` exits nonzero exactly when there is at least one pending fix or strict error.
- `zorg-ls` code actions still rewrite unresolved absolute link typos with no behavioral regression.
- `zorg-fix` exposes a public `FixPlan`/`FixOp`/`FixKind` API used by both the CLI and the LSP.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.

Handoff:

- Later phases add new `FixKind` variants and rule implementations behind `plan_fixes`, then `apply` them.
- `zorg-ls` automatically picks up new autofixes as quickfix code actions in 7.2 and 7.3 without further wiring.

### Phase 7.2: Safe In-Place Autofixes (Bullet Symbols, Property Whitespace, Link-Typo Apply)

Purpose: ship the conservative autofix set that does not require spec extension, plus the CLI write path for any
existing fix rule.

Primary ownership:

- `crates/zorg-fix/src/`
- `crates/zorg-fix/tests/`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- new fixture files under `fixtures/corpus/` for autofix golden tests
- targeted additions to `crates/zorg-parse` only if a span helper is required

Scope:

- Implement bullet-symbol normalization as a `FixKind::BulletSymbol` rule:
  - normalize to a single canonical bullet glyph (default `-` per existing fixtures) only when nesting depth and
    surrounding indentation are unambiguously preserved; never collapse mixed nesting
  - source-span-backed edits per bullet token; one-line edits only
- Implement property whitespace normalization as `FixKind::PropertyWhitespace`:
  - collapse extra whitespace immediately surrounding the `::` separator and trim trailing whitespace on the property
    line value, while preserving slash-separated list values byte-identically
  - never alter the key, value characters, or property ordering
- Promote the existing typo-rewrite rule into an `apply` path so `zorg fix FILE` (without `--check`) writes the same
  edit that the LSP code action already produces.
- Implement the `zorg fix` write path:
  - flags: `--check`, `--root`, plus an explicit confirmation flag if the agent prefers (`--write`/`--apply`); design
    must be unambiguous about when files change
  - writes are atomic per file: stage edits in memory, format-check the result by re-parsing, then write only if no
    strict regressions appear
  - skip files with no edits and never bump mtimes for unchanged files
- Add idempotency tests:
  - run `zorg fix` twice on a malformed fixture and assert byte-equality between runs after the first
  - run `zorg fix --check` against the post-fix output and assert exit zero
- Add golden fixtures for each rule under `fixtures/corpus/` with both an "unfixed" and a "fixed" variant.

Acceptance:

- `zorg fix FILE` rewrites bullet glyphs, normalizes `key::value` whitespace, and applies link-typo rewrites in place.
- A second `zorg fix` invocation produces no further changes for every fixture exercised in tests.
- `zorg fix --check` exits nonzero before fixes and zero after, on the same fixture.
- LSP code actions advertise the new `FixKind` variants automatically (test `zorg-ls` continues to publish at least the
  typo quickfix and now the bullet/property quickfixes when their rule is triggered by an opened `.z` document).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.

Handoff:

- The `apply` helper in `zorg-fix` is the single source of truth for in-place writes and remains stable for 7.3.
- 7.3 adds new `FixKind` variants behind the same `plan_fixes`/`apply` interface.

### Phase 7.3: Spec-Backed Stamping Autofixes (Modified-Date, ID Stamping, SORT Pragma)

Purpose: add the autofix rules that `docs/fix.md` deferred until "explicitly specified by a later implementation."

Primary ownership:

- `docs/fix.md` (spec extension)
- `docs/syntax.md` (where the SORT pragma and stamping property keys are defined)
- `crates/zorg-fix/src/`
- `crates/zorg-fix/tests/`
- `crates/zorg-parse/src/` (only if the parser must learn the SORT pragma token)
- `crates/zorg-cli/src/main.rs` and `crates/zorg-cli/tests/smoke.rs`

Scope:

- Spec PR, landed in the same phase before code:
  - choose and document the modified-date metadata key (e.g. `modified::YYYY-MM-DD`) and the ID-stamp rule for unstamped
    zettel that already have a stable canonical name
  - choose and document the SORT pragma syntax (a single-line directive that delimits a sortable region inside a zettel
    body) and the deterministic ordering it enforces
  - document idempotency requirements and edge cases (already-stamped zettel, already-sorted regions, mixed nested
    bullets that must not be reflowed)
- Implement the parser- or formatter-level recognition of the SORT pragma, preferring formatter-level handling unless
  the pragma must influence semantic spans.
- Implement three new `FixKind` rules:
  - `FixKind::IdStamp`: stamp a missing `@id` on a zettel that has an unambiguous canonical-name source per the spec
  - `FixKind::ModifiedStamp`: maintain the documented modified-date property when source content changes
  - `FixKind::SortPragmaRegion`: deterministically sort the lines or bullets within a SORT-pragma-delimited region
- Honor the legend's "auto-priority" suggestion only if it can be specified safely; otherwise document deferral in
  `docs/fix.md` with an explicit reason and skip implementation.
- Update `docs/fix.md` to remove the "deferred until specified" caveats for the rules that ship and to keep them for any
  rule that does not.
- Tests:
  - golden fixtures for each new rule, including deliberately-tricky idempotency cases
  - LSP integration test verifying the new fixes appear as code actions when triggered by relevant diagnostics or
    pragmas
  - CLI smoke test verifying multiple-rule coexistence on the same file

Acceptance:

- `zorg fix` performs documented modified-date stamping, ID stamping, and SORT-pragma sorting where applicable, and is
  idempotent across a second run for every fixture.
- `docs/fix.md` and `docs/syntax.md` describe each rule and any deliberately-deferred rule with a reason.
- Strict check mode diagnoses malformed SORT pragmas and malformed stamping properties.
- LSP exposes the new fixes as code actions when the underlying diagnostic or pragma is present.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.

Handoff:

- The fix surface is now feature-complete for the MVP and stable for capture work in 7.4/7.5.
- Capture in later phases can rely on the fix planner to clean up generated zettel before write or after capture.

### Phase 7.4: Capture Engine and Noninteractive CLI

Purpose: stand up the capture write path so editors and scripts can drive `zorg capture` against `#z/tmpl` zettel.

Primary ownership:

- `crates/zorg-capture/src/`
- `crates/zorg-capture/tests/`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `docs/capture.md` (variable spec lock-in for v1)
- `fixtures/corpus/` (template fixtures)

Scope:

- Lock the v1 template variable set in `docs/capture.md`: at minimum `{{id}}`, `{{title}}`, `{{date}}`, `{{source}}`,
  plus any `{{body}}` semantics required for the existing fixture template (`fixtures/corpus/query_and_template.z`).
  Document how each variable is sourced (CLI flag, generated, or template property fallback) and what happens when a
  variable is missing.
- Implement template discovery in `zorg-capture` using the existing store APIs:
  - look up zettel by canonical ID or by `title::` when ID is not provided
  - require the zettel to carry the `#z/tmpl` type tag
  - fail with a clear error if the template ID is missing, ambiguous, or not tagged
- Implement template content extraction:
  - prefer a `BodyBlock::FencedCode` whose `info` is `zorg-template`
  - fall back to the textual body of the template zettel when no template fence is present
  - reject templates with multiple `zorg-template` fences (mirroring how query zettel reject multiple `swog` fences)
- Implement variable expansion:
  - case-sensitive `{{name}}` substitution
  - escape rules for `{{` and `}}` in literal template text
  - deterministic, locale-independent date/time using the same `QueryDate` / `now_unix_ms` helpers the CLI already uses
    for `zorg query`
- Implement destination resolution:
  - read `dest::` from the template; if not provided, require a CLI `--dest` argument
  - resolve relative to the configured corpus root; reject paths outside the root unless an explicit `--allow-outside`
    flag is provided
  - if the destination is a directory, write to `init.z` (creating the directory if needed)
  - if the destination is a `.z` file:
    - create the file if it does not exist
    - if it exists and the template designates "append child," append a new nested zettel below the file zettel
    - never silently overwrite; fail with a clear error otherwise
- Implement the write path:
  - format the rendered template with `zorg-fix` before writing (catches malformed bullets, property whitespace, link
    typos in user-supplied values)
  - write atomically (temp file + rename)
  - run `zorg-parse` on the post-write file to confirm validity; if validation fails, leave the corpus untouched and
    report the diagnostic
- Wire `zorg capture` CLI noninteractive flags:
  - `--template @id` or `--template <title>` (required in noninteractive mode)
  - `--title TEXT`, `--source TEXT`, `--dest PATH`, `--id @new-id` (optional overrides)
  - `--root PATH`, `--db PATH` (mirroring other commands)
  - `--allow-outside` (gates writes outside the root)
  - on success, print the absolute destination path and the canonical zettel ID created (one per line) to stdout for
    editor consumers
- Tests:
  - capture into a brand-new file under a temp root
  - capture appended as a child zettel under an existing directory zettel
  - capture refusing overwrite of conflicting content
  - capture rejecting a template missing the `#z/tmpl` tag
  - capture rejecting a destination outside the root without `--allow-outside`
  - capture using both fenced-template and body-template content paths

Acceptance:

- `zorg capture --template ...` produces a new or appended `.z` zettel under the configured root.
- All capture failure modes produce non-zero exits with stable, grep-able error messages.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.

Handoff:

- Phase 7.5 adds an interactive shell and a JSON output mode atop the same engine.

### Phase 7.5: Interactive Capture, Editor Output Modes, and Cross-Surface Polish

Purpose: make capture pleasant on the human side and scriptable for editor integrations, then close the epic with a
documentation and cross-surface validation pass.

Primary ownership:

- `crates/zorg-capture/src/`
- `crates/zorg-cli/src/main.rs`
- `crates/zorg-cli/tests/smoke.rs`
- `crates/zorg-ls/src/` (capture-related code action / command, if it fits the LSP MVP without scope creep)
- `docs/capture.md`, `docs/fix.md`, `docs/lsp.md`
- `fixtures/corpus/` for end-to-end fixtures

Scope:

- Add a minimal interactive mode for `zorg capture`:
  - if no `--template` flag is provided and stdout/stdin are a TTY, prompt for a template selection from the discovered
    `#z/tmpl` zettel sorted deterministically by ID
  - if the template defines required variables that are not provided via flags, prompt one at a time; abort cleanly on
    EOF/Ctrl-C
  - never prompt in non-TTY mode; instead fail with a clear error listing the missing inputs
- Add a `--json` (or `--format json`) output mode for both `zorg fix` and `zorg capture`:
  - capture: `{ "destination": "...", "zettel_id": "@..." }` on success; `{ "error": "...", "code": "..." }` on failure
  - fix: stable, schema-versioned per-file edit summary
  - flag is opt-in; default text output remains stable
- Optionally add an LSP `workspace/executeCommand` `zorg.capture` command that delegates to the same engine. Scope this
  conservatively: if it cannot be done in less than a day of agent work without rewriting the LSP request plumbing,
  document it as deferred and stop.
- Documentation pass:
  - `docs/capture.md`: capture flow diagrams, exhaustive flag list, interactive-vs-noninteractive matrix
  - `docs/fix.md`: full rule list, idempotency policy, JSON output schema
  - `docs/lsp.md`: cross-link to capture and fix surfaces; document the new code actions added by 7.2/7.3 and any LSP
    command added in this phase
- End-to-end test: a single integration test that, against a temporary `~/zorg`-like root, runs `zorg db reindex` →
  `zorg capture` (creates a todo from a `#z/tmpl`) → `zorg db reindex` → `zorg query #z/todo` → `zorg fix` →
  `zorg fix --check`, asserting each step is healthy.
- Sweep `crates/zorg-cli/src/main.rs` for stale TODO/placeholder messages now that fix and capture are real commands;
  update `print_help` text accordingly.

Acceptance:

- `zorg capture` works interactively in a TTY and noninteractively in scripts.
- `--json` output is consumable by editor integrations and is covered by tests.
- The end-to-end test passes from a clean temp workspace.
- `docs/capture.md`, `docs/fix.md`, and `docs/lsp.md` describe the final state of Epic 7.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace` pass.

Handoff:

- Epic 7 is closed. Neovim integration in Epic 8 can call `zorg capture --json` and `zorg fix --json` without further
  capture/fix work.

## Cross-Phase Interfaces

- `zorg-fix::FixPlan` / `FixOp` / `FixKind` is the public contract between CLI and LSP for all autofixes.
- `zorg-fix::plan_fixes(document, corpus_view)` and `zorg-fix::apply(plan, source) -> NewSource` are the only entry
  points downstream surfaces should call.
- `zorg-capture::Engine` (or equivalent) owns template discovery, variable expansion, destination resolution, and write.
  CLI and LSP both go through it; neither re-implements template parsing.
- Capture re-uses `zorg-fix::apply` to format generated zettel before write, ensuring captured zettel land already
  passing `zorg fix --check`.
- Strict check uses the same diagnostic types `zorg-parse` and `zorg-query` already emit; no new diagnostic enum is
  introduced beyond what those crates own today.

## Risks

- **Bullet/whitespace fixes silently changing nesting**: must be guarded by re-parsing the post-fix source and comparing
  zettel hierarchy before/after. Phase 7.2 must include this guard.
- **Stamping rules that are not actually idempotent**: especially modified-date stamping if it churns mtimes on every
  run. Phase 7.3 must define a "stamp only when content hash differs from the last stamp" semantics.
- **Capture overwriting user content**: explicit refusal-by-default and tested overwrite-protection paths must land in
  7.4 before any interactive UX in 7.5.
- **Async creep in the CLI**: avoid pulling `tokio` into `zorg-cli` because of capture or fix needs. Both capture and
  fix should be synchronous in the CLI binary.
- **LSP/CLI drift on fix semantics**: enforced by 7.1 introducing a single `plan_fixes` entry point and 7.2/7.3 only
  adding rules behind it.
- **Capture template grammar drift**: capture must use `zorg-parse` `ZettelDocument` and `BodyBlock::FencedCode` — never
  a parallel template tokenizer.

## Definition of Done

- `zorg check` and `zorg fix --check` diagnose every strict condition in `docs/fix.md`.
- `zorg fix` applies bullet, property-whitespace, link-typo, ID-stamp, modified-date, and SORT-pragma rules idempotently
  and atomically.
- `zorg capture` writes new zettel from `#z/tmpl` templates with both fenced and body content paths, supports scripted
  and interactive flows, and never overwrites silently.
- `zorg-ls` exposes every applicable autofix as a quickfix code action through the shared `zorg-fix` planner.
- `docs/fix.md` and `docs/capture.md` describe the final v1 contract; `docs/lsp.md` cross-links the new code actions.
- A single end-to-end integration test verifies the parse → reindex → capture → reindex → query → fix → check loop on a
  temporary corpus root.

---
research_date: 2026-05-04
title: Pomodoro sections in legacy `*_day.zo` files
status: draft
source_context:
  - ~/org/2026/2026*_day.zo (112 files in 2026; sampled 8 across Jan–Apr)
  - ~/org/2024/20241112_day.zo (legacy header variant)
  - ~/org/2026/20260101_poms.zo (companion poms file)
  - sdd/research/202605/new_zorg_daily_today_transition.md
  - sdd/research/202605/legacy_zo_no_port_triage.md
recommendation: keep the day-file pomodoro section as a first-class grammar feature in the new daily/today loop, but model it as "planned blocks" only — push completed blocks (start/end/@X) to a separate stream so the day file stays a thin plan, mirroring the legacy `*_poms.zo` companion split
---

# Pomodoro Sections In Legacy `*_day.zo` Files

## Why This Note Exists

The new-zorg daily/today transition research (`new_zorg_daily_today_transition.md`) and the legacy port-triage
(`legacy_zo_no_port_triage.md`) both refer to "pomodoro sections" in legacy day files but neither documents the exact
on-disk shape. This note pins down the grammar, semantics, and observed dialects of the pomodoro section that lives
inside `*_day.zo`, so the v1 grammar/parser and the daily/today UX can be designed against real data rather than
secondhand description.

Source of truth is `~/org/202[3-6]/*_day.zo`. There is no `*_day.zo` file inside the `zorg_100` checkout — these are
pure user content under `~/org/`.

## Where The Section Sits In A Day File

Every `*_day.zo` file follows the same skeleton:

1. Header comment block: `day::YYYY-MM-DD Day`, navigation links (`^`, `<`, `>`, `@`), and "now" shortcuts (`# D = …`).
2. Free-form lines for plan items (open todos `o P0 …`, recurring `~ P0 …`, links to projects/bugs).
3. Optional `################################ @WAIT` section.
4. Optional `################################ @EVENT` section.
5. **The pomodoro section**, always last, opened by a `################################` divider.
6. EOF (no trailing trailer).

The pomodoro divider has two observed shapes:

- **Modern (2026)**: `################################ [[YYYY/YYYYMMDD_poms]]`
  Example: `################################ [[2026/20260427_poms]]`. The link points to the companion `*_poms.zo`
  file, which holds the full historical record split into `PLANNED` / `DONE` subsections.
- **Legacy (2024 and earlier)**: `################################ [[pomodoro]] NOTES`, sometimes with a time-window
  suffix (`[[pomodoro]] NOTES (0900-1800)`) or `[[pomodoro]] TODAY`. No companion `*_poms.zo` file existed at the time;
  the day file was the only record.

The shift to the linked-companion form is the model the new system should adopt — the day file becomes the plan, the
poms file is the ledger.

## Pomodoro Block Grammar

A pomodoro section contains zero or more **blocks**. Each block is a header line followed by one or more task lines.

### Block header

```
======================== p::<minutes>[/<cumulative>] [start::HHMM end::HHMM] [@X]
```

- `========================` (24 `=`) is the block delimiter.
- `p::N` — duration in minutes for this block. Required. `N` is typically 5, occasionally 2/3/6/7/8/10/13/20.
- `/<cumulative>` — optional running daily total. Present once `start::` is filled in (the user maintains it manually
  as they log completed blocks). E.g. `p::5/91` = "this 5-min block; total minutes today now 91".
- `start::HHMM end::HHMM` — wall-clock start/end, 24-hour, no separator. Both present together or both absent.
  Absent ⇒ this is a planned block that has not yet been executed.
- `@X` — block-completion marker. Present iff the block actually happened. Usually co-occurs with `start::`/`end::`.

Distribution of variants observed:
- `p::N` only ⇒ planned future block.
- `p::N/total start::HHMM end::HHMM @X` ⇒ completed block with full provenance.
- `p::N/total` (no times, no `@X`) ⇒ in-progress / partially-logged block (rare).

### Block body

After the header, one or more lines, each a single todo reference:

```
- 260427#0J +pa_trouble pat_fix_miss_sponsor @CL!
  * [X] Created http://b/507137691
  * [X] Run agents to fix bug!
```

Body line shape:

- `- ` prefix (literal "- ").
- Optional carryover date prefix (e.g. `- 260427 260424#0E …`) — when an earlier-day todo is being executed today.
- `<YYMMDD>#<id>` — the bead/todo id this work is logged against. The `#` is always present; `<id>` is base-N
  (digits + letters), case-sensitive (`0J`, `0t`, `17`, `1G`).
- Free-form description, identical to how the same todo line appears elsewhere in the day file. Tags include `#tag`,
  area refs `+area`, project refs `[#anchor]`, wiki links `[[path]]`, and trailing `!` for emphasis.
- Optional ID-link suffix `| [<other_id>]` — back-reference to the original todo (typically the parent in `@EVENT`
  or in a previous day's plan). Sometimes there are multiple `| […]` chains.
- Optional trailing markers: `@WIP` (still in progress at end of block), `@CL` (changelist work), `@PAGE (1259)`
  (book/page bookmark — only seen in 2024 reading sessions).
- Optional indented `* [X] …` / `* [ ] …` sub-checklist describing what was actually accomplished in that block.

A block with multiple body lines means the same pomodoro covered several todos; this is common when batching small
GTD tasks into one 5-minute block (`#gtd at @HOME`, `#gtd today`, `#gtd Read emails`, …).

### Concrete examples

Planned-only block (no times):
```
======================== p::5/25
- 260425#07 #gtd today | [260425#01]
- 260425#08 #gtd Read emails | [260425#02]
- 260425#09 #gtd yesterday | [260425#03]
```

Completed block with full provenance:
```
======================== p::6/58 start::1835 end::1905 @X
- 260304#0V +pa_trouble +yserve
```

Mixed completed + planned in same day (from `20260427_day.zo`):
```
======================== p::10/62 start::1740 end::1830 @X
- 260427#0J +pa_trouble pat_fix_miss_sponsor @CL!
  * [X] Created http://b/507137691
  * [X] Run agents to fix bug!

======================== p::5/67
- 260427#0G +pa_trouble pat_fix_miss_sponsor @CL!
  * [ ] Review pat_fix_miss_sponsor CLs!
```

## Frequency And "Liveness" Of The Section

Across 112 `*_day.zo` files in 2026:

- ~74% (83 files) have only the empty pomodoro divider — the section is reserved but no blocks were logged in the
  day file itself. The blocks for those days live in `*_poms.zo` instead.
- ~26% (29 files) carry inline blocks. A handful of days (Jan 22, Mar 4) have 4–5 inline blocks; most have 1–2.
- Total inline blocks observed in 2026: 45. The companion `*_poms.zo` files hold the bulk of the historical record —
  e.g. `20260101_poms.zo` alone has 13 completed blocks.

Read: the day file is the *plan and live working surface*; the poms file is the *consolidated ledger*. The user
appears to drift between recording in either place depending on whether they are still actively planning the day or
already past the day's halfway point.

## Implications For The New Zorg Daily/Today Loop

These follow from the data above; they are recommendations, not facts:

1. **Treat the pomodoro section as a structural part of the day file's grammar, not free text.** The block header
   `======================== p::… [start::… end::… @X]` is regular enough to parse with a small DSL. The
   transition research already lists `^poms #z/ref title::Pomodoros` as a daily-template anchor — that anchor should
   resolve to a typed block list, not a generic ref.

2. **Keep the "planned vs. completed" split first-class.** The presence of `start::`/`end::`/`@X` is the single
   discriminator. The dashboard's Today panel can render planned blocks as an upcoming queue and completed blocks as
   a timeline, exactly mirroring the `PLANNED` / `DONE` headers used inside `*_poms.zo`.

3. **Promote `p::`, `start::`, `end::` to indexed properties.** `v1_mvp_curation.md:120-121` already calls these out
   as keep-in-grammar. The block header should be parsed into `(duration_min, cumulative_min, start, end, completed)`
   and stored on a `Pomodoro` node that points at the bead-id list in its body. This makes it possible to query
   "what did I work on between 17:00 and 18:30 on 2026-04-27?" without grepping.

4. **Body lines reuse existing todo grammar.** The `- <YYMMDD>#<id> …` shape inside a pomodoro block is identical to
   a normal todo line plus an outer `Pomodoro` container — no new tag/area/link grammar is needed inside blocks.
   The parser can reuse the todo-line production; only the `Pomodoro` wrapper is new.

5. **Companion `*_poms.zo` file is optional, not required.** The section in the day file is sufficient to log a day.
   The companion file is a denormalized rollup the user maintains by hand. The new system can either generate it from
   indexed data (preferred — eliminates dual-write drift) or drop it entirely and serve the same view from a query.

6. **Carryover prefix needs a parser rule.** The `- 260427 260424#0E …` shape (current-day date, then earlier-day
   id) is a deliberate way to mark that a planned item is being executed today. Without an explicit rule, this looks
   like a malformed id. Suggest: leading bare `YYMMDD ` token before `<YYMMDD>#<id>` is the carryover-date and goes
   on the `Pomodoro` block body, not the bead.

7. **`@WIP` / `@X` / `@CL` are status sigils, not tags.** They appear at the *end* of body lines or block headers
   and modify the immediately-preceding entity. Treat them as a closed enum: `WIP` (in-progress), `X` (completed),
   `CL` (changelist boundary). New tags should not be invented in this position.

## Open Questions

- Does the user want the v1 grammar to accept the legacy `[[pomodoro]] NOTES` divider as an alias, or only the modern
  `[[YYYY/YYYYMMDD_poms]]` link form? (Legacy form is unused in 2026 day files.)
- Cumulative minutes (`p::5/91`) are maintained manually today. Should the new editor auto-recompute, or is the
  hand-maintained number a deliberate "I last paid attention here" marker?
- The `* [X]` sub-checklist under a body line is freeform today. Should it be promoted to a structured outcome
  list, or left as plain prose? (Argument for prose: the user uses it inconsistently — some blocks have it, most
  don't.)

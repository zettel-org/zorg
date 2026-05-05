---
research_date: 2026-05-04
title: Pomodoro sections in legacy `*_day.zo` files
status: draft
source_context:
  - ~/org/2024/2024*.zo and 2024*_day.zo (pre- and post-2024-03-12 transition)
  - ~/org/2025/2025*_day.zo (full year) and ~/org/2025/2025*_poms.zo (from 2025-10-19)
  - ~/org/2026/2026*_day.zo (112 files in 2026; sampled across Jan–Apr)
  - ~/org/2025/20251019_poms.zo (first-ever companion poms file)
  - ~/org/2026/20260101_poms.zo (modern PLANNED/DONE form)
  - ~/org/zot/poms_log.zot (aggregate log referenced from each poms file)
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

The pomodoro divider has three observed shapes, in chronological order:

- **Earliest (2024-Jan → 2024-Mar-11)**: `################################ Pomodoros`, inside the plain `*.zo` day
  file. No `_day.zo` files existed yet. Body lines used a simpler shape (`- 240115#05 1` — bead-id, count, optional
  start time and `+10` duration adjustment, tags) and there were no `========================` block headers.
- **Mid-legacy (2024-03-12 → 2025-10-18)**: `################################ [[pomodoro]] NOTES`, sometimes with a
  time-window suffix (`[[pomodoro]] NOTES (0900-1800)`) or `[[pomodoro]] TODAY`. The `*_day.zo` file is now the host
  (the `_day.zo` split landed on 2024-03-12). The `========================` block header with `p::N`/`start::`/`end::`
  /`@X` grammar described below is in use. No companion `*_poms.zo` file existed; the day file was the only record.
- **Modern (2025-10-19 → present)**: `################################ [[YYYY/YYYYMMDD_poms]]`. Example:
  `################################ [[2026/20260427_poms]]`. The link points to a companion `*_poms.zo` file. The
  first-ever `_poms.zo` is `~/org/2025/20251019_poms.zo`; the cutover note is recorded inside that file as
  `- 251019#0w #gtd Created *_poms.zo file template!`. From 2026 onward the poms file is internally split into
  `################################ PLANNED` / `################################ DONE` subsections; 2025 poms files
  used neither subsection — blocks were listed directly.

The shift to the linked-companion form is the model the new system should adopt — the day file becomes the plan, the
poms file is the ledger. The further 2026 split into `PLANNED` / `DONE` subsections inside the poms file makes the
"queue vs. timeline" view explicit and is the structure the new dashboard should expect.

Each `*_poms.zo` file also begins with a small navigation header that includes the link
`# @ = [[zot/poms_log.zot]]` — an aggregate, all-time pomodoro ledger. The new system should treat that file as a
denormalized view target, not a source of truth.

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

Distribution of variants observed (across 2025-10 → 2026-04 day and poms files):
- `p::N` only ⇒ planned future block.
- `p::N/total @X` (no times) ⇒ completed block, time not back-filled. Common.
- `p::N/total start::HHMM end::HHMM @X` ⇒ completed block with full provenance. Most common.
- `p::N/total end::HHMM @X` (end-only, no `start::`) ⇒ partial provenance — observed e.g.
  `======================== p::3/50 end::1320 @X` in `~/org/2025/20251019_poms.zo`. Treat the start as
  derivable from the previous block's `end::`.
- `p::N/total start::HHMM end::HHMM` (no `@X`) ⇒ block that was logged but not marked complete (rare).
- `p::N/total` (no times, no `@X`) ⇒ in-progress / partially-logged block (rare).

`@X` is the only sigil ever observed on the **header** line. Sigils like `@WIP`, `@CL`, `@PAGE` always sit on
**body** lines and modify the immediately-preceding bead reference, never the block.

The 24-character `========================` delimiter is a unique-purpose token — across `~/org/2026/` it appears
only as the pomodoro block header and nowhere else. The parser can rely on it as an unambiguous opener.

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

Companion-file timeline: `*_poms.zo` files only exist from **2025-10-19** onward. 72 `_poms.zo` files were created in
2025 (covering 2025-10-19 → 2025-12-31); from 2026-01-01 the format is universal. Any port plan that depends on the
companion file must therefore handle the pre-2025-10-19 case (everything inline in the day file, or — for early 2024
— inline in the plain `*.zo`).

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
   `======================== p::… [start::… end::… @X]` is regular enough to parse with a small DSL, and the
   `========================` token is unique to this construct in the corpus. (Note: the `^poms #z/ref
   title::Pomodoros` anchor referenced in `new_zorg_daily_today_transition.md` is a *new-system proposal*, not a
   convention found in the legacy data — the legacy divider's payload is just the wiki link to the poms file.)

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

5. **Companion `*_poms.zo` file is optional, not required.** The section in the day file is sufficient to log a day,
   and indeed was the *only* recording surface before 2025-10-19. The companion file is a denormalized rollup the
   user maintains by hand, with a small navigation header (`# ^ = [[YYYY/YYYYMMDD]]`, `# < = [[…_poms]]`,
   `# @ = [[zot/poms_log.zot]]`, `# 0 = [[YYYY/YYYYMMDD_day]]`). The 2026-introduced `PLANNED` / `DONE` subsections
   are the cleanest split for the new system to expose. The new system can either generate the companion file from
   indexed data (preferred — eliminates dual-write drift) or drop it entirely and serve the same view from a query;
   either way, `~/org/zot/poms_log.zot` is a third, all-time aggregate that already exists and should be preserved as
   a generated artifact.

6. **Carryover prefix needs a parser rule.** The `- 260427 260424#0E …` shape (current-day date, then earlier-day
   id) is a deliberate way to mark that a planned item is being executed today. Without an explicit rule, this looks
   like a malformed id. Suggest: leading bare `YYMMDD ` token before `<YYMMDD>#<id>` is the carryover-date and goes
   on the `Pomodoro` block body, not the bead.

7. **`@WIP` / `@X` / `@CL` are status sigils, not tags.** They appear at the *end* of body lines or block headers
   and modify the immediately-preceding entity. Treat them as a closed enum: `WIP` (in-progress), `X` (completed),
   `CL` (changelist boundary). New tags should not be invented in this position.

## Open Questions

- Does the user want the v1 grammar to accept the legacy dividers as aliases (`[[pomodoro]] NOTES`,
  `[[pomodoro]] TODAY`, the bare `Pomodoros` form from early 2024), or only the modern `[[YYYY/YYYYMMDD_poms]]`
  link form? (All three legacy forms are unused in 2026 day files but are present in the historical corpus the new
  system may need to read.)
- Cumulative minutes (`p::5/91`) are maintained manually today. Should the new editor auto-recompute, or is the
  hand-maintained number a deliberate "I last paid attention here" marker?
- The `* [X]` sub-checklist under a body line is freeform today. Should it be promoted to a structured outcome
  list, or left as plain prose? (Argument for prose: the user uses it inconsistently — some blocks have it, most
  don't.)
- Should the partial-provenance `end::HHMM`-only header variant be canonicalized (auto-fill `start::` from the
  previous block's `end::`), or preserved as-is to keep the day file byte-stable?
- Does `~/org/zot/poms_log.zot` need to remain hand-edited, or can it be regenerated from the per-day poms files
  on save? The current dual-write surface area is a known drift risk.

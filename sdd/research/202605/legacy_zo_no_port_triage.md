---
research_date: 2026-05-04
last_revised: 2026-05-04
title: Legacy .zo notes that do not need direct .z migration
source_corpus:
  - ~/org/**/*.zo
output_location:
  - sdd/research/202605/
---

# Legacy .zo Notes That Do Not Need Direct .z Migration

## Scope

This research inventories the legacy `~/org/**/*.zo` corpus and recommends which files should not be ported directly to
new `*.z` notes. There is no `sdd/research/README.md` in this checkout, so this file is placed under
`sdd/research/202605/` to match the month-directory layout used by the generated SDD docs.

The corpus currently contains 4,091 `*.zo` files and about 5.4 MB of text. The repo-local `*.z` files are fixture files,
not an already-migrated personal corpus, so this recommendation is based on the legacy files themselves.

## Recommendation Summary

Do not migrate 1,903 files as `*.z` notes. They are generated logs, mechanical indexes, trash, scratch files, or fake
examples whose value is either redundant or intentionally low.

Do not migrate another 1,628 daily/journal files as standalone zettel by default. Keep them as a read-only archive and
only extract specific events, decisions, references, or durable personal/work notes when they are still useful.

That leaves roughly 560 topical, project, literature, reference, and active workflow files for manual migration review.

## Clear No-Port Files

These files should be skipped unless there is a specific known item inside them that must be extracted.

| Category | Count | Patterns | Justification |
| --- | ---: | --- | --- |
| Habit logs | 826 | `~/org/202[3-6]/*_habit.zo` | Mostly daily checklist state such as meds, teeth, run, WFO, and pomodoro counters. This is historical telemetry, not durable knowledge. |
| Done logs | 801 | `~/org/202[3-6]/*_done.zo` | Completed task ledgers. Useful for audit only; many entries are already represented by project notes, day notes, or current todos. |
| Pomodoro logs | 174 | `~/org/202[3-6]/*_poms.zo` | Work-session accounting. The content is timeboxed execution history and is too noisy for the new note graph. |
| Habit rollups | 5 | `~/org/202[3-6]/YYYY_habit_*.zo` | Empty or near-empty habit-period headers. They can be regenerated if habit tracking returns. |
| Journal indexes | 29 | `~/org/2024.zo`, `~/org/2025.zo`, `~/org/2026.zo`, `~/org/202403.zo` through `~/org/202604.zo` | Pure navigation scaffolding linking year/month/day files. New Zorg can reconstruct these from paths, dates, or queries. |
| Tickler scaffolding | 46 | `tick_day.zo`, `tick_month.zo`, `tick_year.zo`, `tick_01.zo` through `tick_31.zo`, `tick_month_01.zo` through `tick_month_12.zo` | Mostly empty calendar buckets. Keep payload-bearing tickler notes such as `tick.zo`, `tick_2025.zo`, `tick_2026.zo`, and `ticktock.zo` for separate review. |
| Trash | 19 | `~/org/trash/**/*.zo` | Already explicitly demoted. Some files are nontrivial, but the directory is a strong authorial signal that they should not enter the new graph unless manually rescued. |
| Scratch/fake examples | 3 | `tmp.zo`, `foobar.zo`, `fake_zo_data.zo` | Scratchpad and example fixtures. They are not personal knowledge records. |

Clear no-port total: 1,903 files.

## Archive-Only Daily Files

These files should not be ported one-for-one, but they should not be deleted. They are best preserved as raw archival
text and mined selectively.

| Category | Count | Patterns | Recommendation |
| --- | ---: | --- | --- |
| Old daily logs | 854 | `~/org/202[3-6]/YYYYMMDD.zo` | Archive raw. Extract only meaningful journal entries, decisions, links, or reusable notes. |
| Newer day plans | 759 | `~/org/202[3-6]/YYYYMMDD_day.zo` | Archive raw. Do not migrate old daily plans unless they contain active tasks, events, or decisions that are not already captured elsewhere. |
| Event sidecars | 4 | `~/org/2025/*_events.zo` | Usually calendar snapshots. Extract only events with lasting references. |
| 2023 week/month indexes | 11 | `~/org/2023/2023_week_*.zo`, `~/org/2023/2023_july.zo`, `~/org/2023/2023_august.zo`, `~/org/2023/2023_september.zo` | Navigation scaffolding from an earlier journal layout. Archive or regenerate. |

Archive-only total: 1,628 files.

The reason to treat this as archive-only instead of strict no-port is that daily files sometimes contain unique context:
one-off decisions, meeting notes, project status, family notes, and links that may not appear in topical files. The
migration should not spend effort converting every daily file into first-class `*.z` notes, but a later extractor could
search for markers like `ID::`, `q::`, `a::`, `DECISION`, `INSPIRED BY`, `LINKS:`, `@EVENT`, `#book`, `#work`, and
project tags (`+...`) to rescue durable entries.

## Files To Review Instead Of Skipping

The remaining files deserve manual triage because they are likely to contain durable knowledge or active workflow state.
Notable groups:

- Topical/root notes such as `agent_ref.zo`, `ai_ref.zo`, `books.zo`, `dev_ref.zo`, `gtd.zo`, `inbox.zo`, `now_dev.zo`,
  `soon_work.zo`, `tick.zo`, `tick_2025.zo`, `tick_2026.zo`, and `ticktock.zo`.
- Zorg design and migration notes such as `zorg.zo`, `zorg_ref.zo`, `zorg_ref_man.zo`, `zorg_archive.zo`,
  `zorg_accepted_ideas.zo`, `zorg_rejected_ideas.zo`, and `zorg_ideas_*.zo`.
- Literature notes under `~/org/lit/`, especially books/manuals that have extracted notes rather than only links.
- Project notes under `~/org/prj/`, plus root project files like `prj_zorg.zo`, `prj_work.zo`, and `done_projects.zo`.
- Meeting notes that may contain commitments or feedback history, especially `*_meet*.zo` files.

Some review files may still end up being no-port decisions. For example, stale work-project material tied to closed
Google projects may be better summarized into one archive note than migrated in full. The key distinction is that these
files need human judgment; the clear no-port and archive-only groups can be handled by rule.

## Suggested Migration Policy

1. Preserve the entire `~/org` legacy tree read-only before migration.
2. Exclude the 1,903 clear no-port files from automated conversion.
3. Exclude the 1,628 archive-only daily files from one-for-one conversion, but run a later extraction pass for durable
   entries.
4. Convert or summarize the remaining 560 files manually, prioritizing active todos, durable references, project notes,
   Zorg design material, literature notes, and named IDs still linked from current work.
5. For skipped files, record the rule that skipped them so future searches can distinguish "not migrated intentionally"
   from "missed by accident."

## Verification Commands

These commands were used from `/home/bryan/projects/github/zettel-org/zorg_100`.

```sh
find "$HOME/org" -type f -name '*.zo' | wc -l
find "$HOME/org" -type f -name '*.zo' -printf '%s %p\n' \
  | awk '{n++; sum+=$1} END {printf "files=%d\nbytes=%d\n", n, sum}'
```

```sh
find "$HOME/org" -mindepth 2 -maxdepth 2 -type f -name '*.zo' \
  | sed "s#$HOME/org/##" \
  | awk -F/ '{
      name=$2
      sub(/\.zo$/, "", name)
      if (name ~ /^[0-9]{8}$/) suf="plain_date"
      else if (name ~ /^[0-9]{8}_/) { suf=name; sub(/^[0-9]{8}_/, "", suf) }
      else if (name ~ /^[0-9]{4}_habit_/) suf="habit_rollup"
      else suf="other"
      c[suf]++
    }
    END { for (s in c) print c[s], s }'
```

```sh
find "$HOME/org" -type f -name '*.zo' \
  | sed "s#$HOME/org/##" \
  | awk '
    function tier1(p) {
      return (
        p ~ /^202[0-9]\/[0-9]{8}_habit\.zo$/ ||
        p ~ /^202[0-9]\/[0-9]{8}_done\.zo$/ ||
        p ~ /^202[0-9]\/[0-9]{8}_poms\.zo$/ ||
        p ~ /^202[0-9]\/[0-9]{4}_habit_/ ||
        p ~ /^trash\// ||
        p ~ /^tick_day\.zo$/ ||
        p ~ /^tick_month\.zo$/ ||
        p ~ /^tick_year\.zo$/ ||
        p ~ /^tick_[0-9][0-9]\.zo$/ ||
        p ~ /^tick_month_[0-9][0-9]\.zo$/ ||
        p ~ /^202[0-9]\.zo$/ ||
        p ~ /^202[0-9][0-9][0-9]\.zo$/ ||
        p ~ /^(tmp|foobar|fake_zo_data)\.zo$/
      )
    }
    function tier2(p) {
      return (
        p ~ /^202[0-9]\/[0-9]{8}(_day|_events)?\.zo$/ ||
        p ~ /^2023\/2023_(july|august|september|week_[0-9]+)\.zo$/
      )
    }
    {
      if (tier1($0)) t1++
      else if (tier2($0)) t2++
      else rem++
    }
    END {
      print "tier1", t1
      print "tier2", t2
      print "remaining", rem
      print "total", t1 + t2 + rem
    }'
```

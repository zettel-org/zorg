---
research_date: 2026-05-04
last_revised: 2026-05-04
title: Legacy .zo notes that do not need direct .z migration
source_corpus:
  - ~/org/**/*.zo
output_location:
  - sdd/research/202605/
revision_notes:
  - 2026-05-04 initial triage: clear-no-port + archive-only + manual-review tiers
  - 2026-05-04 added corpus topology, top-level pattern breakdown, work-confidentiality flag, day-file value re-evaluation, stub-size refinement, extraction-marker reference
---

# Legacy .zo Notes That Do Not Need Direct .z Migration

## Scope

This research inventories the legacy `~/org/**/*.zo` corpus and recommends which files should not be ported directly to
new `*.z` notes. There is no `sdd/research/README.md` in this checkout, so this file is placed under
`sdd/research/202605/` to match the month-directory layout used by the generated SDD docs.

The corpus currently contains 4,091 `*.zo` files and about 5.4 MB of text. The repo-local `*.z` files are fixture files,
not an already-migrated personal corpus, so this recommendation is based on the legacy files themselves.

### Methodology Notes

- File mtime is not a useful recency signal here. Every `*.zo` file in `~/org` shows a modification time within the last
  year, even files dated 2023, so the tree was likely touched by a recent bulk operation (rename, sync, or checkout).
  Recency must come from the date prefix in the filename or from internal `ID::` timestamps, not from `stat`.
- File size is a partial signal. The corpus has no zero-byte files. 131 files are smaller than 200 bytes and 2,150 are
  smaller than 1 KB. Most of those small files are templated stubs (tickler buckets, habit rollups, near-empty habit
  days). Size alone does not classify a file, but a `<200B` cutoff cleanly catches generated stubs.
- The repo holds fixture-grade `*.z` files only, so there is no risk of double-migrating an existing note. Any future
  migration tool can write fresh `*.z` files without collision checks against the legacy graph.

## Corpus Topology

The 4,091 files are not evenly distributed. Knowing where they live changes how a migration tool should iterate.

| Location | Files | Notes |
| --- | ---: | --- |
| `~/org/*.zo` (top level) | 507 | The topical/reference/project/now/soon/maybe/ideas/zorg/tickler hub. Most durable knowledge lives here. |
| `~/org/2024/` | 1,286 | Daily/habit/done/poms/event/day files for 2024. |
| `~/org/2025/` | 1,399 | Same as 2024 plus `_events.zo` sidecars. |
| `~/org/2023/` | 295 | Daily files plus 11 month/week index stragglers from an older journal layout. |
| `~/org/2026/` | 454 | Year-to-date daily/habit/done/poms files. |
| `~/org/lit/` | 81 | Literature notes (books, articles, manuals, blog posts). |
| `~/org/prj/` | 49 | Project subtrees: `3bts/5`, `anchor/4`, `arms/6`, `bs_allow/0`, `dcs_pie/5`, `mas/4`, `rap/22`, `xown/3`. |
| `~/org/trash/` | 19 | Explicitly demoted notes, plus nested `trash/refs/`, `trash/ref_x/p/...` subtrees. |
| `~/org/triage/` | 1 | Single payload file `bug_x_signup.zo` — small but real triage notes. |

The remaining `~/org/` subdirectories — `cfg`, `chat`, `code`, `err`, `images`, `img`, `lib`, `lit_review`, `papis`,
`plans`, `prompts`, `puml`, `query`, `remarkable`, `text`, `vim_utils`, `xmind`, `zoq`, `zot`, `zotero` — contain no
`*.zo` files. They hold non-zo assets (config, images, exported PDFs, query logs) and can be ignored by a `*.zo`
migration pass entirely.

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

### Day Files Are Higher Value Than Plain Dated Files

The `_day.zo` files (759 of them) are not just plans. A representative `2025/20250715_day.zo` contains:

- `@EVENT` blocks with attendee names and outcome notes (`%ccarnesi was still in his seat at ~1438, so I figured this
  meeting was cancelled`).
- Pomodoro-block work entries with `start::`/`end::` timestamps and references back to numbered todos.
- Cross-links to `now_dev`, `now_work`, `now_zorg`, `now_gtd`, etc.

By contrast, the plain `YYYYMMDD.zo` files (854 of them) tend to be lighter free-form journals. A migration extractor
should prioritize `_day.zo` files for `@EVENT`, decision, and meeting-note rescue before scanning plain dated files. The
"archive-only" tier is correct for both groups, but the per-file rescue probability is higher for `_day.zo`.

## Files To Review Instead Of Skipping

The remaining 560 files deserve manual triage because they are likely to contain durable knowledge or active workflow
state. They split into roughly the following groups.

### Top-Level (`~/org/*.zo`, 474 of 507 after subtracting clear-no-port)

| Pattern | Approx Count | Examples | Notes |
| --- | ---: | --- | --- |
| Reference notes (`*_ref.zo`) | 26 | `agent_ref.zo`, `ai_ref.zo`, `dev_ref.zo`, `nvim_ref.zo`, `claude_code_ref.zo`, `work_ref.zo`, `zorg_ref.zo`, `zorg_ref_man.zo` | High keep rate. Among the largest files in the corpus (10–28 KB) and densest with durable knowledge. |
| Meeting notes (`*_meet*.zo`) | 26 | `fscarpel_meet_*.zo`, `team_meet_*.zo`, `pat_meet.zo`, `kboloor_meet.zo`, `thazel_meet_*.zo` | Likely contain commitments and feedback. Quarter-stamped variants (e.g. `fscarpel_meet_2024Q3.zo`) are candidates for one-archive-note summarization rather than per-quarter migration. |
| Idea ledgers (`*_ideas*.zo`) | 15 | `zorg_ideas_24.zo`, `zorg_ideas_25H2.zo`, `dev_ideas.zo`, `book_ideas.zo`, `work_ideas.zo`, `zorg_accepted_ideas.zo`, `zorg_rejected_ideas.zo` | Mixed value. Accepted/rejected splits should be preserved as-is; quarterly idea dumps can often be summarized. |
| Project root notes (`prj_*.zo`) | 14 | `prj_zorg.zo`, `prj_work.zo`, `prj_bs_allow.zo` | Active project state. Keep all by default. |
| Now/Soon/Maybe/Done buckets | 25 | `now_dev.zo`, `now_gtd.zo`, `now_work.zo`, `soon_zorg.zo`, `maybe_book.zo`, `done_books.zo`, `done_projects.zo` | GTD core. Migrate as-is or merge into a dashboard-shaped layout. |
| Zorg corpus | 17 | `zorg.zo`, `zorg_archive.zo`, `zorg_ideas_*.zo`, `zorg_accepted_ideas.zo`, `zorg_rejected_ideas.zo` | Self-referential history of this project. High keep rate. |
| Tickler payload | 4 | `tick.zo`, `tick_2025.zo`, `tick_2026.zo`, `ticktock.zo` | Small but real tickler entries. Distinct from the 46 generated tickler buckets in the no-port table. |
| Other topical | ~305 | `12qs.zo`, `gtd.zo`, `gtd_ideas.zo`, `eat.zo`, `inbox.zo`, `system_for_writing.zo`, `url.zo`, `build_pages.zo`, `greatday_nodue.zo`, etc. | Manual triage required. Many will keep, some will collapse into broader notes. |

### Literature (`~/org/lit/`, 81 files)

Examples include `effective_java.zo`, `ddia.zo`, `the_rust_prog_lang.zo`, `how_to_take_smart_notes.zo`,
`build_a_2nd_brain.zo`, `system_for_writing.zo`, `dorian_gray.zo`, plus a `drx_*` family (Google internal docs:
`drx_access_requirements.zo`, `drx_api_logs.zo`, `drx_api_presubmit.zo`, `drx_tangle_actions.zo`).

Migration heuristic: files with extracted highlights and personal commentary (e.g. `effective_java.zo`,
`how_to_take_smart_notes.zo`) are durable; files that are mostly link dumps to PDFs or Google internal docs are often
better archived than migrated. The `drx_*` and other Google-internal lit notes also need the work-confidentiality
review described below.

### Projects (`~/org/prj/`, 49 files)

Eight project subtrees: `3bts/` (5), `anchor/` (4), `arms/` (6), `bs_allow/` (0 zo files), `dcs_pie/` (5), `mas/` (4),
`rap/` (22), `xown/` (3). The largest, `prj/rap/`, dominates and likely needs the most migration attention. The
empty-of-zo `prj/bs_allow/` directory still has a top-level companion at `~/org/prj_bs_allow.zo` (56 bytes — a stub
worth verifying before migrating).

### Triage (`~/org/triage/`, 1 file)

The lone `triage/bug_x_signup.zo` was overlooked in the first pass of this research. It is a small but real triage note
with bug/screenshot links. Migrate (or fold into the relevant project note).

Some review files may still end up being no-port decisions. For example, stale work-project material tied to closed
Google projects may be better summarized into one archive note than migrated in full. The key distinction is that these
files need human judgment; the clear no-port and archive-only groups can be handled by rule.

## Work-Confidentiality Review (Cross-Cutting)

This is orthogonal to the per-file triage above and applies regardless of which migration tier a file lands in.

- `1,230` files (≈30% of the corpus) reference internal Google systems (`googleplex.com`, `go/<short>`, `http://b/...`,
  `screenshot.googleplex.com`).
- `894` files reference the `@google` work identity directly (`bbugyi@google`, `fscarpel`, `@google` annotations).

A personal zettel that aggregates this content under a fresh path is materially different from a personal journal: a
search index, a public dotfiles repo, or an LLM context window can leak go-links and bug numbers. Before any migration:

1. Decide whether the new `*.z` corpus is allowed to contain Google-internal references at all.
2. If yes, decide where it lives (private machine only, encrypted, separate sync namespace).
3. If no, plan a sanitization pass. The same grep set above identifies candidates for redaction, summarization, or
   exclusion.

This recommendation is independent of the no-port tiers. A 2025 `_done.zo` ledger may still be skipped under the rule
in the no-port table, but if it is read or extracted later, the same confidentiality review applies.

## Extracting Durable Entries From Archive Tier

When a later pass scans the 1,628 archive-only daily files for rescuable entries, these markers carry the highest
signal:

| Marker | Where it appears | Why it matters |
| --- | --- | --- |
| `ID::` | Tagged top-level entries, typically reference notes | Stable identifier the user has already curated. |
| `^- [0-9]{6}#[0-9A-Z]+` | Per-line note IDs in habit/done/day files | Granular addressability into a daily file. |
| `LINKS:` | Reference and meeting notes | Outbound link cluster, often the most reusable part of a note. |
| `INSPIRED BY` | Idea/journal entries | Provenance for original thought. |
| `DECISION` / `q::` / `a::` | Day files and meeting notes | Long-lived decisions and Q&A captures. |
| `@EVENT` | `_day.zo` files | Calendar-anchored meeting notes with attendees and outcomes. |
| `#book`, `#work`, `+<project>` | All file types | Topic and project tagging that already structures the corpus. |

A simple extractor that walks the archive tier looking for these patterns will recover most of the durable content
without forcing the user to read 1,628 daily files end-to-end.

## Suggested Migration Policy

1. Preserve the entire `~/org` legacy tree read-only before migration.
2. Run the work-confidentiality review **before** any migration step. Decide whether the new corpus may contain Google
   internal references; if not, sanitize or exclude the ~1,230 affected files at source.
3. Exclude the 1,903 clear no-port files from automated conversion.
4. Exclude the 1,628 archive-only daily files from one-for-one conversion, but run a later extraction pass for durable
   entries (use the marker table above).
5. Convert or summarize the remaining 560 files manually, in this order of priority:
   1. Active GTD buckets (`now_*`, `soon_*`, `inbox.zo`, `tick.zo`, `tick_2026.zo`, `ticktock.zo`).
   2. Reference notes (`*_ref.zo`) and Zorg design corpus.
   3. Project root notes and the active `~/org/prj/` subtrees.
   4. Meeting notes that may carry open commitments.
   5. Literature notes with personal commentary.
   6. Idea ledgers, summarized rather than copied where appropriate.
6. For skipped files, record the rule that skipped them so future searches can distinguish "not migrated intentionally"
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
# Subdir topology — confirms which directories actually hold *.zo files.
for d in 2023 2024 2025 2026 lit prj trash triage cfg chat code err images img \
         lib lit_review papis plans prompts puml query remarkable text \
         vim_utils xmind zoq zot zotero; do
  n=$(find "$HOME/org/$d" -type f -name '*.zo' 2>/dev/null | wc -l)
  printf '%5d %s\n' "$n" "$d"
done
echo "$(find "$HOME/org" -maxdepth 1 -type f -name '*.zo' | wc -l) <top-level>"
```

```sh
# Work-confidentiality scan — count files referencing Google-internal systems.
grep -rEl 'googleplex|go/[a-z]|http://b/|screenshot\.googleplex' "$HOME/org" --include='*.zo' | wc -l
grep -rEli 'fscarpel|@google|bbugyi@google' "$HOME/org" --include='*.zo' | wc -l
```

```sh
# Stub-size refinement — most files <200B are templated stubs (tickler buckets, habit rollups).
find "$HOME/org" -type f -name '*.zo' -size -200c | wc -l
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

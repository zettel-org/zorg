# Zorg v1 (Rust) — Curated MVP Recommendation

A review of the v1 zorg design notes in `~/org/zorg*.zo`, `~/org/prj_zorg.zo`, `~/org/z_bullets.zo`, and the
query/template examples, with a recommended MVP scope. Source notes range from 2024-04 through 2026-02; recent thinking
(2025-03+, especially the "zettel" convergence) is weighted most heavily, but the accepted/rejected idea ledgers are
used to resolve older ambiguity.

> **Naming caveat.** A late note ([zorg_ideas_25H2.zo:6], 2025-11-03) floats renaming to `zetty` and reverting to Python
> "since Claude Code understands it better." It is still open and unresolved. This document assumes the original framing
> — Rust + the name `zorg` — per the user's request.

---

## 1. The big convergence: everything is a zettel

The most important idea in the recent notes is that **zorg's data model has collapsed into a single primitive: the
zettel**. Files, directories, notes, and subnotes all become the same kind of object, distinguished only by where they
sit in the hierarchy and what type tag they carry.

This convergence subsumes or obsoletes a long list of older, more piecemeal ideas (FIDs vs ZIDs, tag-defs, file-notes,
path properties, alias types, zoq files, zot files, dir-config files, hidden properties, ...). It is the single most
load-bearing decision for v1.

Source ideas, in roughly the order they accreted:

- `[#zorg_file_zettel]` — every file gets a `%%% @foo` header block; the file _is_ a zettel ([zorg_ideas_2503.zo:36-54])
- `[#zorg_dir_zettel]` — every directory's `init.zo` is a zettel ([zorg_ideas_2502.zo:51-57])
- `[#zorg_id_syntax]` — `@foo` declares a zettel ID, `^bar` declares a local ID that renders to `@foo/bar`; this
  replaces both `ID::foo` and `LID::bar`. Author flagged "I LOVE this idea" ([zorg_ideas_2503.zo:76-85])
- `[#zorg_anonymous_notes]` — subnotes without IDs are first-class ([zorg_ideas_2503.zo:103-105])
- `[#zorg_query_zettel]` — every query lives inside a zettel, obsoleting `.zoq` files ([zorg_ideas_2503.zo:239-244])
- `[#zorg_note_types]` — every zettel carries a type tag (`#z/todo`, `#z/ref`, `#z/inbox`, etc.)
  ([zorg_ideas_2503.zo:167-193])
- `[#zorg_tag_inheritance]` — tags propagate down the path/parent hierarchy ([zorg_ideas_2501.zo:103-113]) — author
  flagged "I LOVE this idea"
- `[#zorg_no_zot_files]` — templates also become zettel, rather than a second real file format
  ([zorg_ideas_2503.zo:291-293])

**Recommendation:** make the zettel-as-uniform-primitive the v1 architectural spine. Ship the full zettel model in v1
even if many of the surface features (promotions, anonymous note auto-generation, etc.) are deferred.

**Correction to the previous cut:** the current notes do not support "templates are `.zot` files" and "no `.zot` file
type" as simultaneous v1 goals. Treat legacy `.zot` files as migration input parsed by the same grammar, but make the v1
canonical form a `#z/tmpl` zettel in a normal `.zo` file.

---

## 2. The differentiation thesis

The clearest articulation of _why_ zorg should exist is in [zorg_ideas_2501.zo:181-197] (the `[#zorg_v2]` note from
2025-01-16):

1. **Editor-agnostic** via LSP + Treesitter — unlike org-mode/logseq/neorg which lock you to a specific editor.
2. **Plaintext-first** — the file is the rendered form; no conceal, no live preview required. Markdown export is a
   fallback, not the canonical view.
3. **Files, dirs, and notes are all zettel** — uniform navigation and query.
4. **Powerful, layered linking** — explicit, file/section, folgezettel, and inherited links all coexist.

These four differentiators define the v1 acceptance bar: if shipping an MVP breaks any of them, the MVP is wrong.

---

## 3. Recommended v1 MVP scope

The cut below is conservative — it deliberately ships only what the differentiation thesis above requires, plus a small
amount of capture/query machinery without which the system isn't usable day-to-day.

### Must-have (core)

1. **Treesitter grammar for `.zo`** (`[#syslang]` and `[#zorg_v2]` inspiration, [zorg_ideas_2501.zo:162-179]).
   Treesitter is the parser of record, used by both the editor and the LSP. Reject ANTLR (the Python version's choice),
   and reject the February `@@@<LANG>` code-block syntax in favor of ordinary Markdown-style fences
   ([zorg_ideas_2502.zo:7-12]).
2. **Zettel data model.** A single `Zettel` type covering files, directories (via `init.zo`), and notes. Storage: SQLite
   (continuing the v1 dataflow from [zorg.zo:82-83]). Track file and note hashes so reindex can be incremental; the old
   note-hash idea identifies note-level change as the right invalidation boundary ([prj_zorg.zo:118-124]).
3. **ID syntax: `@foo` / `^bar`** (`[#zorg_id_syntax]`). `^bar` renders to `@foo/bar` against the nearest ancestor with
   an ID. Pick **one** scheme; do not also support legacy `ID::`/`LID::`.
4. **Link syntax: `#foo/bar`** for absolute, plus _one_ of the sibling/child-relative forms from `[#zorg_link_syntax]` /
   `[#zorg_related_id_syntax]` (the `+child` / `~sibling` proposal at [zorg_ideas_2503.zo:158-165] is the most recent).
   Resolve-and-fail-loud on ambiguity.
5. **`%%% @foo ... %%%` file-zettel header** (`[#zorg_file_zettel]`, [zorg_ideas_2503.zo:36-54]) — the canonical
   mechanism by which a file participates in the zettel graph.
6. **LSP server (`zorg-ls`)** with the minimum useful surface: completion for `#`, jump-to-definition, find-references,
   rename, diagnostics for duplicate or unresolved IDs, and code actions for safe ID/link rewrites. This is
   `[#zorg_lsp_idea]` ([zorg_ideas_24.zo:231-238]) — also recently re-affirmed. Do not ship tag "hint characters" or
   query-driven completion in v1; both were rejected or losing support ([zorg_ideas_2501.zo:53-64],
   [zorg_ideas_2504.zo:16-21]).
7. **Note types as type tags** (`[#zorg_note_types]`, [zorg_ideas_2503.zo:167-193]). Use `#z/todo`, `#z/ref`,
   `#z/inbox`, etc. Drop the `&todo` syntax variant — converged on `#z/...` per the 250407 update note.
8. **Tag inheritance** (`[#zorg_tag_inheritance]`, [zorg_ideas_2501.zo:103-113]). Path-ancestor and parent-zettel
   inheritance only; defer link-target inheritance and the `##foo` opt-out syntax.
9. **Property syntax: typed scalar/list properties, with alpha statuses moved to tags** (`[#zorg_no_alpha_props]`,
   [zorg_ideas_2503.zo:116-126]). Keep properties for dates, numbers, IDs, times, and structured values (`p::5`,
   `due::2026-05-15`, `pg::42`, `start::1600`); represent workflow state as tags (`#ref/READ`) or note type (`#z/ref`).
   Keep slash-separated lists only for v1 and defer comma/list-bullet forms ([prj_zorg.zo:163-171]).
10. **Todos as priority-tagged notes** (`[#zorg_todos_are_notes]`, [zorg_ideas_2501.zo:7-18]). `[ ]`, `[N]` for
    priority, `[X]` done, `[?]` in-progress. No separate todo concept.

### Must-have (queries, minimal)

11. **SWOG queries** — already accepted in v1 of the Python version ([zorg_accepted_ideas.zo:27]); port the syntax.
    Required filters: property-equals (`foo:bar`), comparisons (`x:>5`), property-exists (`foo:*`), tag/link, file glob,
    todo status/priority, negation, text search, and relative modify-date ranges. The examples in
    `text/zorg_swog_examples.txt` make these part of the daily workflow, not speculative polish
    ([zorg_swog_examples.txt:8-56]).
12. **LIST output only** in v1. TABLE queries (`[#zorg_table_query]`) and custom functions (`[#zorg_query_functions]`)
    deferred to v1.x. `count()` exists in older planning, but aggregation should not define MVP scope
    ([prj_zorg.zo:92-98]).
13. **Query results live inside a zettel** (`[#zorg_query_zettel]`). Implies _no_ `.zoq` files in v1 — kill that file
    type entirely.

### Must-have (capture / write side)

14. **Capture command** (`[#zorg_capture]`, [prj_zorg.zo:8-16]). Global keybind, multiple templates, records source
    file. Templates are canonical `#z/tmpl` zettel; legacy `.zot` templates can be read during migration.
15. **`do::`, `due::`, `did::` lifecycle dates.** The old `tick::` reminder idea was accepted, but the more recent
    `do/due/did` rename supersedes `tick/due/tock` ([zorg_ideas_2503.zo:230-231]). v1 should ingest `tick::` as a
    migration alias and write `do::`.
16. **Timeboxing properties**: keep `p::`, `start::`, and `end::` in the grammar/query layer because they are already
    accepted practice for pomodoro/timebox notes ([zorg.zo:138-147]). Full habit tracking remains out of scope.

### Must-have (housekeeping)

17. **`zorg fix` plus strict check mode** (`[#zorg_fix]`, [prj_zorg.zo:68-88]) — the format / auto-update / lint
    command. v1 only needs: bullet-symbol normalization (`*`, `+`, `-` cycle per `z_bullets.zo:5-9`), auto-priority,
    ID/modify-date stamping, SORT pragma, duplicate-ID detection, unresolved link detection, and project/tag existence
    checks from the older `zorg check` requirements ([prj_zorg.zo:17-23]).
18. **GitHub org layout** per `[#zorg_gh_org]` ([zorg_ideas_2501.zo:207-217]): `zorg`, `zorg-treesitter`, `zorg-nvim`.
    `pyzorg` is the deprecated v0; decisions live in this repo (the older `zorg_decisions.zo` idea was rejected in favor
    of repo-local).

### Explicitly deferred (good ideas, not v1)

These are recommended-keep but _not_ on the v1 critical path. The MVP is weaker without them but still usable; cutting
them keeps the v1 surface shippable.

- **`zorg dash` TUI** (`[#zorg_dash]`) — high-value but large; v1.1.
- **`zorg export`** (`[#zorg_export]`) — needed eventually for Markdown/PDF but not MVP.
- **Plugin system** (`[#zorg_plugins]`) — defer until the core API has stabilized; ship v1 as a monolith.
- **`zorg move`, `zorg note promote`, and `zorg action copy/open`** — useful editing/navigation commands, but not
  required for first-index/query/LSP viability. The promotion path is strategically important because it exercises the
  "subnote -> note -> page" zettel model ([prj_zorg.zo:147-162]), so make it a v1.1 candidate.
- **Folgezettel ID tree view / jumpers** (`[#zorg_jumpers_idea]`, `[#zorg_fid_tree_view]`, [zorg_ideas_2504.zo:39-65]) —
  depends on a stable zettel graph; defer until graph is solid.
- **Recutils integration** (`[#zorg_rec]`, `[#zorg_recutils_tocks]`) — conceptually attractive but unproven; wait for a
  real use case.
- **Habit tracking** (`[#zorg_babel_habits]`) — interesting but standalone.
- **Todo-comments integration** (`[#todo_comments]`) — May 2025 notes say it became useful, but still not central to
  zorg's own data model ([zorg_ideas_2501.zo:20-26]).
- **Literature-note / org-noter workflow** — the May 2025 idea is good for the long-term zettelkasten story, but it is a
  vertical application of refs, capture, and external links rather than a core primitive ([zorg_ideas_2505.zo:7-9]).
- **Named URL shorthand `!foo`** (`[#zorg_v2_named_urls]`) — keep existing file/URL link support, but defer the new
  shorthand and argument handling.
- **Query pragma `% QUERY:`** (`[#zorg_query_pragma]`) — superseded in spirit by `[#zorg_query_zettel]`; pick one in
  v1.x.
- **PDF page-number links** (`[#zorg_pdf_page_links]`) — small, do post-MVP.
- **Definition lists, table captions, asciidoc-style features** ([zorg_ideas_2503.zo:24-33]) — pandoc-inspired; v2
  polish.
- **Embed notes via `((foo))` or `%(foo)`** (`[#zorg_embed_notes]`, [zorg_ideas_2504.zo:30-31]) — needs the zettel
  graph; v1.1.
- **Datalog/graph-database backend** (`[#zorg_python_query]`, `250321#0W` graph DB consideration) — premature; SQLite is
  fine for v1.
- **Saved queries and dot-snippets** (`$childCount`, `#work.zid`) — attractive once query-as-zettel and LSP APIs are
  stable, but too much surface for v1.
- **Parent-todo progress syntax** (`(1/3)`, `P0 (1/3)`) — plausible, but it depends on mature child-todo semantics;
  defer.

### Explicitly cut

- **Folgezettel as a separate ID system.** The recent `[#zorg_id_syntax]`-based hierarchy collapses ZID, LID, and FID
  into one scheme; do not also implement the alphanumeric `1a2b` FIDs from the v0 era. (See `250313#0W`,
  [zorg_ideas_2503.zo:70-74], which explicitly reconsiders whether FIDs are necessary.)
- **`.zoq` and `.zot` as separate file types** (`[#zorg_no_zoq_files]`, `[#zorg_no_zot_files]`). Templates are zettel of
  type `#z/tmpl`; queries are zettel of type `#z/query`. One file format, one parser.
- **Generated `.zoc` compile files** from the 2024 architecture. The v1 Rust pipeline should parse directly to
  AST/model/store; an invisible compiled file cache adds another truth source without helping the zettel model.
- **Custom `@@@` code-block syntax.** The source notes explicitly cooled on it; use Markdown fences in `.zo` content.
- **Manual tables.** `[#zorg_no_manual_tables]` — tables are query output only.
- **Multiple property-value semantics** (`[#zorg_prop_lists]` slash/comma forms). Pick one (slash) for v1; comma form
  can come later.
- **Tag syntax sugar** (the `@foo` -> `#person/foo` etc. transformations). `[#zorg_no_tag_sugar]` — flat tags only in v1
  ([zorg_ideas_2501.zo:268-282]).

---

## 4. Why this cut is the right one

Three principles guided the curation:

1. **Bias toward recent.** The 2025-03 / 2025-04 design wave is more coherent than anything before it because it was
   forced to reckon with the zettel collapse. Older ideas that survived that wave (LSP+Treesitter, tag inheritance,
   todos-as-notes, reminder dates later renamed from `tick::` to `do::`) are battle-tested by the author's own
   re-evaluation; older ideas that didn't survive (FIDs as a separate system, tag-defs, dir-config files, alias types)
   carry a 2025-period `@REJECTED` or "obsoleted by..." note.
2. **Bias toward differentiation.** Anything in §2 that distinguishes zorg from neorg/org-mode/logseq is in the MVP.
   Anything that's table stakes for any note-taker (capture, basic query, fix command) is in. Everything else can wait
   for the zettel graph to stabilize.
3. **Ship one thing, not three.** The MVP is "an editor-agnostic plaintext zettelkasten with LSP + Treesitter and a SWOG
   query." Each of the deferred items is a feature; none of them are required to make that one sentence true.

---

## 5. Open questions worth resolving before coding

These are unresolved in the source notes and will block design if not decided early.

1. **Sibling vs. child relative-link characters.** `[#zorg_link_syntax]` ([zorg_ideas_2503.zo:135-144]) proposes
   `+child` `-grandchild`, `[#zorg_related_id_syntax]` (a week later) proposes `+child` `~sibling`. The author's last
   note on this prefers `~sibling, +child`. **Recommend: adopt `~sibling, +child`** and document the deprecation of `-`
   in this role.
2. **Datetime syntax.** `[#zorg_v2_design]:18` flags this as `???`. The `YYMMDD@HHMM` form ([zorg_accepted_ideas.zo:60])
   is already accepted for v0; carry it forward unless there's a reason not to.
3. **Description lists, drawers.** `[#zorg_v2_design]:39-40` flags both as open. **Recommend: drop description lists
   from v1 entirely; treat `| KEY: value` as a property-bullet rendering and defer drawers to v1.1.**
4. **`q::` property under no-alpha-props.** `[#zorg_v2_design]:42-47` flags this. **Recommend: questions are zettel of
   type `#z/q`; the answer is a child zettel of type `#z/a`. No special property.**
5. **Habit tracking design** (`[#zorg_babel_habits]`). Out of MVP scope; do not block v1 on this.
6. **Header reference/chunk syntax.** `%%% #foo` is proposed for file or directory zettel whose canonical `@foo` lives
   elsewhere, and also for chunks like quarterly files ([zorg_ideas_2503.zo:256-262]). **Recommend: support `%%% @foo`
   in v1; defer `%%% #foo` until promotion/chunking is designed.**
7. **Legacy migration policy.** The corpus contains `ID::`, `LID::`, `.zoq`, `.zot`, `tick::`, old project tags like
   `+zorg`, and link forms like `[[file#section]]`. **Recommend: v1 parser reads legacy forms and exposes
   diagnostics/fixes, but v1 writer only emits the new forms.**

---

## 6. Suggested v1 milestone breakdown

A possible ordering — each milestone produces something demonstrable.

- **M1: Parser.** Treesitter grammar covering the `[#zorg_id_syntax]` form, the `%%%` file header, Markdown code fences,
  properties (`foo::value` and `* foo:: value` bullets), tags, links, todos, and legacy-read/new-write compatibility
  markers. CLI: `zorg parse FILE` dumps the AST.
- **M2: Zettel store.** SQLite schema; ingest a directory; `zorg db reindex`; incremental file/note hashing.
- **M3: Query.** SWOG LIST queries against the store, including text search, date filters, grouping, ordering, negation,
  and file/link selectors. CLI: `zorg query '<swog>'`.
- **M4: LSP MVP.** Completion, jump-to-def, find-refs over a single file and across the indexed corpus; diagnostics for
  duplicate/unresolved IDs and code actions for canonical rewrites.
- **M5: Capture + fix.** `zorg capture` and `zorg fix` cover the daily-use loop, including `tick::` -> `do::` and
  `.zot`/`.zoq` migration warnings.
- **M6: Polish.** Documentation, error messages, performance pass on db reindex (`251014#1J Optimize zorg db!` from
  [now_zorg.zo:14]).

Anything not in M1–M6 is post-MVP.

---
create_time: 2026-05-02 22:45:38
status: wip
prompt: sdd/prompts/202605/zorg_infographics_1.md
---
# Zorg Infographics Documentation Plan

## Goal

Add a small, consistent set of public documentation infographics that make the hardest Zorg v1 concepts easier to
understand without replacing the normative text contracts in `README.md` and `docs/*.md`.

The work should generate seven PNG images, commit them under a public docs asset directory, and embed each image near
the section that already defines the concept. The images should support the current Rust MVP documentation only; they
must not introduce new syntax, query behavior, editor ownership, migration behavior, or legacy compatibility promises.

## Source Context

The current documentation establishes these concepts as the best visual targets:

- `README.md` explains the editor-agnostic system boundary and documentation map.
- `docs/model.md` defines the single `Zettel` object, hierarchy, IDs, links, explicit versus inherited tags,
  diagnostics, and source spans.
- `docs/syntax.md` defines `.z` file headers, nested zettel blocks, IDs, links, tags, properties, todos, query zettel,
  and template zettel.
- `docs/query.md` defines SWOG LIST query execution against the SQLite index, inline versus `#z/query` definitions,
  stale-index behavior, and deterministic result shape.
- `docs/capture.md`, `docs/fix.md`, and `docs/lsp.md` define the daily authoring workflow across `zorg capture`,
  `zorg fix`, `zorg-ls`, indexing, diagnostics, and graph actions.

## Asset Contract

All generated public documentation images should live under:

```text
docs/assets/infographics/
```

Use lowercase kebab-case PNG filenames. Each image should be landscape 16:9 at 1600x900 or higher. Keep the image output
commit-ready and avoid generated side files.

Use this shared visual direction across all images:

- Clean technical infographic, light background, high contrast.
- Crisp vector-like shapes, restrained palette, consistent line weight, and a shared icon vocabulary.
- Short labels only. Avoid prose-heavy labels because generated text can become distorted.
- Use exact Zorg tokens only when needed and inspect them carefully: `.z`, `%%%`, `@id`, `^local`, `#link`, `+child`,
  `~sibling`, `#z/query`, `#z/tmpl`, `LIST`, `SQLite`, `zorg-ls`, `zorg fix`, and `zorg capture`.
- Do not show unsupported legacy syntax, folgezettel behavior, tag sugar, table output, aggregation, unsafe rewrites, or
  editor-specific source-of-truth ownership.

Each markdown embed should use a relative image path from the embedding file, concise alt text, and one local sentence
explaining why the visual is there.

## Planned Images

1. `zorg-v1-system-map.png`
   - Place in `README.md` near the opening description or Documentation Map.
   - Show `.z` files feeding the parser/model, SQLite store, SWOG query, CLI, `zorg-ls`, capture/fix commands, and
     Neovim integration. The diagram must show Zorg as editor-agnostic and avoid making any editor the source of truth.

2. `zettel-anatomy.png`
   - Place in `docs/model.md` under Core Entities.
   - Show file zettel, directory `init.z`, nested note, query, template, todo, and reference lowering into one `Zettel`
     object with ID, tags, properties, links, body, source spans, and diagnostics.

3. `syntax-building-blocks.png`
   - Place in `docs/syntax.md` under File Format or Zettel Blocks.
   - Show percent-fenced file headers, nested list zettel, tags, properties, todo markers, ordinary Markdown fences,
     query zettel, and template zettel as one syntax family.

4. `id-link-resolution.png`
   - Place in `docs/model.md` near IDs and Links.
   - Show absolute IDs, local IDs resolving under the nearest ancestor, absolute links `#foo/bar`, child links `+child`,
     sibling links `~sibling`, and unresolved or ambiguous references becoming diagnostics.

5. `tag-inheritance.png`
   - Place in `docs/model.md` near Tags.
   - Show directory, file, and parent zettel ancestry creating effective tags while preserving which tags were written
     explicitly and which were inherited.

6. `swog-query-flow.png`
   - Place in `docs/query.md` near Command Sequence or Query Zettel Execution.
   - Show inline SWOG and `#z/query` zettel definitions executing against the current SQLite index to produce
     deterministic `LIST` output. Include the missing or stale index path as a reindex prompt, not as partial execution.

7. `daily-workflow-loop.png`
   - Place in the strongest daily workflow section, likely `docs/capture.md`; cross-link from `docs/fix.md` or
     `docs/lsp.md` only if it remains concise.
   - Show `zorg capture` creating notes from `#z/tmpl`, `zorg fix` checking and applying deterministic safe rewrites,
     `zorg-ls` exposing diagnostics and graph actions, and the index/query loop keeping the corpus usable.

## Phase Plan

### Phase 1: Plan and Asset Contract

Create this self-contained plan, submit it with `sase plan`, and make no image or public documentation implementation
changes yet.

Acceptance:

- `sase_plan_zorg_infographics.md` exists.
- `sase plan sase_plan_zorg_infographics.md` runs successfully.
- No `docs/assets/infographics/` files or markdown embeds are created in this phase.

### Phase 2: Generate Foundation Images

Create `docs/assets/infographics/`, generate the first three PNGs, inspect each image, and embed them in `README.md`,
`docs/model.md`, and `docs/syntax.md`.

Acceptance:

- `zorg-v1-system-map.png`, `zettel-anatomy.png`, and `syntax-building-blocks.png` exist and are valid PNG files.
- Markdown paths resolve from their embedding files.
- Visuals match the current README/model/syntax contracts and do not add new product semantics.

### Phase 3: Generate Graph Semantics Images

Generate `id-link-resolution.png` and `tag-inheritance.png`, inspect both, and embed them in `docs/model.md` near the
relevant sections.

Acceptance:

- Both files exist and are valid PNG files.
- Directionality, inherited versus explicit tag treatment, and diagnostic fallback are visually clear.
- No visual implies legacy folgezettel behavior, tag sugar, or speculative relative-link guessing.

### Phase 4: Generate Query and Workflow Images

Generate `swog-query-flow.png` and `daily-workflow-loop.png`, inspect both, and embed them in the query and workflow
docs.

Acceptance:

- Both files exist and are valid PNG files.
- The query visual distinguishes inline queries, query zettel, SQLite index state, and deterministic `LIST` output.
- The workflow visual does not imply unsafe writes, automatic migration, or `zorg-ls` owning editor-specific behavior.

### Phase 5: Documentation Integration Pass

Review every markdown embed for path correctness, alt text, placement, and concept fit. Keep the docs concept-first
rather than turning them into an image gallery.

Acceptance:

- `rg -n "docs/assets/infographics|assets/infographics|!\\[" README.md docs` shows the intended embeds and no broken old
  paths.
- `find docs/assets/infographics -type f | sort` shows exactly the seven planned PNGs unless a prior phase documented a
  necessary rename.
- `README.md` and `docs/model.md` remain readable after receiving multiple visuals.

### Phase 6: Verification and Handoff

Run lightweight documentation validation without adding new dependencies.

Minimum checks:

```bash
git status --short
file docs/assets/infographics/*.png
rg -n "assets/infographics|!\\[" README.md docs
```

Also run a shell check that every markdown image path under `README.md` and `docs/` points to an existing file. Run Rust
checks only if a later phase touches Rust code, which this plan does not require.

Acceptance:

- All seven PNG files are present, non-empty, and detected as PNG images.
- All markdown image paths under `README.md` and `docs/` exist.
- Git status contains only intentional documentation and asset changes for the infographic work, plus any pre-existing
  unrelated changes left untouched.

## Suggested Image Prompt Pattern

Use a separate prompt per image, grounded in the relevant doc section, while keeping this base language consistent:

```text
Create a clean technical infographic for the Zorg v1 plaintext zettelkasten
documentation. Landscape 16:9, high-resolution PNG, light background, crisp
vector-like shapes, restrained palette, readable short labels, no decorative
stock imagery. Use exact labels only where specified. The diagram should explain
<specific concept>. Show <specific objects/arrows>. Avoid adding unsupported
features or legacy syntax.
```

Regenerate any image with blank output, clipped content, illegible labels, malformed critical Zorg tokens, confusing
arrows, inconsistent style, or invented semantics.

## Risks

- Generated images may distort exact text. Mitigate with short tokens and visual inspection before embedding.
- Visuals may accidentally imply unsupported semantics. Mitigate by reviewing each image against the local README/docs
  section before committing it.
- Multiple visuals in `docs/model.md` may reduce readability. Mitigate by placing each image close to the specific
  concept and using brief local captions only.
- Public docs assets may conflict with SDD-only assets. Mitigate by using `docs/assets/infographics/` and leaving
  `sdd/assets/` unchanged.

---
create_time: 2026-05-02 22:41:34
bead_id: zorg-3
tier: epic
status: wip
prompt: sdd/prompts/202605/zorg_infographics.md
---
# Zorg Infographics Documentation Plan

## Goal

Generate seven GPT-image infographics that explain Zorg v1 concepts which are hardest to understand from text alone,
save them in the Zorg Rust project repo, and reference them from README/docs markdown so readers encounter each visual
near the concept it explains.

The repo currently has public concept documentation under `README.md` and `docs/*.md`. It has one existing SDD image
under `sdd/assets/`, but no public docs image directory. The implementation should add public documentation infographics
under:

```text
docs/assets/infographics/
```

Use lowercase kebab-case filenames, commit-ready PNG output, and relative markdown links from the embedding document.

## Visual Style Contract

- Format: PNG, landscape 16:9, target 1600x900 or higher.
- Look: clean technical infographic, light background, high contrast, minimal decorative styling, readable at
  README/docs scale.
- Avoid tiny prose-heavy labels. GPT image generation can distort text, so each image should rely on shapes, arrows,
  object labels, and short tokens that can be checked visually after generation.
- Use exact Zorg tokens only where necessary: `.z`, `%%%`, `@id`, `^local`, `#link`, `+child`, `~sibling`, `#z/query`,
  `#z/tmpl`, `LIST`, `SQLite`, `zorg-ls`, `zorg fix`, and `zorg capture`.
- Keep a consistent visual system across all seven images: same palette, typography direction, line weight, and icon
  vocabulary.
- Each markdown reference should include concise alt text and one sentence that ties the visual to the local section.

## Seven Infographics

1. `zorg-v1-system-map.png`
   - Topic: Zorg's editor-agnostic architecture.
   - Concept: `.z` files feed parser/model, SQLite store, SWOG query, CLI, `zorg-ls`, capture/fix, and Neovim
     integration without making any editor the source of truth.
   - Primary reference: `README.md`, near the opening description or Documentation Map.

2. `zettel-anatomy.png`
   - Topic: One zettel primitive.
   - Concept: A file, directory `init.z`, nested note, query, template, todo, and reference all lower into one `Zettel`
     object with ID, tags, properties, links, body, source spans, and diagnostics.
   - Primary reference: `docs/model.md`, under Core Entities.

3. `syntax-building-blocks.png`
   - Topic: `.z` syntax anatomy.
   - Concept: Percent-fenced file headers, nested zettel list items, tags, properties, todo markers, ordinary Markdown
     fences, query zettel, and template zettel as one syntax family.
   - Primary reference: `docs/syntax.md`, under File Format or Zettel Blocks.

4. `id-link-resolution.png`
   - Topic: ID and link resolution.
   - Concept: Absolute IDs, local IDs resolving under nearest ancestor, absolute links `#foo/bar`, child links `+child`,
     sibling links `~sibling`, and unresolved/ambiguous links becoming diagnostics rather than guesses.
   - Primary reference: `docs/model.md`, under IDs or Links.

5. `tag-inheritance.png`
   - Topic: Explicit and inherited tags.
   - Concept: Directory/file/parent zettel ancestry creates effective tags, while the model preserves which tags were
     written explicitly and which were inherited.
   - Primary reference: `docs/model.md`, under Tags.

6. `swog-query-flow.png`
   - Topic: SWOG LIST query execution.
   - Concept: Inline SWOG or `#z/query` zettel definitions execute against the current SQLite index and produce
     deterministic LIST output; stale/missing indexes ask the user to reindex.
   - Primary reference: `docs/query.md`, near Command Sequence or Query Zettel Execution.

7. `daily-workflow-loop.png`
   - Topic: Daily authoring loop.
   - Concept: `zorg capture` creates notes from `#z/tmpl`, `zorg fix` checks and applies deterministic safe rewrites,
     `zorg-ls` exposes diagnostics and graph actions, and the index/query loop keeps the corpus usable.
   - Primary references: `docs/capture.md`, `docs/fix.md`, and/or `docs/lsp.md`. Prefer one embed in the strongest
     section and cross-link from the others only if that stays concise.

## Phase Plan

### Phase 1: Plan and Asset Contract

Owner: current planning agent.

Tasks:

- Inspect README/docs to choose visual topics that support core Zorg concepts.
- Define target asset directory, filenames, visual style, markdown reference targets, and validation expectations.
- Submit this plan with `sase plan`.

Acceptance:

- A self-contained `sase_plan_zorg_infographics.md` exists.
- `sase plan sase_plan_zorg_infographics.md` has been run successfully.
- No infographic or documentation implementation changes are made before plan submission.

### Phase 2: Generate Foundation Images

Owner: distinct implementation agent.

Scope:

- Create `docs/assets/infographics/`.
- Generate:
  - `zorg-v1-system-map.png`
  - `zettel-anatomy.png`
  - `syntax-building-blocks.png`
- Save the PNGs in the target directory.
- Visually inspect each image for blank output, illegible text, malformed critical Zorg tokens, poor crop, or
  inconsistent style.
- Add markdown references to:
  - `README.md`
  - `docs/model.md`
  - `docs/syntax.md`

Acceptance:

- All three files exist and are valid PNGs.
- Markdown image paths resolve from their embedding files.
- Visuals use accurate concepts from current docs and do not introduce new product semantics.

### Phase 3: Generate Graph Semantics Images

Owner: distinct implementation agent.

Scope:

- Generate:
  - `id-link-resolution.png`
  - `tag-inheritance.png`
- Save the PNGs in `docs/assets/infographics/`.
- Visually inspect both images for correct directionality, readable layout, and exact critical tokens.
- Add markdown references to `docs/model.md` near the IDs/Links and Tags sections.

Acceptance:

- Both files exist and are valid PNGs.
- The model doc uses both images where they reduce ambiguity in the written rules.
- No image suggests legacy folgezettel behavior, tag sugar, or speculative relative-link guessing.

### Phase 4: Generate Query and Workflow Images

Owner: distinct implementation agent.

Scope:

- Generate:
  - `swog-query-flow.png`
  - `daily-workflow-loop.png`
- Save the PNGs in `docs/assets/infographics/`.
- Visually inspect both images.
- Add markdown references to:
  - `docs/query.md` for SWOG query execution.
  - The most fitting daily workflow doc section, likely `docs/capture.md`, `docs/fix.md`, or `docs/lsp.md`.

Acceptance:

- Both files exist and are valid PNGs.
- The query visual accurately distinguishes inline queries, query zettel, the SQLite index, and LIST output.
- The workflow visual does not imply unsafe writes, automatic legacy migration, or editor-specific ownership by
  `zorg-ls`.

### Phase 5: Documentation Integration Pass

Owner: distinct documentation agent.

Scope:

- Review all markdown image references for relative path correctness, alt text, and placement.
- Ensure the README Documentation Map still reads cleanly after the system map addition.
- Ensure `docs/model.md` remains navigable despite receiving multiple visuals.
- Add a short image inventory if useful, but avoid turning docs into an image gallery disconnected from concept
  sections.

Acceptance:

- `rg -n "docs/assets/infographics|assets/infographics|!\\[" README.md docs` shows all intended references and no broken
  old paths.
- `find docs/assets/infographics -type f | sort` shows exactly the seven planned PNGs unless an implementation agent
  documented a necessary rename.
- Markdown remains concept-first; images are supportive rather than replacing normative contract text.

### Phase 6: Verification and Handoff

Owner: distinct verification agent.

Scope:

- Run documentation/link checks available in the repo without introducing new dependencies.
- At minimum run:
  - `git status --short`
  - `file docs/assets/infographics/*.png`
  - `rg -n "assets/infographics|!\\[" README.md docs`
  - A small shell check that every markdown image path under README/docs points to an existing file.
- Optionally run `cargo fmt --check` only if implementation changes touched Rust, which this work should not do.
- Prepare final handoff notes listing generated images, markdown files changed, and any visual quality caveats.

Acceptance:

- All image paths referenced from README/docs exist.
- All seven image files are present, non-empty, and detected as PNG images.
- Git status contains only intentional docs/assets changes for this task plus any pre-existing unrelated changes left
  untouched.

## Suggested GPT Image Prompt Pattern

Use a separate prompt per image, but keep this base language consistent:

```text
Create a clean technical infographic for the Zorg v1 plaintext zettelkasten
documentation. Landscape 16:9, high-resolution PNG, light background, crisp
vector-like shapes, restrained palette, readable short labels, no decorative
stock imagery. Use exact labels only where specified. The diagram should explain
<specific concept>. Show <specific objects/arrows>. Avoid adding unsupported
features or legacy syntax.
```

After generation, inspect each image before wiring it into docs. Regenerate any image with broken critical text,
confusing arrows, clipped content, or invented semantics.

## Risks

- GPT image models may garble exact text. Mitigation: use short tokens, inspect outputs, and regenerate where core
  labels are wrong.
- Generated visuals can accidentally imply semantics not in the docs. Mitigation: keep prompts grounded in existing
  README/docs contracts and review against the source section before embedding.
- Too many images in one markdown file can reduce readability. Mitigation: spread images across concept sections and
  keep each embed locally relevant.
- Asset naming or placement could conflict with SDD-only images. Mitigation: use `docs/assets/infographics/` for public
  docs and leave `sdd/assets/` unchanged.

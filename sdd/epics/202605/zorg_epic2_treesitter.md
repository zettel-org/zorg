---
create_time: 2026-05-02 15:57:58
status: wip
prompt: sdd/prompts/202605/zorg_epic2_treesitter.md
bead_id: zorg-1.2
tier: epic
legend_bead_id: zorg-1
---
# Zorg Epic 2 Tree-Sitter Grammar And Highlighting Plan

## Objective

Implement Epic 2, "Tree-sitter Grammar and Highlighting", from `sdd/legends/202605/zorg_v1_mvp.md`.

Target repo: `../zorg-treesitter`.

Primary upstream contracts:

- `../zorg/docs/syntax.md`
- `../zorg/docs/model.md`
- `../zorg/docs/query.md`
- `../zorg/fixtures/corpus/`
- `sdd/legends/202605/zorg_v1_mvp.md`

The result should be the parser of record for Zorg `.z` files. It must provide stable Tree-sitter node names and editor
queries that downstream agents can use from `../zorg` and `../zorg-nvim`.

## Current Baseline

Epic 1 foundation work is already present.

`../zorg-treesitter` currently contains:

- `grammar.js`: a token-level grammar.
- `package.json`: `npm run generate` and `npm test`.
- `tree-sitter.json`: file type `z`, scope `source.zorg`, query paths, bindings disabled.
- `queries/highlights.scm`: token-level captures.
- `queries/folds.scm`, `queries/locals.scm`, `queries/injections.scm`: placeholders.
- `test/corpus/minimal.txt` and `test/corpus/query_and_template.txt`.

Baseline validation completed before writing this plan:

- `npm run generate` passes.
- `npm test` passes with 3 successful corpus parses.

Important baseline limitation: the grammar currently treats `.z` files as a flat stream of tokens. It does not preserve
file-header boundaries as rich nodes, list nesting, zettel body spans, paragraph boundaries, fenced-code bodies, or
split property/link/tag fields. Epic 2 should replace that flat token stream with a structured grammar while keeping
tests green throughout.

## Product Decisions To Preserve

All phases must consistently enforce these decisions:

- Canonical files use `.z`.
- Default root is `~/zorg`; this is semantic context, not grammar behavior.
- Directory zettel are represented by `init.z`; this is semantic context, not grammar behavior.
- File zettel headers use percent fences:
  - opening line begins with `%%%`
  - closing line is `%%%` with optional surrounding whitespace
- IDs are `@foo` and `@foo/bar`.
- Local IDs are `^bar`.
- Absolute links are `#foo/bar`.
- Child-relative links are `+child`.
- Sibling-relative links are `~sibling`.
- Tags are `#tag`, `#area/work`, and type tags under `#z/...`.
- Properties are `key::value`.
- Todo markers are `[ ]`, `[N]`, `[X]`, and `[?]`.
- Code fences are ordinary Markdown triple-backtick fences.
- Query definitions are ordinary zettel tagged `#z/query`, using either `query::...` or a fenced `swog` block.
- Template definitions are ordinary zettel tagged `#z/tmpl`, using fenced `zorg-template` blocks.
- Legacy Python-era syntax is not v1 syntax. Do not add compatibility nodes for `.zo`, `.zoq`, `.zot`, `.zoc`, `ID::`,
  `LID::`, `tick::`, `@@@` fences, old folgezettel IDs, tag sugar, or Python-era link behavior.

## Execution Model

Each phase below is intended for one distinct agent instance. Agents should start by checking local status in
`../zorg-treesitter` and should not revert unrelated user changes.

Recommended order:

1. Phase 1: grammar architecture and corpus contract.
2. Phase 2: structural block grammar.
3. Phase 3: zettel opening syntax and inline primitives.
4. Phase 4: query/template and fenced-code handling.
5. Phase 5: editor query pack.
6. Phase 6: integration hardening and handoff.

Phases are mostly sequential because later phases depend on stable node names from earlier phases. Phase 5 may begin
after Phase 4's node names are stable. Phase 6 must run last.

## Phase 1: Grammar Architecture And Corpus Contract

Owner: one Tree-sitter grammar/design agent.

Target repo: `../zorg-treesitter`.

Purpose: turn the current token skeleton into a documented grammar contract so later agents can work without inventing
incompatible node names.

Expected changes:

- Audit `../zorg/docs/syntax.md`, `../zorg/docs/model.md`, and `../zorg/fixtures/corpus/`.
- Update `README.md` with the Epic 2 grammar scope, node naming conventions, validation commands, and no-legacy policy.
- Add or update a contributor-facing grammar note, either in `README.md` or a small `docs/grammar.md`, that defines
  stable public node names for:
  - `source_file`
  - `file_header`
  - `file_header_open`
  - `file_header_close`
  - `zettel_item`
  - `zettel_opening`
  - `paragraph`
  - `blank_line`
  - `comment`
  - `fenced_code_block`
  - `fence_start`
  - `fence_end`
  - inline primitive nodes listed in Phase 3
- Expand corpus coverage with small fixtures copied or adapted from `../zorg/fixtures/corpus/`, including at least:
  - file header with title text
  - nested zettel with child and sibling links
  - query zettel
  - template zettel
  - unsupported legacy-looking text as ordinary text or parse errors, never as compatibility syntax
- Keep the existing npm scripts working.

Design constraints:

- The grammar should parse structural syntax and preserve source spans.
- Link resolution, tag inheritance, duplicate IDs, invalid property values, and local-ID resolution are semantic
  concerns for later Rust code.
- Avoid broad Markdown parsing. Only model the `.z` structure needed by Zorg and leave ordinary prose as paragraph/text
  content.
- Keep node names conservative because `zorg-parse` and `zorg-nvim` will depend on them.

Acceptance:

- `npm run generate` passes.
- `npm test` passes.
- README or `docs/grammar.md` documents the intended public node names and which behaviors are intentionally semantic
  rather than syntactic.
- New corpus tests are small enough for later agents to update during grammar implementation.

## Phase 2: Structural Block Grammar

Owner: one Tree-sitter grammar implementation agent.

Target repo: `../zorg-treesitter`.

Dependency: Phase 1 complete.

Purpose: replace the flat token stream with a real block grammar that preserves file-header, paragraph, list nesting,
blank-line, comment, and code-fence boundaries.

Expected changes:

- Rework `grammar.js` so `source_file` is a sequence of block nodes rather than repeat-any-token.
- Implement structured file headers:
  - opening `%%%` line with inline header content
  - body/title lines inside the header
  - closing `%%%` line
  - recovery behavior for unterminated or malformed headers
- Implement list/nested zettel blocks with indentation-sensitive parent-child structure sufficient for source spans and
  folds.
- Implement paragraphs as contiguous prose lines that can contain inline primitives.
- Implement blank lines as useful separators where needed for stable parse trees.
- Implement Zorg comments only if already documented; otherwise explicitly keep comment parsing out of scope and avoid
  inventing syntax.
- Implement generic Markdown triple-backtick fenced-code blocks as block nodes, including language info and raw content.
- Update corpus expected trees to assert structure, not just token presence.

Design constraints:

- Tree-sitter grammars are not ideal at full indentation semantics. The goal is stable nesting for normal Zorg list
  indentation, not full Markdown list compatibility.
- Prefer simple rules and explicit tests over clever regexes that make recovery brittle.
- Do not add semantic diagnostics in the grammar.
- Do not introduce legacy syntax nodes.

Acceptance:

- Corpus fixtures parse without `ERROR` nodes for valid examples.
- Corpus fixtures prove nested zettel parent-child structure is preserved.
- File headers produce stable open/content/close node boundaries.
- Fenced code blocks preserve language and body spans.
- `npm run generate` passes.
- `npm test` passes.

## Phase 3: Zettel Opening Syntax And Inline Primitives

Owner: one Tree-sitter grammar implementation agent.

Target repo: `../zorg-treesitter`.

Dependency: Phase 2 complete.

Purpose: implement precise syntax nodes for all v1 zettel primitives in file headers, zettel openings, paragraphs, and
fenced-block-adjacent text.

Expected changes:

- Add explicit inline nodes:
  - `id`
  - `id_segment`
  - `local_id`
  - `absolute_link`
  - `child_link`
  - `sibling_link`
  - `tag`
  - `type_tag`
  - `property`
  - `property_key`
  - `property_value`
  - `todo_marker`
  - `title_text` or equivalent plain opening text
- Split hash syntax so tags and absolute links are distinguishable by node name where the grammar can do so without
  semantic context. If the same lexical form cannot always be distinguished syntactically, document the chosen rule and
  keep one node name stable for downstream semantic classification.
- Ensure primitives are recognized in:
  - file header opening lines
  - nested zettel opening lines
  - ordinary paragraphs
  - query/template zettel definitions
- Add malformed examples for:
  - invalid ID segments
  - malformed local IDs
  - malformed relative links
  - property key without value
  - invalid todo marker
  - legacy-looking `ID::`, `LID::`, and `tick::`
- Decide, document, and test whether malformed examples should produce `ERROR` nodes or ordinary text. Either behavior
  is acceptable if it does not create legacy-compatible syntax nodes.
- Regenerate Tree-sitter artifacts locally for testing, while respecting the repository's generated-artifact policy.

Design constraints:

- Keep property values raw and trimmed at syntax level. Typed date/number/list validation belongs in Rust semantic
  validation.
- Avoid parsing query language internals in the `.z` grammar.
- Avoid accepting old folgezettel IDs as IDs.
- Keep source spans precise for IDs, links, tags, property keys, property values, todo markers, and zettel opening
  regions.

Acceptance:

- Corpus tests cover every non-negotiable primitive from the roadmap.
- Valid examples parse without `ERROR` nodes.
- Malformed and legacy-looking examples do not produce v1 compatibility nodes.
- Node names are reflected in `src/node-types.json` after local generation.
- `npm run generate` passes.
- `npm test` passes.

## Phase 4: Query, Template, And Fenced-Code Handling

Owner: one Tree-sitter grammar implementation agent.

Target repo: `../zorg-treesitter`.

Dependency: Phase 3 complete.

Purpose: make query and template zettel parse through the same `.z` grammar as ordinary notes, while preserving
fenced-block information for editor injections and later Rust lowering.

Expected changes:

- Add or refine corpus fixtures for:
  - `#z/query` zettel with `query::...`
  - `#z/query` zettel with fenced `swog`
  - `#z/tmpl` zettel with fenced `zorg-template`
  - ambiguous query examples containing both `query::` and `swog` as syntax that parses but remains semantically invalid
  - ordinary fenced code blocks with other language names
- Ensure fenced-code block nodes expose:
  - opening fence
  - language/info string
  - raw body
  - closing fence
- Ensure `swog` and `zorg-template` are just fenced language names in the grammar, not separate file formats.
- Keep `.zoq` and `.zot` out of scope entirely.
- Update README or grammar docs if the fenced-code node contract changes.

Design constraints:

- The `.z` grammar should not parse SWOG filter syntax internally. That belongs to `zorg-query`.
- The `.z` grammar should not parse template variables internally unless doing so is needed for injection/highlighting
  and does not destabilize the block grammar. Raw body preservation is enough for MVP handoff.
- Query/template semantics are indicated by ordinary `#z/query` and `#z/tmpl` tags; semantic consumers decide what those
  mean.

Acceptance:

- Query and template fixtures parse without `ERROR` nodes.
- Fenced `swog` and `zorg-template` blocks are structurally distinguishable by their info string or stable child node.
- Ordinary non-Zorg fenced code remains valid.
- `npm run generate` passes.
- `npm test` passes.

## Phase 5: Editor Query Pack

Owner: one Tree-sitter editor-query agent.

Target repo: `../zorg-treesitter`.

Dependency: Phase 4 complete.

Purpose: provide useful Tree-sitter queries for highlighting, folds, locals, and injections based on the final Epic 2
node names.

Expected changes:

- Replace token-level `queries/highlights.scm` with captures for:
  - IDs and local IDs
  - absolute, child-relative, and sibling-relative links
  - tags and type tags
  - property keys and values
  - todo markers
  - file-header fences
  - list markers
  - fenced-code fences and language names
  - comments if Phase 2 added comment syntax
- Implement `queries/folds.scm` for:
  - file headers
  - nested zettel subtrees
  - fenced code blocks
- Implement `queries/injections.scm` for fenced code blocks, especially:
  - `swog`
  - `zorg-template`
  - ordinary language-name passthrough where Tree-sitter supports it
- Implement `queries/locals.scm` only if the final grammar has meaningful local scope captures. If not, keep it
  intentionally minimal and document why.
- Add a small highlight smoke fixture or documented command for:
  - `npx tree-sitter highlight path/to/example.z`
  - equivalent direct `tree-sitter highlight` command

Design constraints:

- Prefer standard Tree-sitter capture names where they fit so Neovim themes work reasonably without custom setup.
- Do not make editor queries depend on semantic resolution.
- Keep query files tolerant of optional nodes and parser recovery.

Acceptance:

- `npm run generate` passes.
- `npm test` passes.
- `tree-sitter highlight` or `npx tree-sitter highlight` produces useful output for representative fixtures, or the
  exact local tooling limitation is documented.
- Fold and injection queries compile under the installed Tree-sitter CLI.
- README documents query file purpose and highlight smoke-test command.

## Phase 6: Integration Hardening And Handoff

Owner: one integration/review agent.

Target repo: `../zorg-treesitter`, with read-only checks against `../zorg` and `../zorg-nvim` as needed.

Dependency: Phases 1-5 complete.

Purpose: stabilize the parser contract for Epic 3 Rust parsing and Epic 8 Neovim integration.

Expected changes:

- Run a final node-name audit against:
  - `../zorg/docs/syntax.md`
  - `../zorg/docs/model.md`
  - `../zorg/fixtures/corpus/`
  - `../zorg-nvim/README.md` and parser registration docs if present
- Ensure `README.md` identifies:
  - stable public nodes
  - validation commands
  - generated-artifact policy
  - no-legacy policy
  - downstream expectations for Rust and Neovim agents
- Decide whether generated parser files under `src/` remain ignored or become committed as part of the parser contract.
  If committing them is chosen, update `.gitignore`, `tree-sitter.json`, and README consistently. If not committing them
  is chosen, make sure downstream docs explicitly require generation before integration.
- Add any missing corpus cases needed to cover the final grammar contract.
- Perform a final parser recovery review with malformed but representative inputs.
- Avoid changing `../zorg` or `../zorg-nvim` unless a small documentation sync is required and no unrelated edits would
  be disturbed.

Design constraints:

- This phase should harden and document; it should not rewrite the grammar architecture unless earlier phases left
  blocking defects.
- Generated-artifact policy matters because `zorg-parse` will need a reliable way to build against the grammar.
- Downstream agents need source spans more than they need semantic validation from the grammar.

Acceptance:

- `npm run generate` passes from a clean checkout after dependency install.
- `npm test` passes.
- Highlight, fold, and injection queries compile.
- Representative `.z` fixtures from `../zorg/fixtures/corpus/` parse without unexpected `ERROR` nodes.
- README and grammar docs are enough for Epic 3 and Epic 8 agents to integrate without rediscovering node names.
- No v1 grammar nodes support Python-era legacy syntax.

## Cross-Phase Quality Bar

Every phase should preserve these checks unless the phase explicitly documents a temporary failure and why:

```sh
npm run generate
npm test
```

When editor query work begins, also run:

```sh
npx tree-sitter highlight test/corpus/minimal.txt
```

or an equivalent command against a real `.z` fixture if the CLI requires a file extension for highlighting.

## Risks And Mitigations

- Risk: indentation-sensitive nested zettel structure becomes too complex for a pure JavaScript Tree-sitter grammar.
  Mitigation: support the normal Zorg list indentation needed for MVP and leave deep semantic parent validation to Rust.
- Risk: `#tag` and `#absolute/link` are syntactically similar. Mitigation: choose a stable node strategy, document where
  semantic classification starts, and keep source spans precise.
- Risk: query/template work accidentally creates separate `.zoq` or `.zot` syntax. Mitigation: only recognize ordinary
  zettel with type tags and ordinary fenced code language names.
- Risk: editor queries drift from node names. Mitigation: keep Phase 5 after grammar node stabilization and make Phase 6
  a node-name audit.
- Risk: generated parser policy blocks Rust integration. Mitigation: make Phase 6 explicitly decide and document the
  generated-artifact strategy.

## Definition Of Done For Epic 2

Epic 2 is complete when:

- `.z` files parse through a structured Tree-sitter grammar.
- File headers, nested zettel, paragraphs, blank lines, and fenced code blocks have stable source-backed nodes.
- IDs, local IDs, links, tags/type tags, properties, and todos have stable syntax nodes or a documented
  syntactic/semantic split.
- Query and template zettel parse as ordinary zettel.
- Highlight, fold, locals, and injection queries are useful and documented.
- Corpus tests cover valid and malformed examples for the MVP grammar.
- Generated parser policy is documented and consistent with repo files.
- `npm run generate` and `npm test` pass.
- The grammar does not implement legacy Python-era compatibility.
